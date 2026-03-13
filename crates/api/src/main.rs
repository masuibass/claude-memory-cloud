use std::env;
use std::sync::Arc;

use axum::extract::{FromRequest, Path, Query, Request, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use lambda_http::request::RequestContext;
use lambda_http::{run, Error, RequestExt};
use serde::{Deserialize, Serialize};
use serde_json::json;

struct AppState {
    s3: aws_sdk_s3::Client,
    kb: aws_sdk_bedrockagentruntime::Client,
    transcript_bucket: String,
    kb_id: String,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .without_time()
        .with_ansi(false)
        .init();

    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let state = Arc::new(AppState {
        s3: aws_sdk_s3::Client::new(&config),
        kb: aws_sdk_bedrockagentruntime::Client::new(&config),
        transcript_bucket: env::var("TRANSCRIPT_BUCKET").unwrap_or_default(),
        kb_id: env::var("KB_ID").unwrap_or_default(),
    });

    let app = Router::new()
        .route("/config", get(get_config))
        .route("/transcript", post(post_transcript))
        .route(
            "/transcript/{user_id}/{sid}",
            get(get_transcript).delete(delete_transcript),
        )
        .route("/sessions", get(get_sessions))
        .route("/recall", post(post_recall))
        .with_state(state);

    run(app).await
}

// ---------- GET /config ----------

async fn get_config() -> impl IntoResponse {
    let cognito_domain = env::var("COGNITO_DOMAIN").unwrap_or_default();
    let client_id = env::var("COGNITO_CLIENT_ID").unwrap_or_default();
    Json(json!({ "cognito_domain": cognito_domain, "client_id": client_id }))
}

// ---------- helpers ----------

fn extract_user_id(req: &Request) -> Option<String> {
    let ctx = req.request_context_ref()?;
    match ctx {
        RequestContext::ApiGatewayV2(v2) => v2
            .authorizer
            .as_ref()?
            .jwt
            .as_ref()?
            .claims
            .get("sub")
            .cloned(),
        _ => None,
    }
}

// ---------- POST /transcript ----------

#[derive(Deserialize)]
struct PostTranscriptReq {
    session_id: String,
}

async fn post_transcript(
    State(state): State<Arc<AppState>>,
    req: Request,
) -> impl IntoResponse {
    let user_id = match extract_user_id(&req) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(json!({"error": "unauthorized"}))),
    };

    let body: PostTranscriptReq = match axum::Json::from_request(req, &()).await {
        Ok(Json(b)) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": e.to_string()}))),
    };

    let key = format!("{}/{}.txt", user_id, body.session_id);

    let presigned_config = aws_sdk_s3::presigning::PresigningConfig::builder()
        .expires_in(std::time::Duration::from_secs(3600))
        .build()
        .expect("valid presigning config");

    match state
        .s3
        .put_object()
        .bucket(&state.transcript_bucket)
        .key(&key)
        .presigned(presigned_config)
        .await
    {
        Ok(presigned) => (
            StatusCode::OK,
            Json(json!({ "upload_url": presigned.uri() })),
        ),
        Err(e) => {
            tracing::error!("presign error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "failed to generate presigned url"})),
            )
        }
    }
}

// ---------- GET /transcript/{user_id}/{sid} ----------

async fn get_transcript(
    State(state): State<Arc<AppState>>,
    Path((user_id, sid)): Path<(String, String)>,
    req: Request,
) -> impl IntoResponse {
    let caller = match extract_user_id(&req) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(json!({"error": "unauthorized"}))),
    };

    if caller != user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    let key = format!("{}/{}.txt", user_id, sid);

    let presigned_config = aws_sdk_s3::presigning::PresigningConfig::builder()
        .expires_in(std::time::Duration::from_secs(3600))
        .build()
        .expect("valid presigning config");

    match state
        .s3
        .get_object()
        .bucket(&state.transcript_bucket)
        .key(&key)
        .presigned(presigned_config)
        .await
    {
        Ok(presigned) => (
            StatusCode::OK,
            Json(json!({ "download_url": presigned.uri() })),
        ),
        Err(e) => {
            tracing::error!("presign error: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "failed to generate presigned url"})))
        }
    }
}

// ---------- DELETE /transcript/{user_id}/{sid} ----------

