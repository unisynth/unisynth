//! Instrument mappings (design D10): a legacy WaveSabre instrument's parameters become one unified patch through
//! the physical values of each parameter (frequencies, times, ratios, levels), not by copying normalized values.
//!
//! `build` is a pure function of the legacy parameter vector, so automation is converted by evaluating it with one
//! parameter changed (`auto.rs`). `Cx::automated` tells it which legacy parameters move during the song: a mod route
//! whose legacy parameter is neutral and never moves is left out, an oscillator that is silent and never rises is off.

use crate::legacy::Legacy;
use crate::patch::Patch;
use std::f32::consts::TAU;
use unisynth::law::host as inv;
use unisynth::modmx::{source, target};
use unisynth::params::{filter_type, osc_type, shape, v};
use wavesabre::kind as w;

/// Unified parameters that are a choice among `n` although their law says `Shape` (the sampler's loop mode, loop
/// boundary, reverse and interpolation), for the automation converter.
pub fn discrete_shapes(kind: u8) -> Vec<(usize, usize)> {
    use unisynth::params::osc;
    if kind == tfsynth::device::kind::INSTRUMENT {
        return crate::tf::discrete_shapes();
    }
    if !matches!(kind, w::SPECIMEN | w::ADULTERY) {
        return vec![];
    }
    let s = |k| osc(0, unisynth::params::o::SHAPE + k);
    vec![(s(shape::SMP_LOOP_MODE), 3), (s(shape::SMP_BOUNDARY), 2), (s(shape::SMP_REVERSE), 2), (s(shape::SMP_INTERP), 2)]
}

/// What the builder needs besides the parameters.
pub struct Cx {
    /// Legacy parameters that an automation moves.
    pub automated: Vec<bool>,
    /// Voice capacity (1..=64).
    pub capacity: u32,
}

impl Cx {
    fn auto(&self, i: usize) -> bool {
        self.automated.get(i).copied().unwrap_or(false)
    }
    /// Whether a route driven by legacy parameter `i` is needed: not neutral, or the parameter moves.
    fn keep(&self, neutral: bool, i: usize) -> bool {
        !neutral || self.auto(i)
    }
}

/// The unified parameters of a legacy instrument (without the sample section).
pub fn build(l: &Legacy, cx: &Cx) -> Patch {
    match l.kind {
        w::FALCON => falcon(&l.params, cx),
        w::SLAUGHTER => slaughter(&l.params, cx),
        w::THUNDER => thunder(l, cx),
        w::SPECIMEN => sampler(l, cx, &SPECIMEN),
        w::ADULTERY => sampler(l, cx, &ADULTERY),
        tfsynth::device::kind::INSTRUMENT => crate::tf::build(&l.params, cx),
        k => unreachable!("kind {k} is not an instrument"),
    }
}

// ---- shared helpers ----

/// `(int)(15·p) + 1`, upstream's unison count.
fn unison(p: f32) -> u32 {
    ((p.max(0.0) * 15.0) as u32).saturating_add(1)
}

/// Upstream's `volume_to_scalar`, `(0.4·v)²`, as a unified gain law parameter times `k`: `16p² = k·(0.4v)²`.
fn master_p(v: f32, k: f32) -> f32 {
    inv::inv_gain(k * (0.4 * v) * (0.4 * v))
}

/// The common `SynthDevice` parameters (legacy indices): unison, detune, pan spread, voice mode, slide.
struct Common {
    unison: usize,
    detune: usize,
    pan: usize,
    mode: usize,
    slide: usize,
}

fn common(p: &mut Patch, l: &[f32], c: &Common, cx: &Cx) {
    p.set(v::UNISON, inv::inv_count(unison(l[c.unison]), 1, 16));
    p.set(v::UNI_DETUNE, l[c.detune]);
    p.set(v::UNI_SPREAD, l[c.pan]);
    // upstream: `(int)v == 1` is mono legato
    p.set(v::MODE, if l[c.mode] as i32 == 1 { 1.0 } else { 0.0 });
    p.set(v::SLIDE, l[c.slide]);
    p.set(v::CAPACITY, inv::inv_count(cx.capacity, 1, 64));
}

