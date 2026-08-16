//! CH57x `514c:8850` programming protocol.
//!
//! Reverse-engineered from the vendor app's live `hid_write` traffic (captured via
//! a `DYLD_INSERT_LIBRARIES` hook on the bundled libhidapi). Every write is a 65-byte
//! HID buffer whose first byte is the report id `0x03`; the remaining 64 bytes are the
//! payload this module builds. Programming one binding is:
//!
//!   1. a "bind" message  : `fd <key_id> <layer+1> <kind> 00 00 00 00 00 <count> <entries…>`
//!   2. a "finish" message: `fd fe ff`
//!
//! and once, right after opening the device, an init message: `fb fb fb 01`.
//!
//! The keyboard, modifier, macro, media and mouse encodings are all matched
//! byte-for-byte against vendor captures, and most are confirmed by reading the
//! mapping back off the device. See `docs/protocol.md` for the full byte layouts.

use anyhow::{bail, ensure, Result};

use crate::keys::Macro;

/// Length of the payload we build (the report-id byte `0x03` is added in `Device::send`).
pub const MSG_LEN: usize = 64;
/// HID report id every message on this device carries.
pub const REPORT_ID: u8 = 0x03;

#[derive(Debug, Clone, Copy)]
pub enum KnobAction {
    Ccw = 0,
    Press = 1,
    Cw = 2,
}

#[derive(Debug, Clone, Copy)]
pub enum Target {
    /// Button index, 0-based.
    Button(u8),
    /// Knob index (0-based) + action.
    Knob(u8, KnobAction),
}

impl Target {
    /// Firmware key id. Buttons: `n+1`. Knobs: `16 + 3*n + action`.
    /// (Verified: `knob0.press` = 17. CCW=16 / CW=18 inferred, not yet captured.)
    pub fn key_id(&self) -> Result<u8> {
        const MAX_BUTTONS: u8 = 16;
        Ok(match self {
            Target::Button(n) if *n >= MAX_BUTTONS => bail!("button index {n} out of range (0..15)"),
            Target::Button(n) => n + 1,
            Target::Knob(n, action) => MAX_BUTTONS + 3 * n + (*action as u8),
        })
    }
}

fn pad(mut msg: Vec<u8>) -> Result<[u8; MSG_LEN]> {
    ensure!(msg.len() <= MSG_LEN, "message too long ({} > {MSG_LEN})", msg.len());
    msg.resize(MSG_LEN, 0);
    let mut buf = [0u8; MSG_LEN];
    buf.copy_from_slice(&msg);
    Ok(buf)
}

/// Standard HID modifier bitmask from this crate's modifier report codes (0xf1..=0xf8).
fn modifier_bitmask(codes: &[u8]) -> u8 {
    codes
        .iter()
        .filter(|&&c| (0xf1..=0xf8).contains(&c))
        .fold(0u8, |acc, &c| acc | (1u8 << (c - 0xf1)))
}

/// Build the payloads that program `target` on `layer` to `mac`.
/// Each payload is 64 bytes; `Device::send` prepends the `0x03` report id.
pub fn build_binding(layer: u8, target: Target, mac: &Macro) -> Result<Vec<[u8; MSG_LEN]>> {
    ensure!(layer <= 15, "layer {layer} out of range (0..15)");
    let key_id = target.key_id()?;

    // Header: command 0xfd, key id, layer (1-based), macro kind.
    let mut msg = vec![0xfd, key_id, layer + 1, mac.kind()];

    match mac {
        Macro::Keyboard(accords) => {
            // Layout: 5 reserved zero bytes, key count, then [modifier_mask, keycode] per key.
            let mut entries: Vec<[u8; 2]> = Vec::new();
            for a in accords {
                entries.push([modifier_bitmask(&a.modifier_codes), a.key_code.unwrap_or(0)]);
            }
            ensure!(entries.len() <= 25, "macro sequence too long ({} keys, max 25)", entries.len());
            msg.extend_from_slice(&[0, 0, 0, 0, 0]);
            msg.push(entries.len() as u8);
            for e in entries {
                msg.extend_from_slice(&e);
            }
        }
        Macro::Media(code) => {
            // Same framing as keyboard: 5 reserved zeros, then `0x02`, then the
            // 16-bit consumer code little-endian. (Verified: vol up/down, brightness.)
            let [low, high] = code.to_le_bytes();
            msg.extend_from_slice(&[0, 0, 0, 0, 0, 0x02, low, high]);
        }
        Macro::Mouse(data) => {
            // Same framing as keyboard: 5 reserved zeros, count 1, then
            // [modifier, buttons, dx, dy, wheel].
            msg.extend_from_slice(&[0, 0, 0, 0, 0, 0x01]);
            msg.extend_from_slice(data);
        }
    }

    Ok(vec![pad(msg)?, pad(vec![0xfd, 0xfe, 0xff])?])
}

