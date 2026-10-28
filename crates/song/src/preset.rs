//! `us-patch/1` presets: one device (kind + the values that differ from the kind's defaults, by name, sorted by
//! parameter index). Loading sets every listed value and leaves the others at their defaults.

use crate::model::{Device, Values};
use crate::tables;
use crate::Issue;
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "us-patch/1";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub format: String,
    pub name: String,
    pub kind: u8,
    pub values: Values,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<String>,
}

/// The device as a preset: non-default values only, sorted by parameter index.
pub fn save(d: &Device, name: &str) -> Preset {
    let mut vals: Vec<(usize, String, f32)> = match tables::of_device(d) {
        Some(t) => d
            .values
            .iter()
            .filter_map(|(n, v)| t.param(n).filter(|p| p.default.to_bits() != v.to_bits() || p.name == "mode").map(|p| (p.index, n.to_string(), v)))
            .collect(),
        None => d.values.iter().enumerate().map(|(i, (n, v))| (i, n.to_string(), v)).collect(),
    };
    vals.sort_by_key(|e| e.0);
    Preset { format: FORMAT.into(), name: name.into(), kind: d.kind, values: Values(vals.into_iter().map(|e| (e.1, e.2)).collect()), samples: d.samples.clone() }
}

/// A fresh device holding the preset.
pub fn load(p: &Preset) -> Device {
    Device { kind: p.kind, values: p.values.clone(), samples: p.samples.clone(), speech: vec![], bypass: false, preset: Some(p.name.clone()), trailer: None }
}

/// Parses and validates a preset (format, kind, names, ranges).
pub fn parse(json: &str) -> Result<Preset, Vec<Issue>> {
    let de = &mut serde_json::Deserializer::from_str(json);
    let p: Preset = serde_path_to_error::deserialize(de).map_err(|e| vec![Issue { path: crate::pointer(e.path()), message: e.inner().to_string() }])?;
    let mut out = vec![];
    if p.format != FORMAT {
        out.push(Issue { path: "/format".into(), message: format!("expected \"{FORMAT}\", found \"{}\"", p.format) });
    }
    let d = load(&p);
    match tables::of_device(&d) {
        None => out.push(Issue { path: "/kind".into(), message: format!("unknown device kind {}", p.kind) }),
        Some(t) => {
            for (n, v) in p.values.iter() {
                let path = format!("/values/{}", crate::validate::seg(n));
                match t.param(n) {
                    None => out.push(Issue { path, message: "unknown parameter".into() }),
                    Some(par) => {
                        let (lo, hi) = par.range();
                        if !v.is_finite() || v < lo || v > hi {
                            out.push(Issue { path, message: format!("value {v} out of range {lo}..{hi}") });
                        }
                    }
                }
            }
        }
    }
    if out.is_empty() { Ok(p) } else { Err(out) }
}

pub fn to_json(p: &Preset) -> String {
    crate::pretty(&serde_json::to_value(p).unwrap_or_default())
}
