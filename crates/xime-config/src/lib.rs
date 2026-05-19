use std::fs;
use std::path::PathBuf;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Default)]
pub struct XimeConfig {
    #[serde(default)]
    pub wubi_radicals: WubiRadicalsConfig,
    #[serde(default)]
    pub style: StyleConfig,
    #[serde(default)]
    pub color_schemes: HashMap<String, ColorScheme>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct StyleConfig {
    #[serde(default)]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_candidate_count")]
    pub candidate_count: i32,
    #[serde(default)]
    pub show_code_hint: bool,
    #[serde(default = "default_horizontal")]
    pub horizontal: bool,
    #[serde(default = "default_corner_radius")]
    pub corner_radius: f32,
    #[serde(default = "default_color_scheme")]
    pub color_scheme: String,
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            font_family: String::new(),
            font_size: default_font_size(),
            candidate_count: default_candidate_count(),
            show_code_hint: false,
            horizontal: default_horizontal(),
            corner_radius: default_corner_radius(),
            color_scheme: default_color_scheme(),
        }
    }
}

fn default_font_size() -> f32 {
    14.0
}

fn default_candidate_count() -> i32 {
    5
}

fn default_horizontal() -> bool {
    true
}

fn default_corner_radius() -> f32 {
    8.0
}

