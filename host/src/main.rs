//! ioscpy: mirror and control a jailbroken iPhone from macOS over USB.
//!
//! With no arguments it picks the single attached device, sets up the USB link,
//! handshakes with the daemon, and opens the session. Flags only pick a device
//! or turn on diagnostics.

mod cli;
mod clipboard;
mod config;
mod device;
mod h264;
mod health;
mod input;
mod installer;
mod keyboard;
mod logging;
mod mouse;
mod platform;
mod protocol;
mod remote;
mod sidebar;
mod update;
mod usbmux;
mod video;
#[cfg(all(unix, not(target_os = "macos")))]
mod wayland_compat;
mod window;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::cli::Cli;

const HOST_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let cli = Cli::parse_args();
    logging::set_debug(cli.debug);

    // Must run before any thread is spawned or window/clipboard is created:
    // it may unset WAYLAND_DISPLAY for this process (issue #4).
    #[cfg(all(unix, not(target_os = "macos")))]
    wayland_compat::apply_decoration_workaround(cli.wayland);

    if let Err(e) = run(&cli) {
        eprintln!("ioscpy: error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: &Cli) -> Result<()> {
    if cli.list_json {
        let devices = match &cli.remote {
            Some(host) => remote::list_devices(host)?,
            None => device::list_devices()?,
        };
        println!("{}", serde_json::to_string(&remote::DeviceList {
            protocol: protocol::PROTOCOL_VERSION,
            devices,
        })?);
        return Ok(());
    }
    if cli.relay_stdio {
        let udid = cli.device.as_deref().ok_or_else(|| anyhow::anyhow!("--relay-stdio requires --device"))?;
        return remote::relay_stdio(udid, cli.port.unwrap_or(protocol::DEFAULT_PORT));
    }
    if cli.list {
        return cmd_list(cli);
    }
    cmd_connect(cli)
}

/// Print attached devices, one per line.
fn cmd_list(cli: &Cli) -> Result<()> {
    let devices = match &cli.remote {
        Some(host) => remote::list_devices(host)?,
        None => device::list_devices()?,
    };
    if devices.is_empty() {
        println!("No devices attached.");
        return Ok(());
    }
    for d in &devices {
        println!("{}", d.summary());
    }
    Ok(())
}

/// Default flow: open the device window and keep it live. All the networking runs
/// on a background thread since the window has to own the main thread.
fn cmd_connect(cli: &Cli) -> Result<()> {
    // Show a one line notice if a newer release is out, then kick off the
    // background refresh for next time. Opt out with IOSCPY_NO_UPDATE_CHECK.
    if std::env::var_os("IOSCPY_NO_UPDATE_CHECK").is_none() {
        if let Some(notice) = update::pending_notice(HOST_VERSION) {
            eprintln!("{notice}");
        }
        update::refresh_in_background();
    }

    let banner = format!("ioscpy v{HOST_VERSION} - github.com/dtrukr/ioscpy");
    eprintln!("{banner}");

    let port = cli.port.unwrap_or(protocol::DEFAULT_PORT);

    if cli.debug {
        print_debug_header(cli);
    }

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        let _ = ctrlc::set_handler(move || stop.store(true, Ordering::Relaxed));
    }

    // Handshake-only diagnostic path, no window.
    if cli.handshake_only {
        return run_connection_loop(cli, port, &stop, None, None, None);
    }

    if cli.stdio_bridge {
        return cmd_stdio_bridge(cli, port, &stop);
    }

    // Grab one frame for testing the stream path.
    if let Some(path) = cli.snapshot.clone() {
        return cmd_snapshot(cli, port, &path);
    }

    // Throughput measurement.
    if let Some(secs) = cli.bench {
        return cmd_bench(cli, port, secs);
    }

    // System-action test.
    if let Some(code) = cli.action {
        return cmd_action(cli, port, code);
    }

    // Accessibility tree dump for automation experiments.
    if cli.accessibility_tree {
        return cmd_accessibility_tree(cli, port);
    }

    // Accessibility action for automation experiments.
    if cli.accessibility_action.is_some() {
        return cmd_accessibility_action(cli, port);
    }

    // One-shot input/control operations for automation.
    if cli.has_one_shot_input() {
        return cmd_one_shot_input(cli, port);
    }

    // Run the real session loop headless for a while.
    if let Some(secs) = cli.soak {
        let stop = Arc::new(AtomicBool::new(false));
        {
            let stop = stop.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_secs(secs));
                stop.store(true, Ordering::Relaxed);
            });
        }
        println!("soak: running the streaming session for {secs}s, watching for reconnects…");
        let slot = window::new_frame_slot();
        return run_connection_loop(cli, port, &stop, Some(slot), None, None);
    }

    // Set the Dock icon on the main thread before the window opens, otherwise the
    // default executable icon flashes for a moment.
    window::set_app_icon();

    let slot = window::new_frame_slot();
    let (input_tx, input_rx) = mpsc::channel::<input::InputFrame>();
    // iPhone to Mac clipboard text goes from the net thread to the window thread,
    // which owns the pasteboard (and the main thread).
    let (clip_in_tx, clip_in_rx) = mpsc::channel::<String>();
    let net_slot = slot.clone();
    let net_stop = stop.clone();
    let net_cli = cli.clone();
    let net = thread::spawn(move || {
        if let Err(e) = run_connection_loop(
            &net_cli,
            port,
            &net_stop,
            Some(net_slot),
            Some(input_rx),
            Some(clip_in_tx),
        ) {
            eprintln!("ioscpy: error: {e:#}");
        }
        net_stop.store(true, Ordering::Relaxed);
    });

    let window_title = format!("ioscpy v{HOST_VERSION}");
    let result = window::run_window(&window_title, slot, stop.clone(), input_tx, clip_in_rx);
    stop.store(true, Ordering::Relaxed);
    let _ = net.join();
    result
}

