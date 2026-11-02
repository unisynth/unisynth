//! Parameter tables per device kind (and effect mode): name, group, default, law, unit, display range, options,
//! structural flag. The instrument (kind 64) comes from `unisynth`'s host data; effect tables (kinds 65–73) live
//! here, pinned to the engine's parameter counts by a test. Effect defaults are editor defaults (a new device, a
//! preset that leaves a value out, reset): the legacy constructor values through the law inverses. Exported effect
//! chunks always carry their values, so the engine's own defaults never apply to a song.
//!
//! Names are frozen with unisynth chunk v1: songs store values by name (design D1).

use std::collections::HashMap;
use std::sync::OnceLock;
use unisynth::kind;
use unisynth::law::host::inv_choice;
use unisynth::params::{self, Law};

#[derive(Clone, Debug)]
pub struct Param {
    /// Device parameter index (`set_param`, automation).
    pub index: usize,
    pub name: String,
    /// Panel group: `voice`, `osc1`…, `noise`, `filt1`…, `env1`…, `lfo1`…, `mod1`…, `ctl`, or `fx`.
    pub group: String,
    pub default: f32,
    pub law: Law,
    pub unit: &'static str,
    /// Display range of the physical value.
    pub min: f32,
    pub max: f32,
    /// Choice names (empty for continuous parameters).
    pub options: Vec<String>,
    /// Chunk-only: changing it rebuilds the device; never automated.
    pub structural: bool,
    /// Stored as is (sampler index 0..65535), not normalized.
    pub raw: bool,
}

impl Param {
    /// Physical value for the normalized `p` (module-dependent laws return `p`).
    pub fn physical(&self, p: f32) -> f32 {
        self.law.map(p)
    }

    /// Valid stored range.
    pub fn range(&self) -> (f32, f32) {
        if self.raw { (0.0, unisynth::chunk::MAX_SAMPLE_INDEX) } else { (0.0, 1.0) }
    }
}

#[derive(Debug)]
pub struct Table {
    pub kind: u8,
    /// Effect mode (0 for the instrument).
    pub mode: usize,
    /// Display name: `Unisynth`, `Reverb · Freeverb`, …
    pub label: String,
    pub params: Vec<Param>,
    by_name: HashMap<String, usize>,
}

impl Table {
    pub fn param(&self, name: &str) -> Option<&Param> {
        self.by_name.get(name).map(|&i| &self.params[i])
    }
}

/// Every effect (kind, mode).
pub const EFFECT_MODES: &[(u8, usize)] = &[
    (kind::DISTORTION, 0),
    (kind::DISTORTION, 1),
    (kind::DISTORTION, 2),
    (kind::CRUSHER, 0),
    (kind::EQ, 0),
    (kind::EQ, 1),
    (kind::DELAY, 0),
    (kind::DELAY, 1),
    (kind::CHORUS, 0),
    (kind::FLANGER, 0),
    (kind::FLANGER, 1),
    (kind::REVERB, 0),
    (kind::REVERB, 1),
    (kind::REVERB, 2),
    (kind::COMPRESSOR, 0),
    (kind::FORMANT, 0),
    (kind::EQ, 2),
    (kind::DELAY, 2),
    (kind::REVERB, 3),
    (kind::COMPRESSOR, 1),
    (kind::DISTORTION, 3),
    (kind::FLANGER, 2),
];

pub fn kind_name(k: u8) -> &'static str {
    match k {
        kind::INSTRUMENT => "Unisynth",
        kind::DISTORTION => "Distortion",
        kind::CRUSHER => "Crusher",
        kind::EQ => "EQ",
        kind::DELAY => "Delay",
        kind::CHORUS => "Chorus",
        kind::FLANGER => "Flanger",
        kind::REVERB => "Reverb",
        kind::COMPRESSOR => "Compressor",
        kind::FORMANT => "Formant",
        _ => "unknown",
    }
}

