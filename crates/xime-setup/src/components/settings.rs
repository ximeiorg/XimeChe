use gpui::{prelude::FluentBuilder, *};
use crate::components::{Switch, Dropdown, NumberInput, Button, Kbd, Label};
use crate::theme::ThemeColors;

pub struct SettingsItem {
    label: String,
    control: SettingsControl,
    description: Option<String>,
}

pub enum SettingsControl {
    Switch(Switch),
    Dropdown(Dropdown),
    NumberInput(NumberInput),
    Button(Button),
    Kbd(Kbd),
    Label(Label),
}

impl SettingsControl {
    pub fn switch(checked: bool) -> Self {
        SettingsControl::Switch(Switch::new(checked))
    }
    
    pub fn switch_with(checked: bool, on_change: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        SettingsControl::Switch(Switch::new(checked).on_change(on_change))
    }

    pub fn dropdown(options: Vec<(String, String)>, selected: String) -> Self {
        SettingsControl::Dropdown(Dropdown::new(options, selected))
    }

    pub fn number_input(value: f64) -> Self {
        SettingsControl::NumberInput(NumberInput::new(value))
    }
    
    pub fn number_input_with(value: f64, on_change: impl Fn(f64, &mut Window, &mut App) + 'static) -> Self {
        SettingsControl::NumberInput(NumberInput::new(value).on_change(on_change))
    }

    pub fn button(label: impl Into<String>) -> Self {
        SettingsControl::Button(Button::new(label))
    }
    
    pub fn button_with(label: impl Into<String>, on_click: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        SettingsControl::Button(Button::new(label).on_click(on_click))
    }

    pub fn kbd(key: impl Into<String>) -> Self {
        SettingsControl::Kbd(Kbd::new(key))
    }

    pub fn label(text: impl Into<String>) -> Self {
        SettingsControl::Label(Label::new(text))
    }
}

impl SettingsItem {
    pub fn new(label: impl Into<String>, control: SettingsControl) -> Self {
        Self {
            label: label.into(),
            control,
            description: None,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn switch(checked: bool) -> SettingsControl {
        SettingsControl::switch(checked)
    }

    pub fn dropdown(options: Vec<(String, String)>, selected: String) -> SettingsControl {
        SettingsControl::dropdown(options, selected)
    }

    pub fn number_input(value: f64) -> SettingsControl {
        SettingsControl::number_input(value)
    }

    pub fn button(label: impl Into<String>) -> SettingsControl {
        SettingsControl::button(label)
    }

    pub fn kbd(key: impl Into<String>) -> SettingsControl {
        SettingsControl::kbd(key)
    }

    pub fn label(text: impl Into<String>) -> SettingsControl {
        SettingsControl::label(text)
    }
}

impl SettingsItem {
    pub fn render(&self, colors: &ThemeColors) -> Div {
        let control_element: AnyElement = match &self.control {
            SettingsControl::Switch(s) => {
                let themed = s.clone()
                    .theme(colors.primary, colors.disabled);
                themed.into_any_element()
            },
            SettingsControl::Dropdown(d) => d.clone().into_any_element(),
            SettingsControl::NumberInput(n) => {
                let themed = n.clone()
                    .theme(colors.surface_variant, colors.border_variant, colors.foreground, colors.surface_variant, colors.foreground_muted);
                themed.into_any_element()
            },
            SettingsControl::Button(b) => b.clone().into_any_element(),
            SettingsControl::Kbd(k) => {
                let themed = k.clone()
                    .theme(colors.surface_variant, colors.border_variant, colors.foreground);
                themed.into_any_element()
            },
            SettingsControl::Label(l) => {
                let themed = l.clone()
                    .theme(colors.foreground);
                themed.into_any_element()
            },
        };

        div()
            .flex()
            .items_center()
            .justify_between()
            .py(px(12.0))
            .px(px(16.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(colors.foreground)
                            .child(self.label.clone())
                    )
                    .when_some(self.description.clone(), |this: Div, desc| {
                        this.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(colors.foreground_muted)
                                .child(desc)
                        )
                    })
            )
            .child(control_element)
    }
}

pub struct SettingsGroup {
    title: String,
    description: Option<String>,
    items: Vec<SettingsItem>,
    colors: ThemeColors,
}

impl SettingsGroup {
    pub fn new(title: impl Into<String>, colors: ThemeColors) -> Self {
        Self { title: title.into(), description: None, items: vec![], colors }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn items(mut self, items: Vec<SettingsItem>) -> Self {
        self.items = items;
        self
    }
}

impl IntoElement for SettingsGroup {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .py(px(16.0))
            .px(px(16.0))
            .rounded(px(12.0))
            .bg(self.colors.surface)
            .border_1()
            .border_color(self.colors.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(self.colors.foreground)
                            .child(self.title)
                    )
                    .when_some(self.description, |this, desc| {
                        this.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(self.colors.foreground_muted)
                                .child(desc)
                        )
                    })
            )
            .children(self.items.iter().map(|item| item.render(&self.colors)))
    }
}

pub struct SettingsPage {
    title: String,
    groups: Vec<SettingsGroup>,
    colors: ThemeColors,
}

impl SettingsPage {
    pub fn new(title: impl Into<String>, colors: ThemeColors) -> Self {
        Self { title: title.into(), groups: vec![], colors }
    }

    pub fn group(mut self, group: SettingsGroup) -> Self {
        self.groups.push(group);
        self
    }

    pub fn groups(mut self, groups: Vec<SettingsGroup>) -> Self {
        self.groups = groups;
        self
    }
}

impl IntoElement for SettingsPage {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .p(px(16.0))
            .w_full()
            .child(
                div()
                    .text_size(px(20.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(self.colors.foreground)
                    .pb(px(8.0))
                    .child(self.title)
            )
            .children(self.groups)
    }
}