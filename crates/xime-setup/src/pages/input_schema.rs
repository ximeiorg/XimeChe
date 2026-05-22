use crate::components::{Radio, SettingsControl, SettingsGroup, SettingsItem, SettingsPage};
use crate::pages::SettingsApp;
use crate::state::SettingsState;
use gpui::{prelude::FluentBuilder, IntoElement, ParentElement, *};
use xime_config::{SchemaConfig, SchemaManager};

pub fn render(settings: Entity<SettingsState>, cx: &mut Context<SettingsApp>) -> AnyElement {
    let schemas_loaded = cx.read_entity(&settings, |state, _| state.schemas_loaded);

    if !schemas_loaded {
        cx.update_entity(&settings, |state: &mut SettingsState, cx| {
            if let Ok(manager) = SchemaManager::new() {
                let schemas = manager.get_schema_list();
                let selected_schema = manager
                    .get_selected_schema()
                    .and_then(|selected_id| schemas.iter().position(|s| s.schema_id == selected_id))
                    .unwrap_or(0);
                state.input_schema.available_schemas = schemas;
                state.input_schema.selected_schema = selected_schema;
                state.schemas_loaded = true;
                cx.notify();
            }
        });
    }

    let config_loaded = cx.read_entity(&settings, |state, _| state.input_schema.config_loaded);
    if !config_loaded {
        cx.update_entity(&settings, |state: &mut SettingsState, cx| {
            state.load_schema_config(cx);
        });
    }

    let (selected, schemas, colors, schema_config, schema_name) =
        cx.read_entity(&settings, |state, _| {
            let schema_name = if state.input_schema.selected_schema
                < state.input_schema.available_schemas.len()
            {
                state.input_schema.available_schemas[state.input_schema.selected_schema]
                    .name
                    .clone()
            } else {
                String::new()
            };
            (
                state.input_schema.selected_schema,
                state.input_schema.available_schemas.clone(),
                state.colors(),
                state.input_schema.schema_config.clone(),
                schema_name,
            )
        });

    let settings_clone = settings.clone();

    let schema_items: Vec<AnyElement> = schemas
        .iter()
        .enumerate()
        .map(|(i, schema)| {
            let is_selected = i == selected;
            let settings_item = settings_clone.clone();
            let primary = colors.primary.clone();
            let surface_variant = colors.surface_variant.clone();
            let radio_colors = colors.clone();

            div()
                .id(("schema", i))
                .flex()
                .items_center()
                .gap(px(12.0))
                .py(px(8.0))
                .px(px(12.0))
                .rounded(px(8.0))
                .cursor_pointer()
                .hover(|style| style.bg(surface_variant))
                .when(is_selected, |this| {
                    this.border_1()
                        .border_color(primary.clone())
                        .bg(colors.selection)
                })
                .on_click(move |_event, _window, cx| {
                    settings_item.update(cx, |s: &mut SettingsState, cx| {
                        if s.input_schema.selected_schema != i {
                            s.input_schema.selected_schema = i;
                            s.input_schema.config_loaded = false;
                            if let Err(e) = s.save_schema() {
                                eprintln!("Auto-save schema failed: {}", e);
                            }
                            cx.notify();
                        }
                    });
                })
                .child(Radio::new(is_selected).theme(radio_colors))
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(if is_selected {
                            primary
                        } else {
                            colors.foreground
                        })
                        .child(schema.name.clone()),
                )
                .into_any_element()
        })
        .collect();

    let config_section = if config_loaded {
        render_schema_config(settings.clone(), &schema_config, &colors, &schema_name, cx)
    } else {
        div()
            .text_size(px(14.0))
            .text_color(colors.foreground_muted)
            .child("正在加载方案配置...")
            .into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .p(px(16.0))
        .w_full()
        .child(
            div()
                .text_size(px(20.0))
                .font_weight(FontWeight::BOLD)
                .text_color(colors.foreground)
                .child("输入方案"),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .p(px(16.0))
                .rounded(px(12.0))
                .bg(colors.surface)
                .border_1()
                .border_color(colors.border)
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(colors.foreground_muted)
                        .child("选择默认输入方案"),
                )
                .when(schemas.is_empty(), |this| {
                    this.child(
                        div()
                            .text_size(px(14.0))
                            .text_color(colors.foreground_muted)
                            .child("未找到输入方案，请先部署"),
                    )
                })
                .when(!schemas.is_empty(), |this| this.children(schema_items)),
        )
        .when(
            !schemas.is_empty() && selected < schemas.len() && config_loaded,
            |this| this.child(config_section),
        )
        .into_any_element()
}

