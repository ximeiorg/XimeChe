use std::sync::Arc;
use axum::{
    extract::{State, Query, Json},
    http::header,
    middleware::Next,
};
use crate::types::*;
use crate::error::ApiError;
use crate::auth::{AuthState, compute_hash};
use crate::state::ServerState;

pub async fn clipboard_read(
    State(state): State<ServerState>,
    Query(query): Query<ClipboardQuery>,
) -> Result<Json<ClipboardReadResponse>, ApiError> {
    let clipboard_content = state.providers.clipboard.read()
        .map_err(|e| ApiError::ClipboardReadFailed(e.to_string()))?;
    
    let hash = compute_hash(&clipboard_content);
    
    if let Some(since_hash) = query.since_hash {
        if since_hash == hash {
            return Ok(Json(ClipboardReadResponse {
                content: String::new(),
                hash: hash.clone(),
            }));
        }
    }
    
    Ok(Json(ClipboardReadResponse {
        content: clipboard_content,
        hash,
    }))
}

pub async fn clipboard_write(
    State(state): State<ServerState>,
    Json(req): Json<ClipboardWriteRequest>,
) -> Result<Json<ClipboardWriteResponse>, ApiError> {
    let computed = compute_hash(&req.content);
    if computed != req.hash {
        return Err(ApiError::HashMismatch);
    }
    
    let current_content = state.providers.clipboard.read()
        .map_err(|e| ApiError::ClipboardReadFailed(e.to_string()))?;
    let current_hash = compute_hash(&current_content);
    
    if current_hash == req.hash {
        return Ok(Json(ClipboardWriteResponse {
            accepted: false,
            hash: current_hash,
        }));
    }
    
    state.providers.clipboard.write(&req.content)
        .map_err(|e| ApiError::ClipboardWriteFailed(e.to_string()))?;
    
    Ok(Json(ClipboardWriteResponse {
        accepted: true,
        hash: req.hash.clone(),
    }))
}

pub async fn auth_middleware(
    State(auth): State<Arc<AuthState>>,
    request: axum::extract::Request,
    next: Next,
) -> Result<axum::response::Response, ApiError> {
    let auth_header = request.headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or(ApiError::InvalidToken)?;
    
    if !auth_header.starts_with("Bearer ") {
        return Err(ApiError::InvalidToken);
    }
    
    let token = &auth_header[7..];
    let device_auth = auth.verify_token(token)?;
    
    if !device_auth.is_valid() {
        return Err(ApiError::TokenExpired);
    }
    
    Ok(next.run(request).await)
}