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

Download the binary for your platform from
[Releases](https://github.com/ehsan18t/claude-code-sync/releases). No runtime, no installer.

Or build it:

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
| `~/.claude/CLAUDE.md`, `settings.json` | Authored by you |
| `~/.claude/agents/ commands/ hooks/ skills/ tools/` | Authored by you |
| `~/.agents/skills/` and `.skill-lock.json` | The real skill sources, when `~/.claude/skills/` symlinks into them |
| `~/.claude/projects/*/memory/` | Only with `--with-memory`. Distilled over time, never regenerates |

| Left behind | Why |
|---|---|
| `projects/*.jsonl` transcripts | Regenerated, and large |
| `plugins/` | Reinstalls itself from `enabledPlugins` in `settings.json` |
| `security/ cache/ file-history/ shell-snapshots/ sessions/ debug/ telemetry/` | Derived state |
| `.credentials.json` | An auth token. Never in an archive that travels, unless you pass `--include-credentials` |
| Any `.zip` | Stops an archive being swept into the next one |

The include list is an allowlist, not a denylist. A cache directory added in a future Claude Code
release is excluded by default rather than silently swallowed into your backups.

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

57 tests. The pure logic is covered directly: path tokenization, settings merge across all four
strategies, line-ending and binary handling, include/exclude classification, and archive
round-tripping.

Filesystem walking and symlink recreation are covered by a real backup-and-restore round trip into
a `CLAUDE_SYNC_HOME` directory rather than by mocks, because that is what catches the failures
mocks hide.

## License

MIT
