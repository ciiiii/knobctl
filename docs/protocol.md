# Protocol

The exact HID wire protocol knobctl speaks to the **CH57x knob (`514c:8850`)**.
It was reverse-engineered by capturing the vendor app's live `libhidapi` traffic
(see [Reverse-engineering](#reverse-engineering) below); every layout here is verified
against that capture, not inferred. It differs from the
[`ch57x-keyboard-tool`](https://github.com/kriomant/ch57x-keyboard-tool)
`Keyboard8850_4x4` format that an earlier version of knobctl assumed.

## Transport

- Programming happens over the **vendor HID interface**: usage page `0xff00`,
  usage `0x01`. `knobctl probe` marks it as the *config channel*.
- **USB only.** The mapping can't be written over Bluetooth (the knob is used
  wirelessly afterwards).
- Every message is a **65-byte HID buffer**: report id `0x03` in byte 0, then a
  64-byte payload. All byte offsets below are into this 65-byte buffer.
- Right after opening the interface, one **init** message is sent: `03 fb fb fb 01`
  (rest zero). The device replies `03 fb 00 01`.
- The device stores **3 layers** (`layer` 0..2 in the CLI, sent as `layer+1`).

## Message shapes

| Purpose      | Bytes                                   |
| ------------ | --------------------------------------- |
| init         | `03 fb fb fb 01`                        |
| bind         | `03 fd <key_id> <layer+1> <kind> …`     |
| rgb mode     | `03 fe b0 <layer+1> 08 … <0x50+mode>`   |
| latency      | `03 fd 00 <layer+1> 05 <ms16 LE>`       |
| finish       | `03 fd fe ff` (after every write)       |
| read query   | `03 fa 0f 03 <layer+1>`                 |
| read reply   | `03 fa <key_id> <layer+1> <kind> …`     |

## Binding (command `fd`)

Two messages per binding: the **bind** message, then the **finish** `03 fd fe ff`.

```
byte 0 : 0x03            report id
byte 1 : 0xfd            command
byte 2 : key_id          which button / knob-action (see below)
byte 3 : layer + 1       layer 0..2  ->  1..3
byte 4 : kind            1=keyboard  2=media  3=mouse
byte 5..9 : 00 00 00 00 00
byte 10: count           number of payload entries
byte 11+: payload        depends on kind
```

### key_id

| Target             | key_id                    |
| ------------------ | ------------------------- |
| Button *n* (0..15) | `n + 1`                   |
| Knob *n*, ccw      | `16 + 3*n + 0`            |
| Knob *n*, press    | `16 + 3*n + 1`            |
| Knob *n*, cw       | `16 + 3*n + 2`            |

For the single knob (n=0): **ccw=`16` (0x10), press=`17` (0x11), cw=`18` (0x12)**.
Key ids `19..24` (0x13..0x18) exist as **press+rotate combo** slots (seen in read-back
but not exposed as `set` targets). Ids `1..15` are phantom keyboard defaults on this
knob-only unit.

### kind 1 — keyboard

`count` = number of keys; each entry is `<modifier_bitmask> <keycode>` (2 bytes).

```
byte 10: count
then, per entry (2 bytes): <mod> <code>
```

Modifier bitmask (standard HID): ctrl `0x01`, shift `0x02`, alt `0x04`, cmd `0x08`,
rctrl `0x10`, rshift `0x20`, ralt `0x40`, rcmd `0x80`.

```
enter (press, layer 0):   03 fd 11 01 01 00 00 00 00 00 01 00 28
ctrl-c:                    03 fd 11 01 01 00 00 00 00 00 01 01 06
a,b,c (sequence):          03 fd 11 01 01 00 00 00 00 00 03 00 04 00 05 00 06
```

### kind 2 — media (consumer page)

`count` is `0x02`; the payload is the 16-bit consumer usage code, little-endian.

```
volumeup (0x00e9), cw:     03 fd 12 01 02 00 00 00 00 00 02 e9 00
```

Common codes: mute `0xe2`, volumeup `0xe9`, volumedown `0xea`, play/pause `0xcd`,
next `0xb5`, prev `0xb6`, brightup `0x6f`, brightdown `0x70`.

### kind 3 — mouse

`count` is `0x01`; the payload is 5 bytes `<modifier> <buttons> <dx> <dy> <wheel>`
(buttons bitmap: left=1 right=2 middle=4; dx/dy/wheel signed).

```
click(left):               03 fd 11 01 03 00 00 00 00 00 01 00 01
wheel(1):                  03 fd 11 01 03 00 00 00 00 00 01 00 00 00 00 01
wheel(-1):                 03 fd 11 01 03 00 00 00 00 00 01 00 00 00 00 ff
```

## RGB effect mode (command `fe`)

```
mode N:  03 fe b0 <layer+1> 08 00 00 00 00 00 01 00 <0x50 + N>   + finish
```

Target `0xb0`, kind `0x08`. Modes 0..5 are the distinct effects; mode 6 was tested
and produces **no effect** (the table tops out at 5). **Write-only** — never reported back.

## Mouse latency (command `fd`, kind `05`)

A binding-style write on key_id 0, kind `05`, with the latency as a 16-bit
little-endian millisecond value:

```
100 ms:   03 fd 00 01 05 64 00     + finish
1000 ms:  03 fd 00 01 05 e8 03     + finish
```

**Write-only** — not reported back.

## Reading the stored config (command `fa`)

Send `03 fa 0f 03 <layer+1>`; the device streams one reply per key slot, using the
**same layout as a bind message** but with command `fa`:

```
03 fa <key_id> <layer+1> <kind> 00 00 00 00 00 <count> <payload>
```

Iterate layers 1..3 to read everything. `knobctl read` does this and decodes each
reply. Only key bindings are reported — **RGB and latency are not** (write-only).

## Reverse-engineering

The protocol was captured by interposing `hid_write` / `hid_read` in the vendor's macOS
configuration app (unsigned, and dynamically links `libhidapi` — which is what makes
interposing work). `hidhook.c` builds a `DYLD_INSERT_LIBRARIES` shim that logs every
buffer to `/tmp/hidhook.log`; `make capture APP=/path/to/Vendor.app/Contents/MacOS/Vendor`
launches the app through it. Reprogramming a known action in the app then reveals the
exact bytes for that action. The vendor app is not distributed here — point `APP` at
your own copy.
