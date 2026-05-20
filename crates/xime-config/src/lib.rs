mod config;
mod style;
mod wubi_radicals;
mod smart_suggestion;

pub use config::XimeConfig;
pub use style::{StyleConfig, ColorScheme, deserialize_hex_color, serialize_hex_color};
pub use wubi_radicals::{WubiRadicalsConfig, WubiRadicalsHotkeyConfig, KeyRadicalsConfig};
pub use smart_suggestion::{
    SmartSuggestionConfig,
    SmartSuggestionModelConfig,
    SmartSuggestionModelFile,
};