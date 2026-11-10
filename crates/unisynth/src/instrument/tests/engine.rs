//! Engine oscillator types in the voice (change add-fudge-softsynth): the clock from the slot pitch, the note-on
//! bytes, the resampler's latency, speed following pitch.

use super::{on, render, with_ins, Ev};
use crate::osc::engine::{DrumT, EngineSlot, EngineT, ToneT};
use crate::params::{e, env, o, osc, osc_type, shape as sp, DEFAULTS};

/// An output rate at which note 60 (ratio 1, no coarse or fine) steps just over one engine frame per output frame:
/// the f32 pitch path lands within 1e-7 of the clock, and below a clock of 1 the reconstruction low-pass would run.
const RATE: f64 = 256.0 * 261.625_565_300_598_6 * (1.0 - 1e-6);

fn patch(edits: &[(usize, f32)]) -> Vec<u8> {
    let mut v = DEFAULTS;
    // envelope 1 holds full level from the note-on (an engine instrument's timed attack of 0 is instant)
    v[env(0, e::ATTACK)] = 0.0;
    v[env(0, e::SUSTAIN)] = 1.0;
    for &(i, x) in edits {
        v[i] = x;
    }
    crate::chunk::encode(&v, &[])
}

/// Slot 1 as Fudge's tone 17 (FM on), on the D3 laws.
fn tone() -> Vec<(usize, f32)> {
    let s = |k: usize| osc(0, o::SHAPE + k);
    vec![
        (osc(0, o::TYPE), osc_type::encode(osc_type::SHAPER_TONE)),
        (s(sp::TONE_CARRIER), 128.0 / 1024.0),
        (s(sp::TONE_MODULATOR), 128.0 / 1024.0),
        (s(sp::TONE_DEPTH), 2.5 / 64.0),
        (s(sp::TONE_VOLUME), 110.0 / 256.0),
        (s(sp::TONE_SHAPE), 0.5),
        (s(sp::TONE_FM), 1.0),
        (s(sp::TONE_RATE), 1.0 / 16.0),
    ]
}

/// Slot 1 as Fudge's drum 9 (falling sweep, `1 − sin` envelope).
fn drum() -> Vec<(usize, f32)> {
    let s = |k: usize| osc(0, o::SHAPE + k);
    vec![
        (osc(0, o::TYPE), osc_type::encode(osc_type::SWEEP_DRUM)),
        (s(sp::DRUM_VOLUME), 127.0 / 512.0),
        (s(sp::DRUM_FREQ), 0.1 / 4.0),
        (s(sp::DRUM_SWEEP), 0.03),
        (s(sp::DRUM_GROWTH), 0.0013 * 64.0),
        (s(sp::DRUM_ENV), 1.0),
        (s(sp::DRUM_SLOPE), ((16.0 / 1556.0 + 1.0) / 2.0) as f32),
        (s(sp::DRUM_CURVE), 0.5),
    ]
}

/// The note-on bytes as slot 1's values: start `hi·4096 + lo·16`, backward.
fn bytes(hi: u8, lo: u8, back: bool) -> Vec<(usize, f32)> {
    vec![(osc(0, o::TRACKING), hi as f32 / 255.0), (osc(0, o::PM_OUT), lo as f32 / 255.0), (osc(0, o::FEEDBACK), if back { 1.0 } else { 0.0 })]
}

fn play(edits: &[(usize, f32)], evs: &[Ev], n: usize) -> Vec<f32> {
    with_ins(&patch(edits), RATE, |ins| render(ins, RATE, evs, n, 512)).0
}

/// The engine's own frames from `start` (output scale), as the slot's settings decode them.
fn frames(edits: &[(usize, f32)], start: i64, back: bool, n: usize) -> Vec<f32> {
    let mut sh = [0.0f32; o::SHAPES];
    for &(i, x) in edits {
        if i >= osc(0, o::SHAPE) && i < osc(0, o::SHAPE + o::SHAPES) {
            sh[i - osc(0, o::SHAPE)] = x;
        }
    }
    let e = if edits[0].1 == osc_type::encode(osc_type::SHAPER_TONE) { EngineT::Tone(ToneT::from_shape(&sh)) } else { EngineT::Drum(DrumT::from_shape(&sh)) };
    let mut s = EngineSlot::new();
    s.seek(&e, start, back);
    (0..n).map(|_| s.next(&e)).collect()
}

