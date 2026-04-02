# Thread Inspector Modal

Live-streaming modal overlay for introspecting thread/subagent sessions from the sidebar.

## Requirements

### Phase 1: ThreadEvent plumbing (agent crate)

- Add `AgentEvent::ThreadEvent { thread_id: String, alias: String, event: Box<AgentEvent> }` variant
- When a thread's agent loop emits events (TextDelta, ToolStart, ToolEnd, ThinkingDelta, etc.), wrap them in `ThreadEvent` before forwarding to the parent event sender
- ThreadStart and ThreadEnd remain as top-level events (they mark lifecycle boundaries; ThreadEvent carries the internal stream)
- No changes to the trace JSONL format -- ThreadEvents should still serialize for tracing

### Phase 2: Sidebar navigation + focus cycling

- Add `FocusState::Sidebar` to the existing `FocusState` enum (currently: Editor, Chat)
- Tab cycles focus: Editor -> Chat -> Sidebar -> Editor
- When sidebar is focused:
  - Up/Down arrow keys navigate a cursor over thread entries
  - The selected thread entry is visually highlighted
  - Enter opens the thread inspector modal
  - Esc returns focus to Editor
- All threads (running + completed) persist in the sidebar thread list for the session
- Sidebar thread entries show status indicator: green dot for running, checkmark for completed, X for failed

### Phase 3: Modal rendering + thread message buffer

- TUI maintains a `HashMap<String, Vec<ChatMessage>>` keyed by thread_id for per-thread message buffers
- On receiving `ThreadEvent`, demux by thread_id and append to the appropriate buffer (same ChatMessage construction as main chat: assistant text, tool calls with status icons, thinking blocks)
- Modal overlay:
  - 80% of viewport width and height, centered
  - Rounded border styled with thread status color (green while running, muted when done)
  - Header: thread alias, task description, model, status + elapsed time
  - Body: scrollable viewport rendering the thread's ChatMessage buffer (same rendering as main chat)
  - Thinking blocks included, collapsed by default
  - Up/Down scrolls the viewport, Esc dismisses
- While modal is open:
  - Events continue buffering into both main chat and thread message lists
  - Main chat viewport does NOT re-render (paused) -- only the modal viewport updates
  - Thread modal viewport auto-scrolls to bottom as new events arrive (following mode)
  - Add `FocusState::Modal` (or `ThreadModal { thread_id }`) to handle input routing

## Non-requirements (explicitly deferred)

- Grouping threads by user turn (Josh Wong's approach) -- revisit at 50+ threads
- Thread-to-thread navigation within the modal (next/prev thread)
- Filtering or searching within thread logs
- Persisting thread message buffers across sessions

## Verification

- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `cargo build`
- Manual test: run tau with a prompt that spawns 2+ threads, Tab to sidebar, select a running thread, verify modal shows live-streaming messages, Esc to dismiss, verify main chat catches up

## Success Criteria

- All verification commands pass (existing tests do not break)
- Tab cycles through Editor -> Chat -> Sidebar focus states
- Sidebar thread entries are navigable with arrow keys and visually highlighted
- Enter on a thread opens an 80%-centered modal with the thread's message log
- Modal updates in real-time as thread emits events (assistant text, tool calls, thinking)
- Esc dismisses modal and returns to sidebar focus
- Main chat resumes rendering after modal close

## Commit Plan

One branch, one PR, three commits matching the phases:

1. `feat(agent): add ThreadEvent wrapper for thread-internal event streaming`
2. `feat(tui): add sidebar focus state and thread navigation`
3. `feat(tui): add thread inspector modal with live-streaming message log`
