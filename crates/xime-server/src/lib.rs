mod types;
mod error;
mod auth;
mod pair_store;
mod platform;
mod state;
mod router;
mod handlers;

pub use types::*;
pub use error::ApiError;
pub use auth::{AuthToken, DeviceAuth, compute_hash, AuthState, AuthConfig};
pub use pair_store::{PairStore, PairedDevice, PairSession};
pub use platform::{ClipboardProvider, ConfigDirProvider, PlatformProviders, InMemoryClipboard, DefaultConfigDir};
pub use state::{ServerState, ClipboardState};
pub use router::{create_router, serve};