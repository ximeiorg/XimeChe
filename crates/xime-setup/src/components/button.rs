use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::Arc;

#[derive(Clone)]
pub struct Button {
    label: String,
    id: String,
    on_click: Option<Arc<dyn Fn(&mut Window, &mut App) + 'static>>,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        let label_str = label.into();
        Self { 
            label: label_str.clone(),
            id: label_str,
            on_click: None 
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
}

impl IntoElement for Button {
    type Element = Stateful<Div>;

    fn into_element(self) -> Self::Element {
        let primary = rgb(0x8F73E2);
        let on_click = self.on_click;
        let id: SharedString = self.id.into();
        
        div()
            .id(id)
            .py(px(8.0))
            .px(px(16.0))
            .rounded(px(12.0))
            .bg(primary)
            .text_color(white())
            .text_size(px(14.0))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0x7A5FD0)))
            .when_some(on_click, |this: Stateful<Div>, cb| {
                this.on_click(move |_, window, cx| {
                    cb(window, cx);
                })
            })
            .child(self.label)
    }
}