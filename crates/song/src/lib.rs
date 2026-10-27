//! `song/1` song source and `us-patch/1` presets for the unified synth (change add-synth-daw, spec song-source):
//! serde model, validation with JSON-pointer issues, export to and import from the `synth-mix` blob, parameter
//! tables, presets and rendering through the song player. Shared by the `song` CLI and the browser live host.

pub mod b64;
pub mod blob;
pub mod export;
pub mod import;
pub mod model;
pub mod patterns;
pub mod preset;
pub mod render;
pub mod tables;
pub mod validate;
#[cfg(feature = "threads")]
pub mod workers;

pub use export::{export, export_model, tick_to_sample};
pub use import::{apply_meta, import, import_one_clip};
pub use model::Song;
pub use validate::validate;

use serde::Serialize;

/// A problem at a JSON pointer into the song file (`""` = the whole file).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}: {}", if self.path.is_empty() { "/" } else { &self.path }, self.message)
    }
}

/// JSON pointer of a serde_path_to_error path, built from its segments (parameter names such as `osc1.type`
/// contain dots, so the path's string form cannot be split).
pub(crate) fn pointer(path: &serde_path_to_error::Path) -> String {
    use serde_path_to_error::Segment;
    let mut out = String::new();
    for seg in path.iter() {
        out.push('/');
        match seg {
            Segment::Seq { index } => out.push_str(&index.to_string()),
            Segment::Map { key } => out.push_str(&validate::seg(key)),
            Segment::Enum { variant } => out.push_str(&validate::seg(variant)),
            Segment::Unknown => out.push('?'),
        }
    }
    out
}

/// Parses a `song/1` file (structure only; see [`validate`] for the semantic checks).
pub fn parse(json: &str) -> Result<Song, Vec<Issue>> {
    let de = &mut serde_json::Deserializer::from_str(json);
    serde_path_to_error::deserialize(de).map_err(|e| {
        let msg = e.inner().to_string();
        let mut path = pointer(e.path());
        // an unknown field is reported at the field; point at the object that holds it
        if msg.starts_with("unknown field") {
            if let Some(i) = path.rfind('/') {
                path.truncate(i);
            }
        }
        vec![Issue { path, message: msg }]
    })
}

/// Parses and validates.
pub fn load(json: &str) -> Result<Song, Vec<Issue>> {
    let s = parse(json)?;
    let issues = validate(&s);
    if issues.is_empty() { Ok(s) } else { Err(issues) }
}

/// The song as `song/1` JSON (stable layout: two-space indent, number arrays on one line).
pub fn to_json(s: &Song) -> String {
    pretty(&serde_json::to_value(s).unwrap_or_default())
}

/// Pretty JSON with arrays of scalars kept on one line (notes, points), so diffs stay one line per note.
pub fn pretty(v: &serde_json::Value) -> String {
    fn scalar(v: &serde_json::Value) -> bool {
        !v.is_array() && !v.is_object()
    }
    fn go(v: &serde_json::Value, ind: usize, out: &mut String) {
        let pad = |n: usize| "  ".repeat(n);
        match v {
            serde_json::Value::Array(a) if a.is_empty() => out.push_str("[]"),
            serde_json::Value::Array(a) if a.iter().all(scalar) => {
                out.push('[');
                for (i, x) in a.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&x.to_string());
                }
                out.push(']');
            }
            serde_json::Value::Array(a) => {
                out.push_str("[\n");
                for (i, x) in a.iter().enumerate() {
                    out.push_str(&pad(ind + 1));
                    go(x, ind + 1, out);
                    out.push_str(if i + 1 < a.len() { ",\n" } else { "\n" });
                }
                out.push_str(&pad(ind));
                out.push(']');
            }
            serde_json::Value::Object(m) if m.is_empty() => out.push_str("{}"),
            serde_json::Value::Object(m) => {
                out.push_str("{\n");
                for (i, (k, x)) in m.iter().enumerate() {
                    out.push_str(&pad(ind + 1));
                    out.push_str(&serde_json::Value::String(k.clone()).to_string());
                    out.push_str(": ");
                    go(x, ind + 1, out);
                    out.push_str(if i + 1 < m.len() { ",\n" } else { "\n" });
                }
                out.push_str(&pad(ind));
                out.push('}');
            }
            x => out.push_str(&x.to_string()),
        }
    }
    let mut out = String::new();
    go(v, 0, &mut out);
    out.push('\n');
    out
}