#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StdioBridgeCommand {
    Touch { phase: String, x: f32, y: f32 },
    Action { code: u16 },
    Text { text: String },
    Key { key: String },
    KeyboardMode { suppress: bool },
    /// Ask the phone for a keyframe with fresh SPS/PPS (H.264 bridge).
    Keyframe,
}

fn cmd_stdio_bridge(cli: &Cli, port: u16, stop: &Arc<AtomicBool>) -> Result<()> {
    let mut forward: Option<remote::ConnectionGuard> = None;
    let mut stream = establish(cli, port, &mut forward)?;
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(None).ok();
    stream.set_write_timeout(Some(Duration::from_secs(6))).ok();

    let ack = protocol::handshake(&mut stream, HOST_VERSION).context("handshake failed")?;
    if ack.capabilities.stream_backends.is_empty() {
        bail!("the phone side is not advertising a stream backend");
    }
    if ack.capabilities.input_backends.is_empty() {
        bail!("the phone side is not advertising an input backend");
    }

    // H.264 only when the embedder asked for it and the phone offers it; the
    // phone's encoded frames are then passed through untouched.
    let want_h264 = cli.bridge_codec.eq_ignore_ascii_case("h264")
        && ack.capabilities.stream_backends.iter().any(|b| b == "h264");
    let mut writer = stream.try_clone().context("clone bridge control stream")?;
    protocol::write_frame(
        &mut writer,
        protocol::MessageType::StartStream,
        protocol::CHANNEL_CONTROL,
        0,
        &[if want_h264 { protocol::VIDEO_CODEC_H264 } else { protocol::VIDEO_CODEC_MJPEG }],
    )?;
    if want_h264 {
        protocol::write_frame(
            &mut writer,
            protocol::MessageType::RequestKeyframe,
            protocol::CHANNEL_CONTROL,
            0,
            &[],
        )?;
    }
    eprintln!("ioscpy bridge: codec {}", if want_h264 { "h264" } else { "mjpeg" });

    let (command_tx, command_rx) = mpsc::channel::<StdioBridgeCommand>();
    let input_stop = stop.clone();
    thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in BufReader::new(stdin.lock()).lines() {
            if input_stop.load(Ordering::Relaxed) {
                break;
            }
            match line {
                Ok(line) if !line.trim().is_empty() => {
                    match serde_json::from_str::<StdioBridgeCommand>(&line) {
                        Ok(command) => {
                            if command_tx.send(command).is_err() {
                                break;
                            }
                        }
                        Err(error) => eprintln!("ioscpy bridge: invalid command: {error}"),
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    eprintln!("ioscpy bridge: stdin failed: {error}");
                    break;
                }
            }
        }
        input_stop.store(true, Ordering::Relaxed);
    });

    let writer_stop = stop.clone();
    let writer_handle = thread::spawn(move || -> Result<()> {
        let mut seq = 1u64;
        let mut last_ping = Instant::now();
        let mut last_touch: Option<(f32, f32)> = None;
        while !writer_stop.load(Ordering::Relaxed) {
            match command_rx.recv_timeout(Duration::from_millis(8)) {
                Ok(StdioBridgeCommand::Touch { phase, x, y }) => {
                    let x = x.clamp(0.0, 1.0);
                    let y = y.clamp(0.0, 1.0);
                    let phase = match phase.as_str() {
                        "down" => protocol::TouchPhase::Down,
                        "move" => protocol::TouchPhase::Move,
                        "up" => protocol::TouchPhase::Up,
                        _ => {
                            eprintln!("ioscpy bridge: unknown touch phase {phase:?}");
                            continue;
                        }
                    };
                    if phase == protocol::TouchPhase::Move {
                        if let Some((start_x, start_y)) = last_touch {
                            let distance = (x - start_x).abs().max((y - start_y).abs());
                            let steps = (distance / 0.015).ceil().clamp(1.0, 48.0) as u32;
                            for step in 1..=steps {
                                let t = step as f32 / steps as f32;
                                send_touch_frame(
                                    &mut writer,
                                    phase,
                                    start_x + (x - start_x) * t,
                                    start_y + (y - start_y) * t,
                                    &mut seq,
                                )?;
                                if step < steps {
                                    thread::sleep(Duration::from_millis(2));
                                }
                            }
                        } else {
                            send_touch_frame(&mut writer, phase, x, y, &mut seq)?;
                        }
                    } else {
                        send_touch_frame(&mut writer, phase, x, y, &mut seq)?;
                    }
                    last_touch = if phase == protocol::TouchPhase::Up {
                        None
                    } else {
                        Some((x, y))
                    };
                }
                Ok(StdioBridgeCommand::Action { code }) => {
                    write_control_frame(
                        &mut writer,
                        protocol::MessageType::SystemAction,
                        &code.to_be_bytes(),
                        &mut seq,
                    )?;
                }
                Ok(StdioBridgeCommand::Text { text }) => {
                    write_control_frame(
                        &mut writer,
                        protocol::MessageType::InputText,
                        &protocol::encode_text(&text),
                        &mut seq,
                    )?;
                }
                Ok(StdioBridgeCommand::Key { key }) => {
                    let code = match parse_key_code(&key) {
                        Ok(code) => code,
                        Err(error) => {
                            eprintln!("ioscpy bridge: unknown key {key:?}: {error}");
                            continue;
                        }
                    };
                    write_control_frame(
                        &mut writer,
                        protocol::MessageType::InputKey,
                        &protocol::encode_key(code),
                        &mut seq,
                    )?;
                }
                Ok(StdioBridgeCommand::Keyframe) => {
                    write_control_frame(
                        &mut writer,
                        protocol::MessageType::RequestKeyframe,
                        &[],
                        &mut seq,
                    )?;
                }
                Ok(StdioBridgeCommand::KeyboardMode { suppress }) => {
                    write_control_frame(
                        &mut writer,
                        protocol::MessageType::KeyboardMode,
                        &protocol::encode_keyboard_mode(suppress),
                        &mut seq,
                    )?;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            if last_ping.elapsed() >= Duration::from_secs(3) {
                write_control_frame(&mut writer, protocol::MessageType::Ping, &[], &mut seq)?;
                last_ping = Instant::now();
            }
        }
        Ok(())
    });

    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    while !stop.load(Ordering::Relaxed) {
        let frame = protocol::read_frame(&mut stream)?;
        if frame.message_type() != Some(protocol::MessageType::VideoFrame) {
            continue;
        }
        let Some((width, height, flags, jpeg)) = protocol::parse_video_payload(&frame.payload)
        else {
            continue;
        };
        if flags & protocol::VIDEO_FLAG_H264 != 0 && !want_h264 {
            continue;
        }

        output.write_all(b"ICBR")?;
        output.write_all(&width.to_be_bytes())?;
        output.write_all(&height.to_be_bytes())?;
        output.write_all(&flags.to_be_bytes())?;
        output.write_all(&(jpeg.len() as u32).to_be_bytes())?;
        output.write_all(jpeg)?;
        output.flush()?;
    }

    stop.store(true, Ordering::Relaxed);
    let _ = writer_handle.join();
    Ok(())
}

/// Connect, handshake, run the session, and reconnect on drops until `stop` is set.
/// With a frame sink the session streams video; without one it just holds the
/// control channel. The `--handshake-only` path returns right after the handshake.
fn run_connection_loop(
    cli: &Cli,
    port: u16,
    stop: &Arc<AtomicBool>,
    frame_sink: Option<window::FrameSlot>,
    input_rx: Option<mpsc::Receiver<input::InputFrame>>,
    clip_in: Option<mpsc::Sender<String>>,
) -> Result<()> {
    let mut first = true;
    while !stop.load(Ordering::Relaxed) {
        // The forward has to outlive the session, so keep it in scope here.
        let mut forward: Option<remote::ConnectionGuard> = None;

        let mut stream = match establish(cli, port, &mut forward) {
            Ok(s) => s,
            Err(e) => {
                if cli.addr.is_some() || (cli.remote.is_some() && first) {
                    return Err(e);
                }
                warn!("{e:#}");
                if !reconnect_wait(stop) {
                    break;
                }
                continue;
            }
        };

        stream.set_nodelay(true).ok();
        // Time-bound the handshake so a daemon that accepts but never answers
        // errors out instead of hanging. The session loop drops the read timeout after.
        stream.set_read_timeout(Some(Duration::from_secs(8))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(8))).ok();

        let ack = match protocol::handshake(&mut stream, HOST_VERSION) {
            Ok(ack) => ack,
            Err(e) => {
                if cli.addr.is_some() || (cli.remote.is_some() && first) {
                    return Err(anyhow::Error::new(e).context("handshake with ioscpyd failed"));
                }
                warn!("handshake failed: {e}");
                if !reconnect_wait(stop) {
                    break;
                }
                continue;
            }
        };

        check_versions(&ack)?;
        if first {
            if let Some(notice) = update::phone_behind_notice(&ack.daemon_version, HOST_VERSION) {
                println!("{notice}");
            }
        }
        if first || cli.debug {
            health::print_capabilities(&ack);
        }
        if frame_sink.is_some() && ack.capabilities.stream_backends.is_empty() {
            warn!("the phone side isn't fully up yet, so the screen might not show. Respring the phone (or reinstall ioscpy from Sileo) and reconnect.");
        }
        first = false;

        if cli.handshake_only {
            return Ok(());
        }

        if frame_sink.is_some() {
            info!("session live. Close the window or press Ctrl-C to quit");
        } else {
            info!("session live. Press Ctrl-C to quit");
        }

        // Use H.264 when the device offers it and the user didn't force MJPEG.
        // The daemon also falls back to MJPEG if it can't honor the request.
        let codec = if !cli.mjpeg && ack.capabilities.stream_backends.iter().any(|b| b == "h264") {
            protocol::VIDEO_CODEC_H264
        } else {
            protocol::VIDEO_CODEC_MJPEG
        };

        // Only hide the device keyboard if asked and the tweak can do it.
        let suppress_keyboard = cli.no_keyboard && ack.capabilities.keyboard;

        match health::run_session(
            stream,
            stop.clone(),
            frame_sink.clone(),
            input_rx.as_ref(),
            clip_in.as_ref(),
            codec,
            suppress_keyboard,
        )? {
            health::SessionEnd::Quit => break,
            health::SessionEnd::Lost => {
                warn!("connection lost, reconnecting…");
                if !reconnect_wait(stop) {
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Connect, stream, save the first frame's JPEG to `path`, then exit.
fn cmd_snapshot(cli: &Cli, port: u16, path: &str) -> Result<()> {
    let mut forward: Option<remote::ConnectionGuard> = None;
    let mut stream = establish(cli, port, &mut forward)?;
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(Duration::from_secs(15))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(8))).ok();

    let ack =
        protocol::handshake(&mut stream, HOST_VERSION).context("handshake with ioscpyd failed")?;
    health::print_capabilities(&ack);
    if ack.capabilities.stream_backends.is_empty() {
        warn!("the phone side isn't fully up yet, so the screen might not show. Respring the phone (or reinstall ioscpy from Sileo) and reconnect.");
    }

    protocol::write_frame(
        &mut stream,
        protocol::MessageType::StartStream,
        protocol::CHANNEL_CONTROL,
        0,
        &[],
    )?;

    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if Instant::now() > deadline {
            bail!("no video frame within 15s (is the tweak streaming and the screen on?)");
        }
        let frame = protocol::read_frame(&mut stream)?;
        if frame.message_type() == Some(protocol::MessageType::VideoFrame) {
            if let Some((w, h, flags, jpeg)) = protocol::parse_video_payload(&frame.payload) {
                // A previous H.264 stream may still have a queued frame after
                // START_STREAM requests MJPEG. Only write an actual JPEG.
                if flags & protocol::VIDEO_FLAG_H264 != 0 || !jpeg.starts_with(&[0xff, 0xd8]) {
                    continue;
                }
                std::fs::write(path, jpeg).with_context(|| format!("could not write {path}"))?;
                println!("saved {w}x{h} frame ({} bytes) to {path}", jpeg.len());
                let _ = protocol::write_frame(
                    &mut stream,
                    protocol::MessageType::StopStream,
                    protocol::CHANNEL_CONTROL,
                    0,
                    &[],
                );
                return Ok(());
            }
        }
    }
}

/// Stream for `secs` seconds with no window and print the numbers.
fn cmd_bench(cli: &Cli, port: u16, secs: u64) -> Result<()> {
    let mut forward: Option<remote::ConnectionGuard> = None;
    let mut stream = establish(cli, port, &mut forward)?;
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

    let ack =
        protocol::handshake(&mut stream, HOST_VERSION).context("handshake with ioscpyd failed")?;
    if ack.capabilities.stream_backends.is_empty() {
        warn!("the phone side isn't fully up yet, so the screen might not show. Respring the phone (or reinstall ioscpy from Sileo) and reconnect.");
    }
    let codec = if !cli.mjpeg && ack.capabilities.stream_backends.iter().any(|b| b == "h264") {
        protocol::VIDEO_CODEC_H264
    } else {
        protocol::VIDEO_CODEC_MJPEG
    };
    println!(
        "bench: requesting {} stream",
        if codec == protocol::VIDEO_CODEC_H264 {
            "h264"
        } else {
            "mjpeg"
        }
    );
    protocol::write_frame(
        &mut stream,
        protocol::MessageType::StartStream,
        protocol::CHANNEL_CONTROL,
        0,
        &[codec],
    )?;
    // Ask for a keyframe so H.264 decodes from the first frame.
    let _ = protocol::write_frame(
        &mut stream,
        protocol::MessageType::RequestKeyframe,
        protocol::CHANNEL_CONTROL,
        0,
        &[],
    );

    let start = Instant::now();
    let window = Duration::from_secs(secs);
    let (mut frames, mut bytes, mut decoded, mut h264_frames, mut keyframes) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut decode_total = Duration::ZERO;
    let mut read_total = Duration::ZERO;
    let (mut w, mut h) = (0u32, 0u32);
    let mut h264_dec: Option<h264::H264Decoder> = None;

    while start.elapsed() < window {
        let rt = Instant::now();
        let frame = protocol::read_frame(&mut stream)?;
        read_total += rt.elapsed();
        if frame.message_type() == Some(protocol::MessageType::VideoFrame) {
            if let Some((fw, fh, flags, data)) = protocol::parse_video_payload(&frame.payload) {
                frames += 1;
                bytes += data.len() as u64;
                (w, h) = (fw, fh);
                if flags & protocol::VIDEO_FLAG_H264 != 0 {
                    h264_frames += 1;
                    if flags & protocol::VIDEO_FLAG_KEYFRAME != 0 {
                        keyframes += 1;
                    }
                    // Run the real VideoToolbox decode so we exercise the whole
                    // pipeline, and time it.
                    if h264_dec.is_none() {
                        h264_dec = h264::H264Decoder::new();
                    }
                    if let Some(d) = h264_dec.as_mut() {
                        let t = Instant::now();
                        if let h264::Decoded::Frame(f) = d.decode(data) {
                            decode_total += t.elapsed();
                            decoded += 1;
                            (w, h) = (f.width as u32, f.height as u32);
                        }
                    }
                } else {
                    // MJPEG frame: decode to check it's valid and time it.
                    let t = Instant::now();
                    if video::decode_jpeg(data).is_some() {
                        decode_total += t.elapsed();
                        decoded += 1;
                    }
                }
            }
        }
    }
    let _ = protocol::write_frame(
        &mut stream,
        protocol::MessageType::StopStream,
        protocol::CHANNEL_CONTROL,
        0,
        &[],
    );

    let elapsed = start.elapsed().as_secs_f64();
    let n = frames.max(1) as f64;
    let kind = if h264_frames > 0 { "h264" } else { "mjpeg" };
    println!(
        "bench: {frames} {kind} frames in {elapsed:.1}s = {:.1} fps",
        frames as f64 / elapsed
    );
    println!(
        "  {w}x{h}, avg {:.1} KB/frame, ~{:.2} MB/s over the wire",
        bytes as f64 / n / 1024.0,
        bytes as f64 / elapsed / 1024.0 / 1024.0
    );
    if h264_frames > 0 {
        println!("  h264: {h264_frames} frames, {keyframes} keyframes");
    }
    if decoded > 0 {
        println!(
            "  read {:.1} ms/frame, host decode {:.1} ms/frame",
            read_total.as_secs_f64() * 1000.0 / n,
            decode_total.as_secs_f64() * 1000.0 / decoded as f64
        );
    }
    Ok(())
}

/// Send one or more automation input frames and exit without opening a window.
fn cmd_one_shot_input(cli: &Cli, port: u16) -> Result<()> {
    let mut forward: Option<remote::ConnectionGuard> = None;
    let mut stream = establish(cli, port, &mut forward)?;
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(Duration::from_secs(6))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(6))).ok();

    let ack = protocol::handshake(&mut stream, HOST_VERSION).context("handshake failed")?;
    if ack.capabilities.input_backends.is_empty()
        && (cli.tap.is_some() || cli.swipe.is_some() || cli.text.is_some() || cli.key.is_some())
    {
        bail!("the phone side is not advertising an input backend; make sure ioscpyhook is loaded");
    }

    let mut seq = 1;
    if let Some(mode) = &cli.keyboard_mode {
        let suppress = parse_keyboard_mode(mode)?;
        write_control_frame(
            &mut stream,
            protocol::MessageType::KeyboardMode,
            &protocol::encode_keyboard_mode(suppress),
            &mut seq,
        )?;
    }

    if let Some(text) = &cli.clipboard_set {
        let mut payload = Vec::with_capacity(1 + text.len());
        payload.push(if cli.paste { 1 } else { 0 });
        payload.extend_from_slice(text.as_bytes());
        write_control_frame(
            &mut stream,
            protocol::MessageType::ClipboardSet,
            &payload,
            &mut seq,
        )?;
    } else if cli.paste {
        bail!("--paste requires --clipboard-set");
    }

    if let Some(text) = &cli.text {
        write_control_frame(
            &mut stream,
            protocol::MessageType::InputText,
            &protocol::encode_text(text),
            &mut seq,
        )?;
    }

    if let Some(key) = &cli.key {
        let code = parse_key_code(key)?;
        write_control_frame(
            &mut stream,
            protocol::MessageType::InputKey,
            &protocol::encode_key(code),
            &mut seq,
        )?;
    }

    if let Some(tap) = &cli.tap {
        let (x, y) = parse_pair(tap)?;
        send_touch_frame(&mut stream, protocol::TouchPhase::Down, x, y, &mut seq)?;
        thread::sleep(Duration::from_millis(35));
        send_touch_frame(&mut stream, protocol::TouchPhase::Up, x, y, &mut seq)?;
    }

    if let Some(swipe) = &cli.swipe {
        let (x1, y1, x2, y2) = parse_quad(swipe)?;
        let duration = Duration::from_millis(cli.duration_ms.max(1));
        send_touch_frame(&mut stream, protocol::TouchPhase::Down, x1, y1, &mut seq)?;
        let steps = ((cli.duration_ms / 16).clamp(3, 60)) as u32;
        for step in 1..steps {
            let t = step as f32 / steps as f32;
            let x = x1 + (x2 - x1) * t;
            let y = y1 + (y2 - y1) * t;
            send_touch_frame(&mut stream, protocol::TouchPhase::Move, x, y, &mut seq)?;
            thread::sleep(duration / steps);
        }
        send_touch_frame(&mut stream, protocol::TouchPhase::Up, x2, y2, &mut seq)?;
    }

    // SSH may close immediately when this command returns. A PONG proves the
    // daemon read every preceding control frame before the relay is dropped.
    wait_control_drain(&mut stream, seq)?;
    Ok(())
}

