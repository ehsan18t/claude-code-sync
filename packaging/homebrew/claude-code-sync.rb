# Homebrew formula for claude-code-sync.
#
# This file does not belong in this repository at runtime. It belongs in a tap:
#
#   1. Create a repo named exactly  ehsan18t/homebrew-tap
#   2. Copy this file to           Formula/claude-code-sync.rb
#   3. Fill in the version and both sha256 values from the published release's SHA256SUMS
#
# Users then install with:
#
#   brew install ehsan18t/tap/claude-code-sync
#
# homebrew-core (plain `brew install claude-code-sync`) is not an option yet: it requires the
# project to be notable, currently 75 or more stars, forks or watchers.

class ClaudeCodeSync < Formula
  desc "Portable backup, restore and cross-machine sync for a Claude Code setup"
  homepage "https://github.com/ehsan18t/claude-code-sync"
  version "1.0.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/ehsan18t/claude-code-sync/releases/download/v#{version}/claude-code-sync-macos-arm64"
      sha256 "REPLACE_WITH_SHA256_OF_MACOS_ARM64"
    end
    on_intel do
      url "https://github.com/ehsan18t/claude-code-sync/releases/download/v#{version}/claude-code-sync-macos-x86_64"
      sha256 "REPLACE_WITH_SHA256_OF_MACOS_X86_64"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/ehsan18t/claude-code-sync/releases/download/v#{version}/claude-code-sync-linux-arm64"
      sha256 "REPLACE_WITH_SHA256_OF_LINUX_ARM64"
    end
    on_intel do
      url "https://github.com/ehsan18t/claude-code-sync/releases/download/v#{version}/claude-code-sync-linux-x86_64"
      sha256 "REPLACE_WITH_SHA256_OF_LINUX_X86_64"
    end
  end

  def install
    bin.install Dir["claude-code-sync-*"].first => "claude-code-sync"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/claude-code-sync --version")
  end
end
