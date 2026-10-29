//! Video and audio played inside the slide: the sound through the system's output (rodio,
//! which decodes MP3, WAV, FLAC, Ogg, AAC/M4A and MP4 sound), the pictures of a video decoded
//! by ffmpeg (found on the PATH, in the usual install places, or beside the app) into the
//! object's frame, in step with the sound. A control bar plays, pauses, seeks and sets the
//! volume. Without ffmpeg a video's sound still plays over its poster frame.

use egui::{pos2, vec2, Color32, Rect, Sense, Stroke};
use std::io::Read;
use std::sync::{Arc, Mutex};

pub struct Player {
    /// The media part playing and its file on disk.
    pub part: String,
    file: std::path::PathBuf,
    video: bool,
    // The sound: kept together (the stream must outlive the sink).
    _stream: Option<rodio::OutputStream>,
    sink: Option<rodio::Sink>,
    duration: Option<f64>,
    /// Where playing started (media seconds) and when (wall time), or the paused position.
    start_pos: f64,
    started: std::time::Instant,
    paused: Option<f64>,
    volume: f32,
    muted: bool,
    /// The latest decoded video frame: (media seconds, width, height, RGBA).
    frame: Arc<Mutex<Option<(f64, usize, usize, Vec<u8>)>>>,
    decoder: Option<std::process::Child>,
    texture: Option<egui::TextureHandle>,
    shown: f64,
    size: (usize, usize),
}

/// ffmpeg (and ffprobe), if installed.
pub fn tool(name: &str) -> Option<std::path::PathBuf> {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let mut places: Vec<std::path::PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    places.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].iter().map(std::path::PathBuf::from));
    if let Ok(me) = std::env::current_exe() { if let Some(d) = me.parent() { places.push(d.to_path_buf()); places.push(d.join("../Resources")); } }
    places.into_iter().map(|d| d.join(&exe)).find(|p| p.is_file())
}

