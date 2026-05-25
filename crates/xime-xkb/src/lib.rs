pub mod context;

pub use context::keysym_to_char;
pub use context::keysym_to_letter;
pub use context::keysym_to_rime_keycode;
pub use context::KeyBinding;
pub use context::ModifierState;
pub use context::XkbContext;
pub use xkbcommon::xkb::Keysym;

pub type Result<T> = std::result::Result<T, Error>;

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
    fn test_re_exports_exist() {
        // Verify that the public types can be constructed/accessed
        let binding = KeyBinding::default();
        assert!(!binding.ctrl);

        let state = ModifierState::default();
        assert_eq!(state.effective, 0);
    }
}
