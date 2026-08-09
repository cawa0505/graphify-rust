// ponytail: allow missing errors doc as these are internal library functions propagating anyhow::Result
#![allow(clippy::missing_errors_doc)]
// ponytail: allow uninlined format args to support older toolchains or dynamic string generation
#![allow(clippy::uninlined_format_args)]
// ponytail: allow use_self as keeping structure names explicit improves readability
#![allow(clippy::use_self)]
// ponytail: allow missing const/must_use/redundant closures for simplified functional pipeline operations
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::redundant_closure_for_method_calls)]

pub mod config;
pub mod gateway;
pub mod gbnf;
pub mod pipeline;

pub use config::{LLMConfig, PluginConfig, PluginsConfig, Provider, ProviderType};
pub use gateway::{ChatMessage, CoreLlmProvider, LlmError, PluginContext};
pub use gbnf::get_json_schema_gbnf;
pub use pipeline::AutoRotatePipeline;

// Compatibility re-exports: memory infrastructure now lives in graphify-memory.
pub use graphify_memory::{
    EmbeddingConfig, LongTermMemoryConfig, MemoryConfig, MemoryNode, MemoryQueryInput,
    MemoryQueryResult, MemorySearcher, PluginDomainMemory, QdrantConfig, QdrantMemoryStore,
    ShortTermMemoryConfig, plugin_collection_name,
};
