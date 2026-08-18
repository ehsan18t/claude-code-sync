# claude-code-sync

**Move your Claude Code setup to another machine without it silently breaking.**

`claude-code-sync backup` packs your `~/.claude` config into a single zip. `claude-code-sync restore` unpacks it on any other machine, on any OS, and rewrites every machine-specific path inside it so your hooks, skills, agents and permissions actually work there.

One static binary. No Node, no Python, no runtime, no admin rights.

```console
$ claude-code-sync backup
  skipped secret: claude/.credentials.json  (use --include-credentials to carry it)
68 files, 22 links
141.2 KB -> claude-sync-DESKTOP-P9TI8DK-20260819-043203.zip

$ claude-code-sync restore claude-sync-DESKTOP-P9TI8DK-20260819-043203.zip --dry-run
  create  claude/CLAUDE.md
  create  claude/agents/navigator.md
  create  claude/hooks/load-repo-rules.mjs
  update  claude/settings.json
  link    claude/skills/writing-great-skills -> /home/you/.agents/skills/writing-great-skills
  ...
90 planned (dry run, nothing written)
```

## Why not just copy the folder

Because it breaks in two ways that never print an error.

**Hooks point at absolute paths.** Your `settings.json` stores hook commands as full paths, like `"C:/Program Files/nodejs/node.exe" "C:/Users/you/.claude/hooks/my-hook.mjs"`. Copy that to a Mac, or to the same PC under a different username, and the hook is still listed, still enabled, and never fires again. Nothing warns you. You just lose the behavior and have no reason to suspect it.

**Skills are usually symlinks.** If you install skills from a shared source, entries in `~/.claude/skills/` point into somewhere like `~/.agents/skills/`. A plain zip captures the dangling pointer instead of the content, so you unpack a skills directory full of broken links.

This tool stores those paths as `${HOME}` and `${NODE}` tokens and re-expands them against the target machine, records symlinks separately and recreates them, carries the real skill sources they point at, and normalizes line endings so one archive works on Windows, Linux and macOS alike.

## What it does

- **Path rewriting that survives a round trip.** Absolute paths become tokens on backup and real paths again on restore, and running it twice on the same machine produces the identical file rather than a near-duplicate.
- **Symlinks preserved, not flattened.** Links are recorded and recreated, with the `~/.agents` source tree carried alongside. Where the OS refuses to create a link, it falls back to a copy instead of failing.
- **Cross-OS by default.** Archives store LF and are rewritten to the target's native line ending on restore. Binaries are detected and passed through untouched.
- **Merges rather than clobbers.** `settings.json` is deep merged under a strategy you pick, with arrays unioned and key order preserved, so syncing from one machine does not wipe a permission you added on the other.
- **Never overwrites without a snapshot.** A full pre-restore backup is written every time, automatically.
- **Secrets stay put.** `.credentials.json` is refused by default and cannot be pulled in through the user include list.
- **Curated, not everything.** Transcripts, caches and session state are left behind, so a backup is about 140 KB rather than hundreds of megabytes.

## Commands

```
claude-code-sync backup  [--with-memory] [--include-credentials] [--out DIR]
claude-code-sync restore <archive.zip> [--merge=STRATEGY] [--dry-run]
claude-code-sync --help | --version
```

Both `--flag=value` and `--flag value` are accepted.

### `backup`

Writes `claude-sync-<host>-<YYYYMMDD-HHMMSS>.zip` to the current directory.

| Flag | Effect |
|---|---|
| `--out DIR` | Write the archive to `DIR` instead, creating it if needed |
| `--with-memory` | Also carry `projects/*/memory/`, your distilled per-project memory files |
| `--include-credentials` | Also carry `.credentials.json`. Refused without this flag, and it prints a warning when used. Do not share such an archive |

### `restore`

Unpacks an archive onto this machine, re-expanding every token against this machine's home and node.

