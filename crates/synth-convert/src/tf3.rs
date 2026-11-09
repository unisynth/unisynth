//! A Tunefish 3 instrument as one unified patch plus its insert chain (change add-tf3-import, design D6; behaviour from
//! `research/tunefish3-functional-spec.md`).
//!
//! What the patch holds:
//!
//! * slot 1 is the point oscillator (`points`), slot 2 its sub-oscillator one octave down at half gain when
//!   `OscSubOsc` is on, then the PAD table (`pad`); every slot runs Tunefish 3's decimation stage;
//! * noise with its band, the low- and high-pass filters (band-pass and notch only in editor mode), two LFOs, the two
//!   ADSRs in rate mode, the voice limit as capacity — built in the old layout as for Tunefish 4 and migrated;
//! * every mod route as Scale slots written in the v3 layout (one Tunefish slot can need several: a volume route
//!   reaches the oscillator, its sub-oscillator and the PAD table). The amount is `(20(p − 0.5))²`; a route reading an
//!   LFO is rescaled so the unified LFO (0..1 shapes) gives Tunefish 3's values (its sine is `sin φ + 0.5`, its ramps
//!   reach 0..2 and 1..−1);
//! * envelope 1 ends the voice: the ADSR routed to volume, else the organ gate (the note sounds until every ADSR the
//!   matrix reads has finished);
//! * velocity scales the oscillator slots and the noise, not the PAD table (as Tunefish 3).
//!
//! Player mode (the default, what the intros played) quantises every value to the stored byte and drops the modules
//! the player build leaves out: band-pass and notch, the MOD1–4 sources, chorus, flanger and EQ.
//!
//! Early mode is the 2011 engine's player (Pandora; change add-tf3-pandora, spec §10.4): values are taken as stored
//! (whole percents), unison is `fi(15p) + 1` layers, the sub-oscillator `fi(5p) + 1` passes at halving gain, drive
//! `1 + 32p`, and the LFO sine `0.5 + 0.5·depth·sin` at 25p² Hz. What the unified synth cannot play (the oscillator
//! offset with its one-sided clip, per-layer clipping, the table-free point curve, no decimation) is warned by name.

use crate::e3prj::COUNT;
use crate::model::Dev;
use crate::patch::Patch;
use std::f32::consts::{SQRT_2, TAU};
use unisynth::law::host as inv;
use unisynth::modmx::{op, source, target};
use unisynth::params::old::{self as po, e, filter_type, l, n, o, osc_type, v};
use unisynth::params::{self as p3, pt, shape as sp};

// Parameter indices (spec §5; `names()` order, checked by a test).
const OSC_POINTS: usize = 0;
const OSC_P1: usize = 1; // offset, value, mode per point: 1 + 3k
const OSC_FI_INT: usize = 13;
const OSC_VOL: usize = 14;
const OSC_FREQ: usize = 15;
const OSC_PAN: usize = 16;
const OSC_DETUNE: usize = 17;
const OSC_POLY: usize = 18;
const OSC_UNISON: usize = 19;
const OSC_SPREAD: usize = 20;
const OSC_SUBOSC: usize = 21;
const OSC_GLIDE: usize = 22;
const OSC_DRIVE: usize = 23;
const OSC_OCTAVE: usize = 24;
const OSC_OFFSET: usize = 25;
const OSC_SLOP: usize = 26;
const ADD_VOLUME: usize = 27;
const ADD_PROFILE: usize = 28;
const ADD_OCTAVE: usize = 29;
const ADD_BANDW: usize = 30;
const ADD_DAMP: usize = 31;
const ADD_HARM: usize = 32;
const ADD_SCALE: usize = 33;
const ADD_DRIVE: usize = 34;
const NOISE_ON: usize = 35;
const NOISE_FREQ: usize = 36;
const NOISE_BW: usize = 37;
const LP_ON: usize = 38;
const HP_ON: usize = 41;
const BP_ON: usize = 44;
const NT_ON: usize = 47;
const ENV1: usize = 50;
const LFO1: usize = 60;
const MM: usize = 68;
const EFFECT: usize = 98;
const GAIN: usize = 122;

/// Seconds of one Tunefish 3 control block.
const BLOCK_S: f32 = 384.0 / 44_100.0;
/// Where an abrupt cut inside the closing block carries the energy of the linear fade (as for Tunefish 4).
const FADE_CUT: f32 = 1.0 / 3.0;
/// The longest gate release a unified envelope can express (milliseconds).
const GATE_MAX_MS: f32 = 5001.0;

/// Stored value range per parameter (spec §5 column R; 0 = hundredths).
const RANGE: [u8; COUNT] = {
    let mut r = [0u8; COUNT];
    r[OSC_POINTS] = 3;
    let mut k = 0;
    while k < 4 {
        r[OSC_P1 + 3 * k + 2] = 2;
        k += 1;
    }
    r[OSC_FI_INT] = 2;
    r[OSC_POLY] = 15;
    r[OSC_UNISON] = 9;
    r[OSC_SUBOSC] = 1;
    r[OSC_OCTAVE] = 8;
    r[ADD_PROFILE] = 3;
    r[ADD_OCTAVE] = 8;
    r[LP_ON] = 1;
    r[HP_ON] = 1;
    r[BP_ON] = 1;
    r[NT_ON] = 1;
    r[LFO1 + 2] = 4;
    r[LFO1 + 3] = 1;
    r[LFO1 + 6] = 4;
    r[LFO1 + 7] = 1;
    let mut i = 0;
    while i < 10 {
        r[MM + 3 * i] = 9;
        r[MM + 3 * i + 2] = 25;
        r[EFFECT + i] = 10;
        i += 1;
    }
    r[124] = 4; // FrmtType
    r
};

/// x87 float → int (round half to even), the intro platform's conversion.
pub fn fi(x: f32) -> i32 {
    x.round_ties_even() as i32
}

