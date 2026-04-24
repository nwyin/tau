pub mod bash;
pub mod document;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod from_id;
pub mod glob;
pub mod grep;
pub mod log;
pub mod py_repl;
pub mod query;
pub mod registry;
pub mod subagent;
pub mod thread;
pub mod todo;
pub mod web_fetch;
pub mod web_search;
pub mod worktree;

use std::collections::HashMap;
use std::sync::Arc;

use agent::types::AgentTool;

use crate::orchestration::{event_forwarder_cell, EventForwarderCell};

pub use bash::BashTool;
pub use document::DocumentTool;
pub use file_edit::FileEditTool;
pub use file_read::FileReadTool;
pub use file_write::FileWriteTool;
pub use from_id::FromIdTool;
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use log::LogTool;
pub use query::QueryTool;
pub use registry::{summarize_tool_call, ToolRegistry};
pub use subagent::SubagentTool;
pub use thread::ThreadTool;
pub use todo::{TodoItem, TodoTool};
pub use web_fetch::WebFetchTool;
pub use web_search::WebSearchTool;

/// Return all built-in tools.
pub fn default_tools() -> Vec<Arc<dyn AgentTool>> {
    ToolRegistry::new().default_tools()
}

/// Returns all direct tool implementations keyed by canonical name.
pub fn all_direct_tools() -> HashMap<String, Arc<dyn AgentTool>> {
    ToolRegistry::new().all_direct_tools()
}

/// Returns all direct tool implementations keyed by canonical name.
pub fn all_known_tools() -> HashMap<String, Arc<dyn AgentTool>> {
    ToolRegistry::new().all_known_tools()
}

pub struct OrchestrationToolSet {
    pub tools: Vec<Arc<dyn AgentTool>>,
    pub event_forwarder_cell: EventForwarderCell,
    pub thread_tool: Arc<dyn AgentTool>,
    pub query_tool: Arc<dyn AgentTool>,
    pub document_tool: Arc<dyn AgentTool>,
    pub log_tool: Arc<dyn AgentTool>,
    pub from_id_tool: Arc<dyn AgentTool>,
}

pub struct WrappedOrchestrationToolSet {
    pub tools: Vec<Arc<dyn AgentTool>>,
    pub thread_tool: Arc<dyn AgentTool>,
    pub query_tool: Arc<dyn AgentTool>,
    pub document_tool: Arc<dyn AgentTool>,
}

impl OrchestrationToolSet {
    pub fn wrap_with_permissions(
        self,
        permission_service: Arc<crate::permissions::PermissionService>,
    ) -> WrappedOrchestrationToolSet {
        let thread_tool = crate::permissions::PermissionWrapper::arc(
            self.thread_tool,
            Arc::clone(&permission_service),
        );
        let query_tool = crate::permissions::PermissionWrapper::arc(
            self.query_tool,
            Arc::clone(&permission_service),
        );
        let document_tool = crate::permissions::PermissionWrapper::arc(
            self.document_tool,
            Arc::clone(&permission_service),
        );
        let log_tool = crate::permissions::PermissionWrapper::arc(
            self.log_tool,
            Arc::clone(&permission_service),
        );
        let from_id_tool = crate::permissions::PermissionWrapper::arc(
            self.from_id_tool,
            Arc::clone(&permission_service),
        );

        let tools = vec![
            thread_tool.clone(),
            query_tool.clone(),
            document_tool.clone(),
            log_tool,
            from_id_tool,
        ];

        WrappedOrchestrationToolSet {
            tools,
            thread_tool,
            query_tool,
            document_tool,
        }
    }
}

