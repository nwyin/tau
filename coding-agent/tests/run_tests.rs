use agent::types::AgentTool;
use ai::types::UserBlock;
use coding_agent::tools::RunTestsTool;
use serde_json::json;

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

// INV-4: Tool name is "run_tests", parameters schema is empty object
#[test]
fn test_tool_name_and_schema() {
    let tool = RunTestsTool::new(None);
    assert_eq!(tool.name(), "run_tests");

    let params = tool.parameters();
    assert_eq!(params["type"], "object");
    // Properties must be an empty object (no parameters)
    let props = params["properties"]
        .as_object()
        .expect("properties must be an object");
    assert!(
        props.is_empty(),
        "parameters schema must have no properties"
    );
}

// INV-1: Tool with no command configured returns is_error: true with clear message
#[tokio::test]
async fn test_no_command_configured_returns_error() {
    let tool = RunTestsTool::new(None);
    let result = tool.execute("id1".into(), json!({}), None, None).await;

    assert!(result.is_err(), "expected Err when no command configured");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("No test command configured"),
        "expected clear error message, got: {msg}"
    );
    assert!(
        msg.contains("TAU_BENCHMARK_TEST_CMD") || msg.contains("--test-command"),
        "expected hint about how to configure, got: {msg}"
    );
}

// INV-2: Tool with command configured runs exactly that command (not model-supplied)
// Critical path: "echo hello" → succeeds, exit_code 0, stdout contains "hello"
#[tokio::test]
async fn test_configured_command_runs_echo_hello() {
    let tool = RunTestsTool::new(Some("echo hello".to_string()));
    let result = tool
        .execute("id2".into(), json!({}), None, None)
        .await
        .expect("should not error");

    let out = text_content(&result);
    assert!(
        out.contains("hello"),
        "expected 'hello' in output, got: {out}"
    );

    // INV-3: Exit code faithfully reported in details
    let details = result.details.expect("details must be present");
    assert_eq!(details["exit_code"], 0, "expected exit_code 0");
    assert_eq!(details["passed"], true, "expected passed: true");
    assert_eq!(details["command"], "echo hello", "command must be recorded");
}

// INV-3: Exit code is faithfully reported in details
// Critical path: "exit 1" → exit_code 1, passed: false in details
#[tokio::test]
async fn test_exit_one_reported_in_details() {
    let tool = RunTestsTool::new(Some("exit 1".to_string()));
    let result = tool
        .execute("id3".into(), json!({}), None, None)
        .await
        .expect("execute should not error — non-zero exit is faithfully reported");

    let details = result.details.expect("details must be present");
    assert_eq!(details["exit_code"], 1, "expected exit_code 1");
    assert_eq!(
        details["passed"], false,
        "expected passed: false for exit code 1"
    );
    assert!(
        details["duration_ms"].as_u64().is_some(),
        "duration_ms must be present"
    );
}

// Failure mode: command that doesn't exist on PATH returns non-zero exit faithfully
#[tokio::test]
async fn test_nonexistent_command_reports_exit_code() {
    // sh -c on a nonexistent binary returns exit 127
    let tool = RunTestsTool::new(Some("__tau_nonexistent_binary_xyz_abc__".to_string()));
    let result = tool
        .execute("id4".into(), json!({}), None, None)
        .await
        .expect("execute should return Ok (sh itself runs fine, command returns 127)");

    let details = result.details.expect("details must be present");
    // exit 127 = command not found
    assert_ne!(
        details["exit_code"], 0,
        "nonexistent command must not exit 0"
    );
    assert_eq!(
        details["passed"], false,
        "must not report passed for nonexistent command"
    );
}

// Details field includes all required keys
#[tokio::test]
async fn test_details_shape() {
    let tool = RunTestsTool::new(Some("echo shape_test".to_string()));
    let result = tool
        .execute("id5".into(), json!({}), None, None)
        .await
        .expect("should succeed");

    let details = result.details.expect("details must be Some");
    assert!(details["command"].is_string(), "command must be a string");
    assert!(
        details["exit_code"].is_number(),
        "exit_code must be a number"
    );
    assert!(
        details["duration_ms"].is_number(),
        "duration_ms must be a number"
    );
    assert!(
        details["stdout_lines"].is_number(),
        "stdout_lines must be a number"
    );
    assert!(
        details["stderr_lines"].is_number(),
        "stderr_lines must be a number"
    );
    assert!(details["passed"].is_boolean(), "passed must be a boolean");
}

// Exit code is included in the human-readable content
#[tokio::test]
async fn test_exit_code_in_content() {
    let tool = RunTestsTool::new(Some("exit 42".to_string()));
    let result = tool
        .execute("id6".into(), json!({}), None, None)
        .await
        .expect("should return Ok");

    let out = text_content(&result);
    assert!(
        out.contains("42"),
        "exit code 42 must appear in content, got: {out}"
    );
}
