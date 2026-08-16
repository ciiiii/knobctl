//! YAML mapping file: what each knob action / button should emit.
//!
//! ```yaml
//! layer: 0            # optional, default 0
//! knobs:
//!   - ccw: volumedown
//!     press: mute
//!     cw: volumeup
//! buttons:            # optional; index = position in list
//!   - ctrl-c
//!   - ctrl-v
//! ```

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::keys::{parse_macro, Macro};
use crate::proto::{build_binding, KnobAction, Target, MSG_LEN};

#[derive(Debug, Deserialize)]
pub struct KnobMap {
    pub ccw: Option<String>,
    pub press: Option<String>,
    pub cw: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Mapping {
    #[serde(default)]
    pub layer: u8,
    #[serde(default)]
    pub knobs: Vec<KnobMap>,
    #[serde(default)]
    pub buttons: Vec<String>,
}

pub struct Binding {
    pub describe: String,
    pub messages: Vec<[u8; MSG_LEN]>,
}

impl Mapping {
    pub fn load(path: &str) -> Result<Mapping> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read mapping file `{path}`"))?;
        serde_yaml::from_str(&text).context("parse mapping YAML")
    }

    /// Render every configured action into protocol messages.
    pub fn render(&self) -> Result<Vec<Binding>> {
        let mut out = Vec::new();

        for (i, knob) in self.knobs.iter().enumerate() {
            let n = i as u8;
            for (spec, action, name) in [
                (&knob.ccw, KnobAction::Ccw, "ccw"),
                (&knob.press, KnobAction::Press, "press"),
                (&knob.cw, KnobAction::Cw, "cw"),
            ] {
                if let Some(spec) = spec {
                    out.push(self.one(
                        format!("knob{n}.{name}"),
                        Target::Knob(n, action),
                        spec,
                    )?);
                }
            }
        }

        for (i, spec) in self.buttons.iter().enumerate() {
            if spec.trim().is_empty() {
                continue;
            }
            out.push(self.one(format!("button{i}"), Target::Button(i as u8), spec)?);
        }

        Ok(out)
    }

    fn one(&self, describe: String, target: Target, spec: &str) -> Result<Binding> {
        let mac: Macro = parse_macro(spec)
            .with_context(|| format!("parse macro `{spec}` for {describe}"))?;
        let messages = build_binding(self.layer, target, &mac)
            .with_context(|| format!("build binding for {describe}"))?;
        Ok(Binding { describe: format!("{describe} -> {spec}"), messages })
    }
}
