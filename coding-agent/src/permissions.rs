//! Tool permission system.
//!
//! Each tool can be configured as `allow`, `deny`, or `ask`.
//! - `allow`: tool executes without prompting
//! - `deny`: tool is blocked, error returned to agent
//! - `ask`: user is prompted at runtime (y/n/always)
//!
//! Sensible defaults: read-only tools (`file_read`, `glob`, `grep`) auto-allow,
//! write/exec tools (`bash`, `file_edit`, `file_write`) ask.

use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use agent::types::{AgentTool, AgentToolResult, BoxFuture};
use ai::types::UserBlock;
use serde_json::Value;

use crate::tools::ToolRegistry;

/// Per-tool permission policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Allow,
    Deny,
    Ask,
}

impl Policy {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "allow" => Some(Policy::Allow),
            "deny" => Some(Policy::Deny),
            "ask" => Some(Policy::Ask),
            _ => None,
        }
    }
}

/// Default policies: read-only tools allow, everything else asks.
fn default_policy(tool_name: &str) -> Policy {
    ToolRegistry::new().default_policy(tool_name)
}

/// Prompt function signature: (tool_name, description) -> user choice.
/// Returns `true` to allow, `false` to deny. Sets `upgrade_to_allow` if
/// the user chose "always".
pub type PromptFn = Arc<dyn Fn(&str, &str) -> PromptResult + Send + Sync>;

#[derive(Debug, Clone)]
pub enum PromptResult {
    Allow,
    AlwaysAllow,
    Deny,
}

/// Manages per-tool permission policies with session-level upgrades.
pub struct PermissionService {
    /// Configured policies (from config + defaults).
    policies: Mutex<HashMap<String, Policy>>,
    /// Whether yolo mode is active (bypass all checks).
    yolo: AtomicBool,
    /// Function to prompt the user for approval (interior-mutable so TUI can set it after Arc is shared).
    prompt_fn: Mutex<Option<PromptFn>>,
}

impl PermissionService {
    /// Create a new service from config-defined policies.
    ///
    /// `config_policies` maps tool names to policy strings ("allow"/"deny"/"ask").
    /// Tools not in the map get the built-in default.
    pub fn new(config_policies: &HashMap<String, String>, yolo: bool) -> Self {
        let mut policies = HashMap::new();
        for (tool, policy_str) in config_policies {
            if let Some(p) = Policy::parse(policy_str) {
                policies.insert(tool.clone(), p);
            } else {
                eprintln!(
                    "[warn] invalid permission policy '{}' for tool '{}', ignoring",
                    policy_str, tool
                );
            }
        }
        Self {
            policies: Mutex::new(policies),
            yolo: AtomicBool::new(yolo),
            prompt_fn: Mutex::new(None),
        }
    }

    /// Check if yolo mode is active.
    pub fn is_yolo(&self) -> bool {
        self.yolo.load(Ordering::Relaxed)
    }

    /// Toggle yolo mode. Returns the new state.
    pub fn set_yolo(&self, enabled: bool) {
        self.yolo.store(enabled, Ordering::Relaxed);
    }

    /// Set the prompt function for interactive approval.
    /// Takes `&self` (not `&mut self`) so it can be called after the service is behind an Arc.
    pub fn set_prompt_fn(&self, f: PromptFn) {
        *self.prompt_fn.lock().unwrap() = Some(f);
    }

    /// Get the effective policy for a tool.
    pub fn policy_for(&self, tool_name: &str) -> Policy {
        let policies = self.policies.lock().unwrap();
        policies
            .get(tool_name)
            .cloned()
            .unwrap_or_else(|| default_policy(tool_name))
    }

