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

async fn connect_to_daemon(connection: &Connection, fd: OwnedFd) -> Result<(), Error> {
    let display_name = env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_string());

    debug!("Duplicating fd {} for DBus transfer", fd.as_raw_fd());

    let dup_fd =
        dup(fd.as_raw_fd()).map_err(|e| Error::Io(std::io::Error::from_raw_os_error(e as i32)))?;
    let owned_dup = unsafe { OwnedFd::from_raw_fd(dup_fd) };

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
        zbus::Proxy::new(connection, XIME_DBUS_NAME, XIME_DBUS_PATH, XIME_DBUS_IFACE).await?;

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

/// daemon 失联看门狗：org.xime.Xime 从会话总线消失（daemon 崩溃/被杀）
/// 连续 3 个周期（约 9 秒）后，用 SIGKILL 结束 launcher 自身。必须被
/// **信号**杀死而不能干净退出——KWin 只在 IM 客户端 QProcess::CrashExit
/// 时重拉 launcher（src/inputmethod.cpp 的 finished 处理器），干净退出的
/// 滞留 launcher 会让 KWin 永远不重拉，输入法死到用户手动去设置里切换。
/// 新 launcher 被拉起后经 DBus 激活重建 daemon，整链自动恢复。
async fn watch_daemon(connection: Connection) {
    use nix::sys::signal::{self, Signal};
    use nix::unistd::getpid;
    use zbus::fdo::DBusProxy;

    let Ok(proxy) = DBusProxy::new(&connection).await else {
        warn!("Watchdog: DBus proxy unavailable, daemon watchdog disabled");
        return;
    };

    let mut misses: u32 = 0;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let name = zbus::names::BusName::from_static_str(XIME_DBUS_NAME)
            .expect("valid well-known bus name");
        match proxy.name_has_owner(name).await {
            Ok(true) => misses = 0,
            Ok(false) => {
                misses += 1;
                warn!(
                    "Watchdog: {} not on bus (miss {}/3)",
                    XIME_DBUS_NAME, misses
                );
                if misses >= 3 {
                    error!("Watchdog: daemon gone, killing launcher so KWin respawns the chain");
                    let _ = signal::kill(getpid(), Signal::SIGKILL);
                    // SIGKILL 不可捕获，正常到不了这里
                    std::process::exit(1);
                }
            }
            Err(e) => {
                // 总线抖动不计数，避免误杀
                debug!("Watchdog: name_has_owner failed: {}", e);
            }
        }
    }
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
        let connection = match Connection::session().await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to connect to session bus: {}", e);
                std::process::exit(1);
            }
        };

        let fd = get_wayland_socket_fd().expect("Failed to get WAYLAND_SOCKET fd");

        if let Err(e) = connect_to_daemon(&connection, fd).await {
            error!("{}", e);
            std::process::exit(1);
        }

        // daemon 失联看门狗（见 watch_daemon 注释）
        tokio::spawn(watch_daemon(connection));

        debug!("Launcher keeping process alive");
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}