| Flag | Effect |
|---|---|
| `--dry-run` | Print every create, update and link without writing anything. Skips the snapshot too |
| `--merge=STRATEGY` | How to reconcile `settings.json`. One of `incoming`, `existing`, `replace`, `ask`. Defaults to `incoming` |

**Run `--dry-run` first.** It shows the exact plan, and it is the cheapest way to confirm an archive is what you think it is.

### Global

| Flag | Effect |
|---|---|
| `--help`, `-h` | Usage summary. Also the default when run with no command |
| `--version`, `-V` | Print the version |

### Environment

| Variable | Effect |
|---|---|
| `CLAUDE_SYNC_HOME` | Treat this directory as home instead of the real one. Use it to test a restore against a throwaway directory before touching your live config |

On Windows the home directory comes from a Win32 known-folder lookup, **not** from `USERPROFILE`, so setting that variable redirects nothing. `CLAUDE_SYNC_HOME` is the only override.

```sh
CLAUDE_SYNC_HOME=/tmp/fake claude-code-sync restore archive.zip
```

## Merge strategies

Only `settings.json` is merged. Every other file is written as-is.

| Strategy | Behavior |
|---|---|
| `incoming` | Deep merge, the archive wins a conflict. **Default** |
| `existing` | Deep merge, this machine wins a conflict |
| `replace` | Discard this machine's settings entirely |
| `ask` | Prompt for each conflicting key, showing both values |

Under every strategy except `replace`, objects merge key by key and arrays are unioned with local order preserved. Only a differing scalar, or a type mismatch, counts as a conflict. Key order is preserved, so your file keeps its shape and new keys are appended rather than the whole thing being reshuffled.

## What travels

| Carried | Why |
|---|---|
| `CLAUDE.md`, `settings.json`, `settings.local.json`, `keybindings.json` | Authored by you |
| `agents/ commands/ hooks/ skills/ output-styles/ tools/` | Authored by you |
| `plugins/installed_plugins.json`, `known_marketplaces.json`, `blocklist.json` | Version and commit pins. `settings.json` records only which plugins are on, not which version, so without these a restore drifts to latest |
| Root-level dotfiles such as `.i-have-adhd-always` | Feature markers set by plugins. Tiny, unpredictable, and losing one silently changes behavior |
| `~/.agents/skills/` and `.skill-lock.json` | The real skill sources, when `~/.claude/skills/` symlinks into them |
| `projects/*/memory/` | Only with `--with-memory`. Distilled over time, never regenerates |

| Left behind | Why |
|---|---|
| `projects/*.jsonl` transcripts | Regenerated, and large |
| `plugins/cache/ repos/ marketplaces/`, `plugin-catalog-cache.json` | Re-downloaded from the pins above |
| `security/ cache/ file-history/ shell-snapshots/ sessions/ debug/ telemetry/` | Derived state |
| `.last-cleanup`, `.last-update-result.json` | Tool state, not your choices |
| `.credentials.json` | An auth token. Never in an archive that travels, unless you pass `--include-credentials` |
| Any `.zip` | Stops an archive being swept into the next one |

### Carrying something else

The rules above are an allowlist, so anything unrecognised is dropped rather than swallowed. That keeps a future cache directory out of your backups, but it also means a file nobody anticipated gets missed. The escape hatch is `~/.claude/.claude-sync-include`:

```
# one archive-relative path per line, trailing / for a whole subtree
claude/history.jsonl
claude/context-mode/
```

Paths are archive-relative: `claude/…` is `~/.claude`, `agents/…` is `~/.agents`. The list itself is carried, so it travels with your setup. `.credentials.json` is checked before this list and cannot be included through it.

To see exactly what an archive holds, read `manifest.json` inside the zip. It lists every file with its size and SHA-256, plus the tool version, source OS, hostname and timestamp.

## Install

One line. No runtime, no admin rights.

**Windows** (PowerShell):

```powershell
irm https://raw.githubusercontent.com/ehsan18t/claude-code-sync/main/install.ps1 | iex
```

**Linux and macOS**:

