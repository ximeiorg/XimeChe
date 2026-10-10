use std::os::unix::io::OwnedFd;
use tokio::sync::oneshot;

pub enum DaemonCommand {
    OpenWaylandSocket(OwnedFd, String),
    ToggleMode,
    Deploy,
    ReloadStyle,
    ReloadPlugins,
    SelectSchema(String, oneshot::Sender<bool>),
    /// 读取用户词典词条（设置程序词典页）。在 wayland 线程执行：levers
    /// 导出要求 userdb 独占（关会话→导出→重建，见 RimeEngine::
    /// with_user_dict_closed），不能与按键路径并发。
    ListDictEntries(
        String,
        String,
        oneshot::Sender<Result<crate::user_dict::DictEntriesResult, String>>,
    ),
    /// 用户词典 levers 写操作（造词/删除/备份/恢复/导出/导入）。同样在
    /// wayland 线程 `with_user_dict_closed` 内执行。
    UserDictOp(
        crate::user_dict::UserDictOp,
        oneshot::Sender<Result<i64, String>>,
    ),
    /// 系统亮/暗色模式变化（portal color-scheme，true = 暗色）。
    DarkMode(bool),
    /// 托盘「语音输入」入口：开始/停止听写会话（键盘快捷键 Ctrl+Alt+V
    /// 在 wayland 线程直接处理，不经此命令）。
    ToggleSpeech,
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
    fn test_daemon_command_toggle_speech() {
        let cmd = DaemonCommand::ToggleSpeech;
        match cmd {
            DaemonCommand::ToggleSpeech => {} // expected
            _ => panic!("Expected ToggleSpeech"),
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
