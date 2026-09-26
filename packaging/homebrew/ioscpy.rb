class Ioscpy < Formula
  desc "Mirror and control a jailbroken iPhone from macOS over USB"
  homepage "https://github.com/dtrukr/ioscpy"
  url "https://github.com/dtrukr/ioscpy/archive/e72869fb31d0d88a590292f9992dfc905d3f01fd.tar.gz"
  version "0.1.5"
  sha256 "d91c8ee86621a3c8e49178571777ac5c7d2f9df247580a1b4e3e84ecedb1b31a"
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
