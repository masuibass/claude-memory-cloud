use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::extract_account_id_from_headers;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    pub project: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: Option<i64>,
}

fn default_limit() -> Option<i64> {
    Some(10)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct SessionRow {
    id: String,
    project: String,
    started_at: Option<DateTime<Utc>>,
    turn_count: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct TurnRow {
    turn_number: Option<i32>,
    user_input: Option<String>,
    tools_used: Option<String>,
    ai_response: Option<String>,
    created_at: Option<DateTime<Utc>>,
}

pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListSessionsQuery>,
) -> impl IntoResponse {
    let account_id = match extract_account_id_from_headers(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "missing auth").into_response(),
    };

    let limit = query.limit.unwrap_or(10).min(50);

    let results: Result<Vec<SessionRow>, _> = sqlx::query_as(
        r#"
        SELECT s.id, s.project, s.started_at,
               (SELECT COUNT(*) FROM turns t WHERE t.account_id = s.account_id AND t.session_id = s.id) AS turn_count
        FROM sessions s
        WHERE s.account_id = $1
          AND ($3::text IS NULL OR s.project = $3)
        ORDER BY s.started_at DESC
        LIMIT $2
        "#,
    )
    .bind(&account_id)
    .bind(limit)
    .bind(&query.project)
    .fetch_all(&state.pool)
    .await;

    match results {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => {
            tracing::error!("list_sessions error: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

pub async fn get_session_turns(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let account_id = match extract_account_id_from_headers(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "missing auth").into_response(),
    };

    let results: Result<Vec<TurnRow>, _> = sqlx::query_as(
        r#"
        SELECT turn_number, user_input, tools_used, ai_response, created_at
        FROM turns
        WHERE account_id = $1 AND session_id = $2
        ORDER BY turn_number ASC
        "#,
    )
    .bind(&account_id)
    .bind(&session_id)
    .fetch_all(&state.pool)
    .await;

    match results {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => {
            tracing::error!("get_session_turns error: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
