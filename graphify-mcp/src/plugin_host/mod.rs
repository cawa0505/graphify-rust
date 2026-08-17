//! Third-party MCP plugin subprocess hosting (plugin-scan-v1).
//!
//! graphify-mcp plays the Mode 1 Gateway role: it spawns third-party MCP
//! plugin servers as subprocesses, aggregates their tools under the
//! `graphify_plugin_<id>_<tool>` namespace, and forwards tool calls.

pub mod breaker;
pub mod framing;
pub mod host;
pub mod process;
