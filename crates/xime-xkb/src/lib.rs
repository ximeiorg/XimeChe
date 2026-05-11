pub mod context;

pub use context::XkbContext;
pub use context::keysym_to_rime_keycode;
pub use context::keysym_to_char;
pub use context::ModifierState;

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