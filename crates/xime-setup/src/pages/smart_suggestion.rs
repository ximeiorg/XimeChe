use crate::components::{SettingsControl, SettingsGroup, SettingsItem, SettingsPage};
use crate::pages::SettingsApp;
use crate::state::SettingsState;
use gpui::*;
use std::path::PathBuf;

pub fn render(settings: Entity<SettingsState>, cx: &mut Context<SettingsApp>) -> AnyElement {
    let (
        enabled,
        suggestion_count,
        record_user_frequency,
        auto_adjust_frequency,
        learning_threshold,
        model_name,
        downloading,
        colors,
    ) = cx.read_entity(&settings, |state, _| {
        (
            state.smart_suggestion.enabled,
            state.smart_suggestion.suggestion_count,
            state.smart_suggestion.record_user_frequency,
            state.smart_suggestion.auto_adjust_frequency,
            state.smart_suggestion.learning_threshold,
            state.smart_suggestion.model_name.clone(),
            state.smart_suggestion.downloading,
            state.colors(),
        )
    });

    let model_exists = check_model_exists(&model_name);
    if downloading && model_exists {
        settings.update(cx, |s, cx| {
            s.smart_suggestion.downloading = false;
            cx.notify();
        });
    }
    let model_status = if downloading {
        "下载中..."
    } else if model_exists {
        "已下载"
    } else {
        "下载模型"
    };
    let model_name_for_btn = model_name.clone();

    let updated_settings = settings.clone();

    SettingsPage::new("智能联想", colors.clone())
        .group(SettingsGroup::new("联想功能", colors.clone()).items(vec![
                    SettingsItem::new("启用智能联想",
                        SettingsControl::switch_with(enabled, {
                            let settings = settings.clone();
                            move |val, _window, cx| {
                                settings.update(cx, |s: &mut SettingsState, cx| {
                                    s.smart_suggestion.enabled = val;
                                    if let Err(e) = s.save_smart_suggestion() {
                                        eprintln!("Auto-save smart_suggestion failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        })
                    ).description("根据输入自动联想可能的词语"),
                    SettingsItem::new("联想词数量",
                        SettingsControl::number_input_with(suggestion_count as f64, {
                            let settings = settings.clone();
                            move |val, _window, cx| {
                                settings.update(cx, |s: &mut SettingsState, cx| {
                                    s.smart_suggestion.suggestion_count = val as i32;
                                    if let Err(e) = s.save_smart_suggestion() {
                                        eprintln!("Auto-save smart_suggestion failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        })
                    ).description("显示的联想词数量"),
                ]))
        .group(SettingsGroup::new("学习功能", colors.clone()).items(vec![
                    SettingsItem::new("记录用户词频",
                        SettingsControl::switch_with(record_user_frequency, {
                            let settings = settings.clone();
                            move |val, _window, cx| {
                                settings.update(cx, |s: &mut SettingsState, cx| {
                                    s.smart_suggestion.record_user_frequency = val;
                                    if let Err(e) = s.save_smart_suggestion() {
                                        eprintln!("Auto-save smart_suggestion failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        })
                    ).description("记录用户输入习惯，优化词序"),
                    SettingsItem::new("自动调频",
                        SettingsControl::switch_with(auto_adjust_frequency, {
                            let settings = settings.clone();
                            move |val, _window, cx| {
                                settings.update(cx, |s: &mut SettingsState, cx| {
                                    s.smart_suggestion.auto_adjust_frequency = val;
                                    if let Err(e) = s.save_smart_suggestion() {
                                        eprintln!("Auto-save smart_suggestion failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        })
                    ).description("根据输入频率自动调整候选词顺序"),
                    SettingsItem::new("学习阈值",
                        SettingsControl::number_input_with(learning_threshold as f64, {
                            let settings = settings.clone();
                            move |val, _window, cx| {
                                settings.update(cx, |s: &mut SettingsState, cx| {
                                    s.smart_suggestion.learning_threshold = val as i32;
                                    if let Err(e) = s.save_smart_suggestion() {
                                        eprintln!("Auto-save smart_suggestion failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        })
                    ).description("输入次数达到阈值后开始调整词序"),
                ]))
        .group(SettingsGroup::new("模型管理", colors.clone()).items(vec![
                SettingsItem::new(
                    "下载模型",
                    SettingsControl::button_with(model_status, move |_window, cx| {
                        if downloading {
                            return;
                        }
                        updated_settings.update(cx, |s, cx| {
                            s.smart_suggestion.downloading = true;
                            cx.notify();
                        });
                        let name = model_name_for_btn.clone();
                        std::thread::spawn(move || {
                            if let Err(e) = download_model_blocking(&name) {
                                eprintln!("Download failed: {}", e);
                            } else {
                                println!("Model downloaded successfully");
                            }
                        });
                    }),
                )
                .description(format!("智能联想模型 - {}", model_name)),
            ]))
        .into_any_element()
}

fn get_model_dir(model_name: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    PathBuf::from(home)
        .join(".config/xime/models")
        .join(model_name)
}

fn check_model_exists(model_name: &str) -> bool {
    let model_dir = get_model_dir(model_name);
    model_dir.join("vocab.json").exists()
        && model_dir.join("model.onnx").exists()
        && model_dir.join("model.onnx.data").exists()
}

fn download_model_blocking(model_name: &str) -> Result<(), String> {
    let model_dir = get_model_dir(model_name);
    if !model_dir.exists() {
        std::fs::create_dir_all(&model_dir).map_err(|e| format!("创建模型目录失败: {}", e))?;
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
        .build()
        .map_err(|e| format!("创建HTTP客户端失败: {}", e))?;

    let base_url = format!(
        "https://modelscope.cn/models/bikeand/{}/resolve/master",
        model_name
    );
    let files = [
        ("vocab.json", format!("{}/vocab.json", base_url)),
        ("model.onnx", format!("{}/model.onnx", base_url)),
        ("model.onnx.data", format!("{}/model.onnx.data", base_url)),
    ];

    for (filename, url) in files {
        println!("正在下载 {}...", filename);
        let response = client
            .get(&url)
            .send()
            .map_err(|e| format!("下载 {} 失败: {}", filename, e))?;

        if !response.status().is_success() {
            return Err(format!(
                "下载 {} 失败: HTTP {}",
                filename,
                response.status()
            ));
        }

        let content = response
            .bytes()
            .map_err(|e| format!("读取 {} 失败: {}", filename, e))?;

        let path = model_dir.join(filename);
        std::fs::write(&path, &content).map_err(|e| format!("保存 {} 失败: {}", filename, e))?;

        println!("{} 下载完成 ({} bytes)", filename, content.len());
    }

    Ok(())
}
