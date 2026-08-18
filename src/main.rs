use std::io::{self, BufRead, Write};

use claude_code_sync::app::{backup, home_dir, read_manifest, restore, RestoreOptions};
use claude_code_sync::archive::{read_zip, Result};
use claude_code_sync::classify::BackupOptions;
use claude_code_sync::merge::{find_conflicts, Choice, Conflict, MergeStrategy};
use claude_code_sync::paths::resolve_path;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
claude-code-sync: portable backup and restore for a Claude Code setup

  claude-code-sync backup  [--with-memory] [--include-credentials] [--out DIR]
  claude-code-sync restore <archive.zip> [--merge=STRATEGY] [--dry-run]
  claude-code-sync --version

Merge strategies, applied to settings.json only:
  incoming   deep merge, the archive wins a conflict          (default)
  existing   deep merge, this machine wins a conflict
  replace    discard this machine's settings entirely
  ask        prompt for each conflicting key

Always carried:  CLAUDE.md, settings.json, agents/, commands/, hooks/, skills/, tools/,
                 and the whole ~/.agents skill source tree.
Never carried:   transcripts, plugin checkouts, caches, telemetry, session state.
                 Plugins reinstall themselves from settings.json.";

struct Args {
    command: Option<String>,
    positional: Vec<String>,
    flags: Vec<String>,
    values: Vec<(String, String)>,
}

impl Args {
    /// Accepts both `--out=DIR` and `--out DIR`.
    fn parse(raw: Vec<String>) -> Self {
        const VALUE_FLAGS: &[&str] = &["--out", "--merge"];
        let mut parsed = Args {
            command: None,
            positional: Vec::new(),
            flags: Vec::new(),
            values: Vec::new(),
        };
        let mut index = 0;

        while index < raw.len() {
            let argument = &raw[index];
            if let Some((name, value)) = argument.split_once('=') {
                if name.starts_with("--") {
                    parsed.values.push((name.to_string(), value.to_string()));
                    index += 1;
                    continue;
                }
            }
            if VALUE_FLAGS.contains(&argument.as_str()) {
                if let Some(value) = raw.get(index + 1) {
                    parsed.values.push((argument.clone(), value.clone()));
                    index += 2;
                    continue;
                }
            }
            if argument.starts_with('-') {
                parsed.flags.push(argument.clone());
            } else if parsed.command.is_none() {
                parsed.command = Some(argument.clone());
            } else {
                parsed.positional.push(argument.clone());
            }
            index += 1;
        }

        parsed
    }

    fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

fn ask_for(conflicts: &[Conflict]) -> Vec<(String, Choice)> {
    let stdin = io::stdin();
    let mut answers = Vec::new();

    for conflict in conflicts {
        println!("\n  {}", conflict.path);
        println!("    this machine: {}", conflict.existing);
        println!("    archive:      {}", conflict.incoming);
        print!("    keep [l]ocal or take [a]rchive? ");
        let _ = io::stdout().flush();

        let mut reply = String::new();
        let choice = match stdin.lock().read_line(&mut reply) {
            Ok(_) if reply.trim().to_lowercase().starts_with('l') => Choice::Existing,
            _ => Choice::Incoming,
        };
        answers.push((conflict.path.clone(), choice));
    }

    answers
}

fn run() -> Result<i32> {
    let args = Args::parse(std::env::args().skip(1).collect());

    if args.has("--version") || args.has("-V") || args.command.as_deref() == Some("version") {
        println!("claude-code-sync {VERSION}");
        return Ok(0);
    }
    if args.has("--help") || args.has("-h") || args.command.as_deref() == Some("help") {
        println!("{USAGE}");
        return Ok(0);
    }

    let now = chrono::Local::now();
    let host = gethostname::gethostname().to_string_lossy().to_string();

    match args.command.as_deref() {
        Some("backup") => {
            let options = BackupOptions {
                with_memory: args.has("--with-memory"),
                include_credentials: args.has("--include-credentials"),
            };
            if options.include_credentials {
                eprintln!(
                    "WARNING: this archive will contain .credentials.json. Do not share it.\n"
                );
            }

            let (archive, manifest) = backup(&options, now.to_rfc3339(), host.clone())?;

            let directory = args
                .value("--out")
                .map(std::path::PathBuf::from)
                .unwrap_or(std::env::current_dir()?);
            std::fs::create_dir_all(&directory)?;
            let name = format!("claude-sync-{host}-{}.zip", now.format("%Y%m%d-%H%M%S"));
            let out = directory.join(name);
            std::fs::write(&out, &archive)?;

            println!(
                "{} files, {} links",
                manifest.files.len(),
                manifest.links.len()
            );
            println!(
                "{:.1} KB -> {}",
                archive.len() as f64 / 1024.0,
                out.display()
            );
            Ok(0)
        }

        Some("restore") => {
            let Some(path) = args.positional.first() else {
                eprintln!("restore needs an archive path\n\n{USAGE}");
                return Ok(1);
            };

            let Some(strategy) = MergeStrategy::parse(args.value("--merge").unwrap_or("incoming"))
            else {
                eprintln!(
                    "unknown merge strategy: {}",
                    args.value("--merge").unwrap_or("")
                );
                return Ok(1);
            };

            let dry_run = args.has("--dry-run");
            let entries = read_zip(std::fs::read(path)?)?;
            let manifest = read_manifest(&entries)?;

            // Conflicts are collected up front so the interactive prompt is not tangled into
            // the recursive merge.
            let mut answers: Vec<(String, Choice)> = Vec::new();
            if strategy == MergeStrategy::Ask {
                let settings = home_dir()?.join(".claude").join("settings.json");
                let archived = entries.iter().find(|e| e.path == "claude/settings.json");
                if let (Some(archived), true) = (archived, settings.exists()) {
                    let ctx = claude_code_sync::app::current_context()?;
                    let parsed: serde_json::Value = serde_json::from_slice(&archived.data)?;
                    let incoming =
                        claude_code_sync::app::map_strings(&parsed, &|s| resolve_path(s, &ctx));
                    let existing: serde_json::Value =
                        serde_json::from_slice(&std::fs::read(&settings)?)?;
                    answers = ask_for(&find_conflicts(&existing, &incoming));
                }
            }

            if !dry_run {
                let directory = home_dir()?.join(".claude").join("backups");
                let (snapshot, _) = backup(
                    &BackupOptions {
                        with_memory: true,
                        include_credentials: false,
                    },
                    now.to_rfc3339(),
                    host.clone(),
                )?;
                std::fs::create_dir_all(&directory)?;
                let out =
                    directory.join(format!("pre-restore-{}.zip", now.format("%Y%m%d-%H%M%S")));
                std::fs::write(&out, snapshot)?;
                println!("snapshot of current config -> {}\n", out.display());
            }

            let mut resolve = |conflict: &Conflict| {
                answers
                    .iter()
                    .find(|(path, _)| path == &conflict.path)
                    .map(|(_, choice)| *choice)
                    .unwrap_or(Choice::Incoming)
            };
            let mut options = RestoreOptions {
                strategy,
                dry_run,
                resolve: &mut resolve,
            };
            let actions = restore(&entries, &manifest, &mut options)?;

            for action in &actions {
                println!("  {action}");
            }
            println!(
                "\n{} {}",
                actions.len(),
                if dry_run {
                    "planned (dry run, nothing written)"
                } else {
                    "applied"
                }
            );
            Ok(0)
        }

        Some(other) => {
            eprintln!("unknown command: {other}\n\n{USAGE}");
            Ok(1)
        }
        None => {
            println!("{USAGE}");
            Ok(0)
        }
    }
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    }
}
