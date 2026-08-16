//! Key-name tables and macro-string parsing.
//!
//! A "macro" is what a knob action or button emits. Supported forms:
//!   - Keyboard accords:  `a` · `ctrl-c` · `ctrl-shift-t` · `a,b,c` (comma = sequence)
//!   - Media keys:        `volumeup` · `mute` · `playpause` ...
//!   - Mouse:             `click(left)` · `click(left+right)` · `wheel(1)` · `wheel(-1)`
//!   - Raw HID code:      `<41>` (decimal usage code on the keyboard page)

use anyhow::{bail, Result};

/// One key press with its held modifiers, e.g. `ctrl-shift-t`.
#[derive(Debug, Clone)]
pub struct Accord {
    /// Modifier *report* codes (0xf1..=0xf8), in the order given.
    pub modifier_codes: Vec<u8>,
    /// HID usage code on the keyboard page (0x07). `None` for a bare modifier chord.
    pub key_code: Option<u8>,
}

#[derive(Debug, Clone)]
pub enum Macro {
    Keyboard(Vec<Accord>),
    /// Consumer-page usage code (16-bit).
    Media(u16),
    /// Mouse payload: `[modifier, buttons, dx, dy, wheel]`.
    Mouse([u8; 5]),
}

impl Macro {
    /// Device "kind" byte: 1=keyboard, 2=media, 3=mouse.
    pub fn kind(&self) -> u8 {
        match self {
            Macro::Keyboard(_) => 1,
            Macro::Media(_) => 2,
            Macro::Mouse(_) => 3,
        }
    }
}

/// Modifier name -> report code used inside a binding payload.
fn modifier_code(name: &str) -> Option<u8> {
    Some(match name {
        "ctrl" | "control" | "lctrl" => 0xf1,
        "shift" | "lshift" => 0xf2,
        "alt" | "opt" | "option" | "lalt" => 0xf3,
        "win" | "cmd" | "super" | "gui" | "meta" | "lwin" => 0xf4,
        "rctrl" | "rcontrol" => 0xf5,
        "rshift" => 0xf6,
        "ralt" | "ropt" => 0xf7,
        "rwin" | "rcmd" | "rsuper" | "rgui" => 0xf8,
        _ => return None,
    })
}

/// Keyboard-page (0x07) usage code for a key name.
fn key_code(name: &str) -> Option<u8> {
    // Single a-z / 0-9 fast paths.
    if name.len() == 1 {
        let c = name.as_bytes()[0];
        if c.is_ascii_lowercase() {
            return Some(0x04 + (c - b'a'));
        }
        if c.is_ascii_digit() {
            return Some(if c == b'0' { 0x27 } else { 0x1e + (c - b'1') });
        }
    }
    Some(match name {
        "enter" | "return" => 0x28,
        "esc" | "escape" => 0x29,
        "backspace" | "bksp" => 0x2a,
        "tab" => 0x2b,
        "space" | "spacebar" => 0x2c,
        "minus" | "-" => 0x2d,
        "equal" | "=" => 0x2e,
        "leftbracket" | "[" => 0x2f,
        "rightbracket" | "]" => 0x30,
        "backslash" | "\\" => 0x31,
        "semicolon" | ";" => 0x33,
        "quote" | "'" => 0x34,
        "grave" | "backtick" | "`" => 0x35,
        "comma" => 0x36,
        "dot" | "period" | "." => 0x37,
        "slash" | "/" => 0x38,
        "capslock" => 0x39,
        "f1" => 0x3a,
        "f2" => 0x3b,
        "f3" => 0x3c,
        "f4" => 0x3d,
        "f5" => 0x3e,
        "f6" => 0x3f,
        "f7" => 0x40,
        "f8" => 0x41,
        "f9" => 0x42,
        "f10" => 0x43,
        "f11" => 0x44,
        "f12" => 0x45,
        "printscreen" | "prtsc" => 0x46,
        "scrolllock" => 0x47,
        "pause" => 0x48,
        "insert" | "ins" => 0x49,
        "home" => 0x4a,
        "pageup" | "pgup" => 0x4b,
        "delete" | "del" => 0x4c,
        "end" => 0x4d,
        "pagedown" | "pgdn" => 0x4e,
        "right" | "rightarrow" => 0x4f,
        "left" | "leftarrow" => 0x50,
        "down" | "downarrow" => 0x51,
        "up" | "uparrow" => 0x52,
        "numlock" => 0x53,
        _ => return None,
    })
}

