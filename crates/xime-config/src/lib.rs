mod config;
mod smart_suggestion;
mod style;
mod wubi_radicals;

pub use config::XimeConfig;
pub use smart_suggestion::{
    SmartSuggestionConfig, SmartSuggestionModelConfig, SmartSuggestionModelFile,
};
pub use style::{deserialize_hex_color, serialize_hex_color, ColorScheme, StyleConfig};
pub use wubi_radicals::{KeyRadicalsConfig, WubiRadicalsConfig, WubiRadicalsHotkeyConfig};
