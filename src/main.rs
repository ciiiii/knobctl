mod config;
mod device;
mod keys;
mod proto;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use crate::config::Mapping;
use crate::device::{
    list_interfaces, open_inputs, Device, Selector, DEFAULT_PID, DEFAULT_VID, VENDOR_USAGE_PAGE,
};
use crate::keys::{
    keyboard_code_name, media_code_name, modifier_byte_names, parse_macro, KEY_NAMES, MEDIA_NAMES,
    MODIFIER_NAMES,
};
use crate::proto::{build_binding, KnobAction, Target, MSG_LEN};

/// Program shortcut mappings on a CH57x BLE macro knob (`514c:8850`).
#[derive(Parser)]
#[command(name = "knobctl", version, about)]
struct Cli {
    /// USB vendor id (hex ok: 0x514c).
    #[arg(long, value_parser = parse_u16, default_value_t = DEFAULT_VID, global = true)]
    vid: u16,
    /// USB product id (hex ok: 0x8850).
    #[arg(long, value_parser = parse_u16, default_value_t = DEFAULT_PID, global = true)]
    pid: u16,
    /// HID usage page of the config interface (hex ok).
    #[arg(long, value_parser = parse_u16, default_value_t = VENDOR_USAGE_PAGE, global = true)]
    usage_page: u16,
    /// Print the protocol bytes instead of writing to the device.
    #[arg(long, global = true)]
    dry_run: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the device's HID interfaces (find the vendor config one).
    Probe,
    /// Show supported key, modifier and media names.
    Keys,
    /// Watch what the knob emits live (its *current* mapping). Ctrl-C to stop.
    Watch,
    /// Read the bindings currently stored on the device (layer 0 by default).
    Read {
        /// Show every slot, including phantom defaults (ids 1..15) and unidentified ids (22..24).
        #[arg(long)]
        all: bool,
        /// Read all layers, not just layer 0.
        #[arg(long)]
        layers: bool,
    },
    /// Set the RGB/backlight effect mode (0..15; only 0..5 are real effects, 6+ = no effect).
    Rgb {
        /// Effect mode index.
        mode: u8,
        /// Layer index (0..15).
        #[arg(long, default_value_t = 0)]
        layer: u8,
    },
    /// Set the mouse report latency in milliseconds (write-only; can't be read back).
    Latency {
        /// Latency in milliseconds (0..65535).
        ms: u16,
        /// Layer index (0..15).
        #[arg(long, default_value_t = 0)]
        layer: u8,
    },
    /// Validate a mapping file without touching the device.
    Validate {
        /// Path to the YAML mapping file.
        file: String,
    },
    /// Program one action, e.g. `knobctl set cw volumeup` or `knobctl set knob0.press mute`.
    Set {
        /// Target: cw | ccw | press | press+cw | press+ccw | knob<N>.<action> | button<N>.
        target: String,
        /// Macro: `ctrl-c`, `a,b,c`, `volumeup`, `click(left)`, `wheel(1)`.
        macro_spec: String,
        /// Layer index (0..15).
        #[arg(long, default_value_t = 0)]
        layer: u8,
    },
    /// Program every action from a YAML mapping file.
    Apply {
        /// Path to the YAML mapping file.
        file: String,
    },
}

fn parse_u16(s: &str) -> Result<u16, std::num::ParseIntError> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u16::from_str_radix(hex, 16)
    } else {
        s.parse()
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let selector = Selector {
        vid: cli.vid,
        pid: cli.pid,
        usage_page: Some(cli.usage_page),
    };

    match &cli.command {
        Command::Probe => cmd_probe(&selector),
        Command::Keys => {
            cmd_keys();
            Ok(())
        }
        Command::Watch => cmd_watch(&selector),
        Command::Read { all, layers } => cmd_read(&selector, *all, *layers),
        Command::Rgb { mode, layer } => cmd_rgb(&selector, cli.dry_run, *mode, *layer),
        Command::Latency { ms, layer } => cmd_latency(&selector, cli.dry_run, *ms, *layer),
        Command::Validate { file } => cmd_validate(file),
        Command::Set { target, macro_spec, layer } => {
            cmd_set(&selector, cli.dry_run, target, macro_spec, *layer)
        }
        Command::Apply { file } => cmd_apply(&selector, cli.dry_run, file),
    }
}

