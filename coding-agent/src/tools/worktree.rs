//! Git worktree lifecycle management for thread isolation.
//!
//! Each write-capable thread can get its own git worktree + branch so that
//! parallel threads don't clobber each other's file edits.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Info about a created worktree for a thread.
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    /// Absolute path to the worktree directory.
    pub path: PathBuf,
    /// Branch name (e.g., "tau/fix-auth").
    pub branch: String,
    /// Whether the branch already existed (reuse scenario).
    pub reused_branch: bool,
}

/// Find the root of the git repository containing `dir`.
pub fn find_repo_root(dir: &Path) -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .context("failed to run git rev-parse --show-toplevel")?;
    if !output.status.success() {
        anyhow::bail!("not inside a git repository");
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

/// Check if a git branch exists locally.
fn branch_exists(repo_root: &Path, branch: &str) -> bool {
    std::process::Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{}", branch)])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Get the current HEAD commit hash.
fn head_ref(repo_root: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .context("failed to run git rev-parse HEAD")?;
    if !output.status.success() {
        anyhow::bail!(
            "git rev-parse HEAD failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Sanitize an alias for use in branch names and directory paths.
fn sanitize_alias(alias: &str) -> String {
    alias
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Ensure `.tau-worktrees` is in `.gitignore`.
fn ensure_gitignore(repo_root: &Path) {
    let gitignore = repo_root.join(".gitignore");
    let entry = ".tau-worktrees/";
    if let Ok(content) = std::fs::read_to_string(&gitignore) {
        if content.lines().any(|l| l.trim() == entry) {
            return;
        }
    }
    // Append the entry
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&gitignore)
        .ok();
    if let Some(ref mut f) = file {
        use std::io::Write;
        let _ = writeln!(f, "\n# tau worktrees\n{}", entry);
    }
}

/// Create (or reuse) a git worktree for a thread.
///
/// Branch naming: `tau/{sanitized_alias}`.
/// Worktree directory: `{repo_root}/.tau-worktrees/{alias}-{thread_id}/`
///
/// If `base_alias` is provided, the worktree branches from `tau/{base_alias}`
/// instead of HEAD. This lets verifier threads run against a worker's changes.
///
/// If `include_paths` is provided, those paths (relative to repo_root) are copied
/// into the worktree after creation. Use for untracked directories like test suites
/// that aren't in git but need to be visible to the thread.
pub fn create_worktree(
    repo_root: &Path,
    alias: &str,
    thread_id: &str,
    base_alias: Option<&str>,
    include_paths: &[String],
) -> Result<WorktreeInfo> {
    let sanitized = sanitize_alias(alias);
    let branch = format!("tau/{}", sanitized);
    let wt_dir_name = format!("{}-{}", sanitized, thread_id);
    let wt_base = repo_root.join(".tau-worktrees");
    let wt_path = wt_base.join(&wt_dir_name);

    std::fs::create_dir_all(&wt_base).context("failed to create .tau-worktrees directory")?;
    ensure_gitignore(repo_root);

    let reused = branch_exists(repo_root, &branch);

    // Determine the base ref: another thread's branch, or HEAD
    let base_ref = if let Some(base) = base_alias {
        let base_branch = format!("tau/{}", sanitize_alias(base));
        if !branch_exists(repo_root, &base_branch) {
            anyhow::bail!(
                "worktree_base '{}' refers to branch '{}' which does not exist",
                base,
                base_branch
            );
        }
        base_branch
    } else {
        head_ref(repo_root)?
    };

    let output = if reused {
        // Existing branch — create worktree from it
        std::process::Command::new("git")
            .args(["worktree", "add", &wt_path.to_string_lossy(), &branch])
            .current_dir(repo_root)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .context("failed to create worktree from existing branch")?
    } else {
        // New branch from base_ref
        std::process::Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                &branch,
                &wt_path.to_string_lossy(),
                &base_ref,
            ])
            .current_dir(repo_root)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .context("failed to create worktree with new branch")?
    };

    if !output.status.success() {
        anyhow::bail!(
            "git worktree add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Copy included paths into the worktree.
    // We copy into the parent directory so `cp -a src parent/` yields `parent/src_name/...`
    // instead of the buggy `parent/src_name/src_name/...`.
    for rel_path in include_paths {
        let src = repo_root.join(rel_path);
        let dst = wt_path.join(rel_path);
        if src.exists() {
            if let Some(parent) = dst.parent() {
                let _ = std::fs::create_dir_all(parent);
                let status = std::process::Command::new("cp")
                    .args(["-a", &src.to_string_lossy(), &parent.to_string_lossy()])
                    .output();
                if let Err(e) = status {
                    eprintln!(
                        "[worktree] failed to copy include path '{}': {}",
                        rel_path, e
                    );
                }
            }
        }
    }

    Ok(WorktreeInfo {
        path: wt_path,
        branch,
        reused_branch: reused,
    })
}

/// Remove a worktree directory (but keep the branch).
pub fn remove_worktree(repo_root: &Path, wt_path: &Path) {
    let result = std::process::Command::new("git")
        .args(["worktree", "remove", "--force", &wt_path.to_string_lossy()])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    if result.map(|s| !s.success()).unwrap_or(true) {
        // Fallback: remove dir manually and prune
        let _ = std::fs::remove_dir_all(wt_path);
        let _ = std::process::Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(repo_root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// Auto-commit all changes in the worktree. Returns true if a commit was made.
pub fn auto_commit(wt_path: &Path, alias: &str, thread_id: &str) -> Result<bool> {
    // Check for changes
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(wt_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()?;
    let status_text = String::from_utf8_lossy(&status.stdout);
    if status_text.trim().is_empty() {
        return Ok(false);
    }

    // Stage all changes
    std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(wt_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;

    // Commit
    let msg = format!("tau: {} ({})", alias, thread_id);
    let commit = std::process::Command::new("git")
        .args(["commit", "-m", &msg])
        .current_dir(wt_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .status()?;

    Ok(commit.success())
}

/// Get `git diff --stat` for a branch relative to its merge-base with HEAD.
pub fn diff_stat(repo_root: &Path, branch: &str) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["diff", "--stat", &format!("HEAD...{}", branch)])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .context("failed to run git diff --stat")?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get full diff for a branch relative to its merge-base with HEAD.
/// Truncated to `max_bytes` to avoid enormous results.
pub fn diff_full(repo_root: &Path, branch: &str, max_bytes: usize) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["diff", &format!("HEAD...{}", branch)])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .context("failed to run git diff")?;
    let text = String::from_utf8_lossy(&output.stdout);
    if text.len() > max_bytes {
        Ok(format!(
            "{}\n\n[... diff truncated at {} bytes, {} total ...]",
            &text[..max_bytes],
            max_bytes,
            text.len()
        ))
    } else {
        Ok(text.trim().to_string())
    }
}

/// Parse the `--stat` summary line to extract file count.
/// e.g. " 3 files changed, 45 insertions(+), 12 deletions(-)"
pub fn parse_stat_summary(stat: &str) -> (usize, usize, usize) {
    let summary_line = stat.lines().last().unwrap_or("");
    let files = extract_number(summary_line, "file");
    let insertions = extract_number(summary_line, "insertion");
    let deletions = extract_number(summary_line, "deletion");
    (files, insertions, deletions)
}

fn extract_number(line: &str, keyword: &str) -> usize {
    // Look for "N keyword" pattern
    for (i, word) in line.split_whitespace().enumerate() {
        if word.starts_with(keyword) {
            // The number is the previous word
            if i > 0 {
                let words: Vec<&str> = line.split_whitespace().collect();
                return words[i - 1].parse().unwrap_or(0);
            }
        }
    }
    0
}

/// List all tau thread branches.
pub fn list_branches(repo_root: &Path) -> Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .args(["branch", "--list", "tau/*", "--format=%(refname:short)"])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .context("failed to list branches")?;
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// Detect the main/default branch name (main, master, etc.)
fn detect_main_branch(repo_root: &Path) -> Result<String> {
    // First check current branch
    let output = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .context("failed to detect current branch")?;
    let current = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // If we're on a non-tau branch, that's likely our target
    if !current.is_empty() && !current.starts_with("tau/") {
        return Ok(current);
    }

    // Otherwise, try common names
    for name in &["main", "master"] {
        if branch_exists(repo_root, name) {
            return Ok(name.to_string());
        }
    }

    anyhow::bail!("could not detect main branch")
}

/// Try to merge a branch into the main branch. Returns (success, conflicts, message).
///
/// Ensures the repo is on the main branch before merging. This is critical because
/// worktree operations can leave HEAD in an unexpected state.
pub fn merge_branch(repo_root: &Path, branch: &str) -> Result<(bool, Vec<String>, String)> {
    // Ensure we're on the main branch before merging
    let main_branch = detect_main_branch(repo_root)?;
    let checkout = std::process::Command::new("git")
        .args(["checkout", &main_branch])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .context("failed to checkout main branch for merge")?;
    if !checkout.status.success() {
        let err = String::from_utf8_lossy(&checkout.stderr);
        anyhow::bail!("failed to checkout {}: {}", main_branch, err);
    }

    let output = std::process::Command::new("git")
        .args(["merge", branch, "--no-edit"])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .context("failed to run git merge")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let message = format!("{}{}", stdout, stderr);

    if output.status.success() {
        return Ok((true, vec![], message));
    }

    // Merge failed — find conflicting files
    let conflicts_output = std::process::Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();

    let conflicts: Vec<String> = conflicts_output
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.to_string())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Abort the merge to leave the repo clean
    let _ = std::process::Command::new("git")
        .args(["merge", "--abort"])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    Ok((false, conflicts, message))
}
