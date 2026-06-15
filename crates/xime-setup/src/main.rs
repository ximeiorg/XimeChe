use gpui::*;
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::fs::File;
use std::path::PathBuf;
use xime_setup_lib::{set_notify_deploy, set_notify_reload_style, SettingsApp};

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/assets"]
#[include = "icons/*.svg"]
struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        Ok(Assets::get(path).map(|x| x.data))
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

    match File::create(&lock_path) {
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
        tracing::info!("xime-setup is already running, exiting...");
        return;
    }

    // Inject IPC notify callbacks (Linux: DBus to xime-daemon)
    set_notify_deploy(|| {
        if let Ok(conn) = zbus::blocking::Connection::session() {
            let _ = conn.call_method(
                Some("org.xime.Xime"),
                "/org/xime/Xime",
                Some("org.xime.Xime.Controller"),
                "Deploy",
                &(),
            );
        }
    });
    set_notify_reload_style(|| {
        if let Ok(conn) = zbus::blocking::Connection::session() {
            let _ = conn.call_method(
                Some("org.xime.Xime"),
                "/org/xime/Xime",
                Some("org.xime.Xime.Controller"),
                "ReloadStyle",
                &(),
            );
        }
    });

    Application::new().run(|cx: &mut App| {
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