fn wait_control_drain(stream: &mut TcpStream, seq: u64) -> Result<()> {
    protocol::write_frame(
        stream,
        protocol::MessageType::Ping,
        protocol::CHANNEL_CONTROL,
        seq,
        &[],
    )?;
    loop {
        let frame = protocol::read_frame(stream).context("waiting for input acknowledgment")?;
        match frame.message_type() {
            Some(protocol::MessageType::Pong) if frame.header.seq == seq => return Ok(()),
            Some(protocol::MessageType::Error) => {
                bail!("the iPhone rejected an input command: {}", String::from_utf8_lossy(&frame.payload));
            }
            _ => {}
        }
    }
}

fn write_control_frame(
    stream: &mut TcpStream,
    message_type: protocol::MessageType,
    payload: &[u8],
    seq: &mut u64,
) -> Result<()> {
    protocol::write_frame(
        stream,
        message_type,
        protocol::CHANNEL_CONTROL,
        *seq,
        payload,
    )?;
    *seq += 1;
    Ok(())
}

fn send_touch_frame(
    stream: &mut TcpStream,
    phase: protocol::TouchPhase,
    x: f32,
    y: f32,
    seq: &mut u64,
) -> Result<()> {
    write_control_frame(
        stream,
        protocol::MessageType::InputTouch,
        &protocol::encode_touch(phase, 0, x, y),
        seq,
    )
}

