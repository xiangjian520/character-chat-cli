use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChatMessage,
}

pub async fn send_message_async(
    api_key: &str,
    api_url: &str,
    model: &str,
    max_tokens: u32,
    temperature: f32,
    top_p: f32,
    messages: Vec<ChatMessage>,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let request = ChatRequest {
        model: model.to_string(),
        messages,
        max_tokens: Some(max_tokens),
        temperature: Some(temperature),
        top_p: Some(top_p),
        stream: Some(false),
    };

    let resp = client
        .post(api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", s, b));
    }

    let r: ChatResponse = resp.json().await.map_err(|e| format!("解析响应失败: {}", e))?;
    r.choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .ok_or_else(|| "API 返回为空".into())
}

pub async fn send_message_streaming(
    api_key: &str,
    api_url: &str,
    model: &str,
    max_tokens: u32,
    temperature: f32,
    top_p: f32,
    messages: Vec<ChatMessage>,
    on_chunk: impl Fn(&str),
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let request = ChatRequest {
        model: model.to_string(),
        messages,
        max_tokens: Some(max_tokens),
        temperature: Some(temperature),
        top_p: Some(top_p),
        stream: Some(true),
    };

    let resp = client
        .post(api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", s, b));
    }

    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut full = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("流读取失败: {}", e))?;
        let text = String::from_utf8_lossy(&chunk);

        for line in text.lines() {
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            if line.starts_with("data: ") {
                let data = &line[6..];
                if data == "[DONE]" {
                    return Ok(full);
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(choices) = val.get("choices").and_then(|c| c.as_array()) {
                        for choice in choices {
                            if let Some(delta) = choice.get("delta") {
                                if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                    full.push_str(content);
                                    on_chunk(content);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if full.is_empty() {
        Err("流式响应未收到任何内容".into())
    } else {
        Ok(full)
    }
}

pub async fn health_check(api_key: &str, api_url: &str) -> (bool, Option<u64>) {
    if api_key.is_empty() {
        return (false, None);
    }
    let base = api_url
        .trim_end_matches("/chat/completions")
        .trim_end_matches("/v1/chat/completions")
        .to_string();

    let start = std::time::Instant::now();
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(_) => return (false, None),
    };

    let result = client
        .head(&base)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await;

    let latency_ms = start.elapsed().as_millis() as u64;
    match result {
        Ok(resp) => (resp.status().is_success(), Some(latency_ms)),
        Err(_) => (false, Some(latency_ms)),
    }
}
