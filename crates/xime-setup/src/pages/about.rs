use gpui::*;
use crate::state::SettingsState;
use crate::pages::SettingsApp;
use crate::theme::ThemeColors;

pub fn render(settings: Entity<SettingsState>, cx: &mut Context<SettingsApp>) -> AnyElement {
    let colors = cx.read_entity(&settings, |state, _| state.colors());
    
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
                .child("关于")
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(24.0))
                .p(px(20.0))
                .rounded(px(16.0))
                .bg(colors.surface)
                .border_1()
                .border_color(colors.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(16.0))
                        .child(
                            img("image/icon.png")
                                .w(px(64.0))
                                .h(px(64.0))
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .text_size(px(24.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(colors.foreground)
                                        .child("Xime 曦码输入法")
                                )
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .text_color(colors.foreground_muted)
                                        .child("基于 Rime 引擎的 Linux 五笔输入法")
                                )
                        )
                )
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .p(px(20.0))
                .rounded(px(16.0))
                .bg(colors.surface)
                .border_1()
                .border_color(colors.border)
                .child(
                    div()
                        .text_size(px(16.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(colors.foreground)
                        .child("基本信息")
                )
                .child(render_info_row("版本".to_string(), env!("CARGO_PKG_VERSION").to_string(), &colors))
                .child(render_info_row("作者".to_string(), env!("CARGO_PKG_AUTHORS").to_string(), &colors))
                .child(render_link_row("仓库".to_string(), "https://github.com/xime-wayland/xime".to_string(), &colors))
                .child(render_info_row("许可".to_string(), "GPL-3.0".to_string(), &colors))
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .p(px(20.0))
                .rounded(px(16.0))
                .bg(colors.surface)
                .border_1()
                .border_color(colors.border)
                .child(
                    div()
                        .text_size(px(16.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(colors.foreground)
                        .child("技术栈")
                )
                .child(render_info_row("输入引擎".to_string(), "Rime (librime)".to_string(), &colors))
                .child(render_info_row("界面框架".to_string(), "Rust + Wayland".to_string(), &colors))
                .child(render_info_row("UI 框架".to_string(), "GPUI".to_string(), &colors))
        )
        .into_any_element()
}

fn render_info_row(label: String, value: String, colors: &ThemeColors) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .py(px(8.0))
        .child(
            div()
                .text_size(px(14.0))
                .text_color(colors.foreground_muted)
                .child(label)
        )
        .child(
            div()
                .text_size(px(14.0))
                .text_color(colors.foreground)
                .child(value)
        )
}

fn render_link_row(label: String, url: String, colors: &ThemeColors) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .py(px(8.0))
        .child(
            div()
                .text_size(px(14.0))
                .text_color(colors.foreground_muted)
                .child(label)
        )
        .child(
            div()
                .text_size(px(14.0))
                .text_color(colors.primary)
                .child(url)
        )
}