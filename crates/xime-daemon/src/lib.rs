mod command;
mod daemon;
mod rime;
mod wayland;

pub use command::DaemonCommand;
pub use daemon::XimeDaemon;
pub use rime::RimeEngine;
pub use wayland::WaylandLoop;

pub fn log_file() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    std::path::PathBuf::from(home).join(".config/xime/xime.log")
}

#[macro_export]
macro_rules! log_msg {
    ($($arg:tt)*) => {
        {
            use std::io::Write;
            let msg = format!("[{}] {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), format!($($arg)*));
            eprintln!("{}", msg.trim());
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open($crate::log_file())
            {
                let _ = f.write_all(msg.as_bytes());
            }
        }
    };
}

pub fn get_config_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    std::path::PathBuf::from(home).join(".config/xime/rime")
}