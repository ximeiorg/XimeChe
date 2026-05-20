use crate::auth::{compute_hash, AuthConfig, AuthState};
use crate::pair_store::PairStore;
use crate::platform::PlatformProviders;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ServerState {
    pub auth: Arc<AuthState>,
    pub pair_store: Arc<std::sync::Mutex<PairStore>>,
    pub clipboard: Arc<std::sync::Mutex<ClipboardState>>,
    pub providers: PlatformProviders,
}

#[derive(Debug, Clone)]
pub struct ClipboardState {
    pub content: String,
    pub hash: String,
}

impl Default for ClipboardState {
    fn default() -> Self {
        Self {
            content: String::new(),
            hash: compute_hash(&String::new()),
        }
    }
}

impl ServerState {
    pub fn new(providers: PlatformProviders) -> Self {
        let pair_store = PairStore::load_from(providers.config_dir.config_dir());
        Self {
            auth: Arc::new(AuthState::new()),
            pair_store: Arc::new(std::sync::Mutex::new(pair_store)),
            clipboard: Arc::new(std::sync::Mutex::new(ClipboardState::default())),
            providers,
        }
    }

    pub fn with_auth_secret(providers: PlatformProviders, secret: Vec<u8>) -> Self {
        use crate::auth::AuthConfig;
        let pair_store = PairStore::load_from(providers.config_dir.config_dir());
        Self {
            auth: Arc::new(AuthState::with_config(AuthConfig::from_secret(secret))),
            pair_store: Arc::new(std::sync::Mutex::new(pair_store)),
            clipboard: Arc::new(std::sync::Mutex::new(ClipboardState::default())),
            providers,
        }
    }
}
