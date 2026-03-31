// Agent event bridge is wired directly in tui/mod.rs:
// - agent.subscribe() forwards AgentEvents via ProgramHandle
// - Permission bridge spawns a tokio task forwarding sync requests
//
// No separate module needed — the bridge logic lives in run().