async fn delete_transcript(
    State(state): State<Arc<AppState>>,
    Path((user_id, sid)): Path<(String, String)>,
    req: Request,
) -> impl IntoResponse {
    let caller = match extract_user_id(&req) {
        Some(id) => id,
        None => return StatusCode::UNAUTHORIZED,
    };

    if caller != user_id {
        return StatusCode::FORBIDDEN;
    }

    let key = format!("{}/{}.txt", user_id, sid);

    match state
        .s3
        .delete_object()
        .bucket(&state.transcript_bucket)
        .key(&key)
        .send()
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(e) => {
            tracing::error!("s3 delete error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

// ---------- GET /sessions ----------

#[derive(Deserialize)]
struct SessionsQuery {
    continuation_token: Option<String>,
}

#[derive(Serialize)]
struct SessionEntry {
    session_id: String,
    size: i64,
    last_modified: String,
}

async fn get_sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SessionsQuery>,
    req: Request,
) -> impl IntoResponse {
    let user_id = match extract_user_id(&req) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(json!({"error": "unauthorized"}))),
    };

    let prefix = format!("{}/", user_id);
    let mut list_req = state
        .s3
        .list_objects_v2()
        .bucket(&state.transcript_bucket)
        .prefix(&prefix)
        .max_keys(100);

    if let Some(token) = &query.continuation_token {
        list_req = list_req.continuation_token(token);
    }

    match list_req.send().await {
        Ok(output) => {
            let sessions: Vec<SessionEntry> = output
                .contents()
                .iter()
                .filter_map(|obj| {
                    let key = obj.key()?;
                    let filename = key.strip_prefix(&prefix)?;
                    let session_id = filename.strip_suffix(".txt")?.to_string();
                    Some(SessionEntry {
                        session_id,
                        size: obj.size().unwrap_or(0),
                        last_modified: obj
                            .last_modified()
                            .map(|t| t.to_string())
                            .unwrap_or_default(),
                    })
                })
                .collect();

            let next_token = output.next_continuation_token().map(String::from);

            (
                StatusCode::OK,
                Json(json!({
                    "sessions": sessions,
                    "next_continuation_token": next_token,
                })),
            )
        }
        Err(e) => {
            tracing::error!("s3 list error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "failed to list sessions"})),
            )
        }
    }
}

// ---------- POST /recall ----------

#[derive(Deserialize)]
struct RecallReq {
    query: String,
    #[serde(default = "default_top_k")]
    top_k: i32,
}

fn default_top_k() -> i32 {
    5
}

async fn post_recall(
    State(state): State<Arc<AppState>>,
    req: Request,
) -> impl IntoResponse {
    if extract_user_id(&req).is_none() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        );
    }

    let body: RecallReq = match axum::Json::from_request(req, &()).await {
        Ok(Json(b)) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": e.to_string()}))),
    };

    use aws_sdk_bedrockagentruntime::types::{
        KnowledgeBaseQuery, KnowledgeBaseRetrievalConfiguration,
        KnowledgeBaseVectorSearchConfiguration,
    };

    let search_config = KnowledgeBaseRetrievalConfiguration::builder()
        .vector_search_configuration(
            KnowledgeBaseVectorSearchConfiguration::builder()
                .number_of_results(body.top_k)
                .build(),
        )
        .build();

    match state
        .kb
        .retrieve()
        .knowledge_base_id(&state.kb_id)
        .retrieval_query(
            KnowledgeBaseQuery::builder()
                .text(&body.query)
                .build(),
        )
        .retrieval_configuration(search_config)
        .send()
        .await
    {
        Ok(output) => {
            let results: Vec<serde_json::Value> = output
                .retrieval_results()
                .iter()
                .map(|r| {
                    let text = r
                        .content()
                        .map(|c| c.text())
                        .unwrap_or_default();
                    let score = r.score().unwrap_or(0.0);
                    let uri = r
                        .location()
                        .and_then(|l| l.s3_location())
                        .and_then(|s3| s3.uri())
                        .unwrap_or_default();
                    let session_id = uri
                        .rsplit('/')
                        .next()
                        .and_then(|f| f.strip_suffix(".txt"))
                        .unwrap_or_default();
                    json!({
                        "session_id": session_id,
                        "score": score,
                        "text": text,
                    })
                })
                .collect();

            (StatusCode::OK, Json(json!({ "results": results })))
        }
        Err(e) => {
            tracing::error!("kb retrieve error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "retrieval failed"})),
            )
        }
    }
}
