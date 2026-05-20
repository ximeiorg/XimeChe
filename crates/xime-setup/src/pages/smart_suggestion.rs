use gpui::*;
use crate::components::{SettingsPage, SettingsGroup, SettingsItem, SettingsControl};
use crate::state::SettingsState;
use crate::pages::SettingsApp;

pub fn render(settings: Entity<SettingsState>, cx: &mut Context<SettingsApp>) -> AnyElement {
    let (enabled, suggestion_count, prefer_common_words, record_user_frequency, auto_adjust_frequency, learning_threshold, auto_download, model_downloaded, colors) = 
        cx.read_entity(&settings, |state, _| {
            (
                state.smart_suggestion.enabled,
                state.smart_suggestion.suggestion_count,
                state.smart_suggestion.prefer_common_words,
                state.smart_suggestion.record_user_frequency,
                state.smart_suggestion.auto_adjust_frequency,
                state.smart_suggestion.learning_threshold,
                state.smart_suggestion.auto_download,
                state.smart_suggestion.model_downloaded,
                state.colors(),
            )
        });
    
    let s1 = settings.clone();
    let s2 = settings.clone();
    let s3 = settings.clone();
    let s4 = settings.clone();
    let s5 = settings.clone();
    let s6 = settings.clone();
    let s7 = settings.clone();
    let s8 = settings.clone();
    
    let model_status = if model_downloaded { "已下载" } else { "下载模型" };
    
    SettingsPage::new("智能联想", colors.clone())
        .group(
            SettingsGroup::new("模型管理", colors.clone())
                .items(vec![
                    SettingsItem::new("自动下载模型", 
                        SettingsControl::switch_with(auto_download,
                            move |val, _window, cx| {
                                s7.update(cx, |s: &mut SettingsState, cx| {
                                    s.smart_suggestion.auto_download = val;
                                    if let Err(e) = s.save_smart_suggestion() {
                                        eprintln!("Auto-save smart_suggestion failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("启用后自动下载智能联想模型"),
                    SettingsItem::new("下载模型", 
                        SettingsControl::button_with(model_status,
                            move |_window, cx| {
                                s8.update(cx, |s: &mut SettingsState, cx| {
                                    if let Err(e) = download_model() {
                                        eprintln!("Download model failed: {}", e);
                                    } else {
                                        s.smart_suggestion.model_downloaded = true;
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("predictive-text - 智能联想模型（vocab.json + model.onnx + model.onnx.data）"),
                ])
        )
        .group(
            SettingsGroup::new("联想功能", colors.clone())
                .items(vec![
                    SettingsItem::new("启用智能联想", 
                        SettingsControl::switch_with(enabled,
                            move |val, _window, cx| {
                                s1.update(cx, |s: &mut SettingsState, cx| {
                                    s.smart_suggestion.enabled = val;
                                    if let Err(e) = s.save_smart_suggestion() {
                                        eprintln!("Auto-save smart_suggestion failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("根据输入自动联想可能的词语"),
                    SettingsItem::new("联想词数量", 
                        SettingsControl::number_input_with(suggestion_count as f64,
                            move |val, _window, cx| {
                                s2.update(cx, |s: &mut SettingsState, cx| {
                                    s.smart_suggestion.suggestion_count = val as i32;
                                    if let Err(e) = s.save_smart_suggestion() {
                                        eprintln!("Auto-save smart_suggestion failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("显示的联想词数量"),
                    SettingsItem::new("优先常用词", 
                        SettingsControl::switch_with(prefer_common_words,
                            move |val, _window, cx| {
                                s3.update(cx, |s: &mut SettingsState, cx| {
                                    s.smart_suggestion.prefer_common_words = val;
                                    if let Err(e) = s.save_smart_suggestion() {
                                        eprintln!("Auto-save smart_suggestion failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("优先显示常用词"),
                ])
        )
        .group(
            SettingsGroup::new("学习功能", colors.clone())
                .items(vec![
                    SettingsItem::new("记录用户词频", 
                        SettingsControl::switch_with(record_user_frequency,
                            move |val, _window, cx| {
                                s4.update(cx, |s: &mut SettingsState, cx| {
                                    s.smart_suggestion.record_user_frequency = val;
                                    if let Err(e) = s.save_smart_suggestion() {
                                        eprintln!("Auto-save smart_suggestion failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("记录用户输入习惯，优化词序"),
                    SettingsItem::new("自动调频", 
                        SettingsControl::switch_with(auto_adjust_frequency,
                            move |val, _window, cx| {
                                s5.update(cx, |s: &mut SettingsState, cx| {
                                    s.smart_suggestion.auto_adjust_frequency = val;
                                    if let Err(e) = s.save_smart_suggestion() {
                                        eprintln!("Auto-save smart_suggestion failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("根据输入频率自动调整候选词顺序"),
                    SettingsItem::new("学习阈值", 
                        SettingsControl::number_input_with(learning_threshold as f64,
                            move |val, _window, cx| {
                                s6.update(cx, |s: &mut SettingsState, cx| {
                                    s.smart_suggestion.learning_threshold = val as i32;
                                    if let Err(e) = s.save_smart_suggestion() {
                                        eprintln!("Auto-save smart_suggestion failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("输入次数达到阈值后开始调整词序"),
                ])
        )
        .into_any_element()
}

fn get_model_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    std::path::PathBuf::from(home).join(".config/xime/models")
}

fn check_model_exists() -> bool {
    let model_dir = get_model_dir();
    let vocab_path = model_dir.join("vocab.json");
    let onnx_path = model_dir.join("model.onnx");
    let onnx_data_path = model_dir.join("model.onnx.data");
    vocab_path.exists() && onnx_path.exists() && onnx_data_path.exists()
}

fn download_model() -> Result<(), String> {
    let model_dir = get_model_dir();
    if !model_dir.exists() {
        std::fs::create_dir_all(&model_dir)
            .map_err(|e| format!("创建模型目录失败: {}", e))?;
    }
    
    let files = [
        ("vocab.json", "https://modelscope.cn/models/bikeand/predictive-text/resolve/master/vocab.json"),
        ("model.onnx", "https://modelscope.cn/models/bikeand/predictive-text/resolve/master/model.onnx"),
        ("model.onnx.data", "https://modelscope.cn/models/bikeand/predictive-text/resolve/master/model.onnx.data"),
    ];
    
    for (filename, url) in files {
        println!("正在下载 {}...", filename);
        let response = reqwest::blocking::get(url)
            .map_err(|e| format!("下载 {} 失败: {}", filename, e))?;
        
        let content = response.bytes()
            .map_err(|e| format!("读取 {} 失败: {}", filename, e))?;
        
        let path = model_dir.join(filename);
        std::fs::write(&path, &content)
            .map_err(|e| format!("保存 {} 失败: {}", filename, e))?;
        
        println!("{} 下载完成", filename);
    }
    
    Ok(())
}