/// The init message sent once, right after the device is opened (`03 fb fb fb 01`).
pub fn init_message() -> [u8; MSG_LEN] {
    pad(vec![0xfb, 0xfb, 0xfb, 0x01]).expect("init fits")
}

/// RGB/backlight target ("key id"), kind, and effect-mode base value.
/// Captured layout: `fe b0 <layer+1> 08 00 00 00 00 00 01 00 <0x50+mode>` + finish.
pub const RGB_TARGET: u8 = 0xb0;
pub const RGB_KIND: u8 = 0x08;
pub const RGB_MODE_BASE: u8 = 0x50;

/// Build the messages that set the RGB/backlight effect `mode` on `layer`.
/// (Modes 0..=5 are the distinct effects; mode 6 tested = no effect. Color/brightness
/// control not yet captured.)
pub fn build_rgb_mode(layer: u8, mode: u8) -> Result<Vec<[u8; MSG_LEN]>> {
    ensure!(layer <= 15, "layer {layer} out of range (0..15)");
    ensure!(mode <= 0x0f, "rgb mode {mode} out of range (0..15)");
    let value = RGB_MODE_BASE + mode;
    let msg = vec![0xfe, RGB_TARGET, layer + 1, RGB_KIND, 0, 0, 0, 0, 0, 1, 0, value];
    Ok(vec![pad(msg)?, pad(vec![0xfd, 0xfe, 0xff])?])
}

/// Build the messages that set the mouse report `latency` (ms) on `layer`.
/// Captured layout: `fd 00 <layer+1> 05 <ms_lo> <ms_hi>` + finish — a binding-style
/// write on key_id 0, kind `05`, with the latency as a 16-bit little-endian value.
/// (Verified for 10/20/50/100/1000/0 ms.) Write-only: not reported back by `read`.
pub fn build_latency(layer: u8, ms: u16) -> Result<Vec<[u8; MSG_LEN]>> {
    ensure!(layer <= 15, "layer {layer} out of range (0..15)");
    let [lo, hi] = ms.to_le_bytes();
    let msg = vec![0xfd, 0x00, layer + 1, 0x05, lo, hi];
    Ok(vec![pad(msg)?, pad(vec![0xfd, 0xfe, 0xff])?])
}

/// Number of mapping layers the device stores.
pub const LAYERS: u8 = 3;
/// Command byte the device uses in its read-back reports.
pub const READ_CMD: u8 = 0xfa;

/// Query asking the device to stream back every binding for `layer` (0-based):
/// `03 fa 0f 03 <layer+1>`.
pub fn read_layer_query(layer: u8) -> [u8; MSG_LEN] {
    pad(vec![0xfa, 0x0f, LAYERS, layer + 1]).expect("query fits")
}

/// Human name for a knob key id. 16/17/18 = ccw/press/cw; 19/21 = the press-held
/// "virtual knob 1" combos (press+ccw / press+cw). Slot 20 (press+press) is programmable
/// but never emitted — a double-click fires `press` (17) twice. Others are phantom slots.
pub fn key_id_name(key_id: u8) -> String {
    match key_id {
        16 => "ccw".into(),
        17 => "press".into(),
        18 => "cw".into(),
        19 => "press+ccw".into(),
        20 => "press+press".into(),
        21 => "press+cw".into(),
        n => format!("key{n}"),
    }
}
