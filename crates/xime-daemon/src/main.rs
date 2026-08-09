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

/// 若用户 rime 目录还没有方案文件，则从系统共享目录（/usr/share/xime/rime-data）
/// 复制 rime-wubi 方案到用户目录，保证单目录统一。
fn seed_rime_schemas(user_rime_dir: &std::path::Path) {
    let shared_src = std::path::PathBuf::from("/usr/share/xime/rime-data");
    if !shared_src.exists() || user_rime_dir.join("default.yaml").exists() {
        return;
    }
    if let Err(e) = std::fs::create_dir_all(user_rime_dir) {
        error!("Failed to create rime dir: {}", e);
        return;
    }
    if copy_dir_recursive(&shared_src, user_rime_dir).is_ok() {
        info!(
            "Seeded rime-wubi schemas from {} to {}",
            shared_src.display(),
            user_rime_dir.display()
        );
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            std::fs::copy(&path, &target).map(|_| ())?;
        }
    }
    Ok(())
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

    let file_appender = tracing_appender::rolling::never(&log_dir, "xime.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_timer(LocalTimer);

    let stdout_layer = fmt::layer().with_writer(std::io::stderr).with_ansi(true);

    let default_level = tracing::Level::DEBUG;

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(default_level.into()))
        .with(file_layer)
        .with(stdout_layer)
        .init();

    guard
}

fn main() -> anyhow::Result<()> {
    let _guard = init_tracing();
    info!("xime-daemon starting");

    // 注入 Rime 数据目录（由本应用决定，libximecore 只提供接口）。
    // 统一使用 ~/.config/xime/rime 单目录，不拆分 shared/user。
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let user_data_dir = std::path::PathBuf::from(&home).join(".config/xime/rime");
    seed_rime_schemas(&user_data_dir);
    let _ = xime_config::set_rime_paths(xime_config::RimePaths {
        shared_data_dir: user_data_dir.clone(),
        user_data_dir,
    });

    let rt = tokio::runtime::Runtime::new()?;
    let rt_handle = rt.handle().clone();

    rt.block_on(async {
        let connection = Connection::session().await?;

        let (tray, mut toggle_rx, mut action_rx) = TrayManager::register(&connection).await?;
        let tray = Arc::new(tray);

        let (command_tx, command_rx) = mpsc::channel();

        thread::spawn({
            let tray = tray.clone();
            let rt_handle = rt_handle.clone();
            move || {
                let wayland_loop = WaylandLoop::new(command_rx, tray, rt_handle);
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
        info!("Tray icon registered");
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
