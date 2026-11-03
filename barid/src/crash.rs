//! Preserve the original panic when a later cleanup panic aborts the process.
use std::{
    io::Write,
    sync::atomic::{AtomicBool, Ordering},
};

pub fn install() {
    let default = std::panic::take_hook();
    let reported = AtomicBool::new(false);
    std::panic::set_hook(Box::new(move |info| {
        if !reported.swap(true, Ordering::Relaxed)
            && let Some(dirs) = directories::ProjectDirs::from("com", "barid", "Barid")
        {
            let directory = dirs.data_dir().join("diagnostics");
            let path = directory.join("crash-report.log");
            if std::fs::create_dir_all(&directory).is_ok() {
                if std::fs::metadata(&path).is_ok_and(|m| m.len() > 2_000_000) {
                    let _ = std::fs::rename(&path, directory.join("crash-report.previous.log"));
                }
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                {
                    let _ = writeln!(
                        file,
                        "\nBarid {} · {} · {}\nOriginal panic: {info}\n{}",
                        env!("CARGO_PKG_VERSION"),
                        std::env::consts::OS,
                        chrono::Utc::now(),
                        std::backtrace::Backtrace::force_capture()
                    );
                    let _ = file.flush();
                }
            }
        }
        default(info);
    }));
}
