use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use tracing::{debug, error, info};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use tracing_appender::non_blocking::WorkerGuard;
use zbus::Connection;
use xime_tray::{TrayManager, MenuAction};

use xime_daemon::{XimeDaemon, DaemonCommand, WaylandLoop};

fn init_tracing() -> WorkerGuard {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let log_dir = std::path::PathBuf::from(home).join(".config/xime");
    
    if !log_dir.exists() {
        std::fs::create_dir_all(&log_dir).ok();
    }
    
    let file_appender = tracing_appender::rolling::never(&log_dir, "xime.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    
    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false);
    
    let stdout_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true);
    
    #[cfg(debug_assertions)]
    let default_level = tracing::Level::DEBUG;
    #[cfg(not(debug_assertions))]
    let default_level = tracing::Level::INFO;
    
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env()
            .add_directive(default_level.into()))
        .with(file_layer)
        .with(stdout_layer)
        .init();
    
    guard
}

fn main() -> anyhow::Result<()> {
    let _guard = init_tracing();
    info!("xime-daemon starting");
    
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
        
        connection.object_server()
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