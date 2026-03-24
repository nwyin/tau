//! Skill discovery, parsing, and slash-command expansion.
//!
//! Skills are markdown files (`SKILL.md`) with YAML frontmatter that provide
//! domain-specific instructions to the agent. They follow a progressive
//! disclosure pattern: only name + description appear in the system prompt,
//! and the full content is loaded on-demand via `file_read`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillSource {
    Project,
    User,
    Cli,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub file_path: PathBuf,
    pub base_dir: PathBuf,
    pub source: SkillSource,
}

#[derive(Debug)]
pub struct SkillDiagnostic {
    pub message: String,
    pub path: PathBuf,
}

pub struct LoadedSkills {
    pub skills: Vec<Skill>,
    pub diagnostics: Vec<SkillDiagnostic>,
}

// ---------------------------------------------------------------------------
// Frontmatter parsing
// ---------------------------------------------------------------------------

/// Strip YAML frontmatter delimited by `---` lines, returning the body after it.
pub fn strip_frontmatter(content: &str) -> &str {
    // Must start with `---` on the first line
    if !content.starts_with("---") {
        return content;
    }
    // Find the closing `---`
    if let Some(end) = content[3..].find("\n---") {
        let after = end + 3 + 4; // skip past "\n---"
        if after < content.len() {
            content[after..].trim_start_matches('\n')
        } else {
            ""
        }
    } else {
        content
    }
}

/// Parse YAML frontmatter from a SKILL.md file, extracting `name` and `description`.
fn parse_frontmatter(content: &str) -> Option<(String, String)> {
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("\n---")?;
    let yaml_block = &rest[..end];

    let mut name: Option<String> = None;
    let mut description: Option<String> = None;

    for line in yaml_block.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("name:") {
            name = Some(val.trim().trim_matches('"').trim_matches('\'').to_string());
        } else if let Some(val) = line.strip_prefix("description:") {
            description = Some(val.trim().trim_matches('"').trim_matches('\'').to_string());
        }
    }

    match (name, description) {
        (Some(n), Some(d)) if !n.is_empty() && !d.is_empty() => Some((n, d)),
        _ => None,
    }
}

/// Validate a skill name: lowercase alphanumeric + hyphens, no leading/trailing
/// hyphens, no consecutive hyphens, 1-64 chars.
fn is_valid_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    if name.starts_with('-') || name.ends_with('-') {
        return false;
    }
    if name.contains("--") {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

// ---------------------------------------------------------------------------
// Skill file parsing
// ---------------------------------------------------------------------------

/// Parse a single SKILL.md file into a Skill, validating name and description.
pub fn parse_skill_file(path: &Path, source: SkillSource) -> Result<Skill, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;

    let (name, description) = parse_frontmatter(&content)
        .ok_or_else(|| "missing or incomplete frontmatter".to_string())?;

    if !is_valid_name(&name) {
        return Err(format!("invalid skill name '{}'", name));
    }

    // Name must match parent directory name
    if let Some(parent) = path.parent() {
        if let Some(dir_name) = parent.file_name().and_then(|n| n.to_str()) {
            if dir_name != name {
                return Err(format!(
                    "skill name '{}' does not match directory '{}'",
                    name, dir_name
                ));
            }
        }
    }

    let base_dir = path.parent().unwrap_or(path).to_path_buf();

    Ok(Skill {
        name,
        description,
        file_path: path.to_path_buf(),
        base_dir,
        source,
    })
}

// ---------------------------------------------------------------------------
// Directory scanning
// ---------------------------------------------------------------------------

/// Scan a directory for skills (non-recursive: `dir/<name>/SKILL.md`).
fn scan_skills_dir(dir: &Path, source: SkillSource) -> (Vec<Skill>, Vec<SkillDiagnostic>) {
    let mut skills = Vec::new();
    let mut diagnostics = Vec::new();

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return (skills, diagnostics),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_file = path.join("SKILL.md");
        if !skill_file.is_file() {
            continue;
        }

        match parse_skill_file(&skill_file, source.clone()) {
            Ok(skill) => skills.push(skill),
            Err(msg) => diagnostics.push(SkillDiagnostic {
                message: msg,
                path: skill_file,
            }),
        }
    }

    (skills, diagnostics)
}

/// Walk up from `cwd` to the git root (or filesystem root), collecting
/// `.tau/skills/` directories.
fn project_skills_dirs(cwd: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut current = Some(cwd);

    while let Some(dir) = current {
        let candidate = dir.join(".tau").join("skills");
        if candidate.is_dir() {
            dirs.push(candidate);
        }
        // Stop at git root
        if dir.join(".git").exists() {
            break;
        }
        current = dir.parent();
    }

    dirs
}

// ---------------------------------------------------------------------------
// Main discovery entry point
// ---------------------------------------------------------------------------

