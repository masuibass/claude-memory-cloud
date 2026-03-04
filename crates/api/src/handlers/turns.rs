use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;

use super::extract_account_id_from_headers;
use claude_memory_common::embedding::{build_search_text, embed_text};
use claude_memory_common::models::TurnInput;

use crate::AppState;

pub async fn batch_upsert_turns(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(inputs): Json<Vec<TurnInput>>,
) -> impl IntoResponse {
    let account_id = match extract_account_id_from_headers(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "missing auth").into_response(),
    };

    let mut ids = Vec::new();
    for input in &inputs {
        match do_upsert(&state, &account_id, input).await {
            Ok(id) => ids.push(id),
            Err(e) => {
                tracing::error!("batch upsert error: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        }
    }

    (StatusCode::OK, Json(serde_json::json!({"ids": ids}))).into_response()
}

async fn do_upsert(
    state: &AppState,
    account_id: &str,
    input: &TurnInput,
) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
    // Ensure session exists
    sqlx::query(
        "INSERT INTO sessions (id, account_id, project) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (account_id, id) DO NOTHING",
    )
    .bind(&input.session_id)
    .bind(account_id)
    .bind(&input.project)
    .execute(&state.pool)
    .await?;

    // Build embedding
    let search_text = build_search_text(
        input.user_input.as_deref(),
        input.tools_used.as_deref(),
        input.ai_response.as_deref(),
    );

    let embedding = if !search_text.is_empty() {
        Some(embed_text(&state.bedrock, &search_text).await?)
    } else {
        None
    };

    // Upsert turn
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO turns (account_id, session_id, project, turn_number, user_input, tools_used, ai_response, embedding) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (account_id, session_id, turn_number) DO UPDATE SET \
           user_input = EXCLUDED.user_input, \
           tools_used = EXCLUDED.tools_used, \
           ai_response = EXCLUDED.ai_response, \
           embedding = EXCLUDED.embedding \
         RETURNING id",
    )
    .bind(account_id)
    .bind(&input.session_id)
    .bind(&input.project)
    .bind(input.turn_number)
    .bind(&input.user_input)
    .bind(&input.tools_used)
    .bind(&input.ai_response)
    .bind(embedding.as_ref())
    .fetch_one(&state.pool)
    .await?;

    Ok(row.0)
}

pub async fn delete_session_turns(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let account_id = match extract_account_id_from_headers(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "missing auth").into_response(),
    };

    let result = sqlx::query("DELETE FROM turns WHERE account_id = $1 AND session_id = $2")
        .bind(&account_id)
        .bind(&session_id)
        .execute(&state.pool)
        .await;

    match result {
        Ok(r) => (
            StatusCode::OK,
            Json(serde_json::json!({"deleted": r.rows_affected()})),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("delete_session_turns error: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
