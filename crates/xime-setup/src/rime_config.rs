use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use std::sync::Once;

static RIME_DEPLOYED: Once = Once::new();

fn get_user_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    PathBuf::from(home).join(".config/xime/rime")
}

fn get_shared_data_dir() -> PathBuf {
    PathBuf::from("/usr/share/rime-data")
}

fn get_xime_data_dir() -> PathBuf {
    PathBuf::from("/usr/share/xime/rime-data")
}

fn ensure_config_files() {
    let user_dir = get_user_data_dir();
    if !user_dir.exists() {
        fs::create_dir_all(&user_dir).ok();
    }
    
    let default_custom = user_dir.join("default.custom.yaml");
    if !default_custom.exists() {
        fs::write(&default_custom, 
r#"customization:
  distribution_code_name: Xime
  distribution_version: 1.0

patch:
  schema_list:
    - schema: wubi86_jidian
"#).ok();
    }
    
    let xime_yaml = user_dir.join("xime.yaml");
    if !xime_yaml.exists() {
        fs::write(&xime_yaml, 
r#"config_version: "1.0"
style:
  font_size: 14.0
  candidate_count: 5
  show_code_hint: false
  corner_radius: 8.0
"#).ok();
    }
}

fn init_rime_for_config() {
    RIME_DEPLOYED.call_once(|| {
        ensure_config_files();
        
        let user_dir = get_user_data_dir();
        let shared_dir = get_shared_data_dir();
        
        let mut traits = librime::traits::Traits::new();
        traits.set_shared_data_dir(shared_dir.to_str().unwrap_or(""));
        traits.set_user_data_dir(user_dir.to_str().unwrap_or(""));
        traits.set_log_dir(user_dir.to_str().unwrap_or(""));
        
        librime::setup(&mut traits);
        if let Err(e) = librime::initialize(&mut traits) {
            eprintln!("Failed to initialize Rime: {}", e);
        }
        
        librime::full_deploy_and_wait();
        
        if librime::is_maintenance_mode() {
            librime::join_maintenance_thread();
        }
    });
}

pub fn deploy_all() -> Result<(), String> {
    init_rime_for_config();
    match librime::full_deploy_and_wait() {
        librime::DeployResult::Success => Ok(()),
        librime::DeployResult::Failure => Err("Deploy failed".to_string()),
    }
}

#[derive(Debug, Clone)]
pub struct SchemaInfo {
    pub schema_id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
}

pub struct SchemaManager {
    user_dir: PathBuf,
}

impl SchemaManager {
    pub fn new() -> Result<Self, String> {
        init_rime_for_config();
        Ok(Self {
            user_dir: get_user_data_dir(),
        })
    }
    
    pub fn get_schema_list(&self) -> Vec<SchemaInfo> {
        let xime_dir = get_xime_data_dir();
        let shared_dir = get_shared_data_dir();
        let mut schemas = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();
        
        if let Ok(entries) = fs::read_dir(&self.user_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".schema.yaml") {
                        let schema_id = name.replace(".schema.yaml", "");
                        
                        if let Ok(content) = fs::read_to_string(&path) {
                            let info = extract_schema_info(&content, &schema_id);
                            schemas.push(info.clone());
                            seen_ids.insert(schema_id);
                        }
                    }
                }
            }
        }
        
        if let Ok(entries) = fs::read_dir(&xime_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".schema.yaml") {
                        let schema_id = name.replace(".schema.yaml", "");
                        
                        if seen_ids.contains(&schema_id) {
                            continue;
                        }
                        
                        if let Ok(content) = fs::read_to_string(&path) {
                            let info = extract_schema_info(&content, &schema_id);
                            schemas.push(info.clone());
                            seen_ids.insert(schema_id);
                        }
                    }
                }
            }
        }
        
        if let Ok(entries) = fs::read_dir(&shared_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".schema.yaml") {
                        let schema_id = name.replace(".schema.yaml", "");
                        
                        if seen_ids.contains(&schema_id) {
                            continue;
                        }
                        
                        if let Ok(content) = fs::read_to_string(&path) {
                            let info = extract_schema_info(&content, &schema_id);
                            schemas.push(info);
                        }
                    }
                }
            }
        }
        
        schemas.sort_by(|a, b| a.name.cmp(&b.name));
        schemas
    }
    
    pub fn get_selected_schema(&self) -> Option<String> {
        let default_custom = self.user_dir.join("default.custom.yaml");
        if !default_custom.exists() {
            return None;
        }
        
        let content = fs::read_to_string(&default_custom).ok()?;
        extract_selected_schema(&content)
    }
    
    pub fn set_schema_list(&self, schema_ids: &[&str]) -> Result<(), String> {
        let default_custom = self.user_dir.join("default.custom.yaml");
        
        let schema_list_yaml = schema_ids
            .iter()
            .map(|id| format!("    - schema: {}", id))
            .collect::<Vec<_>>()
            .join("\n");
        
        let content = format!(
r#"customization:
  distribution_code_name: Xime
  distribution_version: 1.0

patch:
  schema_list:
{}
"#, schema_list_yaml);
        
        fs::write(&default_custom, content)
            .map_err(|e| format!("Failed to write default.custom.yaml: {}", e))?;
        
        Ok(())
    }
    
    pub fn save(&self) -> Result<(), String> {
        Ok(())
    }
}

