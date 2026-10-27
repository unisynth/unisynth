//! Self-golden: FNV-1a 64 of the f32 output of each synthetic patch playing `PHRASE`, kept in `tests/golden.txt`.
//! `UPDATE_GOLDEN=1 cargo test -p unisynth golden` rewrites it (review the diff before committing).
//!
//! The patches cover the voice framework (envelopes, LFOs, matrix, unison, mono, steal) and every oscillator type.
//! They pin the numbers: an intended change to a law constant or a rendering detail shows up as a changed hash.

use super::osc_sample::{bank, chunk_of, dls_slot, gsm_entry, sh, with_bank};
use super::*;
use crate::modmx::{source, target};
use crate::params::{e, env, l, lfo, shape as sp, v};

const RATE: f64 = 44100.0;
const FRAMES: usize = 24_000;

/// Frame, note, velocity (0 = note-off): two overlapping notes, a release, a late third note.
fn phrase() -> Vec<Ev> {
    vec![on(0, 60), on(3000, 64), off(9000, 60), off(14_000, 64), on(15_000, 67), off(19_000, 67)]
}

fn fnv(x: &[f32]) -> u64 {
    x.iter().flat_map(|v| v.to_bits().to_le_bytes()).fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

fn osc_on(i: usize, ty: usize) -> (usize, f32) {
    (osc(i, o::TYPE), inv::inv_choice(ty, osc_type::COUNT))
}

fn amp_env(mode_tf: bool) -> Vec<(usize, f32)> {
    vec![
        (env(0, e::MODE), if mode_tf { 1.0 } else { 0.0 }),
        (env(0, e::ATTACK), if mode_tf { 0.6 } else { 0.05 }),
        (env(0, e::DECAY), if mode_tf { 0.5 } else { 0.3 }),
        (env(0, e::SUSTAIN), 0.6),
        (env(0, e::RELEASE), if mode_tf { 0.5 } else { 0.3 }),
    ]
}

/// `(name, chunk, bank)`.
fn patches() -> Vec<(&'static str, Vec<u8>, Option<Vec<u8>>)> {
    let pmp = |edits: Vec<(usize, f32)>| patch(&edits);
    let mut out: Vec<(&'static str, Vec<u8>, Option<Vec<u8>>)> = vec![];
    let mut add = |n: &'static str, c: Vec<u8>| out.push((n, c, None));

    // --- voice framework ---
    add("env_wavesabre", pmp(amp_env(false)));
    add("env_tunefish", pmp(amp_env(true)));
    add("env_routed_only", pmp(merged(&[amp_env(false), vec![(v::ENV1_AT_OUTPUT, 0.0)], route(0, source::ENV1, target::GAIN, true, 0.6)])));
    add(
        "lfo_vibrato_synced",
        pmp(merged(&[vec![(lfo(0, l::RATE), 0.55), (lfo(0, l::DEPTH), 1.0), (lfo(0, l::SYNC), 1.0)], route(0, source::LFO1, target::PITCH, false, 0.7)])),
    );
    add(
        "lfo_tremolo_free",
        pmp(merged(&[vec![(lfo(1, l::SHAPE), 0.25), (lfo(1, l::RATE), 0.6), (lfo(1, l::DEPTH), 0.8)], route(0, source::LFO1 + 1, target::GAIN, true, 1.0)])),
    );
    add(
        "matrix_velocity_key_wheel",
        pmp(merged(&[
            route(0, source::VELOCITY, target::osc(0, target::OSC_GAIN), true, 0.9),
            route(1, source::KEY, target::PAN, false, 0.5),
            route(2, source::WHEEL, target::PITCH, false, 0.2),
        ])),
    );
    add(
        "unison_detune_spread",
        pmp(vec![(v::UNISON, inv::inv_count(3, 1, 16)), (v::UNI_DETUNE, 0.3), (v::UNI_SPREAD, 0.8), (v::CAPACITY, cap(12))]),
    );
    add("mono_legato_glide", pmp(vec![(v::MODE, 1.0), (v::GLIDE, 0.4), (v::SLIDE, 0.3)]));
    add("steal_oldest", pmp(vec![(v::CAPACITY, cap(2)), (v::STEAL, 1.0)]));

    // --- oscillators ---
    add(
        "pm_two_operators",
        pmp(vec![
            (osc(0, o::PM_OUT), 0.4),
            (osc(0, o::MIX), 0.0),
            osc_on(1, osc_type::PM),
            (osc(1, o::RATIO), 0.3),
            (osc(1, o::FEEDBACK), 0.2),
            (sh(sp::PM_WAVEFORM), 0.4),
        ]),
    );
    add(
        "pm_square35_feedback",
        pmp(vec![(osc(0, o::FEEDBACK), 0.35), (sh(sp::PM_WAVEFORM), 1.0), (osc(0, o::COARSE), 0.55), (osc(0, o::OCTAVE), 0.5)]),
    );
    add(
        "pm_fixed_hz_transposed",
        pmp(vec![
            (osc(0, o::TRACKING), 1.0),
            (osc(0, o::FIXED_HZ), inv::inv_fixed_hz(330.0)),
            (osc(0, o::FINE), 0.55),
            (v::TRANSPOSE, inv::inv_transpose(5.0)),
        ]),
    );
    add("blit_saw", pmp(vec![osc_on(0, osc_type::BLIT), (sh(sp::BLIT_WAVEFORM), 0.0)]));
    add("blit_pulse_width", pmp(vec![osc_on(0, osc_type::BLIT), (sh(sp::BLIT_WAVEFORM), 1.0), (sh(sp::BLIT_PULSE_WIDTH), 0.3)]));
    add("wavetable_default", pmp(vec![osc_on(0, osc_type::WAVETABLE)]));
    add(
        "wavetable_layers_drive",
        pmp(vec![
            osc_on(0, osc_type::WAVETABLE),
            (sh(sp::WT_HARMONICS), 0.5),
            (sh(sp::WT_DAMP), 0.4),
            (sh(sp::WT_DRIVE), 0.5),
            (sh(sp::WT_LAYERS), 1.0),
            (sh(sp::WT_LAYER_DETUNE), 0.3),
            (sh(sp::WT_LAYER_SPREAD), 0.3),
        ]),
    );
    let dls = bank(4000, Some((500, 2000)));
    let mut dls_patch = |n: &'static str, extra: &[(usize, f32)]| {
        out.push((n, chunk_of(&dls_slot(extra), &[]), Some(dls.clone())));
    };
    dls_patch("sampler_dls_loop", &[(sh(sp::SMP_LOOP_MODE), inv::inv_choice(1, 3))]);
    dls_patch("sampler_dls_pingpong_reverse", &[(sh(sp::SMP_LOOP_MODE), inv::inv_choice(2, 3)), (sh(sp::SMP_REVERSE), 1.0)]);
    out.push((
        "sampler_gsm_one_shot",
        chunk_of(
            &[osc_on(0, osc_type::SAMPLER), (sh(sp::SMP_SOURCE), 0.0), (pm::smp_index(0), 1.0), (v::ONE_SHOT, 1.0)],
            &[&gsm_entry()],
        ),
        None,
    ));
    let mut add = |n: &'static str, c: Vec<u8>| out.push((n, c, None));

    // --- noise ---
    let no_osc = [osc_on(0, osc_type::OFF), (v::NOISE_LEVEL, 0.25)];
    add("noise_white", pmp(no_osc.to_vec()));
    add(
        "noise_band",
        pmp(merged(&[no_osc.to_vec(), vec![(NOISE + n::BAND_ON, 1.0), (NOISE + n::HP, inv::inv_cutoff(800.0)), (NOISE + n::LP, inv::inv_cutoff(2000.0))]])),
    );

    // --- filters: each type on a saw plus a little noise, then series slots and a modulated cutoff ---
    let src = vec![osc_on(0, osc_type::BLIT), (v::NOISE_LEVEL, 0.12)];
    let flt = |slot: usize, ty: usize, hz: f32, res: f32| {
        vec![
            (filt(slot, fp::TYPE), inv::inv_choice(ty, ft::COUNT)),
            (filt(slot, fp::CUTOFF), inv::inv_cutoff(hz)),
            (filt(slot, fp::RESONANCE), res),
        ]
    };
    let mut add = |n: &'static str, f: Vec<(usize, f32)>| out.push((n, patch(&merged(&[src.clone(), f])), None));
    for (n, ty) in [
        ("filter_svf_lp", ft::SVF_LP),
        ("filter_svf_hp", ft::SVF_HP),
        ("filter_svf_bp", ft::SVF_BP),
        ("filter_svf_notch", ft::SVF_NOTCH),
        ("filter_ladder_lp", ft::LADDER_LP),
        ("filter_rbj_hp", ft::RBJ_HP),
        ("filter_rbj_bp", ft::RBJ_BP),
        ("filter_notch", ft::NOTCH),
    ] {
        add(n, flt(0, ty, 1500.0, 0.4));
    }
    add("filter_ladder_full_resonance", flt(2, ft::LADDER_LP, 900.0, 1.0)); // clamped to 0.85
    add("filter_series_lp_hp", merged(&[flt(0, ft::LADDER_LP, 3000.0, 0.5), flt(3, ft::RBJ_HP, 400.0, 0.2)]));
    add(
        "filter_cutoff_envelope_route",
        merged(&[
            flt(1, ft::SVF_LP, 600.0, 0.3),
            amp_env(false),
            route(0, source::ENV1, target::filter_cutoff(1), false, 6000.0),
            route(1, source::LFO1, target::filter_resonance(1), false, 0.2),
        ]),
    );
    out
}

use crate::params::{f as fp, filt, filter_type as ft, n, NOISE};

fn play(chunk: &[u8], bank: Option<&[u8]>) -> Vec<f32> {
    with_bank(chunk, bank, RATE, |ins| {
        let (l, r) = render(ins, RATE, &phrase(), FRAMES, 512);
        l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]).collect()
    })
}

