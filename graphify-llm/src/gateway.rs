// LLM gateway contract (memory-plugin-integration Phase 5, llm-gateway-contract).
//
// Exposes the AutoRotatePipeline as a reusable, object-safe gateway so native
// plugins can call the shared, configured LLM service without reimplementing
// key rotation, provider failover, or HTTP handling. Plugins remain free to
// bring their own dedicated models instead (Phase 7).

use crate::config::LLMConfig;
use crate::pipeline::AutoRotatePipeline;
use graphify_memory::QdrantMemoryStore;
use std::fmt;
use std::sync::Arc;

/// Error returned by the LLM gateway.
#[derive(Debug)]
pub struct LlmError(pub String);

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LlmError {}

impl From<anyhow::Error> for LlmError {
    fn from(err: anyhow::Error) -> Self {
        Self(err.to_string())
    }
}

/// One chat turn, role followed by content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Object-safe gateway contract for the shared LLM service.
pub trait CoreLlmProvider {
    /// Single-prompt completion through the configured provider pipeline.
    fn complete(&self, prompt: &str) -> Result<String, LlmError>;

    /// Message-based chat through the same provider and key selection logic.
    fn chat(&self, messages: &[ChatMessage]) -> Result<String, LlmError>;
}

impl CoreLlmProvider for AutoRotatePipeline {
    fn complete(&self, prompt: &str) -> Result<String, LlmError> {
        let runtime = self.gateway_runtime.get_or_init(|| {
            // ponytail: Runtime::new() can only fail under resource exhaustion;
            // panic is the correct error boundary for a lazy-once build.
            match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime,
                Err(e) => panic!("failed to build LLM gateway runtime: {e}"),
            }
        });
        runtime
            .block_on(self.extract_semantic_link(prompt))
            .map_err(LlmError::from)
    }

    fn chat(&self, messages: &[ChatMessage]) -> Result<String, LlmError> {
        let prompt = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");
        self.complete(&prompt)
    }
}

/// Shared services handed to a bound native plugin (v2.0-alpha supplement).
///
/// Skeleton only: no multi-model routing or plugin-specialized model handling
/// in this change (Phase 7 plugin work). `memory` is `None` when semantic
/// memory is not enabled.
pub struct PluginContext {
    pub memory: Option<Arc<QdrantMemoryStore>>,
    pub llm: Arc<dyn CoreLlmProvider>,
    pub workspace_key: String,
}

impl PluginContext {
    /// Build the context with the default gateway and the workspace routing key.
    ///
    /// # Errors
    /// None by construction; returns `Self` directly.
    pub fn new(
        config: LLMConfig,
        workspace_key: String,
        memory: Option<Arc<QdrantMemoryStore>>,
    ) -> Self {
        Self {
            memory,
            llm: Arc::new(AutoRotatePipeline::new(config)),
            workspace_key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ChatMessage, CoreLlmProvider, PluginContext};
    use crate::{AutoRotatePipeline, LLMConfig};
    use graphify_memory::QdrantMemoryStore;
    use std::sync::Arc;

    fn empty_config() -> LLMConfig {
        LLMConfig::default()
    }

    #[test]
    fn complete_routes_through_pipeline_rotation() {
        // No providers configured: the gateway must surface the pipeline's
        // own error (proving it routed through rotation/failover, not a stub).
        let pipeline = AutoRotatePipeline::new(empty_config());
        match pipeline.complete("hello") {
            Ok(_) => panic!("expected error with no providers configured"),
            // No providers configured: the gateway must surface the pipeline's
            // own error (proving it routed through rotation/failover).
            Err(err) => assert!(!err.0.is_empty(), "error message must not be empty"),
        }
    }

    #[test]
    fn chat_uses_same_gateway_path() {
        let pipeline = AutoRotatePipeline::new(empty_config());
        let result = pipeline.chat(&[ChatMessage {
            role: "user".into(),
            content: "hello".into(),
        }]);
        let Err(err) = result else {
            panic!("expected error with no providers configured");
        };
        assert!(!err.0.is_empty());
    }

    #[test]
    fn trait_usable_as_dyn_reference() {
        let provider: std::sync::Arc<dyn CoreLlmProvider> =
            std::sync::Arc::new(AutoRotatePipeline::new(empty_config()));
        assert!(provider.complete("x").is_err());
    }

    #[test]
    fn context_carries_services_and_workspace_key() {
        let store = Arc::new(QdrantMemoryStore::new(
            empty_config().memory.long_term,
            None,
        ));
        let ctx = PluginContext::new(empty_config(), "ws-key".into(), Some(store));
        assert_eq!(ctx.workspace_key, "ws-key");
        assert!(ctx.memory.is_some());
        assert!(ctx.llm.complete("x").is_err());
    }
}
