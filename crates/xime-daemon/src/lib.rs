mod command;
mod daemon;
mod rime;
mod wayland;

pub use command::DaemonCommand;
pub use daemon::XimeDaemon;
pub use rime::RimeEngine;
pub use wayland::WaylandLoop;

pub fn get_config_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    std::path::PathBuf::from(home).join(".config/xime/rime")
}