/// Vibrato and rise of Falcon/Slaughter (legacy indices). The legacy phase advances `freq` radians per second, so the
/// LFO runs at `freq / 2π` Hz and adds `amount` semitones; the rise is `24·rise` semitones of transposition.
fn vibrato(p: &mut Patch, l: &[f32], freq: usize, amount: usize, rise: usize, cx: &Cx) {
    let hz = ((l[freq] * l[freq]) + 0.1) * 70.0 / TAU;
    if cx.keep(l[amount] == 0.0, amount) {
        p.lfo_sine(0, hz);
        p.offset(source::LFO1, target::PITCH, l[amount]);
    }
    p.set(v::TRANSPOSE, inv::inv_transpose(24.0 * l[rise]));
}

/// The legacy SVF type decode `(int)(3p)` as a unified filter type.
fn svf_type(p: f32) -> usize {
    match (p * 3.0) as i32 {
        1 => filter_type::SVF_HP,
        2 => filter_type::SVF_BP,
        3 => filter_type::SVF_NOTCH,
        _ => filter_type::SVF_LP,
    }
}

/// Filter 1 of Slaughter and the samplers: SVF, cutoff and resonance as the legacy parameters (same laws), plus the
/// mod-envelope route `modenv · 19 980 Hz · (2m − 1)` from envelope 2.
fn svf_with_mod_env(p: &mut Patch, ty: f32, cutoff: f32, res: f32, m: f32, m_idx: usize, cx: &Cx) {
    p.filter(0, svf_type(ty), cutoff, res);
    if cx.keep(m == 0.5, m_idx) {
        p.offset(source::ENV1 + 1, target::filter_cutoff(0), 19_980.0 * (2.0 * m - 1.0));
    }
}

// ---- Falcon ----

fn falcon(l: &[f32], cx: &Cx) -> Patch {
    use unisynth::params::o;
    let mut p = Patch::new();
    p.set(v::ENV1_AT_OUTPUT, 0.0);
    p.set(v::VELOCITY, 0.0);
    // Falcon's random start phase is a fraction of a sample: its operators start in phase
    p.set(v::START_SPREAD, 0.0);
    p.set(v::MASTER, master_p(l[17], 1.0));
    common(&mut p, l, &Common { unison: 18, detune: 19, pan: 20, mode: 30, slide: 31 }, cx);
    vibrato(&mut p, l, 21, 22, 23, cx);
    // operator k = (first legacy parameter: wave blend, coarse, fine, feedback). Operator 1 is a modulator at gain
    // 13.25 that only feeds operator 2 (send = feed-forward²); operator 2 is the carrier.
    for (k, wf) in [(0usize, 0usize), (1, 9)] {
        let fb = l[wf + 3] * l[wf + 3] / 2.0;
        p.osc_type(k, osc_type::PM);
        p.osc(k, o::RATIO, l[wf + 1]);
        p.osc(k, o::RATIO_FINE, l[wf + 2]);
        p.shape(k, shape::PM_WAVEFORM, l[wf]);
        if k == 0 {
            p.osc(0, o::GAIN, inv::inv_gain(13.25));
            p.osc(0, o::MIX, 0.0);
            p.osc(0, o::FEEDBACK, inv::inv_feedback(fb));
            p.osc(0, o::PM_OUT, inv::inv_pm_out(l[4] * l[4]));
        } else {
            p.osc(1, o::GAIN, inv::inv_gain(1.0));
            p.osc(1, o::FEEDBACK, inv::inv_feedback(13.25 * fb));
        }
    }
    // envelope 1 = operator 2 (the carrier; it ends the voice), 2 = operator 1, 3 = pitch
    p.env(0, l[13], l[14], l[15], l[16]);
    p.env(1, l[5], l[6], l[7], l[8]);
    p.env(2, l[24], l[25], l[26], l[27]);
    p.scale(source::ENV1 + 1, target::osc(0, target::OSC_GAIN), 1.0);
    p.scale(source::ENV1, target::osc(1, target::OSC_GAIN), 1.0);
    for (k, idx) in [(0usize, 28usize), (1, 29)] {
        if cx.keep(l[idx] == 0.5, idx) {
            p.offset(source::ENV1 + 2, target::osc(k, target::OSC_PITCH), (l[idx] - 0.5) * 2.0 * 36.0);
        }
    }
    p
}