/// The values the intro played: each parameter as its stored byte read back (spec §5).
pub fn quantise(p: &[f32]) -> Vec<f32> {
    p.iter()
        .zip(RANGE)
        .map(|(&x, r)| {
            let x = x.clamp(0.0, 2.55);
            if r == 0 { fi(x * 100.0) as f32 / 100.0 } else { fi(x * r as f32) as f32 / r as f32 }
        })
        .collect()
}

/// Which modules to keep: the player build (the intro) or the editor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Player,
    Editor,
    /// The 2011 engine's player (Pandora): its own laws, no band-pass, notch or effects.
    Early,
}

/// A Tunefish 3 mod slot: `(source 0–9, amount, target 0–25)`.
fn slot(p: &[f32], i: usize) -> (i32, f32, i32) {
    let b = MM + 3 * i;
    let a = 20.0 * (p[b + 1] - 0.5);
    (fi(9.0 * p[b]), a * a, fi(25.0 * p[b + 2]))
}

/// The ADSR (0, 1) a source id reads.
fn adsr_of(s: i32) -> Option<usize> {
    matches!(s, 3 | 4).then(|| s as usize - 3)
}

/// The ADSR routed to volume (target 9), if any.
fn volume_adsr(p: &[f32]) -> Option<usize> {
    (0..10).find_map(|i| {
        let (s, _, d) = slot(p, i);
        adsr_of(s).filter(|_| d == 9)
    })
}

/// The ADSRs a slot reads (Tunefish 3 runs an envelope only then; a running one keeps a released voice sounding).
fn used_adsrs(p: &[f32]) -> [bool; 2] {
    let mut used = [false; 2];
    for i in 0..10 {
        let (s, _, d) = slot(p, i);
        if let Some(k) = adsr_of(s).filter(|_| d != 0) {
            used[k] = true;
        }
    }
    used
}

/// The unified envelope of each ADSR: the volume ADSR (or the gate) is envelope 1.
fn env_slots(p: &[f32]) -> [usize; 2] {
    match volume_adsr(p) {
        Some(0) => [0, 1],
        Some(_) => [1, 0],
        None => [1, 2],
    }
}

/// Milliseconds a released ADSR needs from its sustain level to 0.001, in whole blocks (spec §3.6).
fn release_ms(p: &[f32], k: usize) -> f32 {
    let b = ENV1 + 5 * k;
    let (r, slope) = (p[b + 3], p[b + 4]);
    let sigma = 0.0005 * 384.0;
    let step = 0.25 * (0.94 * (r * r * r).max(1e-9)).log10() * sigma;
    let mut v = p[b + 2].clamp(0.0, 1.0);
    let mut blocks = 0u32;
    while v > 0.001 && blocks < 4000 {
        v += step * v.powf(slope);
        blocks += 1;
    }
    (blocks as f32 + FADE_CUT) * BLOCK_S * 1000.0
}

/// How long a voice without a volume ADSR lasts after its key is up: the longest release of the ADSRs the matrix
/// reads (none: the block of the key-up).
fn gate_ms(p: &[f32]) -> f32 {
    let used = used_adsrs(p);
    (0..2).filter(|&k| used[k]).map(|k| release_ms(p, k)).fold(FADE_CUT * BLOCK_S * 1000.0, f32::max).min(GATE_MAX_MS)
}

/// An LFO as the unified LFO: old-layout shape value, unified depth parameter, amount factor for the routes that read
/// it, rate in Hz, warning.
struct Lfo {
    shape: f32,
    depth: f32,
    factor: f32,
    hz: f32,
    warn: Option<String>,
}

/// LFO `k` as the unified LFO. Tunefish 3 shapes by thresholds on `p` (sine `sin φ + 0.5`, ramps `φ/π` and `1 − φ/π`,
/// square, random); `A·(d·s₃ + 1 − d) = A'·(d'·s + 1 − d')` with the unified 0..1 shape `s` gives `d'` and `A'/A`.
/// The 2011 engine (`early`): `< 0.33` sine `0.5 + 0.5·a·sin(πφ)` (a = depth, linear) at 25p² Hz, else ramps without
/// depth, falling below 0.66, rising above, at 50p²/π Hz.
fn lfo(p: &[f32], k: usize, early: bool) -> Lfo {
    let b = LFO1 + 4 * k;
    if early {
        let (a, r2) = (p[b + 1].clamp(0.0, 1.0), p[b] * p[b]);
        return if p[b + 2] < 0.33 {
            // 0.5 − 0.5a + a·s = A'(d'·s + 1 − d'): A' = (1 + a)/2, d' = 2a/(1 + a)
            let f = 0.5 * (1.0 + a);
            Lfo { shape: 0.0, depth: (a / f).sqrt(), factor: f, hz: 25.0 * r2, warn: None }
        } else {
            let shape = if p[b + 2] < 0.66 { 0.5 } else { 0.25 };
            Lfo { shape, depth: 1.0, factor: 1.0, hz: 50.0 * r2 / std::f32::consts::PI, warn: None }
        };
    }
    let d = p[b + 1] * p[b + 1];
    let shape = [0.2, 0.4, 0.6, 0.8].iter().filter(|&&t| p[b + 2] >= t).count();
    let (d2, factor) = match shape {
        0 => (2.0 * d / (1.0 + 0.5 * d), 1.0 + 0.5 * d),
        1 => (2.0 * d / (1.0 + d), 1.0 + d),
        2 => (2.0 * d, 1.0),
        _ => (d, 1.0),
    };
    let warn = (d2 > 1.0).then(|| format!("LFO {} depth {:.2} reaches below 0 in Tunefish 3; clamped", k + 1, p[b + 1]));
    Lfo { shape: shape as f32 / 4.0, depth: d2.min(1.0).sqrt(), factor, hz: 50.0 * p[b] * p[b] / TAU, warn }
}

