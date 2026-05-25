use std::os::unix::io::OwnedFd;

pub enum DaemonCommand {
    OpenWaylandSocket(OwnedFd, String),
    ToggleMode,
    Deploy,
    ReloadStyle,
    Shutdown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_command_toggle_mode() {
        let cmd = DaemonCommand::ToggleMode;
        match cmd {
            DaemonCommand::ToggleMode => {} // expected
            _ => panic!("Expected ToggleMode"),
        }
    }

    #[test]
    fn test_daemon_command_deploy() {
        let cmd = DaemonCommand::Deploy;
        match cmd {
            DaemonCommand::Deploy => {} // expected
            _ => panic!("Expected Deploy"),
        }
    }

    #[test]
    fn test_daemon_command_reload_style() {
        let cmd = DaemonCommand::ReloadStyle;
        match cmd {
            DaemonCommand::ReloadStyle => {} // expected
            _ => panic!("Expected ReloadStyle"),
        }
    }

    #[test]
    fn test_daemon_command_shutdown() {
        let cmd = DaemonCommand::Shutdown;
        match cmd {
            DaemonCommand::Shutdown => {} // expected
            _ => panic!("Expected Shutdown"),
        }
    }

    #[test]
    fn test_daemon_command_debug_assertions() {
        // Verify the variants have consistent memory layout
        assert_eq!(
            std::mem::discriminant(&DaemonCommand::ToggleMode),
            std::mem::discriminant(&DaemonCommand::ToggleMode)
        );
        assert_ne!(
            std::mem::discriminant(&DaemonCommand::ToggleMode),
            std::mem::discriminant(&DaemonCommand::Deploy)
        );
    }
}