// ---- Slaughter ----

fn slaughter(l: &[f32], cx: &Cx) -> Patch {
    let mut p = Patch::new();
    p.set(v::VELOCITY, 0.0);
    // Slaughter draws its start phases in samples (±2π of them), not turns: unison voices start almost in phase
    p.set(v::START_SPREAD, inv::inv_start_spread(4.0 * std::f32::consts::PI));
    p.set(v::MASTER, master_p(l[28], 16.0));
    common(&mut p, l, &Common { unison: 29, detune: 30, pan: 31, mode: 40, slide: 41 }, cx);
    vibrato(&mut p, l, 32, 33, 34, cx);
    for i in 0..3 {
        let b = 5 * i;
        if !cx.keep(l[b + 2] == 0.0, b + 2) {
            continue;
        }
        use unisynth::params::o;
        p.osc_type(i, osc_type::BLIT);
        p.osc(i, o::COARSE, inv::inv_coarse((l[b + 3] * 24.99).floor()));
        p.osc(i, o::FINE, inv::inv_fine(l[b + 4]));
        p.osc(i, o::GAIN, inv::inv_gain(l[b + 2] * l[b + 2]));
        p.shape(i, shape::BLIT_WAVEFORM, l[b]);
        p.shape(i, shape::BLIT_PULSE_WIDTH, 1.0 - l[b + 1]);
    }
    p.set(v::NOISE_LEVEL, inv::inv_gain(l[15] * l[15]));
    p.set(unisynth::params::ext::NOISE_WIDTH, 0.0); // WaveSabre's noise is one signal on both channels
    svf_with_mod_env(&mut p, l[16], l[17], l[18], l[19], 19, cx);
    p.env(0, l[20], l[21], l[22], l[23]);
    p.env(1, l[24], l[25], l[26], l[27]);
    p.env(2, l[35], l[36], l[37], l[38]);
    if cx.keep(l[39] == 0.5, 39) {
        p.offset(source::ENV1 + 2, target::PITCH, (l[39] - 0.5) * 2.0 * 36.0);
    }
    p
}

// ---- samplers ----

/// Legacy parameter indices of the shared sample voice.
struct SmpMap {
    amp: usize,
    start: usize,
    loop_mode: usize,
    boundary: Option<usize>,
    loop_start: usize,
    loop_len: usize,
    reverse: usize,
    interp: usize,
    coarse: usize,
    fine: usize,
    ftype: usize,
    ffreq: usize,
    fres: usize,
    fmod: usize,
    modenv: usize,
    common: Common,
    master: usize,
    /// Adultery's wave index.
    index: Option<usize>,
}

const SPECIMEN: SmpMap = SmpMap {
    amp: 0, start: 4, loop_mode: 5, boundary: None, loop_start: 6, loop_len: 7, reverse: 8, interp: 9, coarse: 10,
    fine: 11, ftype: 12, ffreq: 13, fres: 14, fmod: 15, modenv: 16,
    common: Common { unison: 20, detune: 21, pan: 22, mode: 24, slide: 25 }, master: 23, index: None,
};

const ADULTERY: SmpMap = SmpMap {
    amp: 1, start: 5, loop_mode: 6, boundary: Some(7), loop_start: 8, loop_len: 9, reverse: 10, interp: 11,
    coarse: 12, fine: 13, ftype: 14, ffreq: 15, fres: 16, fmod: 17, modenv: 18,
    common: Common { unison: 22, detune: 23, pan: 24, mode: 26, slide: 27 }, master: 25, index: Some(0),
};

