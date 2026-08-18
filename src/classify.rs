//! Deciding what goes into an archive.
//!
//! An allowlist, not a denylist: anything unrecognised is excluded. A cache directory added in
//! some future Claude Code release then costs nothing, whereas a denylist would swallow it whole.

use crate::paths::forward_slashes;

#[derive(Debug, Clone, Copy)]
pub struct BackupOptions {
    /// Carry `projects/<project>/memory/`, which is distilled by hand and never regenerates.
    pub with_memory: bool,
    /// Carry `.credentials.json`. Off unless asked for: the archive is meant to travel.
    pub include_credentials: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    Include,
    Exclude,
    /// Refused, and worth telling the user about by name.
    Secret,
}

/// Directories under `~/.claude` whose contents are authored, not generated.
const PORTABLE_DIRS: &[&str] = &["agents", "commands", "hooks", "skills", "tools"];

/// Individual files at the root of `~/.claude` worth carrying.
const PORTABLE_FILES: &[&str] = &["CLAUDE.md", "settings.json", "keybindings.json"];

const CREDENTIALS: &str = "claude/.credentials.json";

/// Is this `projects/<something>/memory/...`?
fn is_memory_path(rest: &str) -> bool {
    let mut parts = rest.split('/');
    parts.next() == Some("projects")
        && parts.next().is_some_and(|p| !p.is_empty())
        && parts.next() == Some("memory")
        && parts.next().is_some()
}

/// Decide what happens to one archive-relative path.
pub fn classify_entry(rel_path: &str, options: &BackupOptions) -> Classification {
    let path = forward_slashes(rel_path);

    if path == CREDENTIALS {
        return if options.include_credentials {
            Classification::Include
        } else {
            Classification::Secret
        };
    }

    // Archives this tool writes must never be swept into the next one.
    if path.ends_with(".zip") {
        return Classification::Exclude;
    }

    // The whole ~/.agents tree is authored skill sources plus their lock file.
    if path.starts_with("agents/") {
        return Classification::Include;
    }

    let Some(rest) = path.strip_prefix("claude/") else {
        return Classification::Exclude;
    };

    if is_memory_path(rest) {
        return if options.with_memory {
            Classification::Include
        } else {
            Classification::Exclude
        };
    }

    match rest.split_once('/') {
        Some((head, _)) if PORTABLE_DIRS.contains(&head) => Classification::Include,
        Some(_) => Classification::Exclude,
        None if PORTABLE_FILES.contains(&rest) => Classification::Include,
        None => Classification::Exclude,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULTS: BackupOptions = BackupOptions { with_memory: false, include_credentials: false };
    const WITH_MEMORY: BackupOptions = BackupOptions { with_memory: true, include_credentials: false };
    const WITH_CREDS: BackupOptions = BackupOptions { with_memory: false, include_credentials: true };

    #[test]
    fn hand_written_config_is_included() {
        for path in [
            "claude/CLAUDE.md",
            "claude/settings.json",
            "claude/agents/skeptic.md",
            "claude/commands/lean.md",
            "claude/hooks/load-repo-rules.mjs",
            "claude/skills/lean-orchestration/SKILL.md",
            "claude/tools/helper.ts",
        ] {
            assert_eq!(classify_entry(path, &DEFAULTS), Classification::Include, "{path}");
        }
    }

    #[test]
    fn the_agents_skill_source_tree_is_included() {
        assert_eq!(
            classify_entry("agents/skills/grilling/SKILL.md", &DEFAULTS),
            Classification::Include
        );
        assert_eq!(classify_entry("agents/.skill-lock.json", &DEFAULTS), Classification::Include);
    }

    #[test]
    fn bulk_generated_state_is_excluded() {
        for path in [
            "claude/projects/c--foo/session-abc.jsonl",
            "claude/plugins/repos/official/README.md",
            "claude/security/db.sqlite",
            "claude/context-mode/index.db",
            "claude/file-history/x.json",
            "claude/shell-snapshots/snap-1.sh",
            "claude/sessions/s.json",
            "claude/cache/blob",
            "claude/debug/log.txt",
            "claude/telemetry/events.jsonl",
            "claude/history.jsonl",
            "claude/stats-cache.json",
        ] {
            assert_eq!(classify_entry(path, &DEFAULTS), Classification::Exclude, "{path}");
        }
    }

    #[test]
    fn credentials_are_refused_by_default_and_flagged_rather_than_merely_skipped() {
        assert_eq!(classify_entry("claude/.credentials.json", &DEFAULTS), Classification::Secret);
    }

    #[test]
    fn credentials_are_included_only_behind_the_explicit_opt_in() {
        assert_eq!(classify_entry("claude/.credentials.json", &WITH_CREDS), Classification::Include);
    }

    #[test]
    fn memory_is_excluded_by_default() {
        assert_eq!(
            classify_entry("claude/projects/c--foo/memory/MEMORY.md", &DEFAULTS),
            Classification::Exclude
        );
    }

    #[test]
    fn memory_is_included_with_the_flag_while_transcripts_stay_out() {
        assert_eq!(
            classify_entry("claude/projects/c--foo/memory/MEMORY.md", &WITH_MEMORY),
            Classification::Include
        );
        assert_eq!(
            classify_entry("claude/projects/c--foo/session.jsonl", &WITH_MEMORY),
            Classification::Exclude
        );
    }

    #[test]
    fn a_memory_directory_outside_projects_is_not_mistaken_for_a_memory_store() {
        assert_eq!(
            classify_entry("claude/skills/memory/SKILL.md", &WITH_MEMORY),
            Classification::Include
        );
        assert_eq!(classify_entry("claude/cache/memory/blob", &WITH_MEMORY), Classification::Exclude);
    }

    #[test]
    fn an_archive_is_not_swept_into_the_next_backup() {
        assert_eq!(
            classify_entry("claude/tools/claude-sync-host-20260819.zip", &DEFAULTS),
            Classification::Exclude
        );
        assert_eq!(
            classify_entry("claude/backups/pre-restore-20260819.zip", &DEFAULTS),
            Classification::Exclude
        );
    }

    #[test]
    fn an_unknown_top_level_file_is_excluded_rather_than_guessed() {
        assert_eq!(classify_entry("claude/some-future-cache.bin", &DEFAULTS), Classification::Exclude);
    }

    #[test]
    fn classification_does_not_depend_on_path_separator_style() {
        assert_eq!(
            classify_entry(r"claude\hooks\load-repo-rules.mjs", &DEFAULTS),
            Classification::Include
        );
    }
}
