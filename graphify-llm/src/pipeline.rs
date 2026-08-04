use crate::config::{LLMConfig, Provider, ProviderType};
use crate::gbnf::get_json_schema_gbnf;
use anyhow::{Result, anyhow};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

pub struct AutoRotatePipeline {
    config: LLMConfig,
    client: Client,
}

impl AutoRotatePipeline {
    pub fn new(config: LLMConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self { config, client }
    }

    pub async fn extract_semantic_link(&self, prompt: &str) -> Result<String> {
        let mut last_err = anyhow!("No providers configured");

        for provider in &self.config.providers {
            match self.try_provider(provider, prompt).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    last_err = e;
                    // Automatically rotatess to next provider on failure
                }
            }
        }

        Err(anyhow!("LLM Pipeline failed after trying all providers. Last error: {}", last_err))
    }

    async fn try_provider(&self, provider: &Provider, prompt: &str) -> Result<String> {
        match provider.r#type {
            ProviderType::Ollama => {
                let url = format!("{}/api/generation", provider.endpoint.trim_end_matches('/'));
                let payload = serde_json::json!({
                    "model": provider.model,
                    "prompt": prompt,
                    "stream": false,
                    "grammar": get_json_schema_gbnf()
                });

                let res = self.client.post(&url)
                    .json(&payload)
                    .send()
                    .await?;

                if !res.status().is_success() {
                    anyhow::bail!("Ollama error status: {}", res.status());
                }

                let body: Value = res.json().await?;
                body["response"]
                    .as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| anyhow!("Ollama missing response body field"))
            }
            ProviderType::Gemini => {
                let api_key = provider.api_key.as_deref()
                    .ok_or_else(|| anyhow!("Gemini provider requires api_key"))?;
                let url = format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                    provider.model, api_key
                );

                let payload = serde_json::json!({
                    "contents": [{
                        "parts": [{ "text": prompt }]
                    }]
                });

                let res = self.client.post(&url)
                    .json(&payload)
                    .send()
                    .await?;

                if !res.status().is_success() {
                    anyhow::bail!("Gemini error status: {}", res.status());
                }

                let body: Value = res.json().await?;
                body["candidates"][0]["content"]["parts"][0]["text"]
                    .as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| anyhow!("Gemini response structural mismatch"))
            }
            ProviderType::OpenRouter => {
                let api_key = provider.api_key.as_deref()
                    .ok_or_else(|| anyhow!("OpenRouter provider requires api_key"))?;
                
                let res = self.client.post("https://openrouter.ai/api/v1/chat/completions")
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("HTTP-Referer", "https://github.com/cawa0505/graphify-rust")
                    .json(&serde_json::json!({
                        "model": provider.model,
                        "messages": [{ "role": "user", "content": prompt }]
                    }))
                    .send()
                    .await?;

                if !res.status().is_success() {
                    anyhow::bail!("OpenRouter error status: {}", res.status());
                }

                let body: Value = res.json().await?;
                body["choices"][0]["message"]["content"]
                    .as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| anyhow!("OpenRouter structural response mismatch"))
            }
        }
    }
}