/// Cargo features of `unisynth` a song needs (`us-*`), derived from its device values. Conservative: a module that
/// might sound is listed. (`add-unified-synth` task 6.4's `features` command is the reference once it lands.)
pub fn features(s: &Song) -> Vec<String> {
    use unisynth::law::choice;
    use unisynth::params::{self as p, e, f, filter_type as ft, o, osc_type as ot, shape};
    let mut out = std::collections::BTreeSet::new();
    for d in s.tracks.iter().chain([&s.master]).flat_map(|t| &t.devices).filter(|d| !d.bypass) {
        if d.kind == unisynth::kind::INSTRUMENT {
            let mut v = p::DEFAULTS;
            if let Some(t) = tables::table(d.kind, 0) {
                for (n, x) in d.values.iter() {
                    if let Some(par) = t.param(n) {
                        v[par.index] = x;
                    }
                }
            }
            for slot in 0..p::OSC_SLOTS {
                let at = |fld: usize| v[p::OSC + slot * o::LEN + fld];
                match choice(at(o::TYPE), ot::COUNT) {
                    ot::PM => out.insert("us-osc-pm"),
                    ot::BLIT => out.insert("us-osc-blit"),
                    ot::WAVETABLE => out.insert("us-osc-wt"),
                    ot::SAMPLER => out.insert(if choice(at(o::SHAPE + shape::SMP_SOURCE), p::smp_source::COUNT) != p::smp_source::GSM { "us-sample-dls" } else { "us-sample-gsm" }),
                    _ => false,
                };
            }
            if v[p::v::NOISE_LEVEL] > 0.0 {
                out.insert("us-noise");
            }
            for slot in 0..p::FILT_SLOTS {
                match choice(v[p::FILT + slot * f::LEN + f::TYPE], ft::COUNT) {
                    ft::SVF_LP | ft::SVF_HP | ft::SVF_BP | ft::SVF_NOTCH => out.insert("us-flt-svf"),
                    ft::LADDER_LP => out.insert("us-flt-ladder"),
                    ft::RBJ_HP | ft::RBJ_BP => out.insert("us-flt-rbj"),
                    ft::NOTCH => out.insert("us-flt-notch"),
                    _ => false,
                };
            }
            for slot in 0..p::ENV_SLOTS {
                out.insert(if choice(v[p::ENV + slot * e::LEN + e::MODE], 2) == 1 { "us-env-rate" } else { "us-env-timed" });
            }
        } else if let Some(name) = fx_feature(d.kind, tables::mode_of(d.kind, &d.values)) {
            out.insert(name);
        }
    }
    out.into_iter().map(String::from).collect()
}

fn fx_feature(k: u8, mode: usize) -> Option<&'static str> {
    use unisynth::kind::*;
    Some(match (k, mode) {
        (DISTORTION, 0) => "us-fx-distortion-shaper",
        (DISTORTION, _) => "us-fx-distortion-power",
        (CRUSHER, _) => "us-fx-crusher",
        (EQ, 0) => "us-fx-eq-parametric",
        (EQ, _) => "us-fx-eq-3band",
        (DELAY, 0) => "us-fx-delay-tempo",
        (DELAY, _) => "us-fx-delay-ms",
        (CHORUS, _) => "us-fx-chorus",
        (FLANGER, 0) => "us-fx-flanger-phaser",
        (FLANGER, _) => "us-fx-flanger-ring",
        (REVERB, 0) => "us-fx-reverb-freeverb",
        (REVERB, 1) => "us-fx-reverb-compact",
        (REVERB, _) => "us-fx-reverb-rooms",
        (COMPRESSOR, _) => "us-fx-compressor",
        (FORMANT, _) => "us-fx-formant",
        _ => return None,
    })
}
