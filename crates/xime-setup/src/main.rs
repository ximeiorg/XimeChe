mod components;
mod pages;
mod state;
mod theme;
mod webdav;

use gpui::*;
use pages::SettingsApp;
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::fs::File;
use std::path::PathBuf;
use tracing::info;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/assets"]
#[include = "image/*.png"]
#[include = "icons/*.svg"]
#[allow(dead_code)]
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

fn get_lock_file_path() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            std::env::var("HOME")
                .map(|home| PathBuf::from(home).join(".local/share/xime"))
                .ok()
        })
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("xime-setup.lock")
}

fn try_acquire_singleton_lock() -> bool {
    let lock_path = get_lock_file_path();

    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let file = File::create(&lock_path);

    match file {
        Ok(f) => {
            use nix::fcntl::{Flock, FlockArg};
            use std::mem::forget;

            match Flock::lock(f, FlockArg::LockExclusiveNonblock) {
                Ok(flock) => {
                    forget(flock);
                    true
                }
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

fn main() {
    if !try_acquire_singleton_lock() {
        info!("xime-setup is already running, exiting...");
        return;
    }

    Application::new().run(|cx: &mut App| {
        components::text_input::register_key_bindings(cx);

        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(800.0), px(640.0)), cx)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Xime 设置".into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            },
            |_window, cx| cx.new(SettingsApp::new),
        );
    });
}
