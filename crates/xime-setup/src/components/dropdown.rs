use gpui::*;

#[derive(Clone)]
pub struct Dropdown {
    selected_label: String,
}

impl Dropdown {
    pub fn new(options: Vec<(String, String)>, selected: String) -> Self {
        let selected_label = options
            .iter()
            .find(|(_, v)| v == &selected)
            .map(|(l, _)| l.clone())
            .unwrap_or_else(|| selected.clone());
        Self { selected_label }
    }
}

impl IntoElement for Dropdown {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        div()
            .py(px(6.0))
            .px(px(12.0))
            .rounded(px(4.0))
            .bg(rgb(0x2d2d2d))
            .border_1()
            .border_color(rgb(0x4a4a4a))
            .text_size(px(14.0))
            .text_color(rgb(0xe0e0e0))
            .cursor_pointer()
            .child(self.selected_label)
    }
}
