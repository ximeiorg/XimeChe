use crate::error::ApiError;
use crate::types::PairStatus;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

const PAIR_CODE_VALIDITY_MINUTES: i64 = 10;
const PAIRS_FILE: &str = "pairs.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device_id: String,
    pub device_name: String,
    pub token: String,
    pub paired_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub auto_approve: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairSession {
    pub code: String,
    pub device_id: String,
    pub device_name: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub status: PairStatus,
}

impl PairSession {
    pub fn new(device_id: String, device_name: String) -> Self {
        let now = Utc::now();
        let code = generate_pair_code();
        Self {
            code,
            device_id,
            device_name,
            created_at: now,
            expires_at: now + Duration::minutes(PAIR_CODE_VALIDITY_MINUTES),
            status: PairStatus::Pending,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at < Utc::now()
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

fn generate_pair_code() -> String {
    let code = Uuid::new_v4();
    let bytes = code.as_bytes();
    let num = ((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | (bytes[2] as u32);
    format!("{:06}", num % 1_000_000)
}

#[derive(Debug)]
pub struct PairStore {
    config_dir: PathBuf,
    paired_devices: HashMap<String, PairedDevice>,
    pub pending_sessions: HashMap<String, PairSession>,
}

impl PairStore {
    pub fn load_from(config_dir: PathBuf) -> Self {
        let path = config_dir.join(PAIRS_FILE);
        let paired_devices = if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(devices) = serde_json::from_str::<Vec<PairedDevice>>(&content) {
                    devices
                        .into_iter()
                        .map(|d| (d.device_id.clone(), d))
                        .collect()
                } else {
                    HashMap::new()
                }
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        Self {
            config_dir,
            paired_devices,
            pending_sessions: HashMap::new(),
        }
    }

    pub fn save(&self) -> Result<(), ApiError> {
        let path = self.config_dir.join(PAIRS_FILE);
        if !self.config_dir.exists() {
            fs::create_dir_all(&self.config_dir)?;
        }

        let devices: Vec<&PairedDevice> = self.paired_devices.values().collect();
        let content = serde_json::to_string_pretty(&devices)?;
        fs::write(&path, content)?;
        Ok(())
    }

    pub fn create_session(&mut self, device_id: String, device_name: String) -> PairSession {
        let session = PairSession::new(device_id, device_name);
        self.pending_sessions
            .insert(session.code.clone(), session.clone());
        session
    }

    pub fn get_session(&self, code: &str) -> Option<&PairSession> {
        self.pending_sessions.get(code)
    }

    pub fn get_session_mut(&mut self, code: &str) -> Option<&mut PairSession> {
        self.pending_sessions.get_mut(code)
    }

    pub fn confirm_session(&mut self, code: &str, token: String) -> Result<PairedDevice, ApiError> {
        let session = self
            .pending_sessions
            .get_mut(code)
            .ok_or(ApiError::PairCodeNotFound)?;

        if session.is_expired() {
            return Err(ApiError::PairCodeExpired);
        }

        if session.status != PairStatus::Pending {
            return Err(ApiError::PairAlreadyConfirmed);
        }

        session.status = PairStatus::Confirmed;

        let device = PairedDevice {
            device_id: session.device_id.clone(),
            device_name: session.device_name.clone(),
            token,
            paired_at: Utc::now(),
            last_seen: Utc::now(),
            auto_approve: false,
        };

        self.paired_devices
            .insert(device.device_id.clone(), device.clone());
        self.pending_sessions.remove(code);
        self.save()?;

        Ok(device)
    }

    pub fn reject_session(&mut self, code: &str) -> Result<(), ApiError> {
        let session = self
            .pending_sessions
            .get_mut(code)
            .ok_or(ApiError::PairCodeNotFound)?;

        session.status = PairStatus::Rejected;
        self.pending_sessions.remove(code);
        Ok(())
    }

    pub fn get_device(&self, device_id: &str) -> Option<&PairedDevice> {
        self.paired_devices.get(device_id)
    }

    pub fn get_device_by_token(&self, token: &str) -> Option<&PairedDevice> {
        self.paired_devices.values().find(|d| d.token == token)
    }

    pub fn update_last_seen(&mut self, device_id: &str) {
        if let Some(device) = self.paired_devices.get_mut(device_id) {
            device.last_seen = Utc::now();
            self.save().ok();
        }
    }

    pub fn list_devices(&self) -> Vec<&PairedDevice> {
        self.paired_devices.values().collect()
    }

    pub fn remove_device(&mut self, device_id: &str) -> Result<(), ApiError> {
        self.paired_devices.remove(device_id);
        self.save()?;
        Ok(())
    }

    pub fn is_device_paired(&self, device_id: &str) -> bool {
        self.paired_devices.contains_key(device_id)
    }
}