fn extract_schema_info(content: &str, schema_id: &str) -> SchemaInfo {
    let mut name = schema_id.to_string();
    let mut version = "未知".to_string();
    let mut author = "未知".to_string();
    let mut description = "".to_string();
    
    let lines: Vec<&str> = content.lines().collect();
    let mut in_schema_block = false;
    let mut indent_level = 0;
    
    for i in 0..lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        
        if trimmed == "schema:" {
            in_schema_block = true;
            indent_level = line.len() - line.trim_start().len();
            continue;
        }
        
        if in_schema_block {
            let current_indent = line.len() - line.trim_start().len();
            
            if current_indent <= indent_level && !trimmed.is_empty() && !trimmed.starts_with('#') {
                break;
            }
            
            if trimmed.starts_with("name:") {
                name = trimmed.split(':').nth(1)
                    .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                    .unwrap_or_else(|| schema_id.to_string());
            } else if trimmed.starts_with("version:") {
                version = trimmed.split(':').nth(1)
                    .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                    .unwrap_or_else(|| "未知".to_string());
            } else if trimmed.starts_with("author:") {
                author = extract_author(&lines, i);
            } else if trimmed.starts_with("description:") {
                description = extract_description(&lines, i);
            }
        }
    }
    
    SchemaInfo {
        schema_id: schema_id.to_string(),
        name,
        version,
        author,
        description,
    }
}

fn extract_author(lines: &[&str], start_idx: usize) -> String {
    let line = lines[start_idx];
    let trimmed = line.trim();
    
    if let Some(single) = trimmed.split(':').nth(1) {
        let author = single.trim().trim_matches('"').trim_matches('\'');
        if !author.is_empty() && !author.starts_with('-') && !author.starts_with('[') {
            return author.to_string();
        }
    }
    
    let indent = line.len() - line.trim_start().len();
    let mut authors = Vec::new();
    
    for i in (start_idx + 1)..lines.len() {
        let next_line = lines[i];
        let next_trimmed = next_line.trim();
        let next_indent = next_line.len() - next_line.trim_start().len();
        
        if next_indent <= indent || next_trimmed.is_empty() || !next_trimmed.starts_with('-') {
            break;
        }
        
        let author_name = next_trimmed.trim_start_matches('-').trim().trim_matches('"').trim_matches('\'');
        if !author_name.is_empty() {
            authors.push(author_name.to_string());
        }
    }
    
    if authors.is_empty() {
        "未知".to_string()
    } else {
        authors.join(", ")
    }
}

fn extract_description(lines: &[&str], start_idx: usize) -> String {
    let line = lines[start_idx];
    let trimmed = line.trim();
    
    if let Some(desc) = trimmed.split(':').nth(1) {
        let desc = desc.trim();
        if !desc.is_empty() && !desc.starts_with('|') {
            return desc.trim_matches('"').trim_matches('\'').to_string();
        }
    }
    
    if trimmed.ends_with('|') {
        let indent = line.len() - line.trim_start().len();
        let mut desc_lines = Vec::new();
        
        for i in (start_idx + 1)..lines.len() {
            let next_line = lines[i];
            let next_trimmed = next_line.trim();
            let next_indent = next_line.len() - next_line.trim_start().len();
            
            if next_indent < indent && !next_trimmed.is_empty() {
                break;
            }
            
            if !next_trimmed.is_empty() {
                desc_lines.push(next_trimmed.to_string());
            }
        }
        
        desc_lines.join(" ").trim().to_string()
    } else {
        "".to_string()
    }
}

