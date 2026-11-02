//! The Tunefish instrument (`tfsynth`, synth-mix kind 32 or a `.tfm` instrument) as one unified patch (design D10).
//!
//! The patch is a pure function of the 112 legacy values, like the WaveSabre mappings, so automation is converted by
//! evaluating it with one value changed. What the unified patch holds:
//!
//! * slot 1 is the wavetable generator with every `Gen*` value copied (the unified wavetable is the same generator),
//!   noise with its band, the four filters in Tunefish's fixed order (low-pass ladder, high-pass, band-pass, notch),
//!   two LFOs and the two ADSRs in rate mode, the voice limit as capacity;
//! * mod slots 1–8 are Scale slots at the same index with the same source and amount (Tunefish's matrix is
//!   multiplicative, so a slot's own-amount targets stay valid), plus two velocity routes (slot 9, 10);
//! * envelope 1 ends the voice: it is the ADSR routed to volume, or a gate when none is (the organ-gate rule: the note
//!   sounds while the key is held and until every envelope used as a source has finished);
//! * levels: the voice master is the gain curve, the oscillator carries `0.4 · GenVol` times the factor that turns the
//!   unified equal-power pan into Tunefish's linear balance (√2 at the centre), noise `NoiseOn · √2`.

use crate::instr::Cx;
use crate::model::Dev;
use crate::patch::{Op, Patch};
use std::f32::consts::{SQRT_2, TAU};
use tfsynth::env::{Adsr, Stage};
use tfsynth::modmx::{dst, LIVE};
use tfsynth::params as tp;
use unisynth::law::host as inv;
use unisynth::modmx::{source, target};
use unisynth::params::old::{filter_type, o, osc_type, shape, v, MOD_SLOTS};

/// The `GenUnis` unison layers of the wavetable slot (1..=10).
const LAYERS: usize = 10;
/// Where an abrupt cut inside the closing fade of a block has the energy of the linear fade: `∫(1 − t)² dt = 1/3`.
const FADE_CUT: f32 = 1.0 / 3.0;
/// Seconds of one control block.
const BLOCK_S: f32 = 512.0 / 44_100.0;
/// The longest gate release a unified WaveSabre-mode envelope can express (milliseconds).
const GATE_MAX_MS: f32 = 5001.0;

/// Unified parameters that are a choice among `n` although their law says `Shape` or `Ratio`, for the automation
/// converter: the wavetable's unison layers and the oscillator octave.
pub fn discrete_shapes() -> Vec<(usize, usize)> {
    vec![(unisynth::params::old::osc(0, o::SHAPE + shape::WT_LAYERS), LAYERS), (unisynth::params::old::osc(0, o::OCTAVE), 9)]
}

/// Which Tunefish ADSR a live mod slot reads, `None` for other sources.
fn adsr_of(s: u8) -> Option<usize> {
    matches!(s, 3 | 4).then(|| s as usize - 3)
}

/// The live slot `i` of the patch `p` as `(source, amount, target)` Tunefish ids.
fn slot(p: &[f32], i: usize) -> (u8, f32, u8) {
    let b = tp::MM_FIRST + 3 * i;
    (tp::mod_source(p[b]), p[b + 1], tp::mod_target(p[b + 2]))
}

/// The ADSR routed to volume, if any.
pub fn volume_adsr(p: &[f32]) -> Option<usize> {
    (0..LIVE).find_map(|i| {
        let (s, _, d) = slot(p, i);
        adsr_of(s).filter(|_| d == dst::VOLUME)
    })
}

/// The ADSRs a live slot reads as a source (the ones whose running keeps the voice sounding after the key is up).
fn used_adsrs(p: &[f32]) -> [bool; 2] {
    let mut used = [false; 2];
    for i in 0..LIVE {
        if let Some(k) = adsr_of(slot(p, i).0) {
            used[k] = true;
        }
    }
    used
}

/// The unified envelope slot of each Tunefish ADSR: the volume ADSR (or the gate) is envelope 1.
fn env_slots(p: &[f32]) -> [usize; 2] {
    match volume_adsr(p) {
        Some(0) => [0, 1],
        Some(_) => [1, 0],
        None => [1, 2],
    }
}

/// Milliseconds a released Tunefish ADSR needs to finish, from its sustain level, in whole control blocks.
fn release_ms(p: &[f32], k: usize) -> f32 {
    let base = if k == 0 { tp::ENV1 } else { tp::ENV2 };
    let mut a = Adsr { stage: Stage::Release, value: p[base + 2].clamp(0.0, 1.0).min(0.99) };
    let mut blocks = 0u32;
    while !a.done() && blocks < 2000 {
        a.step(&p[base..base + 5], 1.0);
        blocks += 1;
    }
    // The voice reads "an envelope is still running" at the start of a block: it sounds at full level for `blocks`
    // blocks, then fades out linearly over one more (its volume ramps to 0). A cut after a third of that block carries
    // the same energy as the fade.
    (blocks as f32 + FADE_CUT) * BLOCK_S * 1000.0
}