/// Discover and load all skills from project, user, and CLI-specified paths.
///
/// Deduplication: first skill with a given name wins.
pub fn load_skills(cwd: &Path, no_skills: bool, extra_paths: &[PathBuf]) -> LoadedSkills {
    let mut all_skills = Vec::new();
    let mut all_diagnostics = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    let mut add = |skills: Vec<Skill>, diagnostics: Vec<SkillDiagnostic>| {
        for skill in skills {
            if seen_names.contains(&skill.name) {
                // collision — skip silently (first wins)
                continue;
            }
            seen_names.insert(skill.name.clone());
            all_skills.push(skill);
        }
        all_diagnostics.extend(diagnostics);
    };

    if !no_skills {
        // 1. Project-local (walk up from cwd)
        for dir in project_skills_dirs(cwd) {
            let (skills, diags) = scan_skills_dir(&dir, SkillSource::Project);
            add(skills, diags);
        }

        // 2. User-global
        if let Ok(home) = std::env::var("HOME") {
            let user_dir = PathBuf::from(home).join(".tau").join("skills");
            let (skills, diags) = scan_skills_dir(&user_dir, SkillSource::User);
            add(skills, diags);
        }
    }

    // 3. CLI-specified paths (always loaded, even with --no-skills)
    for path in extra_paths {
        let path = if path.is_dir() {
            path.join("SKILL.md")
        } else {
            path.clone()
        };
        if path.is_file() {
            match parse_skill_file(&path, SkillSource::Cli) {
                Ok(skill) => {
                    if !seen_names.contains(&skill.name) {
                        seen_names.insert(skill.name.clone());
                        all_skills.push(skill);
                    }
                }
                Err(msg) => all_diagnostics.push(SkillDiagnostic { message: msg, path }),
            }
        }
    }

    LoadedSkills {
        skills: all_skills,
        diagnostics: all_diagnostics,
    }
}

// ---------------------------------------------------------------------------
// Slash command expansion
// ---------------------------------------------------------------------------

