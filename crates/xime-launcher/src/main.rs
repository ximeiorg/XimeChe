use std::env;
use std::os::unix::io::{OwnedFd, AsRawFd, FromRawFd};
use nix::unistd::dup;
use zbus::Connection;
use zbus::zvariant::Fd;

const XIME_DBUS_NAME: &str = "org.xime.Xime";
const XIME_DBUS_PATH: &str = "/org/xime/Xime";
const XIME_DBUS_IFACE: &str = "org.xime.Xime.Controller";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No WAYLAND_SOCKET environment variable")]
    NoWaylandSocket,
    
    #[error("Failed to parse WAYLAND_SOCKET: {0}")]
    ParseError(#[from] std::num::ParseIntError),
    
    #[error("DBus error: {0}")]
    DBus(#[from] zbus::Error),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

fn get_wayland_socket_fd() -> Result<OwnedFd, Error> {
    let socket_env = env::var("WAYLAND_SOCKET")
        .map_err(|_| Error::NoWaylandSocket)?;
    
    let fd: i32 = socket_env.parse()?;
    
    let owned_fd = unsafe { OwnedFd::from_raw_fd(fd) };
    Ok(owned_fd)
}

async fn connect_to_daemon(fd: OwnedFd) -> Result<(), Error> {
    let display = env::var("WAYLAND_DISPLAY")
        .unwrap_or_else(|_| "wayland-0".to_string());
    
    eprintln!("DEBUG: Duplicating fd {} for DBus transfer", fd.as_raw_fd());
    
    // Duplicate fd because we need to keep the original alive until DBus call completes
    let dup_fd = dup(fd.as_raw_fd())
        .map_err(|e| Error::Io(std::io::Error::from_raw_os_error(e as i32)))?;
    let owned_dup = unsafe { OwnedFd::from_raw_fd(dup_fd) };
    
    let connection = Connection::session().await?;
    
    // Activate daemon first
    eprintln!("DEBUG: Activating daemon via DBus");
    let _ = connection.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "StartServiceByName",
        &(XIME_DBUS_NAME, 0u32),
    ).await?;
    
    let proxy = zbus::Proxy::new(
        &connection,
        XIME_DBUS_NAME,
        XIME_DBUS_PATH,
        XIME_DBUS_IFACE,
    ).await?;
    
    eprintln!("DEBUG: Launcher calling OpenWaylandSocket with fd and display={}", display);
    
    let fd_for_dbus = Fd::from(&owned_dup);
    proxy.call_method("OpenWaylandSocket", &(fd_for_dbus, &display)).await?;
    
    eprintln!("DEBUG: OpenWaylandSocket succeeded");
    Ok(())
}

fn main() {
    if env::args().any(|arg| arg == "--reopen") {
        eprintln!("DEBUG: Launcher started with --reopen flag");
    }
    
    if let Ok(socket) = env::var("WAYLAND_SOCKET") {
        eprintln!("DEBUG: WAYLAND_SOCKET={} at startup", socket);
    } else {
        eprintln!("WARNING: No WAYLAND_SOCKET, exiting");
        std::process::exit(0);
    }
    
    let rt = tokio::runtime::Runtime::new()
        .expect("Failed to create tokio runtime");
    
    rt.block_on(async {
        let fd = get_wayland_socket_fd()
            .expect("Failed to get WAYLAND_SOCKET fd");
        
        if let Err(e) = connect_to_daemon(fd).await {
            eprintln!("ERROR: {}", e);
            std::process::exit(1);
        }
        
        eprintln!("DEBUG: Launcher keeping process alive");
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}