/// How long the voice lasts after its key is up when no ADSR routes to volume: the longest release among the
/// envelopes used as sources (none: the gate closes in the block of the key-up, as Tunefish's does).
pub fn gate_ms(p: &[f32]) -> f32 {
    let used = used_adsrs(p);
    (0..2).filter(|&k| used[k]).map(|k| release_ms(p, k)).fold(FADE_CUT * BLOCK_S * 1000.0, f32::max).min(GATE_MAX_MS)
}

/// The unified target of a Tunefish destination id, `None` for the ones that do nothing.
fn target_of(d: u8, env: [usize; 2]) -> Option<u8> {
    let osc = |f: u8| target::osc(0, f);
    let shp = |k: usize| target::osc(0, target::OSC_SHAPE + k as u8);
    Some(match d {
        dst::BANDWIDTH => shp(shape::WT_BANDWIDTH),
        dst::DAMP => shp(shape::WT_DAMP),
        dst::HARMONICS => shp(shape::WT_HARMONICS),
        dst::SCALE => shp(shape::WT_SCALE),
        dst::VOLUME => osc(target::OSC_GAIN),
        dst::FREQ => osc(target::OSC_PITCH),
        dst::PAN => osc(target::OSC_PAN),
        dst::DETUNE => shp(shape::WT_LAYER_DETUNE),
        dst::SPREAD => shp(shape::WT_LAYER_SPREAD),
        dst::DRIVE => shp(shape::WT_DRIVE),
        dst::NOISE => target::NOISE_LEVEL,
        dst::LP_CUT => target::filter_cutoff(0),
        dst::LP_RES => target::filter_resonance(0),
        dst::HP_CUT => target::filter_cutoff(1),
        dst::HP_RES => target::filter_resonance(1),
        dst::BP_CUT => target::filter_cutoff(2),
        dst::BP_Q => target::filter_resonance(2),
        dst::NT_CUT => target::filter_cutoff(3),
        dst::NT_Q => target::filter_resonance(3),
        dst::ADSR1_DECAY => target::ENV_DECAY + env[0] as u8,
        dst::ADSR2_DECAY => target::ENV_DECAY + env[1] as u8,
        d if (dst::SLOT1..dst::SLOT1 + LIVE as u8).contains(&d) => target::SLOT + (d - dst::SLOT1),
        dst::LFO1_DEPTH => target::LFO_DEPTH,
        dst::LFO2_DEPTH => target::LFO_DEPTH + 1,
        _ => return None,
    })
}

/// The unified mod source of a Tunefish source id (1 LFO 1, 2 LFO 2, 3 ADSR 1, 4 ADSR 2, 5 wheel).
fn source_of(s: u8, env: [usize; 2]) -> Option<usize> {
    Some(match s {
        1 => source::LFO1,
        2 => source::LFO1 + 1,
        3 => source::ENV1 + env[0],
        4 => source::ENV1 + env[1],
        5 => source::WHEEL,
        _ => return None,
    })
}

/// The cutoff parameter of a Tunefish filter at `hz`: the unified law is `20 + 19 980·p²`.
fn cutoff_p(hz: f32) -> f32 {
    inv::inv_cutoff(hz)
}

/// Tunefish cutoff laws: ladder and notch `30 + 20 000·f²`, RBJ `30 + 10 000·f²`, `f` clamped to `0..1`.
fn hz(f: f32, span: f32) -> f32 {
    let f = f.clamp(0.0, 1.0);
    30.0 + span * f * f
}

/// Tunefish's balance `p` as the equal-power pan and the gain factor that give the same left and right levels: the
/// louder side is 1, the other `r = 2·min(p, 1 − p)`.
fn pan(p: f32) -> (f32, f32) {
    let p = p.clamp(0.0, 1.0);
    let r = 2.0 * p.min(1.0 - p);
    let quiet = r * r / (1.0 + r * r);
    (if p <= 0.5 { quiet } else { 1.0 - quiet }, (1.0 + r * r).sqrt())
}

