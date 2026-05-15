use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use zbus::Connection;
use xime_tray::{TrayManager, MenuAction};

use xime_daemon::{XimeDaemon, DaemonCommand, WaylandLoop, log_msg};

fn main() -> anyhow::Result<()> {
    log_msg!("DEBUG: xime-daemon starting");
    
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
        
        log_msg!("DEBUG: DBus service registered at org.xime.Xime");
        log_msg!("DEBUG: Tray icon registered");
        log_msg!("DEBUG: Waiting for Wayland connection from launcher...");
        
        loop {
            tokio::select! {
                Some(_) = toggle_rx.recv() => {
                    log_msg!("DEBUG: Toggle request received from tray");
                    command_tx.send(DaemonCommand::ToggleMode).ok();
                }
                Some(action) = action_rx.recv() => {
                    log_msg!("DEBUG: Menu action received: {:?}", action);
                    match action {
                        MenuAction::ToggleMode => {
                            command_tx.send(DaemonCommand::ToggleMode).ok();
                        }
                        MenuAction::Settings => {
                            let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
                            let setup_path = format!("{}/.local/bin/xime-setup", home);
                            std::process::Command::new(&setup_path)
                                .spawn()
                                .map_err(|e| log_msg!("ERROR: Failed to launch xime-setup: {}", e))
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