fn extract_selected_schema(content: &str) -> Option<String> {
    for line in content.lines() {
        if line.contains("schema:") {
            let schema = line.split("schema:")
                .nth(1)
                .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string());
            if let Some(s) = schema {
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    None
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SpellerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_code_length: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_select: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_clear: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TranslatorConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_charset_filter: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_completion: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_sentence: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_user_dict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_encoder: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encode_commit_history: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_phrase_length: Option<i32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReverseLookupConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TraditionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opencc_config: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SchemaConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speller: Option<SpellerConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translator: Option<TranslatorConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse_lookup: Option<ReverseLookupConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tradition: Option<TraditionConfig>,
}

pub struct SchemaConfigManager {
    user_dir: PathBuf,
    schema_id: String,
}

impl SchemaConfigManager {
    pub fn new(schema_id: &str) -> Result<Self, String> {
        Ok(Self {
            user_dir: get_user_data_dir(),
            schema_id: schema_id.to_string(),
        })
    }
    
    pub fn get_config(&self) -> SchemaConfig {
        let custom_yaml = self.user_dir.join(format!("{}.custom.yaml", self.schema_id));
        let base_yaml = get_shared_data_dir().join(format!("{}.schema.yaml", self.schema_id));
        
        let base_config = if base_yaml.exists() {
            fs::read_to_string(&base_yaml)
                .ok()
                .and_then(|c| parse_schema_config(&c))
        } else {
            None
        };
        
        let custom_config = if custom_yaml.exists() {
            fs::read_to_string(&custom_yaml)
                .ok()
                .and_then(|c| parse_custom_patch(&c))
        } else {
            None
        };
        
        merge_configs(base_config, custom_config)
    }
    
    pub fn set_int(&self, key: &str, value: i32) -> Result<(), String> {
        self.update_custom_yaml(key, value.to_string())
    }
    
    pub fn set_bool(&self, key: &str, value: bool) -> Result<(), String> {
        self.update_custom_yaml(key, value.to_string())
    }
    
    pub fn set_string(&self, key: &str, value: &str) -> Result<(), String> {
        self.update_custom_yaml(key, value.to_string())
    }
    
    fn update_custom_yaml(&self, key: &str, value: String) -> Result<(), String> {
        let custom_yaml = self.user_dir.join(format!("{}.custom.yaml", self.schema_id));
        
        let existing_content = if custom_yaml.exists() {
            fs::read_to_string(&custom_yaml).ok()
        } else {
            None
        };
        
        let mut lines: Vec<String> = existing_content
            .map(|c| c.lines().map(|l| l.to_string()).collect())
            .unwrap_or_else(|| {
                vec![
                    "customization:".to_string(),
                    "  distribution_code_name: Xime".to_string(),
                    "  distribution_version: 1.0".to_string(),
                    "".to_string(),
                    "patch:".to_string(),
                ]
            });
        
        let key_parts: Vec<&str> = key.split('/').collect();
        let key_indent = key_parts.len() * 2;
        let formatted_key = key_parts.join("_");
        
        let new_line = format!("  {}: {}", formatted_key, value);
        
        let patch_idx = lines.iter().position(|l| l.trim() == "patch:");
        if let Some(idx) = patch_idx {
            let key_line_prefix = format!("  {}:", formatted_key);
            let existing_idx = lines.iter().skip(idx + 1)
                .position(|l| l.starts_with(&key_line_prefix));
            
            if let Some(e_idx) = existing_idx {
                lines[idx + 1 + e_idx] = new_line;
            } else {
                lines.insert(idx + 1, new_line);
            }
        }
        
        fs::write(&custom_yaml, lines.join("\n") + "\n")
            .map_err(|e| format!("Failed to write: {}", e))?;
        
        Ok(())
    }
    
    pub fn save(&self) -> Result<(), String> {
        Ok(())
    }
}

fn parse_schema_config(content: &str) -> Option<SchemaConfig> {
    let yaml: serde_yaml::Value = serde_yaml::from_str(content).ok()?;
    
    Some(SchemaConfig {
        speller: yaml.get("speller").and_then(|v| serde_yaml::from_value(v.clone()).ok()),
        translator: yaml.get("translator").and_then(|v| serde_yaml::from_value(v.clone()).ok()),
        reverse_lookup: yaml.get("reverse_lookup").and_then(|v| serde_yaml::from_value(v.clone()).ok()),
        tradition: yaml.get("tradition").and_then(|v| serde_yaml::from_value(v.clone()).ok()),
    })
}

fn parse_custom_patch(content: &str) -> Option<SchemaConfig> {
    let yaml: serde_yaml::Value = serde_yaml::from_str(content).ok()?;
    let patch = yaml.get("patch")?;
    serde_yaml::from_value(patch.clone()).ok()
}

fn merge_configs(base: Option<SchemaConfig>, custom: Option<SchemaConfig>) -> SchemaConfig {
    let mut result = base.unwrap_or_default();
    
    if let Some(c) = custom {
        if let Some(s) = c.speller {
            result.speller = Some(merge_speller(result.speller.unwrap_or_default(), s));
        }
        if let Some(t) = c.translator {
            result.translator = Some(merge_translator(result.translator.unwrap_or_default(), t));
        }
        if let Some(r) = c.reverse_lookup {
            result.reverse_lookup = Some(merge_reverse_lookup(result.reverse_lookup.unwrap_or_default(), r));
        }
        if let Some(tr) = c.tradition {
            result.tradition = Some(tr);
        }
    }
    
    result
}

fn merge_speller(base: SpellerConfig, custom: SpellerConfig) -> SpellerConfig {
    SpellerConfig {
        max_code_length: custom.max_code_length.or(base.max_code_length),
        auto_select: custom.auto_select.or(base.auto_select),
        auto_clear: custom.auto_clear.or(base.auto_clear),
    }
}

fn merge_translator(base: TranslatorConfig, custom: TranslatorConfig) -> TranslatorConfig {
    TranslatorConfig {
        enable_charset_filter: custom.enable_charset_filter.or(base.enable_charset_filter),
        enable_completion: custom.enable_completion.or(base.enable_completion),
        enable_sentence: custom.enable_sentence.or(base.enable_sentence),
        enable_user_dict: custom.enable_user_dict.or(base.enable_user_dict),
        enable_encoder: custom.enable_encoder.or(base.enable_encoder),
        encode_commit_history: custom.encode_commit_history.or(base.encode_commit_history),
        max_phrase_length: custom.max_phrase_length.or(base.max_phrase_length),
    }
}

fn merge_reverse_lookup(base: ReverseLookupConfig, custom: ReverseLookupConfig) -> ReverseLookupConfig {
    ReverseLookupConfig {
        prefix: custom.prefix.or(base.prefix),
        suffix: custom.suffix.or(base.suffix),
    }
}

pub struct RimeConfigManager {
    user_dir: PathBuf,
}

impl RimeConfigManager {
    pub fn new() -> Result<Self, String> {
        ensure_config_files();
        Ok(Self {
            user_dir: get_user_data_dir(),
        })
    }
    
    pub fn get_double(&self, key: &str) -> Option<f64> {
        self.get_value(key).and_then(|v| v.parse::<f64>().ok())
    }
    
    pub fn get_int(&self, key: &str) -> Option<i32> {
        self.get_value(key).and_then(|v| v.parse::<i32>().ok())
    }
    
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get_value(key).and_then(|v| v.parse::<bool>().ok())
    }
    
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.get_value(key)
    }
    
    fn get_value(&self, key: &str) -> Option<String> {
        let xime_yaml = self.user_dir.join("xime.yaml");
        let xime_custom = self.user_dir.join("xime.custom.yaml");
        
        if xime_custom.exists() {
            let content = fs::read_to_string(&xime_custom).ok()?;
            if let Some(v) = get_yaml_value(&content, key) {
                return Some(v);
            }
        }
        
        if xime_yaml.exists() {
            let content = fs::read_to_string(&xime_yaml).ok()?;
            get_yaml_value(&content, key)
        } else {
            None
        }
    }
    
    pub fn set_double(&self, key: &str, value: f64) -> Result<(), String> {
        self.set_value(key, value.to_string())
    }
    
    pub fn set_int(&self, key: &str, value: i32) -> Result<(), String> {
        self.set_value(key, value.to_string())
    }
    
    pub fn set_bool(&self, key: &str, value: bool) -> Result<(), String> {
        self.set_value(key, value.to_string())
    }
    
    pub fn set_string(&self, key: &str, value: &str) -> Result<(), String> {
        self.set_value(key, value.to_string())
    }
    
    fn set_value(&self, key: &str, value: String) -> Result<(), String> {
        let xime_custom = self.user_dir.join("xime.custom.yaml");
        
        let existing_content = if xime_custom.exists() {
            fs::read_to_string(&xime_custom).ok()
        } else {
            None
        };
        
        let mut lines: Vec<String> = existing_content
            .map(|c| c.lines().map(|l| l.to_string()).collect())
            .unwrap_or_else(|| {
                vec![
                    "customization:".to_string(),
                    "  distribution_code_name: Xime".to_string(),
                    "  distribution_version: 1.0".to_string(),
                    "".to_string(),
                    "patch:".to_string(),
                ]
            });
        
        let key_parts: Vec<&str> = key.split('/').collect();
        let formatted_key = if key_parts.len() > 1 {
            format!("{}{}", "  ".repeat(key_parts.len()), key_parts.join("_"))
        } else {
            format!("  {}", key)
        };
        
        let new_line = format!("{}: {}", formatted_key, value);
        
        let patch_idx = lines.iter().position(|l| l.trim() == "patch:");
        if let Some(idx) = patch_idx {
            let key_prefix = format!("{}:", formatted_key);
            let existing_idx = lines.iter().skip(idx + 1)
                .position(|l| l.starts_with(&key_prefix));
            
            if let Some(e_idx) = existing_idx {
                lines[idx + 1 + e_idx] = new_line;
            } else {
                lines.insert(idx + 1, new_line);
            }
        }
        
        fs::write(&xime_custom, lines.join("\n") + "\n")
            .map_err(|e| format!("Failed to write: {}", e))?;
        
        Ok(())
    }
    
    pub fn save(&self) -> Result<(), String> {
        Ok(())
    }
}