/// The unified parameters of a Tunefish instrument (without insert effects).
pub fn build(p: &[f32], cx: &Cx) -> Patch {
    let env = env_slots(p);
    let mut u = Patch::new();
    // voice
    u.set(v::MASTER, inv::inv_gain(tp::gain_curve(p[tp::GAIN])));
    u.set(v::CAPACITY, inv::inv_count(cx.capacity, 1, 64));
    u.set(v::VELOCITY, 0.0);
    u.set(v::ENV1_AT_OUTPUT, 0.0);
    if p[tp::GEN_GLIDE] > 0.0 {
        // per block `f += (target − f)/(10g + 1)`: an exponential glide of that many blocks
        u.set(v::GLIDE, inv::inv_time((p[tp::GEN_GLIDE] * 10.0 + 1.0) * BLOCK_S));
    }
    // generator: slot 1
    u.osc_type(0, osc_type::WAVETABLE);
    let fixed = p[tp::GEN_FREQ] >= 0.00001;
    u.osc(0, o::TRACKING, if fixed { 1.0 } else { 0.0 });
    u.osc(0, o::FIXED_HZ, p[tp::GEN_FREQ]);
    let octave = 4.0 - tp::round(p[tp::GEN_OCTAVE] * 8.0).clamp(0.0, 8.0);
    u.osc(0, o::OCTAVE, inv::inv_octave(octave));
    // Tunefish's pan is a linear balance (full level on the louder side); the unified pan is equal power. The same left
    // and right levels come from the pan whose ratio is the square of the balance's and the gain that restores the loud side
    let (pan, comp) = pan(p[tp::GEN_PAN]);
    u.osc(0, o::GAIN, inv::inv_gain(0.4 * comp * p[tp::GEN_VOLUME]));
    u.osc(0, o::PAN, pan);
    u.shape(0, shape::WT_BANDWIDTH, p[tp::GEN_BANDWIDTH]);
    u.shape(0, shape::WT_HARMONICS, p[tp::GEN_HARMONICS]);
    u.shape(0, shape::WT_DAMP, p[tp::GEN_DAMP]);
    u.shape(0, shape::WT_SCALE, p[tp::GEN_SCALE]);
    u.shape(0, shape::WT_MOD, p[tp::GEN_MOD]);
    u.shape(0, shape::WT_DRIVE, p[tp::GEN_DRIVE]);
    u.shape(0, shape::WT_LAYERS, inv::inv_count(tp::unison(p[tp::GEN_UNISON]) as u32, 1, LAYERS as u32));
    // a fixed frequency has no left/right detune
    u.shape(0, shape::WT_LAYER_DETUNE, if fixed { 0.0 } else { p[tp::GEN_DETUNE] });
    u.shape(0, shape::WT_LAYER_SPREAD, p[tp::GEN_SPREAD]);
    // noise and its band (a ladder low-pass at f + bw, then an RBJ high-pass at f − bw)
    u.set(v::NOISE_LEVEL, inv::inv_gain(SQRT_2 * p[tp::NOISE_AMOUNT]));
    let (nf, bw) = (p[tp::NOISE_FREQ], p[tp::NOISE_BW]);
    u.set(unisynth::params::old::NOISE + unisynth::params::old::n::BAND_ON, if bw < 0.99 { 1.0 } else { 0.0 });
    u.set(unisynth::params::old::NOISE + unisynth::params::old::n::HP, cutoff_p(hz(nf - bw, 10_000.0)));
    u.set(unisynth::params::old::NOISE + unisynth::params::old::n::LP, cutoff_p(hz(nf + bw, 20_000.0)));
    // filters, in Tunefish's order, each on above 0.5
    let filters = [
        (tp::LP_ON, filter_type::LADDER_LP, tp::LP_CUT, 20_000.0, tp::LP_RES),
        (tp::HP_ON, filter_type::RBJ_HP, tp::HP_CUT, 10_000.0, tp::HP_RES),
        (tp::BP_ON, filter_type::RBJ_BP, tp::BP_CUT, 10_000.0, tp::BP_Q),
        (tp::NT_ON, filter_type::NOTCH, tp::NT_CUT, 20_000.0, tp::NT_Q),
    ];
    for (i, (on, kind, cut, span, res)) in filters.into_iter().enumerate() {
        u.filter(i, if p[on] > 0.5 { kind } else { filter_type::OFF }, cutoff_p(hz(p[cut], span)), p[res]);
    }
    // LFOs: 50·p²/2π Hz, shapes in the same order, key sync from 0.5 up
    for k in 0..2 {
        let b = if k == 0 { tp::LFO1 } else { tp::LFO2 };
        let at = |f: usize| unisynth::params::old::lfo(k, f);
        u.set(at(unisynth::params::old::l::SHAPE), p[b + 2]);
        u.set(at(unisynth::params::old::l::RATE), inv::inv_lfo_rate(50.0 * p[b] * p[b] / TAU));
        u.set(at(unisynth::params::old::l::DEPTH), p[b + 1]);
        u.set(at(unisynth::params::old::l::SYNC), if p[b + 3] >= 0.5 { 1.0 } else { 0.0 });
    }
    // envelopes
    for k in 0..2 {
        let b = if k == 0 { tp::ENV1 } else { tp::ENV2 };
        let at = |f: usize| unisynth::params::old::env(env[k], f);
        u.set(at(unisynth::params::old::e::MODE), 1.0);
        u.set(at(unisynth::params::old::e::ATTACK), p[b]);
        u.set(at(unisynth::params::old::e::DECAY), p[b + 1]);
        u.set(at(unisynth::params::old::e::SUSTAIN), p[b + 2]);
        u.set(at(unisynth::params::old::e::RELEASE), p[b + 3]);
        u.set(at(unisynth::params::old::e::SLOPE), p[b + 4]);
    }
    if volume_adsr(p).is_none() {
        // the organ gate: instant attack, sustain 1, release as long as the longest envelope in use
        u.env(0, 0.0, 0.0, 1.0, inv::inv_env_ms(gate_ms(p)));
    }
    // mod slots 1–8 at the same index, Scale
    for i in 0..LIVE {
        let (s, amount, d) = slot(p, i);
        match (source_of(s, env), target_of(d, env)) {
            (Some(src), Some(tgt)) => u.route(src, tgt, amount, Op::Scale),
            _ => u.skip(),
        }
    }
    // velocity (note-on `vel/128`) scales the generator and the noise before the filters
    debug_assert!(LIVE + 2 <= MOD_SLOTS);
    u.scale(source::VELOCITY, target::osc(0, target::OSC_GAIN), 1.0);
    u.scale(source::VELOCITY, target::NOISE_LEVEL, 1.0);
    u
}