impl Player {
    pub fn open(part: &str, bytes: &[u8], video: bool) -> Option<Player> {
        let ext = part.rsplit('.').next().unwrap_or("bin");
        let file = std::env::temp_dir().join(format!("sard-media-{}.{ext}", part.replace('/', "-")));
        if std::fs::metadata(&file).map(|m| m.len() as usize != bytes.len()).unwrap_or(true) { std::fs::write(&file, bytes).ok()?; }
        let duration = tool("ffprobe").and_then(|p| std::process::Command::new(p).args(["-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1"]).arg(&file).output().ok())
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<f64>().ok());
        let (w, h) = if video { probe_size(&file).unwrap_or((640, 360)) } else { (0, 0) };
        let mut p = Player { part: part.to_string(), file, video, _stream: None, sink: None, duration, start_pos: 0.0, started: std::time::Instant::now(), paused: Some(0.0), volume: 1.0, muted: false, frame: Default::default(), decoder: None, texture: None, shown: -1.0, size: (w, h) };
        p.play();
        Some(p)
    }

    pub fn position(&self) -> f64 {
        let p = self.paused.unwrap_or_else(|| self.start_pos + self.started.elapsed().as_secs_f64());
        match self.duration { Some(d) => p.min(d), None => p }
    }
    pub fn playing(&self) -> bool { self.paused.is_none() && !self.ended() }
    pub fn ended(&self) -> bool { self.duration.is_some_and(|d| self.position() >= d - 0.05) || (!self.video && self.sink.as_ref().is_some_and(|s| s.empty()) && self.paused.is_none()) }

    /// The sound as a file rodio reads well: videos and MP4/Windows audio are turned into WAV
    /// once (with ffmpeg) when possible.
    fn audio_file(&self) -> std::path::PathBuf {
        let ext = self.file.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !self.video && matches!(ext.as_str(), "mp3" | "wav" | "flac" | "ogg") { return self.file.clone(); }
        let wav = self.file.with_extension("sound.wav");
        if wav.is_file() { return wav; }
        if let Some(ff) = tool("ffmpeg") {
            let ok = std::process::Command::new(ff).args(["-y", "-v", "error", "-i"]).arg(&self.file).args(["-vn", "-ac", "2", "-ar", "44100"]).arg(&wav).status().is_ok_and(|s| s.success());
            if ok { return wav; }
        }
        self.file.clone()
    }

    /// (Re)start the sound and the pictures at `t` seconds.
    fn seek(&mut self, t: f64) {
        let t = t.max(0.0);
        if let Some(mut c) = self.decoder.take() { let _ = c.kill(); let _ = c.wait(); }
        self.sink = None;
        self._stream = None;
        if let Ok((stream, handle)) = rodio::OutputStream::try_default() {
            let audio = self.audio_file();
            if let (Ok(sink), Ok(f)) = (rodio::Sink::try_new(&handle), std::fs::File::open(&audio)) {
                // Some containers trip the decoder; a failure just means no sound.
                let src = std::panic::catch_unwind(move || rodio::Decoder::new(std::io::BufReader::new(f)).ok()).ok().flatten();
                if let Some(src) = src {
                    use rodio::Source;
                    if self.duration.is_none() { self.duration = src.total_duration().map(|d| d.as_secs_f64()); }
                    sink.append(src.skip_duration(std::time::Duration::from_secs_f64(t)));
                    sink.set_volume(if self.muted { 0.0 } else { self.volume });
                    sink.pause();
                    self.sink = Some(sink);
                    self._stream = Some(stream);
                }
            }
        }
        if self.video && self.paused.is_none() { self.start_decoder(t); }
        self.start_pos = t;
        self.started = std::time::Instant::now();
        if self.paused.is_some() { self.paused = Some(t); }
    }

    /// ffmpeg writes frames (RGBA, at real-time pace) that a thread keeps the latest of.
    fn start_decoder(&mut self, t: f64) {
        let Some(ff) = tool("ffmpeg") else { return };
        let (w, h) = (self.size.0.clamp(16, 1280), self.size.1.clamp(16, 720));
        let (w, h) = (w & !1, h & !1);
        let child = std::process::Command::new(ff)
            .args(["-v", "error", "-ss", &format!("{t:.3}"), "-re", "-i"]).arg(&self.file)
            .args(["-an", "-f", "rawvideo", "-pix_fmt", "rgba", "-vf", &format!("scale={w}:{h}"), "-"])
            .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).spawn();
        let Ok(mut child) = child else { return };
        let Some(mut out) = child.stdout.take() else { return };
        let frame = self.frame.clone();
        let fps_guess = 30.0;
        std::thread::spawn(move || {
            let mut buf = vec![0u8; w * h * 4];
            let mut n = 0u64;
            while out.read_exact(&mut buf).is_ok() {
                *frame.lock().unwrap() = Some((t + n as f64 / fps_guess, w, h, buf.clone()));
                n += 1;
            }
        });
        self.decoder = Some(child);
    }

    pub fn play(&mut self) {
        let at = if self.ended() { 0.0 } else { self.paused.unwrap_or_else(|| self.position()) };
        self.paused = None;
        self.seek(at);
        if let Some(s) = &self.sink { s.play(); }
    }
    pub fn pause(&mut self) {
        if self.paused.is_some() { return; }
        let p = self.position();
        self.paused = Some(p);
        if let Some(s) = &self.sink { s.pause(); }
        if let Some(mut c) = self.decoder.take() { let _ = c.kill(); }
    }
    pub fn toggle(&mut self) { if self.playing() { self.pause() } else { self.play() } }
    pub fn seek_to(&mut self, t: f64) {
        let playing = self.playing();
        self.paused = Some(t);
        if playing { self.play(); } else { self.seek(t); if self.video { self.start_decoder(t); } }
    }
    pub fn set_volume(&mut self, v: f32) { self.volume = v.clamp(0.0, 1.0); if let Some(s) = &self.sink { s.set_volume(if self.muted { 0.0 } else { self.volume }); } }
    pub fn toggle_mute(&mut self) { self.muted = !self.muted; self.set_volume(self.volume); }

    /// Draw the playing video's picture in `r` (true when there is one to show).
    pub fn paint(&mut self, ctx: &egui::Context, p: &egui::Painter, r: Rect) -> bool {
        if !self.video { return false; }
        if let Some((t, w, h, px)) = self.frame.lock().unwrap().clone() {
            if t != self.shown {
                self.shown = t;
                let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &px);
                match &mut self.texture { Some(tex) => tex.set(img, egui::TextureOptions::LINEAR), None => self.texture = Some(ctx.load_texture(format!("media-{}", self.part), img, egui::TextureOptions::LINEAR)) }
            }
        }
        let Some(tex) = &self.texture else { return false };
        // Letterboxed in the frame.
        let (w, h) = (tex.size()[0] as f32, tex.size()[1] as f32);
        let k = (r.width() / w).min(r.height() / h);
        p.rect_filled(r, 0.0, Color32::BLACK);
        p.image(tex.id(), Rect::from_center_size(r.center(), vec2(w * k, h * k)), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        if self.playing() { ctx.request_repaint(); }
        true
    }

    /// The control bar under the media: play/pause, the time line (click to seek), the time,
    /// mute and volume.
    pub fn controls(&mut self, ui: &mut egui::Ui, frame: Rect) {
        let bar = Rect::from_min_size(pos2(frame.min.x, frame.max.y + 4.0), vec2(frame.width().max(220.0), 26.0));
        let p = ui.painter();
        p.rect_filled(bar, 4.0, Color32::from_black_alpha(170));
        let play = Rect::from_min_size(bar.min + vec2(4.0, 3.0), vec2(20.0, 20.0));
        let r = ui.interact(play, ui.id().with(("media-play", &self.part)), Sense::click());
        if self.playing() {
            p.rect_filled(Rect::from_min_size(play.min + vec2(5.0, 4.0), vec2(3.5, 12.0)), 0.0, Color32::WHITE);
            p.rect_filled(Rect::from_min_size(play.min + vec2(11.5, 4.0), vec2(3.5, 12.0)), 0.0, Color32::WHITE);
        } else {
            p.add(egui::Shape::convex_polygon(vec![play.min + vec2(6.0, 4.0), play.min + vec2(16.0, 10.0), play.min + vec2(6.0, 16.0)], Color32::WHITE, Stroke::NONE));
        }
        if r.clicked() { self.toggle(); }
        let vol = Rect::from_min_size(pos2(bar.max.x - 74.0, bar.min.y + 3.0), vec2(70.0, 20.0));
        let line = Rect::from_min_max(pos2(play.max.x + 8.0, bar.center().y - 2.0), pos2(vol.min.x - 76.0, bar.center().y + 2.0));
        let d = self.duration.unwrap_or(0.0).max(0.001);
        let f = (self.position() / d).clamp(0.0, 1.0) as f32;
        p.rect_filled(line, 2.0, Color32::from_gray(110));
        p.rect_filled(Rect::from_min_max(line.min, pos2(line.min.x + line.width() * f, line.max.y)), 2.0, Color32::WHITE);
        p.circle_filled(pos2(line.min.x + line.width() * f, line.center().y), 5.0, Color32::WHITE);
        let seek = ui.interact(line.expand2(vec2(0.0, 8.0)), ui.id().with(("media-seek", &self.part)), Sense::click_and_drag());
        if let (true, Some(pt)) = (seek.clicked() || seek.drag_stopped(), seek.interact_pointer_pos()) { let k = ((pt.x - line.min.x) / line.width()).clamp(0.0, 1.0) as f64; self.seek_to(k * d); }
        let fmt = |s: f64| format!("{:02}:{:02}", (s / 60.0) as u32, s as u32 % 60);
        p.text(pos2(line.max.x + 8.0, bar.center().y), egui::Align2::LEFT_CENTER, format!("{} / {}", fmt(self.position()), fmt(self.duration.unwrap_or(0.0))), egui::FontId::monospace(10.5), Color32::WHITE);
        // Mute, then the volume slider.
        let mute = Rect::from_min_size(vol.min, vec2(18.0, 20.0));
        if ui.interact(mute, ui.id().with(("media-mute", &self.part)), Sense::click()).clicked() { self.toggle_mute(); }
        p.text(mute.center(), egui::Align2::CENTER_CENTER, if self.muted { "🔇" } else { "🔊" }, egui::FontId::proportional(12.0), Color32::WHITE);
        let vl = Rect::from_min_max(pos2(mute.max.x + 4.0, bar.center().y - 2.0), pos2(vol.max.x - 4.0, bar.center().y + 2.0));
        p.rect_filled(vl, 2.0, Color32::from_gray(110));
        p.rect_filled(Rect::from_min_max(vl.min, pos2(vl.min.x + vl.width() * self.volume, vl.max.y)), 2.0, Color32::WHITE);
        let vr = ui.interact(vl.expand2(vec2(0.0, 8.0)), ui.id().with(("media-vol", &self.part)), Sense::click_and_drag());
        if let (true, Some(pt)) = (vr.clicked() || vr.dragged(), vr.interact_pointer_pos()) { self.set_volume((pt.x - vl.min.x) / vl.width()); }
        if self.playing() { ui.ctx().request_repaint_after(std::time::Duration::from_millis(250)); }
    }
}

