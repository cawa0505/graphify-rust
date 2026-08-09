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
pub mod memory;
pub mod plugin_memory;

pub use config::{
    EmbeddingConfig, LongTermMemoryConfig, MemoryConfig, QdrantConfig, ShortTermMemoryConfig,
};
pub use memory::{
    MemoryNode, MemoryQueryInput, MemoryQueryResult, MemorySearcher, QdrantMemoryStore,
};
pub use plugin_memory::{PluginDomainMemory, plugin_collection_name};
