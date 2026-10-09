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

    /// 丢弃当前会话未上屏的组合内容（启停输入法时调用）。
    pub fn clear_composition(&mut self) {
        if let Some(session) = self.session.as_ref() {
            unsafe {
                let api = librime::get_api();
                if let Some(clear) = (*api).clear_composition {
                    clear(session.session_id());
                }
            }
        }
    }

    pub fn get_current_schema(&self) -> Option<String> {
        if let Some(session) = self.session.as_ref() {
            session.status().ok().map(|s| s.schema_id().to_string())
        } else {
            None
        }
    }

    /// 可切换方案列表 (id, 显示名)（托盘菜单用）。
    ///
    /// id 来自 levers 的 switcher 列表；显示名读 `<id>.schema.yaml` 的
    /// `name:` 行（解析不到用 id）。levers 不碰会话，失败退化为空列表。
    pub fn available_schemas(&self) -> Vec<(String, String)> {
        let Ok(manager) = librime::SwitcherSettings::new() else {
            return Vec::new();
        };
        let Ok(ids) = manager.get_available_schema_list() else {
            return Vec::new();
        };
        let user_dir = get_config_dir();
        let (shared_dir, _) = get_data_dirs();
        ids.into_iter()
            .map(|id| {
                let name = [user_dir.as_path(), shared_dir.as_path()]
                    .iter()
                    .find_map(|dir| {
                        std::fs::read_to_string(dir.join(format!("{id}.schema.yaml"))).ok()
                    })
                    .and_then(|text| {
                        text.lines()
                            .filter_map(|line| {
                                let rest = line.trim().strip_prefix("name:")?;
                                let v = rest.trim().trim_matches('"').trim_matches('\'');
                                (!v.is_empty()).then_some(v.to_string())
                            })
                            .next()
                    })
                    .unwrap_or_else(|| id.clone());
                (id, name)
            })
            .collect()
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

    /// 关闭会话 → 执行 levers 词典操作（导出/导入要求 userdb 独占）→ 重建会话。
    ///
    /// 对齐 librime user_dict_manager 的 CAVEAT 与安卓/XimeYao 的
    /// withUserDictClosed：**正在输入的 composition 会丢**（设置页词典操作
    /// 与打字互斥的代价）。重建失败只记警告——后续按键无会话即无响应，
    /// 重启 daemon 可恢复（导出失败不至此，见 wayland 层错误路径）。
    pub fn with_user_dict_closed<T>(&mut self, op: impl FnOnce() -> T) -> T {
        if let Some(session) = self.session.take() {
            drop(session); // Drop → close（释放 userdb 的 LevelDB 锁）
            debug!("Session closed for user dict operation");
        }
        let out = op();
        self.session = librime::create_session().ok();
        if self.session.is_some() {
            debug!("Session recreated after user dict operation");
        } else {
            warn!("Failed to recreate session after user dict operation");
        }
        out
    }

    pub fn redeploy(&mut self) {
        let _ = self.redeploy_with_result();
    }

    /// 重新部署并返回结果（托盘「重新部署」后发桌面通知用）。
    pub fn redeploy_with_result(&mut self) -> librime::DeployResult {
        debug!("Redeploying Rime...");
        // 先丢弃旧会话再 finalize。不能拖到下面 create_session 之后：
        // Rust 赋值语义是先求值右值再 drop 旧值，旧句柄的 destroy_session
        // 会在 finalize 之后执行（librime API 上是未定义行为）；且 librime
        // 的 SessionId 就是 Session 对象的堆地址，部署期大量分配释放后新
        // 会话可能恰好复用旧地址——destroy_session(旧地址) 会抹掉新会话，
        // 输入法此后静默失灵。
        if let Some(session) = self.session.take() {
            drop(session);
        }
        librime::finalize();

        let (shared_data_dir, _) = get_data_dirs();
        let mut traits = Traits::new();
        traits.set_shared_data_dir(shared_data_dir.to_string_lossy().as_ref());
        traits.set_user_data_dir(&self.config_dir);
        traits.set_log_dir(&self.config_dir);

        librime::setup(&mut traits);
        let result = if let Err(e) = librime::initialize(&mut traits) {
            error!("Failed to reinitialize Rime: {}", e);
            librime::DeployResult::Failure
        } else {
            let result = librime::full_deploy_and_wait();
            match result {
                librime::DeployResult::Success => debug!("Rime redeployed successfully"),
                librime::DeployResult::Failure => warn!("Rime deploy failed"),
            }

            if librime::is_maintenance_mode() {
                librime::join_maintenance_thread();
            }

            self.session = librime::create_session().ok();
            debug!("New Rime session created after deployment");
            result
        };
        result
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