fn parse_pair(raw: &str) -> Result<(f32, f32)> {
    let parts: Vec<_> = raw.split(',').map(str::trim).collect();
    if parts.len() != 2 {
        bail!("expected X,Y normalized coordinates");
    }
    Ok((parse_norm(parts[0], "x")?, parse_norm(parts[1], "y")?))
}

fn parse_quad(raw: &str) -> Result<(f32, f32, f32, f32)> {
    let parts: Vec<_> = raw.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        bail!("expected X1,Y1,X2,Y2 normalized coordinates");
    }
    Ok((
        parse_norm(parts[0], "x1")?,
        parse_norm(parts[1], "y1")?,
        parse_norm(parts[2], "x2")?,
        parse_norm(parts[3], "y2")?,
    ))
}

fn parse_norm(raw: &str, name: &str) -> Result<f32> {
    let value: f32 = raw
        .parse()
        .with_context(|| format!("invalid {name} coordinate: {raw}"))?;
    if !(0.0..=1.0).contains(&value) {
        bail!("{name} must be in [0,1], got {value}");
    }
    Ok(value)
}

fn parse_keyboard_mode(raw: &str) -> Result<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "hide" | "hidden" | "suppress" | "on" | "1" | "true" => Ok(true),
        "show" | "restore" | "off" | "0" | "false" => Ok(false),
        other => bail!("unknown keyboard mode {other:?}; expected hide or show"),
    }
}

