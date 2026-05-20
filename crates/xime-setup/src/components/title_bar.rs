use crate::state::SettingsState;
use crate::theme::ThemeColors;
use gpui::*;

pub struct TitleBar;

impl TitleBar {
    pub fn render(
        settings: Entity<SettingsState>,
        colors: &ThemeColors,
        cx: &mut App,
    ) -> impl IntoElement {
        let deploy_message = cx.read_entity(&settings, |state, _| state.deploy_message.clone());

        let drag_region = div()
            .id("drag-region")
            .flex_1()
            .h(px(40.0))
            .bg(colors.background)
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(MouseButton::Left, |_event, window, _cx| {
                window.start_window_move();
            });

        let drag_region_with_msg = if let Some(msg) = deploy_message {
            drag_region.child(
                div()
                    .text_size(px(13.0))
                    .text_color(colors.primary)
                    .child(msg),
            )
        } else {
            drag_region
        };

        let logo_region = div()
            .id("logo-title")
            .w(px(213.0))
            .h(px(40.0))
            .bg(colors.sidebar_bg)
            .flex()
            .items_center()
            .pl(px(12.0))
            .on_mouse_down(MouseButton::Left, |_event, window, _cx| {
                window.start_window_move();
            })
            .child(Self::logo())
            .child(Self::title_text(colors));

        div()
            .flex()
            .w_full()
            .h(px(40.0))
            .child(logo_region)
            .child(drag_region_with_msg)
            .child(Self::deploy_button(colors, settings))
            .child(Self::separator(colors))
            .child(Self::close_button(colors.foreground, colors.background))
    }

    fn logo() -> impl IntoElement {
        img("image/icon.png")
            .w(px(24.0))
            .h(px(24.0))
            .flex_none()
            .mr_2()
    }

    fn title_text(colors: &ThemeColors) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors.on_primary)
                    .child("Xime"),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(hsla(0.0, 0.0, 1.0, 0.7))
                    .child("曦码输入法"),
            )
    }

    fn deploy_button(colors: &ThemeColors, settings: Entity<SettingsState>) -> impl IntoElement {
        let primary_hover = hsla(colors.primary.h, colors.primary.s, colors.primary.l, 0.1);

        div()
            .id("deploy-btn")
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .px(px(12.0))
            .h(px(40.0))
            .bg(colors.background)
            .text_size(px(13.0))
            .text_color(colors.primary)
            .cursor_pointer()
            .hover(|style| style.bg(primary_hover))
            .on_click(move |_event, _window, cx| {
                settings.update(cx, |s: &mut SettingsState, cx| {
                    match s.deploy() {
                        Ok(_) => {
                            println!("部署成功！");
                        }
                        Err(e) => {
                            eprintln!("部署失败: {}", e);
                        }
                    }
                    cx.notify();
                });
            })
            .child("部署")
    }

    fn separator(colors: &ThemeColors) -> impl IntoElement {
        div().w(px(1.0)).h(px(24.0)).my(px(8.0)).bg(colors.border)
    }

    fn close_button(text_color: Hsla, bg: Hsla) -> impl IntoElement {
        div()
            .id("close-btn")
            .flex()
            .items_center()
            .justify_center()
            .w(px(46.0))
            .h(px(40.0))
            .bg(bg)
            .text_size(px(14.0))
            .text_color(text_color)
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0xc42b1c)))
            .on_click(|_event, window, _cx| {
                window.remove_window();
            })
            .child("✕")
    }
}
