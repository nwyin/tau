use coding_agent::validate_benchmark_mode;

// INV-1: benchmark mode without --tools → error with expected message
#[test]
fn test_no_tools_flag_is_error() {
    let err = validate_benchmark_mode(
        false,                               // no --tools
        true,                                // --trace-output present
        &["file_read", "glob", "run_tests"], // tool names (no bash)
        Some("cargo test"),                  // test command present
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("benchmark mode requires --tools"),
        "unexpected error: {err}"
    );
}

// INV-2: benchmark mode without --trace-output → error with expected message
#[test]
fn test_no_trace_output_is_error() {
    let err = validate_benchmark_mode(
        true,  // --tools present
        false, // no --trace-output
        &["file_read", "glob", "run_tests"],
        Some("cargo test"),
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("benchmark mode requires --trace-output"),
        "unexpected error: {err}"
    );
}

// INV-3: benchmark mode with bash in tool list → error with expected message
#[test]
fn test_bash_in_tools_is_error() {
    let err = validate_benchmark_mode(
        true,
        true,
        &["bash", "file_read", "glob", "run_tests"], // bash present
        Some("cargo test"),
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("benchmark mode does not allow bash tool"),
        "unexpected error: {err}"
    );
    assert!(
        err.to_string().contains("--tools"),
        "error should mention --tools: {err}"
    );
}

// INV-4: benchmark mode without test command → error with expected message
#[test]
fn test_no_test_command_is_error() {
    let err = validate_benchmark_mode(
        true,
        true,
        &["file_read", "glob", "run_tests"],
        None, // no test command
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("benchmark mode requires --test-command")
            || msg.contains("TAU_BENCHMARK_TEST_CMD"),
        "unexpected error: {msg}"
    );
}

// INV-5: all required flags present and valid → no error
#[test]
fn test_all_flags_valid_passes() {
    validate_benchmark_mode(
        true,
        true,
        &["file_read", "glob", "grep", "run_tests"],
        Some("cargo test"),
    )
    .expect("all constraints satisfied should not error");
}

// Critical path: each missing flag individually produces a specific error
#[test]
fn test_error_messages_are_specific() {
    // Missing tools flag
    let e1 = validate_benchmark_mode(false, true, &["file_read"], Some("cmd")).unwrap_err();
    assert!(e1.to_string().contains("--tools"), "e1: {e1}");

    // Missing trace-output
    let e2 = validate_benchmark_mode(true, false, &["file_read"], Some("cmd")).unwrap_err();
    assert!(e2.to_string().contains("--trace-output"), "e2: {e2}");

    // bash in tools
    let e3 = validate_benchmark_mode(true, true, &["bash"], Some("cmd")).unwrap_err();
    assert!(e3.to_string().contains("bash"), "e3: {e3}");

    // No test command
    let e4 = validate_benchmark_mode(true, true, &["file_read"], None).unwrap_err();
    assert!(
        e4.to_string().contains("--test-command")
            || e4.to_string().contains("TAU_BENCHMARK_TEST_CMD"),
        "e4: {e4}"
    );
}

// Failure mode: only --tools missing, all others present → describes what's missing
#[test]
fn test_first_violation_reported() {
    // tools is checked first per the spec order
    let err = validate_benchmark_mode(
        false, // no tools
        false, // no trace-output (also missing, but tools checked first)
        &[],
        None,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("--tools"),
        "first error should mention --tools: {err}"
    );
}
