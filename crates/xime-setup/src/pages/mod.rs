pub mod input_schema;
pub mod appearance;
pub mod clipboard;
pub mod hotkeys;
pub mod smart_suggestion;
pub mod dictionary;
pub mod about;

use gpui::{prelude::FluentBuilder, ParentElement, IntoElement, *};
use crate::components::{TitleBar};
use crate::state::SettingsState;

pub struct SettingsApp {
    current_page: usize,
    settings: Entity<SettingsState>,
}

impl SettingsApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let settings = cx.new(|cx| SettingsState::new(cx));
        
        Self { 
            current_page: 0,
            settings,
        }
    }
}

fn get_page_icon(index: usize) -> &'static str {
    match index {
        0 => "icons/keyboard.svg",
        1 => "icons/palette.svg",
        2 => "icons/clipboard.svg",
        3 => "icons/command.svg",
        4 => "icons/thinking.svg",
        5 => "icons/word.svg",
        6 => "icons/about.svg",
        _ => "icons/keyboard.svg",
    }
}

impl Render for SettingsApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_background_appearance(WindowBackgroundAppearance::Blurred);
        
        let pages = ["输入方案", "外观", "剪切板", "快捷键", "智能联想", "词库管理", "关于"];
        let current = self.current_page;
        let settings = self.settings.clone();
        let settings_for_title = settings.clone();
        let colors = cx.read_entity(&settings, |state, _| state.colors());
        
        let sidebar = div()
            .w(px(213.0))
            .h_full()
            .bg(rgb(0x2d1f3d))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .p(px(8.0))
            .children(
                pages
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
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
                            .when(is_current, |this: Stateful<Div>| this.bg(rgb(0x8F73E2)))
                            .when(!is_current, |this: Stateful<Div>| {
                                this.cursor_pointer()
                                    .hover(|style: StyleRefinement| style.bg(hsla(0.0, 0.0, 1.0, 0.05)))
                            })
                            .text_size(px(15.0))
                            .text_color(if is_current { rgb(0xffffff) } else { rgb(0xb0b0b0) })
                            .on_click(move |_, _window: &mut Window, cx: &mut App| {
                                cx.update_entity(&view, |app: &mut SettingsApp, cx: &mut Context<SettingsApp>| {
                                    app.current_page = i;
                                    cx.notify();
                                });
                            })
                            .child(
                                img(icon_path)
                                    .w(px(20.0))
                                    .h(px(20.0))
                            )
                            .child(
                                div()
                                    .text_size(px(15.0))
                                    .text_color(if is_current { rgb(0xffffff) } else { rgb(0xb0b0b0) })
                                    .child(name.to_string())
                            )
                    })
            );

        let content = match self.current_page {
            0 => input_schema::render(settings, cx),
            1 => appearance::render(settings, cx),
            2 => clipboard::render(settings, cx),
            3 => hotkeys::render(settings, cx),
            4 => smart_suggestion::render(settings, cx),
            5 => dictionary::render(settings, cx),
            6 => about::render(settings, cx),
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
                            .overflow_y_scroll()
                            .bg(colors.background)
                            .child(content)
                    )
            )
    }
}