pub fn mode_names(k: u8) -> &'static [&'static str] {
    match k {
        kind::DISTORTION => &["Shaper", "Curve", "Power", "Wrap"],
        kind::CRUSHER => &["Crusher"],
        kind::EQ => &["Parametric", "3-band", "Cut"],
        kind::DELAY => &["Tempo", "Ms", "Modulated"],
        kind::CHORUS => &["Chorus"],
        kind::FLANGER => &["Phaser", "Ring", "Comb"],
        kind::REVERB => &["Freeverb", "Compact", "Rooms", "Comb"],
        kind::COMPRESSOR => &["Compressor", "Look-ahead"],
        kind::FORMANT => &["Formant"],
        _ => &[],
    }
}

/// Normalized value of a legacy `param_to_frequency` corner (`20 + 19 980 p²`).
fn hz(f: f32) -> f32 {
    ((f - 20.0) / 19_980.0).max(0.0).sqrt()
}

/// Parameter names and editor defaults after the mode, per effect (kind, mode), in legacy order.
fn effect_params(k: u8, mode: usize) -> Vec<(&'static str, f32)> {
    let env_ms = |ms: f32| ((ms - 1.0) / 5000.0).max(0.0).sqrt(); // scalar_to_env_value inverse
    match (k, mode) {
        (kind::DISTORTION, 0) => vec![("drive", 0.2), ("threshold", 0.8), ("foldover", 0.0), ("dry_wet", 1.0), ("shaper", 0.0), ("oversampling", 0.0)],
        (kind::DISTORTION, 1) => vec![
            ("x1", 0.1), ("y1", 0.1), ("x2", 0.3), ("y2", 0.3), ("power1", 0.5), ("power2", 0.5), ("power3", 0.5), ("dry_wet", 1.0),
        ],
        (kind::DISTORTION, 2) => vec![("amount", 0.0)],
        (kind::CRUSHER, 0) => vec![("vertical", 0.0), ("horizontal", 0.0), ("dry_wet", 1.0)],
        (kind::EQ, 0) => vec![
            ("low_cut", hz(20.0)), ("low_cut_q", 0.5),
            ("peak1_freq", hz(1000.0)), ("peak1_gain", 0.5), ("peak1_q", 0.5),
            ("peak2_freq", hz(3000.0)), ("peak2_gain", 0.5), ("peak2_q", 0.5),
            ("peak3_freq", hz(7000.0)), ("peak3_gain", 0.5), ("peak3_q", 0.5),
            ("high_cut", hz(20_000.0)), ("high_cut_q", 0.5), ("master", 1.0),
        ],
        // Tunefish band gain is 2p below the middle: 0.5 = unity (the engine's own 0 would silence the track)
        // pan 0: off (both channels); a byte 1..255 pans equal-power after the bands (a converted XM pans after its chain)
        (kind::EQ, 1) => vec![("low", 0.5), ("mid", 0.5), ("high", 0.5), ("pan", 0.0)],
        (kind::DELAY, 0) => vec![
            ("left_coarse", 3.0 / 16.0), ("left_fine", 0.0), ("right_coarse", 4.0 / 16.0), ("right_fine", 0.0),
            ("low_cut", hz(20.0)), ("high_cut", hz(19_980.0)), ("feedback", 0.5), ("cross", 0.0), ("dry_wet", 0.5),
        ],
        (kind::DELAY, 1) => vec![("left_time", 0.0), ("right_time", 0.0), ("decay", 0.0)],
        (kind::CHORUS, 0) => vec![("rate", 0.0), ("depth", 0.0), ("gain", 0.0)],
        (kind::FLANGER, 0) => vec![
            ("type", 0.0), ("amount", 0.0), ("feedback", 0.0), ("spread", 0.0), ("vibrato_freq", 0.0),
            ("vibrato_amount", 0.0), ("low_cut", hz(20.0)), ("high_cut", hz(19_980.0)), ("dry_wet", 0.5),
        ],
        (kind::FLANGER, 1) => vec![("lfo", 0.0), ("top", 0.0), ("amp", 0.0), ("wet", 0.0)],
        (kind::REVERB, 0) => vec![
            ("freeze", 0.0), ("room", 0.5), ("damp", 0.0), ("width", 1.0), ("low_cut", hz(20.0)),
            ("high_cut", hz(19_980.0)), ("dry_wet", 0.25), ("pre_delay", 0.0),
        ],
        (kind::REVERB, 1) => vec![("room", 0.0), ("damp", 0.0), ("wet", 0.0), ("width", 0.0)],
        (kind::REVERB, 2) => vec![
            ("room_size", 0.5), ("feedback", 0.76), ("low_cut", hz(200.0)), ("high_cut", hz(8000.0)), ("dry_wet", 0.27),
            ("pre_delay", 0.0),
        ],
        (kind::COMPRESSOR, 0) => vec![
            ("sidechain", 0.0), ("input_gain", 0.5), ("threshold", 1.0), ("ratio", 0.0), ("attack", env_ms(5.0)),
            ("release", env_ms(200.0)), ("output_gain", 0.5),
        ],
        (kind::FORMANT, 0) => vec![("vowel", 0.0), ("wet", 0.0)],
        // the 7-bit modes (change add-v2-import): step / 128; the cut EQ's DC filter is a choice (default on)
        (kind::EQ, 2) => vec![("low_cut", 0.0), ("high_cut", 127.0 / 128.0), ("dc_filter", 1.0)],
        (kind::DELAY, 2) => vec![
            ("volume", 100.0 / 128.0), ("feedback", 80.0 / 128.0), ("delay_l", 0.5), ("delay_r", 0.5), ("mod_rate", 0.0),
            ("mod_depth", 0.0), ("mod_phase", 0.5),
        ],
        (kind::REVERB, 3) => vec![("time", 0.5), ("high_cut", 0.5), ("low_cut", 0.25), ("volume", 127.0 / 128.0)],
        (kind::COMPRESSOR, 1) => vec![
            ("detect", 0.0), ("couple", 0.0), ("auto_gain", 1.0 / 128.0), ("lookahead", 2.0 / 128.0), ("threshold", 90.0 / 128.0),
            ("ratio", 0.25), ("attack", 20.0 / 128.0), ("release", 0.5), ("out_gain", 0.5),
        ],
        // mini-cauldron modes (change add-cauldron-xm-import); defaults pass the signal unchanged: drive p·4 (0.25 = ×1),
        // offset 2p − 1; comb delay p·16384 frames, amount, sweep p·0.01 per frame, restart (any change)
        (kind::DISTORTION, 3) => vec![("drive", 0.25), ("offset", 0.5)],
        (kind::FLANGER, 2) => vec![("delay", 0.0), ("amount", 0.0), ("sweep", 0.0), ("restart", 0.0)],
        _ => vec![],
    }
}

