use gpui::*;

#[derive(Clone)]
pub struct Kbd {
    key: String,
    bg: Option<Hsla>,
    border: Option<Hsla>,
    text: Option<Hsla>,
}

impl Kbd {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            bg: None,
            border: None,
            text: None,
        }
    }

    pub fn theme(mut self, bg: Hsla, border: Hsla, text: Hsla) -> Self {
        self.bg = Some(bg);
        self.border = Some(border);
        self.text = Some(text);
        self
    }
}

impl IntoElement for Kbd {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        div()
            .py(px(2.0))
            .px(px(6.0))
            .rounded(px(4.0))
            .bg(self.bg.unwrap_or(rgb(0x2d2d2d).into()))
            .border_1()
            .border_color(self.border.unwrap_or(rgb(0x4a4a4a).into()))
            .text_size(px(12.0))
            .text_color(self.text.unwrap_or(rgb(0xe0e0e0).into()))
            .font_weight(FontWeight::MEDIUM)
            .child(self.key)
    }
}