    /// Check whether a tool call should be allowed.
    /// Returns Ok(()) if allowed, Err(message) if denied.
    pub fn check(&self, tool_name: &str, description: &str) -> Result<(), String> {
        if self.yolo.load(Ordering::Relaxed) {
            return Ok(());
        }

        match self.policy_for(tool_name) {
            Policy::Allow => Ok(()),
            Policy::Deny => Err(format!(
                "Tool '{}' is denied by permission policy.",
                tool_name
            )),
            Policy::Ask => {
                let prompt_fn = self.prompt_fn.lock().unwrap().clone();
                if let Some(ref prompt_fn) = prompt_fn {
                    match prompt_fn(tool_name, description) {
                        PromptResult::Allow => Ok(()),
                        PromptResult::AlwaysAllow => {
                            // Upgrade to allow for rest of session
                            self.policies
                                .lock()
                                .unwrap()
                                .insert(tool_name.to_string(), Policy::Allow);
                            Ok(())
                        }
                        PromptResult::Deny => {
                            Err(format!("Tool '{}' execution denied by user.", tool_name))
                        }
                    }
                } else {
                    // No prompt function (non-interactive) — allow by default
                    Ok(())
                }
            }
        }
    }
}

/// Format a short description of a tool call for the permission prompt.
pub fn describe_tool_call(tool_name: &str, params: &Value) -> String {
    crate::tools::summarize_tool_call(tool_name, params)
}

/// The default interactive prompt function that reads from stdin.
pub fn interactive_prompt(tool_name: &str, description: &str) -> PromptResult {
    let label = if description.is_empty() {
        format!("[permission] {} — allow? [y/n/a]: ", tool_name)
    } else {
        format!(
            "[permission] {} ({}) — allow? [y/n/a]: ",
            tool_name, description
        )
    };

    eprint!("{}", label);
    let _ = io::stderr().flush();

    let mut line = String::new();
    match io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => PromptResult::Deny, // EOF or error → deny
        Ok(_) => match line.trim().to_lowercase().as_str() {
            "y" | "yes" => PromptResult::Allow,
            "a" | "always" => PromptResult::AlwaysAllow,
            _ => PromptResult::Deny,
        },
    }
}

// ---------------------------------------------------------------------------
// PermissionWrapper — wraps an AgentTool with permission checks
// ---------------------------------------------------------------------------

/// Wraps an `AgentTool` to check permissions before execution.
pub struct PermissionWrapper {
    inner: Arc<dyn AgentTool>,
    service: Arc<PermissionService>,
}

impl PermissionWrapper {
    pub fn new(inner: Arc<dyn AgentTool>, service: Arc<PermissionService>) -> Self {
        Self { inner, service }
    }

    pub fn arc(inner: Arc<dyn AgentTool>, service: Arc<PermissionService>) -> Arc<dyn AgentTool> {
        Arc::new(Self::new(inner, service))
    }
}

impl AgentTool for PermissionWrapper {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters(&self) -> &Value {
        self.inner.parameters()
    }

    fn execute(
        &self,
        tool_call_id: String,
        params: Value,
        signal: Option<tokio_util::sync::CancellationToken>,
    ) -> BoxFuture<anyhow::Result<AgentToolResult>> {
        let desc = describe_tool_call(self.inner.name(), &params);

        match self.service.check(self.inner.name(), &desc) {
            Ok(()) => self.inner.execute(tool_call_id, params, signal),
            Err(msg) => Box::pin(async move {
                Ok(AgentToolResult {
                    content: vec![UserBlock::Text { text: msg }],
                    details: None,
                })
            }),
        }
    }
}

