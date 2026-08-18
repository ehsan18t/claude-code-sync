# claude-code-sync

Portable backup, restore and cross-machine sync for a Claude Code setup. One zip, one static
binary, no runtime.

Your Claude Code setup accumulates: custom skills, subagents, slash commands, hooks, and a
`settings.json` that ties them together. None of it is in git, and copying `~/.claude` to another
machine does not work. This fixes that.

## Why copying the folder fails

Two reasons, both silent.

**Hardcoded absolute paths.** `settings.json` stores hook commands as full paths:

```json
"command": "\"C:/Program Files/nodejs/node.exe\" \"C:/Users/you/.claude/hooks/my-hook.mjs\""
```

Restore that on another machine, or the same machine under a different username, and the hook is
still listed, still enabled, and never runs. Nothing errors. You lose the behavior and have no
reason to suspect it.

**Symlinked skills.** If you install skills from a shared source, entries in `~/.claude/skills/`
are often symlinks into somewhere like `~/.agents/skills/`. A plain zip captures dangling
pointers, so restore leaves you a skills directory full of broken links.

This tool stores paths as `${HOME}` and `${NODE}` tokens and re-expands them against the target
machine, records symlinks separately and recreates them, and normalizes line endings so one
archive works on any OS.

## Install

One line, no runtime, no admin rights.

**Windows** (PowerShell):

```powershell
irm https://raw.githubusercontent.com/ehsan18t/claude-code-sync/main/install.ps1 | iex
```

**Linux and macOS**:

```sh
curl -fsSL https://raw.githubusercontent.com/ehsan18t/claude-code-sync/main/install.sh | sh
```

Then open a new terminal and run `claude-code-sync`.

Windows installs to `%LOCALAPPDATA%\Programs\claude-code-sync` and adds it to your user PATH.
Linux and macOS install to `/usr/local/bin`, falling back to `~/.local/bin` when that is not
writable. Set `BINDIR` to choose somewhere else.

### Uninstall

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/ehsan18t/claude-code-sync/main/install.ps1))) -Uninstall
```

```sh
curl -fsSL https://raw.githubusercontent.com/ehsan18t/claude-code-sync/main/install.sh | sh -s -- --uninstall
```

Both remove the binary and the PATH entry. Neither touches your `~/.claude` config or your
backups.

### Homebrew

Not available yet. `brew install claude-code-sync` needs the project in homebrew-core, which
requires 75 or more stars, forks or watchers. A personal tap works before then, and the formula
is ready at [packaging/homebrew/claude-code-sync.rb](packaging/homebrew/claude-code-sync.rb):
create a repo named `homebrew-tap`, copy the formula to `Formula/`, fill in the version and
checksums from a release's `SHA256SUMS`, and it becomes:

```sh
brew install ehsan18t/tap/claude-code-sync
```

Until then, the `curl` line above works on macOS.

### From source

```sh
cargo build --release
```

## Usage

```
claude-code-sync backup  [--with-memory] [--include-credentials] [--out DIR]
claude-code-sync restore <archive.zip> [--merge=STRATEGY] [--dry-run]
```

Both `--flag=value` and `--flag value` work.

### Back up

```sh
claude-code-sync backup
claude-code-sync backup --with-memory --out D:/sync
```

Writes `claude-sync-<host>-<YYYYMMDD-HHMMSS>.zip` to the current directory, or to `--out`.

### Restore

```sh
claude-code-sync restore claude-sync-desktop-20260819-034932.zip --dry-run
claude-code-sync restore claude-sync-desktop-20260819-034932.zip --merge=ask
```

**Run `--dry-run` first.** It prints every create, update and link without writing anything.

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

The rules above are an allowlist, so anything unrecognised is dropped rather than swallowed. That
keeps a future cache directory out of your backups, but it also means a file nobody anticipated
gets missed. The escape hatch is `~/.claude/.claude-sync-include`:

```
# one archive-relative path per line, trailing / for a whole subtree
claude/history.jsonl
claude/context-mode/
```

Paths are archive-relative: `claude/…` is `~/.claude`, `agents/…` is `~/.agents`. The list itself
is carried, so it travels with your setup. `.credentials.json` is checked before this list and
cannot be included through it.

To see exactly what a backup would carry, read `manifest.json` inside the zip. It lists every file
with its size and SHA-256.

## Path rewriting

Only the matched home or node prefix and the path immediately following it are rewritten.
Everything else in a string is copied through byte for byte, and a backslash counts as a
separator only when what follows could start a path segment.

That last rule matters more than it sounds. A permission rule like:

```
Bash(cp "C:/Users/you/app/src/\(payload\)/index.tsx")
```

has backslashes that are shell escapes, not separators. Converting them produces a different
string that no longer dedupes against itself, so every restore appends another near-duplicate
entry to `permissions.allow`. Tokenizing is idempotent here, and there is a test that proves it.

## Line endings

The archive always stores LF. On restore, files are rewritten to the target's native ending: CRLF
on Windows, LF elsewhere. Binary files are detected by a NUL byte in the first 8 KB and passed
through untouched.

## settings.json merge strategies

Only `settings.json` is merged. Every other file is written as-is.

| Strategy | Behavior |
|---|---|
| `incoming` | Deep merge, the archive wins a conflict. **Default** |
| `existing` | Deep merge, this machine wins a conflict |
| `replace` | Discard this machine's settings entirely |
| `ask` | Prompt for each conflicting key |

Under every strategy except `replace`, objects merge key by key and arrays are unioned with local
order preserved. Only a differing scalar, or a type mismatch, counts as a conflict, so a
permission you added on one machine is not lost by syncing from the other.

Key order is preserved: your file keeps its shape, with new keys appended rather than the whole
thing reshuffled.

## Safety

- A snapshot of the current config goes to `~/.claude/backups/pre-restore-<stamp>.zip` before
  anything is overwritten. Always, not optionally.
- `--dry-run` writes nothing and skips the snapshot.
- Every entry carries a CRC32, verified on read. A corrupt archive fails loudly.
- The manifest records tool version, source OS, hostname, timestamp, and a SHA-256 per file.
- Credentials are refused by default, with a warning naming the file that was skipped.

### Testing against a throwaway home

Set `CLAUDE_SYNC_HOME` to point the tool at a different directory:

```sh
CLAUDE_SYNC_HOME=/tmp/fake claude-code-sync restore archive.zip
```

On Windows the home directory comes from a Win32 known-folder lookup, **not** from `USERPROFILE`,
so setting that variable does not redirect anything. Use `CLAUDE_SYNC_HOME`.

## Tests

```sh
cargo test
```

63 tests. The pure logic is covered directly: path tokenization, settings merge across all four
strategies, line-ending and binary handling, include/exclude classification, and archive
round-tripping.

Filesystem walking and symlink recreation are covered by a real backup-and-restore round trip into
a `CLAUDE_SYNC_HOME` directory rather than by mocks, because that is what catches the failures
mocks hide.

## Releasing

Two pipelines, deliberately separate.

**`ci.yml`** runs on every push and pull request: `cargo fmt --check`, `cargo clippy -D warnings`,
and the test suite. No binaries are built, so a commit stays cheap.

**`release.yml`** runs only on a `v*` tag, or manually via workflow dispatch. It builds all six
targets, generates `SHA256SUMS`, and opens a **draft** release. Nothing goes public until you read
the draft on GitHub and publish it yourself.

```sh
git tag v1.0.0
git push origin v1.0.0
# then review the draft at github.com/ehsan18t/claude-code-sync/releases and publish
```

Because the release is a draft, the install one-liners above will not find the assets until you
publish it. That is the intended order.

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