fn render_schema_config(
    settings: Entity<SettingsState>,
    config: &SchemaConfig,
    colors: &crate::theme::ThemeColors,
    schema_name: &str,
    _cx: &mut Context<SettingsApp>,
) -> AnyElement {
    let settings_for_switch = settings.clone();
    let settings_for_int = settings.clone();

    let speller_group = SettingsGroup::new("编码设置", colors.clone())
        .description(format!("{} - 拼写/编码相关设置", schema_name))
        .items(vec![
            SettingsItem::new(
                "最大编码长度",
                SettingsControl::number_input_with(
                    config
                        .speller
                        .max_code_length
                        .map(|v| v as f64)
                        .unwrap_or(4.0),
                    {
                        let s = settings_for_int.clone();
                        move |v, _window, cx| {
                            s.update(cx, |state: &mut SettingsState, cx| {
                                state.input_schema.schema_config.speller.max_code_length =
                                    Some(v as i32);
                                if let Err(e) = state.save_schema_config() {
                                    eprintln!("Save failed: {}", e);
                                }
                                cx.notify();
                            });
                        }
                    },
                ),
            )
            .description("如五笔为4码上屏"),
            SettingsItem::new(
                "四码唯一自动上屏",
                SettingsControl::switch_with(config.speller.auto_select.unwrap_or(true), {
                    let s = settings_for_switch.clone();
                    move |v, _window, cx| {
                        s.update(cx, |state: &mut SettingsState, cx| {
                            state.input_schema.schema_config.speller.auto_select = Some(v);
                            if let Err(e) = state.save_schema_config() {
                                eprintln!("Save failed: {}", e);
                            }
                            cx.notify();
                        });
                    }
                }),
            )
            .description("编码唯一时自动上屏"),
        ]);

    let translator_group = SettingsGroup::new("翻译器设置", colors.clone())
        .description("候选词生成和显示设置")
        .items(vec![
            SettingsItem::new(
                "显示未完成编码词条",
                SettingsControl::switch_with(
                    config.translator.enable_completion.unwrap_or(true),
                    {
                        let s = settings_for_switch.clone();
                        move |v, _window, cx| {
                            s.update(cx, |state: &mut SettingsState, cx| {
                                state
                                    .input_schema
                                    .schema_config
                                    .translator
                                    .enable_completion = Some(v);
                                if let Err(e) = state.save_schema_config() {
                                    eprintln!("Save failed: {}", e);
                                }
                                cx.notify();
                            });
                        }
                    },
                ),
            )
            .description("提前显示编码未输入完整的词条"),
            SettingsItem::new(
                "启用字符集过滤",
                SettingsControl::switch_with(
                    config.translator.enable_charset_filter.unwrap_or(true),
                    {
                        let s = settings_for_switch.clone();
                        move |v, _window, cx| {
                            s.update(cx, |state: &mut SettingsState, cx| {
                                state
                                    .input_schema
                                    .schema_config
                                    .translator
                                    .enable_charset_filter = Some(v);
                                if let Err(e) = state.save_schema_config() {
                                    eprintln!("Save failed: {}", e);
                                }
                                cx.notify();
                            });
                        }
                    },
                ),
            )
            .description("根据字符集过滤候选词"),
            SettingsItem::new(
                "启用用户词典",
                SettingsControl::switch_with(
                    config.translator.enable_user_dict.unwrap_or(false),
                    {
                        let s = settings_for_switch.clone();
                        move |v, _window, cx| {
                            s.update(cx, |state: &mut SettingsState, cx| {
                                state.input_schema.schema_config.translator.enable_user_dict =
                                    Some(v);
                                if let Err(e) = state.save_schema_config() {
                                    eprintln!("Save failed: {}", e);
                                }
                                cx.notify();
                            });
                        }
                    },
                ),
            )
            .description("记录用户词频和动态词"),
            SettingsItem::new(
                "启用自动造词",
                SettingsControl::switch_with(config.translator.enable_encoder.unwrap_or(false), {
                    let s = settings_for_switch.clone();
                    move |v, _window, cx| {
                        s.update(cx, |state: &mut SettingsState, cx| {
                            state.input_schema.schema_config.translator.enable_encoder = Some(v);
                            if let Err(e) = state.save_schema_config() {
                                eprintln!("Save failed: {}", e);
                            }
                            cx.notify();
                        });
                    }
                }),
            )
            .description("根据输入自动生成新词"),
            SettingsItem::new(
                "最大自动造词长度",
                SettingsControl::number_input_with(
                    config
                        .translator
                        .max_phrase_length
                        .map(|v| v as f64)
                        .unwrap_or(10.0),
                    {
                        let s = settings_for_int.clone();
                        move |v, _window, cx| {
                            s.update(cx, |state: &mut SettingsState, cx| {
                                state
                                    .input_schema
                                    .schema_config
                                    .translator
                                    .max_phrase_length = Some(v as i32);
                                if let Err(e) = state.save_schema_config() {
                                    eprintln!("Save failed: {}", e);
                                }
                                cx.notify();
                            });
                        }
                    },
                ),
            )
            .description("自动生成词的最大字数"),
        ]);

    let reverse_lookup_group = SettingsGroup::new("反查设置", colors.clone())
        .description("通过拼音等反查编码")
        .items(vec![
            SettingsItem::new(
                "反查前缀",
                SettingsControl::label(
                    config
                        .reverse_lookup
                        .prefix
                        .clone()
                        .unwrap_or_else(|| "z".to_string()),
                ),
            )
            .description("输入此字符后开始反查"),
            SettingsItem::new(
                "反查后缀",
                SettingsControl::label(
                    config
                        .reverse_lookup
                        .suffix
                        .clone()
                        .unwrap_or_else(|| "'".to_string()),
                ),
            )
            .description("输入此字符结束反查"),
        ]);

    let tradition_items: Vec<SettingsItem> = if config.tradition.opencc_config.is_some() {
        vec![SettingsItem::new(
            "简繁转换配置",
            SettingsControl::label(
                config
                    .tradition
                    .opencc_config
                    .clone()
                    .unwrap_or_else(|| "s2hk.json".to_string()),
            ),
        )
        .description("可选: s2t(繁体), s2hk(香港), s2tw(台湾)")]
    } else {
        vec![]
    };

    let tradition_group = if tradition_items.is_empty() {
        None
    } else {
        Some(
            SettingsGroup::new("简繁转换", colors.clone())
                .description("OpenCC 简入繁出设置")
                .items(tradition_items),
        )
    };

    SettingsPage::new("方案详细设置", colors.clone())
        .group(speller_group)
        .group(translator_group)
        .group(reverse_lookup_group)
        .when_some(tradition_group, |this, g| this.group(g))
        .into_any_element()
}
