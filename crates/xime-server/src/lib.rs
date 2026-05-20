mod auth;
mod error;
mod handlers;
mod pair_store;
mod platform;
mod router;
mod state;
mod types;

pub use auth::{compute_hash, AuthConfig, AuthState, AuthToken, DeviceAuth};
pub use error::ApiError;
pub use pair_store::{PairSession, PairStore, PairedDevice};
pub use platform::{
    ClipboardProvider, ConfigDirProvider, DefaultConfigDir, InMemoryClipboard, PlatformProviders,
};
pub use router::{create_router, serve};
pub use state::{ClipboardState, ServerState};
pub use types::*;