fn cmd_probe(sel: &Selector) -> Result<()> {
    let ifaces = list_interfaces(sel)?;
    if ifaces.is_empty() {
        bail!(
            "no HID interface for {:04x}:{:04x}. Plug the knob in over USB (BLE can't be programmed).",
            sel.vid, sel.pid
        );
    }
    println!("Interfaces for {:04x}:{:04x}:", sel.vid, sel.pid);
    for i in &ifaces {
        let mark = if i.usage_page == VENDOR_USAGE_PAGE { "  <- config channel" } else { "" };
        println!(
            "  iface #{:<2} usage_page={:#06x} usage={:#04x}  {} / {}{}",
            i.interface_number, i.usage_page, i.usage, i.manufacturer, i.product, mark
        );
    }
    Ok(())
}

fn cmd_keys() {
    println!("Modifiers (join with '-'):");
    for m in MODIFIER_NAMES {
        println!("  {m}");
    }
    println!("\nKeys:");
    for k in KEY_NAMES {
        println!("  {k}");
    }
    println!("\nMedia keys (use alone):");
    for m in MEDIA_NAMES {
        println!("  {m}");
    }
    println!("\nMouse: click(left|right|middle[+..])  wheel(N)  move(dx,dy)");
    println!("Raw keyboard code: <NN> (decimal)");
    println!("\nExamples: ctrl-c   ctrl-shift-t   a,b,c   volumeup   click(left)   wheel(1)");
}

