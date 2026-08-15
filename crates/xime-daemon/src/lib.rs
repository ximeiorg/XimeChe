mod command;
mod daemon;
mod plugin_host;
mod rime;
mod wayland;

pub use command::DaemonCommand;
pub use daemon::XimeDaemon;
pub use plugin_host::{plugins_dir, PluginHost};
pub use rime::RimeEngine;
pub use wayland::WaylandLoop;

pub fn get_config_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    std::path::PathBuf::from(home).join(".config/xime/rime")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_config_dir_format() {
        let dir = get_config_dir();
        let path_str = dir.to_string_lossy();
        assert!(
            path_str.ends_with("/.config/xime/rime"),
            "Config dir should end with '.config/xime/rime', got: {}",
            path_str
        );
    }

    #[test]
    fn test_get_config_dir_is_absolute() {
        let dir = get_config_dir();
        assert!(dir.is_absolute(), "Config dir should be absolute");
    }

    #[test]
    fn test_get_config_dir_has_rime_component() {
        let dir = get_config_dir();
        assert!(
            dir.components().any(|c| c.as_os_str() == "rime"),
            "Config dir should contain 'rime' component"
        );
    }
}
