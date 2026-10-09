mod clipboard_sync;
mod command;
mod custom_phrase;
mod daemon;
mod emoji;
mod plugin_host;
mod recent_usage;
mod rime;
mod schema_dict;
mod speech;
mod symbols;
mod user_dict;
mod wayland;

pub use clipboard_sync::{scan_descriptors, spawn_bridge, SyncMessage, SyncPluginDescriptor};
pub use command::DaemonCommand;
pub use daemon::XimeDaemon;
pub use plugin_host::{plugins_dir, PluginHost};
pub use rime::RimeEngine;
pub use wayland::WaylandLoop;

use std::sync::OnceLock;
use tracing::{info, warn};
use tracing_subscriber::{reload, EnvFilter};
use xime_config::XimeConfig;

/// daemon 进程内"自家 crate"的 tracing target 前缀。`log_level` 配置只作用于
/// 这些 crate；三方库固定 info——它们的 debug 量极大（cosmic_text 每帧字体
/// 排版），而 tracing 的格式化发生在日志调用线程（这里就是按键处理线程），
/// 打字时逐条格式化等于白送延迟（同 libximecore DEFAULT_LOG_FILTER 的结论）。
const OWN_LOG_TARGETS: &[&str] = &[
    "xime_daemon",
    "xime_wayland",
    "xime_tray",
    "xime_clipboard",
    "xime_predict",
    "xime_ui",
    "xime_config",
    "librime",
];

/// 运行时日志过滤器的热重载句柄。RUST_LOG 显式覆盖时不注册——
/// 此时配置里的 log_level 不生效（环境变量优先，且不能被 ReloadStyle 偷偷冲掉）。
pub static LOG_FILTER_HANDLE: OnceLock<reload::Handle<EnvFilter, tracing_subscriber::Registry>> =
    OnceLock::new();

/// 由配置构建过滤器：三方库 info + 自家 crate = 配置级别。
pub fn build_log_filter(cfg: &XimeConfig) -> EnvFilter {
    EnvFilter::new(log_filter_spec(cfg))
}

/// 过滤器规格字符串（拆出来供测试断言）。
fn log_filter_spec(cfg: &XimeConfig) -> String {
    let level = cfg.effective_log_level();
    let mut spec = String::from("info");
    for target in OWN_LOG_TARGETS {
        spec.push_str(&format!(",{target}={level}"));
    }
    spec
}

/// ReloadStyle / 启动后应用配置中的日志级别。RUST_LOG 生效时为 no-op。
pub fn apply_log_level(cfg: &XimeConfig) {
    let Some(handle) = LOG_FILTER_HANDLE.get() else {
        debug_rust_log_override_notice();
        return;
    };
    let filter = build_log_filter(cfg);
    match handle.reload(filter) {
        Ok(()) => info!(
            "Log level applied: own crates = {} (third-party stays info)",
            cfg.effective_log_level()
        ),
        Err(e) => warn!("Failed to apply log level: {e}"),
    }
}

/// 只在 RUST_LOG 覆盖生效时提示一次配置被忽略（每个进程至多一条 debug）。
fn debug_rust_log_override_notice() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static NOTICED: AtomicBool = AtomicBool::new(false);
    if !NOTICED.swap(true, Ordering::Relaxed) {
        info!("RUST_LOG override active, config log_level ignored");
    }
}

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

    /// 过滤器规格必须能被 EnvFilter 解析（写错指令会被静默丢掉、无人察觉），
    /// 且自家 crate 拿到配置级别、三方库固定 info。
    #[test]
    fn test_build_log_filter_valid_and_scoped() {
        let spec = log_filter_spec(&XimeConfig {
            log_level: Some("debug".to_string()),
            ..XimeConfig::default()
        });
        assert!(
            tracing_subscriber::EnvFilter::try_new(&spec).is_ok(),
            "过滤器规格无法解析: {spec}"
        );
        assert!(spec.starts_with("info,"), "三方库必须固定 info: {spec}");
        for target in ["xime_daemon", "xime_wayland", "xime_tray"] {
            assert!(
                spec.contains(&format!("{target}=debug")),
                "缺少自家 crate 的配置级别指令: {spec}"
            );
        }

        // 非法级别回退 info（xime-config 侧已测，这里锁守护端行为）。
        let spec = log_filter_spec(&XimeConfig {
            log_level: Some("verbose".to_string()),
            ..XimeConfig::default()
        });
        assert!(spec.contains("xime_daemon=info"));
    }
}
