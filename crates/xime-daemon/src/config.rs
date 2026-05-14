use std::fs;
use std::path::PathBuf;
use serde::Deserialize;
use xime_xkb::KeyBinding;

#[derive(Debug, Deserialize, Default)]
pub struct XimeConfig {
    #[serde(default)]
    pub hotkeys: HotkeyConfig,
    #[serde(default)]
    pub wubi_root: WubiRootConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct HotkeyConfig {
    #[serde(default = "default_show_root_table")]
    pub show_root_table: String,
    #[serde(default = "default_show_single_root")]
    pub show_single_root: String,
}

fn default_show_root_table() -> String {
    "Win+Alt+F1".to_string()
}

fn default_show_single_root() -> String {
    "Win+Alt".to_string()
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
        let path1 = PathBuf::from(&home).join(".config/xime/xime.yaml");
        let path2 = PathBuf::from(&home).join(".config/xime/rime/xime.yaml");
        if path1.exists() {
            path1
        } else {
            path2
        }
    }

    /// Parse show_root_table hotkey binding
    pub fn get_root_table_binding(&self) -> KeyBinding {
        KeyBinding::parse(&self.hotkeys.show_root_table)
    }

    /// Parse show_single_root modifier binding (no key, just modifiers)
    pub fn get_single_root_binding(&self) -> KeyBinding {
        KeyBinding::parse(&self.hotkeys.show_single_root)
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