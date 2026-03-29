use std::env;
use std::sync::Arc;

use axum::extract::{FromRequest, Path, Query, Request, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use lambda_http::request::RequestContext;
use lambda_http::{run, Error, RequestExt};
use serde::{Deserialize, Serialize};
use serde_json::json;

struct AppState {
    s3: aws_sdk_s3::Client,
    kb: aws_sdk_bedrockagentruntime::Client,
    ddb: aws_sdk_dynamodb::Client,
    cognito: aws_sdk_cognitoidentityprovider::Client,
    transcript_bucket: String,
    kb_id: String,
    shares_table: String,
    user_pool_id: String,
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
        ddb: aws_sdk_dynamodb::Client::new(&config),
        cognito: aws_sdk_cognitoidentityprovider::Client::new(&config),
        transcript_bucket: env::var("TRANSCRIPT_BUCKET").unwrap_or_default(),
        kb_id: env::var("KB_ID").unwrap_or_default(),
        shares_table: env::var("SHARES_TABLE").unwrap_or_default(),
        user_pool_id: env::var("COGNITO_USER_POOL_ID").unwrap_or_default(),
    });

    let app = Router::new()
        .route("/config", get(get_config))
        .route("/transcript", post(post_transcript))
        .route(
            "/transcript/{user_id}/{project}/{sid}",
            get(get_transcript).delete(delete_transcript),
        )
        .route("/sessions", get(get_sessions))
        .route("/recall", post(post_recall))
        .route("/shares", get(get_shares).post(post_share))
        .route("/shares/{owner_id}", delete(delete_share))
        .route("/shares/recipients/{recipient_id}", delete(delete_share_by_owner))
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

/// Get the list of user_ids whose transcripts the caller can search.
/// Returns [caller_id, shared_owner_1, shared_owner_2, ...].
async fn get_searchable_user_ids(
    state: &AppState,
    caller: &str,
) -> Result<Vec<String>, aws_sdk_dynamodb::Error> {
    use aws_sdk_dynamodb::types::AttributeValue;

    let mut ids = vec![caller.to_string()];

    let result = state
        .ddb
        .query()
        .table_name(&state.shares_table)
        .key_condition_expression("pk = :pk AND begins_with(sk, :prefix)")
        .expression_attribute_values(":pk", AttributeValue::S(caller.to_string()))
        .expression_attribute_values(":prefix", AttributeValue::S("share#".to_string()))
        .consistent_read(true)
        .send()
        .await?;

    if let Some(items) = result.items {
        for item in &items {
            if let Some(AttributeValue::S(owner)) = item.get("owner_id") {
                ids.push(owner.clone());
            }
        }
    }

    Ok(ids)
}

// ---------- POST /transcript ----------

#[derive(Deserialize)]
struct PostTranscriptReq {
    session_id: String,
    project: String,
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

    let key = format!("{}/{}/{}.jsonl", user_id, body.project, body.session_id);

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

// ---------- GET /transcript/{user_id}/{project}/{sid} ----------

async fn get_transcript(
    State(state): State<Arc<AppState>>,
    Path((user_id, project, sid)): Path<(String, String, String)>,
    req: Request,
) -> impl IntoResponse {
    let caller = match extract_user_id(&req) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(json!({"error": "unauthorized"}))),
    };

    if caller != user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    let key = format!("{}/{}/{}.jsonl", user_id, project, sid);

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

// ---------- DELETE /transcript/{user_id}/{project}/{sid} ----------

