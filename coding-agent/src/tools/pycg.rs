use std::process::Stdio;
use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture, ToolUpdateFn};
use ai::types::UserBlock;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Shared helper
// ---------------------------------------------------------------------------

/// Invoke pycg with the given subcommand and args, parse JSON output.
///
/// Builds: `pycg --root <root> --format json <subcommand> <args...>`
/// Returns parsed JSON on success, or an error containing stderr on failure.
pub async fn pycg_invoke(subcommand: &str, args: &[&str], root: &str) -> Result<Value> {
    let mut cmd_args = vec!["--root", root, "--format", "json", subcommand];
    cmd_args.extend_from_slice(args);

    let mut child = tokio::process::Command::new("pycg")
        .args(&cmd_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("failed to launch pycg: {}", e))?;

    let mut stdout = child.stdout.take().expect("stdout piped");
    let mut stderr = child.stderr.take().expect("stderr piped");

    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).await.ok();
        buf
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        stderr.read_to_end(&mut buf).await.ok();
        buf
    });

    let status = child.wait().await?;
    let stdout_bytes = stdout_task.await.unwrap_or_default();
    let stderr_bytes = stderr_task.await.unwrap_or_default();

    if !status.success() {
        let stderr_str = String::from_utf8_lossy(&stderr_bytes);
        return Err(anyhow!(
            "pycg exited with status {}: {}",
            status.code().unwrap_or(-1),
            stderr_str.trim()
        ));
    }

    let json_str = String::from_utf8_lossy(&stdout_bytes);
    serde_json::from_str(&json_str).map_err(|e| {
        anyhow!(
            "pycg returned invalid JSON: {} (output: {})",
            e,
            json_str.trim()
        )
    })
}

fn format_json_output(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn current_root() -> String {
    std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .to_string_lossy()
        .to_string()
}

// ---------------------------------------------------------------------------
// CgSymbolsTool
// ---------------------------------------------------------------------------

pub struct CgSymbolsTool;

impl CgSymbolsTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(Self)
    }
}

impl AgentTool for CgSymbolsTool {
    fn name(&self) -> &str {
        "cg_symbols"
    }

    fn label(&self) -> &str {
        "CgSymbols"
    }

    fn description(&self) -> &str {
        "List symbols (functions, methods, classes) defined in a Python file or module."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "File path or module name"
                    }
                },
                "required": ["target"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        Box::pin(async move {
            let target = params["target"]
                .as_str()
                .ok_or_else(|| anyhow!("missing 'target' parameter"))?
                .to_string();
            let root = current_root();

            let result = pycg_invoke("symbols-in", &[&target, "."], &root).await?;

            let symbol_count = result.as_array().map(|a| a.len()).unwrap_or(0);
            let text = if symbol_count == 0 {
                format!("No symbols found in '{}'.", target)
            } else {
                format!(
                    "Symbols in '{}':\n```json\n{}\n```",
                    target,
                    format_json_output(&result)
                )
            };

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text }],
                details: Some(json!({
                    "target": target,
                    "symbol_count": symbol_count
                })),
            })
        })
    }
}

// ---------------------------------------------------------------------------
// CgCallersTool
// ---------------------------------------------------------------------------

pub struct CgCallersTool;

impl CgCallersTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(Self)
    }
}

impl AgentTool for CgCallersTool {
    fn name(&self) -> &str {
        "cg_callers"
    }

    fn label(&self) -> &str {
        "CgCallers"
    }

