use axum::{
    extract::{State, Query, Json, Path},
    response::IntoResponse,
};
use crate::types::*;
use crate::error::ApiError;
use crate::state::ServerState;

pub async fn pair_request(
    State(state): State<ServerState>,
    Json(req): Json<PairRequest>,
) -> Result<Json<PairRequestResponse>, ApiError> {
    if req.device_id.is_empty() || req.device_name.is_empty() {
        return Err(ApiError::InvalidRequest("device_id and device_name required".into()));
    }
    
    let mut store = state.pair_store.lock().unwrap();
    
    if store.is_device_paired(&req.device_id) {
        return Err(ApiError::PairAlreadyConfirmed);
    }
    
    let session = store.create_session(req.device_id, req.device_name);
    let code = session.code.clone();
    let expires_in = session.expires_in_seconds();
    
    Ok(Json(PairRequestResponse {
        code,
        expires_in,
    }))
}

pub async fn pair_status(
    State(state): State<ServerState>,
    Query(query): Query<PairStatusQuery>,
) -> Result<Json<PairStatusResponse>, ApiError> {
    let mut store = state.pair_store.lock().unwrap();
    
    let session = store.get_session(&query.code)
        .ok_or(ApiError::PairCodeNotFound)?;
    
    if session.is_expired() {
        store.pending_sessions.remove(&query.code);
        return Err(ApiError::PairCodeExpired);
    }
    
    let status = session.status;
    let expires_in_seconds = session.expires_in_seconds();
    
    let token = if status == PairStatus::Confirmed {
        let auth_token = state.auth.generate_token(&session.device_id);
        store.confirm_session(&query.code, auth_token.token().to_string())?;
        Some(auth_token.token().to_string())
    } else {
        None
    };
    
    Ok(Json(PairStatusResponse {
        status,
        token,
        expires_in: if status == PairStatus::Confirmed {
            Some(7 * 24 * 3600)
        } else {
            Some(expires_in_seconds)
        },
    }))
}

pub async fn pair_confirm(
    State(state): State<ServerState>,
    Json(req): Json<PairConfirmRequest>,
) -> Result<Json<PairConfirmResponse>, ApiError> {
    let mut store = state.pair_store.lock().unwrap();
    
    let session = store.get_session(&req.code)
        .ok_or(ApiError::PairCodeNotFound)?;
    
    if session.is_expired() {
        return Err(ApiError::PairCodeExpired);
    }
    
    if req.approve {
        let auth_token = state.auth.generate_token(&session.device_id);
        store.confirm_session(&req.code, auth_token.token().to_string())?;
    } else {
        store.reject_session(&req.code)?;
    }
    
    Ok(Json(PairConfirmResponse { success: true }))
}

pub async fn pair_list(
    State(state): State<ServerState>,
) -> Json<DeviceListResponse> {
    let store = state.pair_store.lock().unwrap();
    let devices: Vec<DeviceInfo> = store.list_devices()
        .into_iter()
        .map(|d| DeviceInfo {
            device_id: d.device_id.clone(),
            device_name: d.device_name.clone(),
            paired_at: d.paired_at,
            last_seen: d.last_seen,
        })
        .collect();
    
    Json(DeviceListResponse { devices })
}

pub async fn pair_remove(
    State(state): State<ServerState>,
    Path(device_id): Path<String>,
) -> impl IntoResponse {
    let mut store = state.pair_store.lock().unwrap();
    match store.remove_device(&device_id) {
        Ok(_) => Json(serde_json::json!({ "removed": true })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}