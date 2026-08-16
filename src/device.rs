//! Device discovery and HID transport (macOS-friendly, via hidapi).
//!
//! The knob exposes several HID interfaces; the programmable config channel is the
//! vendor-defined one (usage page 0xFF00). We open that interface and write the
//! 64-byte protocol messages as output reports (report id 0x00 prepended).

use anyhow::{anyhow, bail, Context, Result};
use hidapi::{HidApi, HidDevice};

use crate::proto::MSG_LEN;

pub const DEFAULT_VID: u16 = 0x514c;
pub const DEFAULT_PID: u16 = 0x8850;
pub const VENDOR_USAGE_PAGE: u16 = 0xff00;

pub struct Selector {
    pub vid: u16,
    pub pid: u16,
    /// Vendor usage page to match; `None` disables the filter (picks first interface).
    pub usage_page: Option<u16>,
}

impl Default for Selector {
    fn default() -> Self {
        Self { vid: DEFAULT_VID, pid: DEFAULT_PID, usage_page: Some(VENDOR_USAGE_PAGE) }
    }
}

pub struct Interface {
    pub usage_page: u16,
    pub usage: u16,
    pub interface_number: i32,
    pub manufacturer: String,
    pub product: String,
}

/// List every HID interface that matches vid/pid (ignores the usage-page filter).
pub fn list_interfaces(sel: &Selector) -> Result<Vec<Interface>> {
    let api = HidApi::new().context("init hidapi")?;
    let mut out = Vec::new();
    for d in api.device_list() {
        if d.vendor_id() != sel.vid || d.product_id() != sel.pid {
            continue;
        }
        out.push(Interface {
            usage_page: d.usage_page(),
            usage: d.usage(),
            interface_number: d.interface_number(),
            manufacturer: d.manufacturer_string().unwrap_or_default().to_string(),
            product: d.product_string().unwrap_or_default().to_string(),
        });
    }
    Ok(out)
}

/// An opened input interface we can poll for the reports the knob emits.
pub struct InputInterface {
    pub label: String,
    pub usage_page: u16,
    pub usage: u16,
    handle: HidDevice,
}

impl InputInterface {
    /// Non-blocking read of one report; returns the bytes read (may be empty).
    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        self.handle.read_timeout(buf, 0).context("HID read")
    }
}

/// Open every input-producing interface (everything except the vendor config channel),
/// in non-blocking mode, so we can watch what the knob emits live.
pub fn open_inputs(sel: &Selector) -> Result<Vec<InputInterface>> {
    let api = HidApi::new().context("init hidapi")?;
    let mut out = Vec::new();
    for d in api.device_list() {
        if d.vendor_id() != sel.vid || d.product_id() != sel.pid {
            continue;
        }
        if d.usage_page() == VENDOR_USAGE_PAGE {
            continue; // config channel emits nothing
        }
        let label = match (d.usage_page(), d.usage()) {
            (0x01, 0x06) => "keyboard",
            (0x0c, _) => "consumer/media",
            (0x01, 0x02) => "mouse",
            (0x01, 0x01) => "pointer",
            _ => "other",
        }
        .to_string();
        match api.open_path(d.path()) {
            Ok(handle) => {
                let _ = handle.set_blocking_mode(false);
                out.push(InputInterface {
                    label,
                    usage_page: d.usage_page(),
                    usage: d.usage(),
                    handle,
                });
            }
            Err(e) => {
                eprintln!(
                    "warning: could not open {} interface (usage_page={:#06x}): {e}",
                    label,
                    d.usage_page()
                );
            }
        }
    }
    Ok(out)
}

pub struct Device {
    handle: HidDevice,
}

impl Device {
    /// Open the vendor config interface for the selected device.
    pub fn open(sel: &Selector) -> Result<Device> {
        let api = HidApi::new().context("init hidapi")?;

        let mut candidate_path = None;
        let mut any_match = false;
        for d in api.device_list() {
            if d.vendor_id() != sel.vid || d.product_id() != sel.pid {
                continue;
            }
            any_match = true;
            let ok = match sel.usage_page {
                Some(up) => d.usage_page() == up,
                None => true,
            };
            if ok {
                candidate_path = Some(d.path().to_owned());
                break;
            }
        }

        if !any_match {
            bail!(
                "device {:04x}:{:04x} not found. Is it plugged in over USB? \
                 (Programming must be done over USB, not Bluetooth.)",
                sel.vid, sel.pid
            );
        }
        let path = candidate_path.ok_or_else(|| {
            anyhow!(
                "found {:04x}:{:04x} but no interface with usage page {:#06x}. \
                 Run `knobctl probe` and pass --usage-page.",
                sel.vid,
                sel.pid,
                sel.usage_page.unwrap_or(0)
            )
        })?;

        let handle = api.open_path(&path).context("open HID interface")?;
        let dev = Device { handle };
        // One-shot init message the firmware expects right after opening.
        dev.send(&crate::proto::init_message())?;
        Ok(dev)
    }

    /// Write a single 64-byte protocol payload (report id 0x03 prepended -> 65 bytes).
    pub fn send(&self, msg: &[u8; MSG_LEN]) -> Result<()> {
        let mut buf = [0u8; MSG_LEN + 1];
        buf[0] = crate::proto::REPORT_ID;
        buf[1..].copy_from_slice(msg);
        let n = self.handle.write(&buf).context("HID write")?;
        if n != buf.len() {
            bail!("short HID write: {n} of {} bytes", buf.len());
        }
        Ok(())
    }

    pub fn send_all(&self, msgs: &[[u8; MSG_LEN]]) -> Result<()> {
        for m in msgs {
            self.send(m)?;
        }
        Ok(())
    }

    /// Read one report from the config channel; `timeout_ms` <0 blocks, 0 is non-blocking.
    /// Returns the bytes read (0 on timeout).
    pub fn read(&self, buf: &mut [u8], timeout_ms: i32) -> Result<usize> {
        self.handle
            .read_timeout(buf, timeout_ms)
            .context("HID read")
    }
}
