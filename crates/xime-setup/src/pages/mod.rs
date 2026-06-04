pub mod about;
pub mod appearance;
pub mod dictionary;
pub mod hotkeys;
pub mod input_schema;
pub mod sync;

use crate::components::{TextInput, TitleBar};
use crate::state::SettingsState;
use crate::webdav::WebdavConfig;
use gpui::{prelude::FluentBuilder, IntoElement, ParentElement, *};

pub struct SettingsApp {
    current_page: usize,
    settings: Entity<SettingsState>,
    // Text inputs for sync page (inline editable)
    pub url_input: Entity<TextInput>,
    pub username_input: Entity<TextInput>,
    pub password_input: Entity<TextInput>,
    pub remote_dir_input: Entity<TextInput>,
}

impl SettingsApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let settings = cx.new(SettingsState::new);
        let config = WebdavConfig::load();

        // 获取主题颜色：浅色模式下用淡灰背景，深色模式下用深灰背景
        let theme_colors = cx.read_entity(&settings, |s, _| s.colors());
        let input_bg = theme_colors.surface_variant;
        let input_border = theme_colors.border_variant;
        let input_text = theme_colors.foreground;
        let input_placeholder = theme_colors.foreground_muted;

        let url_input = cx.new(|cx| {
            TextInput::new(
                cx,
                config.url.clone(),
                "https://example.com/remote.php/dav/...",
            )
            .theme(input_bg, input_border, input_text, input_placeholder)
        });
        let username_input = cx.new(|cx| {
            TextInput::new(cx, config.username.clone(), "输入用户名").theme(
                input_bg,
                input_border,
                input_text,
                input_placeholder,
            )
        });
        let password_input = cx.new(|cx| {
            TextInput::new(cx, config.password.clone(), "输入密码").theme(
                input_bg,
                input_border,
                input_text,
                input_placeholder,
            )
        });
        let remote_dir_input = cx.new(|cx| {
            TextInput::new(cx, config.remote_dir.clone(), "xime").theme(
                input_bg,
                input_border,
                input_text,
                input_placeholder,
            )
        });

        Self {
            current_page: 0,
            settings,
            url_input,
            username_input,
            password_input,
            remote_dir_input,
        }
    }
}

fn get_page_icon(index: usize) -> &'static str {
    match index {
        0 => "icons/keyboard.svg",
        1 => "icons/palette.svg",
        2 => "icons/command.svg",
        3 => "icons/sync.svg",
        4 => "icons/about.svg",
        _ => "icons/keyboard.svg",
    }
}

impl Render for SettingsApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_background_appearance(WindowBackgroundAppearance::Blurred);

        let pages = ["输入方案", "外观", "快捷键", "同步", "关于"];
        let current = self.current_page;
        let settings = self.settings.clone();
        let settings_for_title = settings.clone();
        let colors = cx.read_entity(&settings, |state, _| state.colors());

        let sidebar = div()
            .w(px(213.0))
            .h_full()
            .bg(colors.sidebar_bg)
            .flex()
            .flex_col()
            .gap(px(2.0))
            .p(px(8.0))
            .children(pages.iter().enumerate().map(|(i, name)| {
                let is_current = i == current;
                let view = cx.entity();
                let icon_path = get_page_icon(i);
                div()
                    .id(("menu", i))
                    .py(px(10.0))
                    .px(px(12.0))
                    .rounded(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .when(is_current, |this: Stateful<Div>| this.bg(colors.primary))
                    .when(!is_current, |this: Stateful<Div>| {
                        this.cursor_pointer()
                            .hover(|style: StyleRefinement| style.bg(hsla(0.0, 0.0, 1.0, 0.15)))
                    })
                    .text_size(px(15.0))
                    .text_color(colors.on_primary)
                    .on_click(move |_, _window: &mut Window, cx: &mut App| {
                        cx.update_entity(
                            &view,
                            |app: &mut SettingsApp, cx: &mut Context<SettingsApp>| {
                                app.current_page = i;
                                cx.notify();
                            },
                        );
                    })
                    .child(img(icon_path).w(px(20.0)).h(px(20.0)))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .text_color(colors.on_primary)
                            .child(name.to_string()),
                    )
            }));

        let content = match self.current_page {
            0 => input_schema::render(settings, cx),
            1 => appearance::render(settings, cx),
            2 => hotkeys::render(settings, cx),
            3 => sync::render(
                settings,
                self.url_input.clone(),
                self.username_input.clone(),
                self.password_input.clone(),
                self.remote_dir_input.clone(),
                cx,
            ),
            4 => about::render(settings, cx),
            _ => input_schema::render(settings, cx),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(TitleBar::render(settings_for_title, &colors, cx))
            .child(
                div()
                    .id("content-area")
                    .flex()
                    .flex_1()
                    .h_full()
                    .overflow_hidden()
                    .child(sidebar)
                    .child(
                        div()
                            .id("content-scroll")
                            .flex_1()
                            .min_w(px(400.0))
                            .overflow_y_scroll()
                            .bg(colors.background)
                            .child(content),
                    ),
            )
    }
}
