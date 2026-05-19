use gpui::*;
use crate::theme::{SystemTheme, ThemeColors};
use crate::rime_config::{RimeConfigManager, SchemaManager, SchemaConfig, SchemaConfigManager, deploy_all, SchemaInfo, XimeStyleManager};

pub struct SettingsState {
    pub appearance: AppearanceState,
    pub input_schema: InputSchemaState,
    pub smart_suggestion: SmartSuggestionState,
    pub system_theme: SystemTheme,
    pub deploy_message: Option<String>,
    pub schemas_loaded: bool,
}

impl SettingsState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut state = Self {
            appearance: AppearanceState::default(),
            input_schema: InputSchemaState::default(),
            smart_suggestion: SmartSuggestionState::default(),
            system_theme: SystemTheme::detect(),
            deploy_message: None,
            schemas_loaded: false,
        };
        state.load_color_schemes(cx);
        state
    }

    pub fn load_schemas(&mut self, cx: &mut Context<Self>) {
        if self.schemas_loaded {
            return;
        }
        if let Ok(manager) = SchemaManager::new() {
            let schemas = manager.get_schema_list();
            self.input_schema.available_schemas = schemas;
            self.schemas_loaded = true;
            cx.notify();
        }
    }

    pub fn load_schema_config(&mut self, cx: &mut Context<Self>) {
        if self.input_schema.config_loaded {
            return;
        }
        if self.input_schema.selected_schema >= self.input_schema.available_schemas.len() {
            return;
        }
        let schema_id = &self.input_schema.available_schemas[self.input_schema.selected_schema].schema_id;
        if let Ok(manager) = SchemaConfigManager::new(schema_id) {
            self.input_schema.schema_config = manager.get_config();
            self.input_schema.config_loaded = true;
            cx.notify();
        }
    }

    pub fn colors(&self) -> ThemeColors {
        let primary_color = self.get_primary_color();
        ThemeColors::from_theme(&self.system_theme, primary_color)
    }
    
    fn get_primary_color(&self) -> u32 {
        self.appearance.available_color_schemes
            .iter()
            .find(|(id, _, _)| id == &self.appearance.color_scheme)
            .map(|(_, _, color)| *color)
            .unwrap_or(0x8F73E2)
    }
    
    pub fn load_color_schemes(&mut self, cx: &mut Context<Self>) {
        if self.appearance.color_schemes_loaded {
            return;
        }
        if let Ok(manager) = XimeStyleManager::load() {
            let style = manager.get_style();
            self.appearance.color_scheme = style.color_scheme;
            self.appearance.available_color_schemes = manager.get_color_schemes();
            self.appearance.font_size = style.font_size as f64;
            self.appearance.candidate_count = style.candidate_count;
            self.appearance.show_code_hint = style.show_code_hint;
            self.appearance.corner_radius = style.corner_radius as f64;
            self.appearance.color_schemes_loaded = true;
            cx.notify();
        }
    }

