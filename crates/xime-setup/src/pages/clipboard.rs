use gpui::*;
use crate::components::{SettingsPage, SettingsGroup, SettingsItem, SettingsControl};
use crate::state::SettingsState;
use crate::pages::SettingsApp;

pub fn render(settings: Entity<SettingsState>, cx: &mut Context<SettingsApp>) -> AnyElement {
    let (enabled, history_count, retention_days, colors) = cx.read_entity(&settings, |state, _| {
        (state.clipboard.enabled, state.clipboard.history_count, state.clipboard.retention_days, state.colors())
    });
    
    let settings_clone = settings.clone();
    let settings_clone2 = settings.clone();
    let settings_clone3 = settings.clone();
    
    SettingsPage::new("剪切板", colors.clone())
        .group(
            SettingsGroup::new("剪切板功能", colors.clone())
                .items(vec![
                    SettingsItem::new("启用剪切板", 
                        SettingsControl::switch_with(enabled,
                            move |val, _window, cx| {
                                settings_clone.update(cx, |s: &mut SettingsState, cx| {
                                    s.clipboard.enabled = val;
                                    if let Err(e) = s.save_clipboard() {
                                        eprintln!("Auto-save clipboard failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("启用剪切板历史记录功能"),
                    SettingsItem::new("历史数量", 
                        SettingsControl::number_input_with(history_count as f64,
                            move |val, _window, cx| {
                                settings_clone2.update(cx, |s: &mut SettingsState, cx| {
                                    s.clipboard.history_count = val as i32;
                                    if let Err(e) = s.save_clipboard() {
                                        eprintln!("Auto-save clipboard failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("保存的剪切板历史条数"),
                    SettingsItem::new("保留天数", 
                        SettingsControl::number_input_with(retention_days as f64,
                            move |val, _window, cx| {
                                settings_clone3.update(cx, |s: &mut SettingsState, cx| {
                                    s.clipboard.retention_days = val as i32;
                                    if let Err(e) = s.save_clipboard() {
                                        eprintln!("Auto-save clipboard failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("剪切板历史保留天数"),
                ])
        )
        .into_any_element()
}