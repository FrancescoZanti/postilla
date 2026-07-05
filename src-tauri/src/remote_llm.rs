use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize)]
struct OpenAIChatRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    temperature: f32,
}

#[derive(Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAIChatResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
}

#[derive(Deserialize)]
struct OpenAIModelsResponse {
    data: Vec<OpenAIModelItem>,
}

#[derive(Deserialize)]
struct OpenAIModelItem {
    id: String,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessage>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: String,
}

#[derive(Deserialize)]
struct AnthropicModelsResponse {
    data: Vec<AnthropicModelItem>,
}

#[derive(Deserialize)]
struct AnthropicModelItem {
    #[serde(rename = "type")]
    model_type: String,
    id: String,
}

// ──────────────────────────────
// OpenAI
// ──────────────────────────────

pub async fn openai_list_models(api_key: &str) -> Result<Vec<String>, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| format!("OpenAI connection failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("OpenAI error ({}): {}", status, body));
    }

    let models: OpenAIModelsResponse = resp.json().await
        .map_err(|e| format!("Failed to parse OpenAI models: {}", e))?;

    // Filter to chat-capable models
    let chat_models: Vec<String> = models.data.into_iter()
        .map(|m| m.id)
        .filter(|id| {
            id.contains("gpt-4") || id.contains("gpt-3.5") || id.contains("gpt-4o")
        })
        .collect();

    if chat_models.is_empty() {
        return Err("No compatible chat models found on this account.".into());
    }

    Ok(chat_models)
}

pub async fn openai_chat(api_key: &str, model: &str, system: &str, prompt: &str) -> Result<String, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;

    let req = OpenAIChatRequest {
        model: model.to_string(),
        messages: vec![
            OpenAIMessage { role: "system".into(), content: system.to_string() },
            OpenAIMessage { role: "user".into(), content: prompt.to_string() },
        ],
        temperature: 0.3,
    };

    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("OpenAI API error: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("OpenAI error ({}): {}", status, body));
    }

    let result: OpenAIChatResponse = resp.json().await
        .map_err(|e| format!("Failed to parse OpenAI response: {}", e))?;

    result.choices
        .first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| "OpenAI returned no choices".into())
}

// ──────────────────────────────
// Anthropic (Claude)
// ──────────────────────────────

pub async fn anthropic_list_models(api_key: &str) -> Result<Vec<String>, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(|e| format!("Anthropic connection failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Anthropic error ({}): {}", status, body));
    }

    let models: AnthropicModelsResponse = resp.json().await
        .map_err(|e| format!("Failed to parse Anthropic models: {}", e))?;

    let chat_models: Vec<String> = models.data.into_iter()
        .map(|m| m.id)
        .filter(|id| id.contains("claude"))
        .collect();

    if chat_models.is_empty() {
        return Err("No Claude models found on this account.".into());
    }

    Ok(chat_models)
}

pub async fn anthropic_chat(api_key: &str, model: &str, system: &str, prompt: &str) -> Result<String, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;

    let req = AnthropicRequest {
        model: model.to_string(),
        max_tokens: 4096,
        system: system.to_string(),
        messages: vec![
            AnthropicMessage { role: "user".into(), content: prompt.to_string() },
        ],
    };

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("Anthropic API error: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Anthropic error ({}): {}", status, body));
    }

    let result: AnthropicResponse = resp.json().await
        .map_err(|e| format!("Failed to parse Anthropic response: {}", e))?;

    result.content
        .first()
        .map(|c| c.text.clone())
        .ok_or_else(|| "Anthropic returned no content".into())
}
