use gpui::*;

#[derive(Clone)]
pub enum SystemTheme {
    Light,
    Dark,
}

impl SystemTheme {
    pub fn detect() -> Self {
        #[cfg(target_os = "linux")]
        {
            if let Ok(output) = std::process::Command::new("gsettings")
                .args(["get", "org.gnome.desktop.interface", "color-scheme"])
                .output()
            {
                let value = String::from_utf8_lossy(&output.stdout);
                if value.contains("'prefer-dark'") || value.contains("'dark'") {
                    return SystemTheme::Dark;
                }
            }

            if let Ok(output) = std::process::Command::new("gsettings")
                .args(["get", "org.gnome.desktop.interface", "gtk-theme"])
                .output()
            {
                let value = String::from_utf8_lossy(&output.stdout);
                if value.contains("dark") || value.contains("Dark") {
                    return SystemTheme::Dark;
                }
            }

            if std::env::var("GTK_THEME")
                .map(|t| t.contains("dark") || t.contains("Dark"))
                .unwrap_or(false)
            {
                return SystemTheme::Dark;
            }

            if let Ok(style) = std::env::var("COLOR_SCHEME") {
                if style == "dark" || style == "Dark" {
                    return SystemTheme::Dark;
                }
            }
        }

        SystemTheme::Light
    }

    pub fn is_dark(&self) -> bool {
        matches!(self, SystemTheme::Dark)
    }
}

#[derive(Clone)]
pub struct ThemeColors {
    pub background: Hsla,
    pub surface: Hsla,
    pub surface_variant: Hsla,
    pub primary: Hsla,
    pub primary_hover: Hsla,
    pub on_primary: Hsla,
    pub foreground: Hsla,
    pub foreground_muted: Hsla,
    pub border: Hsla,
    pub border_variant: Hsla,
    pub disabled: Hsla,
    pub error: Hsla,
    pub on_error: Hsla,
    pub selection: Hsla,
}

impl ThemeColors {
    pub fn from_theme(theme: &SystemTheme, primary_color: u32) -> Self {
        let (r, g, b) = ((primary_color >> 16) as u8, (primary_color >> 8) as u8, primary_color as u8);
        let hover_r = (r as f32 * 0.9) as u8;
        let hover_g = (g as f32 * 0.9) as u8;
        let hover_b = (b as f32 * 0.9) as u8;
        let primary_hover = ((hover_r as u32) << 16) | ((hover_g as u32) << 8) | hover_b as u32;
        
        if theme.is_dark() {
            Self {
                background: hsla(0.0, 0.0, 0.05, 0.85),
                surface: rgb(0x1a1a1a).into(),
                surface_variant: rgb(0x262626).into(),
                primary: rgb(primary_color).into(),
                primary_hover: rgb(primary_hover).into(),
                on_primary: rgb(0xffffff).into(),
                foreground: rgb(0xe0e0e0).into(),
                foreground_muted: rgb(0x808080).into(),
                border: rgb(0x303030).into(),
                border_variant: rgb(0x404040).into(),
                disabled: rgb(0x4d4d4d).into(),
                error: rgb(0xc42b1c).into(),
                on_error: rgb(0xffffff).into(),
                selection: rgb(0x3a2a5a).into(),
            }
        } else {
            Self {
                background: hsla(0.0, 0.0, 1.0, 0.95),
                surface: rgb(0xffffff).into(),
                surface_variant: rgb(0xf5f5f5).into(),
                primary: rgb(primary_color).into(),
                primary_hover: rgb(primary_hover).into(),
                on_primary: rgb(0xffffff).into(),
                foreground: rgb(0x1a1a1a).into(),
                foreground_muted: rgb(0x666666).into(),
                border: rgb(0xe0e0e0).into(),
                border_variant: rgb(0xd0d0d0).into(),
                disabled: rgb(0xaaaaaa).into(),
                error: rgb(0xc42b1c).into(),
                on_error: rgb(0xffffff).into(),
                selection: rgb(0xe8e0f8).into(),
            }
        }
    }
}