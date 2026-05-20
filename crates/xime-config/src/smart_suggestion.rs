use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct SmartSuggestionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_suggestion_count")]
    pub suggestion_count: i32,
    #[serde(default)]
    pub prefer_common_words: bool,
    #[serde(default)]
    pub record_user_frequency: bool,
    #[serde(default)]
    pub auto_adjust_frequency: bool,
    #[serde(default = "default_learning_threshold")]
    pub learning_threshold: i32,
    #[serde(default)]
    pub model: SmartSuggestionModelConfig,
}

fn default_suggestion_count() -> i32 {
    5
}
fn default_learning_threshold() -> i32 {
    3
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct SmartSuggestionModelConfig {
    #[serde(default = "default_model_provider")]
    pub provider: String,
    #[serde(default = "default_model_name")]
    pub name: String,
    #[serde(default)]
    pub auto_download: bool,
    #[serde(default)]
    pub files: Vec<SmartSuggestionModelFile>,
}

fn default_model_provider() -> String {
    "modelscope".to_string()
}
fn default_model_name() -> String {
    "predictive-text-small".to_string()
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct SmartSuggestionModelFile {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub filename: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smart_suggestion_defaults() {
        let config = SmartSuggestionConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.suggestion_count, 5);
        assert_eq!(config.learning_threshold, 3);
        assert_eq!(config.model.provider, "modelscope");
        assert_eq!(config.model.name, "predictive-text-small");
    }

    #[test]
    fn test_smart_suggestion_deserialize() {
        let yaml = "
enabled: true
suggestion_count: 10
prefer_common_words: true
model:
  provider: custom
  name: my-model
  files:
    - url: https://example.com/file1.bin
      filename: file1.bin
    - url: https://example.com/file2.bin
      filename: file2.bin
";
        let config: SmartSuggestionConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.enabled);
        assert_eq!(config.suggestion_count, 10);
        assert!(config.prefer_common_words);
        assert_eq!(config.model.provider, "custom");
        assert_eq!(config.model.name, "my-model");
        assert_eq!(config.model.files.len(), 2);
        assert_eq!(config.model.files[0].filename, "file1.bin");
        assert_eq!(config.model.files[1].filename, "file2.bin");
    }
}
