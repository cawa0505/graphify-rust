use crate::config::{LLMConfig, Provider, ProviderType};
use crate::gbnf::get_json_schema_gbnf;
use anyhow::{Result, anyhow};
use reqwest::Client;
use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct AutoRotatePipeline {
    config: LLMConfig,
    client: Client,
    counter: AtomicUsize,
}

fn get_jitter() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| (duration.as_nanos() % 500) as u64)
}

impl AutoRotatePipeline {
    pub fn new(config: LLMConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self {
            config,
            client,
            counter: AtomicUsize::new(0),
        }
    }

    /// Extracts semantic links using the configured providers and thread-safe API key rotation with failover.
    ///
    /// # Errors
    /// Returns an error if all providers (including fallbacks) fail.
    pub async fn extract_semantic_link(&self, prompt: &str) -> Result<String> {
        const MAX_BACKOFF_RETRIES: u32 = 3;
        let mut last_err = anyhow!("No providers configured");

        for (idx, provider) in self.config.providers.iter().enumerate() {
            let is_backup = idx > 0;

            // 1. Determine key rotation list
            let keys: Vec<String> = if !self.config.api_keys.is_empty() {
                self.config.api_keys.clone()
            } else if let Some(ref k) = provider.api_key {
                k.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
            } else {
                Vec::new()
            };

            if provider.r#type != ProviderType::Ollama && keys.is_empty() {
                last_err = anyhow!("Provider {} requires an API key but none was supplied", provider.name);
                continue;
            }

            // 2. Retry loop
            let mut key_failed_count = 0;

            let mut backoff_attempt = 0;
            let mut base_delay = Duration::from_secs(1);

            loop {
                // Get the current key using atomic modulo
                let current_key = if keys.is_empty() {
                    None
                } else {
                    let key_idx = self.counter.load(Ordering::SeqCst) % keys.len();
                    Some(keys[key_idx].as_str())
                };

                match self.try_provider(provider, current_key, prompt).await {
                    Ok(response) => return Ok(response),
                    Err(e) => {
                        // Check if it's a 429 rate limit or RESOURCE_EXHAUSTED
                        let is_429 = e.downcast_ref::<reqwest::Error>().map_or_else(
                            || {
                                let err_msg = e.to_string().to_lowercase();
                                err_msg.contains("429") || err_msg.contains("resource_exhausted") || err_msg.contains("too many requests")
                            },
                            |req_err| req_err.status().map(|s| s.as_u16()) == Some(429),
                        );

                        if is_429 && !keys.is_empty() && key_failed_count < keys.len() - 1 {
                            // Thread-safe atomic rotation increment without sleep
                            self.counter.fetch_add(1, Ordering::SeqCst);
                            key_failed_count += 1;
                            // Retry immediately with next key
                            continue;
                        }

                        // Otherwise, check if we should do exponential backoff (only on backup providers)
                        if is_backup && backoff_attempt < MAX_BACKOFF_RETRIES {
                            let jitter_ms = get_jitter();
                            let sleep_dur = base_delay + Duration::from_millis(jitter_ms);
                            tokio::time::sleep(sleep_dur).await;
                            base_delay *= 2;
                            backoff_attempt += 1;
                            continue;
                        }

                        last_err = e;
                        break; // Move to next provider
                    }
                }
            }
        }

