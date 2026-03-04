use std::sync::Arc;

use aws_sdk_s3::presigning::PresigningConfig;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;

use super::extract_account_id_from_headers;
use claude_memory_common::models::{PresignedUrlResponse, TranscriptUploadRequest};

use crate::AppState;

/// S3 presigned URL を生成して返す (PUT 用)
pub async fn upload_transcript(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<TranscriptUploadRequest>,
) -> impl IntoResponse {
    let account_id = match extract_account_id_from_headers(&headers) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "missing auth").into_response(),
    };

    let s3_key = format!("{account_id}/{}.jsonl", input.session_id);

    let presigned = match state
        .s3
        .put_object()
        .bucket(&state.transcript_bucket)
        .key(&s3_key)
        .content_type("application/x-ndjson")
        .presigned(
            PresigningConfig::expires_in(std::time::Duration::from_secs(300))
                .expect("valid presigning config"),
        )
        .await
    {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("presign error: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };

    // Record in DB
    if let Err(e) = sqlx::query(
        "INSERT INTO transcripts (account_id, session_id, project, s3_key, size_bytes) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (account_id, session_id) DO UPDATE SET \
           s3_key = EXCLUDED.s3_key, \
           size_bytes = EXCLUDED.size_bytes, \
           uploaded_at = NOW()",
    )
    .bind(&account_id)
    .bind(&input.session_id)
    .bind(&input.project)
    .bind(&s3_key)
    .bind(input.size_bytes)
    .execute(&state.pool)
    .await
    {
        tracing::error!("transcript db error: {e}");
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    (
        StatusCode::OK,
        Json(PresignedUrlResponse {
            url: presigned.uri().to_string(),
            s3_key,
        }),
    )
        .into_response()
}
