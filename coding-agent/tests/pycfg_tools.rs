//! Tests for pycfg tool wrappers.
//!
//! These tests invoke the real pycfg binary. pycfg must be available on PATH
//! (expected at /Users/tau/.cargo/bin/pycfg or equivalent).

use agent::types::AgentTool;
use ai::types::UserBlock;
use coding_agent::tools::{CfgFunctionsTool, CfgGraphTool, CfgSummaryTool};
use serde_json::json;
use tempfile::TempDir;

/// Returns true if pycfg is available on PATH.
fn has_pycfg() -> bool {
    std::process::Command::new("pycfg")
        .arg("--version")
        .output()
        .is_ok()
}

fn text_content(result: &agent::types::AgentToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|b| match b {
            UserBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Write a simple Python file with known functions into a TempDir.
fn simple_python_file(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("sample.py");
    std::fs::write(
        &path,
        r#"def add(a, b):
    return a + b

def greet(name):
    if name:
        return f"Hello, {name}!"
    return "Hello!"
"#,
    )
    .unwrap();
    path
}

// ---------------------------------------------------------------------------
// INV-1: All tools have correct names and valid JSON Schema parameters
// ---------------------------------------------------------------------------

#[test]
fn test_cfg_functions_tool_name_and_schema() {
    let tool = CfgFunctionsTool;
    assert_eq!(tool.name(), "cfg_functions");

    let schema = tool.parameters();
    assert_eq!(schema["type"], "object");
    let required = &schema["required"];
    assert!(
        required
            .as_array()
            .map(|a| a.iter().any(|v| v == "target"))
            .unwrap_or(false),
        "cfg_functions schema must require 'target'"
    );
    assert!(schema["properties"]["target"].is_object());
}

#[test]
fn test_cfg_summary_tool_name_and_schema() {
    let tool = CfgSummaryTool;
    assert_eq!(tool.name(), "cfg_summary");

    let schema = tool.parameters();
    assert_eq!(schema["type"], "object");
    let required = &schema["required"];
    assert!(
        required
            .as_array()
            .map(|a| a.iter().any(|v| v == "target"))
            .unwrap_or(false),
        "cfg_summary schema must require 'target'"
    );
    assert!(schema["properties"]["target"].is_object());
}

#[test]
fn test_cfg_graph_tool_name_and_schema() {
    let tool = CfgGraphTool;
    assert_eq!(tool.name(), "cfg_graph");

    let schema = tool.parameters();
    assert_eq!(schema["type"], "object");
    let required = &schema["required"];
    assert!(
        required
            .as_array()
            .map(|a| a.iter().any(|v| v == "target"))
            .unwrap_or(false),
        "cfg_graph schema must require 'target'"
    );
    assert!(schema["properties"]["target"].is_object());
}

// ---------------------------------------------------------------------------
// INV-2: pycfg_invoke builds correct CLI arguments for each mode
// (verified indirectly: each mode returns distinct, mode-appropriate output)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cfg_functions_returns_function_list() {
    if !has_pycfg() {
        eprintln!("skipping: pycfg not found on PATH");
        return;
    }
    let dir = TempDir::new().unwrap();
    let py = simple_python_file(&dir);
    let target = py.to_str().unwrap();

    let tool = CfgFunctionsTool;
    let result = tool
        .execute("id1".into(), json!({ "target": target }), None, None)
        .await
        .expect("cfg_functions should succeed on valid Python file");

    let out = text_content(&result);

    // Must list "add" and "greet"
    assert!(
        out.contains("add"),
        "should find function 'add', got:\n{out}"
    );
    assert!(
        out.contains("greet"),
        "should find function 'greet', got:\n{out}"
    );

    // Details must include target and function_count
    let details = result.details.as_ref().expect("details must be present");
    assert_eq!(details["target"].as_str(), Some(target));
    assert_eq!(details["function_count"].as_u64(), Some(2));
}

#[tokio::test]
async fn test_cfg_summary_returns_per_function_metrics() {
    if !has_pycfg() {
        eprintln!("skipping: pycfg not found on PATH");
        return;
    }
    let dir = TempDir::new().unwrap();
    let py = simple_python_file(&dir);
    let target = py.to_str().unwrap();

    let tool = CfgSummaryTool;
    let result = tool
        .execute("id2".into(), json!({ "target": target }), None, None)
        .await
        .expect("cfg_summary should succeed on valid Python file");

    let out = text_content(&result);

    // Output should include complexity metrics
    assert!(
        out.contains("cyclomatic_complexity") || out.contains("complexity"),
        "summary should include complexity metrics, got:\n{out}"
    );
    assert!(
        out.contains("add"),
        "summary should mention function 'add', got:\n{out}"
    );

    // Details must include target and functions_analyzed
    let details = result.details.as_ref().expect("details must be present");
    assert_eq!(details["target"].as_str(), Some(target));
    let count = details["functions_analyzed"]
        .as_u64()
        .expect("functions_analyzed must be present");
    assert_eq!(count, 2, "should analyze both functions");
}

#[tokio::test]
async fn test_cfg_graph_returns_full_cfg() {
    if !has_pycfg() {
        eprintln!("skipping: pycfg not found on PATH");
        return;
    }
    let dir = TempDir::new().unwrap();
    let py = simple_python_file(&dir);
    let target = format!("{}::greet", py.to_str().unwrap());

    let tool = CfgGraphTool;
    let result = tool
        .execute("id3".into(), json!({ "target": target }), None, None)
        .await
        .expect("cfg_graph should succeed on valid function target");

    let out = text_content(&result);

    // Full CFG output includes blocks with successors
    assert!(
        out.contains("blocks") || out.contains("successors"),
        "cfg_graph should include block/successor data, got:\n{out}"
    );

    // greet has a branch (if name:), so at least 2 blocks expected
    let details = result.details.as_ref().expect("details must be present");
    assert_eq!(details["target"].as_str(), Some(target.as_str()));
    let nodes = details["nodes"].as_u64().expect("nodes must be present");
    assert!(nodes >= 2, "greet has at least 2 blocks, got: {nodes}");
}

// ---------------------------------------------------------------------------
// INV-3: Non-zero pycfg exit returns Err (agent loop maps to is_error: true)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cfg_functions_nonexistent_target_returns_error() {
    let tool = CfgFunctionsTool;
    let result = tool
        .execute(
            "id4".into(),
            json!({ "target": "/nonexistent/path/that/does/not/exist.py" }),
            None,
            None,
        )
        .await;

    assert!(result.is_err(), "nonexistent target should return Err");
}