        Err(anyhow!("LLM Pipeline failed after trying all providers. Last error: {}", last_err))
    }

    async fn try_provider(&self, provider: &Provider, api_key: Option<&str>, prompt: &str) -> Result<String> {
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
                let res = res.error_for_status()?;

                let body: Value = res.json().await?;
                body["response"]
                    .as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| anyhow!("Ollama missing response body field"))
            }
            ProviderType::Gemini => {
                let key = api_key
                    .ok_or_else(|| anyhow!("Gemini provider requires api_key"))?;
                let url = format!(
                    "{}/v1beta/models/{}:generateContent?key={}",
                    if provider.endpoint.is_empty() {
                        "https://generativelanguage.googleapis.com"
                    } else {
                        provider.endpoint.trim_end_matches('/')
                    },
                    provider.model,
                    key
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
                let res = res.error_for_status()?;

                let body: Value = res.json().await?;
                body["candidates"][0]["content"]["parts"][0]["text"]
                    .as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| anyhow!("Gemini response structural mismatch"))
            }
            ProviderType::OpenRouter | ProviderType::OpenAI => {
                let key = api_key
                    .ok_or_else(|| anyhow!("Provider requires api_key"))?;
                
                let default_base = match provider.r#type {
                    ProviderType::OpenRouter => "https://openrouter.ai/api",
                    _ => "https://api.openai.com",
                };
                
                let base_url = if provider.endpoint.is_empty() {
                    default_base
                } else {
                    provider.endpoint.trim_end_matches('/')
                };

                let url = if provider.r#type == ProviderType::OpenRouter && provider.endpoint.is_empty() {
                    format!("{}/v1/chat/completions", base_url)
                } else {
                    format!("{}/chat/completions", base_url)
                };
                
                let res = self.client.post(&url)
                    .header("Authorization", format!("Bearer {}", key))
                    .header("HTTP-Referer", "https://github.com/cawa0505/graphify-rust")
                    .json(&serde_json::json!({
                        "model": provider.model,
                        "messages": [{ "role": "user", "content": prompt }]
                    }))
                    .send()
                    .await?;
                let res = res.error_for_status()?;

                let body: Value = res.json().await?;
                body["choices"][0]["message"]["content"]
                    .as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| anyhow!("OpenAI/OpenRouter structural response mismatch"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Provider, ProviderType, ExtractionConfig, MemoryConfig};
    use tokio::net::TcpListener;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use std::sync::Arc;
    use std::sync::atomic::AtomicU32;

    #[allow(clippy::too_many_lines)] // ponytail: mock server matches multiple HTTP routes in one loop, acceptable length for test helper
    async fn run_mock_server() -> Result<(String, tokio::task::JoinHandle<()>, Arc<AtomicU32>)> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let url = format!("http://127.0.0.1:{}", addr.port());
        
        let req_count = Arc::new(AtomicU32::new(0));
        let req_count_clone = req_count.clone();

        let handle = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let req_count = req_count_clone.clone();
                tokio::spawn(async move {
                    let mut buf = [0; 4096];
                    if let Ok(n) = socket.read(&mut buf).await {
                        let req_str = String::from_utf8_lossy(&buf[..n]);
                        req_count.fetch_add(1, Ordering::SeqCst);
                        
                        if req_str.contains("key=badkey") {
                            // Respond with HTTP 429
                            let response = "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n";
                            let _ = socket.write_all(response.as_bytes()).await;
                        } else if req_str.contains("api/embeddings") {
                            // Ollama embeddings mock
                            let mut vector = vec![0.0; 1024];
                            vector[0] = 0.5; // dummy values
                            let json_body = serde_json::json!({
                                "embedding": vector
                            });
                            let json_str = serde_json::to_string(&json_body).unwrap_or_default();
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                json_str.len(),
                                json_str
                            );
                            let _ = socket.write_all(response.as_bytes()).await;
                        } else if req_str.contains("points/search") {
                            // Qdrant search mock
                            let json_body = serde_json::json!({
                                "result": [{
                                    "id": 1,
                                    "payload": {
                                        "node_id": "test_node_id"
                                    }
                                }]
                            });
                            let json_str = serde_json::to_string(&json_body).unwrap_or_default();
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                json_str.len(),
                                json_str
                            );
                            let _ = socket.write_all(response.as_bytes()).await;
                        } else if req_str.contains("points") {
                            // Qdrant upsert mock
                            let json_body = serde_json::json!({
                                "status": "ok"
                            });
                            let json_str = serde_json::to_string(&json_body).unwrap_or_default();
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                json_str.len(),
                                json_str
                            );
                            let _ = socket.write_all(response.as_bytes()).await;
                        } else if req_str.contains("collections/graphify_memory") {
                            // Qdrant collection check
                            let json_body = serde_json::json!({
                                "result": {
                                    "status": "green"
                                }
                            });
                            let json_str = serde_json::to_string(&json_body).unwrap_or_default();
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                json_str.len(),
                                json_str
                            );
                            let _ = socket.write_all(response.as_bytes()).await;
                        } else if req_str.contains("api/generation") {
                            // Ollama mock
                            let json_body = serde_json::json!({
                                "response": "{\"links\": []}"
                            });
                            let json_str = serde_json::to_string(&json_body).unwrap_or_default();
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                json_str.len(),
                                json_str
                            );
                            let _ = socket.write_all(response.as_bytes()).await;
                        } else {
                            // Gemini mock success
                            let json_body = serde_json::json!({
                                "candidates": [{
                                    "content": {
                                        "parts": [{ "text": "{\"links\": []}" }]
                                    }
                                }]
                            });
                            let json_str = serde_json::to_string(&json_body).unwrap_or_default();
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                json_str.len(),
                                json_str
                            );
                            let _ = socket.write_all(response.as_bytes()).await;
                        }
                    }
                });
            }
        });

        Ok((url, handle, req_count))
    }

    #[tokio::test]
    async fn test_api_key_rotation_on_429() -> Result<()> {
        let (mock_url, server_handle, req_count) = run_mock_server().await?;

        let config = LLMConfig {
            providers: vec![Provider {
                name: "GeminiPrimary".to_string(),
                r#type: ProviderType::Gemini,
                endpoint: mock_url,
                model: "gemini-1.5-flash".to_string(),
                api_key: None,
                priority: 1,
            }],
            extraction: ExtractionConfig {
                chunk_size: 1024,
                max_concurrency: 1,
                concurrency: None,
            },
            api_keys: vec!["badkey".to_string(), "goodkey".to_string()],
            memory: MemoryConfig::default(),
        };

        let pipeline = AutoRotatePipeline::new(config);
        let res = pipeline.extract_semantic_link("hello").await?;
        
        assert_eq!(res, "{\"links\": []}");
        // First try (badkey) -> 429, Second try (goodkey) -> 200. Total 2 requests.
        assert_eq!(req_count.load(Ordering::SeqCst), 2);
        // Counter should have been incremented exactly once
        assert_eq!(pipeline.counter.load(Ordering::SeqCst), 1);

        server_handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn test_adaptive_failover_to_backup() -> Result<()> {
        let (mock_url, server_handle, req_count) = run_mock_server().await?;

        let config = LLMConfig {
            providers: vec![
                Provider {
                    name: "GeminiPrimary".to_string(),
                    r#type: ProviderType::Gemini,
                    endpoint: mock_url.clone(),
                    model: "gemini-1.5-flash".to_string(),
                    api_key: Some("badkey".to_string()),
                    priority: 1,
                },
                Provider {
                    name: "OllamaBackup".to_string(),
                    r#type: ProviderType::Ollama,
                    endpoint: mock_url,
                    model: "llama3".to_string(),
                    api_key: None,
                    priority: 2,
                },
            ],
            extraction: ExtractionConfig {
                chunk_size: 1024,
                max_concurrency: 1,
                concurrency: None,
            },
            api_keys: vec![], // Empty global keys, fallback to provider api_key
            memory: MemoryConfig::default(),
        };

        let pipeline = AutoRotatePipeline::new(config);
        let res = pipeline.extract_semantic_link("hello").await?;

        assert_eq!(res, "{\"links\": []}");
        // Request 1: Gemini (badkey) -> 429 (no more keys to rotate, so fails)
        // Request 2: Ollama (OllamaBackup) -> 200 OK. Total 2 requests.
        assert_eq!(req_count.load(Ordering::SeqCst), 2);

        server_handle.abort();
        Ok(())
    }

    #[tokio::test]
    async fn test_qdrant_memory_store() -> Result<()> {
        let (mock_url, server_handle, _) = run_mock_server().await?;

        let config = LLMConfig {
            providers: vec![],
            extraction: ExtractionConfig {
                chunk_size: 1024,
                max_concurrency: 1,
                concurrency: None,
            },
            api_keys: vec![],
            memory: crate::config::MemoryConfig {
                short_term: crate::config::ShortTermMemoryConfig::default(),
                long_term: crate::config::LongTermMemoryConfig {
                    enabled: true,
                    provider: "qdrant".to_string(),
                    embedding: crate::config::EmbeddingConfig {
                        provider: "ollama".to_string(),
                        endpoint: mock_url.clone(),
                        model: "bge-m3".to_string(),
                        vector_size: 1024,
                    },
                    qdrant: crate::config::QdrantConfig {
                        url: mock_url,
                        api_key: None,
                        collection: "graphify_memory".to_string(),
                        distance: "Cosine".to_string(),
                    },
                    index_kinds: vec!["module".to_string(), "class".to_string(), "struct".to_string(), "trait".to_string(), "interface".to_string()],
                },
            },
        };

        let store = crate::memory::QdrantMemoryStore::new(config);
        
        // 1. Ensure collection check/create
        store.ensure_collection().await?;

        // 2. Upsert nodes
        let nodes = vec![graphify_core::Node {
            id: graphify_core::NodeId("test_node".to_string()),
            label: "test_label".to_string(),
            file_type: graphify_core::FileType::Code,
            kind: "function".to_string(),
            language: "rust".to_string(),
            source_file: "lib.rs".to_string(),
            start_line: 1,
            end_line: 10,
            doc_comment: Some("Doc text".to_string()),
            description: Some("Desc text".to_string()),
            metadata: None,
        }];
        store.upsert_nodes(&nodes).await?;

        // 3. Query similar nodes
        let results = store.query_similar_nodes("query", 5).await?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["payload"]["node_id"], "test_node_id");

        server_handle.abort();
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires physical homelab connectivity"]
    async fn test_real_homelab_memory_store() -> Result<()> {
        let config_path = std::path::PathBuf::from("/home/zeng/.config/graphify/config.toml");
        if !config_path.exists() {
            println!("Skipping real homelab test because config.toml doesn't exist");
            return Ok(());
        }
        let config_str = std::fs::read_to_string(config_path)?;
        let mut config: crate::config::LLMConfig = toml::from_str(&config_str)?;
        
        // Force-enable for integration test
        config.memory.long_term.enabled = true;

        let store = crate::memory::QdrantMemoryStore::new(config);
        
        println!("Checking real homelab connection and collection auto-creation...");
        store.ensure_collection().await?;
        
        println!("Embedding dummy node via Ollama and upserting into Qdrant...");
        let nodes = vec![graphify_core::Node {
            id: graphify_core::NodeId("test_real_node".to_string()),
            label: "test_real_label".to_string(),
            file_type: graphify_core::FileType::Code,
            kind: "function".to_string(),
            language: "rust".to_string(),
            source_file: "lib.rs".to_string(),
            start_line: 1,
            end_line: 10,
            doc_comment: Some("Testing real homelab connection with Qdrant and Ollama".to_string()),
            description: Some("Provides physical validation of local network setup".to_string()),
            metadata: None,
        }];
        store.upsert_nodes(&nodes).await?;
        
        println!("Performing semantic query against Qdrant...");
        let results = store.query_similar_nodes("physical validation", 5).await?;
        println!("Real search results returned: {} items", results.len());
        
        Ok(())
    }
}
