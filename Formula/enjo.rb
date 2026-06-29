# Template for the Homebrew formula. The release workflow substitutes the
# @PLACEHOLDERS@ and writes the result to Formula/enjo.rb on each tagged release.
# Do not edit Formula/enjo.rb by hand — edit this template instead.
class Enjo < Formula
  desc "Local-first TUI task manager"
  homepage "https://github.com/stoykotolev/enjo"
  version "0.1.1"

  # Apple Silicon only for now. Add an `on_intel` block (and the x86_64 build
  # back to the release workflow) if an Intel Mac ever needs it.
  on_macos do
    on_arm do
      url "https://github.com/stoykotolev/enjo/releases/download/v0.1.1/enjo-aarch64-apple-darwin.tar.gz"
      sha256 "6d020236db3f11734145eda2f7703b995f2b8048cdb6ad7ab6479948d51137fd"
    end
  end

  def install
    bin.install "enjo"
  end

  test do
    assert_match "enjo", shell_output("#{bin}/enjo --version")
  end
end
