# Reading the current mapping

## `knobctl read` — dump the stored bindings

The device **can** report its stored key bindings. `knobctl read` queries every layer
and decodes each slot:

```sh
knobctl read           # known slots on layer 0 (knob + press-combos, ids 16..21)
knobctl read --all     # every slot, incl. phantom (1..15) and unidentified (22..24)
knobctl read --layers  # all 3 layers (layer 0 is the active one)
```

```
ccw         (id 16)  media: volumedown
press       (id 17)  key: enter
cw          (id 18)  media: volumeup
press+ccw   (id 19)  (unset)
press+press (id 20)  key: 5
press+cw    (id 21)  media: brightup
```

This is a real device read (via the `fa` query, see [`protocol.md`](./protocol.md)),
not a guess — use it to confirm what's actually programmed.

### What can't be read

- **RGB mode** (`knobctl rgb`) and **mouse latency** (`knobctl latency`) are
  **write-only** — the firmware never reports them back. Keep track of these yourself.
- Which physical gesture maps to combo ids 19..24 isn't reported (only the value is);
  see [`device-514c-8850.md`](./device-514c-8850.md).

## `knobctl watch` — observe what the knob emits live

Complementary to `read`: watch the input reports the knob sends when you actuate it.
Useful for identifying the combo gestures, or confirming a binding behaves as expected.

```sh
knobctl watch      # turn / press the knob; Ctrl-C to stop
```

```
[keyboard      ] key: enter            01 00 00 28
[keyboard      ] media: volumedown     05 ea
[keyboard      ] media: brightup       05 6f
```

The knob multiplexes keyboard / media / mouse on the keyboard interface via **report
ids** (byte 0: 1=keyboard, 2=mouse, 5=consumer), so everything is labelled
`[keyboard]`; `watch` decodes by report id and skips release reports.

### macOS: Input Monitoring permission

The knob's keyboard / mouse / consumer interfaces are **protected** HID usages on
macOS. Opening them fails with a `privilege violation` (`0xE00002C1`) unless the
terminal you run knobctl from has **Input Monitoring** permission:

> System Settings › Privacy & Security › Input Monitoring → enable your terminal
> (Ghostty, Terminal, iTerm…), then run `knobctl watch` from that terminal.

Because the binary is unsigned, its identity changes on every rebuild — grant the
permission to the **terminal app** (which is stable), or `sudo knobctl watch`.

The *config channel* (`0xff00`) is **not** protected, so `set` / `apply` / `read` /
`rgb` / `latency` all work without this permission — only `watch` needs it.