fn get_yaml_value(content: &str, key: &str) -> Option<String> {
    let key_parts: Vec<&str> = key.split('/').collect();
    
    let yaml: serde_yaml::Value = serde_yaml::from_str(content).ok()?;
    let mut current = &yaml;
    
    for part in &key_parts {
        current = current.get(part)?;
    }
    
    match current {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn get_xime_config_path() -> PathBuf {
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

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct XimeStyleConfig {
    #[serde(default)]
    pub color_scheme: String,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_candidate_count")]
    pub candidate_count: i32,
    #[serde(default)]
    pub show_code_hint: bool,
    #[serde(default = "default_corner_radius")]
    pub corner_radius: f32,
}

fn default_font_size() -> f32 { 14.0 }
fn default_candidate_count() -> i32 { 5 }
fn default_corner_radius() -> f32 { 8.0 }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ColorSchemeEntry {
    pub name: String,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub primary_color: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct XimeConfigFile {
    #[serde(default)]
    pub style: XimeStyleConfig,
    #[serde(default)]
    pub color_schemes: std::collections::HashMap<String, ColorSchemeEntry>,
}

fn deserialize_hex_color<'de, D>(deserializer: D) -> Result<u32, D::Error>
where D: serde::Deserializer<'de> {
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

pub struct XimeStyleManager {
    config_path: PathBuf,
    config: XimeConfigFile,
}

impl XimeStyleManager {
    pub fn load() -> Result<Self, String> {
        let config_path = get_xime_config_path();
        
        if !config_path.exists() {
            return Err("xime.yaml not found".to_string());
        }
        
        let content = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read xime.yaml: {}", e))?;
        
        let config: XimeConfigFile = serde_yaml::from_str(&content)
            .map_err(|e| format!("Failed to parse xime.yaml: {}", e))?;
        
        Ok(Self { config_path, config })
    }
    
    pub fn get_style(&self) -> XimeStyleConfig {
        self.config.style.clone()
    }
    
    pub fn get_color_schemes(&self) -> Vec<(String, String, u32)> {
        self.config.color_schemes.iter()
            .map(|(id, entry)| (id.clone(), entry.name.clone(), entry.primary_color))
            .collect()
    }
    
    pub fn set_color_scheme(&mut self, scheme_id: &str) -> Result<(), String> {
        self.config.style.color_scheme = scheme_id.to_string();
        self.save()
    }
    
    pub fn set_font_size(&mut self, size: f32) -> Result<(), String> {
        self.config.style.font_size = size;
        self.save()
    }
    
    pub fn set_candidate_count(&mut self, count: i32) -> Result<(), String> {
        self.config.style.candidate_count = count;
        self.save()
    }
    
    pub fn set_show_code_hint(&mut self, show: bool) -> Result<(), String> {
        self.config.style.show_code_hint = show;
        self.save()
    }
    
    pub fn set_corner_radius(&mut self, radius: f32) -> Result<(), String> {
        self.config.style.corner_radius = radius;
        self.save()
    }
    
    fn save(&self) -> Result<(), String> {
        let content = serde_yaml::to_string(&self.config)
            .map_err(|e| format!("Failed to serialize xime.yaml: {}", e))?;
        
        fs::write(&self.config_path, content)
            .map_err(|e| format!("Failed to write xime.yaml: {}", e))?;
        
        Ok(())
    }
}