fn choice_options(i: usize, law: Law) -> Vec<String> {
    // mod-matrix targets: id = index(p) (p = id / 255), names from unisynth (`osc2.gain`, `filt1.cutoff`, …)
    let target_field = (0..params::MOD_SLOTS).any(|s| params::modslot(s, params::m::TARGET) == i);
    if matches!(law, Law::Index) && target_field {
        return (0..unisynth::modmx::target::COUNT).map(unisynth::modmx::target::name).collect();
    }
    let field = params::name(i);
    let field = field.rsplit('.').next().unwrap_or("");
    let names: &[&str] = match law {
        Law::Choice(n) => {
            let base = |b: usize, stride: usize, slots: usize| i >= b && i < b + stride * slots;
            let at = |b: usize, stride: usize, slots: usize, fld: usize| base(b, stride, slots) && (i - b) % stride == fld;
            let in_mod = (0..params::MOD_SLOTS).any(|s| params::modslot(s, 0) <= i && i < params::modslot(s, 0) + params::m::LEN);
            if at(params::OSC, params::o::LEN, params::OSC_SLOTS, params::o::TYPE) {
                &[
                    "Off", "Phase mod", "Saw/pulse", "Wavetable", "Sampler", "Naive saw/tri", "Naive pulse", "Sine", "Coloured noise", "FM", "Aux in A",
                    "Aux in B", "Clocked",
                ]
            } else if at(params::OSC, params::o::LEN, params::OSC_SLOTS, params::o::COMBINE) {
                &["Add", "Multiply (ring)", "Replace"]
            } else if at(params::OSC, params::o::LEN, params::OSC_SLOTS, params::o::LAW) {
                &["Semitones/Hz", "7-bit steps"]
            } else if at(params::FILT, params::f::LEN, params::FILT_SLOTS, params::f::TYPE) {
                &[
                    "Off", "SVF low-pass", "SVF high-pass", "SVF band-pass", "SVF notch", "4-pole low-pass", "RBJ high-pass", "RBJ band-pass",
                    "Resonator notch", "SVF all-pass", "4-pole high-pass",
                ]
            } else if at(params::FILT, params::f::LEN, params::FILT_SLOTS, params::f::FORM) {
                &["Input / feedback before the clip", "Feedback / after the clip"]
            } else if at(params::FILT, params::f::LEN, params::FILT_SLOTS, params::f::OVERSAMPLE) {
                &["Interpolated / 1 step", "Held / 2 steps"]
            } else if at(params::FILT, params::f::LEN, params::FILT_SLOTS, params::f::LAW) {
                &["Hz", "7-bit steps"]
            } else if at(params::ENV, params::e::LEN, params::ENV_SLOTS, params::e::LAW) {
                &["Seconds", "Rate", "Frame"]
            } else if at(params::ENV, params::e::LEN, params::ENV_SLOTS, params::e::CURVE) {
                &["By law", "Quadratic", "Power", "Exponential"]
            } else if at(params::LFO, params::l::LEN, params::LFO_SLOTS, params::l::SHAPE) {
                &["Sine", "Ramp up", "Ramp down", "Square", "Random", "Triangle", "Sample and hold", "Pulse"]
            } else if at(params::LFO, params::l::LEN, params::LFO_SLOTS, params::l::SYNC) {
                &["Song time", "Continue", "Restart"]
            } else if at(params::LFO, params::l::LEN, params::LFO_SLOTS, params::l::LAW) {
                &["Hz", "Frame"]
            } else if at(params::LFO, params::l::LEN, params::LFO_SLOTS, params::l::POLARITY) {
                &["Positive", "Negative", "Bipolar"]
            } else if in_mod && field == "source" {
                &[
                    "None", "Env 1", "Env 2", "Env 3", "Env 4", "LFO 1", "LFO 2", "Mod wheel", "Velocity", "Key", "Breath (CC2)", "CC3", "CC4", "CC5",
                    "CC6", "Volume (CC7)", "Note (centred)",
                ]
            } else if in_mod && field == "operation" {
                &["Scale", "Offset", "Add"]
            } else if i == params::vs::KEYSYNC {
                &["Free", "Restart", "Full reset"]
            } else if i == params::vs::ROUTING {
                &["Serial", "Parallel"]
            } else if i == params::vs::AMP_SOURCE {
                &["Master × velocity × env 1", "Env 1 only"]
            } else if i == params::vs::PAN_STAGE {
                &["Per slot", "After the drive"]
            } else if i == params::vs::LAW {
                &["Semitones", "7-bit steps"]
            } else if i == params::v::MODE {
                &["Poly", "Mono legato"]
            } else if i == params::v::STEAL {
                &["Off", "Oldest"]
            } else if n == 2 {
                &["Off", "On"]
            } else {
                return (0..n).map(|k| k.to_string()).collect();
            }
        }
        _ => return vec![],
    };
    names.iter().map(|s| s.to_string()).collect()
}