fn line(name: &str, x: &[f32]) -> String {
    let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    format!("{name} {:016x} peak={peak:.4}", fnv(x))
}

/// The patches this build can load: a build without an optional voice path skips the patches that use it and must
/// play every other one bit for bit as the full build does.
fn loadable() -> Vec<(&'static str, Vec<u8>, Option<Vec<u8>>)> {
    patches().into_iter().filter(|(_, c, _)| UInstrument::need(c).is_ok()).collect()
}

#[test]
fn golden() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden.txt");
    let all = patches().len();
    let patches = loadable;
    assert!(patches().len() * 2 > all, "most golden patches use no optional voice path");
    let got: Vec<String> = patches().iter().map(|(n, c, b)| line(n, &play(c, b.as_deref()))).collect();
    for l in &got {
        assert!(!l.contains("peak=0.0000") && !l.contains("NaN") && !l.contains("inf"), "{l}");
    }
    let text = got.join("\n") + "\n";
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(path, &text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(path).expect("tests/golden.txt missing: run with UPDATE_GOLDEN=1");
    for g in &got {
        let name = g.split(' ').next().unwrap();
        let w = want.lines().find(|w| w.split(' ').next() == Some(name)).unwrap_or_else(|| panic!("{name} missing from golden.txt"));
        assert_eq!(g, w);
    }
    assert_eq!(want.lines().count(), all, "golden.txt has patches this test no longer plays");
}

/// Every patch plays the same whatever the caller's buffer size (the patches use no automation).
#[test]
fn the_patches_do_not_depend_on_the_call_size() {
    for (n, c, b) in loadable() {
        let a = with_bank(&c, b.as_deref(), RATE, |ins| render(ins, RATE, &phrase(), FRAMES, 512));
        let z = with_bank(&c, b.as_deref(), RATE, |ins| render(ins, RATE, &phrase(), FRAMES, 77));
        assert!(a == z, "{n}");
    }
}
