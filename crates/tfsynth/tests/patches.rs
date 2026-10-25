//! In-repo synthetic patches and phrases for the self-golden tests (our own values; no third-party presets).

use tfsynth::params::*;

/// Neutral base: one partial, unity gain, centred, octave ×1, 16 voices, unfiltered noise off, mod slots empty
/// (amount 1), bend ±2 semitones.
pub fn base() -> [f32; COUNT] {
    let mut p = [0.0f32; COUNT];
    p[GAIN] = 0.5;
    p[GEN_VOLUME] = 1.0;
    p[GEN_PAN] = 0.5;
    p[GEN_OCTAVE] = 0.5;
    p[GEN_POLY] = 1.0;
    p[GEN_SCALE] = 0.25;
    p[NOISE_BW] = 1.0;
    p[BEND_UP] = 1.0 / 12.0;
    p[BEND_DOWN] = 1.0 / 12.0;
    for i in 0..10 {
        p[MM_FIRST + 3 * i + 1] = 0.5;
    }
    p
}

pub fn route(p: &mut [f32; COUNT], slot: usize, src: u8, amount: f32, target: u8) {
    let b = MM_FIRST + 3 * slot;
    p[b] = src as f32 / 14.0;
    p[b + 1] = amount;
    p[b + 2] = target as f32 / 39.0;
}

fn set(p: &mut [f32; COUNT], kv: &[(usize, f32)]) {
    kv.iter().for_each(|&(k, v)| p[k] = v);
}

/// `(name, patch)` covering the generator, noise, filters, envelopes, LFOs and mod matrix (sections 2–3).
pub fn instrument_patches() -> Vec<(&'static str, [f32; COUNT])> {
    let mut out = vec![];
    let mut p = base();
    set(&mut p, &[(GEN_HARMONICS, 0.3), (GEN_BANDWIDTH, 0.2), (GEN_DAMP, 0.4), (GEN_MOD, 0.5)]);
    out.push(("gen_spectrum", p));
    let mut p = base();
    set(&mut p, &[(GEN_HARMONICS, 0.2), (GEN_SCALE, 0.37), (GEN_UNISON, 0.5), (GEN_SPREAD, 0.6), (GEN_DETUNE, 0.4)]);
    out.push(("gen_unison_inharmonic", p));
    let mut p = base();
    set(&mut p, &[(GEN_HARMONICS, 0.1), (GEN_DRIVE, 0.4), (GEN_OCTAVE, 0.25), (GEN_SLOP, 0.6), (GEN_PAN, 0.3)]);
    out.push(("gen_drive_octave_slop", p));
    let mut p = base();
    set(&mut p, &[(GEN_FREQ, 0.4), (GEN_HARMONICS, 0.15)]);
    out.push(("gen_fixed_freq", p));
    let mut p = base();
    set(&mut p, &[(GEN_POLY, 0.0), (GEN_GLIDE, 0.5), (GEN_HARMONICS, 0.2)]);
    out.push(("gen_mono_glide", p));
    let mut p = base();
    set(&mut p, &[(GEN_VOLUME, 0.3), (NOISE_AMOUNT, 0.6), (NOISE_FREQ, 0.5), (NOISE_BW, 0.15)]);
    out.push(("noise_bandpass", p));
    let mut p = base();
    set(&mut p, &[(GEN_HARMONICS, 0.5), (LP_ON, 1.0), (LP_CUT, 0.4), (LP_RES, 0.7), (HP_ON, 1.0), (HP_CUT, 0.2), (HP_RES, 0.3)]);
    out.push(("filter_lp_hp", p));
    let mut p = base();
    set(&mut p, &[(GEN_HARMONICS, 0.5), (NOISE_AMOUNT, 0.3), (BP_ON, 1.0), (BP_CUT, 0.4), (BP_Q, 0.5), (NT_ON, 1.0), (NT_CUT, 0.3), (NT_Q, 0.6)]);
    out.push(("filter_bp_notch", p));
    let mut p = base();
    set(&mut p, &[(GEN_HARMONICS, 0.3)]);
    p[ENV1..ENV1 + 5].copy_from_slice(&[0.3, 0.5, 0.4, 0.5, 0.7]);
    p[ENV2..ENV2 + 5].copy_from_slice(&[0.1, 0.6, 0.2, 0.4, 0.2]);
    set(&mut p, &[(LP_ON, 1.0), (LP_CUT, 0.8), (LP_RES, 0.4)]);
    route(&mut p, 0, 3, 0.5, 5); // ADSR1 → volume
    route(&mut p, 1, 4, 0.7, 12); // ADSR2 → LP cutoff (amount > 1)
    route(&mut p, 2, 4, 0.5, 20); // ADSR2 → ADSR1 decay
    out.push(("env_adsr_routes", p));
    for (shape, name) in [(0.0, "lfo_sine"), (0.25, "lfo_ramp_up"), (0.5, "lfo_ramp_down"), (0.75, "lfo_square"), (1.0, "lfo_random")] {
        let mut p = base();
        set(&mut p, &[(GEN_HARMONICS, 0.3)]);
        p[LFO1..LFO1 + 4].copy_from_slice(&[0.6, 0.8, shape, 0.0]);
        p[LFO2..LFO2 + 4].copy_from_slice(&[0.3, 0.6, 0.0, 1.0]);
        route(&mut p, 0, 1, 0.5, 5); // LFO1 → volume
        route(&mut p, 1, 2, 0.5, 4); // LFO2 → scale
        route(&mut p, 2, 1, 0.5, 31); // LFO1 → LFO2 depth
        out.push((name, p));
    }
    let mut p = base();
    set(&mut p, &[(GEN_HARMONICS, 0.3)]);
    route(&mut p, 0, 5, 0.8, 10); // wheel → drive
    route(&mut p, 1, 5, 0.5, 23); // wheel → slot 2's own amount
    route(&mut p, 2, 3, 0.5, 5);
    p[ENV1..ENV1 + 5].copy_from_slice(&[0.2, 0.4, 0.6, 0.3, 0.5]);
    out.push(("mod_wheel_self_mod", p));
    out
}