/// Consumer-page (0x0c) usage code for a media key name.
fn media_code(name: &str) -> Option<u16> {
    Some(match name {
        "brightnessup" | "brightup" => 0x6f,
        "brightnessdown" | "brightdown" => 0x70,
        "next" | "nexttrack" => 0xb5,
        "prev" | "previous" | "prevtrack" => 0xb6,
        "stop" => 0xb7,
        "play" | "playpause" | "pause_media" => 0xcd,
        "mute" => 0xe2,
        "volumeup" | "volup" => 0xe9,
        "volumedown" | "voldown" => 0xea,
        "favorites" | "favourites" => 0x182,
        "calculator" | "calc" => 0x192,
        "screenlock" | "lock" => 0x19e,
        "webhome" | "home_page" => 0x223,
        "webback" | "back" => 0x224,
        "webforward" | "forward" => 0x225,
        _ => return None,
    })
}

/// Names printed by `knobctl keys`.
pub const KEY_NAMES: &[&str] = &[
    "a..z", "0..9", "enter", "esc", "backspace", "tab", "space", "minus", "equal",
    "leftbracket", "rightbracket", "backslash", "semicolon", "quote", "grave", "comma",
    "dot", "slash", "capslock", "f1..f12", "printscreen", "scrolllock", "pause", "insert",
    "home", "pageup", "delete", "end", "pagedown", "left", "right", "up", "down", "numlock",
];
pub const MODIFIER_NAMES: &[&str] =
    &["ctrl", "shift", "alt/opt", "cmd", "rctrl", "rshift", "ralt", "rcmd"];
pub const MEDIA_NAMES: &[&str] = &[
    "volumeup/volup", "volumedown/voldown", "mute", "play/playpause", "next", "prev",
    "stop", "brightup", "brightdown", "calc", "screenlock/lock", "favorites",
    "webhome", "back", "forward",
];

/// Reverse lookup: keyboard-page usage code -> human name (best effort).
pub fn keyboard_code_name(code: u8) -> String {
    match code {
        0x04..=0x1d => ((b'a' + (code - 0x04)) as char).to_string(),
        0x1e..=0x26 => ((b'1' + (code - 0x1e)) as char).to_string(),
        0x27 => "0".to_string(),
        0x3a..=0x45 => format!("f{}", code - 0x39),
        0x28 => "enter".into(),
        0x29 => "esc".into(),
        0x2a => "backspace".into(),
        0x2b => "tab".into(),
        0x2c => "space".into(),
        0x4f => "right".into(),
        0x50 => "left".into(),
        0x51 => "down".into(),
        0x52 => "up".into(),
        0x4a => "home".into(),
        0x4d => "end".into(),
        0x4b => "pageup".into(),
        0x4e => "pagedown".into(),
        0x49 => "insert".into(),
        0x4c => "delete".into(),
        _ => format!("<{code}>"),
    }
}

/// Reverse lookup: consumer-page usage code -> media name (best effort).
pub fn media_code_name(code: u16) -> String {
    match code {
        0x6f => "brightup".into(),
        0x70 => "brightdown".into(),
        0xb5 => "next".into(),
        0xb6 => "prev".into(),
        0xb7 => "stop".into(),
        0xcd => "playpause".into(),
        0xe2 => "mute".into(),
        0xe9 => "volumeup".into(),
        0xea => "volumedown".into(),
        0x182 => "favorites".into(),
        0x192 => "calc".into(),
        0x19e => "screenlock".into(),
        0x223 => "webhome".into(),
        0x224 => "back".into(),
        0x225 => "forward".into(),
        0 => "release".into(),
        _ => format!("<0x{code:04x}>"),
    }
}

/// Decode a standard HID keyboard modifier byte into names.
pub fn modifier_byte_names(bits: u8) -> Vec<&'static str> {
    const NAMES: [(u8, &str); 8] = [
        (0x01, "ctrl"),
        (0x02, "shift"),
        (0x04, "alt"),
        (0x08, "cmd"),
        (0x10, "rctrl"),
        (0x20, "rshift"),
        (0x40, "ralt"),
        (0x80, "rcmd"),
    ];
    NAMES.iter().filter(|(b, _)| bits & b != 0).map(|(_, n)| *n).collect()
}

