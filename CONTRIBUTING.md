# Contributing

Bug reports are the most useful thing you can send. This tool rewrites a config directory people depend on, so a report that names the exact archive path and the exact settings value that went wrong is worth more than a feature request.

## Before you open a pull request

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs exactly these three and nothing else, so a green local run is a green pipeline.

## Testing against your real config

Never point a work-in-progress build at your live `~/.claude`. Use the home override:

```sh
CLAUDE_SYNC_HOME=/tmp/fake claude-code-sync restore archive.zip
```

On Windows this is the only override that works, because the home directory comes from a Win32 known-folder lookup rather than from `USERPROFILE`.

## What a good change looks like

- **A behavior change comes with a test that fails without it.** The interesting bugs in this tool are all round-trip bugs, so prefer a test that tokenizes, resolves and compares over one that asserts on an intermediate string.
- **The allowlist stays an allowlist.** Anything unrecognised under `~/.claude` is dropped rather than swallowed. If a new file genuinely needs to travel, add it to `classify.rs` with a comment saying why it is authored rather than generated.
- **An archive is untrusted input.** Restore reads zips that may come from anywhere. Anything that widens what an entry may write needs a test in `app.rs` alongside the existing traversal cases.
- **Secrets stay refused.** `.credentials.json` is checked before every other rule and must remain impossible to include by accident.

## Style

Comments explain why, not what. The existing ones are the reference: they name the failure that motivated the code, because that is the part a reader cannot recover from the code itself.