    fn description(&self) -> &str {
        "Find all functions that call a given symbol."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "Fully qualified symbol name or suffix"
                    },
                    "match_mode": {
                        "type": "string",
                        "enum": ["exact", "suffix"],
                        "default": "suffix"
                    }
                },
                "required": ["symbol"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        Box::pin(async move {
            let symbol = params["symbol"]
                .as_str()
                .ok_or_else(|| anyhow!("missing 'symbol' parameter"))?
                .to_string();
            let mode = params["match_mode"]
                .as_str()
                .unwrap_or("suffix")
                .to_string();
            let root = current_root();

            let mut args = vec![symbol.as_str(), "."];
            let match_flag;
            if mode == "suffix" {
                match_flag = "--match".to_string();
                args.push(&match_flag);
                args.push("suffix");
            }

            let result = pycg_invoke("callers", &args, &root).await?;

            let count = result.as_array().map(|a| a.len()).unwrap_or(0);
            let text = if count == 0 {
                format!("No callers found for '{}'.", symbol)
            } else {
                format!(
                    "Callers of '{}':\n```json\n{}\n```",
                    symbol,
                    format_json_output(&result)
                )
            };

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text }],
                details: Some(json!({
                    "symbol": symbol,
                    "match_mode": mode,
                    "result_count": count
                })),
            })
        })
    }
}

// ---------------------------------------------------------------------------
// CgCalleesTool
// ---------------------------------------------------------------------------

pub struct CgCalleesTool;

impl CgCalleesTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(Self)
    }
}

impl AgentTool for CgCalleesTool {
    fn name(&self) -> &str {
        "cg_callees"
    }

    fn label(&self) -> &str {
        "CgCallees"
    }

    fn description(&self) -> &str {
        "Find all functions called by a given symbol."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "Fully qualified symbol name or suffix"
                    },
                    "match_mode": {
                        "type": "string",
                        "enum": ["exact", "suffix"],
                        "default": "suffix"
                    }
                },
                "required": ["symbol"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        Box::pin(async move {
            let symbol = params["symbol"]
                .as_str()
                .ok_or_else(|| anyhow!("missing 'symbol' parameter"))?
                .to_string();
            let mode = params["match_mode"]
                .as_str()
                .unwrap_or("suffix")
                .to_string();
            let root = current_root();

            let mut args = vec![symbol.as_str(), "."];
            let match_flag;
            if mode == "suffix" {
                match_flag = "--match".to_string();
                args.push(&match_flag);
                args.push("suffix");
            }

            let result = pycg_invoke("callees", &args, &root).await?;

            let count = result.as_array().map(|a| a.len()).unwrap_or(0);
            let text = if count == 0 {
                format!("No callees found for '{}'.", symbol)
            } else {
                format!(
                    "Callees of '{}':\n```json\n{}\n```",
                    symbol,
                    format_json_output(&result)
                )
            };

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text }],
                details: Some(json!({
                    "symbol": symbol,
                    "match_mode": mode,
                    "result_count": count
                })),
            })
        })
    }
}

// ---------------------------------------------------------------------------
// CgPathTool
// ---------------------------------------------------------------------------

pub struct CgPathTool;

impl CgPathTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(Self)
    }
}

impl AgentTool for CgPathTool {
    fn name(&self) -> &str {
        "cg_path"
    }

    fn label(&self) -> &str {
        "CgPath"
    }

    fn description(&self) -> &str {
        "Find call chains between two symbols."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Source symbol (caller side)"
                    },
                    "target": {
                        "type": "string",
                        "description": "Target symbol (callee side)"
                    },
                    "match_mode": {
                        "type": "string",
                        "enum": ["exact", "suffix"],
                        "default": "suffix"
                    }
                },
                "required": ["source", "target"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        Box::pin(async move {
            let source = params["source"]
                .as_str()
                .ok_or_else(|| anyhow!("missing 'source' parameter"))?
                .to_string();
            let target = params["target"]
                .as_str()
                .ok_or_else(|| anyhow!("missing 'target' parameter"))?
                .to_string();
            let mode = params["match_mode"]
                .as_str()
                .unwrap_or("suffix")
                .to_string();
            let root = current_root();

            let mut args = vec![source.as_str(), target.as_str(), "."];
            let match_flag;
            if mode == "suffix" {
                match_flag = "--match".to_string();
                args.push(&match_flag);
                args.push("suffix");
            }

            let result = pycg_invoke("path", &args, &root).await?;

            let paths_found = result.as_array().map(|a| a.len()).unwrap_or(0);
            let text = if paths_found == 0 {
                format!("No call paths found from '{}' to '{}'.", source, target)
            } else {
                format!(
                    "Call paths from '{}' to '{}':\n```json\n{}\n```",
                    source,
                    target,
                    format_json_output(&result)
                )
            };

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text }],
                details: Some(json!({
                    "source": source,
                    "target": target,
                    "paths_found": paths_found
                })),
            })
        })
    }
}

