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
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let shared_candidates = [
        // 开发安装：~/.local/share/xime/rime-data
        std::path::PathBuf::from(&home).join(".local/share/xime/rime-data"),
        // 系统安装：/usr/share/xime/rime-data
        std::path::PathBuf::from("/usr/share/xime/rime-data"),
    ];
    let shared_data_dir = shared_candidates
        .iter()
        .find(|dir| dir.join("default.yaml").exists())
        .cloned()
        .unwrap_or_else(|| shared_candidates[0].clone());
    let user_data_dir = std::path::PathBuf::from(&home).join(".config/xime/rime");
    let _ = xime_config::set_rime_paths(xime_config::RimePaths {
        shared_data_dir,
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