```sh
curl -fsSL https://raw.githubusercontent.com/ehsan18t/claude-code-sync/main/install.sh | sh
```

Then open a new terminal and run `claude-code-sync`.

Windows installs to `%LOCALAPPDATA%\Programs\claude-code-sync` and adds it to your user PATH. Linux and macOS install to `/usr/local/bin`, falling back to `~/.local/bin` when that is not writable. Set `BINDIR` to choose somewhere else.

### Uninstall

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/ehsan18t/claude-code-sync/main/install.ps1))) -Uninstall
```

```sh
curl -fsSL https://raw.githubusercontent.com/ehsan18t/claude-code-sync/main/install.sh | sh -s -- --uninstall
```

Both remove the binary and the PATH entry. Neither touches your `~/.claude` config or your backups.

### Homebrew

Not available yet. `brew install claude-code-sync` needs the project in homebrew-core, which requires 75 or more stars, forks or watchers. A personal tap works before then, and the formula is ready at [packaging/homebrew/claude-code-sync.rb](packaging/homebrew/claude-code-sync.rb): create a repo named `homebrew-tap`, copy the formula to `Formula/`, fill in the version and checksums from a release's `SHA256SUMS`, and it becomes `brew install ehsan18t/tap/claude-code-sync`. Until then, the `curl` line above works on macOS.

## How it works

### Path rewriting

Only the matched home or node prefix, and the path immediately following it, are rewritten. Everything else in a string is copied through byte for byte, and a backslash counts as a separator only when what follows could start a path segment.

That last rule matters more than it sounds. A permission rule like `Bash(cp "C:/Users/you/app/src/\(payload\)/index.tsx")` has backslashes that are shell escapes, not separators. Converting them produces a different string that no longer dedupes against itself, so every restore appends another near-duplicate entry to `permissions.allow`. Tokenizing is idempotent here, and there is a test that proves it.

### Line endings

The archive always stores LF. On restore, files are rewritten to the target's native ending: CRLF on Windows, LF elsewhere. Binary files are detected by a NUL byte in the first 8 KB and passed through untouched.

### Safety

- A snapshot of the current config goes to `~/.claude/backups/pre-restore-<stamp>.zip` before anything is overwritten. Always, not optionally.
- `--dry-run` writes nothing and skips the snapshot.
- Every entry carries a CRC32, verified on read. A corrupt archive fails loudly.
- Credentials are refused by default, with a warning naming the file that was skipped.

## Building from source

```sh
cargo build --release
cargo test
```

63 tests. The pure logic is covered directly: path tokenization, settings merge across all four strategies, line-ending and binary handling, include/exclude classification, and archive round-tripping. Filesystem walking and symlink recreation are covered by a real backup-and-restore round trip into a `CLAUDE_SYNC_HOME` directory rather than by mocks, because that is what catches the failures mocks hide.

## Releasing

Two pipelines, deliberately separate. `ci.yml` runs on every push and pull request: `cargo fmt --check`, `cargo clippy -D warnings`, and the test suite. No binaries are built, so a commit stays cheap. `release.yml` runs only on a `v*` tag, or manually via workflow dispatch. It builds all six targets, generates `SHA256SUMS`, and opens a **draft** release. Nothing goes public until you read the draft on GitHub and publish it yourself.

```sh
git tag v1.0.0
git push origin v1.0.0
# then review the draft at github.com/ehsan18t/claude-code-sync/releases and publish
```

Because the release is a draft, the install one-liners above will not find the assets until you publish it. That is the intended order.

| Target | Asset |
|---|---|
| Windows x86_64 | `claude-code-sync-windows-x86_64.exe` |
| Windows arm64 | `claude-code-sync-windows-arm64.exe` |
| Linux x86_64 | `claude-code-sync-linux-x86_64` |
| Linux arm64 | `claude-code-sync-linux-arm64` |
| macOS arm64 | `claude-code-sync-macos-arm64` |
| macOS x86_64 | `claude-code-sync-macos-x86_64` |

## License

MIT
