use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture, ToolUpdateFn};
use ai::types::UserBlock;
use anyhow::Result;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

/// Invoke pycfg with the given flags and targets, parse JSON output.
fn pycfg_invoke(flags: &[&str], targets: &[&str]) -> Result<Value> {
    let mut cmd = std::process::Command::new("pycfg");
    cmd.arg("--format").arg("json");
    for flag in flags {
        cmd.arg(flag);
    }
    for target in targets {
        cmd.arg(target);
    }

    let output = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run pycfg: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(anyhow::anyhow!("{}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).map_err(|e| {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        anyhow::anyhow!(
            "pycfg returned invalid JSON: {}\nstderr: {}",
            e,
            stderr.trim()
        )
    })
}

// ---------------------------------------------------------------------------
// CfgFunctionsTool
// ---------------------------------------------------------------------------

pub struct CfgFunctionsTool;

impl CfgFunctionsTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(Self)
    }
}

impl AgentTool for CfgFunctionsTool {
    fn name(&self) -> &str {
        "cfg_functions"
    }

    fn label(&self) -> &str {
        "CfgFunctions"
    }

    fn description(&self) -> &str {
        "List function names discovered in a Python file or directory."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "target": { "type": "string", "description": "Python file path or directory" }
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
                .ok_or_else(|| anyhow::anyhow!("missing 'target' parameter"))?
                .to_string();

            let result = tokio::task::spawn_blocking({
                let target = target.clone();
                move || pycfg_invoke(&["--list-functions"], &[target.as_str()])
            })
            .await??;

            // Count functions across all files
            let function_count: usize = result["files"]
                .as_array()
                .map(|files| {
                    files
                        .iter()
                        .filter_map(|f| f["functions"].as_array())
                        .map(|fns| fns.len())
                        .sum()
                })
                .unwrap_or(0);

            let text = if function_count == 0 {
                format!("No functions found in '{}'.", target)
            } else {
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
            };

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text }],
                details: Some(json!({
                    "target": target,
                    "function_count": function_count
                })),
            })
        })
    }
}

// ---------------------------------------------------------------------------
// CfgSummaryTool
// ---------------------------------------------------------------------------

pub struct CfgSummaryTool;

impl CfgSummaryTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(Self)
    }
}

impl AgentTool for CfgSummaryTool {
    fn name(&self) -> &str {
        "cfg_summary"
    }

    fn label(&self) -> &str {
        "CfgSummary"
    }

    fn description(&self) -> &str {
        "Per-function metrics: complexity, branches, exits for functions in a file or directory."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "target": { "type": "string", "description": "Python file path, directory, or file::FunctionName target" }
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
                .ok_or_else(|| anyhow::anyhow!("missing 'target' parameter"))?
                .to_string();

            let result = tokio::task::spawn_blocking({
                let target = target.clone();
                move || pycfg_invoke(&["--summary"], &[target.as_str()])
            })
            .await??;

            // Prefer the totals.functions count; fall back to counting from files array
            let functions_analyzed: usize = result["totals"]["functions"]
                .as_u64()
                .map(|n| n as usize)
                .unwrap_or_else(|| {
                    result["files"]
                        .as_array()
                        .map(|files| {
                            files
                                .iter()
                                .filter_map(|f| f["functions"].as_array())
                                .map(|fns| fns.len())
                                .sum()
                        })
                        .unwrap_or(0)
                });

            let text = if functions_analyzed == 0 {
                format!("No functions found in '{}'.", target)
            } else {
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
            };

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

// ---------------------------------------------------------------------------
// CfgGraphTool
// ---------------------------------------------------------------------------

pub struct CfgGraphTool;

impl CfgGraphTool {
    pub fn arc() -> Arc<dyn AgentTool> {
        Arc::new(Self)
    }
}

impl AgentTool for CfgGraphTool {
    fn name(&self) -> &str {
        "cfg_graph"
    }

    fn label(&self) -> &str {
        "CfgGraph"
    }

    fn description(&self) -> &str {
        "Full control flow graph for a specific function. Use file.py::FunctionName syntax to target a specific function."
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "target": { "type": "string", "description": "file.py::QualifiedName target for the function to analyze" }
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
                .ok_or_else(|| anyhow::anyhow!("missing 'target' parameter"))?
                .to_string();

            let result = tokio::task::spawn_blocking({
                let target = target.clone();
                move || pycfg_invoke(&[], &[target.as_str()])
            })
            .await??;

            // Count nodes (blocks) and edges across all functions in all files
            let (node_count, edge_count) = result["files"]
                .as_array()
                .map(|files| {
                    files
                        .iter()
                        .filter_map(|f| f["functions"].as_array())
                        .flatten()
                        .fold((0usize, 0usize), |(nodes, edges), func| {
                            let fn_nodes = func["blocks"]
                                .as_array()
                                .map(|b| b.len())
                                .unwrap_or_else(|| {
                                    func["metrics"]["blocks"].as_u64().unwrap_or(0) as usize
                                });
                            let fn_edges = func["metrics"]["edges"].as_u64().unwrap_or(0) as usize;
                            (nodes + fn_nodes, edges + fn_edges)
                        })
                })
                .unwrap_or((0, 0));

            let text = if node_count == 0 {
                format!("No CFG data found for '{}'.", target)
            } else {
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
            };

            Ok(AgentToolResult {
                content: vec![UserBlock::Text { text }],
                details: Some(json!({
                    "target": target,
                    "nodes": node_count,
                    "edges": edge_count
                })),
            })
        })
    }
}
