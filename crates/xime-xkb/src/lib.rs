pub mod context;

pub use context::keysym_to_char;
pub use context::keysym_to_letter;
pub use context::keysym_to_rime_keycode;
pub use context::KeyBinding;
pub use context::ModifierState;
pub use context::XkbContext;
pub use xkbcommon::xkb::Keysym;

pub type Result<T> = std::result::Result<T, Error>;

/// 该 keysym 是否是修饰键本身（Shift / Ctrl / Alt / Super / CapsLock / ISO_Level*）。
///
/// 只用于「按住重复」的过滤：修饰键的按下状态由合成器的 `Modifiers` 事件
/// 独立维护，把修饰键当普通键重复投递只会打扰应用；而按住方向键时再按
/// Shift（Shift+方向键选择文本），方向键的重复**不应**被打断。
pub fn is_modifier_keysym(sym: u32) -> bool {
    matches!(
        sym,
        // Shift_L .. Hyper_R（0xffe1..=0xffee）
        0xffe1..=0xffee
            // ISO_Lock .. ISO_Level5_Lock（含 group switch 键）
            | 0xfe01..=0xfe13
            // Mode_switch / Num_Lock
            | 0xff7e
            | 0xff7f
    )
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to create XKB context")]
    ContextCreationFailed,

    #[error("Failed to create keymap")]
    KeymapCreationFailed,

    #[error("Failed to create state")]
    StateCreationFailed,

    #[error("Invalid keymap format")]
    InvalidKeymapFormat,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_context_creation() {
        let err = Error::ContextCreationFailed;
        assert_eq!(format!("{}", err), "Failed to create XKB context");
    }

    #[test]
    fn test_error_display_keymap_creation() {
        let err = Error::KeymapCreationFailed;
        assert_eq!(format!("{}", err), "Failed to create keymap");
    }

    #[test]
    fn test_error_display_state_creation() {
        let err = Error::StateCreationFailed;
        assert_eq!(format!("{}", err), "Failed to create state");
    }

    #[test]
    fn test_error_display_invalid_keymap() {
        let err = Error::InvalidKeymapFormat;
        assert_eq!(format!("{}", err), "Invalid keymap format");
    }

    #[test]
    fn test_error_debug() {
        let err = Error::ContextCreationFailed;
        let debug = format!("{:?}", err);
        assert!(debug.contains("ContextCreationFailed"));
    }

    #[test]
    fn test_modifier_keysyms() {
        assert!(is_modifier_keysym(0xffe1), "Shift_L");
        assert!(is_modifier_keysym(0xffe3), "Control_L");
        assert!(is_modifier_keysym(0xffe9), "Alt_L");
        assert!(is_modifier_keysym(0xffeb), "Super_L");
        assert!(is_modifier_keysym(0xffe5), "Caps_Lock");
        assert!(is_modifier_keysym(0xfe03), "ISO_Level3_Shift");
        assert!(is_modifier_keysym(0xff7f), "Num_Lock");
        // 方向键、字母、空格、Tab 都不是修饰键
        assert!(!is_modifier_keysym(0xff51), "Left");
        assert!(!is_modifier_keysym(0xff53), "Right");
        assert!(!is_modifier_keysym(0x0061), "a");
        assert!(!is_modifier_keysym(0x0020), "space");
        assert!(!is_modifier_keysym(0xff09), "Tab");
        assert!(!is_modifier_keysym(0xfe20), "ISO_Left_Tab");
    }

    #[test]
    fn test_re_exports_exist() {
        // Verify that the public types can be constructed/accessed
        let binding = KeyBinding::default();
        assert!(!binding.ctrl);

        let state = ModifierState::default();
        assert_eq!(state.effective, 0);
    }
}
