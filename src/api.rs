use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const OPENAI_COMPLETIONS_PATH: &str = "/chat/completions";
pub const OPENAI_MODELS_PATH: &str = "/models";

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Serialize)]
pub(crate) struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ApiErrorDetail {
    #[serde(default)]
    code: String,
    message: String,
    #[serde(rename = "type", default)]
    error_type: String,
}

#[derive(Deserialize)]
struct ApiErrorResponse {
    error: ApiErrorDetail,
}

pub struct ChatClient;

impl ChatClient {
    fn normalize_url(api_url: &str) -> String {
        let trimmed = api_url.trim_end_matches('/');
        if trimmed.ends_with(OPENAI_COMPLETIONS_PATH) {
            trimmed.to_string()
        } else {
            format!("{}{}", trimmed, OPENAI_COMPLETIONS_PATH)
        }
    }

    fn normalize_messages(messages: &[ChatMessage]) -> Vec<ChatMessage> {
        if messages.len() <= 1 {
            return messages.to_vec();
        }

        let system_content: String = messages
            .iter()
            .filter(|m| m.role == "system")
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        let non_system: Vec<&ChatMessage> = messages.iter().filter(|m| m.role != "system").collect();

        let compact: String = non_system
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        let content = if system_content.is_empty() {
            compact
        } else {
            format!("[System]\n{}\n\n[Conversation]\n{}", system_content, compact)
        };

        vec![ChatMessage {
            role: "user".into(),
            content,
        }]
    }

    fn extract_error(body: &str) -> String {
        if let Ok(err) = serde_json::from_str::<ApiErrorResponse>(body) {
            let code = if err.error.code.is_empty() {
                String::new()
            } else {
                format!(" [{}]", err.error.code)
            };
            format!("{}{}", err.error.message, code)
        } else {
            body.to_string()
        }
    }

    fn build_request(
        model: &str,
        max_tokens: u32,
        temperature: f32,
        top_p: f32,
        messages: Vec<ChatMessage>,
        stream: bool,
    ) -> (ChatCompletionRequest, reqwest::Client) {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to build HTTP client");

        let request = ChatCompletionRequest {
            model: model.to_string(),
            messages,
            max_tokens: Some(max_tokens),
            temperature: Some(temperature),
            top_p: Some(top_p),
            frequency_penalty: None,
            presence_penalty: None,
            stop: None,
            stream: if stream { Some(true) } else { Some(false) },
        };

        (request, client)
    }

    pub async fn complete(
        api_key: &str,
        api_url: &str,
        model: &str,
        max_tokens: u32,
        temperature: f32,
        top_p: f32,
        messages: Vec<ChatMessage>,
    ) -> Result<String, String> {
        let messages = Self::normalize_messages(&messages);
        let (request, client) = Self::build_request(model, max_tokens, temperature, top_p, messages, false);
        let url = Self::normalize_url(api_url);

        let mut req = client
            .post(&url)
            .json(&request);

        if !api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let resp = req.send().await.map_err(|e| format!("请求失败: {}", e))?;

        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            let detail = Self::extract_error(&b);
            return Err(format!("API 错误 (model: {}): HTTP {} - {}", model, s.as_u16(), detail));
        }

        let r: ChatCompletionResponse = resp.json().await.map_err(|e| format!("解析响应失败: {}", e))?;
        r.choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| "API 返回为空".into())
    }

    pub async fn complete_streaming(
        api_key: &str,
        api_url: &str,
        model: &str,
        max_tokens: u32,
        temperature: f32,
        top_p: f32,
        messages: Vec<ChatMessage>,
        on_chunk: impl Fn(&str),
    ) -> Result<String, String> {
        let messages = Self::normalize_messages(&messages);
        let (request, client) = Self::build_request(model, max_tokens, temperature, top_p, messages, true);
        let url = Self::normalize_url(api_url);

        let mut req = client
            .post(&url)
            .header("Accept", "text/event-stream")
            .json(&request);

        if !api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let resp = req.send().await.map_err(|e| format!("请求失败: {}", e))?;

        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            let detail = Self::extract_error(&b);
            return Err(format!("API 错误 (model: {}): HTTP {} - {}", model, s.as_u16(), detail));
        }

        let mut stream = resp.bytes_stream();
        let mut full = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("流读取失败: {}", e))?;
            let text = String::from_utf8_lossy(&chunk);

            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with(':') {
                    continue;
                }
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if data == "[DONE]" {
                        return Ok(full);
                    }
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                        if let Some(content) = val
                            .get("choices")
                            .and_then(|c| c.as_array().and_then(|a| a.first()))
                            .and_then(|c| c.get("delta"))
                            .and_then(|d| d.get("content"))
                            .and_then(|c| c.as_str())
                        {
                            full.push_str(content);
                            on_chunk(content);
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
        let base = api_url
            .trim_end_matches(OPENAI_COMPLETIONS_PATH)
            .trim_end_matches('/');
        let models_url = format!("{}{}", base, OPENAI_MODELS_PATH);

        let start = std::time::Instant::now();
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .build()
        {
            Ok(c) => c,
            Err(_) => return (false, None),
        };

        let mut req = client.get(&models_url).header("Accept", "application/json");
        if !api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let latency_ms = start.elapsed().as_millis() as u64;
        match req.send().await {
            Ok(resp) => (resp.status().is_success(), Some(latency_ms)),
            Err(_) => (false, Some(latency_ms)),
        }
    }
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
    ChatClient::complete(api_key, api_url, model, max_tokens, temperature, top_p, messages).await
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
    ChatClient::complete_streaming(api_key, api_url, model, max_tokens, temperature, top_p, messages, on_chunk).await
}

pub async fn health_check(api_key: &str, api_url: &str) -> (bool, Option<u64>) {
    ChatClient::health_check(api_key, api_url).await
}
