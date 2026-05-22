use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::smart_suggestion::SmartSuggestionConfig;
use crate::style::{ColorScheme, StyleConfig};
use crate::wubi_radicals::WubiRadicalsConfig;

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct XimeConfig {
    #[serde(default)]
    pub wubi_radicals: WubiRadicalsConfig,
    #[serde(default)]
    pub style: StyleConfig,
    #[serde(default)]
    pub color_schemes: HashMap<String, ColorScheme>,
    #[serde(default)]
    pub smart_suggestion: SmartSuggestionConfig,
}

impl XimeConfig {
    pub fn load() -> Self {
        let system_config = Self::load_system_config();
        let user_config = Self::load_user_config();
        Self::merge_configs(system_config, user_config)
    }

    fn load_system_config() -> Self {
        let system_path = PathBuf::from("/usr/share/xime/xime.yaml");
        if system_path.exists() {
            if let Ok(content) = fs::read_to_string(&system_path) {
                if let Ok(config) = serde_yaml::from_str::<XimeConfig>(&content) {
                    return config;
                }
            }
        }
        Self::builtin_default()
    }

    fn builtin_default() -> Self {
        const DEFAULT_CONFIG: &[u8] = include_bytes!("../../../resources/xime.yaml");
        serde_yaml::from_slice(DEFAULT_CONFIG).unwrap_or_default()
    }