/// Things the unified instrument cannot reproduce, one line each.
fn warn(w: &mut Vec<String>, m: String) {
    if !w.contains(&m) {
        w.push(m);
    }
}

/// The unified slots of the instrument: point oscillator, its sub-oscillator passes, PAD table.
#[derive(Clone, Debug, Default)]
struct Slots {
    points: Option<usize>,
    subs: Vec<usize>,
    pad: Option<usize>,
}

/// Oscillator passes: 2012 `fi(p) + 1` (1 or 2), the 2011 engine `fi(5p) + 1` (1..6).
fn passes(p: &[f32], early: bool) -> usize {
    (fi(if early { 5.0 } else { 1.0 } * p[OSC_SUBOSC]).max(0) + 1) as usize
}

fn slots(p: &[f32], early: bool, w: &mut Vec<String>) -> Slots {
    let points = (p[OSC_VOL] > 0.01).then_some(0);
    let pad_on = p[ADD_VOLUME] > 0.0;
    let want = if points.is_some() { passes(p, early) - 1 } else { 0 };
    let free = p3::OSC_SLOTS - points.iter().count() - usize::from(pad_on);
    if want > free {
        warn(w, format!("{} sub-oscillator passes: {} slot(s) left, the lowest passes are dropped", want + 1, free));
    }
    let subs: Vec<usize> = (1..=want.min(free)).collect();
    let pad = pad_on.then_some(points.iter().count() + subs.len());
    Slots { points, subs, pad }
}

