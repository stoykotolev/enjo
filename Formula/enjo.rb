# Template for the Homebrew formula. The release workflow substitutes the
# @PLACEHOLDERS@ and writes the result to Formula/enjo.rb on each tagged release.
# Do not edit Formula/enjo.rb by hand — edit this template instead.
class Enjo < Formula
  desc "Local-first TUI task manager"
  homepage "https://github.com/stoykotolev/enjo"
  version "0.2.0"

  # Apple Silicon only for now. Add an `on_intel` block (and the x86_64 build
  # back to the release workflow) if an Intel Mac ever needs it.
  on_macos do
    on_arm do
      url "https://github.com/stoykotolev/enjo/releases/download/v0.2.0/enjo-aarch64-apple-darwin.tar.gz"
      sha256 "7f9d66ca4219d36bfb7e3bc5d9b16ad880179e5cba439edd904d0e658a0741f7"
    end
  end

  def install
    bin.install "enjo"
  end

  test do
    assert_match "enjo", shell_output("#{bin}/enjo --version")
  end
end