/// Create orchestration tools except py_repl, which needs the final wrapped tool set.
pub fn orchestration_core_tools(
    orchestrator: Arc<agent::orchestrator::OrchestratorState>,
    get_api_key: Option<agent::types::GetApiKeyFn>,
    model: ai::types::Model,
    model_slots: crate::config::ModelSlots,
) -> OrchestrationToolSet {
    let cell = event_forwarder_cell();
    let thread_tool = ThreadTool::arc(
        orchestrator.clone(),
        get_api_key.clone(),
        model.clone(),
        cell.clone(),
        model_slots.clone(),
    );
    let query_tool = QueryTool::arc(
        orchestrator.clone(),
        get_api_key.clone(),
        model.clone(),
        model_slots.clone(),
        cell.clone(),
    );
    let document_tool = DocumentTool::arc(orchestrator.clone(), cell.clone());
    let log_tool = LogTool::arc(orchestrator.clone());
    let from_id_tool = FromIdTool::arc(orchestrator);
    let tools = vec![
        thread_tool.clone(),
        query_tool.clone(),
        document_tool.clone(),
        log_tool.clone(),
        from_id_tool.clone(),
    ];
    OrchestrationToolSet {
        tools,
        event_forwarder_cell: cell,
        thread_tool,
        query_tool,
        document_tool,
        log_tool,
        from_id_tool,
    }
}

/// Create orchestration tools (thread + query) that require runtime state.
pub fn orchestration_tools(
    orchestrator: Arc<agent::orchestrator::OrchestratorState>,
    get_api_key: Option<agent::types::GetApiKeyFn>,
    model: ai::types::Model,
    model_slots: crate::config::ModelSlots,
) -> (Vec<Arc<dyn AgentTool>>, EventForwarderCell) {
    let core = orchestration_core_tools(orchestrator.clone(), get_api_key, model, model_slots);
    let py_repl_tool = py_repl::PyReplTool::arc(
        orchestrator.clone(),
        core.thread_tool.clone(),
        core.query_tool.clone(),
        core.document_tool.clone(),
    );
    let mut tools = core.tools;
    tools.push(py_repl_tool);
    (tools, core.event_forwarder_cell)
}

/// Resolve an allowlist of tool names against the registry.
pub fn tools_from_allowlist(names: &[String]) -> Vec<Arc<dyn AgentTool>> {
    ToolRegistry::new().tools_from_allowlist(names)
}

/// Returns all direct tools with a custom working directory for filesystem tools.
pub fn all_direct_tools_with_cwd(cwd: std::path::PathBuf) -> HashMap<String, Arc<dyn AgentTool>> {
    ToolRegistry::new().all_direct_tools_with_cwd(cwd)
}

/// Returns all direct tools with a custom working directory for filesystem tools.
pub fn all_known_tools_with_cwd(cwd: std::path::PathBuf) -> HashMap<String, Arc<dyn AgentTool>> {
    ToolRegistry::new().all_known_tools_with_cwd(cwd)
}

/// Resolve an allowlist of tool names with a custom working directory.
pub fn tools_from_allowlist_with_cwd(
    names: &[String],
    cwd: std::path::PathBuf,
) -> Vec<Arc<dyn AgentTool>> {
    ToolRegistry::new().tools_from_allowlist_with_cwd(names, cwd)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn test_model() -> ai::types::Model {
        ai::types::Model {
            id: "mock".into(),
            name: "mock".into(),
            api: "openai-responses".into(),
            provider: "openai".into(),
            base_url: "https://example.invalid".into(),
            reasoning: false,
            input: vec!["text".into()],
            cost: ai::types::ModelCost {
                input: 0.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
            },
            context_window: 8192,
            max_tokens: 2048,
            headers: None,
        }
    }

    #[test]
    fn orchestration_permission_wrap_preserves_named_handles() {
        let core = orchestration_core_tools(
            agent::orchestrator::OrchestratorState::new(),
            None,
            test_model(),
            crate::config::ModelSlots::default(),
        );
        let service = Arc::new(crate::permissions::PermissionService::new(
            &HashMap::new(),
            false,
        ));

        let wrapped = core.wrap_with_permissions(service);
        assert_eq!(wrapped.thread_tool.name(), "thread");
        assert_eq!(wrapped.query_tool.name(), "query");
        assert_eq!(wrapped.document_tool.name(), "document");

        let names: Vec<&str> = wrapped.tools.iter().map(|tool| tool.name()).collect();
        assert_eq!(names, vec!["thread", "query", "document", "log", "from_id"]);
    }
}