/// The unified instrument values of Tunefish 3 parameters `p` (already quantised in player mode).
pub fn build(p: &[f32], mode: Mode, w: &mut Vec<String>) -> [f32; p3::COUNT] {
    assert_eq!(p.len(), COUNT);
    let editor = mode == Mode::Editor;
    let early = mode == Mode::Early;
    let env = env_slots(p);
    let s = slots(p, early, w);
    let osc_slots: Vec<usize> = s.points.iter().chain(&s.subs).copied().collect();
    // pass k's gain: 2012 1/(k + 1), the 2011 engine halves it each pass
    let pass_gain = |k: usize| if early { 0.5f32.powi(k as i32) } else { 1.0 / (k + 1) as f32 };
    let fixed = p[OSC_FREQ] >= 0.00001;
    let fixed_hz = 441.0 * p[OSC_FREQ] * p[OSC_FREQ];
    let octave = |x: f32| 4 - fi(8.0 * x).clamp(0, 8);

    // ---- the old-layout part: voice, slot frames, noise, filters, LFOs, envelopes ----
    let mut u = Patch::new();
    u.set(v::MASTER, inv::inv_gain(tfsynth::params::gain_curve(p[GAIN])));
    u.set(v::CAPACITY, inv::inv_count(fi(15.0 * p[OSC_POLY] + 1.0).clamp(1, 16) as u32, 1, 64));
    u.set(v::VELOCITY, 0.0);
    u.set(v::ENV1_AT_OUTPUT, 0.0);
    if p[OSC_GLIDE] > 0.0 {
        u.set(v::GLIDE, inv::inv_time((p[OSC_GLIDE] * 10.0 + 1.0) * BLOCK_S));
    }
    let (pan, comp) = crate::tf::pan(p[OSC_PAN]);
    for (k, &i) in osc_slots.iter().enumerate() {
        u.osc_type(i, osc_type::PM); // the type and shapes are set in the v3 layout below
        u.osc(i, o::TRACKING, if fixed { 1.0 } else { 0.0 });
        u.osc(i, o::FIXED_HZ, inv::inv_fixed_hz(if k == 0 { fixed_hz } else { fixed_hz * 0.5f32.powi(k as i32) }));
        u.osc(i, o::OCTAVE, inv::inv_octave((octave(p[OSC_OCTAVE]) - k as i32) as f32));
        u.osc(i, o::GAIN, inv::inv_gain(comp * p[OSC_VOL] * pass_gain(k)));
        u.osc(i, o::PAN, pan);
    }
    if let Some(i) = s.pad {
        u.osc_type(i, osc_type::PM);
        u.osc(i, o::TRACKING, if fixed { 1.0 } else { 0.0 });
        u.osc(i, o::FIXED_HZ, inv::inv_fixed_hz(fixed_hz));
        u.osc(i, o::OCTAVE, inv::inv_octave(octave(p[ADD_OCTAVE]) as f32));
        // both channels at full level: the equal-power centre times √2
        u.osc(i, o::GAIN, inv::inv_gain(SQRT_2 * p[ADD_VOLUME]));
        u.osc(i, o::PAN, 0.5);
    }
    u.set(v::NOISE_LEVEL, inv::inv_gain(SQRT_2 * p[NOISE_ON]));
    let (nf, bw) = (p[NOISE_FREQ], p[NOISE_BW]);
    u.set(po::NOISE + n::BAND_ON, if bw < 0.99 { 1.0 } else { 0.0 });
    u.set(po::NOISE + n::HP, crate::tf::cutoff_p(crate::tf::hz(nf - bw, 10_000.0)));
    u.set(po::NOISE + n::LP, crate::tf::cutoff_p(crate::tf::hz(nf + bw, 20_000.0)));
    let filters = [
        (LP_ON, filter_type::LADDER_LP, 20_000.0, true),
        (HP_ON, filter_type::RBJ_HP, 10_000.0, true),
        (BP_ON, filter_type::RBJ_BP, 10_000.0, editor),
        (NT_ON, filter_type::NOTCH, 10_000.0, editor),
    ];
    for (i, (on, kind, span, kept)) in filters.into_iter().enumerate() {
        let used = p[on] > 0.5;
        if used && !kept {
            warn(w, format!("{} filter: not in the Tunefish 3 player; dropped", ["low-pass", "high-pass", "band-pass", "notch"][i]));
        }
        if used && kept && i == 0 && p[on + 2] > 0.85 {
            warn(w, format!("low-pass resonance {:.2}: the unified ladder stops at 0.85", p[on + 2]));
        }
        let kind = if used && kept { kind } else { filter_type::OFF };
        u.filter(i, kind, crate::tf::cutoff_p(crate::tf::hz(p[on + 1], span)), p[on + 2].min(0.95));
    }
    let mut lfo_factor = [1.0f32; 2];
    for k in 0..2 {
        let b = LFO1 + 4 * k;
        let lf = lfo(p, k, early);
        if let Some(m) = lf.warn {
            warn(w, m);
        }
        lfo_factor[k] = lf.factor;
        let at = |f: usize| po::lfo(k, f);
        u.set(at(l::SHAPE), lf.shape);
        u.set(at(l::RATE), inv::inv_lfo_rate(lf.hz));
        u.set(at(l::DEPTH), lf.depth);
        u.set(at(l::SYNC), if p[b + 3] > 0.5 { 1.0 } else { 0.0 });
    }
    for k in 0..2 {
        let b = ENV1 + 5 * k;
        let at = |f: usize| po::env(env[k], f);
        u.set(at(e::MODE), 1.0);
        u.set(at(e::ATTACK), p[b]);
        u.set(at(e::DECAY), p[b + 1]);
        u.set(at(e::SUSTAIN), p[b + 2]);
        u.set(at(e::RELEASE), p[b + 3]);
        u.set(at(e::SLOPE), p[b + 4]);
    }
    if volume_adsr(p).is_none() {
        u.env(0, 0.0, 0.0, 1.0, inv::inv_env_ms(gate_ms(p)));
        if s.pad.is_some() {
            warn(w, "PAD table without an envelope on volume: Tunefish 3 lets it sound until the voice is stolen; it ends with the note's gate".into());
        }
    }
    let mut x = unisynth::migrate::values(&u.finish()).expect("a built old-layout patch migrates");
    // Tunefish 3's form of the RBJ and notch types (form 1): its shelf-slope band-pass and RBJ notch, and resonance up to
    // 0.95 (the high-pass only needs it above 0.85, where the laws part)
    for (i, on) in [(1, HP_ON), (2, BP_ON), (3, NT_ON)] {
        let kept = i == 1 || editor;
        if p[on] > 0.5 && kept && (i > 1 || p[on + 2] > 0.85) {
            x[p3::filt(i, p3::f::FORM)] = 1.0;
        }
    }

    // ---- the v3 part: slot types, shapes, point fields ----
    let det = |hz: f32| (hz / 10.0).cbrt(); // the unified layer detune law is 10p³ Hz
    let det_p = if fixed { 0.0 } else { det(8.82 * p[OSC_DETUNE].powi(3)) };
    for (k, &i) in osc_slots.iter().enumerate() {
        x[p3::osc(i, p3::o::TYPE)] = p3::osc_type::encode(p3::osc_type::POINTS);
        for j in 0..4 {
            x[p3::osc(i, p3::o::SHAPE + sp::PTS_POINT + 2 * j)] = p[OSC_P1 + 3 * j].clamp(0.0, 1.0);
            x[p3::osc(i, p3::o::SHAPE + sp::PTS_POINT + 2 * j + 1)] = p[OSC_P1 + 3 * j + 1].clamp(0.0, 1.0);
            x[p3::pts(i, pt::MODE + j)] = p3::choice_value(fi(2.0 * p[OSC_P1 + 3 * j + 2]).clamp(0, 2) as usize, 3);
        }
        x[p3::pts(i, pt::MODE + 4)] = p3::choice_value(fi(2.0 * p[OSC_FI_INT]).clamp(0, 2) as usize, 3);
        // the unified drive law is 1 + 32p³; the 2011 engine's is 1 + 32p
        let drive = p[OSC_DRIVE].clamp(0.0, 1.0);
        x[p3::osc(i, p3::o::SHAPE + sp::PTS_DRIVE)] = if early { drive.cbrt() } else { drive };
        x[p3::pts(i, pt::COUNT_)] = inv::inv_count((fi(3.0 * p[OSC_POINTS]) + 1).clamp(1, 4) as u32, 1, 4);
        x[p3::pts(i, pt::LAYERS)] = inv::inv_count((layers(p, early) as i32).clamp(1, 11) as u32, 1, 11);
        // each sub pass plays at (f ± D)·(0.5 ∓ D): about half the previous pass's detune
        x[p3::pts(i, pt::DETUNE)] = if k == 0 { det_p } else { det_p * 0.5f32.powi(k as i32).cbrt() };
        x[p3::pts(i, pt::SPREAD)] = (4.41 * p[OSC_SPREAD].powi(4) / 10.0).powf(0.25); // 10p⁴ Hz
    }
    if let Some(i) = s.pad {
        x[p3::osc(i, p3::o::TYPE)] = p3::osc_type::encode(p3::osc_type::PAD);
        for k in 0..p3::o::SHAPES {
            x[p3::osc(i, p3::o::SHAPE + k)] = 0.0;
        }
        x[p3::osc(i, p3::o::SHAPE + sp::PAD_BANDWIDTH)] = p[ADD_BANDW].clamp(0.0, 1.0);
        x[p3::osc(i, p3::o::SHAPE + sp::PAD_SCALE)] = p[ADD_SCALE].clamp(0.0, 1.0);
        x[p3::osc(i, p3::o::SHAPE + sp::PAD_DAMP)] = p[ADD_DAMP].clamp(0.0, 1.0);
        x[p3::osc(i, p3::o::SHAPE + sp::PAD_HARMONICS)] = p[ADD_HARM].clamp(0.0, 1.0);
        x[p3::osc(i, p3::o::SHAPE + sp::PAD_PROFILE)] = p3::choice_value(fi(3.0 * p[ADD_PROFILE]).clamp(0, 3) as usize, 4);
        x[p3::osc(i, p3::o::SHAPE + sp::PAD_DRIVE)] = p[ADD_DRIVE].clamp(0.0, 1.0);
        x[p3::pts(i, pt::DETUNE)] = det_p;
    }
    if early && s.points.is_some() {
        early_differences(p, osc_slots.len(), w);
    }
    let slop = 8.0 * p[OSC_SLOP].powi(3);
    if slop > 1.0 {
        warn(w, format!("OscSlop {:.2} (up to ±{slop:.1} Hz of random detune per note) is not converted", p[OSC_SLOP]));
    }

    // ---- mod routes, in Tunefish's slot order, as v3 Scale slots ----
    let mut routes: Vec<(usize, u8, f32)> = vec![];
    for i in 0..10 {
        let (src, amount, dst) = slot(p, i);
        if src == 0 || dst == 0 || src == 9 {
            continue; // no source, no target, or the wheel (never fed): the slot's result is 1
        }
        let usrc = match src {
            1 | 2 => source::LFO1 + (src - 1) as usize,
            3 | 4 => source::ENV1 + env[(src - 3) as usize],
            _ if editor => {
                // MOD1–4 hold 1 until a note's effect word sets them (spec §3.9): the controller starts there
                x[p3::cc(2 + (src - 5) as usize)] = 1.0;
                source::CC2 + (src - 5) as usize
            }
            _ => continue, // MOD1–4: compiled out of the player
        };
        let a = amount * if (1..=2).contains(&src) { lfo_factor[(src - 1) as usize] } else { 1.0 };
        let shp = |slot: usize, k: usize| target::osc(slot, target::OSC_SHAPE + k as u8);
        let tg: Vec<u8> = match dst {
            1..=8 => osc_slots.iter().map(|&j| shp(j, (dst - 1) as usize)).collect(),
            9 => osc_slots.iter().chain(s.pad.iter()).map(|&j| target::osc(j, target::OSC_GAIN)).collect(),
            // a fixed frequency is (p·M)²/100: the route acts squared, so it runs twice (Scale results multiply)
            10 => osc_slots.iter().flat_map(|&j| std::iter::repeat_n(target::osc(j, target::OSC_PITCH), if fixed { 2 } else { 1 })).collect(),
            11 => osc_slots.iter().map(|&j| target::osc(j, target::OSC_PAN)).collect(),
            12 | 13 => {
                warn(w, format!("mod slot {}: a route to {} is not converted", i + 1, if dst == 12 { "detune" } else { "spread" }));
                vec![]
            }
            14 if early => {
                warn(w, format!("mod slot {}: a drive route (2011 engine: 1 + 32·p·M) is not converted", i + 1));
                vec![]
            }
            14 => osc_slots
                .iter()
                .map(|&j| shp(j, sp::PTS_DRIVE))
                .chain(s.pad.iter().map(|&j| shp(j, sp::PAD_DRIVE)))
                .collect(),
            15 => vec![target::NOISE_LEVEL],
            16..=19 => vec![if dst % 2 == 0 { target::filter_cutoff(((dst - 16) / 2) as usize) } else { target::filter_resonance(((dst - 16) / 2) as usize) }],
            20..=23 if editor => vec![if dst % 2 == 0 { target::filter_cutoff(((dst - 16) / 2) as usize) } else { target::filter_resonance(((dst - 16) / 2) as usize) }],
            20..=23 => vec![],
            24 | 25 => vec![target::ENV_DECAY + env[(dst - 24) as usize] as u8],
            _ => vec![],
        };
        if a > 10.0 && !tg.is_empty() {
            warn(w, format!("mod slot {}: amount {a:.1} is above the unified 10; clamped", i + 1));
        }
        routes.extend(tg.into_iter().map(|t| (usrc, t, a)));
    }
    // velocity (note-on vel/128) scales the oscillator slots and the noise, not the PAD table
    for &j in &osc_slots {
        routes.push((source::VELOCITY, target::osc(j, target::OSC_GAIN), 1.0));
    }
    routes.push((source::VELOCITY, target::NOISE_LEVEL, 1.0));
    if routes.len() > p3::MOD_SLOTS {
        warn(w, format!("{} mod routes for {} slots; the last ones are dropped", routes.len(), p3::MOD_SLOTS));
        routes.truncate(p3::MOD_SLOTS);
    }
    for (k, (src, tgt, a)) in routes.into_iter().enumerate() {
        x[p3::modslot(k, p3::m::SOURCE)] = p3::choice_value(src, source::COUNT);
        x[p3::modslot(k, p3::m::TARGET)] = inv::inv_index(tgt);
        x[p3::modslot(k, p3::m::AMOUNT)] = inv::inv_scale_amount(a);
        x[p3::modslot(k, p3::m::OPERATION)] = p3::choice_value(op::SCALE, op::COUNT);
    }
    x
}