async fn delete_transcript(
    State(state): State<Arc<AppState>>,
    Path((user_id, project, sid)): Path<(String, String, String)>,
    req: Request,
) -> impl IntoResponse {
    let caller = match extract_user_id(&req) {
        Some(id) => id,
        None => return StatusCode::UNAUTHORIZED,
    };

    if caller != user_id {
        return StatusCode::FORBIDDEN;
    }

    let key = format!("{}/{}/{}.jsonl", user_id, project, sid);

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
                    let session_id = filename.strip_suffix(".jsonl")?.to_string();
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
    let caller = match extract_user_id(&req) {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "unauthorized"})),
            );
        }
    };

    let body: RecallReq = match axum::Json::from_request(req, &()).await {
        Ok(Json(b)) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": e.to_string()}))),
    };

    // Get searchable user_ids (caller + shared owners)
    let user_ids = match get_searchable_user_ids(&state, &caller).await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::error!("ddb query error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "failed to query shares"})),
            );
        }
    };

    use aws_sdk_bedrockagentruntime::types::{
        FilterAttribute, KnowledgeBaseQuery, KnowledgeBaseRetrievalConfiguration,
        KnowledgeBaseVectorSearchConfiguration, RetrievalFilter,
    };

    // Build user_id IN [...] filter
    let user_id_values: Vec<aws_smithy_types::Document> = user_ids
        .iter()
        .map(|id| aws_smithy_types::Document::String(id.clone()))
        .collect();

    let filter = RetrievalFilter::In(
        FilterAttribute::builder()
            .key("user_id")
            .value(aws_smithy_types::Document::Array(user_id_values))
            .build()
            .expect("valid filter"),
    );

    let search_config = KnowledgeBaseRetrievalConfiguration::builder()
        .vector_search_configuration(
            KnowledgeBaseVectorSearchConfiguration::builder()
                .number_of_results(body.top_k)
                .filter(filter)
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
                    let filename = uri
                        .rsplit('/')
                        .next()
                        .unwrap_or_default();
                    let session_id = filename
                        .strip_suffix(".md")
                        .or_else(|| filename.strip_suffix(".jsonl"))
                        .or_else(|| filename.strip_suffix(".txt"))
                        .unwrap_or(filename);
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

// ---------- POST /shares ----------

#[derive(Deserialize)]
struct PostShareReq {
    /// The user_id (Cognito sub) to share your transcripts with.
    recipient_id: String,
}

async fn post_share(
    State(state): State<Arc<AppState>>,
    req: Request,
) -> impl IntoResponse {
    let caller = match extract_user_id(&req) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(json!({"error": "unauthorized"}))),
    };

    let body: PostShareReq = match axum::Json::from_request(req, &()).await {
        Ok(Json(b)) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": e.to_string()}))),
    };

    if body.recipient_id == caller {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "cannot share with yourself"})),
        );
    }

    // Verify recipient exists in Cognito
    match state
        .cognito
        .admin_get_user()
        .user_pool_id(&state.user_pool_id)
        .username(&body.recipient_id)
        .send()
        .await
    {
        Ok(_) => {}
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "recipient user not found"})),
            );
        }
    }

    use aws_sdk_dynamodb::types::AttributeValue;

    // Put share record: PK=recipient, SK=share#owner
    if let Err(e) = state
        .ddb
        .put_item()
        .table_name(&state.shares_table)
        .item("pk", AttributeValue::S(body.recipient_id.clone()))
        .item("sk", AttributeValue::S(format!("share#{}", caller)))
        .item("owner_id", AttributeValue::S(caller.clone()))
        .item("recipient_id", AttributeValue::S(body.recipient_id.clone()))
        .send()
        .await
    {
        tracing::error!("ddb put error: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "failed to create share"})),
        );
    }

    (
        StatusCode::CREATED,
        Json(json!({
            "owner_id": caller,
            "recipient_id": body.recipient_id,
        })),
    )
}

// ---------- GET /shares ----------

async fn get_shares(
    State(state): State<Arc<AppState>>,
    req: Request,
) -> impl IntoResponse {
    let caller = match extract_user_id(&req) {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, Json(json!({"error": "unauthorized"}))),
    };

    use aws_sdk_dynamodb::types::AttributeValue;

    // 1. Shares I receive (who shared with me)
    let received = state
        .ddb
        .query()
        .table_name(&state.shares_table)
        .key_condition_expression("pk = :pk AND begins_with(sk, :prefix)")
        .expression_attribute_values(":pk", AttributeValue::S(caller.clone()))
        .expression_attribute_values(":prefix", AttributeValue::S("share#".to_string()))
        .consistent_read(true)
        .send()
        .await;

    let shared_with_me: Vec<String> = received
        .ok()
        .and_then(|r| r.items)
        .unwrap_or_default()
        .iter()
        .filter_map(|item| {
            item.get("owner_id")
                .and_then(|v| v.as_s().ok())
                .cloned()
        })
        .collect();

    // 2. Shares I gave (who I shared with) — via GSI
    let given = state
        .ddb
        .query()
        .table_name(&state.shares_table)
        .index_name("ByOwner")
        .key_condition_expression("owner_id = :owner")
        .expression_attribute_values(":owner", AttributeValue::S(caller.clone()))
        .send()
        .await;

    let shared_by_me: Vec<String> = given
        .ok()
        .and_then(|r| r.items)
        .unwrap_or_default()
        .iter()
        .filter_map(|item| {
            item.get("recipient_id")
                .and_then(|v| v.as_s().ok())
                .cloned()
        })
        .collect();

    (
        StatusCode::OK,
        Json(json!({
            "shared_with_me": shared_with_me,
            "shared_by_me": shared_by_me,
        })),
    )
}

// ---------- DELETE /shares/{owner_id} ----------

async fn delete_share(
    State(state): State<Arc<AppState>>,
    Path(owner_id): Path<String>,
    req: Request,
) -> impl IntoResponse {
    let caller = match extract_user_id(&req) {
        Some(id) => id,
        None => return StatusCode::UNAUTHORIZED,
    };

    use aws_sdk_dynamodb::types::AttributeValue;

    // Caller is the recipient — remove the share from owner_id
    match state
        .ddb
        .delete_item()
        .table_name(&state.shares_table)
        .key("pk", AttributeValue::S(caller))
        .key("sk", AttributeValue::S(format!("share#{}", owner_id)))
        .send()
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(e) => {
            tracing::error!("ddb delete error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

// ---------- DELETE /shares/recipients/{recipient_id} ----------

async fn delete_share_by_owner(
    State(state): State<Arc<AppState>>,
    Path(recipient_id): Path<String>,
    req: Request,
) -> impl IntoResponse {
    let caller = match extract_user_id(&req) {
        Some(id) => id,
        None => return StatusCode::UNAUTHORIZED,
    };

    use aws_sdk_dynamodb::types::AttributeValue;

    // Caller is the owner — remove the share to recipient_id
    match state
        .ddb
        .delete_item()
        .table_name(&state.shares_table)
        .key("pk", AttributeValue::S(recipient_id))
        .key("sk", AttributeValue::S(format!("share#{}", caller)))
        .send()
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(e) => {
            tracing::error!("ddb delete error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
