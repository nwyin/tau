# Refactor TUI to use ruse Scene/Pane compositor

## Context

ruse recently gained a `Scene` compositor + `Pane` trait (commit `3362a4b`) that handles input routing, focus lifecycle, z-ordered view composition, and dynamic pane add/remove. tau's TUI is currently a monolithic `TauModel` (~1750 lines) with manual focus state machine and manual region composition. Refactoring to Scene gives:

1. **Live chat updates while in thread modal** -- Scene broadcasts WindowSize/resize to all visible panes, and the parent pushes content updates to all viewports regardless of focus.
2. **Clean input routing** -- KeyPress routes to focused pane automatically, no manual `match self.focus` dispatch.
3. **Dynamic thread modal** -- `scene.add()`/`scene.remove()` with z-order, mouse hit-testing, and proper focus lifecycle.
4. **Extensibility** -- future modals/panels (command palette, model picker) are just new Panes.

## Architecture

### What becomes a Pane vs stays in parent

**Panes** (own their UI component + handle focused input):
| Pane ID | Wraps | Key bindings |
|---------|-------|-------------|
| `"chat"` | Viewport, selected_msg, scroll_follow | j/k/d/u/G/g scroll, J/K msg nav, Space expand |
| `"editor"` | TextInput, TabState, spinner | typing, Enter submit, Tab completion |
| `"sidebar"` | cursor, cached render data | j/k navigate, Enter open thread, Esc back |
| `"thread_modal"` | Viewport, header data (dynamic) | j/k/d/u/G/g scroll, Esc close |

**Parent TauModel** (orchestrator -- owns shared domain state):
- `messages`, `streaming`, `thread_messages`, `thread_streaming` -- canonical data
- `agent`, `permission_service`, `perm_queue` -- cross-cutting concerns
- `model_id`, `tokens_in/out`, `total_cost`, `thinking_level` -- metrics
- `thread_entries`, `skills`, `session_manager` -- config/state

### Data flow: parent -> pane (unidirectional)

Panes never access each other. After any state mutation, parent pushes rendered content:
```
AgentEvent arrives -> parent updates messages/streaming
                   -> parent calls scene.pane_as_mut::<ChatPane>("chat").set_content(...)
                   -> parent calls scene.pane_as_mut::<ThreadModalPane>("thread_modal").set_content(...) (if open)
```

### Pane-to-parent communication: `pending_action` pattern

Each pane has an optional action field the parent checks after `scene.update()`:
- `EditorPane.pending_action: Option<String>` -- set on Enter, parent runs submit_prompt()
- `SidebarPane.pending_action: Option<SidebarAction>` -- OpenThread(idx) or Back
- `ThreadModalPane.pending_close: bool` -- set on Esc

### Update flow in refactored Model

```
1. Msg::Custom(TauMsg)? -> handle_tau_msg() [intercept BEFORE scene, return early]
   - Mutates messages/streaming/metrics/threads
   - Pushes updated content to relevant panes

2. perm_queue non-empty? -> intercept keys [BEFORE scene, return early]

3. Global keys? -> Ctrl-C/D/T, Tab focus cycling [BEFORE scene]
   - Tab: scene.set_focus() to cycle editor->chat->sidebar->editor

4. WindowSize? -> recompute layout, scene.set_layout() [BEFORE scene.update()]
   - Then fall through to scene.update() for broadcast to panes

5. Everything else -> scene.update(&msg) [routes to focused pane]

6. Check pane pending_actions -> submit_prompt(), open/close modal, etc.
```

## Implementation Steps (incremental, each commit keeps TUI functional)

### Step 1: Scaffold Scene + ChatPane
- Create `tui/panes/` module with `ChatPane` (Viewport + selected_msg + scroll_follow)
- Add Scene to TauModel, add ChatPane at init
- Move chat keybindings (j/k/d/u/G/g/J/K/Space) into `ChatPane::update()`
- Replace `self.chat_viewport` with `scene.pane_as_mut::<ChatPane>("chat")`
- `view_chat()` uses scene.view() for chat region + manual append for sidebar/input/status
- **Files**: new `tui/panes/mod.rs`, `tui/panes/chat_pane.rs`; edit `tui/model.rs`

### Step 2: Extract EditorPane
- Create `EditorPane` (TextInput + TabState + GradientSpinner + pending_action)
- Move Enter/Tab/typing handling from FocusState::Editor into pane
- Parent checks pending_action after scene.update(), runs submit_prompt()
- Move spinner display into pane (busy state set by parent)
- Permission prompt rendering stays in parent (it replaces the editor view entirely)
- **Files**: new `tui/panes/editor_pane.rs`; edit `tui/model.rs`

### Step 3: Extract SidebarPane
- Create `SidebarPane` (cursor + cached SidebarRenderState + pending_action)
- Move j/k/Enter/Esc handling from FocusState::Sidebar into pane
- Parent pushes render state after agent events, checks pending_action
- **Files**: new `tui/panes/sidebar_pane.rs`; edit `tui/model.rs`

### Step 4: Dynamic ThreadModalPane
- Create `ThreadModalPane` (Viewport + header data + pending_close)
- On SidebarAction::OpenThread: `scene.add("thread_modal", pane, PaneLayout{z:1})` + set_focus
- On pending_close: `scene.remove("thread_modal")` + set_focus back to sidebar
- Move j/k/d/u/G/g/Esc from FocusState::ThreadModal into pane
- **Files**: new `tui/panes/thread_modal_pane.rs`; edit `tui/model.rs`

### Step 5: Clean up
- Delete FocusState enum -- replaced by `scene.focused()` string matching
- Simplify Tab cycling to `scene.set_focus()` calls
- Extract `handle_agent_event()`/`handle_thread_event()` into `tui/events.rs` if model.rs still large
- Landing screen stays as direct `View::new()` (no pane needed)
- model.rs target: ~500-600 lines

## Key files
- `coding-agent/src/tui/model.rs` -- monolith being decomposed
- `coding-agent/src/tui/panes/*.rs` -- new pane modules (4 files)
- `ruse-runtime/src/scene.rs` -- Scene API reference
- `ruse-runtime/src/pane.rs` -- Pane trait
- `ruse/examples/composite.rs` -- reference pattern

## Verification
- `cargo build` after each step
- `cargo clippy` clean
- `cargo fmt --check` clean
- Manual testing: launch tau, type prompts, Tab between panes, open thread modal during active agent work, verify both chat and modal update live
