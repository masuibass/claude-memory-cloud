use crate::models::{ParsedToolResult, TranscriptEntry};

pub async fn fetch_transcript_bytes(
    s3: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
) -> Option<Vec<u8>> {
    let resp = s3.get_object().bucket(bucket).key(key).send().await.ok()?;
    let body = resp.body.collect().await.ok()?;
    Some(body.into_bytes().to_vec())
}

pub fn parse_entries(bytes: &[u8]) -> Vec<TranscriptEntry> {
    let text = String::from_utf8_lossy(bytes);
    text.lines()
        .filter_map(|line| serde_json::from_str::<TranscriptEntry>(line).ok())
        .collect()
}

pub fn extract_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|block| {
                if block.get("type")?.as_str()? == "text" {
                    block.get("text")?.as_str().map(String::from)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

pub fn extract_tool_uses(content: &serde_json::Value) -> Vec<(String, String, serde_json::Value)> {
    match content {
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|block| {
                if block.get("type")?.as_str()? == "tool_use" {
                    let id = block.get("id")?.as_str()?.to_string();
                    let name = block.get("name")?.as_str()?.to_string();
                    let input = block.get("input").cloned().unwrap_or(serde_json::Value::Null);
                    Some((id, name, input))
                } else {
                    None
                }
            })
            .collect(),
        _ => vec![],
    }
}

pub fn extract_tool_results(content: &serde_json::Value) -> Vec<ParsedToolResult> {
    match content {
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|block| {
                if block.get("type")?.as_str()? == "tool_result" {
                    let tool_use_id = block.get("tool_use_id")?.as_str()?.to_string();
                    let content = match block.get("content") {
                        Some(serde_json::Value::String(s)) => s.clone(),
                        Some(serde_json::Value::Array(arr)) => arr
                            .iter()
                            .filter_map(|b| {
                                if b.get("type")?.as_str()? == "text" {
                                    b.get("text")?.as_str().map(String::from)
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                        _ => String::new(),
                    };
                    Some(ParsedToolResult {
                        tool_use_id,
                        content,
                    })
                } else {
                    None
                }
            })
            .collect(),
        _ => vec![],
    }
}