fn parse_key_code(raw: &str) -> Result<protocol::KeyCode> {
    match raw.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "enter" | "return" => Ok(protocol::KeyCode::Enter),
        "backspace" | "delete" => Ok(protocol::KeyCode::Backspace),
        "tab" => Ok(protocol::KeyCode::Tab),
        "escape" | "esc" => Ok(protocol::KeyCode::Escape),
        "left" | "arrow-left" => Ok(protocol::KeyCode::Left),
        "right" | "arrow-right" => Ok(protocol::KeyCode::Right),
        "up" | "arrow-up" => Ok(protocol::KeyCode::Up),
        "down" | "arrow-down" => Ok(protocol::KeyCode::Down),
        "select-all" | "selectall" | "cmd-a" => Ok(protocol::KeyCode::SelectAll),
        "copy" | "cmd-c" => Ok(protocol::KeyCode::Copy),
        "paste" | "cmd-v" => Ok(protocol::KeyCode::Paste),
        "cut" | "cmd-x" => Ok(protocol::KeyCode::Cut),
        "undo" | "cmd-z" => Ok(protocol::KeyCode::Undo),
        other => bail!("unknown key {other:?}"),
    }
}

/// Request a JSON accessibility tree from the device side and print it.
fn cmd_accessibility_tree(cli: &Cli, port: u16) -> Result<()> {
    let mut forward: Option<remote::ConnectionGuard> = None;
    let mut stream = establish(cli, port, &mut forward)?;
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(6))).ok();

    let ack = protocol::handshake(&mut stream, HOST_VERSION).context("handshake failed")?;
    if !ack.capabilities.accessibility {
        bail!("the phone side is not advertising accessibility support; install a matching ioscpy device package and respring");
    }

    let request = serde_json::json!({
        "max_depth": 12,
        "max_nodes": 2000,
        "include_hidden": false
    });
    protocol::write_frame(
        &mut stream,
        protocol::MessageType::AccessibilitySnapshot,
        protocol::CHANNEL_CONTROL,
        1,
        &serde_json::to_vec(&request)?,
    )?;

    loop {
        let frame = protocol::read_frame(&mut stream)?;
        match frame.message_type() {
            Some(protocol::MessageType::AccessibilityTree) => {
                println!("{}", String::from_utf8_lossy(&frame.payload));
                return Ok(());
            }
            Some(protocol::MessageType::Error) => {
                let error: protocol::DaemonError = serde_json::from_slice(&frame.payload)?;
                bail!(
                    "accessibility tree failed [{}]: {}",
                    error.code,
                    error.message
                );
            }
            Some(protocol::MessageType::Log) | Some(protocol::MessageType::Pong) => {
                continue;
            }
            _ => continue,
        }
    }
}

