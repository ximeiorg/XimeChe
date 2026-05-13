use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::Arc;

#[derive(Clone)]
pub struct Switch {
    checked: bool,
    on_change: Option<Arc<dyn Fn(bool, &mut Window, &mut App) + 'static>>,
    primary: Option<Hsla>,
    bg_off: Option<Hsla>,
}

impl Switch {
    pub fn new(checked: bool) -> Self {
        Self { checked, on_change: None, primary: None, bg_off: None }
    }
    
    pub fn on_change(mut self, callback: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Arc::new(callback));
        self
    }
    
    pub fn theme(mut self, primary: Hsla, bg_off: Hsla) -> Self {
        self.primary = Some(primary);
        self.bg_off = Some(bg_off);
        self
    }
}

impl IntoElement for Switch {
    type Element = Stateful<Div>;

    fn into_element(self) -> Self::Element {
        let checked = self.checked;
        let on_change = self.on_change;
        let primary = self.primary.unwrap_or(rgb(0x8F73E2).into());
        let bg_off = self.bg_off.unwrap_or(rgb(0x4d4d4d).into());
        let toggle_width = px(44.0);
        let toggle_height = px(24.0);
        let knob_size = px(18.0);
        let padding = px(3.0);

        div()
            .id("switch")
            .w(toggle_width)
            .h(toggle_height)
            .rounded(px(12.0))
            .flex()
            .items_center()
            .when(checked, |this: Stateful<Div>| {
                this.bg(primary)
                    .justify_end()
                    .pr(padding)
            })
            .when(!checked, |this: Stateful<Div>| {
                this.bg(bg_off)
                    .justify_start()
                    .pl(padding)
            })
            .cursor_pointer()
            .when_some(on_change, |this: Stateful<Div>, cb| {
                this.on_click(move |_, window, cx| {
                    cb(!checked, window, cx);
                })
            })
            .child(
                div()
                    .w(knob_size)
                    .h(knob_size)
                    .rounded(px(10.0))
                    .bg(white())
            )
    }
}