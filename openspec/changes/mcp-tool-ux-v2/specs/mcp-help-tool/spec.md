## Purpose

Provide a discovery entry point for all Graphify MCP tools, listing available tools with descriptions so AI agents can self-discover available capabilities without external documentation.

## ADDED Requirements

### Requirement: Help tool listing all tools

The server SHALL expose a `graphify_help` tool (or equivalent named entry point) that returns a categorized list of all available MCP tools, their names, a one-line description, and their expected parameter signatures.

#### Scenario: Agent requests tool listing

- **WHEN** an agent invokes the help tool
- **THEN** the server SHALL return a list of every registered MCP tool, each with its name, description, and required parameters
- **AND** the tools SHALL be grouped by domain (graph, memory, plugin, coverage, relay, etc.)

#### Scenario: Help tool itself is discoverable

- **WHEN** an agent connects to the MCP server for the first time
- **THEN** the help tool SHALL be listed first among the server's capabilities
- **AND** its description SHALL clearly indicate it is the discovery entry point

### Requirement: Human-readable descriptions

Each tool SHALL carry a concise, human-readable description (one to two sentences) that explains what the tool does, not just its implementation mechanics.

#### Scenario: Description helps agent choose tools

- **WHEN** an agent uses the help tool to decide which tool to call
- **THEN** each tool description SHALL explain the tool's purpose in a way that allows the agent to determine relevance without calling it first