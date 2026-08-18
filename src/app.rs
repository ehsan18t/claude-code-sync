//! Walking the filesystem, building an archive, and putting one back.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::archive::{Entry, FileRecord, LinkRecord, Manifest, Result, ARCHIVE_VERSION, MANIFEST};
use crate::classify::{classify_entry, BackupOptions, Classification};
use crate::eol::{denormalize_for_disk, normalize_for_archive, LineEnding};
use crate::merge::{merge_settings, Choice, Conflict, MergeStrategy};
use crate::paths::{forward_slashes, resolve_path, tokenize_path, PathContext};

/// The only file whose contents are machine-specific.
const SETTINGS: &str = "claude/settings.json";

/// Overrides the home directory this tool reads and writes.
///
/// On Windows the home directory comes from a Win32 known-folder lookup, not from `USERPROFILE`,
/// so setting that variable does NOT redirect it. Without an explicit override there is no safe
/// way to exercise a real restore, which is how an early test wrote onto a live config.
pub const HOME_OVERRIDE: &str = "CLAUDE_SYNC_HOME";

pub fn home_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os(HOME_OVERRIDE) {
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    dirs::home_dir().ok_or_else(|| "could not determine the home directory".into())
}

/// Where node lives on this machine, so hook commands survive the trip.
///
/// Falling back to a bare `node` is deliberate: an unresolved `${NODE}` would be written into
/// settings.json verbatim and the hook would never run, whereas `node` resolves via PATH.
pub fn find_node() -> String {
    let binary = if cfg!(windows) { "node.exe" } else { "node" };
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(binary);
            if candidate.is_file() {
                return forward_slashes(&candidate.to_string_lossy());
            }
        }
    }
    "node".into()
}

pub fn current_context() -> Result<PathContext> {
    Ok(PathContext {
        home: forward_slashes(home_dir()?.to_string_lossy().trim_end_matches(['/', '\\'])),
        node: find_node(),
    })
}

/// The two roots an archive carries, keyed by their prefix inside it.
fn roots(home: &Path) -> Vec<(&'static str, PathBuf)> {
    vec![
        ("claude", home.join(".claude")),
        ("agents", home.join(".agents")),
    ]
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

struct Walked {
    files: Vec<(String, PathBuf)>,
    links: Vec<LinkRecord>,
    skipped_secrets: Vec<String>,
}

fn walk(root: &Path, prefix: &str, ctx: &PathContext, options: &BackupOptions) -> Result<Walked> {
    let mut found = Walked {
        files: Vec::new(),
        links: Vec::new(),
        skipped_secrets: Vec::new(),
    };
    if !root.exists() {
        return Ok(found);
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = fs::read_dir(&dir)?.collect::<std::io::Result<_>>()?;
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let absolute = entry.path();
            let relative = absolute.strip_prefix(root).unwrap_or(&absolute);
            let archive_path = format!("{prefix}/{}", forward_slashes(&relative.to_string_lossy()));
            let metadata = fs::symlink_metadata(&absolute)?;

            if metadata.is_symlink() {
                // Store the link, never its contents: the target lives elsewhere in the archive.
                if classify_entry(&archive_path, options) == Classification::Include {
                    let target = fs::read_link(&absolute)?;
                    found.links.push(LinkRecord {
                        path: archive_path,
                        target: tokenize_path(&forward_slashes(&target.to_string_lossy()), ctx),
                    });
                }
                continue;
            }

            if metadata.is_dir() {
                stack.push(absolute);
                continue;
            }

            match classify_entry(&archive_path, options) {
                Classification::Secret => found.skipped_secrets.push(archive_path),
                Classification::Include => found.files.push((archive_path, absolute)),
                Classification::Exclude => {}
            }
        }
    }

    found.files.sort_by(|a, b| a.0.cmp(&b.0));
    found.links.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

pub fn backup(
    options: &BackupOptions,
    created_at: String,
    host: String,
) -> Result<(Vec<u8>, Manifest)> {
    let ctx = current_context()?;
    let home = home_dir()?;
    let mut manifest = Manifest::new(created_at, host, options);
    let mut entries: Vec<Entry> = Vec::new();

    for (prefix, root) in roots(&home) {
        let walked = walk(&root, prefix, &ctx, options)?;
        for secret in &walked.skipped_secrets {
            eprintln!("  skipped secret: {secret}  (use --include-credentials to carry it)");
        }
        manifest.links.extend(walked.links);

        for (archive_path, absolute) in walked.files {
            let mut data = normalize_for_archive(&fs::read(&absolute)?);
            if archive_path == SETTINGS {
                let parsed: serde_json::Value = serde_json::from_slice(&data)?;
                let tokenized = map_strings(&parsed, &|s| tokenize_path(s, &ctx));
                data = format!("{}\n", serde_json::to_string_pretty(&tokenized)?).into_bytes();
            }
            manifest.files.push(FileRecord {
                path: archive_path.clone(),
                sha256: sha256_hex(&data),
                size: data.len(),
            });
            entries.push(Entry {
                path: archive_path,
                data,
            });
        }
    }

    let mut all = vec![Entry {
        path: MANIFEST.into(),
        data: serde_json::to_vec_pretty(&manifest)?,
    }];
    all.extend(entries);

    Ok((crate::archive::write_zip(&all)?, manifest))
}

/// Apply a transform to every string in a JSON tree, leaving structure and key order untouched.
pub fn map_strings(value: &serde_json::Value, f: &dyn Fn(&str) -> String) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(s) => Value::String(f(s)),
        Value::Array(items) => Value::Array(items.iter().map(|v| map_strings(v, f)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), map_strings(v, f)))
                .collect(),
        ),
        other => other.clone(),
    }
}

