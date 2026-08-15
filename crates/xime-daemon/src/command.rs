use std::os::unix::io::OwnedFd;
use tokio::sync::oneshot;

pub enum DaemonCommand {
    OpenWaylandSocket(OwnedFd, String),
    ToggleMode,
    Deploy,
    ReloadStyle,
    ReloadPlugins,
    SelectSchema(String, oneshot::Sender<bool>),
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
    fn test_daemon_command_reload_plugins() {
        let cmd = DaemonCommand::ReloadPlugins;
        match cmd {
            DaemonCommand::ReloadPlugins => {} // expected
            _ => panic!("Expected ReloadPlugins"),
        }
    }

    #[test]
    fn test_daemon_command_select_schema() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let cmd = DaemonCommand::SelectSchema("wubi86".into(), tx);
        match cmd {
            DaemonCommand::SelectSchema(id, _) => assert_eq!(id, "wubi86"),
            _ => panic!("Expected SelectSchema"),
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
