use aws_sdk_secretsmanager::Client as SmClient;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Aurora Secrets Manager のシークレットから接続文字列を組み立てて PgPool を作成する
pub async fn create_pool(sm_client: &SmClient, secret_arn: &str, db_name: &str) -> PgPool {
    let secret = sm_client
        .get_secret_value()
        .secret_id(secret_arn)
        .send()
        .await
        .expect("failed to get DB secret");

    let secret_str = secret.secret_string().expect("no secret string");
    let secret_json: serde_json::Value =
        serde_json::from_str(secret_str).expect("invalid secret JSON");

    let host = secret_json["host"].as_str().expect("no host in secret");
    let port = secret_json["port"].as_u64().unwrap_or(5432);
    let username = secret_json["username"].as_str().expect("no username");
    let password = secret_json["password"].as_str().expect("no password");

    let url = format!("postgres://{username}:{password}@{host}:{port}/{db_name}");

    PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("failed to connect to Aurora")
}