fn default_color_scheme() -> String {
    "lavender_purple".to_string()
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
pub struct ColorScheme {
    #[serde(default)]
    pub name: String,
    #[serde(deserialize_with = "deserialize_hex_color", default = "default_primary_color")]
    pub primary_color: u32,
}

fn default_primary_color() -> u32 {
    0x8F73E2
}

fn deserialize_hex_color<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: serde_yaml::Value = serde::Deserialize::deserialize(deserializer)?;
    match value {
        serde_yaml::Value::Number(n) => {
            if let Some(num) = n.as_u64() {
                Ok(num as u32)
            } else {
                Ok(0x8F73E2)
            }
        }
        serde_yaml::Value::String(s) => {
            let s = s.trim();
            if s.starts_with("0x") || s.starts_with("0X") {
                u32::from_str_radix(&s[2..], 16).map_err(|_| serde::de::Error::custom("Invalid hex color"))
            } else if s.starts_with('#') {
                u32::from_str_radix(&s[1..], 16).map_err(|_| serde::de::Error::custom("Invalid hex color"))
            } else {
                s.parse::<u32>().map_err(|_| serde::de::Error::custom("Invalid color number"))
            }
        }
        _ => Ok(0x8F73E2),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct WubiRadicalsConfig {
    #[serde(default)]
    pub hotkeys: WubiRadicalsHotkeyConfig,
    #[serde(default)]
    pub schema_radicals: HashMap<String, KeyRadicalsConfig>,
}

#[derive(Debug, Deserialize)]
pub struct WubiRadicalsHotkeyConfig {
    #[serde(default = "default_show_key")]
    pub show_key: String,
    #[serde(default)]
    pub show_all_keys: String,
}

impl Default for WubiRadicalsHotkeyConfig {
    fn default() -> Self {
        Self {
            show_key: default_show_key(),
            show_all_keys: String::new(),
        }
    }
}

fn default_show_key() -> String {
    "Ctrl".to_string()
}

#[derive(Debug, Deserialize, Default)]
pub struct KeyRadicalsConfig {
    #[serde(default)]
    pub g: String,
    #[serde(default)]
    pub f: String,
    #[serde(default)]
    pub d: String,
    #[serde(default)]
    pub s: String,
    #[serde(default)]
    pub a: String,
    #[serde(default)]
    pub h: String,
    #[serde(default)]
    pub j: String,
    #[serde(default)]
    pub k: String,
    #[serde(default)]
    pub l: String,
    #[serde(default)]
    pub m: String,
    #[serde(default)]
    pub t: String,
    #[serde(default)]
    pub r: String,
    #[serde(default)]
    pub e: String,
    #[serde(default)]
    pub w: String,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub y: String,
    #[serde(default)]
    pub u: String,
    #[serde(default)]
    pub i: String,
    #[serde(default)]
    pub o: String,
    #[serde(default)]
    pub p: String,
    #[serde(default)]
    pub n: String,
    #[serde(default)]
    pub b: String,
    #[serde(default)]
    pub v: String,
    #[serde(default)]
    pub c: String,
    #[serde(default)]
    pub x: String,
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
            },
            None => system,
        }
    }
    
    fn user_config_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let user_paths = [
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

    pub fn get_last_key_root_binding(&self) -> String {
        self.wubi_radicals.hotkeys.show_key.clone()
    }

    pub fn is_schema_enabled_for_radicals(&self, current_schema: &str) -> bool {
        self.wubi_radicals.schema_radicals.contains_key(current_schema)
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
        let radicals = self.wubi_radicals.schema_radicals.get(schema)?;
        let root = match key.to_lowercase().next()? {
            'g' => &radicals.g,
            'f' => &radicals.f,
            'd' => &radicals.d,
            's' => &radicals.s,
            'a' => &radicals.a,
            'h' => &radicals.h,
            'j' => &radicals.j,
            'k' => &radicals.k,
            'l' => &radicals.l,
            'm' => &radicals.m,
            't' => &radicals.t,
            'r' => &radicals.r,
            'e' => &radicals.e,
            'w' => &radicals.w,
            'q' => &radicals.q,
            'y' => &radicals.y,
            'u' => &radicals.u,
            'i' => &radicals.i,
            'o' => &radicals.o,
            'p' => &radicals.p,
            'n' => &radicals.n,
            'b' => &radicals.b,
            'v' => &radicals.v,
            'c' => &radicals.c,
            'x' => &radicals.x,
            _ => return None,
        };
        if root.is_empty() {
            None
        } else {
            Some(root.clone())
        }
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
    fn test_get_root_for_key() {
        let config = XimeConfig::builtin_default();
        
        assert_eq!(config.get_root_for_key("wubi86", 'g'), Some("王龶五一戋".to_string()));
        assert_eq!(config.get_root_for_key("wubi86", 'G'), Some("王龶五一戋".to_string()));
        assert_eq!(config.get_root_for_key("wubi86", 'a'), Some("工匚戈艹廿龷七弋戈".to_string()));
        assert!(config.get_root_for_key("wubi86", 'h').unwrap().contains("目"));
        assert!(config.get_root_for_key("wubi86", 'h').unwrap().contains("丨"));
        assert_eq!(config.get_root_for_key("wubi86", 'z'), None);
        assert_eq!(config.get_root_for_key("wubi86", '1'), None);
        assert_eq!(config.get_root_for_key("unknown_schema", 'g'), None);
    }

    #[test]
    fn test_deserialize_hex_color() {
        let yaml_with_0x = "
color_schemes:
  test:
    name: Test
    primary_color: 0xFF0000
";
        let config: XimeConfig = serde_yaml::from_str(yaml_with_0x).unwrap();
        assert_eq!(config.color_schemes.get("test").unwrap().primary_color, 0xFF0000);
        
        let yaml_with_hash = "
color_schemes:
  test:
    name: Test
    primary_color: '#FF0000'
";
        let config: XimeConfig = serde_yaml::from_str(yaml_with_hash).unwrap();
        assert_eq!(config.color_schemes.get("test").unwrap().primary_color, 0xFF0000);
        
        let yaml_with_decimal = "
color_schemes:
  test:
    name: Test
    primary_color: 16711680
";
        let config: XimeConfig = serde_yaml::from_str(yaml_with_decimal).unwrap();
        assert_eq!(config.color_schemes.get("test").unwrap().primary_color, 16711680);
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
    fn test_style_config_defaults() {
        let yaml = "
style:
  font_size: 20
";
        let config: XimeConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.style.font_size, 20.0);
        assert_eq!(config.style.candidate_count, 5);
        assert_eq!(config.style.color_scheme, "lavender_purple");
    }

    #[test]
    fn test_hotkey_config_default() {
        let yaml = "{}";
        let config: XimeConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.wubi_radicals.hotkeys.show_key, "Ctrl");
        
        let yaml_with_hotkey = "
wubi_radicals:
  hotkeys:
    show_key: Alt
";
        let config: XimeConfig = serde_yaml::from_str(yaml_with_hotkey).unwrap();
        assert_eq!(config.wubi_radicals.hotkeys.show_key, "Alt");
    }

    #[test]
    fn test_wubi_root_config() {
        let config = XimeConfig::builtin_default();
        
        let radicals = config.wubi_radicals.schema_radicals.get("wubi86").unwrap();
        assert!(!radicals.g.is_empty());
        assert!(!radicals.f.is_empty());
        assert!(!radicals.d.is_empty());
        
        let yaml_partial = "
wubi_radicals:
  schema_radicals:
    wubi86:
      g: \"测试字根\"
";
        let config: XimeConfig = serde_yaml::from_str(yaml_partial).unwrap();
        let radicals = config.wubi_radicals.schema_radicals.get("wubi86").unwrap();
        assert_eq!(radicals.g, "测试字根");
        assert!(radicals.f.is_empty());
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
    }

    #[test]
    fn test_merge_configs_empty_wubi_root() {
        let system = XimeConfig::builtin_default();
        let user_yaml = "
style:
  color_scheme: coral_red
";
        let user: XimeConfig = serde_yaml::from_str(user_yaml).unwrap();
        let merged = XimeConfig::merge_configs(system, Some(user));
        
        let radicals = merged.wubi_radicals.schema_radicals.get("wubi86").unwrap();
        assert_eq!(radicals.g, "王龶五一戋");
        assert_eq!(merged.style.color_scheme, "coral_red");
    }

    #[test]
    fn test_merge_configs_empty_color_schemes() {
        let system = XimeConfig::builtin_default();
        let user_yaml = "
style:
  font_size: 16
";
        let user: XimeConfig = serde_yaml::from_str(user_yaml).unwrap();
        let merged = XimeConfig::merge_configs(system, Some(user));
        
        assert!(!merged.color_schemes.is_empty());
        assert_eq!(merged.color_schemes.get("lavender_purple").unwrap().primary_color, 0x8F73E2);
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
            assert_eq!(config.color_schemes.get(*name).unwrap().primary_color, *color);
        }
    }

    #[test]
    fn test_get_last_key_root_binding() {
        let config = XimeConfig::builtin_default();
        assert_eq!(config.get_last_key_root_binding(), "Ctrl");
        
        let yaml = "
wubi_radicals:
  hotkeys:
    show_key: Super
";
        let config: XimeConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.get_last_key_root_binding(), "Super");
    }

    #[test]
    fn test_is_schema_enabled_for_radicals() {
        let config = XimeConfig::builtin_default();
        assert!(config.is_schema_enabled_for_radicals("wubi86_pinyin"));
        assert!(config.is_schema_enabled_for_radicals("wubi86"));
        assert!(!config.is_schema_enabled_for_radicals("luna_pinyin"));
        
        let yaml = "
wubi_radicals:
  schema_radicals:
    wubi86:
      g: \"王龶五一戋\"
    wubi86_pinyin:
      g: \"王龶五一戋\"
";
        let config: XimeConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.is_schema_enabled_for_radicals("wubi86"));
        assert!(config.is_schema_enabled_for_radicals("wubi86_pinyin"));
        assert!(!config.is_schema_enabled_for_radicals("double_pinyin"));
    }

    #[test]
    fn test_invalid_yaml() {
        let invalid_yaml = "invalid: ::: yaml";
        let result: Result<XimeConfig, _> = serde_yaml::from_str(invalid_yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_wubi_root_returns_none() {
        let config = XimeConfig::default();
        assert_eq!(config.get_root_for_key("wubi86", 'g'), None);
        assert_eq!(config.get_root_for_key("wubi86", 'a'), None);
    }

    #[test]
    fn test_load_returns_valid_config() {
        let config = XimeConfig::load();
        assert_eq!(config.wubi_radicals.hotkeys.show_key, "Ctrl");
        assert!(!config.color_schemes.is_empty());
    }
}