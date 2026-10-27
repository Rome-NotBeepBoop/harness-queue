class Hq < Formula
  desc "Terminal-based coding agent queue for Orca IDE, Herdr, and Ghostty"
  homepage "https://github.com/Rome-NotBeepBoop/harness-queue"
  url "https://github.com/Rome-NotBeepBoop/harness-queue/archive/refs/heads/main.tar.gz"
  version "0.1.0"
  license "MIT"
  head "https://github.com/Rome-NotBeepBoop/harness-queue.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "Harness Queue (hq)", shell_output("#{bin}/hq --help")
  end
end
