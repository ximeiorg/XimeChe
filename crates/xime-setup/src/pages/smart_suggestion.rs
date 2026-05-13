use gpui::*;
use crate::components::{SettingsPage, SettingsGroup, SettingsItem, SettingsControl};
use crate::state::SettingsState;
use crate::pages::SettingsApp;

pub fn render(settings: Entity<SettingsState>, cx: &mut Context<SettingsApp>) -> AnyElement {
    let (enabled, suggestion_count, prefer_common_words, record_user_frequency, auto_adjust_frequency, learning_threshold, colors) = 
        cx.read_entity(&settings, |state, _| {
            (
                state.smart_suggestion.enabled,
                state.smart_suggestion.suggestion_count,
                state.smart_suggestion.prefer_common_words,
                state.smart_suggestion.record_user_frequency,
                state.smart_suggestion.auto_adjust_frequency,
                state.smart_suggestion.learning_threshold,
                state.colors(),
            )
        });
    
    let s1 = settings.clone();
    let s2 = settings.clone();
    let s3 = settings.clone();
    let s4 = settings.clone();
    let s5 = settings.clone();
    let s6 = settings.clone();
    
    SettingsPage::new("智能联想", colors.clone())
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