pub fn save_color_scheme(&self) -> Result<(), String> {
        let mut manager = XimeStyleManager::load()?;
        manager.set_color_scheme(&self.appearance.color_scheme)?;
        notify_daemon_reload_style();
        Ok(())
    }
    
    pub fn save_appearance(&self) -> Result<(), String> {
        let mut manager = XimeStyleManager::load()?;
        
        manager.set_font_size(self.appearance.font_size as f32)?;
        manager.set_candidate_count(self.appearance.candidate_count)?;
        manager.set_show_code_hint(self.appearance.show_code_hint)?;
        manager.set_corner_radius(self.appearance.corner_radius as f32)?;
        
        notify_daemon_reload_style();
        Ok(())
    }

    pub fn save_schema(&self) -> Result<(), String> {
        if self.input_schema.selected_schema < self.input_schema.available_schemas.len() {
            let selected_id = &self.input_schema.available_schemas[self.input_schema.selected_schema].schema_id;
            
            let manager = RimeConfigManager::new()?;
            manager.set_string("default_schema", selected_id)?;
            manager.save()?;
            
            let schema_manager = SchemaManager::new()?;
            schema_manager.set_schema_list(&[selected_id])?;
            schema_manager.save()?;
            
            deploy_all()?;
            
            notify_daemon_reload();
        }
        Ok(())
    }
    
    pub fn save_schema_config(&self) -> Result<(), String> {
        if self.input_schema.selected_schema >= self.input_schema.available_schemas.len() {
            return Ok(())
        }
        
        let schema_id = &self.input_schema.available_schemas[self.input_schema.selected_schema].schema_id;
        let manager = SchemaConfigManager::new(schema_id)?;
        
        let config = &self.input_schema.schema_config;
        
        if let Some(v) = config.speller.max_code_length {
            manager.set_int("speller/max_code_length", v)?;
        }
        if let Some(v) = config.speller.auto_select {
            manager.set_bool("speller/auto_select", v)?;
        }
        if let Some(v) = &config.speller.auto_clear {
            if !v.is_empty() {
                manager.set_string("speller/auto_clear", v)?;
            }
        }
        
        if let Some(v) = config.translator.enable_charset_filter {
            manager.set_bool("translator/enable_charset_filter", v)?;
        }
        if let Some(v) = config.translator.enable_completion {
            manager.set_bool("translator/enable_completion", v)?;
        }
        if let Some(v) = config.translator.enable_sentence {
            manager.set_bool("translator/enable_sentence", v)?;
        }
        if let Some(v) = config.translator.enable_user_dict {
            manager.set_bool("translator/enable_user_dict", v)?;
        }
        if let Some(v) = config.translator.enable_encoder {
            manager.set_bool("translator/enable_encoder", v)?;
        }
        if let Some(v) = config.translator.encode_commit_history {
            manager.set_bool("translator/encode_commit_history", v)?;
        }
        if let Some(v) = config.translator.max_phrase_length {
            manager.set_int("translator/max_phrase_length", v)?;
        }
        
        if let Some(v) = &config.reverse_lookup.prefix {
            manager.set_string("reverse_lookup/prefix", v)?;
        }
        if let Some(v) = &config.reverse_lookup.suffix {
            manager.set_string("reverse_lookup/suffix", v)?;
        }
        
        if let Some(v) = &config.tradition.opencc_config {
            manager.set_string("tradition/opencc_config", v)?;
        }
        
        manager.save()?;
        
        Ok(())
}

pub fn save_smart_suggestion(&self) -> Result<(), String> {
        let manager = RimeConfigManager::new()?;
        
        manager.set_bool("smart_suggestion/enabled", self.smart_suggestion.enabled)?;
        manager.set_int("smart_suggestion/suggestion_count", self.smart_suggestion.suggestion_count)?;
        manager.set_bool("smart_suggestion/prefer_common_words", self.smart_suggestion.prefer_common_words)?;
        manager.set_bool("smart_suggestion/record_user_frequency", self.smart_suggestion.record_user_frequency)?;
        manager.set_bool("smart_suggestion/auto_adjust_frequency", self.smart_suggestion.auto_adjust_frequency)?;
        manager.set_int("smart_suggestion/learning_threshold", self.smart_suggestion.learning_threshold)?;
        
        manager.save()?;
        
        Ok(())
    }

    pub fn deploy(&mut self) -> Result<(), String> {
        let result = deploy_all();
        match &result {
            Ok(_) => {
                if notify_daemon_reload() {
                    self.deploy_message = Some("部署成功！配置已重载。".to_string());
                } else {
                    self.deploy_message = Some("部署成功！(服务器未运行，配置将在下次启动时生效)".to_string());
                }
            }
            Err(e) => {
                self.deploy_message = Some(format!("部署失败: {}", e));
            }
        }
        result
    }
}

fn notify_daemon_reload() -> bool {
    zbus::blocking::Connection::session()
        .ok()
        .and_then(|conn| {
            conn.call_method(
                Some("org.xime.Xime"),
                "/org/xime/Xime",
                Some("org.xime.Xime.Controller"),
                "Deploy",
                &(),
            )
            .ok()
        })
        .is_some()
}

fn notify_daemon_reload_style() {
    zbus::blocking::Connection::session()
        .ok()
        .and_then(|conn| {
            conn.call_method(
                Some("org.xime.Xime"),
                "/org/xime/Xime",
                Some("org.xime.Xime.Controller"),
                "ReloadStyle",
                &(),
            )
            .ok()
        });
}

#[derive(Clone, Default)]
pub struct AppearanceState {
    pub font_size: f64,
    pub candidate_count: i32,
    pub show_code_hint: bool,
    pub corner_radius: f64,
    pub color_scheme: String,
    pub available_color_schemes: Vec<(String, String, u32)>,
    pub color_schemes_loaded: bool,
}

#[derive(Clone, Default)]
pub struct InputSchemaState {
    pub selected_schema: usize,
    pub available_schemas: Vec<SchemaInfo>,
    pub schema_config: SchemaConfig,
    pub config_loaded: bool,
}

#[derive(Clone, Default)]
pub struct SmartSuggestionState {
    pub enabled: bool,
    pub suggestion_count: i32,
    pub prefer_common_words: bool,
    pub record_user_frequency: bool,
    pub auto_adjust_frequency: bool,
    pub learning_threshold: i32,
}