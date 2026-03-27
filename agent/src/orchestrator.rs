//! Shared orchestrator state for thread-based orchestration.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::thread::Episode;
use crate::types::AgentMessage;

/// A live thread's persistent state (conversation history for reuse).
struct LiveThread {
    messages: Vec<AgentMessage>,
    system_prompt: String,
    episodes: Vec<Episode>,
}

/// Result of looking up or creating a thread.
pub struct ThreadLookup {
    pub messages: Vec<AgentMessage>,
    pub system_prompt: String,
    pub is_reuse: bool,
}

/// Shared orchestrator state, safe for concurrent access from multiple threads.
pub struct OrchestratorState {
    threads: Mutex<HashMap<String, LiveThread>>,
    episode_log: Mutex<Vec<Episode>>,
    documents: Mutex<HashMap<String, String>>,
    counter: AtomicU64,
}

impl OrchestratorState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            threads: Mutex::new(HashMap::new()),
            episode_log: Mutex::new(Vec::new()),
            documents: Mutex::new(HashMap::new()),
            counter: AtomicU64::new(0),
        })
    }

    /// Generate a unique thread ID.
    pub fn next_thread_id(&self) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("t-{:04x}", n)
    }

    /// Get or create a live thread. Returns existing messages if reusing.
    pub fn get_or_create_thread(&self, alias: &str, base_system_prompt: &str) -> ThreadLookup {
        let mut threads = self.threads.lock().unwrap();
        if let Some(thread) = threads.get(alias) {
            ThreadLookup {
                messages: thread.messages.clone(),
                system_prompt: thread.system_prompt.clone(),
                is_reuse: true,
            }
        } else {
            threads.insert(
                alias.to_string(),
                LiveThread {
                    messages: Vec::new(),
                    system_prompt: base_system_prompt.to_string(),
                    episodes: Vec::new(),
                },
            );
            ThreadLookup {
                messages: Vec::new(),
                system_prompt: base_system_prompt.to_string(),
                is_reuse: false,
            }
        }
    }

    /// Record a completed episode and update the live thread's message history.
    pub fn record_episode(&self, episode: Episode, final_messages: Vec<AgentMessage>) {
        let alias = episode.alias.clone();
        {
            let mut threads = self.threads.lock().unwrap();
            if let Some(thread) = threads.get_mut(&alias) {
                thread.messages = final_messages;
                thread.episodes.push(episode.clone());
            }
        }
        self.episode_log.lock().unwrap().push(episode);
    }

    /// Retrieve the most recent episode for a given alias.
    pub fn get_episode(&self, alias: &str) -> Option<Episode> {
        let threads = self.threads.lock().unwrap();
        threads.get(alias).and_then(|t| t.episodes.last().cloned())
    }

    /// Retrieve the most recent episodes for multiple aliases.
    pub fn get_episodes(&self, aliases: &[String]) -> Vec<Episode> {
        let threads = self.threads.lock().unwrap();
        aliases
            .iter()
            .filter_map(|alias| {
                threads
                    .get(alias.as_str())
                    .and_then(|t| t.episodes.last().cloned())
            })
            .collect()
    }

    /// Get all episodes in sequence order.
    pub fn all_episodes(&self) -> Vec<Episode> {
        self.episode_log.lock().unwrap().clone()
    }

    /// Check if a thread alias exists.
    pub fn has_thread(&self, alias: &str) -> bool {
        self.threads.lock().unwrap().contains_key(alias)
    }

    /// Create a named virtual document for inter-thread data sharing.
    pub fn allocate_document(&self, name: &str) {
        self.documents
            .lock()
            .unwrap()
            .entry(name.to_string())
            .or_default();
    }

    /// Read a virtual document.
    pub fn read_document(&self, name: &str) -> Option<String> {
        self.documents.lock().unwrap().get(name).cloned()
    }

    /// Write to a virtual document.
    pub fn write_document(&self, name: &str, content: String) {
        self.documents
            .lock()
            .unwrap()
            .insert(name.to_string(), content);
    }

    /// Format prior episodes as a system prompt section for injection.
    pub fn format_prior_episodes(&self, aliases: &[String]) -> Option<String> {
        let episodes = self.get_episodes(aliases);
        if episodes.is_empty() {
            return None;
        }
        let mut out = String::from(
            "# Prior episodes\n\nThe following episodes contain context from other threads that may be relevant to your task.\n\n",
        );
        for ep in &episodes {
            out.push_str(&ep.compact_trace);
            out.push('\n');
        }
        Some(out)
    }
}

