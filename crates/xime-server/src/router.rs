use axum::middleware;
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::auth::AuthState;
use crate::handlers::{
    auth_middleware, clipboard_read, clipboard_write, health_check, pair_confirm, pair_list,
    pair_remove, pair_request, pair_status,
};
use crate::state::ServerState;

pub fn create_router(state: ServerState) -> Router {
    let auth = state.auth.clone();
    Router::new()
        .nest("/pair", pair_routes())
        .nest("/clipboard", clipboard_routes(auth))
        .route("/health", get(health_check))
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state)
}

fn pair_routes() -> Router<ServerState> {
    Router::new()
        .route("/request", post(pair_request))
        .route("/status", get(pair_status))
        .route("/confirm", post(pair_confirm))
        .route("/list", get(pair_list))
        .route("/remove/{device_id}", post(pair_remove))
}

fn clipboard_routes(auth: Arc<AuthState>) -> Router<ServerState> {
    Router::new()
        .route("/read", get(clipboard_read))
        .route("/write", post(clipboard_write))
        .layer(middleware::from_fn_with_state(auth, auth_middleware))
}

pub async fn serve(state: ServerState, port: u16) -> Result<(), anyhow::Error> {
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("HTTP server listening on {}", addr);

    axum::serve(listener, create_router(state)).await?;

    Ok(())
}