/// Unit and display range of a law.
fn unit_range(law: Law) -> (&'static str, f32, f32) {
    match law {
        Law::Cutoff => ("Hz", 20.0, 20_000.0),
        Law::Gain => ("×", 0.0, 16.0),
        Law::LfoRate => ("Hz", 0.01, 100.0),
        Law::Time => ("s", 0.0, 10.0),
        Law::Transpose | Law::Coarse => ("st", -24.0, 24.0),
        Law::Fine => ("st", -1.0, 1.0),
        Law::BendRange => ("st", 0.0, 24.0),
        Law::Octave => ("oct", -4.0, 4.0),
        Law::Ratio | Law::RatioFine => ("×", 0.0, 16.0),
        Law::FixedHz => ("Hz", 0.0, 20_000.0),
        Law::Feedback => ("", 0.0, 8.0),
        Law::PmOut => ("", 0.0, 4.0),
        Law::Choice(n) => ("", 0.0, n.saturating_sub(1) as f32),
        Law::Count(lo, hi) => ("", lo as f32, hi as f32),
        Law::Index => ("", 0.0, 255.0),
        Law::EnvTime => ("ms", 1.0, 5001.0),
        Law::Raw => ("", 0.0, 65_535.0),
        Law::Steps => ("steps", 0.0, 128.0),
        Law::Unit | Law::Pan | Law::Shape | Law::ModAmount => ("", 0.0, 1.0),
    }
}

