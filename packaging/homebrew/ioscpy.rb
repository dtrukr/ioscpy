class Ioscpy < Formula
  desc "Mirror and control a jailbroken iPhone from macOS over USB or SSH"
  homepage "https://github.com/dtrukr/ioscpy"
  url "https://github.com/dtrukr/ioscpy/archive/63a1bc041eee776a1d20ff2ef91669efe360100a.tar.gz"
  version "0.1.6"
  sha256 "520f75dfb7bb3682a4afb2104bf566ea47a794f38fe08a6d0da2be216970b7d7"
  license "MIT"
  head "https://github.com/dtrukr/ioscpy.git", branch: "main"

  # Builds with the Rust toolchain; the libimobiledevice tools (iproxy,
  # idevice_id, ideviceinfo) are needed at runtime for the USB transport.
  depends_on "rust" => :build
  depends_on "libimobiledevice"
  depends_on :macos

  def install
    # The host crate lives in host/; everything else in the repo is the device
    # package and docs.
    cd "host" do
      system "cargo", "install", *std_cargo_args(path: ".")
    end
  end

  test do
    assert_match "ioscpy", shell_output("#{bin}/ioscpy --version")
  end
end
