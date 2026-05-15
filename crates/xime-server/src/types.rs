use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct PairRequest {
    pub device_name: String,
    pub device_id: String,
}

#[derive(Debug, Serialize)]
pub struct PairRequestResponse {
    pub code: String,
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
pub struct PairStatusQuery {
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct PairStatusResponse {
    pub status: PairStatus,
    pub token: Option<String>,
    pub expires_in: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum PairStatus {
    Pending,
    Confirmed,
    Expired,
    Rejected,
}

#[derive(Debug, Deserialize)]
pub struct PairConfirmRequest {
    pub code: String,
    pub approve: bool,
}

#[derive(Debug, Serialize)]
pub struct PairConfirmResponse {
    pub success: bool,
}

#[derive(Debug, Serialize)]
pub struct ClipboardReadResponse {
    pub content: String,
    pub hash: String,
}

#[derive(Debug, Deserialize)]
pub struct ClipboardWriteRequest {
    pub content: String,
    pub hash: String,
}

#[derive(Debug, Serialize)]
pub struct ClipboardWriteResponse {
    pub accepted: bool,
    pub hash: String,
}

#[derive(Debug, Serialize)]
pub struct DeviceListResponse {
    pub devices: Vec<DeviceInfo>,
}

#[derive(Debug, Serialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub device_name: String,
    pub paired_at: chrono::DateTime<chrono::Utc>,
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: u16,
}

#[derive(Debug, Deserialize)]
pub struct ClipboardQuery {
    pub since_hash: Option<String>,
}