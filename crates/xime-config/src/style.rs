use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct StyleConfig {
    #[serde(default)]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_candidate_count")]
    pub candidate_count: i32,
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

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct ColorScheme {
    #[serde(default)]
    pub name: String,
    #[serde(
        deserialize_with = "deserialize_hex_color",
        serialize_with = "serialize_hex_color",
        default = "default_primary_color"
    )]
    pub primary_color: u32,
}

fn default_primary_color() -> u32 {
    0x8F73E2
}

pub fn deserialize_hex_color<'de, D>(deserializer: D) -> Result<u32, D::Error>
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
                u32::from_str_radix(&s[2..], 16)
                    .map_err(|_| serde::de::Error::custom("Invalid hex color"))
            } else if let Some(stripped) = s.strip_prefix('#') {
                u32::from_str_radix(stripped, 16)
                    .map_err(|_| serde::de::Error::custom("Invalid hex color"))
            } else {
                s.parse::<u32>()
                    .map_err(|_| serde::de::Error::custom("Invalid color number"))
            }
        }
        _ => Ok(0x8F73E2),
    }
}

pub fn serialize_hex_color<S>(value: &u32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&format!("0x{:06X}", value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_config_defaults() {
        let yaml = "font_size: 20";
        let config: StyleConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.font_size, 20.0);
        assert_eq!(config.candidate_count, 5);
        assert_eq!(config.color_scheme, "lavender_purple");
    }

    #[test]
    fn test_deserialize_hex_color_0x() {
        let yaml = "name: Test\nprimary_color: 0xFF0000";
        let scheme: ColorScheme = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(scheme.primary_color, 0xFF0000);
    }

    #[test]
    fn test_deserialize_hex_color_hash() {
        let yaml = "name: Test\nprimary_color: '#FF0000'";
        let scheme: ColorScheme = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(scheme.primary_color, 0xFF0000);
    }

    #[test]
    fn test_deserialize_hex_color_decimal() {
        let yaml = "name: Test\nprimary_color: 16711680";
        let scheme: ColorScheme = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(scheme.primary_color, 16711680);
    }
}
