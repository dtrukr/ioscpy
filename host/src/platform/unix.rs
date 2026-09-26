//! macOS + Linux implementation of the platform seam. This reproduces the
//! behavior the host had before the seam existed: command-line tools, the per-OS cache dir (macOS `~/Library/Caches/ioscpy`,
//! Linux `$XDG_CACHE_HOME`/`~/.cache`), `sw_vers`/`uname`, and the existing
//! install hints.

use std::path::PathBuf;
use std::process::Command;

/// Prefer the installed libimobiledevice tools on macOS. Other tools named
/// `idevice_id` can appear earlier on PATH and may have different behavior.
/// Fall back to PATH for non-Homebrew setups and on Linux.
pub fn tool_path(name: &str) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    if matches!(name, "idevice_id" | "ideviceinfo" | "iproxy") {
        for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
            let path = PathBuf::from(dir).join(name);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    Some(PathBuf::from(name))
}

pub fn cache_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")?;
        let mut p = PathBuf::from(home);
        p.push("Library/Caches/ioscpy");
        let _ = std::fs::create_dir_all(&p);
        Some(p)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut p = if let Some(x) = std::env::var_os("XDG_CACHE_HOME") {
            PathBuf::from(x)
        } else {
            let home = std::env::var_os("HOME")?;
            let mut h = PathBuf::from(home);
            h.push(".cache");
            h
        };
        p.push("ioscpy");
        let _ = std::fs::create_dir_all(&p);
        Some(p)
    }
}

pub fn os_version() -> String {
    #[cfg(target_os = "macos")]
    let (cmd, args): (&str, &[&str]) = ("sw_vers", &["-productVersion"]);
    #[cfg(not(target_os = "macos"))]
    let (cmd, args): (&str, &[&str]) = ("uname", &["-sr"]);

    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn missing_tools_hint() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "The USB tools are missing. Install them with:  brew install libimobiledevice"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "The USB tools are missing. Install them with:  sudo apt install libimobiledevice-utils usbmuxd"
    }
}
