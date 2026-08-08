// ponytail: allow missing errors doc as these are internal library functions and we use anyhow::Result to propagate errors up to the caller
#![allow(clippy::missing_errors_doc)]
// ponytail: allow uninlined format args to support older rust/clippy toolchains or legacy python-compatible style
#![allow(clippy::uninlined_format_args)]
// ponytail: allow collapsible_if for cleaner matching of tree-sitter AST node structures
#![allow(clippy::collapsible_if)]
// ponytail: allow too_many_lines as AST tree traversal handles a wide variety of token kinds in single match blocks
#![allow(clippy::too_many_lines)]
// ponytail: allow implicit_hasher as we use std HashMap with default SipHash 1-3
#![allow(clippy::implicit_hasher)]

pub mod extract;
pub mod graph;
pub mod plugin;
pub mod toon;
pub mod types;

pub use extract::extract_file;
pub use graph::{build_graph, find_shortest_path, query_bfs};
pub use plugin::{GraphifyPlugin, WorkspaceContext};
pub use toon::{from_toon, to_toon};
pub use types::{Edge, ExtractionResult, FileType, GraphMetadata, GraphOutput, Node, NodeId};
