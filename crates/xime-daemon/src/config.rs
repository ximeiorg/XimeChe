use std::fs;
use std::path::PathBuf;
use serde::Deserialize;
use std::collections::HashMap;
use xime_xkb::KeyBinding;

#[derive(Debug, Deserialize, Default)]
pub struct XimeConfig {
    #[serde(default)]
    pub hotkeys: HotkeyConfig,
    #[serde(default)]
    pub wubi_root: WubiRootConfig,
    #[serde(default)]
    pub style: StyleConfig,
    #[serde(default)]
    pub color_schemes: HashMap<String, ColorScheme>,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
pub struct StyleConfig {
    #[serde(default)]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: i32,
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

fn default_font_size() -> i32 {
    14
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
pub struct HotkeyConfig {
    #[serde(default = "default_show_last_key_root")]
    pub show_last_key_root: String,
}

fn default_show_last_key_root() -> String {
    "Ctrl".to_string()
}

#[derive(Debug, Deserialize, Default)]
pub struct WubiRootConfig {
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
        let config_path = Self::config_path();
        if config_path.exists() {
            let content = fs::read_to_string(&config_path).ok();
            if let Some(content) = content {
                let config: XimeConfig = serde_yaml::from_str(&content).ok().unwrap_or_default();
                eprintln!("DEBUG: Loaded xime config from {:?}", config_path);
                return config;
            }
        }
        Self::default()
    }

    pub fn config_path() -> PathBuf {
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
        
        let system_path = PathBuf::from("/usr/share/xime/xime.yaml");
        if system_path.exists() {
            return system_path;
        }
        
        user_paths[0].clone()
    }

    /// Parse show_last_key_root hotkey binding
    pub fn get_last_key_root_binding(&self) -> KeyBinding {
        KeyBinding::parse(&self.hotkeys.show_last_key_root)
    }

    /// Get primary color from current color scheme
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

    /// Get wubi root for a key (letter)
    pub fn get_root_for_key(&self, key: char) -> Option<String> {
        let root = match key.to_lowercase().next()? {
            'g' => &self.wubi_root.g,
            'f' => &self.wubi_root.f,
            'd' => &self.wubi_root.d,
            's' => &self.wubi_root.s,
            'a' => &self.wubi_root.a,
            'h' => &self.wubi_root.h,
            'j' => &self.wubi_root.j,
            'k' => &self.wubi_root.k,
            'l' => &self.wubi_root.l,
            'm' => &self.wubi_root.m,
            't' => &self.wubi_root.t,
            'r' => &self.wubi_root.r,
            'e' => &self.wubi_root.e,
            'w' => &self.wubi_root.w,
            'q' => &self.wubi_root.q,
            'y' => &self.wubi_root.y,
            'u' => &self.wubi_root.u,
            'i' => &self.wubi_root.i,
            'o' => &self.wubi_root.o,
            'p' => &self.wubi_root.p,
            'n' => &self.wubi_root.n,
            'b' => &self.wubi_root.b,
            'v' => &self.wubi_root.v,
            'c' => &self.wubi_root.c,
            'x' => &self.wubi_root.x,
            _ => return None,
        };
        if root.is_empty() {
            None
        } else {
            Some(root.clone())
        }
    }
}