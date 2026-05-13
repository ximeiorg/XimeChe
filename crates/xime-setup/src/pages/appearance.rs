use gpui::*;
use crate::components::{SettingsPage, SettingsGroup, SettingsItem, SettingsControl};
use crate::state::SettingsState;
use crate::pages::SettingsApp;

pub fn render(settings: Entity<SettingsState>, cx: &mut Context<SettingsApp>) -> AnyElement {
    let (font_size, candidate_count, show_code_hint, colors) = cx.read_entity(&settings, |state, _| {
        (state.appearance.font_size, state.appearance.candidate_count, state.appearance.show_code_hint, state.colors())
    });
    
    let settings_clone = settings.clone();
    let settings_clone2 = settings.clone();
    let settings_clone3 = settings.clone();
    
    SettingsPage::new("外观", colors.clone())
        .group(
            SettingsGroup::new("候选栏设置", colors.clone())
                .items(vec![
                    SettingsItem::new("字体大小", 
                        SettingsControl::number_input_with(font_size, 
                            move |val, _window, cx| {
                                settings_clone.update(cx, |s: &mut SettingsState, cx| {
                                    s.appearance.font_size = val;
                                    if let Err(e) = s.save_appearance() {
                                        eprintln!("Auto-save appearance failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("候选栏字体大小"),
                    SettingsItem::new("候选数量", 
                        SettingsControl::number_input_with(candidate_count as f64,
                            move |val, _window, cx| {
                                settings_clone2.update(cx, |s: &mut SettingsState, cx| {
                                    s.appearance.candidate_count = val as i32;
                                    if let Err(e) = s.save_appearance() {
                                        eprintln!("Auto-save appearance failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("候选栏显示的候选词数量"),
                    SettingsItem::new("显示编码提示", 
                        SettingsControl::switch_with(show_code_hint,
                            move |val, _window, cx| {
                                settings_clone3.update(cx, |s: &mut SettingsState, cx| {
                                    s.appearance.show_code_hint = val;
                                    if let Err(e) = s.save_appearance() {
                                        eprintln!("Auto-save appearance failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("在候选词旁显示编码"),
                ])
        )
        .into_any_element()
}