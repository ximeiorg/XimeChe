use nix::unistd::dup;
use std::env;
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd};
use tracing::{debug, error, warn};
use tracing_subscriber::EnvFilter;
use zbus::zvariant::Fd;
use zbus::Connection;

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
    let socket_env = env::var("WAYLAND_SOCKET").map_err(|_| Error::NoWaylandSocket)?;

    let fd: i32 = socket_env.parse()?;

    let owned_fd = unsafe { OwnedFd::from_raw_fd(fd) };
    Ok(owned_fd)
}

async fn connect_to_daemon(fd: OwnedFd) -> Result<(), Error> {
    let display_name = env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_string());

    debug!("Duplicating fd {} for DBus transfer", fd.as_raw_fd());

    let dup_fd =
        dup(fd.as_raw_fd()).map_err(|e| Error::Io(std::io::Error::from_raw_os_error(e as i32)))?;
    let owned_dup = unsafe { OwnedFd::from_raw_fd(dup_fd) };

    let connection = Connection::session().await?;

    debug!("Activating daemon via DBus");
    let _ = connection
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "StartServiceByName",
            &(XIME_DBUS_NAME, 0u32),
        )
        .await?;

    let proxy =
        zbus::Proxy::new(&connection, XIME_DBUS_NAME, XIME_DBUS_PATH, XIME_DBUS_IFACE).await?;

    debug!(
        "Launcher calling OpenWaylandSocket with fd and display={:?}",
        &display_name
    );

    let fd_for_dbus = Fd::from(&owned_dup);
    proxy
        .call_method("OpenWaylandSocket", &(fd_for_dbus, &display_name))
        .await?;

    debug!("OpenWaylandSocket succeeded");
    Ok(())
}

fn main() {
    #[cfg(debug_assertions)]
    let default_level = tracing::Level::DEBUG;
    #[cfg(not(debug_assertions))]
    let default_level = tracing::Level::INFO;

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(default_level.into()))
        .with_writer(std::io::stderr)
        .init();

    if env::args().any(|arg| arg == "--reopen") {
        debug!("Launcher started with --reopen flag");
    }

    if let Ok(socket) = env::var("WAYLAND_SOCKET") {
        debug!("WAYLAND_SOCKET={} at startup", socket);
    } else {
        warn!("No WAYLAND_SOCKET, exiting");
        std::process::exit(0);
    }

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    rt.block_on(async {
        let fd = get_wayland_socket_fd().expect("Failed to get WAYLAND_SOCKET fd");

        if let Err(e) = connect_to_daemon(fd).await {
            error!("{}", e);
            std::process::exit(1);
        }

        debug!("Launcher keeping process alive");
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}
