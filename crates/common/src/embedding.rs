use aws_sdk_bedrockruntime::Client as BedrockClient;
use aws_sdk_bedrockruntime::primitives::Blob;
use pgvector::Vector;

const MODEL_ID: &str = "amazon.titan-embed-text-v2:0";
const DIMENSIONS: usize = 1024;

/// Bedrock Titan Text Embedding V2 を呼び出してベクトルを返す
pub async fn embed_text(client: &BedrockClient, text: &str) -> Result<Vector, EmbeddingError> {
    let body = serde_json::json!({
        "inputText": text,
        "dimensions": DIMENSIONS,
    });

    let resp = client
        .invoke_model()
        .model_id(MODEL_ID)
        .body(Blob::new(serde_json::to_vec(&body)?))
        .content_type("application/json")
        .send()
        .await
        .map_err(|e| EmbeddingError::Bedrock(e.to_string()))?;

    let resp_json: serde_json::Value = serde_json::from_slice(resp.body().as_ref())?;

    let embedding = resp_json["embedding"]
        .as_array()
        .ok_or(EmbeddingError::Parse("no embedding array".into()))?
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect::<Vec<f32>>();

    if embedding.len() != DIMENSIONS {
        return Err(EmbeddingError::Parse(format!(
            "expected {DIMENSIONS} dims, got {}",
            embedding.len()
        )));
    }

    Ok(Vector::from(embedding))
}

/// 検索用のテキストを結合して embedding する
pub fn build_search_text(user_input: Option<&str>, tools_used: Option<&str>, ai_response: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some(u) = user_input {
        if !u.is_empty() {
            parts.push(u);
        }
    }
    if let Some(t) = tools_used {
        if !t.is_empty() {
            parts.push(t);
        }
    }
    if let Some(a) = ai_response {
        if !a.is_empty() {
            // AI response は長くなりがちなので先頭 500 文字（char）のみ
            let char_end = a.char_indices().nth(500).map(|(i, _)| i).unwrap_or(a.len());
            let truncated = &a[..char_end];
            parts.push(truncated);
        }
    }
    parts.join("\n")
}

#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("bedrock error: {0}")]
    Bedrock(String),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("parse error: {0}")]
    Parse(String),
}