/// Unison layers: 2012 `fi(10p) + 1`, the 2011 engine `fi(15p) + 1`.
fn layers(p: &[f32], early: bool) -> usize {
    (fi(if early { 15.0 } else { 10.0 } * p[OSC_UNISON]).max(0) + 1) as usize
}

/// The 2011 engine's oscillator behaviour the unified synth cannot play (known differences, spec §10.4), one warning
/// each.
fn early_differences(p: &[f32], passes: usize, w: &mut Vec<String>) {
    let offset = 2.0 * (p[OSC_OFFSET] - 0.5);
    if offset.abs() >= 0.01 {
        warn(w, format!("2011 engine: oscillator offset {offset:+.2} before the drive, clipped at +1 only, is not played"));
    }
    if layers(p, true) * passes > 1 {
        warn(w, "2011 engine: each unison layer and sub pass is clipped on its own; the unified synth clips their sum".into());
    }
    warn(w, "2011 engine: the point curve is evaluated per sample with its mean kept; played from the 257-sample table".into());
    warn(w, "2011 engine: no decimation stage; the unified point slot runs one".into());
}

/// The insert chain as unified effect devices in slot order (Tunefish 4 modes: the same effects and parameter names).
/// Player mode drops chorus, flanger and EQ, which the Tunefish 3 player leaves out.
pub fn insert_chain(p: &[f32], mode: Mode, w: &mut Vec<String>) -> Vec<Dev> {
    use tfsynth::params as tp;
    let mut p4 = vec![0.0f32; tp::COUNT];
    let names4: Vec<&str> = tp::NAMES.to_vec();
    let names3 = crate::e3prj::names();
    for (k, name) in names3.iter().enumerate().skip(108) {
        if let Some(j) = names4.iter().position(|n| n == name) {
            p4[j] = p[k];
        }
    }
    for i in 0..10 {
        let kind = fi(10.0 * p[EFFECT + i]);
        let editor_only = matches!(kind, 3 | 4 | 7);
        if editor_only && mode != Mode::Editor {
            warn(w, format!("effect slot {}: {} is not in the Tunefish 3 player; dropped", i + 1, ["", "", "", "chorus", "flanger", "", "", "EQ"][kind as usize]));
        }
        p4[tp::EFFECT_FIRST + i] = if editor_only && mode != Mode::Editor { 0.0 } else { kind as f32 / 10.0 };
    }
    // Tunefish 3's chorus is one tap per channel at its own law: the chorus kind's one-tap mode (change add-tf3-import)
    let chorus = tfsynth::device::kind::INSTRUMENT + 3;
    crate::tf::insert_chain(&p4)
        .into_iter()
        .map(|(k, d)| if k == chorus { chorus_device(p) } else { d })
        .collect()
}

