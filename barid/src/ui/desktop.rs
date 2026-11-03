use super::*;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(super) struct Desktop {
    tray: tray_icon::TrayIcon,
    last: usize,
    items: Vec<(tray_icon::menu::MenuItem, String)>,
    language: crate::i18n::Language,
    #[cfg(target_os = "windows")]
    rx: std::sync::mpsc::Receiver<String>,
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) struct Desktop;
#[cfg(any(target_os = "macos", target_os = "windows"))]
impl Desktop {
    pub fn new(_ctx: &egui::Context) -> Result<Self, String> {
        use tray_icon::{
            Icon, TrayIconBuilder,
            menu::{Menu, MenuItem},
        };
        #[cfg(target_os = "windows")]
        let rx = {
            let (tx, rx) = std::sync::mpsc::channel();
            let repaint = _ctx.clone();
            tray_icon::menu::MenuEvent::set_event_handler(Some(
                move |e: tray_icon::menu::MenuEvent| {
                    let _ = tx.send(e.id.0);
                    repaint.request_repaint();
                },
            ));
            rx
        };
        let menu = Menu::new();
        let mut items = Vec::new();
        for (id, title) in [
            ("tray_show", "Show Barid"),
            ("tray_new", "New message"),
            ("tray_sync", "Refresh"),
            ("tray_quit", "Quit Barid"),
        ] {
            let item = MenuItem::with_id(id, crate::i18n::tr(title), true, None);
            menu.append(&item).map_err(|e| e.to_string())?;
            items.push((item, title.into()));
        }
        let tray = TrayIconBuilder::new()
            .with_tooltip("Barid")
            .with_icon(Icon::from_rgba(icon(0), 32, 32).map_err(|e| e.to_string())?)
            .with_menu(Box::new(menu))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            tray,
            last: usize::MAX,
            items,
            language: crate::i18n::language(),
            #[cfg(target_os = "windows")]
            rx,
        })
    }
    pub fn update(&mut self, count: usize, ctx: &egui::Context) {
        if self.language != crate::i18n::language() {
            self.language = crate::i18n::language();
            for (item, title) in &self.items {
                item.set_text(crate::i18n::tr(title));
            }
        }
        if self.last == count {
            return;
        }
        self.last = count;
        let _ = self
            .tray
            .set_tooltip(Some(format!("Barid · {count} unread")));
        let _ = self.tray.set_icon(Some(
            tray_icon::Icon::from_rgba(icon(count), 32, 32).expect("Fixed RGBA icon"),
        ));
        #[cfg(target_os = "macos")]
        if let Some(mtm) = objc2::MainThreadMarker::new() {
            use objc2_app_kit::NSApplication;
            use objc2_foundation::NSString;
            let app = NSApplication::sharedApplication(mtm);
            let badge = if count == 0 {
                None
            } else {
                Some(NSString::from_str(&count.to_string()))
            };
            app.dockTile().setBadgeLabel(badge.as_deref());
        }
        #[cfg(target_os = "windows")]
        ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(std::sync::Arc::new(
            egui::IconData {
                rgba: icon(count),
                width: 32,
                height: 32,
            },
        ))));
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(if count > 0 {
            format!("Barid ({count})")
        } else {
            "Barid".into()
        }));
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl Desktop {
    pub fn new(_ctx: &egui::Context) -> Result<Self, String> {
        Err("System tray is available in the macOS and Windows builds.".into())
    }
    pub fn update(&mut self, _count: usize, _ctx: &egui::Context) {}
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn icon(count: usize) -> Vec<u8> {
    let mut rgba = vec![0; 32 * 32 * 4];
    let mut pixel = |x: usize, y: usize, color: [u8; 4]| {
        rgba[(y * 32 + x) * 4..(y * 32 + x + 1) * 4].copy_from_slice(&color);
    };
    for y in 4..28 {
        for x in 2..30 {
            pixel(x, y, [24, 118, 224, 255]);
            if y == 7 || y == 24 || x == 5 || x == 26 {
                pixel(x, y, [255, 255, 255, 255]);
            }
            if (y as i32 - 8 - (x as i32 - 5).min(26 - x as i32)).abs() <= 1 && y < 19 {
                pixel(x, y, [255, 255, 255, 255]);
            }
        }
    }
    if count > 0 {
        for y in 15..32 {
            for x in 13..32 {
                pixel(x, y, [210, 45, 55, 255]);
            }
        }
        let digits = [
            0b111101101101111u16,
            0b010110010010111,
            0b111001111100111,
            0b111001111001111,
            0b101101111001001,
            0b111100111001111,
            0b111100111101111,
            0b111001001001001,
            0b111101111101111,
            0b111101111001111,
        ];
        let text = count.min(99).to_string();
        for (i, c) in text.chars().enumerate() {
            let bits = digits[c.to_digit(10).unwrap() as usize];
            for row in 0..5 {
                for col in 0..3 {
                    if (bits >> (14 - (row * 3 + col))) & 1 == 1 {
                        for dy in 0..2 {
                            for dx in 0..2 {
                                pixel(
                                    16 + i * 7 + col * 2 + dx,
                                    19 + row * 2 + dy,
                                    [255, 255, 255, 255],
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    rgba
}
impl Barid {
    pub(super) fn restore_window(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }
    pub(super) fn hide_to_tray(&mut self, ctx: &egui::Context) {
        if self.desktop.is_some() {
            self.save_all_composers();
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        } else {
            self.status = "The system tray is unavailable. Barid remains open.".into();
        }
    }
    pub(super) fn background(&mut self, ctx: &egui::Context) {
        self.events();
        if self.refresh_pending && self.busy == 0 {
            self.refresh_pending = false;
            self.sync();
        }
        #[cfg(target_os = "windows")]
        {
            let actions = self
                .desktop
                .as_ref()
                .map(|d| d.rx.try_iter().collect::<Vec<_>>())
                .unwrap_or_default();
            for id in actions {
                match id.as_str() {
                    "tray_show" => self.restore_window(ctx),
                    "tray_new" => {
                        self.restore_window(ctx);
                        self.compose("new");
                    }
                    "tray_sync" => self.sync(),
                    "tray_quit" => {
                        self.quitting = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    _ => {}
                }
            }
        }

        if self.desktop.is_none() && !self.desktop_attempted {
            self.desktop_attempted = true;
            match Desktop::new(ctx) {
                Ok(d) => self.desktop = Some(d),
                Err(e) => self.status = e,
            }
        }
        let count = if self.prefs.unread_badge {
            let mut seen = HashSet::new();
            self.mails
                .iter()
                .filter(|m| m.account != "demo" && m.unread && m.folder == "Inbox")
                .filter(|m| {
                    seen.insert((
                        m.account.clone(),
                        if m.message_id.is_empty() {
                            m.id.clone()
                        } else {
                            m.message_id.clone()
                        },
                    ))
                })
                .count()
        } else {
            0
        };
        if let Some(desktop) = &mut self.desktop {
            desktop.update(count, ctx);
        }
        if self.prefs.minimize_to_tray
            && !self.quitting
            && self.desktop.is_some()
            && ctx.input(|i| i.viewport().close_requested())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_to_tray(ctx);
        }
        if self.prefs.sync_minutes > 0
            && self.busy == 0
            && self.accounts.iter().any(|a| a.enabled)
            && self.last_sync.elapsed().as_secs() >= u64::from(self.prefs.sync_minutes) * 60
        {
            self.sync();
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(if self.busy > 0 {
            1
        } else {
            30
        }));
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
mod tests {
    use super::*;
    #[test]
    fn unread_icons_are_valid_at_zero_and_large_counts() {
        for count in [0, 1, 9, 10, 99, 100, 1000] {
            let rgba = icon(count);
            assert_eq!(rgba.len(), 32 * 32 * 4);
            assert!(rgba.iter().any(|v| *v != 0));
        }
    }
}
