//! The PAD table oscillator (change add-tf3-import, Tunefish 3's additive generator): pitch, load-time table, memory.

use super::{on, render, with_ins, UInstrument};
use crate::law::host as inv;
use crate::params::{o, osc, osc_type, shape as sp, v, DEFAULTS};
use synth_core::device::Device;

/// A v3 patch with slot 1 = PAD table (gauss, width growing as h) and `edits`.
fn patch(edits: &[(usize, f32)]) -> Vec<u8> {
    let mut x = DEFAULTS;
    x[osc(0, o::TYPE)] = osc_type::encode(osc_type::PAD);
    x[osc(0, o::SHAPE + sp::PAD_PROFILE)] = 1.0; // gauss
    x[osc(0, o::SHAPE + sp::PAD_BANDWIDTH)] = 0.2;
    x[osc(0, o::SHAPE + sp::PAD_DAMP)] = 0.5;
    x[osc(0, o::SHAPE + sp::PAD_SCALE)] = 0.0; // shape 1 defaults to 0.5 (a saw/pulse slot's pulse width)
    for &(i, val) in edits {
        x[i] = val;
    }
    crate::chunk::encode(&x, &[])
}

fn power(x: &[f32], hz: f64) -> f64 {
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for (i, s) in x.iter().enumerate() {
        let a = 2.0 * std::f64::consts::PI * hz * i as f64 / 44100.0;
        re += *s as f64 * a.cos();
        im += *s as f64 * a.sin();
    }
    re * re + im * im
}

/// Energy of `x` within ±`w` Hz of `hz` (probes every 0.5 Hz: a PAD partial is a band of fine lines).
fn band(x: &[f32], hz: f64, w: f64) -> f64 {
    (0..=(4.0 * w) as usize).map(|k| power(x, hz - w + k as f64 * 0.5)).sum()
}

#[test]
fn a_pad_slot_sounds_on_the_harmonics_of_half_the_note() {
    // note 69: the fundamental at 440 · 0.499 = 219.56 Hz, partials on its harmonics and nothing between
    let (x, _) = with_ins(&patch(&[]), 44100.0, |ins| render(ins, 44100.0, &[on(0, 69)], 26460, 512));
    let tail = &x[8820..];
    let f = 219.56;
    let (h1, h2, h3) = (band(tail, f, 12.0), band(tail, 2.0 * f, 12.0), band(tail, 3.0 * f, 12.0));
    for between in [f * 0.5, f * 1.5, f * 2.5] {
        let e = band(tail, between, 12.0);
        assert!(h1 > 50.0 * e && h2 > 20.0 * e && h3 > 20.0 * e, "{between} Hz: {e} vs {h1} {h2} {h3}");
    }
    assert!(h1 > 50.0 * band(tail, 440.0 * 1.25, 12.0), "nothing on the note's own series");
}

#[test]
fn a_wider_band_spreads_each_partial() {
    // the share of a partial's energy 1 % off its centre grows with the bandwidth
    let spread = |bw: f32| {
        let (x, _) = with_ins(&patch(&[(osc(0, o::SHAPE + sp::PAD_BANDWIDTH), bw)]), 44100.0, |ins| render(ins, 44100.0, &[on(0, 69)], 44100, 512));
        let f = 219.56 * 3.0;
        band(&x[8820..], f * 1.04, 4.0) / band(&x[8820..], f, 4.0)
    };
    assert!(spread(0.8) > 3.0 * spread(0.1), "{} vs {}", spread(0.8), spread(0.1));
}

#[test]
fn the_table_ignores_later_shape_changes() {
    // the table is built at load: setting the bandwidth afterwards changes nothing (the drive stays live)
    let chunk = patch(&[]);
    let (a, _) = with_ins(&chunk, 44100.0, |ins| render(ins, 44100.0, &[on(0, 60)], 8820, 512));
    let (b, _) = with_ins(&chunk, 44100.0, |ins| {
        ins.set_param(osc(0, o::SHAPE + sp::PAD_BANDWIDTH), 0.9);
        ins.set_param(osc(0, o::SHAPE + sp::PAD_HARMONICS), 1.0);
        render(ins, 44100.0, &[on(0, 60)], 8820, 512)
    });
    assert_eq!(a.iter().map(|s| s.to_bits()).collect::<Vec<_>>(), b.iter().map(|s| s.to_bits()).collect::<Vec<_>>());
}

#[test]
fn one_table_per_slot_whatever_the_voice_count() {
    let need = |voices: u32| UInstrument::need(&patch(&[(v::CAPACITY, inv::inv_count(voices, 1, 64))])).unwrap();
    let pm_only = |voices: u32| {
        let mut x = DEFAULTS;
        x[osc(0, o::TYPE)] = osc_type::encode(osc_type::PM);
        x[v::CAPACITY] = inv::inv_count(voices, 1, 64);
        UInstrument::need(&crate::chunk::encode(&x, &[])).unwrap()
    };
    // a PAD slot adds one table (2^16 f32) and one per-voice state per voice
    let per_voice = core::mem::size_of::<crate::pts::Pts>();
    let table = crate::pad::LEN * 4;
    for n in [1u32, 16] {
        let extra = need(n) - pm_only(n);
        assert!(extra >= table + n as usize * per_voice && extra < table + n as usize * (per_voice + 64) + 64, "{n} voices: {extra}");
    }
}

/// Load time of a PAD slot (run with `--ignored --nocapture`, release).
#[test]
#[ignore]
fn load_time_of_a_pad_table() {
    for (h, bw) in [(0.1f32, 0.2f32), (1.0, 0.8)] {
        let chunk = patch(&[(osc(0, o::SHAPE + sp::PAD_HARMONICS), h), (osc(0, o::SHAPE + sp::PAD_BANDWIDTH), bw)]);
        let t = std::time::Instant::now();
        with_ins(&chunk, 44100.0, |_| ());
        std::eprintln!("harmonics {h}, bandwidth {bw}: {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    }
}