/// Send one accessibility action request and print the result JSON.
fn cmd_accessibility_action(cli: &Cli, port: u16) -> Result<()> {
    let raw = cli
        .accessibility_action
        .as_deref()
        .context("--accessibility-action requires JSON")?;
    let payload = accessibility_action_payload(raw)?;

    let mut forward: Option<remote::ConnectionGuard> = None;
    let mut stream = establish(cli, port, &mut forward)?;
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(6))).ok();

    let ack = protocol::handshake(&mut stream, HOST_VERSION).context("handshake failed")?;
    if !ack.capabilities.accessibility {
        bail!("the phone side is not advertising accessibility support; install a matching ioscpy device package and respring");
    }

    protocol::write_frame(
        &mut stream,
        protocol::MessageType::AccessibilityAction,
        protocol::CHANNEL_CONTROL,
        1,
        &payload,
    )?;

    loop {
        let frame = protocol::read_frame(&mut stream)?;
        match frame.message_type() {
            Some(protocol::MessageType::AccessibilityActionResult) => {
                println!("{}", String::from_utf8_lossy(&frame.payload));
                return Ok(());
            }
            Some(protocol::MessageType::Error) => {
                println!("{}", String::from_utf8_lossy(&frame.payload));
                return Ok(());
            }
            Some(protocol::MessageType::Log) | Some(protocol::MessageType::Pong) => {
                continue;
            }
            _ => continue,
        }
    }
}

