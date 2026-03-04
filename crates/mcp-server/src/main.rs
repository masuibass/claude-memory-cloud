mod tools;

use std::env;
use std::sync::Arc;

use aws_sdk_bedrockruntime::Client as BedrockClient;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_secretsmanager::Client as SmClient;
use lambda_http::{run, Error};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tokio_util::sync::CancellationToken;

use claude_memory_common::db;
use tools::McpServer;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .without_time()
        .with_ansi(false)
        .init();

    let aws_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let sm_client = SmClient::new(&aws_config);

    let db_secret_arn = env::var("DB_SECRET_ARN").expect("DB_SECRET_ARN required");
    let db_name = env::var("DB_NAME").unwrap_or_else(|_| "memory".to_string());
    let transcript_bucket =
        env::var("TRANSCRIPT_BUCKET").expect("TRANSCRIPT_BUCKET required");

    let pool = db::create_pool(&sm_client, &db_secret_arn, &db_name).await;
    let bedrock = BedrockClient::new(&aws_config);
    let s3 = S3Client::new(&aws_config);

    let pool = Arc::new(pool);
    let bedrock = Arc::new(bedrock);
    let s3 = Arc::new(s3);
    let transcript_bucket = Arc::new(transcript_bucket);

    let mcp_service = StreamableHttpService::new(
        move || {
            Ok(McpServer::new(
                pool.clone(),
                bedrock.clone(),
                s3.clone(),
                transcript_bucket.clone(),
            ))
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig {
            stateful_mode: false,
            cancellation_token: CancellationToken::new(),
            ..Default::default()
        },
    );

    let app = axum::Router::new().nest_service("/mcp", mcp_service);

    run(app).await
}
