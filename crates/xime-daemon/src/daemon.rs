use std::sync::mpsc::Sender;
use std::os::unix::io::AsRawFd;
use std::os::fd::AsFd;
use nix::unistd::dup;
use zbus::{interface};
use zbus::zvariant::Fd;

use crate::{DaemonCommand, log_msg};

pub struct XimeDaemon {
    command_tx: Sender<DaemonCommand>,
}

impl Clone for XimeDaemon {
    fn clone(&self) -> Self {
        Self {
            command_tx: self.command_tx.clone(),
        }
    }
}

impl XimeDaemon {
    pub fn new(command_tx: Sender<DaemonCommand>) -> Self {
        Self { command_tx }
    }
}

#[interface(name = "org.xime.Xime.Controller")]
impl XimeDaemon {
    async fn open_wayland_socket(
        &self,
        fd: Fd<'_>,
        display: String,
    ) -> zbus::fdo::Result<()> {
        let raw_fd = fd.as_raw_fd();
        log_msg!("DEBUG: Received OpenWaylandSocket(fd={}, display={})", raw_fd, display);
        
        let owned_fd = dup(fd.as_fd())
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to dup fd: {}", e)))?;
        
        self.command_tx
            .send(DaemonCommand::OpenWaylandSocket(owned_fd, display))
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        
        Ok(())
    }
    
    async fn deploy(&self) -> zbus::fdo::Result<()> {
        log_msg!("DEBUG: Received Deploy request");
        self.command_tx
            .send(DaemonCommand::Deploy)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }
    
    async fn reload_style(&self) -> zbus::fdo::Result<()> {
        log_msg!("DEBUG: Received ReloadStyle request");
        self.command_tx
            .send(DaemonCommand::ReloadStyle)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }
}