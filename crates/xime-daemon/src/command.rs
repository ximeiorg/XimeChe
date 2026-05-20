use std::os::unix::io::OwnedFd;

pub enum DaemonCommand {
    OpenWaylandSocket(OwnedFd, String),
    ToggleMode,
    Deploy,
    ReloadStyle,
    Shutdown,
}
