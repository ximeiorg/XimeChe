use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::error::ApiError;

type HmacSha256 = Hmac<Sha256>;

const TOKEN_VALIDITY_DAYS: i64 = 7;

#[derive(Debug)]
pub struct AuthConfig {
    secret: Vec<u8>,
}

impl AuthConfig {
    pub fn new() -> Self {
        let secret = uuid::Uuid::new_v4().as_bytes().to_vec();
        Self { secret }
    }

    pub fn from_secret(secret: Vec<u8>) -> Self {
        Self { secret }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct AuthToken {
    #[allow(dead_code)]
    device_id: String,
    token: String,
    expires_at: DateTime<Utc>,
}

impl AuthToken {
    pub fn generate(device_id: &str, config: &AuthConfig) -> Self {
        let now = Utc::now();
        let expires_at = now + Duration::days(TOKEN_VALIDITY_DAYS);

        let mut mac =
            HmacSha256::new_from_slice(&config.secret).expect("HMAC can take key of any size");

        let payload = format!("{}:{}", device_id, expires_at.timestamp());
        mac.update(payload.as_bytes());

        let result = mac.finalize();
        let signature =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(result.into_bytes());

        let token = format!(
            "{}.{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(device_id),
            signature
        );

        Self {
            device_id: device_id.to_string(),
            token,
            expires_at,
        }
    }

    pub fn verify(token_str: &str, config: &AuthConfig) -> Result<DeviceAuth, ApiError> {
        let parts: Vec<&str> = token_str.split('.').collect();
        if parts.len() != 2 {
            return Err(ApiError::InvalidToken);
        }

        let device_id_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[0])
            .map_err(|_| ApiError::InvalidToken)?;
        let device_id = String::from_utf8(device_id_bytes).map_err(|_| ApiError::InvalidToken)?;

        let signature_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|_| ApiError::InvalidToken)?;

        let mut mac =
            HmacSha256::new_from_slice(&config.secret).expect("HMAC can take key of any size");

        let expires_at = Utc::now() + Duration::days(TOKEN_VALIDITY_DAYS);
        let payload = format!("{}:{}", device_id, expires_at.timestamp());
        mac.update(payload.as_bytes());

        mac.verify_slice(&signature_bytes)
            .map_err(|_| ApiError::InvalidToken)?;

        Ok(DeviceAuth {
            device_id,
            expires_at,
        })
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    pub fn expires_in_seconds(&self) -> u64 {
        let now = Utc::now();
        if self.expires_at > now {
            (self.expires_at - now).num_seconds() as u64
        } else {
            0
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeviceAuth {
    device_id: String,
    expires_at: DateTime<Utc>,
}

impl DeviceAuth {
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn is_valid(&self) -> bool {
        self.expires_at > Utc::now()
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
}

pub fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(result)
}

#[derive(Debug, Clone)]
pub struct AuthState {
    config: Arc<AuthConfig>,
}

impl AuthState {
    pub fn new() -> Self {
        Self {
            config: Arc::new(AuthConfig::new()),
        }
    }

    pub fn with_config(config: AuthConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
}

impl Default for AuthState {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthState {
    pub fn generate_token(&self, device_id: &str) -> AuthToken {
        AuthToken::generate(device_id, &self.config)
    }

    pub fn verify_token(&self, token: &str) -> Result<DeviceAuth, ApiError> {
        AuthToken::verify(token, &self.config)
    }
}
