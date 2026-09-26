# ioscpy

This is [Dennis's public fork](https://github.com/dtrukr/ioscpy) of
[lautarovculic/ioscpy](https://github.com/lautarovculic/ioscpy). It adds the
Ghostty bridge, accessibility prototype, and tested iOS 18.7.1 support. Its
host and phone package use protocol 5 and must be installed together.

## ToDo
- [ ] https://github.com/lautarovculic/ioscpy/issues/7
- [ ] Screen Recording (showing status) button in sidepanel.
- [ ] Research: Providing support for Windows with a GUI as well, installer (Reserved, Author will implement this fully)
- [ ] Add Authentication & Authorization, also a notifiation in device that `ioscpy` is running (Reserved, Author will implement this fully)
- [ ] Make `ioscpy` wireless (BLE, Wi-Fi) (Reserved, Author will implement this fully)
- [ ] Custom Buttons Actions (Macros) (In phases, concept coming soon)

A macOS, Linux, and Windows CLI that mirrors and controls a jailbroken iPhone over USB.

With one device attached, that is all you need. It connects on its own.

<p align="center">
  <img src="docs/assets/ioscpy-demo.gif" width="720" alt="ioscpy demo">
</p>

## Install

There are two sides. Build and install both from this fork. The upstream
Homebrew tap and Sileo repository currently ship protocol 4 and cannot be mixed
with this fork's protocol 5.

On macOS, install Rust, libimobiledevice, Theos, and the iPhoneOS SDK. Then:

```bash
git clone https://github.com/dtrukr/ioscpy.git
cd ioscpy
brew install libimobiledevice
make host-release
make install-host PREFIX="$(brew --prefix)"
ioscpy --version
```

Build the matching device package on the Mac:

```bash
make device-rootless THEOS="$HOME/theos"  # Dopamine / palera1n rootless
# or: make device-rootful THEOS="$HOME/theos"  # palera1n rootful
```

Install the generated `device/packages/*.deb` with `dpkg -i` on the jailbroken
phone, then respring. See [device packaging](device/packaging/README.md) for the
package variants and [deployment scripts](scripts/install-device.sh) for a USB
SSH workflow.

```bash
ioscpy --device <UDID>   # pick a device when several are attached
ioscpy --list            # list attached devices
ioscpy --no-keyboard     # hide the on-screen keyboard, type from the Mac
ioscpy --mjpeg           # force MJPEG video instead of the default H.264
ioscpy --debug           # full diagnostics
ioscpy --version
```

## Linux

There is no prebuilt for Linux yet. Build the host from source and use the
matching phone package built from this fork.

Clone this repo, then run the installer:

```bash
./install.sh
```

`--yes` skips the confirmation prompts. `--prefix DIR` installs to `DIR` instead of `/usr/local` and omits `sudo` when the directory is under `$HOME`. `--uninstall` removes the installed binary.

### Manual install

Prereqs on Debian/Ubuntu:

```bash
sudo apt install build-essential pkg-config nasm \
                 libimobiledevice-utils usbmuxd \
                 libxkbcommon-dev libwayland-dev \
                 libxcb1-dev libxkbcommon-x11-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Make sure `usbmuxd` is running so the device shows up:

```bash
sudo systemctl enable --now usbmuxd
idevice_id -l   # should print the UDID with the phone attached
```

Build the host as your user, then install:

```bash
make host-release
sudo make install-host    # installs to /usr/local/bin/ioscpy
ioscpy --version
```

## For Arch Linux

`./install.sh` also works on Arch Linux.

```bash
sudo pacman -S --needed \
  base-devel pkgconf nasm \
  libimobiledevice usbmuxd \
  libxkbcommon wayland libxcb libxkbcommon-x11 \
  rustup curl git
```

```bash
rustup default stable
```

Enable the service

```bash
sudo systemctl enable --now usbmuxd.service
```

Check if device is connected

```bash
idevice_id -l
```

Then run the binary

```bash
cd host/target/release && chmod +x ioscpy
```

```bash
./ioscpy
```

The build step stays as your user so a rustup-installed `cargo` is found.
`install-host` is a separate target that just copies the built binary, so it
runs fine under `sudo` without needing `cargo` on root's PATH. For a no-sudo
install in your home, use `make install-host PREFIX=$HOME/.local` and make
sure `$HOME/.local/bin` is on your `PATH`.

H.264 decoding works on Linux through the `openh264` software decoder. Pass
`--mjpeg` if you prefer the MJPEG path.

On Wayland compositors that refuse server-side decorations (GNOME/mutter),
ioscpy automatically falls back to X11/XWayland so the window gets a normal
titlebar. Pass `--wayland` to stay on native Wayland instead.

## Windows

There is no prebuilt for Windows yet. Build the host from source and use the
matching phone package built from this fork.

ioscpy on Windows reuses the USB tooling an iOS setup already has, and does **not** replace any driver, so Frida, objection, grapefruit, ideviceinfo, iTunes, and any other usbmux tool keep working while it runs.

### 1. USB stack

- **Apple Mobile Device Support**: installed with iTunes from apple.com (get the
  Apple-website build, not the Microsoft Store one), or the standalone AMDS
  package. This provides the USB driver and usbmuxd. iTunes must see the phone.
- **libimobiledevice-win32**: provides `iproxy.exe`, `idevice_id.exe`,
  `ideviceinfo.exe`. Install with [Scoop](https://scoop.sh):

      scoop install libimobiledevice

  or with MSYS2:

      pacman -S mingw-w64-x86_64-libimobiledevice

  Make sure the three `.exe` files are on your `PATH` (or drop them next to
  `ioscpy.exe`). Check with `idevice_id -l`, which should print your device UDID.

### 2. Build toolchain

- **Rust** with the MSVC toolchain: install from [rustup.rs](https://rustup.rs).
- **Visual Studio Build Tools** with the C++ workload (the `openh264` decoder
  compiles a small C library through `cc`).
- **nasm** on `PATH`: `openh264`'s assembler (`scoop install nasm`).

### 3. Build and run

    git clone https://github.com/dtrukr/ioscpy
    cd ioscpy
    cargo build --release --manifest-path host/Cargo.toml
    .\host\target\release\ioscpy.exe --list      # confirms the device is seen
    .\host\target\release\ioscpy.exe             # opens the mirror window

Pass `--mjpeg` if you prefer the MJPEG path over H.264. If `openh264` fails to
build, confirm `nasm` is on `PATH`.

## Automation and Ghostty bridge

Automation callers can send one-shot input without opening the mirror window:

```bash
ioscpy --tap 0.50,0.75
ioscpy --swipe 0.50,0.80,0.50,0.20 --duration-ms 450
ioscpy --text "hello"
ioscpy --key enter
ioscpy --keyboard-mode hide
ioscpy --clipboard-set "hello" --paste
ioscpy --accessibility-tree
ioscpy --accessibility-action '{"schema":"ioscpy.accessibility.action.v1","action":"tap","frame":{"x":80,"y":412,"width":240,"height":44}}'
```

Touch coordinates are normalized screen coordinates in `[0,1]`, independent of
the device resolution. `--key` accepts `enter`, `backspace`, `tab`, `escape`,
`left`, `right`, `up`, `down`, `select-all`, `copy`, `paste`, `cut`, and `undo`.
`--accessibility-tree` prints the current prototype accessibility tree JSON and
requires a matching phone-side package that advertises accessibility support.
`--accessibility-action` accepts `protocol/accessibility-action.schema.json`
request JSON; the prototype supports `tap` when a `point`, `frame`, or
`node.frame` is supplied.

Before enabling selector-based automation in a host integration, validate the
prototype against a real jailbroken device:

```bash
scripts/validate-accessibility-prototype.sh --tap-first --out-dir /tmp/ioscpy-ax-validation
```

For Ghostty MCP, the combined handoff command is:

```bash
scripts/validate-accessibility-for-ghostty.sh --device <UDID> --out-dir /tmp/ioscpy-ax-validation
```

The script writes schema-checked `validation-summary.json` only after the tree
dump and optional tap action both pass. Prototype runs record
`selector_ready: false`, so they prove the transport without unlocking committed
selector semantics. Promoting a run with `--selector-ready` also requires
`--selector-proof FILE`; the proof must be
`protocol/accessibility-selector-proof.schema.json` JSON covering `find`,
selector tap, `form_input`, and `get_accessibility_tree` against the same tree
snapshot. It is copied into the artifact directory and hashed in the summary.
Selector-ready summaries also record a concrete device UDID so host integrations
do not unlock selectors for a different iPhone. Ghostty MCP can inspect that file via
`GHOSTTY_IPHONE_ACCESSIBILITY_VALIDATION_SUMMARY=/tmp/ioscpy-ax-validation/validation-summary.json`.
From `webview-poc/mcp-server`, run
`npm run check:iphone-accessibility-validation -- --summary /tmp/ioscpy-ax-validation/validation-summary.json --device <UDID>`
to confirm Ghostty will enable selector APIs for that target.
Use `--selector-proof-template FILE` after a successful `--tap-first` run to
draft the proof from the captured tree; the draft starts with `ok: false` cases
and must be filled with real device evidence before `--selector-ready` accepts it.
After the proof is filled, rerun the handoff with
`--selector-ready --selector-proof FILE`.
Run `scripts/test-accessibility-validation.js` to exercise the summary gate
locally without a phone.

## First touch

> After a respring or a fresh connection, give the phone one physical tap on its
> screen before driving it from the host. iOS only trusts touch events that come
> from the real digitizer, so that first real tap is what lets the injected ones
> through. You do it once, then the host takes over.

## Controls

- Mouse: click to tap, click and drag to swipe.
- Typing: keystrokes go to the focused field, in any Mac keyboard layout. Accents
  and emoji go through the clipboard.
- Esc: go back.
- Cmd+J, Cmd+L, Cmd+T, Cmd+R: Home, Lock, App Switcher, Rotate.
- Cmd+A, Cmd+C, Cmd+V, Cmd+X, Cmd+Z: Select All, Copy, Paste, Cut, Undo. The
  clipboard syncs both ways, so Cmd+C on the phone reaches the Mac.
- Enter, Backspace, Tab, arrows: the matching editing keys.

On Linux and Windows, use **Ctrl** in place of Cmd for these shortcuts (Ctrl+C,
Ctrl+V, and so on); the clipboard still syncs both ways.

Rotating the phone rotates and resizes the mirror.

## Tested on

Hosts:
- Debian, KDE/Wayland
- Ubuntu 26.04, GNOME/Wayland
- Arch Linux, BSPWM
- Windows 11 x64

Devices:

| layout | device | iOS | injection |
| --- | --- | --- | --- |
| roothide | iPhone11,2 | 15.5 | ElleKit |
| roothide | iPhone10,4 | 16.7.12 | ElleKit |
| roothide | iPhone10,3 | 16.7.10 | ElleKit |
| roothide | iPhone14 | 16.3 | ------- |
| rootless | iPhone13 | 15.5 | ElleKit |
| rootless (Dopamine) | iPhone12,8 | 18.7.1 | ElleKit |

ioscpy is developed and tested on the devices above. I don't have a rootful device
or every iOS version on hand, so coverage by layout is incomplete:

| layout   | status                                          |
|----------|-------------------------------------------------|
| rootless | builds; runs on the roothide unit via `/var/jb` |
| roothide | working (rootless `.deb` via `/var/jb`)         |
| rootful  | builds and layout validated; runtime not tested |

If you run ioscpy on a different iPhone, iOS version, or jailbreak, please help fill
this in. Rootful and other iOS versions especially need testing.

- If it works, add a row with your device, iOS version, layout, and injection
  framework, and open a pull request.
- If it fails, open an issue with enough detail to fix it:
  - iPhone model and iOS version
  - jailbreak and layout (Dopamine, palera1n rootless or rootful, roothide)
  - injection framework (ElleKit, Substitute, Substrate)
  - the output of `ioscpy --debug`
  - what broke (screen, touch, keyboard, clipboard, install, and so on)

## Layout

```text
host/       Rust host CLI (macOS, Linux, and Windows)
device/     iOS package (Theos): daemon, ctl, tweak, jbcompat, packaging
protocol/   wire format docs, kept in lockstep with the code
scripts/    device deploy, respring, log, and diagnostics helpers
docs/       architecture, jailbreak compatibility, troubleshooting, release
```

## Building

```bash
make host-release        # macOS host binary
make device-rootless     # rootless .deb (Dopamine, palera1n-rootless, /var/jb)
make device-rootful      # rootful .deb (palera1n-rootful, /)
make release             # host plus both device variants
```

Requires Rust, Theos (`$THEOS`), libimobiledevice (`idevice_id`, `iproxy`), `ldid`,
and `dpkg-deb`.

## Scope

ioscpy is for controlling your own jailbroken iPhone from your Mac and Linux (Debian, Ubuntu and Arch Linux) device, over the USB cable, on the same desk.

For Linux hosts, same external requirement as macOS: libimobiledevice tools (`iproxy`,
  `idevice_id`, `ideviceinfo`) and `usbmuxd`.

It now runs on Windows too, reusing the standard iOS USB tools without replacing
any driver. It is still **ONLY** jailbroken iPhones.
