//! Interface colors (editor-chrome.css): the light theme, or Dark Mode's palette.

use egui::Color32;

// Chrome tokens from editor-chrome.css: the light theme, or Dark Mode's palette.
thread_local! {
    /// Dark Mode for the interface chrome.
    pub static DARK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
pub fn dark() -> bool { DARK.with(|d| d.get()) }
fn pick(light: (u8, u8, u8), dark_rgb: (u8, u8, u8)) -> Color32 { let c = if dark() { dark_rgb } else { light }; Color32::from_rgb(c.0, c.1, c.2) }
pub fn c_frame() -> Color32 { pick((0xF3, 0xF3, 0xF3), (0x1F, 0x1F, 0x1F)) }
pub fn c_card() -> Color32 { pick((0xFF, 0xFF, 0xFF), (0x2B, 0x2B, 0x2B)) }
pub fn c_ink() -> Color32 { pick((0x24, 0x24, 0x24), (0xF2, 0xF2, 0xF2)) }
pub fn c_ink2() -> Color32 { pick((0x42, 0x42, 0x42), (0xDC, 0xDC, 0xDC)) }
pub fn c_ink3() -> Color32 { pick((0x61, 0x61, 0x61), (0xA8, 0xA8, 0xA8)) }
pub fn c_line() -> Color32 { pick((0xE0, 0xE0, 0xE0), (0x40, 0x40, 0x40)) }
pub fn c_hover() -> Color32 { pick((0xF0, 0xF0, 0xF0), (0x3A, 0x3A, 0x3A)) }
pub fn c_press() -> Color32 { pick((0xE0, 0xE0, 0xE0), (0x4A, 0x4A, 0x4A)) }
pub const ACCENT: Color32 = Color32::from_rgb(0x18, 0x5A, 0xBD);