/// A legacy Tunefish effect device chunk (kinds 33–39) for the parameters `p`: the effect's subset of the patch in table
/// order, then the size trailer.
pub fn effect_chunk(kind: u8, p: &[f32]) -> Vec<u8> {
    let mut c: Vec<u8> = tfsynth::device::effect_params(kind).iter().flat_map(|&i| p[i].to_le_bytes()).collect();
    let n = c.len() as i32 + 4;
    c.extend(n.to_le_bytes());
    c
}

/// The insert chain of a patch as unified effect devices, in slot order: `(legacy kind 33..=39, device)`. Slot types
/// 0 and 8–10 are none.
pub fn insert_chain(p: &[f32]) -> Vec<(u8, Dev)> {
    (0..tfsynth::fx::SLOTS)
        .filter_map(|i| {
            let k = tp::effect_kind(p[tp::EFFECT_FIRST + i]);
            let kind = tfsynth::device::kind::INSTRUMENT + k;
            let (uk, mode) = crate::fx::target(kind).filter(|_| (1..=7).contains(&k))?;
            Some((kind, Dev { kind: uk, chunk: crate::fx::chunk(uk, mode, &effect_chunk(kind, p)) }))
        })
        .collect()
}

/// The unified instrument device of a patch with voice capacity `capacity`.
pub fn instrument_device(p: &[f32], capacity: u32) -> Dev {
    let cx = Cx { automated: vec![false; tp::COUNT], capacity };
    let patch = build(p, &cx).finish();
    Dev { kind: unisynth::kind::INSTRUMENT, chunk: unisynth::migrate::encode_old(&patch, &[], &[]) }
}

/// The voice capacity of a patch: `GenPoly`, 1..=16.
pub fn capacity(p: &[f32]) -> u32 {
    tp::polyphony(p[tp::GEN_POLY]) as u32
}

/// Things the unified instrument cannot reproduce for this patch, one line each.
pub fn warnings(p: &[f32]) -> Vec<String> {
    let mut w = vec![];
    let slop = 8.0 * p[tp::GEN_SLOP].powi(3);
    if slop > 1.0 {
        w.push(format!("GenSlop {:.2} (up to ±{slop:.1} Hz of random detune per voice) is not converted", p[tp::GEN_SLOP]));
    }
    if volume_adsr(p).is_none() && (0..2).any(|k| used_adsrs(p)[k] && release_ms(p, k) > GATE_MAX_MS) {
        w.push(format!("an envelope release longer than {} s ends the note after {} s", GATE_MAX_MS / 1000.0, GATE_MAX_MS / 1000.0));
    }
    for i in LIVE..10 {
        let (s, a, d) = slot(p, i);
        if s != 0 && a != 0.0 && d != 0 {
            w.push(format!("mod slot {} is not evaluated by Tunefish (only the first {LIVE} run); dropped", i + 1));
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unified_cutoff_law_reproduces_the_tunefish_ones() {
        for f in [0.0f32, 0.2, 0.5, 0.9, 1.0] {
            let p = cutoff_p(hz(f, 20_000.0));
            assert!((unisynth::law::cutoff(p) - hz(f, 20_000.0)).abs() < 1.0 + 0.002 * hz(f, 20_000.0));
        }
    }

    #[test]
    fn a_patch_without_routes_gates_the_note() {
        let p = [0.0f32; tp::COUNT];
        assert_eq!(volume_adsr(&p), None);
        assert!((gate_ms(&p) - FADE_CUT * BLOCK_S * 1000.0).abs() < 1e-3);
    }
}
