//! SSH transport for an iPhone plugged into another Mac. The remote copy of
//! ioscpy owns usbmux; the local copy owns the window and all input handling.
//! SSH carries the existing device protocol without exposing a TCP port.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};

use anyhow::{anyhow, bail, Context, Result};

use crate::{device, protocol, usbmux};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DeviceList {
    pub protocol: u16,
    pub devices: Vec<device::Device>,
}

#[allow(dead_code)]
pub enum ConnectionGuard {
    Usb(usbmux::UsbForward),
    Remote(RemoteForward),
}

/// Limit values SSH will interpolate into the remote shell command. SSH config
/// aliases and `user@host` work; options and shell metacharacters do not.
fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && !host.starts_with('-')
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-@".contains(&b))
}

fn valid_udid(udid: &str) -> bool {
    !udid.is_empty() && udid.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

fn ssh_command(host: &str) -> Result<Command> {
    if !valid_host(host) {
        bail!("invalid SSH host {host:?}; use an SSH alias or user@host");
    }
    let mut cmd = Command::new("ssh");
    cmd.args([
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=8",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
        "--",
    ]);
    cmd.arg(host);
    Ok(cmd)
}

pub fn list_devices(host: &str) -> Result<Vec<device::Device>> {
    let mut cmd = ssh_command(host)?;
    let output = cmd
        .args(["ioscpy", "--list-json"])
        .output()
        .with_context(|| format!("start SSH device discovery on {host}"))?;
    if !output.status.success() {
        bail!(
            "remote device discovery on {host} failed: {}. Install the forked ioscpy host there and check SSH access",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let list: DeviceList = serde_json::from_slice(&output.stdout).with_context(|| {
        format!("invalid device list from {host}; install the forked ioscpy host there")
    })?;
    if list.protocol != protocol::PROTOCOL_VERSION {
        bail!(
            "ioscpy protocol mismatch: this Mac uses {}, {host} uses {}",
            protocol::PROTOCOL_VERSION,
            list.protocol
        );
    }
    Ok(list.devices)
}

pub struct RemoteForward {
    child: Child,
    bridge: TcpStream,
    pumps: Vec<JoinHandle<()>>,
}

impl RemoteForward {
    pub fn start(host: &str, udid: &str, device_port: u16) -> Result<(Self, TcpStream)> {
        if !valid_udid(udid) {
            bail!("invalid device UDID {udid:?}");
        }

        // Existing session code uses TcpStream::try_clone and socket timeouts.
        // A private loopback pair lets all of those paths use the SSH relay.
        let listener =
            TcpListener::bind(("127.0.0.1", 0)).context("reserve local SSH relay socket")?;
        let frontend =
            TcpStream::connect(listener.local_addr()?).context("connect local SSH relay socket")?;
        frontend.set_nodelay(true).ok();
        let (bridge, _) = listener.accept().context("accept local SSH relay socket")?;
        bridge.set_nodelay(true).ok();

        let mut cmd = ssh_command(host)?;
        let mut child = cmd
            .args([
                "ioscpy",
                "--relay-stdio",
                "--device",
                udid,
                "--port",
                &device_port.to_string(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("start ioscpy relay over SSH to {host}"))?;

        let mut ssh_stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("SSH stdin unavailable"))?;
        let mut ssh_stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("SSH stdout unavailable"))?;
        let mut up = bridge.try_clone()?;
        let mut down = bridge.try_clone()?;
        let pumps = vec![
            thread::spawn(move || {
                let _ = io::copy(&mut up, &mut ssh_stdin);
                drop(ssh_stdin);
            }),
            thread::spawn(move || {
                let _ = io::copy(&mut ssh_stdout, &mut down);
                let _ = down.shutdown(Shutdown::Write);
            }),
        ];

        Ok((
            Self {
                child,
                bridge,
                pumps,
            },
            frontend,
        ))
    }
}

impl Drop for RemoteForward {
    fn drop(&mut self) {
        let _ = self.bridge.shutdown(Shutdown::Both);
        let _ = self.child.kill();
        let _ = self.child.wait();
        for pump in self.pumps.drain(..) {
            let _ = pump.join();
        }
    }
}

/// Run on the USB Mac. stdout carries only protocol bytes; errors go to stderr.
pub fn relay_stdio(udid: &str, device_port: u16) -> Result<()> {
    if !valid_udid(udid) {
        bail!("invalid device UDID {udid:?}");
    }
    let forward =
        usbmux::UsbForward::start(udid, device_port).context("remote USB connection failed")?;
    let mut device_stream = forward.connect()?;
    let mut device_writer = device_stream.try_clone()?;
    let writer = thread::spawn(move || {
        let _ = io::copy(&mut io::stdin().lock(), &mut device_writer);
        let _ = device_writer.shutdown(Shutdown::Write);
    });
    // Rust stdout is line buffered. Flush each binary chunk so a small HELLO
    // acknowledgment does not wait indefinitely for a newline or full buffer.
    let mut stdout = io::stdout().lock();
    let mut chunk = [0u8; 64 * 1024];
    let result = (|| -> io::Result<()> {
        loop {
            let n = device_stream.read(&mut chunk)?;
            if n == 0 {
                return Ok(());
            }
            stdout.write_all(&chunk[..n])?;
            stdout.flush()?;
        }
    })();
    // SSH closes stdin on disconnect; avoid waiting for a blocked input thread.
    drop(writer);
    result.context("remote ioscpy relay failed")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_arguments_reject_shell_injection() {
        assert!(valid_host("mac-studio1"));
        assert!(valid_host("dennis@mac-studio1"));
        assert!(!valid_host("-oProxyCommand=evil"));
        assert!(!valid_host("studio;evil"));
        assert!(valid_udid("00008030-000879013C53402E"));
        assert!(!valid_udid("abc;evil"));
    }
}