fn sampler(le: &Legacy, cx: &Cx, m: &SmpMap) -> Patch {
    use unisynth::params::o;
    let l = &le.params;
    let mut p = Patch::new();
    p.set(v::MASTER, master_p(l[m.master], 1.0));
    // velocity / 128 scales the output (sensitivity 1, the default); the envelope gates it
    common(&mut p, l, &m.common, cx);
    p.osc_type(0, osc_type::SAMPLER);
    let dls = m.index.is_some();
    let index = match m.index {
        Some(i) => l[i].max(0.0).floor(),
        None => le.sample.is_some() as u32 as f32,
    };
    p.sampler_source(0, dls, index);
    p.shape(0, shape::SMP_START, l[m.start]);
    // `(int)(2v)`: 0 off, 1 repeat, 2 ping-pong
    let lm = match (l[m.loop_mode] * 2.0) as i32 {
        1 => 1,
        2 => 2,
        _ => 0,
    };
    p.shape(0, shape::SMP_LOOP_MODE, inv::inv_choice(lm, 3));
    // Specimen's bounds are manual; Adultery's `(int)v`: 0 from the sample, else manual
    let manual = match m.boundary {
        None => true,
        Some(i) => l[i] as i32 != 0,
    };
    p.shape(0, shape::SMP_BOUNDARY, inv::inv_choice(manual as usize, 2));
    p.shape(0, shape::SMP_LOOP_START, l[m.loop_start]);
    p.shape(0, shape::SMP_LOOP_LENGTH, l[m.loop_len]);
    p.shape(0, shape::SMP_REVERSE, if l[m.reverse] >= 0.5 { 1.0 } else { 0.0 });
    // `(int)v`: 0 nearest, 1 linear; any other value plays silence upstream (the slot is muted)
    p.shape(0, shape::SMP_INTERP, if l[m.interp] as i32 == 0 { 0.0 } else { 1.0 });
    if !matches!(l[m.interp] as i32, 0 | 1) && !cx.auto(m.interp) {
        p.osc(0, o::GAIN, 0.0);
    }
    p.osc(0, o::COARSE, inv::inv_coarse(((l[m.coarse] * 2.0 - 1.0) * 12.0).floor()));
    p.osc(0, o::FINE, l[m.fine]);
    svf_with_mod_env(&mut p, l[m.ftype], l[m.ffreq], l[m.fres], l[m.fmod], m.fmod, cx);
    let a = m.amp;
    p.env(0, l[a], l[a + 1], l[a + 2], l[a + 3]);
    let e = m.modenv;
    p.env(1, l[e], l[e + 1], l[e + 2], l[e + 3]);
    p
}

// ---- Thunder ----

/// Plays its sample once from the start at unity pitch, mono to both sides, no envelope; note-off does nothing.
fn thunder(le: &Legacy, cx: &Cx) -> Patch {
    use unisynth::params::o;
    let mut p = Patch::new();
    p.set(v::VELOCITY, 0.0);
    p.set(v::ONE_SHOT, 1.0);
    // centred, the equal-power pan gives each side 1/√2; full level on both sides needs √2 more
    p.set(v::MASTER, inv::inv_gain(std::f32::consts::SQRT_2));
    p.set(v::CAPACITY, inv::inv_count(cx.capacity, 1, 64));
    p.osc_type(0, osc_type::SAMPLER);
    p.sampler_source(0, false, le.sample.is_some() as u32 as f32);
    // unity pitch whatever the note: fixed frequency at middle C
    p.osc(0, o::TRACKING, 1.0);
    p.osc(0, o::FIXED_HZ, inv::inv_fixed_hz(261.625_56));
    p.shape(0, shape::SMP_LOOP_MODE, 0.0);
    p.shape(0, shape::SMP_INTERP, 0.0);
    p.shape(0, shape::SMP_BOUNDARY, 0.0);
    p.env(0, 0.0, 0.0, 1.0, 0.0);
    p
}
