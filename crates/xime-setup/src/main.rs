mod components;
mod pages;
mod rime_config;
mod state;
mod theme;

use gpui::*;
use pages::SettingsApp;
use rust_embed::RustEmbed;
use std::borrow::Cow;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/assets"]
#[include = "image/*.png"]
#[include = "icons/*.svg"]
struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        Assets::get(path)
            .map(|x| Some(x.data))
            .ok_or_else(|| anyhow::anyhow!("Asset not found"))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Assets::iter()
            .filter_map(|p| {
                if p.starts_with(path) {
                    Some(p.into())
                } else {
                    None
                }
            })
            .collect())
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::centered(
                    size(px(800.0), px(640.0)),
                    cx,
                )),
                titlebar: Some(TitlebarOptions {
                    title: Some("Xime 设置".into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| SettingsApp::new(cx)),
        );
    });
}