pub struct RestoreOptions<'a> {
    pub strategy: MergeStrategy,
    pub dry_run: bool,
    pub resolve: &'a mut dyn FnMut(&Conflict) -> Choice,
}

pub fn read_manifest(entries: &[Entry]) -> Result<Manifest> {
    let entry = entries
        .iter()
        .find(|e| e.path == MANIFEST)
        .ok_or("archive has no manifest.json; refusing to restore")?;
    let manifest: Manifest = serde_json::from_slice(&entry.data)?;

    if manifest.tool != "claude-code-sync" {
        return Err("not a claude-code-sync archive".into());
    }
    if manifest.version > ARCHIVE_VERSION {
        return Err(format!(
            "archive version {} is newer than this build understands",
            manifest.version
        )
        .into());
    }
    Ok(manifest)
}

pub fn restore(
    entries: &[Entry],
    manifest: &Manifest,
    options: &mut RestoreOptions,
) -> Result<Vec<String>> {
    let ctx = current_context()?;
    let home = home_dir()?;
    let eol = LineEnding::native();
    let mut actions = Vec::new();

    for entry in entries {
        if entry.path == MANIFEST {
            continue;
        }
        let Some(absolute) = target_path(&home, &entry.path) else {
            continue;
        };

        let mut data = entry.data.clone();
        if entry.path == SETTINGS {
            let archived: serde_json::Value = serde_json::from_slice(&data)?;
            let incoming = map_strings(&archived, &|s| resolve_path(s, &ctx));
            let existing = if absolute.exists() {
                serde_json::from_slice(&fs::read(&absolute)?)?
            } else {
                serde_json::json!({})
            };
            let merged = merge_settings(&existing, &incoming, options.strategy, options.resolve);
            data = format!("{}\n", serde_json::to_string_pretty(&merged)?).into_bytes();
        }
        let data = denormalize_for_disk(&data, eol);

        actions.push(format!(
            "{}  {}",
            if absolute.exists() {
                "update"
            } else {
                "create"
            },
            entry.path
        ));
        if options.dry_run {
            continue;
        }
        if let Some(parent) = absolute.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&absolute, data)?;
    }

    for link in &manifest.links {
        let Some(absolute) = target_path(&home, &link.path) else {
            continue;
        };
        if absolute.exists() {
            continue;
        }
        let target = resolve_path(&link.target, &ctx);

        actions.push(format!("link    {} -> {}", link.path, target));
        if options.dry_run {
            continue;
        }
        if let Some(parent) = absolute.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Err(error) = create_link(Path::new(&target), &absolute) {
            // Windows without developer mode cannot create links; a real copy is the honest fallback.
            let source = Path::new(&target);
            if source.exists() {
                copy_tree(source, &absolute)?;
            } else {
                eprintln!("  could not link or copy {}: {error}", link.path);
            }
        }
    }

    Ok(actions)
}

fn target_path(home: &Path, archive_path: &str) -> Option<PathBuf> {
    let (prefix, rest) = archive_path.split_once('/')?;
    let root = match prefix {
        "claude" => home.join(".claude"),
        "agents" => home.join(".agents"),
        _ => return None,
    };
    Some(rest.split('/').fold(root, |acc, part| acc.join(part)))
}

#[cfg(windows)]
fn create_link(target: &Path, link: &Path) -> std::io::Result<()> {
    if target.is_dir() {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    }
}

#[cfg(not(windows))]
fn create_link(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

fn copy_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    if source.is_file() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, destination)?;
        return Ok(());
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_archive_path_maps_onto_the_right_root() {
        let home = Path::new("/home/dev");
        assert_eq!(
            target_path(home, "claude/hooks/x.mjs"),
            Some(Path::new("/home/dev/.claude/hooks/x.mjs").to_path_buf())
        );
        assert_eq!(
            target_path(home, "agents/skills/grilling/SKILL.md"),
            Some(Path::new("/home/dev/.agents/skills/grilling/SKILL.md").to_path_buf())
        );
    }

    #[test]
    fn an_unknown_archive_prefix_maps_nowhere() {
        assert_eq!(target_path(Path::new("/home/dev"), "etc/passwd"), None);
        assert_eq!(target_path(Path::new("/home/dev"), "manifest.json"), None);
    }

    #[test]
    fn every_string_in_a_json_tree_is_transformed_and_key_order_is_kept() {
        let value = serde_json::json!({
            "z": "a", "a": ["b", { "c": "d" }], "n": 1, "t": true
        });
        let shouted = map_strings(&value, &|s| s.to_uppercase());
        assert_eq!(shouted["z"], serde_json::json!("A"));
        assert_eq!(shouted["a"], serde_json::json!(["B", { "c": "D" }]));
        assert_eq!(shouted["n"], serde_json::json!(1));
        assert_eq!(shouted["t"], serde_json::json!(true));
        let keys: Vec<&String> = shouted.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["z", "a", "n", "t"]);
    }

    #[test]
    fn a_missing_node_falls_back_to_the_bare_command_rather_than_an_unresolved_token() {
        // find_node never returns an empty string, so ${NODE} can never survive into settings.json.
        assert!(!find_node().is_empty());
    }
}
