use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use agent::types::{AgentTool, AgentToolResult, BoxFuture};
use ai::types::UserBlock;
use coding_agent::system_prompt::build_system_prompt;
use coding_agent::tools::py_repl::PyReplTool;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("coding-agent has repo parent")
}

fn manifest() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts/py_tau_api.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn py_kernel() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts/py_kernel.py"))
        .unwrap()
}

struct FakeTool;

impl AgentTool for FakeTool {
    fn name(&self) -> &str {
        "fake"
    }

    fn label(&self) -> &str {
        "Fake"
    }

    fn description(&self) -> &str {
        "fake"
    }

    fn parameters(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| json!({"type": "object"}))
    }

    fn execute(
        &self,
        _tool_call_id: String,
        _params: Value,
        _signal: Option<CancellationToken>,
    ) -> BoxFuture<anyhow::Result<AgentToolResult>> {
        Box::pin(async move {
            Ok(AgentToolResult {
                content: vec![UserBlock::Text {
                    text: "ok".to_string(),
                }],
                details: None,
            })
        })
    }
}

#[test]
fn generated_tau_api_artifacts_are_fresh() {
    let output = Command::new("python3")
        .arg("scripts/generate-py-tau-api.py")
        .arg("--check")
        .current_dir(repo_root())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "generator check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn generated_py_kernel_compiles() {
    let output = Command::new("python3")
        .arg("-c")
        .arg(
            "import ast, pathlib; ast.parse(pathlib.Path('coding-agent/prompts/py_kernel.py').read_text())",
        )
        .current_dir(repo_root())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "py_kernel.py did not compile\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn manifest_methods_and_results_are_in_generated_kernel() {
    let manifest = manifest();
    let kernel = py_kernel();

    for method in manifest["methods"].as_array().unwrap() {
        let name = method["name"].as_str().unwrap();
        let signature = method["signature"].as_str().unwrap();
        assert!(
            kernel.contains(&format!("def {signature}:")),
            "generated kernel missing method signature for {name}"
        );
        assert!(
            kernel.contains(&format!("\"{}\"", method["rpc"].as_str().unwrap())),
            "generated kernel missing RPC method for {name}"
        );
    }

    for factory in manifest["factories"].as_array().unwrap() {
        let signature = factory["signature"].as_str().unwrap();
        assert!(
            kernel.contains(&format!("def {signature}:")),
            "generated kernel missing factory signature {signature}"
        );
    }

    for result in manifest["result_types"].as_array().unwrap() {
        let name = result["name"].as_str().unwrap();
        assert!(
            kernel.contains(&format!("class {name}:")),
            "generated kernel missing result type {name}"
        );
        for field in result["fields"].as_array().unwrap() {
            let field = field["name"].as_str().unwrap();
            assert!(
                kernel.contains(&format!("self.{field}")),
                "generated kernel missing field {name}.{field}"
            );
        }
    }
}

#[test]
fn manifest_rpc_methods_are_handled_by_runtime_facade() {
    let manifest = manifest();
    let rpc_source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/orchestration/rpc.rs"),
    )
    .unwrap();
    let handled = [
        "tool", "thread", "launch", "poll", "wait", "query", "document", "parallel", "diff",
        "merge", "branches", "log",
    ]
    .into_iter()
    .collect::<HashSet<_>>();

    for method in manifest["methods"].as_array().unwrap() {
        let rpc = method["rpc"].as_str().unwrap();
        assert!(
            handled.contains(rpc),
            "test fixture missing expected RPC method {rpc}"
        );
        assert!(
            rpc_source.contains(&format!("\"{rpc}\"")),
            "OrchestrationRpcFacade missing dispatch arm for RPC method {rpc}"
        );
    }
}

#[test]
fn py_repl_description_and_prompt_cover_complete_tau_surface() {
    let tool = PyReplTool::new(
        agent::orchestrator::OrchestratorState::new(),
        Arc::new(FakeTool),
        Arc::new(FakeTool),
        Arc::new(FakeTool),
        Default::default(),
    );
    let prompt = build_system_prompt(&[Arc::new(tool) as Arc<dyn AgentTool>], &[], "/tmp");
    let description = PyReplTool::new(
        agent::orchestrator::OrchestratorState::new(),
        Arc::new(FakeTool),
        Arc::new(FakeTool),
        Arc::new(FakeTool),
        Default::default(),
    )
    .description()
    .to_string();

    for name in [
        "tool", "thread", "launch", "poll", "wait", "query", "parallel", "document", "log", "diff",
        "merge", "branches",
    ] {
        let needle = format!("tau.{name}");
        assert!(
            description.contains(&needle),
            "py_repl description missing {needle}"
        );
        assert!(prompt.contains(&needle), "system prompt missing {needle}");
    }
}
