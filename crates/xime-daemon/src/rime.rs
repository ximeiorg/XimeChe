use librime::session::Session;
use librime::traits::Traits;
use tracing::{debug, error, warn};

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

        let mut traits = Traits::new();
        traits.set_shared_data_dir("/usr/share/rime-data");
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

    pub fn redeploy(&mut self) {
        debug!("Redeploying Rime...");
        librime::finalize();

        let mut traits = Traits::new();
        traits.set_shared_data_dir("/usr/share/rime-data");
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