/// The one-tap chorus device (kind 69, mode 1) of `ChrsRate`, `ChrsDep`, `ChrsGain`.
fn chorus_device(p: &[f32]) -> Dev {
    use unisynth::kind;
    let at = |n: &str| p[crate::e3prj::index(n).expect("a Tunefish 3 parameter")];
    let vals = [unisynth::fx::mode_value(kind::CHORUS, 1), at("ChrsRate"), at("ChrsDep"), at("ChrsGain")];
    let mut chunk: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
    chunk.extend(unisynth::fx::TAG);
    Dev { kind: kind::CHORUS, chunk }
}

/// The voice capacity: `round(15p + 1)`, 1..=16.
pub fn capacity(p: &[f32]) -> u32 {
    fi(15.0 * p[OSC_POLY] + 1.0).clamp(1, 16) as u32
}

/// The unified instrument device of Tunefish 3 parameters `p` (player mode quantises them first).
pub fn instrument_device(p: &[f32], mode: Mode, w: &mut Vec<String>) -> Dev {
    let x = build(p, mode, w);
    Dev { kind: unisynth::kind::INSTRUMENT, chunk: unisynth::chunk::encode(&x, &[]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(pairs: &[(&str, f32)]) -> Vec<f32> {
        let mut p = vec![0.0f32; COUNT];
        for &(n, x) in pairs {
            p[crate::e3prj::index(n).unwrap_or_else(|| panic!("{n}"))] = x;
        }
        p
    }

    #[test]
    fn the_index_constants_are_the_names() {
        let n = crate::e3prj::names();
        for (i, name) in [
            (OSC_POINTS, "OscPoints"), (OSC_P1, "OscP1Off"), (OSC_FI_INT, "OscFiInt"), (OSC_VOL, "OscVol"), (OSC_SUBOSC, "OscSubOsc"),
            (OSC_SLOP, "OscSlop"), (ADD_VOLUME, "AddVolume"), (ADD_DRIVE, "AddDrive"), (NOISE_BW, "NoiseBw"), (NT_ON, "FltNTOn"),
            (ENV1, "Env1Att"), (LFO1, "Lfo1Rate"), (MM, "Mm1Source"), (EFFECT, "Effect1"), (GAIN, "GainAmount"), (124, "FrmtType"),
        ] {
            assert_eq!(n[i], name);
        }
    }

    #[test]
    fn quantise_reads_back_the_stored_byte() {
        let p = quantise(&named(&[("OscVol", 0.123), ("OscUnisn", 0.5), ("Effect1", 0.52), ("OscP2Int", 0.6)]));
        let at = |n: &str| p[crate::e3prj::index(n).unwrap()];
        assert_eq!((at("OscVol"), at("OscUnisn"), at("Effect1"), at("OscP2Int")), (0.12, 4.0 / 9.0, 0.5, 0.5));
    }

    #[test]
    fn a_sub_oscillator_doubles_the_point_slot_and_its_routes() {
        // ADSR 1 → point 2 value (target 4) and → volume (target 9)
        let p = named(&[
            ("OscVol", 0.5), ("OscSubOsc", 1.0), ("OscPoints", 1.0 / 3.0), ("OscP2Val", 0.7), ("AddVolume", 0.3),
            ("Mm1Source", 3.0 / 9.0), ("Mm1Mod", 0.55), ("Mm1Target", 4.0 / 25.0),
            ("Mm2Source", 3.0 / 9.0), ("Mm2Mod", 0.55), ("Mm2Target", 9.0 / 25.0),
        ]);
        let mut w = vec![];
        let x = build(&p, Mode::Player, &mut w);
        let ty = |i: usize| p3::osc_type::decode(x[p3::osc(i, p3::o::TYPE)]);
        assert_eq!([ty(0), ty(1), ty(2)], [p3::osc_type::POINTS, p3::osc_type::POINTS, p3::osc_type::PAD]);
        let shape = |i: usize, k: usize| x[p3::osc(i, p3::o::SHAPE + k)];
        assert_eq!((shape(0, 3), shape(1, 3)), (0.7, 0.7), "the same points");
        assert_eq!(unisynth::law::octave(x[p3::osc(1, p3::o::OCTAVE)]), unisynth::law::octave(x[p3::osc(0, p3::o::OCTAVE)]) - 1.0);
        let gain = |i: usize| unisynth::law::gain(x[p3::osc(i, p3::o::GAIN)]);
        assert!((gain(1) / gain(0) - 0.5).abs() < 1e-3, "half gain: {} {}", gain(0), gain(1));
        let targets: Vec<u8> = (0..p3::MOD_SLOTS).filter(|&k| x[p3::modslot(k, p3::m::SOURCE)] > 0.0).map(|k| unisynth::law::index(x[p3::modslot(k, p3::m::TARGET)])).collect();
        let shp = |j: usize, k: u8| target::osc(j, target::OSC_SHAPE + k);
        for t in [shp(0, 3), shp(1, 3), target::osc(0, target::OSC_GAIN), target::osc(1, target::OSC_GAIN), target::osc(2, target::OSC_GAIN)] {
            assert!(targets.contains(&t), "{t} in {targets:?}");
        }
        assert!(!targets.contains(&shp(2, 3)), "the PAD table has no points");
    }

    #[test]
    fn player_mode_drops_editor_modules_with_warnings() {
        let p = named(&[("OscVol", 0.5), ("FltBPOn", 1.0), ("Effect2", 0.3), ("Effect3", 0.5), ("Mm1Source", 5.0 / 9.0), ("Mm1Mod", 0.55), ("Mm1Target", 16.0 / 25.0)]);
        let mut w = vec![];
        let fx = insert_chain(&p, Mode::Player, &mut w);
        let x = build(&p, Mode::Player, &mut w);
        assert_eq!(fx.len(), 1, "the reverb stays, the chorus goes");
        assert!(w.iter().any(|m| m.contains("effect slot 2: chorus")), "{w:?}");
        assert!(w.iter().any(|m| m.contains("band-pass filter")), "{w:?}");
        assert_eq!(p3::filter_type::decode(x[p3::filt(2, p3::f::TYPE)]), p3::filter_type::OFF);
        let mut w2 = vec![];
        let fx2 = insert_chain(&p, Mode::Editor, &mut w2);
        let x2 = build(&p, Mode::Editor, &mut w2);
        assert_eq!(fx2.len(), 2, "editor mode keeps the chorus");
        assert!(x2[p3::modslot(0, p3::m::SOURCE)] == p3::choice_value(source::CC2, source::COUNT), "MOD1 → CC2");
    }

    #[test]
    fn the_chorus_is_the_one_tap_mode_with_tunefish_3_values() {
        let p = named(&[("OscVol", 0.5), ("Effect1", 0.3), ("ChrsRate", 0.2), ("ChrsDep", 0.4), ("ChrsGain", 0.85)]);
        let mut w = vec![];
        let fx = insert_chain(&p, Mode::Editor, &mut w);
        assert_eq!(fx.len(), 1);
        assert_eq!(fx[0].kind, unisynth::kind::CHORUS);
        assert_eq!(unisynth::fx::mode_of(unisynth::kind::CHORUS, &fx[0].chunk), 1);
        let v = |i: usize| f32::from_le_bytes(fx[0].chunk[4 * i..4 * i + 4].try_into().unwrap());
        assert_eq!([v(1), v(2), v(3)], [0.2, 0.4, 0.85]);
    }

    #[test]
    fn a_mod_input_starts_at_1_until_a_note_sets_it() {
        // MOD3 → volume at amount 1: Tunefish 3 holds MOD1–4 at 1, so the route changes nothing until an effect word
        let p = named(&[("OscVol", 0.5), ("Mm1Source", 0.75), ("Mm1Mod", 0.55), ("Mm1Target", 0.36)]);
        let mut w = vec![];
        let x = build(&p, Mode::Editor, &mut w);
        assert_eq!(x[p3::modslot(0, p3::m::SOURCE)], p3::choice_value(source::CC2 + 2, source::COUNT), "MOD3 → CC4");
        assert_eq!(x[p3::cc(4)], 1.0, "CC4 starts at 1");
        assert_eq!(x[p3::cc(2)], p3::DEFAULTS[p3::cc(2)], "unused controllers keep their default");
    }

    #[test]
    fn lfo_shapes_keep_tunefish_3_values() {
        // for each shape and depth, A'·(1 − d' + d'·s) equals d·s₃ + 1 − d over the unified shape s
        for (shape_p, s3) in [(0.0f32, (|s: f32| 2.0 * s - 0.5) as fn(f32) -> f32), (0.25, |s| 2.0 * s), (0.5, |s| 2.0 * s - 1.0), (0.75, |s| s)] {
            for depth in [0.3f32, 0.5, 0.7] {
                let p = named(&[("Lfo1Shp", shape_p), ("Lfo1Dep", depth)]);
                let l = lfo(&p, 0, false);
                if l.warn.is_some() {
                    continue;
                }
                let (dp, factor) = (l.depth, l.factor);
                let (d, d2) = (depth * depth, dp * dp);
                for s in [0.0f32, 0.3, 0.8, 1.0] {
                    // the unified ramp down is 1 − ph; Tunefish 3's is 1 − 2·ph: in the unified shape's terms 2s − 1
                    let want = d * s3(s) + 1.0 - d;
                    let got = factor * (1.0 - d2 + d2 * s);
                    assert!((want - got).abs() < 1e-5, "shape {shape_p} depth {depth} s {s}: {want} {got}");
                }
            }
        }
    }

    // ---- the 2011 engine (Pandora; change add-tf3-pandora, spec §10.4) ----

    #[test]
    fn early_unison_is_15p_plus_1_layers() {
        let p = named(&[("OscVol", 0.5), ("OscUnisn", 0.33)]);
        let x = build(&p, Mode::Early, &mut vec![]);
        assert_eq!(unisynth::law::count(x[p3::pts(0, pt::LAYERS)], 1, 11), 6, "fi(4.95) + 1");
        let x12 = build(&p, Mode::Player, &mut vec![]);
        assert_eq!(unisynth::law::count(x12[p3::pts(0, pt::LAYERS)], 1, 11), 4, "2012: fi(3.3) + 1");
    }

    #[test]
    fn early_sub_passes_are_5p_plus_1_at_halving_gain() {
        let p = named(&[("OscVol", 0.5), ("OscSubOsc", 0.33), ("OscOctave", 0.5)]);
        let mut w = vec![];
        let x = build(&p, Mode::Early, &mut w);
        let ty = |i: usize| p3::osc_type::decode(x[p3::osc(i, p3::o::TYPE)]);
        assert_eq!([ty(0), ty(1), ty(2)], [p3::osc_type::POINTS; 3], "three passes");
        let gain = |i: usize| unisynth::law::gain(x[p3::osc(i, p3::o::GAIN)]);
        let oct = |i: usize| unisynth::law::octave(x[p3::osc(i, p3::o::OCTAVE)]);
        assert!((gain(1) / gain(0) - 0.5).abs() < 1e-3 && (gain(2) / gain(0) - 0.25).abs() < 1e-3, "{} {} {}", gain(0), gain(1), gain(2));
        assert_eq!([oct(1) - oct(0), oct(2) - oct(0)], [-1.0, -2.0]);
        // 2012: OscSubOsc 0.33 rounds to off
        let x12 = build(&p, Mode::Player, &mut vec![]);
        assert_eq!(p3::osc_type::decode(x12[p3::osc(1, p3::o::TYPE)]), p3::osc_type::OFF);
    }

    #[test]
    fn early_sub_passes_beyond_the_free_slots_warn() {
        let p = named(&[("OscVol", 0.5), ("OscSubOsc", 1.0)]); // 6 passes, 3 slots
        let mut w = vec![];
        build(&p, Mode::Early, &mut w);
        assert!(w.iter().any(|m| m.contains("6 sub-oscillator passes")), "{w:?}");
    }

    #[test]
    fn early_drive_is_linear() {
        let p = named(&[("OscVol", 0.5), ("OscDrive", 0.09)]);
        let x = build(&p, Mode::Early, &mut vec![]);
        let d = x[p3::osc(0, p3::o::SHAPE + sp::PTS_DRIVE)];
        assert!((32.0 * d.powi(3) - 32.0 * 0.09).abs() < 1e-4, "1 + 32p: {d}");
    }

    #[test]
    fn early_lfo_sine_is_half_plus_half_depth_sine_at_25p2_hz() {
        // 0.5 + 0.5·a·sin = 0.5 + 0.5a(2s − 1) over the unified sine s
        for a in [0.1f32, 0.5, 0.83, 1.0] {
            let p = named(&[("Lfo1Rate", 0.2), ("Lfo1Dep", a), ("Lfo1Shp", 0.0)]);
            let l = lfo(&p, 0, true);
            assert_eq!(l.shape, 0.0);
            assert!(l.warn.is_none(), "never below 0");
            assert!((l.hz - 25.0 * 0.04).abs() < 1e-5, "{}", l.hz);
            let d2 = l.depth * l.depth;
            for s in [0.0f32, 0.3, 1.0] {
                let (want, got) = (0.5 + 0.5 * a * (2.0 * s - 1.0), l.factor * (1.0 - d2 + d2 * s));
                assert!((want - got).abs() < 1e-5, "a {a} s {s}: {want} {got}");
            }
        }
        // the ramps ignore the depth: falling below 0.66, rising above, at 50p²/π Hz
        let p = named(&[("Lfo1Rate", 0.2), ("Lfo1Dep", 0.3), ("Lfo1Shp", 0.5)]);
        let l = lfo(&p, 0, true);
        assert_eq!((l.shape, l.depth, l.factor), (0.5, 1.0, 1.0));
        assert!((l.hz - 50.0 * 0.04 / std::f32::consts::PI).abs() < 1e-5);
        assert_eq!(lfo(&named(&[("Lfo1Shp", 0.75)]), 0, true).shape, 0.25);
    }

    #[test]
    fn early_known_differences_are_named() {
        let p = named(&[("OscVol", 0.5), ("OscUnisn", 0.11), ("OscOffset", 0.79)]);
        let mut w = vec![];
        build(&p, Mode::Early, &mut w);
        for k in ["offset +0.58", "each unison layer", "257-sample table", "decimation"] {
            assert!(w.iter().any(|m| m.contains(k)), "{k}: {w:?}");
        }
        let mut w12 = vec![];
        build(&p, Mode::Player, &mut w12);
        assert!(!w12.iter().any(|m| m.contains("2011")), "{w12:?}");
    }

    #[test]
    fn the_gate_lasts_the_longest_release_in_use() {
        // no routes: the gate closes in the key-up's block; an ADSR read by the matrix extends it
        assert!((gate_ms(&named(&[])) - FADE_CUT * BLOCK_S * 1000.0).abs() < 1e-3);
        let p = named(&[("Env1Sus", 0.5), ("Env1Rel", 0.5), ("Mm1Source", 3.0 / 9.0), ("Mm1Mod", 0.55), ("Mm1Target", 16.0 / 25.0)]);
        assert!(gate_ms(&p) > 50.0, "{}", gate_ms(&p));
    }
}