/// The note-on's fade-in (frames).
const FADE: usize = 16;

/// `out` is `want` delayed by `delay` frames, up to one gain (the voice's level), within `tol` of the peak, after the
/// note-on's fade-in.
fn delayed(out: &[f32], want: &[f32], delay: usize, tol: f32) {
    let k = (FADE..out.len()).max_by(|&a, &b| want[a - delay].abs().total_cmp(&want[b - delay].abs())).unwrap();
    let g = out[k] / want[k - delay];
    let peak = want.iter().fold(0.0f32, |m, v| m.max(v.abs())) * g.abs();
    assert!(g.abs() > 0.1, "gain {g}");
    for i in FADE..out.len() {
        assert!((out[i] - g * want[i - delay]).abs() <= tol * peak, "frame {i}: {} vs {}", out[i], g * want[i - delay]);
    }
}

#[test]
fn at_one_engine_frame_per_output_frame_the_slot_is_the_engine_two_frames_late() {
    for ed in [tone(), drum()] {
        let out = play(&ed, &[on(0, 60)], 1500);
        delayed(&out, &frames(&ed, 0, false, 1500), 2, 2e-3);
    }
}

#[test]
fn speed_follows_pitch() {
    let ed = drum();
    let low = play(&ed, &[on(0, 60)], 1200);
    let high = play(&ed, &[on(0, 72)], 600);
    // two engine frames per output frame: output n holds frame 2n − 1, which note 60 outputs at 2n + 1
    for n in FADE..590 {
        assert!((high[n] - low[2 * n + 1]).abs() < 2e-3, "frame {n}: {} vs {}", high[n], low[2 * n + 1]);
    }
}

#[test]
fn the_start_bytes_seek_and_a_high_position_byte_keeps_the_note_pitch() {
    // TRACKING ≥ 128 would read as the fixed-frequency switch on another kind
    let mut ed = tone();
    ed.extend(bytes(130, 3, false));
    let out = play(&ed, &[on(0, 60)], 800);
    delayed(&out, &frames(&ed, 130 * 4096 + 48, false, 800), 2, 2e-3);
    let mut ed = drum();
    ed.extend(bytes(0, 40, false));
    let out = play(&ed, &[on(0, 60)], 800);
    delayed(&out, &frames(&ed, 640, false, 800), 2, 2e-3);
}

#[test]
fn the_backward_byte_plays_the_frames_in_reverse() {
    for mut ed in [tone(), drum()] {
        ed.extend(bytes(0, 16, true));
        let out = play(&ed, &[on(0, 60)], 300);
        let want = frames(&ed, 256, true, 300);
        assert!(want[0] != want[100], "a backward run moves");
        delayed(&out, &want, 2, 2e-3);
    }
}

#[test]
fn a_retrigger_seeks_again() {
    let mut ed = drum();
    ed.extend(bytes(0, 0, false));
    // one voice, the new note cuts the old one
    ed.push((crate::params::v::CAPACITY, 0.0));
    // the second note-on starts from frame 0 again: its output repeats the first's (after the declick)
    let out = play(&ed, &[on(0, 60), on(1000, 60)], 1600);
    let first = &out[..600];
    let again = &out[1000..1600];
    for k in 200..600 {
        assert!((first[k] - again[k]).abs() < 2e-3, "frame {k}: {} vs {}", first[k], again[k]);
    }
}

#[cfg(feature = "std")]
#[test]
fn the_engine_modules_are_named_by_needs() {
    let n = crate::needs::instrument(&patch(&tone()), &|_| None).unwrap();
    assert!(n.contains(&crate::module::OSC_SHAPER_TONE));
    let n = crate::needs::instrument(&patch(&drum()), &|_| None).unwrap();
    assert!(n.contains(&crate::module::OSC_SWEEP_DRUM));
}