fn cmd_watch(sel: &Selector) -> Result<()> {
    let inputs = open_inputs(sel)?;
    if inputs.is_empty() {
        bail!(
            "could not open any input interface for {:04x}:{:04x}.\n\
             If you saw `privilege violation` above, macOS is blocking access to the knob's \
             keyboard/mouse/media reports. Grant your terminal app Input Monitoring:\n  \
             System Settings > Privacy & Security > Input Monitoring > enable your terminal,\n\
             then run `knobctl watch` again from that terminal.\n\
             (Otherwise: make sure the knob is plugged in over USB.)",
            sel.vid, sel.pid
        );
    }
    eprintln!(
        "Watching {} interface(s). Turn and press the knob to see its current mapping.",
        inputs.len()
    );
    eprintln!("(macOS: if nothing appears, grant your terminal Input Monitoring in System Settings > Privacy.)");
    eprintln!("Ctrl-C to stop.\n");

    let mut buf = [0u8; 64];
    loop {
        let mut idle = true;
        for iface in &inputs {
            let n = iface.read(&mut buf)?;
            if n == 0 {
                continue;
            }
            idle = false;
            let report = &buf[..n];
            let end = report.iter().rposition(|&b| b != 0).map(|p| p + 1).unwrap_or(0);
            if end <= 1 {
                continue; // all-zero, or only the report-id byte set = release, skip the noise
            }
            let hex: Vec<String> = report[..end].iter().map(|b| format!("{b:02x}")).collect();
            let decoded = decode_report(iface.usage_page, iface.usage, report);
            println!("[{:<14}] {:<28} {}", iface.label, decoded, hex.join(" "));
        }
        if idle {
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    }
}

/// Decode an input report into a human-readable action. This device sends **numbered
/// reports** on the keyboard interface — byte 0 is the report id, which selects the
/// format: 1 = keyboard, 2 = mouse, 5 = consumer/media.
fn decode_report(_usage_page: u16, _usage: u16, report: &[u8]) -> String {
    let at = |i: usize| report.get(i).copied().unwrap_or(0);
    match at(0) {
        0x01 => {
            // keyboard: [id, modifiers, reserved, k1..k6]
            let mut parts: Vec<String> =
                modifier_byte_names(at(1)).into_iter().map(str::to_string).collect();
            parts.extend(
                report
                    .get(3..)
                    .unwrap_or(&[])
                    .iter()
                    .filter(|&&c| (0x04..=0xa4).contains(&c))
                    .map(|&c| keyboard_code_name(c)),
            );
            if parts.is_empty() {
                "key: (release)".into()
            } else {
                format!("key: {}", parts.join("-"))
            }
        }
        0x02 => {
            // mouse: [id, buttons, dx, dy, wheel]
            let (buttons, dx, dy, wheel) = (at(1), at(2) as i8, at(3) as i8, at(4) as i8);
            if wheel != 0 {
                format!("mouse: wheel({wheel})")
            } else if dx != 0 || dy != 0 {
                format!("mouse: move({dx},{dy})")
            } else if buttons != 0 {
                format!("mouse: buttons={buttons:#04x}")
            } else {
                "mouse: (release)".into()
            }
        }
        0x05 | 0x0c => {
            // consumer/media: [id, code_lo, code_hi]
            let code = u16::from_le_bytes([at(1), at(2)]);
            if code == 0 {
                "media: (release)".into()
            } else {
                format!("media: {}", media_code_name(code))
            }
        }
        _ => "raw".into(),
    }
}

fn cmd_rgb(sel: &Selector, dry_run: bool, mode: u8, layer: u8) -> Result<()> {
    let messages = proto::build_rgb_mode(layer, mode)?;
    if dry_run {
        print_messages(&format!("rgb mode {mode} (layer {layer})"), &messages);
        return Ok(());
    }
    let dev = Device::open(sel)?;
    dev.send_all(&messages)?;
    println!("set rgb mode {mode} (layer {layer})");
    Ok(())
}

fn cmd_latency(sel: &Selector, dry_run: bool, ms: u16, layer: u8) -> Result<()> {
    let messages = proto::build_latency(layer, ms)?;
    if dry_run {
        print_messages(&format!("latency {ms}ms (layer {layer})"), &messages);
        return Ok(());
    }
    let dev = Device::open(sel)?;
    dev.send_all(&messages)?;
    println!("set mouse latency {ms}ms (layer {layer})");
    Ok(())
}

fn cmd_read(sel: &Selector, all: bool, layers: bool) -> Result<()> {
    let dev = Device::open(sel)?; // sends the init handshake (03 fb fb fb 01)

    // (layer, key_id) -> description, deduped in case a layer echoes twice.
    let mut rows: Vec<(u8, u8, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut buf = [0u8; 64];

    let last_layer = if layers { proto::LAYERS } else { 1 };
    for layer in 0..last_layer {
        dev.send(&proto::read_layer_query(layer))?;
        let mut idle = 0;
        loop {
            let n = dev.read(&mut buf, 100)?;
            if n == 0 {
                idle += 1;
                if idle >= 3 {
                    break; // ~300ms of silence: this layer is done
                }
                continue;
            }
            idle = 0;
            let r = &buf[..n];
            if r.len() < 13 || r[0] != proto::REPORT_ID || r[1] != proto::READ_CMD {
                continue; // not a read-back report (e.g. the fb ack)
            }
            let (key_id, rep_layer, kind, count) = (r[2], r[3], r[4], r[10] as usize);
            if seen.insert((rep_layer, key_id)) {
                rows.push((rep_layer, key_id, decode_stored(kind, count, &r[11..])));
            }
        }
    }

    if rows.is_empty() {
        bail!("no bindings read back from {:04x}:{:04x} (device may not support read)", sel.vid, sel.pid);
    }
    rows.sort_by_key(|(layer, key_id, _)| (*layer, *key_id));

    let mut cur_layer = u8::MAX;
    for (layer, key_id, desc) in &rows {
        if !all && !(16..=21).contains(key_id) {
            continue; // default: only the known slots (knob + press-combos, ids 16..21)
        }
        if layers && *layer != cur_layer {
            cur_layer = *layer;
            println!("layer {}:", layer.saturating_sub(1)); // reported layer is 1-based
        }
        println!("  {:<11} (id {:>2})  {}", proto::key_id_name(*key_id), key_id, desc);
    }
    Ok(())
}

/// Decode a stored binding payload (bytes after the count) into a human string.
fn decode_stored(kind: u8, count: usize, payload: &[u8]) -> String {
    match kind {
        1 => {
            // keyboard: `count` entries of [modifier_bitmask, keycode]
            let mut accords = Vec::new();
            for i in 0..count {
                let m = payload.get(2 * i).copied().unwrap_or(0);
                let c = payload.get(2 * i + 1).copied().unwrap_or(0);
                if m == 0 && c == 0 {
                    continue;
                }
                let mut parts: Vec<String> =
                    modifier_byte_names(m).into_iter().map(str::to_string).collect();
                if c != 0 {
                    parts.push(keyboard_code_name(c));
                }
                accords.push(parts.join("-"));
            }
            if accords.is_empty() {
                "(unset)".into()
            } else {
                format!("key: {}", accords.join(","))
            }
        }
        2 => {
            let code = u16::from_le_bytes([
                payload.first().copied().unwrap_or(0),
                payload.get(1).copied().unwrap_or(0),
            ]);
            if code == 0 {
                "(unset)".into()
            } else {
                format!("media: {}", media_code_name(code))
            }
        }
        3 => {
            // mouse: [modifier, buttons, dx, dy, wheel]
            let b = |i: usize| payload.get(i).copied().unwrap_or(0);
            let (buttons, dx, dy, wheel) = (b(1), b(2) as i8, b(3) as i8, b(4) as i8);
            if wheel != 0 {
                format!("mouse: wheel({wheel})")
            } else if dx != 0 || dy != 0 {
                format!("mouse: move({dx},{dy})")
            } else if buttons != 0 {
                let mut names = Vec::new();
                if buttons & 1 != 0 { names.push("left"); }
                if buttons & 2 != 0 { names.push("right"); }
                if buttons & 4 != 0 { names.push("middle"); }
                format!("mouse: click({})", names.join("+"))
            } else {
                "(unset)".into()
            }
        }
        other => format!("kind {other} (raw)"),
    }
}

fn cmd_validate(file: &str) -> Result<()> {
    let mapping = Mapping::load(file)?;
    let bindings = mapping.render()?;
    println!("mapping OK: {} binding(s) on layer {}", bindings.len(), mapping.layer);
    for b in &bindings {
        println!("  {}", b.describe);
    }
    Ok(())
}

fn cmd_set(sel: &Selector, dry_run: bool, target: &str, macro_spec: &str, layer: u8) -> Result<()> {
    let target = parse_target(target)?;
    let mac = parse_macro(macro_spec).context("parse macro")?;
    let messages = build_binding(layer, target, &mac)?;

    if dry_run {
        print_messages(&format!("{target:?} -> {macro_spec}"), &messages);
        return Ok(());
    }
    let dev = Device::open(sel)?;
    dev.send_all(&messages)?;
    println!("programmed {target:?} -> {macro_spec} (layer {layer})");
    Ok(())
}

fn cmd_apply(sel: &Selector, dry_run: bool, file: &str) -> Result<()> {
    let mapping = Mapping::load(file)?;
    let bindings = mapping.render()?;
    if bindings.is_empty() {
        bail!("mapping `{file}` has no bindings");
    }

    if dry_run {
        for b in &bindings {
            print_messages(&b.describe, &b.messages);
        }
        return Ok(());
    }

    let dev = Device::open(sel)?;
    for b in &bindings {
        dev.send_all(&b.messages)?;
        println!("  {}", b.describe);
    }
    println!("applied {} binding(s) from {file}", bindings.len());
    Ok(())
}

fn print_messages(label: &str, messages: &[[u8; MSG_LEN]]) {
    println!("{label}");
    for m in messages {
        // Trim trailing zeros for readability.
        let end = m.iter().rposition(|&b| b != 0).map(|p| p + 1).unwrap_or(0);
        let mut hex = vec![format!("{:02x}", crate::proto::REPORT_ID)];
        hex.extend(m[..end].iter().map(|b| format!("{b:02x}")));
        println!("  send: {}", hex.join(" "));
    }
}

/// Parse a `set` target: `cw`/`ccw`/`press` (knob 0), the `press+ccw`/`press+cw`
/// press-held combos (firmware "knob 1"), `knob<N>.<action>`, or `button<N>`.
fn parse_target(s: &str) -> Result<Target> {
    let s = s.trim().to_ascii_lowercase();
    let knob_action = |a: &str| -> Result<KnobAction> {
        Ok(match a {
            "ccw" | "left" | "ccw_rotate" => KnobAction::Ccw,
            "cw" | "right" | "cw_rotate" => KnobAction::Cw,
            "press" | "click" | "push" => KnobAction::Press,
            other => bail!("unknown knob action `{other}` (use ccw|cw|press)"),
        })
    };

    // Press-held combos map to the firmware's "virtual knob 1" (ids 19/20/21).
    match s.as_str() {
        "press+ccw" | "press-ccw" => return Ok(Target::Knob(1, KnobAction::Ccw)),
        "press+press" | "press-press" | "press+click" => return Ok(Target::Knob(1, KnobAction::Press)),
        "press+cw" | "press-cw" => return Ok(Target::Knob(1, KnobAction::Cw)),
        _ => {}
    }

    if let Ok(action) = knob_action(&s) {
        return Ok(Target::Knob(0, action));
    }
    if let Some(rest) = s.strip_prefix("knob") {
        let (idx, action) = rest
            .split_once('.')
            .context("knob target must be knob<N>.<ccw|cw|press>")?;
        let n: u8 = idx.parse().context("bad knob index")?;
        return Ok(Target::Knob(n, knob_action(action)?));
    }
    for pfx in ["button", "btn", "b", "key"] {
        if let Some(rest) = s.strip_prefix(pfx) {
            if let Ok(n) = rest.parse::<u8>() {
                return Ok(Target::Button(n));
            }
        }
    }
    bail!("unknown target `{s}` (try cw|ccw|press, press+cw|press+ccw, knob0.cw, or button0)")
}