fn build(kind: u8, mode: usize, label: String, params: Vec<Param>) -> Table {
    let by_name = params.iter().enumerate().map(|(i, p)| (p.name.clone(), i)).collect();
    Table { kind, mode, label, params, by_name }
}

fn instrument() -> Table {
    let ps = (0..params::COUNT)
        .map(|i| {
            let name = params::name(i);
            let law = params::law_of(i);
            let (unit, min, max) = unit_range(law);
            Param {
                index: i,
                group: name.split_once('.').map_or("voice", |g| g.0).to_string(),
                name,
                default: params::DEFAULTS[i],
                law,
                unit,
                min,
                max,
                options: choice_options(i, law),
                structural: params::is_structural(i),
                raw: params::is_raw(i),
            }
        })
        .collect();
    build(kind::INSTRUMENT, 0, kind_name(kind::INSTRUMENT).to_string(), ps)
}

fn effect(k: u8, mode: usize) -> Table {
    let modes = mode_names(k);
    // the mode is `Choice(mode_total(k))`
    let positions = unisynth::fx::mode_total(k);
    let _ = inv_choice;
    let mut ps = vec![Param {
        index: 0,
        name: "mode".into(),
        group: "fx".into(),
        default: unisynth::fx::mode_value(k, mode),
        law: Law::Choice(positions),
        unit: "",
        min: 0.0,
        max: positions.saturating_sub(1) as f32,
        options: (0..positions).map(|m| modes.get(m).map_or("—".to_string(), |s| s.to_string())).collect(),
        structural: true,
        raw: false,
    }];
    for (i, (name, d)) in effect_params(k, mode).into_iter().enumerate() {
        ps.push(Param {
            index: i + 1,
            name: name.into(),
            group: "fx".into(),
            default: d,
            law: Law::Unit,
            unit: "",
            min: 0.0,
            max: 1.0,
            options: vec![],
            structural: false,
            raw: false,
        });
    }
    let label = if modes.len() > 1 { format!("{} · {}", kind_name(k), modes[mode]) } else { kind_name(k).to_string() };
    build(k, mode, label, ps)
}

fn all() -> &'static [Table] {
    static T: OnceLock<Vec<Table>> = OnceLock::new();
    T.get_or_init(|| {
        let mut v = vec![instrument()];
        v.extend(EFFECT_MODES.iter().map(|&(k, m)| effect(k, m)));
        v
    })
}

/// The table of `kind` (and effect `mode`; ignored for the instrument).
pub fn table(kind: u8, mode: usize) -> Option<&'static Table> {
    let mode = if kind == unisynth::kind::INSTRUMENT { 0 } else { mode };
    all().iter().find(|t| t.kind == kind && t.mode == mode)
}

/// Every table (instrument first, then the effect modes).
pub fn tables() -> &'static [Table] {
    all()
}

/// The mode an effect's values select (`mode` value, default 0), or 0 for the instrument.
pub fn mode_of(kind: u8, values: &crate::model::Values) -> usize {
    if unisynth::fx::mode_total(kind) == 0 {
        return 0;
    }
    unisynth::fx::mode_of_value(kind, values.get("mode").unwrap_or(0.0))
}

/// The table for a device (kind + its `mode` value).
pub fn of_device(d: &crate::model::Device) -> Option<&'static Table> {
    table(d.kind, mode_of(d.kind, &d.values))
}
