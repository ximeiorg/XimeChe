use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::Arc;

#[derive(Clone)]
pub struct Button {
    label: String,
    id: String,
    on_click: Option<Arc<dyn Fn(&mut Window, &mut App) + 'static>>,
    primary: Option<Hsla>,
    primary_hover: Option<Hsla>,
    on_primary: Option<Hsla>,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        let label_str = label.into();
        Self {
            label: label_str.clone(),
            id: label_str,
            on_click: None,
            primary: None,
            primary_hover: None,
            on_primary: None,
        }
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn on_click(mut self, callback: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Arc::new(callback));
        self
    }

    pub fn theme(mut self, primary: Hsla, hover: Hsla, on_primary: Hsla) -> Self {
        self.primary = Some(primary);
        self.primary_hover = Some(hover);
        self.on_primary = Some(on_primary);
        self
    }
}

impl IntoElement for Button {
    type Element = Stateful<Div>;

    fn into_element(self) -> Self::Element {
        let primary = self.primary.unwrap_or(rgb(0x8F73E2).into());
        let hover = self.primary_hover.unwrap_or(rgb(0x7A5FD0).into());
        let text_color = self.on_primary.unwrap_or(white().into());
        let on_click = self.on_click;
        let id: SharedString = self.id.into();

        div()
            .id(id)
            .py(px(8.0))
            .px(px(16.0))
            .rounded(px(12.0))
            .bg(primary)
            .text_color(text_color)
            .text_size(px(14.0))
            .cursor_pointer()
            .hover(|style| style.bg(hover))
            .when_some(on_click, |this: Stateful<Div>, cb| {
                this.on_click(move |_, window, cx| {
                    cb(window, cx);
                })
            })
            .child(self.label)
    }
}
