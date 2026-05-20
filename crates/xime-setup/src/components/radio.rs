use crate::theme::ThemeColors;
use gpui::{prelude::FluentBuilder, *};

pub struct Radio {
    checked: bool,
    colors: Option<ThemeColors>,
}

impl Radio {
    pub fn new(checked: bool) -> Self {
        Self {
            checked,
            colors: None,
        }
    }

    pub fn theme(mut self, colors: ThemeColors) -> Self {
        self.colors = Some(colors);
        self
    }
}

impl IntoElement for Radio {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        let size = px(16.0);
        let colors = self.colors.unwrap_or_else(|| {
            ThemeColors::from_theme(&crate::theme::SystemTheme::Light, 0x8F73E2)
        });

        div()
            .w(size)
            .h(size)
            .rounded(size / 2.0)
            .border_1()
            .border_color(if self.checked {
                colors.primary
            } else {
                colors.border
            })
            .flex()
            .items_center()
            .justify_center()
            .when(self.checked, |this: Div| {
                this.child(
                    div()
                        .w(px(8.0))
                        .h(px(8.0))
                        .rounded(px(4.0))
                        .bg(colors.primary),
                )
            })
    }
}