// ---------------------------------------------------------------------------
// CgNeighborsTool
// ---------------------------------------------------------------------------

pub struct CgNeighborsTool;

impl CgNeighborsTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(Self)
    }
}

impl AgentTool for CgNeighborsTool {
    fn name(&self) -> &str {
        "cg_neighbors"
    }

    fn label(&self) -> &str {
        "CgNeighbors"
    }

    fn description(&self) -> &str {
        "List both callers and callees of a given symbol."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "Fully qualified symbol name or suffix"
                    },
                    "match_mode": {
                        "type": "string",
                        "enum": ["exact", "suffix"],
                        "default": "suffix"
                    }
                },
                "required": ["symbol"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        Box::pin(async move {
            let symbol = params["symbol"]
                .as_str()
                .ok_or_else(|| anyhow!("missing 'symbol' parameter"))?
                .to_string();
            let mode = params["match_mode"]
                .as_str()
                .unwrap_or("suffix")
                .to_string();
            let root = current_root();

            let mut args = vec![symbol.as_str(), "."];
            let match_flag;
            if mode == "suffix" {
                match_flag = "--match".to_string();
                args.push(&match_flag);
                args.push("suffix");
            }

            let result = pycg_invoke("neighbors", &args, &root).await?;

            let caller_count = result["callers"].as_array().map(|a| a.len()).unwrap_or(0);
            let callee_count = result["callees"].as_array().map(|a| a.len()).unwrap_or(0);

            let text = if caller_count == 0 && callee_count == 0 {
                format!("No neighbors found for '{}'.", symbol)
            } else {
                format!(
                    "Neighbors of '{}':\n```json\n{}\n```",
                    symbol,
                    format_json_output(&result)
                )
            };

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text }],
                details: Some(json!({
                    "symbol": symbol,
                    "caller_count": caller_count,
                    "callee_count": callee_count
                })),
            })
        })
    }
}

// ---------------------------------------------------------------------------
// CgSummaryTool
// ---------------------------------------------------------------------------

pub struct CgSummaryTool;

impl CgSummaryTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(Self)
    }
}

impl AgentTool for CgSummaryTool {
    fn name(&self) -> &str {
        "cg_summary"
    }

    fn label(&self) -> &str {
        "CgSummary"
    }

    fn description(&self) -> &str {
        "Aggregate call graph statistics for a file or module."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "File path or module name"
                    }
                },
                "required": ["target"]
            })
        })
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<ToolUpdateFn>,
    ) -> BoxFuture<Result<AgentToolResult>> {
        Box::pin(async move {
            let target = params["target"]
                .as_str()
                .ok_or_else(|| anyhow!("missing 'target' parameter"))?
                .to_string();
            let root = current_root();

            let result = pycg_invoke("summary", &[target.as_str(), "."], &root).await?;

            let functions_analyzed = result["functions"]
                .as_array()
                .map(|a| a.len())
                .or_else(|| result["function_count"].as_u64().map(|n| n as usize))
                .unwrap_or(0);

            let text = format!(
                "Call graph summary for '{}':\n```json\n{}\n```",
                target,
                format_json_output(&result)
            );

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text }],
                details: Some(json!({
                    "target": target,
                    "functions_analyzed": functions_analyzed
                })),
            })
        })
    }
}
