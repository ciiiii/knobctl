# knobctl

[![CI](https://github.com/ciiiii/knobctl/actions/workflows/ci.yml/badge.svg)](https://github.com/ciiiii/knobctl/actions/workflows/ci.yml)

> Program **CH57x BLE macro knobs** (`514c:8850`) from the command line.

A small CLI for the WCH CH57x rotary-encoder macro knobs — USB `514c:8850` and
relatives, sold under a handful of brand names — for when the vendor's configuration
software won't work.

Developed and tested on macOS against a single-knob unit; there's no platform-specific
code, so it should work anywhere `hidapi` does.

It speaks the device's vendor HID protocol directly, reverse-engineered by capturing
the vendor app's own traffic (see [docs/protocol.md](docs/protocol.md)).
**Programming must be done over USB** — the mapping can't be written over Bluetooth,
though the knob is used wirelessly afterwards.

## Docs

- [docs/protocol.md](docs/protocol.md) — the exact HID wire protocol (messages, key
  ids, kinds, byte layouts, and how it was captured).
- [docs/device-514c-8850.md](docs/device-514c-8850.md) — this device's
  identity, HID interfaces, confirmed knob key-ids, combo slots, and layers.
- [docs/reading-the-current-mapping.md](docs/reading-the-current-mapping.md) —
  `knobctl read`, `knobctl watch`, what can't be read, and the macOS Input Monitoring caveat.
- [docs/releasing.md](docs/releasing.md) — how a release is cut and what it publishes.

## Install

Grab a prebuilt archive from the [latest release](https://github.com/ciiiii/knobctl/releases/latest)
— macOS (Apple Silicon / Intel) and Linux (x86_64 / arm64):

```sh
tar -xzf knobctl-*-aarch64-apple-darwin.tar.gz
cd knobctl-*-aarch64-apple-darwin

# macOS only: the binary is unsigned, so clear the download quarantine flag.
xattr -d com.apple.quarantine ./knobctl

./knobctl probe
```

Each archive ships a `.sha256` next to it — `shasum -a 256 -c knobctl-*.tar.gz.sha256`.

Or install from source with cargo:

```sh
cargo install --git https://github.com/ciiiii/knobctl
```

## Build

```sh
cargo build --release     # binary: target/release/knobctl
# or:
make build
make dist                 # package a release archive for this host
```

On Linux, `hidapi` needs libudev headers: `sudo apt install pkg-config libudev-dev`.

## Usage

```sh
knobctl probe                        # list HID interfaces, find the config channel
knobctl keys                         # list supported keys / modifiers / media names
knobctl read                         # dump the bindings stored on the device (layer 0)
knobctl watch                        # see what the knob emits now (macOS: needs Input Monitoring)

# Program one action (knob 0 is the default):
knobctl set cw volumeup              # rotate right
knobctl set ccw volumedown           # rotate left
knobctl set press enter              # push the knob (e.g. approve Claude Code prompts)
knobctl set knob0.cw next            # explicit knob index

# Program everything from a file:
knobctl apply mapping.yaml
knobctl validate mapping.yaml        # check a file without a device

# Global settings (write-only — can't be read back):
knobctl rgb 3                        # RGB backlight effect mode (0..5), if the unit has LEDs
knobctl latency 100                  # mouse report latency in ms
```

Add `--dry-run` to any write command to print the exact protocol bytes instead of
writing to the device. `--layer N` (0..2) targets a non-default layer.

## Makefile shortcuts

```sh
make read            # dump device config
make approve         # bind press -> Enter
make volume          # cw=volup, ccw=voldown
make rgb MODE=3      # RGB effect mode
make latency MS=100  # mouse latency
make watch           # live knob output (sudo)
make capture APP=... # launch your copy of the vendor app through the HID-capture hook
make dist            # package a release archive for this host
make help            # list all targets
```

## Macro syntax

| Form           | Example                       |
| -------------- | ----------------------------- |
| Key            | `a`, `enter`, `f5`            |
| Chord          | `ctrl-c`, `ctrl-shift-t`      |
| Sequence       | `a,b,c` (comma = press order) |
| Media key      | `volumeup`, `mute`, `next`    |
| Mouse          | `click(left)`, `wheel(1)`, `move(dx,dy)` |
| Raw HID code   | `<41>` (decimal)              |

Run `knobctl keys` for the full list.

## Mapping file

```yaml
layer: 0            # optional (0..2), default 0
knobs:
  - ccw: volumedown
    press: enter
    cw: volumeup
```

## Overrides

`--vid`, `--pid`, `--usage-page` (hex ok, e.g. `0x514c`) let you target a different
CH57x variant. Use `knobctl probe` to find the right interface if the defaults miss.

## Protocol

Each binding is two HID reports (report id `0x03`) written to the vendor interface
(usage page `0xff00`); the device stores 3 layers and reports bindings back via a read
query. Full byte-level details — plus RGB and latency — are in
[docs/protocol.md](docs/protocol.md).

## What's verified vs. not

- **Verified** (matched byte-for-byte to vendor traffic, most confirmed by read-back):
  keyboard / modifier / macro / media / mouse bindings, the knob key-ids
  (ccw=16, press=17, cw=18), `read`, `rgb`, `latency`.
- **Not confirmed:** which gesture drives combo ids 19..24, and how the active layer is
  switched at runtime. See [docs/device-514c-8850.md](docs/device-514c-8850.md).

## Limitations

- **USB required for programming** — Bluetooth is for using the knob, not configuring it.
- **RGB and latency are write-only** — the device doesn't report them back. Not every
  unit in this family has a backlight; the firmware accepts `rgb` either way, so on a
  knob without LEDs the command is a silent no-op.
- **macOS** — `watch` needs Input Monitoring; grant it to your terminal app (the
  unsigned binary's identity changes each rebuild). Programming does not.