/// If input starts with `/skill:<name>`, expand it by reading the SKILL.md,
/// stripping frontmatter, and wrapping the body in an XML block.
///
/// Returns `None` if the input doesn't match a `/skill:` command.
pub fn expand_skill_command(input: &str, skills: &[Skill]) -> Option<String> {
    let rest = input.strip_prefix("/skill:")?;

    let (skill_name, args) = match rest.find(char::is_whitespace) {
        Some(pos) => (&rest[..pos], Some(rest[pos..].trim())),
        None => (rest, None),
    };

    let skill = skills.iter().find(|s| s.name == skill_name)?;

    let content = std::fs::read_to_string(&skill.file_path).ok()?;
    let body = strip_frontmatter(&content).trim();

    let mut expanded = format!(
        "<skill name=\"{}\" location=\"{}\">\nReferences are relative to {}.\n\n{}\n</skill>",
        skill.name,
        skill.file_path.display(),
        skill.base_dir.display(),
        body,
    );

    if let Some(args) = args {
        if !args.is_empty() {
            expanded.push_str("\n\n");
            expanded.push_str(args);
        }
    }

    Some(expanded)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_strip_frontmatter_basic() {
        let content = "---\nname: foo\ndescription: bar\n---\n# Hello\nworld";
        assert_eq!(strip_frontmatter(content), "# Hello\nworld");
    }

    #[test]
    fn test_strip_frontmatter_no_frontmatter() {
        let content = "# Hello\nworld";
        assert_eq!(strip_frontmatter(content), "# Hello\nworld");
    }

    #[test]
    fn test_strip_frontmatter_only_frontmatter() {
        let content = "---\nname: foo\n---";
        assert_eq!(strip_frontmatter(content), "");
    }

    #[test]
    fn test_parse_frontmatter_valid() {
        let content = "---\nname: my-skill\ndescription: Does things\n---\nbody";
        let (name, desc) = parse_frontmatter(content).unwrap();
        assert_eq!(name, "my-skill");
        assert_eq!(desc, "Does things");
    }

    #[test]
    fn test_parse_frontmatter_quoted() {
        let content = "---\nname: \"my-skill\"\ndescription: 'Does things'\n---\nbody";
        let (name, desc) = parse_frontmatter(content).unwrap();
        assert_eq!(name, "my-skill");
        assert_eq!(desc, "Does things");
    }

    #[test]
    fn test_parse_frontmatter_missing_description() {
        let content = "---\nname: my-skill\n---\nbody";
        assert!(parse_frontmatter(content).is_none());
    }

    #[test]
    fn test_parse_frontmatter_no_frontmatter() {
        assert!(parse_frontmatter("no frontmatter").is_none());
    }

    #[test]
    fn test_is_valid_name() {
        assert!(is_valid_name("my-skill"));
        assert!(is_valid_name("a"));
        assert!(is_valid_name("skill123"));
        assert!(!is_valid_name(""));
        assert!(!is_valid_name("-start"));
        assert!(!is_valid_name("end-"));
        assert!(!is_valid_name("double--hyphen"));
        assert!(!is_valid_name("UpperCase"));
        assert!(!is_valid_name("has space"));
        assert!(!is_valid_name(&"a".repeat(65)));
    }

    #[test]
    fn test_parse_skill_file() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("my-skill");
        fs::create_dir(&skill_dir).unwrap();
        let skill_file = skill_dir.join("SKILL.md");
        fs::write(
            &skill_file,
            "---\nname: my-skill\ndescription: A test skill\n---\n# Instructions\nDo stuff.",
        )
        .unwrap();

        let skill = parse_skill_file(&skill_file, SkillSource::Project).unwrap();
        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.description, "A test skill");
        assert_eq!(skill.file_path, skill_file);
        assert_eq!(skill.base_dir, skill_dir);
        assert_eq!(skill.source, SkillSource::Project);
    }

    #[test]
    fn test_parse_skill_file_name_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("wrong-name");
        fs::create_dir(&skill_dir).unwrap();
        let skill_file = skill_dir.join("SKILL.md");
        fs::write(
            &skill_file,
            "---\nname: my-skill\ndescription: A test skill\n---\nbody",
        )
        .unwrap();

        let result = parse_skill_file(&skill_file, SkillSource::Project);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not match"));
    }

    #[test]
    fn test_scan_skills_dir() {
        let dir = tempfile::tempdir().unwrap();
        let skills_root = dir.path().join("skills");
        fs::create_dir(&skills_root).unwrap();

        // Valid skill
        let s1 = skills_root.join("alpha");
        fs::create_dir(&s1).unwrap();
        fs::write(
            s1.join("SKILL.md"),
            "---\nname: alpha\ndescription: First skill\n---\nbody",
        )
        .unwrap();

        // Another valid skill
        let s2 = skills_root.join("beta");
        fs::create_dir(&s2).unwrap();
        fs::write(
            s2.join("SKILL.md"),
            "---\nname: beta\ndescription: Second skill\n---\nbody",
        )
        .unwrap();

        // Directory without SKILL.md (ignored)
        let s3 = skills_root.join("gamma");
        fs::create_dir(&s3).unwrap();

        let (skills, diagnostics) = scan_skills_dir(&skills_root, SkillSource::User);
        assert_eq!(skills.len(), 2);
        assert!(diagnostics.is_empty());

        let names: HashSet<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains("alpha"));
        assert!(names.contains("beta"));
    }

    #[test]
    fn test_load_skills_dedup() {
        let dir = tempfile::tempdir().unwrap();

        // Simulate project-local skills
        let project_skills = dir.path().join(".tau").join("skills");
        fs::create_dir_all(&project_skills).unwrap();
        let s1 = project_skills.join("dupe");
        fs::create_dir(&s1).unwrap();
        fs::write(
            s1.join("SKILL.md"),
            "---\nname: dupe\ndescription: Project version\n---\nbody",
        )
        .unwrap();

        // Create a .git to mark repo root
        fs::create_dir(dir.path().join(".git")).unwrap();

        let loaded = load_skills(dir.path(), false, &[]);
        // Should contain "dupe" from project (may also contain user-global skills from ~/.tau/skills/)
        let dupe = loaded.skills.iter().find(|s| s.name == "dupe").unwrap();
        assert_eq!(dupe.description, "Project version");
        assert_eq!(dupe.source, SkillSource::Project);
    }

    #[test]
    fn test_expand_skill_command() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("my-skill");
        fs::create_dir(&skill_dir).unwrap();
        let skill_file = skill_dir.join("SKILL.md");
        fs::write(
            &skill_file,
            "---\nname: my-skill\ndescription: test\n---\n# Instructions\nDo the thing.",
        )
        .unwrap();

        let skills = vec![Skill {
            name: "my-skill".into(),
            description: "test".into(),
            file_path: skill_file.clone(),
            base_dir: skill_dir.clone(),
            source: SkillSource::Project,
        }];

        // With args
        let result = expand_skill_command("/skill:my-skill hello world", &skills).unwrap();
        assert!(result.contains("<skill name=\"my-skill\""));
        assert!(result.contains("# Instructions\nDo the thing."));
        assert!(result.contains("hello world"));

        // Without args
        let result = expand_skill_command("/skill:my-skill", &skills).unwrap();
        assert!(result.contains("<skill name=\"my-skill\""));
        assert!(!result.contains("\n\nhello"));

        // Non-matching
        assert!(expand_skill_command("hello world", &skills).is_none());
        assert!(expand_skill_command("/skill:unknown", &skills).is_none());
    }

    #[test]
    fn test_no_skills_flag() {
        let dir = tempfile::tempdir().unwrap();

        let project_skills = dir.path().join(".tau").join("skills");
        fs::create_dir_all(&project_skills).unwrap();
        let s1 = project_skills.join("alpha");
        fs::create_dir(&s1).unwrap();
        fs::write(
            s1.join("SKILL.md"),
            "---\nname: alpha\ndescription: test\n---\nbody",
        )
        .unwrap();

        fs::create_dir(dir.path().join(".git")).unwrap();

        // With no_skills=true, auto-discovered skills are skipped
        let loaded = load_skills(dir.path(), true, &[]);
        assert!(loaded.skills.is_empty());

        // But explicit CLI paths still work
        let loaded = load_skills(dir.path(), true, &[s1]);
        assert_eq!(loaded.skills.len(), 1);
    }
}