    fn load_user_config() -> Option<Self> {
        let config_path = Self::user_config_path();
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                if let Ok(config) = serde_yaml::from_str::<XimeConfig>(&content) {
                    return Some(config);
                }
            }
        }
        None
    }

    fn merge_configs(system: Self, user: Option<Self>) -> Self {
        match user {
            Some(user) => Self {
                wubi_radicals: if user.wubi_radicals.schema_radicals.is_empty() {
                    system.wubi_radicals
                } else {
                    user.wubi_radicals
                },
                style: user.style,
                color_schemes: if user.color_schemes.is_empty() {
                    system.color_schemes
                } else {
                    user.color_schemes
                },
                smart_suggestion: user.smart_suggestion,
            },
            None => system,
        }
    }

    fn user_config_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let user_paths = [
            PathBuf::from(&home).join(".config/xime/xime.custom.yaml"),
            PathBuf::from(&home).join(".config/xime/rime/xime.custom.yaml"),
            PathBuf::from(&home).join(".config/xime/xime.yaml"),
            PathBuf::from(&home).join(".config/xime/rime/xime.yaml"),
        ];

        for path in &user_paths {
            if path.exists() {
                return path.clone();
            }
        }
        user_paths[0].clone()
    }

    pub fn config_path() -> PathBuf {
        Self::user_config_path()
    }

    pub fn get_primary_color(&self) -> (u8, u8, u8) {
        let scheme_name = &self.style.color_scheme;
        if let Some(scheme) = self.color_schemes.get(scheme_name) {
            let r = (scheme.primary_color >> 16) as u8;
            let g = (scheme.primary_color >> 8) as u8;
            let b = scheme.primary_color as u8;
            (r, g, b)
        } else {
            (0x8F, 0x73, 0xE2)
        }
    }

    pub fn get_root_for_key(&self, schema: &str, key: char) -> Option<String> {
        self.wubi_radicals.get_root_for_key(schema, key)
    }

    pub fn get_last_key_root_binding(&self) -> String {
        self.wubi_radicals.get_last_key_root_binding()
    }

    pub fn is_schema_enabled_for_radicals(&self, current_schema: &str) -> bool {
        self.wubi_radicals
            .is_schema_enabled_for_radicals(current_schema)
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        let content = serde_yaml::to_string(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config dir: {}", e))?;
        }

        fs::write(&path, content).map_err(|e| format!("Failed to write config: {}", e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_default() {
        let config = XimeConfig::builtin_default();

        assert_eq!(config.wubi_radicals.hotkeys.show_key, "Ctrl");
        assert!(config.wubi_radicals.schema_radicals.contains_key("wubi86"));
        let radicals = config.wubi_radicals.schema_radicals.get("wubi86").unwrap();
        assert!(!radicals.g.is_empty());
        assert_eq!(radicals.g, "王龶五一戋");
        assert_eq!(config.style.font_size, 14.0);
        assert_eq!(config.style.candidate_count, 5);
        assert_eq!(config.style.color_scheme, "lavender_purple");
        assert!(!config.color_schemes.is_empty());
    }

    #[test]
    fn test_get_primary_color() {
        let config = XimeConfig::builtin_default();

        let lavender_color = config.get_primary_color();
        assert_eq!(lavender_color, (0x8F, 0x73, 0xE2));

        let mut config = XimeConfig::builtin_default();
        config.style.color_scheme = "slate_gray".to_string();
        assert_eq!(config.get_primary_color(), (0x42, 0x42, 0x42));

        config.style.color_scheme = "ocean_blue".to_string();
        assert_eq!(config.get_primary_color(), (0x1A, 0x73, 0xE8));

        config.style.color_scheme = "unknown_scheme".to_string();
        assert_eq!(config.get_primary_color(), (0x8F, 0x73, 0xE2));
    }

    #[test]
    fn test_default_values() {
        let empty_yaml = "{}";
        let config: XimeConfig = serde_yaml::from_str(empty_yaml).unwrap();

        assert_eq!(config.wubi_radicals.hotkeys.show_key, "Ctrl");
        assert_eq!(config.style.font_size, 14.0);
        assert_eq!(config.style.candidate_count, 5);
        assert!(config.style.horizontal);
        assert_eq!(config.style.corner_radius, 8.0);
        assert_eq!(config.style.color_scheme, "lavender_purple");
    }

    #[test]
    fn test_all_color_schemes() {
        let config = XimeConfig::builtin_default();

        let expected_schemes = [
            ("lavender_purple", 0x8F73E2),
            ("ocean_blue", 0x1A73E8),
            ("forest_green", 0x2E7D32),
            ("sunset_orange", 0xE65100),
            ("coral_red", 0xC62828),
            ("slate_gray", 0x424242),
            ("rose_pink", 0xAD1457),
            ("teal_cyan", 0x00796B),
        ];

        for (name, color) in &expected_schemes {
            assert!(config.color_schemes.contains_key(*name));
            assert_eq!(
                config.color_schemes.get(*name).unwrap().primary_color,
                *color
            );
        }
    }

    #[test]
    fn test_merge_configs_full_user() {
        let system = XimeConfig::builtin_default();
        let user_yaml = "
wubi_radicals:
  hotkeys:
    show_key: Shift
  schema_radicals:
    wubi86:
      g: \"用户字根\"

style:
  color_scheme: ocean_blue
  font_size: 18

smart_suggestion:
  enabled: true

color_schemes:
  ocean_blue:
    name: \"海洋蔚蓝\"
    primary_color: 0x1A73E8
";
        let user: XimeConfig = serde_yaml::from_str(user_yaml).unwrap();
        let merged = XimeConfig::merge_configs(system, Some(user));

        assert_eq!(merged.wubi_radicals.hotkeys.show_key, "Shift");
        let radicals = merged.wubi_radicals.schema_radicals.get("wubi86").unwrap();
        assert_eq!(radicals.g, "用户字根");
        assert_eq!(merged.style.color_scheme, "ocean_blue");
        assert_eq!(merged.style.font_size, 18.0);
        assert!(!merged.color_schemes.is_empty());
        assert!(merged.smart_suggestion.enabled);
    }

    #[test]
    fn test_merge_configs_empty_wubi_root() {
        let system = XimeConfig::builtin_default();
        let user_yaml = "style:\n  color_scheme: coral_red";
        let user: XimeConfig = serde_yaml::from_str(user_yaml).unwrap();
        let merged = XimeConfig::merge_configs(system, Some(user));

        let radicals = merged.wubi_radicals.schema_radicals.get("wubi86").unwrap();
        assert_eq!(radicals.g, "王龶五一戋");
        assert_eq!(merged.style.color_scheme, "coral_red");
    }

    #[test]
    fn test_merge_configs_empty_color_schemes() {
        let system = XimeConfig::builtin_default();
        let user_yaml = "style:\n  font_size: 16";
        let user: XimeConfig = serde_yaml::from_str(user_yaml).unwrap();
        let merged = XimeConfig::merge_configs(system, Some(user));

        assert!(!merged.color_schemes.is_empty());
        assert_eq!(
            merged
                .color_schemes
                .get("lavender_purple")
                .unwrap()
                .primary_color,
            0x8F73E2
        );
    }

    #[test]
    fn test_merge_configs_no_user() {
        let system = XimeConfig::builtin_default();
        let merged = XimeConfig::merge_configs(system, None);

        assert_eq!(merged.wubi_radicals.hotkeys.show_key, "Ctrl");
        let radicals = merged.wubi_radicals.schema_radicals.get("wubi86").unwrap();
        assert_eq!(radicals.g, "王龶五一戋");
        assert_eq!(merged.style.color_scheme, "lavender_purple");
    }

    #[test]
    fn test_load_returns_valid_config() {
        let config = XimeConfig::load();
        assert_eq!(config.wubi_radicals.hotkeys.show_key, "Ctrl");
        assert!(!config.color_schemes.is_empty());
    }
}
