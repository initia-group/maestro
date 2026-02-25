# Template — the live formula is auto-generated in initia-group/homebrew-tap
# by the release workflow. Edit .github/workflows/release.yml to change the formula.
class Maestro < Formula
  desc "TUI agent dashboard for Claude Code"
  homepage "https://github.com/initia-group/maestro"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/initia-group/maestro/releases/download/v#{version}/maestro-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/initia-group/maestro/releases/download/v#{version}/maestro-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/initia-group/maestro/releases/download/v#{version}/maestro-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/initia-group/maestro/releases/download/v#{version}/maestro-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER"
    end
  end

  def install
    bin.install "maestro"
  end

  test do
    assert_match "maestro", shell_output("#{bin}/maestro --version")
  end
end
