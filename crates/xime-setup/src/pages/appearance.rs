use crate::components::{SettingsControl, SettingsGroup, SettingsItem, SettingsPage};
use crate::pages::SettingsApp;
use crate::state::SettingsState;
use gpui::*;

pub fn render(settings: Entity<SettingsState>, cx: &mut Context<SettingsApp>) -> AnyElement {
    let (
        font_size,
        candidate_count,
        corner_radius,
        color_scheme,
        color_schemes,
        color_schemes_loaded,
        colors,
    ) = cx.read_entity(&settings, |state, _| {
        (
            state.appearance.font_size,
            state.appearance.candidate_count,
            state.appearance.corner_radius,
            state.appearance.color_scheme.clone(),
            state.appearance.available_color_schemes.clone(),
            state.appearance.color_schemes_loaded,
            state.colors(),
        )
    });

    if !color_schemes_loaded {
        settings.update(cx, |s, cx| {
            s.load_color_schemes(cx);
        });
    }

    let settings_clone = settings.clone();
    let settings_clone2 = settings.clone();
    let settings_clone3 = settings.clone();
    let settings_clone4 = settings.clone();

    let color_scheme_items: Vec<AnyElement> = color_schemes
        .iter()
        .enumerate()
        .map(|(i, (id, name, color))| {
            let is_selected = id == &color_scheme;
            let settings_for_color = settings_clone4.clone();
            let id_clone = id.clone();
            let r = (*color >> 16) as u8;
            let g = (*color >> 8) as u8;
            let b = *color as u8;
            let rgb_color = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
            let bg_with_alpha = rgba(rgb_color << 8 | 0x26);
            let hover_with_alpha = rgba(rgb_color << 8 | 0x40);

            div()
                .id(("color_scheme", i))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.0))
                .p(px(10.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(if is_selected {
                    rgb(rgb_color)
                } else {
                    rgb(0xE0E0E0)
                })
                .bg(bg_with_alpha)
                .cursor_pointer()
                .hover(|style| style.bg(hover_with_alpha))
                .on_click(move |_event, _window, cx| {
                    settings_for_color.update(cx, |s, cx| {
                        s.appearance.color_scheme = id_clone.clone();
                        if let Err(e) = s.save_color_scheme() {
                            eprintln!("Failed to save color scheme: {}", e);
                        }
                        cx.notify();
                    });
                })
                .child(
                    div()
                        .w(px(80.0))
                        .h(px(12.0))
                        .rounded(px(12.0))
                        .border_2()
                        .border_color(rgb(0xCCCCCC))
                        .bg(rgb(0xFFFFFF))
                        .child(
                            div()
                                .w(px(20.0))
                                .h_full()
                                .rounded(px(12.0))
                                .bg(rgb(rgb_color)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .w(px(20.0))
                                .h(px(20.0))
                                .rounded(px(6.0))
                                .bg(rgb(rgb_color))
                                .mr_1(),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(colors.foreground)
                                .child(name.clone()),
                        )
                        .justify_between()
                        .into_any_element(),
                )
                .into_any_element()
        })
        .collect();

    SettingsPage::new("外观", colors.clone())
        .group(
            SettingsGroup::new("主题设置", colors.clone()).custom_item(
                SettingsItem::render_custom(
                    &colors,
                    "配色方案",
                    Some("选择输入法的主题颜色"),
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(12.0))
                        .w_full()
                        .children(color_scheme_items),
                ),
            ),
        )
        .group(SettingsGroup::new("候选栏设置", colors.clone()).items(vec![
                    SettingsItem::new("字体大小",
                        SettingsControl::number_input_with(font_size,
                            move |val, _window, cx| {
                                settings_clone.update(cx, |s, cx| {
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
                                settings_clone2.update(cx, |s, cx| {
                                    s.appearance.candidate_count = val as i32;
                                    if let Err(e) = s.save_appearance() {
                                        eprintln!("Auto-save appearance failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("候选栏显示的候选词数量"),
                    SettingsItem::new("圆角半径",
                        SettingsControl::number_input_with(corner_radius,
                            move |val, _window, cx| {
                                settings_clone3.update(cx, |s, cx| {
                                    s.appearance.corner_radius = val;
                                    if let Err(e) = s.save_appearance() {
                                        eprintln!("Auto-save appearance failed: {}", e);
                                    }
                                    cx.notify();
                                });
                            }
                        )
                    ).description("候选栏窗口圆角大小"),
                ]))
        .into_any_element()
}
