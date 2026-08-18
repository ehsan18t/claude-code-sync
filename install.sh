#!/bin/sh
# Install or remove claude-code-sync on Linux and macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/ehsan18t/claude-code-sync/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/ehsan18t/claude-code-sync/main/install.sh | sh -s -- --uninstall
#
# Installs to /usr/local/bin when that is writable or sudo is available, otherwise to
# ~/.local/bin. Set BINDIR to choose explicitly.

set -eu

REPO="ehsan18t/claude-code-sync"
NAME="claude-code-sync"

say() { printf '%s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

need() {
  command -v "$1" >/dev/null 2>&1 || die "$1 is required but not installed"
}

resolve_bindir() {
  if [ -n "${BINDIR:-}" ]; then
    printf '%s' "$BINDIR"
  elif [ -w /usr/local/bin ] 2>/dev/null; then
    printf '/usr/local/bin'
  elif command -v sudo >/dev/null 2>&1 && [ -d /usr/local/bin ]; then
    printf '/usr/local/bin'
  else
    printf '%s/.local/bin' "$HOME"
  fi
}

# Write to a directory that may need elevation, without demanding it when it does not.
place() {
  from="$1"
  to="$2"
  dir=$(dirname "$to")
  if [ -w "$dir" ] 2>/dev/null; then
    mv "$from" "$to"
    chmod 755 "$to"
  else
    say "Elevated permission needed to write $to"
    sudo mv "$from" "$to"
    sudo chmod 755 "$to"
  fi
}

remove() {
  bindir=$(resolve_bindir)
  target="$bindir/$NAME"
  [ -e "$target" ] || die "$NAME is not installed at $target"
  if [ -w "$bindir" ] 2>/dev/null; then
    rm -f "$target"
  else
    sudo rm -f "$target"
  fi
  say "Removed $target"
  say ""
  say "Your backups and config were not touched. To remove those as well:"
  say "  rm -rf ~/.claude/backups"
  exit 0
}

case "${1:-}" in
  --uninstall|uninstall) remove ;;
esac

need uname
if command -v curl >/dev/null 2>&1; then
  fetch='curl -fsSL -o'
elif command -v wget >/dev/null 2>&1; then
  fetch='wget -qO'
else
  die "curl or wget is required"
fi

case "$(uname -s)" in
  Linux)  os=linux ;;
  Darwin) os=macos ;;
  *)      die "unsupported operating system: $(uname -s). Windows users: see install.ps1" ;;
esac

case "$(uname -m)" in
  x86_64|amd64)  arch=x86_64 ;;
  arm64|aarch64) arch=arm64 ;;
  *)             die "unsupported architecture: $(uname -m)" ;;
esac

# The macOS x86_64 asset keeps the x86_64 name; only Linux/macOS arm uses arm64.
asset="$NAME-$os-$arch"
url="https://github.com/$REPO/releases/latest/download/$asset"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

say "Downloading $asset"
$fetch "$tmp/$NAME" "$url" || die "download failed. Is there a published release with $asset?"
[ -s "$tmp/$NAME" ] || die "downloaded file is empty"
chmod +x "$tmp/$NAME"

bindir=$(resolve_bindir)
mkdir -p "$bindir" 2>/dev/null || true
place "$tmp/$NAME" "$bindir/$NAME"

say "Installed to $bindir/$NAME"

case ":$PATH:" in
  *":$bindir:"*) ;;
  *)
    say ""
    say "$bindir is not on your PATH. Add this to your shell profile:"
    say "  export PATH=\"$bindir:\$PATH\""
    ;;
esac

say ""
"$bindir/$NAME" --version
say "Run '$NAME' with no arguments for usage."
