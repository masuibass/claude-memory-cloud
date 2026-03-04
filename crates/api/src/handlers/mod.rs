mod sessions;
mod transcripts;
mod turns;

use std::sync::Arc;

use axum::Router;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use claude_memory_common::auth::extract_account_id;

use crate::AppState;

pub(crate) fn extract_account_id_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(extract_account_id)
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/sessions", get(sessions::list_sessions))
        .route(
            "/api/sessions/{sid}/turns",
            get(sessions::get_session_turns).delete(turns::delete_session_turns),
        )
        .route("/api/turns/batch", post(turns::batch_upsert_turns))
        .route("/api/transcripts", post(transcripts::upload_transcript))
        .route("/api/health", get(health))
}

async fn health() -> &'static str {
    "ok"
}
