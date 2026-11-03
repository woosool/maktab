// The Windows program icon and version info (Explorer, taskbar, Alt+Tab).
fn main() {
    // Also when cross-compiling from macOS/Linux (MinGW's windres).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../assets/icon.ico").set("ProductName", "Qalam").set("FileDescription", "Qalam word processor").set("CompanyName", "Woosool");
        res.compile().expect("Windows resources");
    }
    println!("cargo:rerun-if-changed=../assets/icon.ico");
}
