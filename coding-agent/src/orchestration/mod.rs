use std::sync::{Arc, Mutex};

use agent::types::AgentEvent;

pub mod rpc;
pub mod runtime;

pub use rpc::{OrchestrationRpcFacade, ThreadState};
pub use runtime::{
    AgentRuntimeConfig, BranchDiffResult, BranchMergeResult, DocumentRequest, DocumentResult,
    EpisodeLookupRequest, EpisodeLookupResult, LogRequest, OrchestrationRuntime, QueryRequest,
    QueryResult, ThreadRequest, ThreadRunResult,
};

/// Shared cell for forwarding agent events once the parent agent exists.
pub type EventForwarderCell = Arc<Mutex<Option<Arc<dyn Fn(AgentEvent) + Send + Sync>>>>;

pub fn event_forwarder_cell() -> EventForwarderCell {
    Arc::new(Mutex::new(None))
}
