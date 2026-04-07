//! Tool specifications for the mol-engine agentic turn loop.
//!
//! Each tool has a name, description, and JSON-Schema `input_schema` that is
//! passed to the LLM via the API `tools` field — NOT embedded in the system
//! prompt.

use serde_json::{Value, json};

/// A single tool specification sent to the LLM API.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    /// Tool name (e.g. "bash", "read_file").
    pub name: &'static str,
    /// Human-readable description shown to the LLM.
    pub description: &'static str,
    /// JSON-Schema object for the tool's parameters.
    pub parameters: Value,
}

impl ToolSpec {
    /// Serialize to the OpenAI function-calling format.
    pub fn to_api_tool(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }
        })
    }
}

/// Build all default tool specs.
pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "bash",
            description: "Execute a shell command in the experiment workspace. \
                Use for running scripts, installing packages, checking output, etc. \
                Commands run with a timeout. Prefer short, targeted commands.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute."
                    },
                    "timeout": {
                        "type": "integer",
                        "minimum": 1,
                        "default": 60,
                        "description": "Timeout in seconds (default 60)."
                    },
                    "description": {
                        "type": "string",
                        "description": "Brief description of what this command does (5-10 words)."
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "read_file",
            description: "Read a text file from the workspace or allowed data directories. \
                Returns numbered lines. Use offset/limit for large files.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to read."
                    },
                    "offset": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Line offset to start reading from (0-based)."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Max number of lines to return."
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "write_file",
            description: "Write a text file in the workspace. Creates parent directories if needed. \
                Use for creating new experiment files.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to write."
                    },
                    "content": {
                        "type": "string",
                        "description": "Full content to write to the file."
                    }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "edit_file",
            description: "Replace text in an existing workspace file. Finds old_string and replaces \
                with new_string. Use for targeted fixes instead of rewriting entire files.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to edit."
                    },
                    "old_string": {
                        "type": "string",
                        "description": "Exact text to find and replace."
                    },
                    "new_string": {
                        "type": "string",
                        "description": "Replacement text."
                    },
                    "replace_all": {
                        "type": "boolean",
                        "default": false,
                        "description": "Replace all occurrences (default: first only)."
                    }
                },
                "required": ["path", "old_string", "new_string"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "glob_search",
            description: "Find files by glob pattern in the workspace or data directories. \
                Returns matching file paths sorted by modification time (newest first). \
                Capped at 200 results.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern (e.g. '**/*.py', '*.yaml')."
                    },
                    "path": {
                        "type": "string",
                        "description": "Base directory to search in (default: workspace root)."
                    }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "grep_search",
            description: "Search file contents with a regex pattern. Returns matching lines with \
                file paths and line numbers. Use for finding function definitions, imports, \
                variable usages in codebases.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for."
                    },
                    "path": {
                        "type": "string",
                        "description": "File or directory to search in (default: workspace root)."
                    },
                    "glob": {
                        "type": "string",
                        "description": "File glob filter (e.g. '*.py')."
                    },
                    "context": {
                        "type": "integer",
                        "minimum": 0,
                        "default": 2,
                        "description": "Number of context lines before and after each match."
                    }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }),
        },
    ]
}

/// The set of valid tool names, used for fallback recovery parsing.
pub const TOOL_NAMES: &[&str] = &[
    "bash",
    "read_file",
    "write_file",
    "edit_file",
    "glob_search",
    "grep_search",
];

/// Returns true if the given name is a known tool.
pub fn is_known_tool(name: &str) -> bool {
    TOOL_NAMES.contains(&name)
}
