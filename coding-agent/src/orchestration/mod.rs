use std::sync::{Arc, Mutex};

use agent::types::AgentEvent;

pub mod runtime;

pub use runtime::{DocumentRequest, OrchestrationRuntime};

/// Shared cell for forwarding agent events once the parent agent exists.
pub type EventForwarderCell = Arc<Mutex<Option<Arc<dyn Fn(AgentEvent) + Send + Sync>>>>;

pub fn event_forwarder_cell() -> EventForwarderCell {
    Arc::new(Mutex::new(None))
}
