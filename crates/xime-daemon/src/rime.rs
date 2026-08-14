use librime::session::Session;
use librime::traits::Traits;
use tracing::{debug, error, warn};
use xime_config::get_data_dirs;

use crate::get_config_dir;

pub struct RimeEngine {
    session: Option<Session>,
    config_dir: String,
}

impl RimeEngine {
    pub fn new() -> Self {
        let config_dir = get_config_dir();
        if !config_dir.exists() {
            std::fs::create_dir_all(&config_dir).expect("Failed to create config directory");
            debug!("Created config directory: {}", config_dir.display());
        }
        let config_dir_str = config_dir.to_string_lossy().to_string();

        let (shared_data_dir, _) = get_data_dirs();
        let mut traits = Traits::new();
        traits.set_shared_data_dir(shared_data_dir.to_string_lossy().as_ref());
        traits.set_user_data_dir(&config_dir_str);
        traits.set_log_dir(&config_dir_str);

        librime::setup(&mut traits);
        if let Err(e) = librime::initialize(&mut traits) {
            error!("Failed to initialize Rime: {}", e);
            return Self {
                session: None,
                config_dir: config_dir_str,
            };
        }

        match librime::full_deploy_and_wait() {
            librime::DeployResult::Success => debug!("Rime deployed"),
            librime::DeployResult::Failure => warn!("Rime deploy failed"),
        }

        if librime::is_maintenance_mode() {
            librime::join_maintenance_thread();
        }

        let session = librime::create_session().ok();
        Self {
            session,
            config_dir: config_dir_str,
        }
    }

    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    pub fn session_mut(&mut self) -> Option<&mut Session> {
        self.session.as_mut()
    }

    pub fn toggle_ascii_mode(&mut self) -> bool {
        if let Some(session) = self.session.as_ref() {
            let current_ascii: bool = session.get_option("ascii_mode").unwrap_or(false);
            let new_ascii = !current_ascii;
            session.set_option("ascii_mode", new_ascii).ok();
            debug!("Set ascii_mode to {}", new_ascii);
            return new_ascii;
        }
        false
    }

    pub fn get_ascii_mode(&self) -> bool {
        if let Some(session) = self.session.as_ref() {
            session.get_option("ascii_mode").unwrap_or(false)
        } else {
            false
        }
    }

    pub fn get_current_schema(&self) -> Option<String> {
        if let Some(session) = self.session.as_ref() {
            session.status().ok().map(|s| s.schema_id().to_string())
        } else {
            None
        }
    }

    pub fn select_schema(&mut self, schema_id: &str) -> bool {
        if let Some(session) = self.session.as_ref() {
            match session.select_schema(schema_id) {
                Ok(_) => {
                    debug!("Selected schema: {}", schema_id);
                    true
                }
                Err(e) => {
                    error!("Failed to select schema {}: {}", schema_id, e);
                    false
                }
            }
        } else {
            false
        }
    }

    pub fn redeploy(&mut self) {
        debug!("Redeploying Rime...");
        librime::finalize();

        let (shared_data_dir, _) = get_data_dirs();
        let mut traits = Traits::new();
        traits.set_shared_data_dir(shared_data_dir.to_string_lossy().as_ref());
        traits.set_user_data_dir(&self.config_dir);
        traits.set_log_dir(&self.config_dir);

        librime::setup(&mut traits);
        if let Err(e) = librime::initialize(&mut traits) {
            error!("Failed to reinitialize Rime: {}", e);
        } else {
            match librime::full_deploy_and_wait() {
                librime::DeployResult::Success => debug!("Rime redeployed successfully"),
                librime::DeployResult::Failure => warn!("Rime deploy failed"),
            }

            if librime::is_maintenance_mode() {
                librime::join_maintenance_thread();
            }

            self.session = librime::create_session().ok();
            debug!("New Rime session created after deployment");
        }
    }
}

impl Default for RimeEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RimeEngine {
    fn drop(&mut self) {
        librime::finalize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // librime 是进程级全局库，setup/finalize 非线程安全，测试必须串行执行。
    static LIBRIME_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_deploy_contains_only_wubi_schemas() {
        let _guard = LIBRIME_TEST_LOCK.lock().unwrap();
        // 注入与 daemon 一致的 rime paths（统一 single dir，仅 rime-wubi）
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let rime_dir = std::path::PathBuf::from(&home).join(".config/xime/rime");
        assert!(
            !rime_dir.starts_with("/usr/share/rime-data"),
            "should not use system librime-data dir: {}",
            rime_dir.display()
        );
        let _ = xime_config::set_rime_paths(xime_config::RimePaths {
            shared_data_dir: rime_dir.clone(),
            user_data_dir: rime_dir,
        });

        let engine = RimeEngine::new();
        assert!(engine.session().is_some(), "Rime session should initialize");
        let build_dir = get_config_dir().join("build");
        let schemas: Vec<_> = std::fs::read_dir(&build_dir)
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .filter(|e| {
                        e.path()
                            .file_name()
                            .map(|f| f.to_string_lossy().ends_with(".schema.yaml"))
                            .unwrap_or(false)
                    })
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        println!("deployed schemas: {:?}", schemas);
        // 不应包含系统内置方案 stroke
        assert!(
            !schemas.iter().any(|s| s.contains("stroke")),
            "system librime-data schema stroke should not be deployed: {:?}",
            schemas
        );
    }

    /// Enter 在「有组合输入 / 无组合输入」下的按下/释放处理结果。
    #[test]
    fn test_enter_key_handling() {
        let _guard = LIBRIME_TEST_LOCK.lock().unwrap();
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let rime_dir = std::path::PathBuf::from(&home).join(".config/xime/rime");
        let _ = xime_config::set_rime_paths(xime_config::RimePaths {
            shared_data_dir: rime_dir.clone(),
            user_data_dir: rime_dir,
        });

        let engine = RimeEngine::new();
        let session = engine.session().expect("session");
        let enter: i32 = 0xFF0D;
        let release: i32 = 0x8000_0000u32 as i32;

        // 1) 无组合输入：直接按回车
        let result = session.process_key(enter, 0);
        let commit = session.commit().map(|c| c.text().to_string());
        println!(
            "[empty] Enter-press: result={}, commit={:?}",
            result, commit
        );
        assert!(!result, "空输入时 Enter 不应被 Rime 拦截");
        assert!(commit.is_none());

        // 2) 输入拼音/编码后按回车（模拟终端里打了字再回车）
        for ch in "ls".chars() {
            session.process_key(ch as i32, 0);
        }
        let result = session.process_key(enter, 0);
        let commit = session.commit().map(|c| c.text().to_string());
        println!(
            "[composing] Enter-press: result={}, commit={:?}",
            result, commit
        );

        // 3) 同一 Enter 的释放事件
        let result = session.process_key(enter, release);
        let commit = session.commit().map(|c| c.text().to_string());
        println!(
            "[composing] Enter-release: result={}, commit={:?}",
            result, commit
        );

        // 4) 组合已提交后再次回车
        let result = session.process_key(enter, 0);
        println!("[after-commit] Enter-press: result={}", result);
    }
}