#[tokio::test]
async fn test_cfg_summary_nonexistent_target_returns_error() {
    let tool = CfgSummaryTool;
    let result = tool
        .execute(
            "id5".into(),
            json!({ "target": "/nonexistent/path.py" }),
            None,
            None,
        )
        .await;

    assert!(result.is_err(), "nonexistent target should return Err");
    let err = result.unwrap_err().to_string();
    assert!(!err.is_empty(), "error message should not be empty");
}

#[tokio::test]
async fn test_cfg_graph_nonexistent_target_returns_error() {
    let tool = CfgGraphTool;
    let result = tool
        .execute(
            "id6".into(),
            json!({ "target": "/nonexistent/path.py::foo" }),
            None,
            None,
        )
        .await;

    assert!(result.is_err(), "nonexistent target should return Err");
}

// ---------------------------------------------------------------------------
// Failure mode: missing 'target' parameter returns Err
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cfg_functions_missing_target_parameter() {
    let tool = CfgFunctionsTool;
    let result = tool.execute("id7".into(), json!({}), None, None).await;

    assert!(result.is_err(), "missing target should return Err");
}

#[tokio::test]
async fn test_cfg_summary_missing_target_parameter() {
    let tool = CfgSummaryTool;
    let result = tool.execute("id8".into(), json!({}), None, None).await;

    assert!(result.is_err(), "missing target should return Err");
}

#[tokio::test]
async fn test_cfg_graph_missing_target_parameter() {
    let tool = CfgGraphTool;
    let result = tool.execute("id9".into(), json!({}), None, None).await;

    assert!(result.is_err(), "missing target should return Err");
}
