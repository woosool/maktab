//! Page bitmaps: rendered off the UI thread and cached as textures.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui;
use pdf_engine::{RenderedPage, Renderer};

/// What a bitmap is for; each purpose has its own resolution ladder.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Purpose {
    Page,
    Thumb,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Key {
    page: usize,
    purpose: Purpose,
}

struct Entry {
    texture: egui::TextureHandle,
    /// Pixels per point it was rendered at.
    scale: f32,
    revision: u64,
}

enum Job {
    Document { revision: u64, bytes: Arc<Vec<u8>> },
    Render { revision: u64, key: Key, scale: f32 },
}

struct Done {
    revision: u64,
    key: Key,
    scale: f32,
    image: Option<RenderedPage>,
}

/// Texture cache backed by a rendering worker thread (or inline rendering in tests).
pub struct Raster {
    revision: u64,
    entries: HashMap<Key, Entry>,
    pending: HashSet<(Key, u32)>,
    jobs: Option<Sender<Job>>,
    done: Receiver<Done>,
    done_tx: Sender<Done>,
    inline: Option<Renderer>,
}

impl Raster {
    /// `threaded = false` renders synchronously on request (deterministic for tests).
    pub fn new(ctx: &egui::Context, threaded: bool) -> Self {
        let (done_tx, done) = channel::<Done>();
        let jobs = threaded.then(|| {
            let (tx, rx) = channel::<Job>();
            let out = done_tx.clone();
            let ctx = ctx.clone();
            std::thread::Builder::new()
                .name("pdf-render".into())
                .spawn(move || worker(rx, out, ctx))
                .expect("spawn render thread");
            tx
        });
        Self {
            revision: u64::MAX,
            entries: HashMap::new(),
            pending: HashSet::new(),
            jobs,
            done,
            done_tx,
            inline: None,
        }
    }

    /// Points the cache at a new document state. With `keep_stale`, old bitmaps
    /// stay visible until fresh ones arrive (fine when page order is unchanged).
    pub fn set_document(&mut self, revision: u64, bytes: Arc<Vec<u8>>, keep_stale: bool) {
        if revision == self.revision {
            return;
        }
        self.revision = revision;
        self.pending.clear();
        if !keep_stale {
            self.entries.clear();
        }
        match &self.jobs {
            Some(tx) => {
                let _ = tx.send(Job::Document { revision, bytes });
            }
            None => self.inline = Renderer::new(bytes),
        }
    }

    /// Drops everything, e.g. when another file is opened.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.pending.clear();
        self.revision = u64::MAX;
    }

    /// Collects finished renders into textures.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(done) = self.done.try_recv() {
            self.pending.remove(&(done.key, bucket(done.scale)));
            if done.revision != self.revision {
                continue;
            }
            let Some(img) = done.image else { continue };
            let image = egui::ColorImage::from_rgba_premultiplied(
                [img.width as usize, img.height as usize],
                &img.rgba,
            );
            let name = format!("page-{}-{:?}", done.key.page, done.key.purpose);
            match self.entries.get_mut(&done.key) {
                Some(e) => {
                    e.texture.set(image, egui::TextureOptions::LINEAR);
                    e.scale = done.scale;
                    e.revision = done.revision;
                }
                None => {
                    let texture = ctx.load_texture(name, image, egui::TextureOptions::LINEAR);
                    self.entries.insert(
                        done.key,
                        Entry {
                            texture,
                            scale: done.scale,
                            revision: done.revision,
                        },
                    );
                }
            }
        }
    }

    /// The best texture for `page`, requesting a sharper or fresher one when needed.
    pub fn texture(
        &mut self,
        ctx: &egui::Context,
        page: usize,
        purpose: Purpose,
        scale: f32,
    ) -> Option<egui::TextureId> {
        let key = Key { page, purpose };
        let want = quantize(scale);
        let current = self.entries.get(&key);
        let fresh =
            current.is_some_and(|e| e.revision == self.revision && (e.scale - want).abs() < 0.01);
        if !fresh && !self.pending.contains(&(key, bucket(want))) {
            self.pending.insert((key, bucket(want)));
            self.request(key, want);
            // Inline rendering finishes right away.
            if self.jobs.is_none() {
                self.poll(ctx);
            }
        }
        self.entries.get(&key).map(|e| e.texture.id())
    }

    fn request(&mut self, key: Key, scale: f32) {
        match &self.jobs {
            Some(tx) => {
                let _ = tx.send(Job::Render {
                    revision: self.revision,
                    key,
                    scale,
                });
            }
            None => {
                let image = self.inline.as_ref().and_then(|r| r.render(key.page, scale));
                let _ = self.done_tx.send(Done {
                    revision: self.revision,
                    key,
                    scale,
                    image,
                });
            }
        }
    }
}

/// Rendering resolutions come in steps so small zoom changes reuse bitmaps.
fn quantize(scale: f32) -> f32 {
    let steps = [0.25, 0.35, 0.5, 0.7, 1.0, 1.4, 2.0, 2.8, 4.0, 5.6, 8.0];
    steps
        .iter()
        .copied()
        .find(|s| *s >= scale * 0.98)
        .unwrap_or(8.0)
}

fn bucket(scale: f32) -> u32 {
    (scale * 100.0).round() as u32
}

fn worker(rx: Receiver<Job>, out: Sender<Done>, ctx: egui::Context) {
    let mut renderer: Option<(u64, Renderer)> = None;
    let mut queue: Vec<(u64, Key, f32)> = Vec::new();
    loop {
        // Block for work when idle, then drain everything that is waiting.
        if queue.is_empty() {
            match rx.recv() {
                Ok(job) => absorb(job, &mut renderer, &mut queue),
                Err(_) => return,
            }
        }
        while let Ok(job) = rx.try_recv() {
            absorb(job, &mut renderer, &mut queue);
        }
        // Newest requests first: they are what is on screen now.
        let Some((revision, key, scale)) = queue.pop() else {
            continue;
        };
        let image = match &renderer {
            Some((rev, r)) if *rev == revision => r.render(key.page, scale),
            _ => continue,
        };
        if out
            .send(Done {
                revision,
                key,
                scale,
                image,
            })
            .is_err()
        {
            return;
        }
        ctx.request_repaint();
    }
}

fn absorb(job: Job, renderer: &mut Option<(u64, Renderer)>, queue: &mut Vec<(u64, Key, f32)>) {
    match job {
        Job::Document { revision, bytes } => {
            queue.clear();
            *renderer = Renderer::new(bytes).map(|r| (revision, r));
        }
        Job::Render {
            revision,
            key,
            scale,
        } => {
            queue.retain(|(_, k, _)| *k != key);
            queue.push((revision, key, scale));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantizes_up() {
        assert_eq!(quantize(1.0), 1.0);
        assert_eq!(quantize(1.1), 1.4);
        assert_eq!(quantize(0.1), 0.25);
        assert_eq!(quantize(50.0), 8.0);
    }
}