fn accessibility_action_payload(raw: &str) -> Result<Vec<u8>> {
    let mut value: serde_json::Value =
        serde_json::from_str(raw).context("--accessibility-action must be valid JSON")?;
    let object = value
        .as_object_mut()
        .context("--accessibility-action must be a JSON object")?;
    object
        .entry("schema".to_string())
        .or_insert_with(|| serde_json::Value::String("ioscpy.accessibility.action.v1".to_string()));
    Ok(serde_json::to_vec(&value)?)
}

/// Send one system action and exit without taking video ownership.
fn cmd_action(cli: &Cli, port: u16, code: u16) -> Result<()> {
    let mut forward: Option<remote::ConnectionGuard> = None;
    let mut stream = establish(cli, port, &mut forward)?;
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(Duration::from_secs(6))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(6))).ok();

    let ack = protocol::handshake(&mut stream, HOST_VERSION).context("handshake failed")?;
    if ack.capabilities.input_backends.is_empty() {
        bail!("the phone side is not advertising an input backend; make sure ioscpyhook is loaded");
    }
    protocol::write_frame(
        &mut stream,
        protocol::MessageType::SystemAction,
        protocol::CHANNEL_CONTROL,
        1,
        &code.to_be_bytes(),
    )?;
    wait_control_drain(&mut stream, 2)?;
    println!("sent SYSTEM_ACTION {code}");
    Ok(())
}