/// A phrase: `(block, note, velocity)` with velocity 0 = note-off, plus wheel/bend changes in the test.
pub const PHRASE: [(usize, u8, u8); 10] = [
    (0, 48, 100),
    (8, 55, 90),
    (16, 48, 0),
    (16, 60, 110),
    (24, 55, 0),
    (30, 64, 80),
    (30, 67, 80),
    (44, 60, 0),
    (52, 64, 0),
    (52, 67, 0),
];
pub const PHRASE_BLOCKS: usize = 80;

/// One patch per effect kind (slot 1, plus a second slot of another kind for chain order): `(name, kind, patch)`.
pub fn effect_patches() -> Vec<(&'static str, u8, [f32; COUNT])> {
    let mut v = vec![];
    let mut base = base();
    set(&mut base, &[(GEN_HARMONICS, 0.4), (GEN_UNISON, 0.2), (GEN_SPREAD, 0.4)]);
    base[ENV1..ENV1 + 5].copy_from_slice(&[0.1, 0.4, 0.6, 0.4, 0.5]);
    route(&mut base, 0, 3, 0.5, 5);
    let fx = |p: &mut [f32; COUNT], slot: usize, k: u8| p[EFFECT_FIRST + slot] = k as f32 / 10.0;
    for (name, k, kv) in [
        ("fx_distortion", 1u8, vec![(DIST_AMOUNT, 0.6)]),
        ("fx_delay", 2, vec![(DELAY_LEFT, 0.25), (DELAY_RIGHT, 0.375), (DELAY_DECAY, 0.5)]),
        ("fx_chorus", 3, vec![(CHORUS_RATE, 0.4), (CHORUS_DEPTH, 0.6), (CHORUS_GAIN, 0.8)]),
        ("fx_flanger", 4, vec![(FLANGER_LFO, 0.3), (FLANGER_FREQ, 0.6), (FLANGER_AMP, 0.7), (FLANGER_WET, 0.6)]),
        ("fx_reverb", 5, vec![(REV_ROOM, 0.7), (REV_DAMP, 0.4), (REV_WET, 0.4), (REV_WIDTH, 0.8)]),
        ("fx_formant", 6, vec![(FORMANT_TYPE, 0.5), (FORMANT_WET, 0.7)]),
        ("fx_eq", 7, vec![(EQ_LOW, 0.9), (EQ_MID, 0.3), (EQ_HIGH, 0.7)]),
    ] {
        let mut p = base;
        fx(&mut p, 0, k);
        set(&mut p, &kv);
        v.push((name, k, p));
    }
    v
}
