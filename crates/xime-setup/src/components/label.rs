use gpui::*;

#[derive(Clone)]
pub struct Label {
    text: String,
    text_color: Option<Hsla>,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            text_color: None,
        }
    }

    pub fn theme(mut self, color: Hsla) -> Self {
        self.text_color = Some(color);
        self
    }
}

impl IntoElement for Label {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        div()
            .text_size(px(14.0))
            .text_color(self.text_color.unwrap_or(rgb(0xc0c0c0).into()))
            .child(self.text)
    }
}