/// Open the transport for one connection attempt. Stashes the USB forward (if any)
/// in `forward_slot` so the caller can keep it alive for the session.
fn establish(
    cli: &Cli,
    port: u16,
    forward_slot: &mut Option<remote::ConnectionGuard>,
) -> Result<TcpStream> {
    if let Some(addr) = &cli.addr {
        info!("connecting directly to {addr}");
        return TcpStream::connect(addr).with_context(|| format!("could not connect to {addr}"));
    }

    if let Some(host) = &cli.remote {
        let devices = remote::list_devices(host)?;
        if devices.is_empty() {
            bail!("no USB iPhone attached to {host}");
        }
        let dev = device::select_device(devices, cli.device.as_deref())?;
        info!("remote device {} on {} (iOS {})", dev.udid, host, dev.ios_version);
        let (forward, stream) = remote::RemoteForward::start(host, &dev.udid, port)?;
        *forward_slot = Some(remote::ConnectionGuard::Remote(forward));
        return Ok(stream);
    }

    let devices = device::list_devices()?;
    let dev = device::select_device(devices, cli.device.as_deref())?;
    info!(
        "device {}, {} (iOS {})",
        dev.udid, dev.product_type, dev.ios_version
    );
    let forward = usbmux::UsbForward::start(&dev.udid, port)
        .context("couldn't set up the USB link to the iPhone")?;
    debug!(
        "usbmux 127.0.0.1:{} -> device :{}",
        forward.local_port, forward.device_port
    );
    let stream = forward.connect()?;
    *forward_slot = Some(remote::ConnectionGuard::Usb(forward));
    Ok(stream)
}

/// The protocol version must match. A different build version is just noted under
/// `--debug`.
fn check_versions(ack: &protocol::HelloAck) -> Result<()> {
    if ack.protocol_version != protocol::PROTOCOL_VERSION {
        bail!(
            "the Mac and the phone are running different ioscpy versions (Mac speaks v{}, phone speaks v{}). \
             Update the forked host and device package from github.com/dtrukr/ioscpy.",
            protocol::PROTOCOL_VERSION,
            ack.protocol_version
        );
    }
    if ack.daemon_version != HOST_VERSION {
        debug!(
            "version note: host {HOST_VERSION}, daemon {}",
            ack.daemon_version
        );
    }
    Ok(())
}

/// Short pause between reconnect attempts, interruptible with Ctrl-C. Returns false
/// if the user asked to quit during the wait.
fn reconnect_wait(stop: &Arc<AtomicBool>) -> bool {
    for _ in 0..15 {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        thread::sleep(Duration::from_millis(100));
    }
    !stop.load(Ordering::Relaxed)
}

/// Diagnostics header for `--debug`.
fn print_debug_header(cli: &Cli) {
    eprintln!("ioscpy {HOST_VERSION}");
    eprintln!("os     {}", platform::os_version());
    let target = cli.remote.as_ref().map(|host| format!("remote {host}"))
        .or_else(|| cli.addr.clone())
        .or_else(|| cli.device.clone())
        .unwrap_or_else(|| "auto (single attached device)".to_string());
    eprintln!("target {target}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessibility_action_payload_adds_schema() {
        let payload = accessibility_action_payload(
            r#"{"action":"tap","frame":{"x":1,"y":2,"width":3,"height":4}}"#,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(value["schema"], "ioscpy.accessibility.action.v1");
        assert_eq!(value["action"], "tap");
        assert_eq!(value["frame"]["width"], 3);
    }

    #[test]
    fn accessibility_action_payload_preserves_schema() {
        let payload =
            accessibility_action_payload(r#"{"schema":"custom","action":"tap"}"#).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(value["schema"], "custom");
    }

    #[test]
    fn accessibility_action_payload_rejects_non_object() {
        assert!(accessibility_action_payload(r#"["tap"]"#).is_err());
    }

    #[test]
    fn parses_normalized_coordinates() {
        assert_eq!(parse_pair("0.25, 1").unwrap(), (0.25, 1.0));
        assert!(parse_pair("1.2,0").is_err());
    }

    #[test]
    fn parses_one_shot_keys() {
        assert_eq!(
            parse_key_code("cmd-a").unwrap() as u8,
            protocol::KeyCode::SelectAll as u8
        );
        assert_eq!(
            parse_key_code("arrow-left").unwrap() as u8,
            protocol::KeyCode::Left as u8
        );
        assert!(parse_key_code("volume-up").is_err());
    }
}