impl Default for OrchestratorState {
    fn default() -> Self {
        Self {
            threads: Mutex::new(HashMap::new()),
            episode_log: Mutex::new(Vec::new()),
            documents: Mutex::new(HashMap::new()),
            counter: AtomicU64::new(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thread::ThreadOutcome;

    fn make_episode(alias: &str, n: u32) -> Episode {
        Episode {
            thread_id: format!("t-{:04x}", n),
            alias: alias.to_string(),
            task: format!("Task for {}", alias),
            outcome: ThreadOutcome::Completed {
                result: format!("Done {}", n),
                evidence: vec![],
            },
            full_trace: format!("--- Thread: {} [completed] ---\nfull trace {}\n", alias, n),
            compact_trace: format!(
                "--- Thread: {} [completed] ---\nTASK: Task for {}\nRESULT: Done {}\n",
                alias, alias, n
            ),
            duration_ms: 1000,
            turn_count: 2,
        }
    }

    #[test]
    fn test_thread_creation_and_reuse() {
        let orch = OrchestratorState::new();

        let lookup1 = orch.get_or_create_thread("scanner", "You are a scanner.");
        assert!(!lookup1.is_reuse);
        assert!(lookup1.messages.is_empty());

        let lookup2 = orch.get_or_create_thread("scanner", "You are a scanner.");
        assert!(lookup2.is_reuse);
    }

    #[test]
    fn test_record_and_retrieve_episode() {
        let orch = OrchestratorState::new();
        orch.get_or_create_thread("scanner", "prompt");

        let ep = make_episode("scanner", 1);
        orch.record_episode(ep.clone(), vec![]);

        let retrieved = orch.get_episode("scanner").unwrap();
        assert_eq!(retrieved.thread_id, "t-0001");

        let all = orch.all_episodes();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn test_get_episodes_multiple() {
        let orch = OrchestratorState::new();
        orch.get_or_create_thread("a", "p");
        orch.get_or_create_thread("b", "p");

        orch.record_episode(make_episode("a", 1), vec![]);
        orch.record_episode(make_episode("b", 2), vec![]);

        let eps = orch.get_episodes(&["a".to_string(), "b".to_string(), "missing".to_string()]);
        assert_eq!(eps.len(), 2);
        assert_eq!(eps[0].alias, "a");
        assert_eq!(eps[1].alias, "b");
    }

    #[test]
    fn test_thread_id_generation() {
        let orch = OrchestratorState::new();
        assert_eq!(orch.next_thread_id(), "t-0000");
        assert_eq!(orch.next_thread_id(), "t-0001");
        assert_eq!(orch.next_thread_id(), "t-0002");
    }

    #[test]
    fn test_documents() {
        let orch = OrchestratorState::new();

        assert!(orch.read_document("findings").is_none());

        orch.allocate_document("findings");
        assert_eq!(orch.read_document("findings"), Some(String::new()));

        orch.write_document("findings", "endpoint: /login".to_string());
        assert_eq!(
            orch.read_document("findings"),
            Some("endpoint: /login".to_string())
        );
    }

    #[test]
    fn test_format_prior_episodes() {
        let orch = OrchestratorState::new();
        orch.get_or_create_thread("scanner", "p");
        orch.record_episode(make_episode("scanner", 1), vec![]);

        let section = orch
            .format_prior_episodes(&["scanner".to_string()])
            .unwrap();
        assert!(section.contains("# Prior episodes"));
        assert!(section.contains("Thread: scanner [completed]"));
        assert!(section.contains("RESULT: Done 1"));

        // Empty list returns None
        assert!(orch
            .format_prior_episodes(&["nonexistent".to_string()])
            .is_none());
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let orch = OrchestratorState::new();
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let orch = orch.clone();
                thread::spawn(move || {
                    let alias = format!("thread-{}", i);
                    orch.get_or_create_thread(&alias, "prompt");
                    orch.record_episode(make_episode(&alias, i), vec![]);
                    orch.write_document(&alias, format!("data-{}", i));
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(orch.all_episodes().len(), 10);
    }
}
