use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use chrono::Local;
use tracing::{debug, error, info};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use xime_tray::{MenuAction, TrayManager};
use zbus::Connection;

use xime_daemon::{DaemonCommand, WaylandLoop, XimeDaemon};

fn get_log_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    std::path::PathBuf::from(home).join(".config/xime")
}

fn init_tracing() -> WorkerGuard {
    let log_dir = get_log_dir();

    if !log_dir.exists() {
        std::fs::create_dir_all(&log_dir).ok();
    }

    struct LocalTimer;
    impl FormatTime for LocalTimer {
        fn format_time(
            &self,
            w: &mut tracing_subscriber::fmt::format::Writer<'_>,
        ) -> std::fmt::Result {
            write!(w, "{}", Local::now().format("%Y-%m-%dT%H:%M:%S%.6f%:z"))
        }
    }

    // 按天轮转，避免单文件无限增长；默认 INFO，需要 DEBUG 时用 RUST_LOG 覆盖
    // （如 RUST_LOG=debug 或 RUST_LOG=cosmic_text=debug,xime_daemon=debug）。
    let file_appender = tracing_appender::rolling::daily(&log_dir, "xime.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_timer(LocalTimer);

    let stdout_layer = fmt::layer().with_writer(std::io::stderr).with_ansi(true);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .init();

    guard
}

fn main() -> anyhow::Result<()> {
    let _guard = init_tracing();
    // pid 进日志：方便和 coredumpctl / journal 的记录互相对上。
    info!("xime-daemon starting (pid={})", std::process::id());

    // 注入应用元数据（目录沿用 xime，librime 分发标识为 XimeChe）。
    let _ = xime_config::set_app_metadata(xime_config::AppMetadata {
        display_name: "曦码·澈输入法",
        config_dir_name: "xime",
        config_file_base: "xime",
        distribution_name: "XimeChe",
        distribution_code_name: "ximeche",
        app_name: "rime.xime.daemon",
        version: env!("CARGO_PKG_VERSION"),
    });

    // 单目录模型（对齐 XimeYao / Xime 3.0）：shared == user == ~/.config/xime/rime。
    // 随包方案数据（dev-install 装到 ~/.local/share/xime/rime-data 或系统的
    // /usr/share/xime/rime-data）降级为「数据源」，启动时部署进 rime 目录：
    // 首装全量、升级只强更内容有变化且非 custom 的文件（保护用户定制与弃用方案）。
    // 旧 shared/user 分离导致方案来源混乱（码表在 shared、用户数据在 user，
    // 词典/词表读取到处回退查找）。
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let rime_dir = std::path::PathBuf::from(&home).join(".config/xime/rime");
    let bundled_sources = [
        std::path::PathBuf::from(&home).join(".local/share/xime/rime-data"),
        std::path::PathBuf::from("/usr/share/xime/rime-data"),
    ];
    xime_config::ensure_bundled_rime_data(&bundled_sources, &rime_dir);
    let paths = xime_config::RimePaths {
        shared_data_dir: rime_dir.clone(),
        user_data_dir: rime_dir,
    };
    info!("rime dir (single): {}", paths.user_data_dir.display());
    let _ = xime_config::set_rime_paths(paths);

    let rt = tokio::runtime::Runtime::new()?;
    let rt_handle = rt.handle().clone();

    rt.block_on(async {
        let connection = Connection::session().await?;

        let (tray, mut toggle_rx, mut action_rx) = TrayManager::register(&connection).await?;
        let tray = Arc::new(tray);

        let (command_tx, command_rx) = mpsc::channel();

        // 剪贴板/快捷发送存储（SQLite，表结构对齐 Android）+ 系统剪贴板监听
        // + 剪贴板同步桥（clipboard_sync 插件，独立线程避免网络阻塞按键）。
        let clipboard_store = xime_clipboard::store::init(xime_clipboard::default_db_dir());
        let (sync_tx, sync_rx) = mpsc::channel();
        xime_daemon::spawn_bridge(clipboard_store.clone(), sync_rx);
        let watcher_sync_tx = sync_tx.clone();
        xime_clipboard::watcher::spawn_watcher_with_callback(
            clipboard_store.clone(),
            Some(Box::new(move |text| {
                let _ = watcher_sync_tx.send(xime_daemon::SyncMessage::Captured(text.to_string()));
            })),
        );

        // 系统亮/暗色模式监听（org.freedesktop.portal.Settings 的
        // color-scheme，KDE/GNOME 均支持）。portal 不可用时保持亮色。
        rt.spawn({
            let connection = connection.clone();
            let command_tx = command_tx.clone();
            async move {
                if let Err(e) = watch_color_scheme(connection, command_tx).await {
                    debug!("Color scheme watcher unavailable: {}", e);
                }
            }
        });

        thread::spawn({
            let tray = tray.clone();
            let rt_handle = rt_handle.clone();
            move || {
                let wayland_loop =
                    WaylandLoop::new(command_rx, tray, rt_handle, clipboard_store, sync_tx);
                wayland_loop.run();
            }
        });

        let daemon = XimeDaemon::new(command_tx.clone());

        connection
            .object_server()
            .at("/org/xime/Xime", daemon)
            .await?;

        connection.request_name("org.xime.Xime").await?;

        info!("DBus service registered at org.xime.Xime");
        info!("Tray registered (background retry if watcher was not up yet)");

        // 托盘常驻显示（fcitx5 风格）：图标是 IM 未激活时唯一的控制入口，
        // 隐藏会让用户在输入法"卡死"（KWin 未激活）时失去恢复手段。
        tray.set_visible(true).await;
        info!("Waiting for Wayland connection from launcher...");

        loop {
            tokio::select! {
                Some(_) = toggle_rx.recv() => {
                    debug!("Toggle request received from tray");
                    command_tx.send(DaemonCommand::ToggleMode).ok();
                }
                Some(action) = action_rx.recv() => {
                    debug!("Menu action received: {:?}", action);
                    match action {
                        MenuAction::ToggleMode => {
                            command_tx.send(DaemonCommand::ToggleMode).ok();
                        }
                        MenuAction::Settings => {
                            let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
                            let setup_path = format!("{}/.local/bin/xime-setup", home);
                            std::process::Command::new(&setup_path)
                                .spawn()
                                .map_err(|e| {
                                    error!("Failed to launch xime-setup: {}", e);
                                    e
                                })
                                .ok();
                        }
                        MenuAction::Deploy => {
                            command_tx.send(DaemonCommand::Deploy).ok();
                        }
                        MenuAction::SelectSchema(schema_id) => {
                            // 结果接收端即弃：托盘切换不关心结果，
                            // wayland 侧 send 失败已被 `let _` 忽略。
                            let (result_tx, _result_rx) = tokio::sync::oneshot::channel();
                            command_tx
                                .send(DaemonCommand::SelectSchema(schema_id, result_tx))
                                .ok();
                        }
                        MenuAction::Exit => {
                            command_tx.send(DaemonCommand::Shutdown).ok();
                            break;
                        }
                    }
                }
            }
        }

        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}

/// 监听 portal 的 `color-scheme` 设置变化，向 daemon 发送 DarkMode 命令。
/// 值语义：0 = 无偏好，1 = 偏好暗色，2 = 偏好亮色。
async fn watch_color_scheme(
    connection: Connection,
    command_tx: mpsc::Sender<DaemonCommand>,
) -> zbus::Result<()> {
    use futures_lite::StreamExt;
    use zbus::zvariant::OwnedValue;

    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
    )
    .await?;

    // 初值：Read 返回 (v)，v 为 u32
    let reply: (OwnedValue,) = proxy
        .call("Read", &("org.freedesktop.appearance", "color-scheme"))
        .await?;
    if let Ok(mode) = reply.0.downcast_ref::<u32>() {
        info!("System color scheme: mode={}", mode);
        let _ = command_tx.send(DaemonCommand::DarkMode(mode == 1));
    }

    let mut changes = proxy.receive_signal("SettingChanged").await?;
    while let Some(msg) = changes.next().await {
        let Ok((namespace, key, value)) = msg.body().deserialize::<(String, String, OwnedValue)>()
        else {
            continue;
        };
        if namespace == "org.freedesktop.appearance" && key == "color-scheme" {
            if let Ok(mode) = value.downcast_ref::<u32>() {
                debug!("System color scheme changed: mode={}", mode);
                let _ = command_tx.send(DaemonCommand::DarkMode(mode == 1));
            }
        }
    }
    Ok(())
}
