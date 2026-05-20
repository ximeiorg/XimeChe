use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::Arc;

#[derive(Clone)]
pub struct NumberInput {
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    on_change: Option<Arc<dyn Fn(f64, &mut Window, &mut App) + 'static>>,
    bg: Option<Hsla>,
    border_color: Option<Hsla>,
    text_color: Option<Hsla>,
    primary_color: Option<Hsla>,
    btn_hover_bg: Option<Hsla>,
    disabled_color: Option<Hsla>,
}

impl NumberInput {
    pub fn new(value: f64) -> Self {
        Self {
            value,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            on_change: None,
            bg: None,
            border_color: None,
            text_color: None,
            primary_color: None,
            btn_hover_bg: None,
            disabled_color: None,
        }
    }

    pub fn min(mut self, min: f64) -> Self {
        self.min = min;
        self
    }

    pub fn max(mut self, max: f64) -> Self {
        self.max = max;
        self
    }

    pub fn step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }

    pub fn on_change(mut self, callback: impl Fn(f64, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Arc::new(callback));
        self
    }

    pub fn theme(
        mut self,
        bg: Hsla,
        border: Hsla,
        text: Hsla,
        primary: Hsla,
        hover: Hsla,
        disabled: Hsla,
    ) -> Self {
        self.bg = Some(bg);
        self.border_color = Some(border);
        self.text_color = Some(text);
        self.primary_color = Some(primary);
        self.btn_hover_bg = Some(hover);
        self.disabled_color = Some(disabled);
        self
    }
}

impl IntoElement for NumberInput {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        let primary = self.primary_color.unwrap_or(rgb(0x8F73E2).into());
        let bg = self.bg.unwrap_or(rgb(0x262626).into());
        let border = self.border_color.unwrap_or(rgb(0x404040).into());
        let text = self.text_color.unwrap_or(rgb(0xe0e0e0).into());
        let hover_bg = self.btn_hover_bg.unwrap_or(rgb(0x404040).into());
        let disabled = self.disabled_color.unwrap_or(rgb(0x808080).into());

        let value = self.value;
        let min = self.min;
        let max = self.max;
        let step = self.step;
        let on_change = self.on_change.clone();

        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .w(px(140.0))
            .h(px(36.0))
            .rounded(px(12.0))
            .bg(bg)
            .border_1()
            .border_color(border)
            .child(
                div()
                    .id("dec-btn")
                    .w(px(32.0))
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(hover_bg))
                    .text_size(px(16.0))
                    .when(value > min, |this: Stateful<Div>| this.text_color(text))
                    .when(value <= min, |this: Stateful<Div>| {
                        this.text_color(disabled)
                    })
                    .when_some(on_change.clone(), |this: Stateful<Div>, cb| {
                        this.on_click(move |_, window, cx| {
                            let new_value = (value - step).max(min);
                            cb(new_value, window, cx);
                        })
                    })
                    .child("-"),
            )
            .child(
                div()
                    .flex_1()
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(14.0))
                    .text_color(primary)
                    .font_weight(FontWeight::MEDIUM)
                    .child(format!("{}", value as i32)),
            )
            .child(
                div()
                    .id("inc-btn")
                    .w(px(32.0))
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(hover_bg))
                    .text_size(px(16.0))
                    .when(value < max, |this: Stateful<Div>| this.text_color(text))
                    .when(value >= max, |this: Stateful<Div>| {
                        this.text_color(disabled)
                    })
                    .when_some(on_change, |this: Stateful<Div>, cb| {
                        this.on_click(move |_, window, cx| {
                            let new_value = (value + step).min(max);
                            cb(new_value, window, cx);
                        })
                    })
                    .child("+"),
            )
    }
}
