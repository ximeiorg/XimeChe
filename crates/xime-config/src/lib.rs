mod config;
mod rime;
mod style;
mod wubi_radicals;

pub use config::XimeConfig;
pub use rime::{
    deploy_all, ReverseLookupConfig, RimeConfigManager, SchemaConfig, SchemaConfigManager,
    SchemaInfo, SchemaManager, SpellerConfig, TraditionConfig, TranslatorConfig,
};
pub use style::{deserialize_hex_color, serialize_hex_color, ColorScheme, StyleConfig};
pub use wubi_radicals::{KeyRadicalsConfig, WubiRadicalsConfig, WubiRadicalsHotkeyConfig};
