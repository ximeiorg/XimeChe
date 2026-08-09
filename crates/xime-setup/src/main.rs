use std::fs::File;
use std::path::PathBuf;
use xime_setup_lib::{set_notify_deploy, set_notify_reload_style, set_notify_select_schema};

/// rime-wubi 方案数据目录（shared_data_dir）。
/// 仅使用项目自带的 rime-wubi 方案，不使用系统 librime-data 内置方案。
fn shared_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let candidates = [
        // 开发安装：~/.local/share/xime/rime-data
        PathBuf::from(&home).join(".local/share/xime/rime-data"),
        // 系统安装：/usr/share/xime/rime-data
        PathBuf::from("/usr/share/xime/rime-data"),
    ];
    for dir in candidates.iter() {
        if dir.join("default.yaml").exists() {
            return dir.clone();
        }
    }
    candidates[0].clone()
}

/// 用户数据目录（user_data_dir）。
fn user_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    PathBuf::from(home).join(".config/xime/rime")
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

fn main() -> iced::Result {
    if !try_acquire_singleton_lock() {
        tracing::info!("xime-setup is already running, exiting...");
        return Ok(());
    }

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
    set_notify_select_schema(|schema_id| {
        let Ok(conn) = zbus::blocking::Connection::session() else {
            return false;
        };
        let Ok(reply) = conn.call_method(
            Some("org.xime.Xime"),
            "/org/xime/Xime",
            Some("org.xime.Xime.Controller"),
            "SelectSchema",
            &(schema_id,),
        ) else {
            return false;
        };
        reply.body().deserialize::<bool>().unwrap_or(false)
    });

    let _ = xime_setup_lib::set_rime_paths(xime_setup_lib::RimePaths {
        shared_data_dir: shared_data_dir(),
        user_data_dir: user_data_dir(),
    });

    xime_setup_lib::run()
}
