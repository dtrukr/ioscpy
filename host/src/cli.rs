//! Command-line flags. The normal case is just `ioscpy` with no flags, the rest
//! is for support and debugging.

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "ioscpy",
    version,
    about = "Mirror and control a jailbroken iPhone from macOS over USB",
    long_about = "ioscpy mirrors and controls a jailbroken iPhone from macOS over USB.\n\
                  Run with no arguments to auto-connect the single attached device.\n\
                  All core features (screen, mouse, keyboard, clipboard, shortcuts,\n\
                  orientation, reconnect) are enabled by default."
)]
pub struct Cli {
    /// Select a specific device by UDID (required when multiple are attached).
    #[arg(long, value_name = "UDID")]
    pub device: Option<String>,

    /// List attached compatible devices and exit.
    #[arg(long)]
    pub list: bool,

    /// Print full diagnostics (host/device versions, transport, backends).
    #[arg(long)]
    pub debug: bool,

    /// Force MJPEG instead of H.264, in case H.264 acts up on some device.
    #[arg(long)]
    pub mjpeg: bool,

    /// Stay on native Wayland even when the compositor draws no window
    /// decorations for us (GNOME/mutter). By default ioscpy falls back to
    /// X11/XWayland there so the window gets a titlebar.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[arg(long)]
    pub wayland: bool,

    /// Hide the on-screen iOS keyboard while connected, so the mirror shows the
    /// full screen (you type from the Mac; the device acts as if a hardware
    /// keyboard is attached). The keyboard returns when ioscpy exits. iOS 16+.
    #[arg(long)]
    pub no_keyboard: bool,

    /// Tap normalized screen coordinates, formatted as X,Y in [0,1].
    #[arg(long, value_name = "X,Y")]
    pub tap: Option<String>,

    /// Swipe normalized screen coordinates, formatted as X1,Y1,X2,Y2 in [0,1].
    #[arg(long, value_name = "X1,Y1,X2,Y2")]
    pub swipe: Option<String>,

    /// Duration for --swipe in milliseconds.
    #[arg(long, value_name = "MS", default_value_t = 350)]
    pub duration_ms: u64,

    /// Type literal UTF-8 text into the focused iOS field.
    #[arg(long, value_name = "TEXT")]
    pub text: Option<String>,

    /// Send one non-text key/editing action: enter, backspace, tab, escape,
    /// left, right, up, down, select-all, copy, paste, cut, undo.
    #[arg(long, value_name = "KEY")]
    pub key: Option<String>,

    /// Set the iOS clipboard to literal UTF-8 text.
    #[arg(long, value_name = "TEXT")]
    pub clipboard_set: Option<String>,

    /// Paste after --clipboard-set.
    #[arg(long)]
    pub paste: bool,

    /// Hide or restore the iOS software keyboard: hide, show.
    #[arg(long, value_name = "hide|show")]
    pub keyboard_mode: Option<String>,

    /// Print the current accessibility tree as JSON and exit.
    #[arg(long)]
    pub accessibility_tree: bool,

    /// Send one accessibility action JSON request and print the result.
    #[arg(long, value_name = "JSON")]
    pub accessibility_action: Option<String>,

    // hidden options for debugging, not part of normal use
    /// Connect directly to host:port, bypassing usbmux/iproxy (debugging only).
    #[arg(long, value_name = "ADDR", hide = true)]
    pub addr: Option<String>,

    /// Override the daemon port (default 27183).
    #[arg(long, value_name = "PORT", hide = true)]
    pub port: Option<u16>,

    /// Connect, handshake, print the capability map, then exit (no UI).
    #[arg(long, hide = true)]
    pub handshake_only: bool,

    /// Save the first streamed frame (JPEG) to this path and exit. For testing
    /// the capture/stream path without opening a window.
    #[arg(long, value_name = "PATH", hide = true)]
    pub snapshot: Option<String>,

    /// Stream for N seconds with no window and report fps / bandwidth / decode
    /// time. For measuring stream performance.
    #[arg(long, value_name = "SECONDS", hide = true)]
    pub bench: Option<u64>,

    /// Send one SYSTEM_ACTION code (1=Home 2=Lock 3=Wake 4=AppSwitcher) and report
    /// whether the stream survives it. For testing system actions headlessly.
    #[arg(long, value_name = "CODE", hide = true)]
    pub action: Option<u16>,

    /// Run the full streaming session (no window) for N seconds, surfacing any
    /// reconnects. For reproducing session-loop instability headlessly.
    #[arg(long, value_name = "SECONDS", hide = true)]
    pub soak: Option<u64>,

    /// Stream MJPEG frames to stdout and accept JSON-line controls on stdin.
    /// Intended for embedding ioscpy in another native application.
    #[arg(long, hide = true)]
    pub stdio_bridge: bool,
}

impl Cli {
    pub fn parse_args() -> Self {
        Cli::parse()
    }

    pub fn has_one_shot_input(&self) -> bool {
        self.tap.is_some()
            || self.swipe.is_some()
            || self.text.is_some()
            || self.key.is_some()
            || self.clipboard_set.is_some()
            || self.paste
            || self.keyboard_mode.is_some()
            || self.accessibility_action.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_only_is_one_shot_input() {
        let cli = Cli::parse_from(["ioscpy", "--paste"]);
        assert!(cli.has_one_shot_input());
    }
}
