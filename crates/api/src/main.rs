mod handlers;

use std::env;
use std::sync::Arc;

use aws_sdk_bedrockruntime::Client as BedrockClient;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_secretsmanager::Client as SmClient;
use lambda_http::{run, Error};
use sqlx::PgPool;

use claude_memory_common::db;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub bedrock: BedrockClient,
    pub s3: S3Client,
    pub transcript_bucket: String,
}

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
    let transcript_bucket = env::var("TRANSCRIPT_BUCKET").expect("TRANSCRIPT_BUCKET required");

    let pool = db::create_pool(&sm_client, &db_secret_arn, &db_name).await;

    let state = AppState {
        pool,
        bedrock: BedrockClient::new(&aws_config),
        s3: S3Client::new(&aws_config),
        transcript_bucket,
    };

    let app = handlers::router().with_state(Arc::new(state));

    run(app).await
}