/// Parse a mouse spec: `click(left)`, `click(left+right)`, `wheel(1)`, `wheel(-1)`, `move(dx,dy)`.
fn parse_mouse(spec: &str) -> Result<Option<[u8; 5]>> {
    let open = match spec.find('(') {
        Some(i) => i,
        None => return Ok(None),
    };
    if !spec.ends_with(')') {
        bail!("malformed mouse action `{spec}` (missing `)`)");
    }
    let verb = &spec[..open];
    let arg = &spec[open + 1..spec.len() - 1];

    // Layout: [modifier, buttons, dx, dy, wheel]
    let mut data = [0u8; 5];
    match verb {
        "click" => {
            let mut bits = 0u8;
            for b in arg.split('+') {
                bits |= match b.trim() {
                    "left" | "l" => 1,
                    "right" | "r" => 2,
                    "middle" | "m" => 4,
                    other => bail!("unknown mouse button `{other}`"),
                };
            }
            if bits == 0 {
                bail!("click() needs at least one button");
            }
            data[1] = bits;
        }
        "wheel" => {
            let d: i8 = arg.trim().parse().map_err(|_| anyhow::anyhow!("wheel() needs an integer -128..127"))?;
            data[4] = d as u8;
        }
        "move" => {
            let (x, y) = arg.split_once(',').ok_or_else(|| anyhow::anyhow!("move() needs `dx,dy`"))?;
            data[2] = x.trim().parse::<i8>().map_err(|_| anyhow::anyhow!("bad dx"))? as u8;
            data[3] = y.trim().parse::<i8>().map_err(|_| anyhow::anyhow!("bad dy"))? as u8;
        }
        other => bail!("unknown mouse action `{other}` (use click/wheel/move)"),
    }
    Ok(Some(data))
}

/// Parse a full macro string into a [`Macro`].
pub fn parse_macro(spec: &str) -> Result<Macro> {
    let spec = spec.trim();
    if spec.is_empty() {
        bail!("empty macro");
    }

    // Mouse?
    if let Some(data) = parse_mouse(spec)? {
        return Ok(Macro::Mouse(data));
    }

    // Single media key? (only when the whole spec is one token with no modifiers)
    if !spec.contains([',', '-']) {
        if let Some(code) = media_code(&spec.to_ascii_lowercase()) {
            return Ok(Macro::Media(code));
        }
    }

    // Otherwise a keyboard sequence: comma-separated accords.
    let mut accords = Vec::new();
    for chunk in spec.split(',') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            bail!("empty accord in `{spec}`");
        }
        accords.push(parse_accord(chunk)?);
    }
    Ok(Macro::Keyboard(accords))
}

fn parse_accord(chunk: &str) -> Result<Accord> {
    // Custom raw code `<NN>`.
    if let Some(inner) = chunk.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
        let code: u8 = inner.parse().map_err(|_| anyhow::anyhow!("bad raw code `{chunk}`"))?;
        return Ok(Accord { modifier_codes: vec![], key_code: Some(code) });
    }

    let mut modifier_codes = Vec::new();
    let mut key: Option<u8> = None;
    for token in chunk.split('-') {
        let t = token.trim().to_ascii_lowercase();
        if t.is_empty() {
            bail!("empty token in `{chunk}`");
        }
        if let Some(m) = modifier_code(&t) {
            modifier_codes.push(m);
        } else if let Some(k) = key_code(&t) {
            if key.is_some() {
                bail!("more than one non-modifier key in accord `{chunk}`");
            }
            key = Some(k);
        } else if let Some(inner) = t.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
            key = Some(inner.parse().map_err(|_| anyhow::anyhow!("bad raw code `{token}`"))?);
        } else {
            bail!("unknown key or modifier `{token}` (try `knobctl keys`)");
        }
    }
    if modifier_codes.is_empty() && key.is_none() {
        bail!("accord `{chunk}` has no keys");
    }
    Ok(Accord { modifier_codes, key_code: key })
}
