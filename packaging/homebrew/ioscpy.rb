class Ioscpy < Formula
  desc "Mirror and control a jailbroken iPhone from macOS over USB or SSH"
  homepage "https://github.com/dtrukr/ioscpy"
  url "https://github.com/dtrukr/ioscpy/archive/adc25a45c696c07b55d17b2170a532133ac02113.tar.gz"
  version "0.1.7"
  sha256 "1481968c41d18ee0e6e2a48d2af1cefbe035e91aad1cb9ad873edc1400de5e55"
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