impl Drop for Player {
    fn drop(&mut self) { if let Some(mut c) = self.decoder.take() { let _ = c.kill(); let _ = c.wait(); } }
}

fn probe_size(file: &std::path::Path) -> Option<(usize, usize)> {
    let o = std::process::Command::new(tool("ffprobe")?).args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=s=x:p=0"]).arg(file).output().ok()?;
    let s = String::from_utf8_lossy(&o.stdout);
    let (w, h) = s.trim().split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// The media objects of a slide: (top-level index, media part, video?, screen frame) given
/// the slide's screen rectangle.
pub fn objects(pres: &slides_engine::Presentation, part: &str, slide: Rect) -> Vec<(usize, String, bool, Rect)> {
    let scene = pres.scene(part);
    let k = slide.width() / scene.w as f32;
    scene.items.iter().filter(|i| i.own && i.loc.path.len() == 1).filter_map(|it| {
        let loc = slides_engine::scene::Loc { part: part.to_string(), path: vec![it.loc.path[0]] };
        let m = pres.media_part(&loc)?;
        let video = pres.element(&loc).is_some_and(|e| e.find("a:videoFile").is_some());
        let mut r = Rect::NOTHING;
        for (x, y) in [(0.0, 0.0), (it.w, 0.0), (it.w, it.h), (0.0, it.h)] { let (a, b) = it.transform.apply(x, y); r.extend_with(pos2(slide.min.x + a as f32 * k, slide.min.y + b as f32 * k)); }
        Some((it.loc.path[0], m, video, r))
    }).collect()
}

/// Media on a slide: the playing one's picture and, when wanted, controls (under the
/// pointer, or while paused). Returns the media part clicked to play or pause, if any.
pub fn overlay(player: &mut Option<Player>, pres: &slides_engine::Presentation, ui: &mut egui::Ui, part: &str, slide: Rect, controls: bool) {
    let ctx = ui.ctx().clone();
    let hover = ctx.input(|i| i.pointer.hover_pos());
    for (_, m, _, r) in objects(pres, part, slide) {
        let Some(pl) = player.as_mut().filter(|p| p.part == m) else { continue };
        pl.paint(&ctx, ui.painter(), r);
        let near = hover.is_some_and(|h| r.expand2(vec2(0.0, 32.0)).contains(h));
        if controls && (near || !pl.playing()) { pl.controls(ui, r); }
    }
}

/// Play or pause the media part (opening it the first time).
pub fn toggle(player: &mut Option<Player>, pres: &slides_engine::Presentation, part: &str, video: bool) {
    match player {
        Some(p) if p.part == part => p.toggle(),
        _ => { *player = pres.pkg.bytes(part).and_then(|b| Player::open(part, &b, video)); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_a_video_in_place() {
        let Some(ff) = tool("ffmpeg") else { eprintln!("no ffmpeg; skipped"); return };
        let dir = std::env::temp_dir().join("sard-media-test");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("clip.mp4");
        let ok = std::process::Command::new(ff).args(["-y", "-v", "error", "-f", "lavfi", "-i", "testsrc=size=320x180:rate=30", "-f", "lavfi", "-i", "anullsrc=r=44100:cl=stereo", "-t", "2", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"]).arg(&file).status().is_ok_and(|s| s.success());
        if !ok { eprintln!("ffmpeg couldn't make a clip; skipped"); return; }
        let bytes = std::fs::read(&file).unwrap();
        let mut p = Player::open("ppt/media/media1.mp4", &bytes, true).unwrap();
        p.set_volume(0.0);
        assert!((p.duration.unwrap() - 2.0).abs() < 0.2);
        std::thread::sleep(std::time::Duration::from_millis(600));
        assert!(p.playing());
        assert!(p.position() > 0.3);
        let f = p.frame.lock().unwrap().clone().expect("a decoded frame");
        assert_eq!((f.1, f.2), (320, 180));
        p.pause();
        let at = p.position();
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert_eq!(p.position(), at);
        p.seek_to(1.5);
        assert!((p.position() - 1.5).abs() < 0.01);
        p.play();
        std::thread::sleep(std::time::Duration::from_millis(800));
        assert!(p.ended() || p.position() > 1.9);
    }
}