/// Wrap a list of tools with permission checks.
pub fn wrap_tools(
    tools: Vec<Arc<dyn AgentTool>>,
    service: Arc<PermissionService>,
) -> Vec<Arc<dyn AgentTool>> {
    tools
        .into_iter()
        .map(|t| PermissionWrapper::arc(t, Arc::clone(&service)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_config() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn test_default_policies() {
        let svc = PermissionService::new(&empty_config(), false);

        assert_eq!(svc.policy_for("file_read"), Policy::Allow);
        assert_eq!(svc.policy_for("glob"), Policy::Allow);
        assert_eq!(svc.policy_for("grep"), Policy::Allow);
        assert_eq!(svc.policy_for("web_fetch"), Policy::Allow);
        assert_eq!(svc.policy_for("web_search"), Policy::Allow);

        assert_eq!(svc.policy_for("bash"), Policy::Ask);
        assert_eq!(svc.policy_for("file_edit"), Policy::Ask);
        assert_eq!(svc.policy_for("file_write"), Policy::Ask);
    }

    #[test]
    fn test_config_override() {
        let mut config = HashMap::new();
        config.insert("bash".to_string(), "allow".to_string());
        config.insert("file_read".to_string(), "deny".to_string());

        let svc = PermissionService::new(&config, false);
        assert_eq!(svc.policy_for("bash"), Policy::Allow);
        assert_eq!(svc.policy_for("file_read"), Policy::Deny);
        // Unspecified tools still get defaults
        assert_eq!(svc.policy_for("grep"), Policy::Allow);
        assert_eq!(svc.policy_for("file_edit"), Policy::Ask);
    }

    #[test]
    fn test_yolo_bypasses_all() {
        let mut config = HashMap::new();
        config.insert("bash".to_string(), "deny".to_string());

        let svc = PermissionService::new(&config, true);
        assert!(svc.check("bash", "echo hello").is_ok());
    }

    #[test]
    fn test_deny_blocks() {
        let mut config = HashMap::new();
        config.insert("bash".to_string(), "deny".to_string());

        let svc = PermissionService::new(&config, false);
        let result = svc.check("bash", "rm -rf /");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("denied by permission policy"));
    }

    #[test]
    fn test_allow_passes() {
        let svc = PermissionService::new(&empty_config(), false);
        assert!(svc.check("file_read", "/tmp/foo").is_ok());
    }

    #[test]
    fn test_ask_with_prompt_fn_allow() {
        let svc = PermissionService::new(&empty_config(), false);
        svc.set_prompt_fn(Arc::new(|_name, _desc| PromptResult::Allow));

        assert!(svc.check("bash", "echo hello").is_ok());
        // Still ask next time
        assert_eq!(svc.policy_for("bash"), Policy::Ask);
    }

    #[test]
    fn test_ask_with_prompt_fn_always() {
        let svc = PermissionService::new(&empty_config(), false);
        svc.set_prompt_fn(Arc::new(|_name, _desc| PromptResult::AlwaysAllow));

        assert!(svc.check("bash", "echo hello").is_ok());
        // Should now be upgraded to allow
        assert_eq!(svc.policy_for("bash"), Policy::Allow);
    }

    #[test]
    fn test_ask_with_prompt_fn_deny() {
        let svc = PermissionService::new(&empty_config(), false);
        svc.set_prompt_fn(Arc::new(|_name, _desc| PromptResult::Deny));

        let result = svc.check("bash", "echo hello");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("denied by user"));
    }

    #[test]
    fn test_ask_without_prompt_fn_allows() {
        // Non-interactive mode (no prompt fn) should allow by default
        let svc = PermissionService::new(&empty_config(), false);
        assert!(svc.check("bash", "echo hello").is_ok());
    }

    #[test]
    fn test_describe_tool_call() {
        assert_eq!(
            describe_tool_call("bash", &serde_json::json!({"command": "echo hello"})),
            "echo hello"
        );
        assert_eq!(
            describe_tool_call("file_read", &serde_json::json!({"path": "/tmp/foo.rs"})),
            "/tmp/foo.rs"
        );
        assert_eq!(
            describe_tool_call("web_search", &serde_json::json!({"query": "rust async"})),
            "rust async"
        );
        assert!(describe_tool_call("unknown_tool", &serde_json::json!({})).is_empty());
    }

    #[test]
    fn test_invalid_config_policy_ignored() {
        let mut config = HashMap::new();
        config.insert("bash".to_string(), "invalid_value".to_string());

        let svc = PermissionService::new(&config, false);
        // Falls back to default (ask)
        assert_eq!(svc.policy_for("bash"), Policy::Ask);
    }

    #[test]
    fn test_policy_parse() {
        assert_eq!(Policy::parse("allow"), Some(Policy::Allow));
        assert_eq!(Policy::parse("DENY"), Some(Policy::Deny));
        assert_eq!(Policy::parse(" Ask "), Some(Policy::Ask));
        assert_eq!(Policy::parse("nope"), None);
    }
}
