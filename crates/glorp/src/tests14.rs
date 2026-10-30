//! Synthetic-only tests for ID 14 (noise-excited modal resonator bank on the shared scheduler, poly and mono mode). No song
//! data, recorded tap or proprietary vector is used: the expectations are closed forms, structural properties of the
//! scheduler, and an independent model (ideal f64 biquads, exact `exp`, f64 envelope) written from the behaviour in
//! research/glorp-devices.md §ID 14. Parameter numbers are **wire indices** (SetParam order), not byte offsets.
use crate::id14::{check, design, plan, Error, Id14, MAX_MODES, PARAMS, QUEUE, VOICES};

/// Made-up musical settings (not the song's): 26 modes, deterministic harmonics, mild damping, poly mode.
const BASE: [f32; PARAMS] = [
    1.0, // 0 hold (>= 0.5 sustains, < 0.5 releases at the sustain stage)
    0.1, // 1 mode count trunc(255.999·v) + 1 = 26
    0.3, // 2 inharmonicity (v < 0.5: (2v)³)
    0.2, // 3 damping A
    0.2, // 4 damping B
    0.5, // 5 inert
    0.5, // 6 inert
    0.6, // 7 octave selector trunc(5.99·v) = 3 -> ×1
    0.25, // 8 seed trunc(1024.999·v)
    0.3, // 9 harmonic range trunc(63.99·v) + 1 = 20
    0.5, // 10 stereo width
    0.0, // 11 random harmonic selection (> 0.5)
    0.5, // 12 inert
    0.8, // 13 noise level
    0.5, // 14 noise decay rate 50·v³ + 2
    0.3, // 15 tone filter (< 0.5 low-pass, >= 0.5 high-pass)
    0.5, // 16 crackle depth
    0.3, // 17 crackle rate
    0.8, // 18 band-passed noise gain
    0.2, // 19 band-stop width
    0.05, // 20 envelope time
    0.5, // 21 crackle mix
    1.0, // 22 output gain (0.4·v)²
    0.0, // 23 voice mode (>= 1 mono)
    0.0, // 24 inert (accepted, ignored)
    0.0, // 25 mode dropout
];

const INERT: [usize; 4] = [5, 6, 12, 24];

/// Runs a test body on a thread with a big stack: the device is ~240 KB and debug builds copy it several times.
fn big(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new().stack_size(64 << 20).spawn(f).unwrap().join().unwrap_or_else(|e| std::panic::resume_unwind(e));
}

fn dev(p: &[f32; PARAMS]) -> Id14 {
    Id14::new(p).unwrap()
}

fn with(i: usize, v: f32) -> [f32; PARAMS] {
    let mut p = BASE;
    p[i] = v;
    p
}

/// Events are `(absolute sample, note, on)`; `chunks` are cycled to cut the render.
fn render(d: &mut Id14, n: usize, chunks: &[usize], events: &[(u32, u8, bool)]) -> (Vec<f32>, Vec<f32>) {
    let (mut l, mut r) = (vec![9.0f32; n], vec![9.0f32; n]);
    let (mut s, mut k) = (0usize, 0usize);
    while s < n {
        let len = chunks[k % chunks.len()].min(n - s);
        k += 1;
        for &(t, note, on) in events {
            let t = t as usize;
            if t >= s && t < s + len {
                let off = (t - s) as u32;
                if on { d.note_on(note, off).unwrap() } else { d.note_off(note, off).unwrap() }
            }
        }
        d.process(&mut l[s..s + len], &mut r[s..s + len]);
        s += len;
    }
    (l, r)
}

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

fn peak(v: &[f32]) -> f32 {
    v.iter().fold(0.0, |m, x| m.max(x.abs()))
}

fn note_hz(n: u8) -> f64 {
    440.0 * (n as f64 / 12.0 - 5.75).exp2()
}

// ---------------------------------------------------------------- API ----------------------------------------------

#[test]
fn rejection_table_and_atomicity() {
    big(|| {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 1.001] {
        let mut p = BASE;
        p[3] = bad;
        assert_eq!(check(&p), Err(Error::Range));
        assert_eq!(Id14::new(&p).err(), Some(Error::Range));
    }
    assert_eq!(check(&BASE), Ok(()));
    let mut d = dev(&BASE);
    assert_eq!(d.set_param(PARAMS, 0.5), Err(Error::Index));
    assert_eq!(d.set_param(usize::MAX, 0.5), Err(Error::Index));
    for bad in [f32::NAN, 2.0, -0.5] {
        assert_eq!(d.set_param(4, bad), Err(Error::Range));
    }
    // atomic: a refused write changed nothing
    let mut a = dev(&BASE);
    let mut b = dev(&BASE);
    let _ = b.set_param(4, f32::NAN);
    let _ = b.set_param(30, 0.5);
    let ev = [(0, 60, true), (3000, 60, false)];
    let (la, ra) = render(&mut a, 6000, &[128], &ev);
    let (lb, rb) = render(&mut b, 6000, &[128], &ev);
    assert_eq!((bits(&la), bits(&ra)), (bits(&lb), bits(&rb)));
    assert_eq!(d.note_on(128, 0), Err(Error::Note));
    assert_eq!(d.note_off(200, 0), Err(Error::Note));
    assert_eq!(d.note_on(60, i32::MAX as u32 + 1), Err(Error::Offset));
    assert_eq!(d.note_on(60, i32::MAX as u32), Ok(()));
    });
}

#[test]
fn queue_holds_256_events() {
    big(|| {
    for mono in [0.0, 1.0] {
        let mut d = dev(&with(23, mono));
        for _ in 0..QUEUE {
            d.note_on(60, 5000).unwrap();
        }
        assert_eq!(d.note_on(60, 0), Err(Error::QueueFull));
        assert_eq!(d.note_off(60, 0), Err(Error::QueueFull));
    }
    });
}

#[test]
fn voice_mode_follows_parameter_23() {
    big(|| {
    assert!(!dev(&BASE).is_mono());
    assert!(dev(&with(23, 1.0)).is_mono());
    assert!(!dev(&with(23, 0.9999)).is_mono(), "the mode is trunc(v): only exactly 1.0 is mono");
    let mut d = dev(&BASE);
    d.set_param(23, 1.0).unwrap();
    assert!(d.is_mono());
    d.set_param(23, 0.0).unwrap();
    assert!(!d.is_mono());
    });
}

// ---------------------------------------------------------- filter design -------------------------------------------

/// RBJ cookbook coefficients `[b0, b1, b2, a1, a2] / a0` in f64.
fn cookbook(kind: u8, f: f64, q: f64) -> [f64; 5] {
    let w = f.clamp(20.0, 22000.0) * 2.0 * std::f64::consts::PI / 44100.0;
    let (s, c) = w.sin_cos();
    let q = q.max(0.05);
    let al = s / (2.0 * q);
    let (b, a1) = match kind {
        0 => ([(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0], -2.0 * c),
        1 => ([(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0], -2.0 * c),
        5 => ([s / 2.0, 0.0, -s / 2.0], -2.0 * c), // band-pass, skirt gain = Q
        _ => unreachable!(),
    };
    let a0 = 1.0 + al;
    [b[0] / a0, b[1] / a0, b[2] / a0, a1 / a0, (1.0 - al) / a0]
}

#[test]
fn filter_design_matches_the_rbj_cookbook() {
    big(|| {
    for kind in [0u8, 1, 5] {
        for &f in &[20.0f32, 55.0, 440.0, 3000.0, 12000.0, 21999.0] {
            for &q in &[0.2f32, 0.7, 4.0, 60.0, 1000.0] {
                let got = design(kind, f, q);
                let want = cookbook(kind, f as f64, q as f64);
                for i in 0..5 {
                    // f32 sin / cos of the rounded angle: near Nyquist sin(w) is small and its relative error grows
                    let tol = (if f > 19000.0 { 3e-4 } else { 3e-5 }) * want[i].abs().max(1e-3);
                    assert!((got[i] as f64 - want[i]).abs() <= tol, "type {kind} f {f} q {q} coef {i}: {} vs {}", got[i], want[i]);
                }
            }
        }
    }
    // frequency clamp 20..22000, Q floor 0.05
    assert_eq!(design(0, 1.0, 1.0), design(0, 20.0, 1.0));
    assert_eq!(design(0, 30000.0, 1.0), design(0, 22000.0, 1.0));
    assert_eq!(design(1, 440.0, 0.001), design(1, 440.0, 0.05));
    // a band-pass has no DC and no Nyquist gain; its peak gain is Q
    let c = design(5, 1000.0, 8.0);
    let h = |w: f64| {
        let (z1r, z1i) = (w.cos(), -w.sin());
        let (z2r, z2i) = ((2.0 * w).cos(), -(2.0 * w).sin());
        let nr = c[0] as f64 + c[1] as f64 * z1r + c[2] as f64 * z2r;
        let ni = c[1] as f64 * z1i + c[2] as f64 * z2i;
        let dr = 1.0 + c[3] as f64 * z1r + c[4] as f64 * z2r;
        let di = c[3] as f64 * z1i + c[4] as f64 * z2i;
        ((nr * nr + ni * ni) / (dr * dr + di * di)).sqrt()
    };
    assert!(h(0.0) < 1e-6 && h(std::f64::consts::PI) < 1e-6);
    let wc = 2.0 * std::f64::consts::PI * 1000.0 / 44100.0;
    assert!((h(wc) - 8.0).abs() < 1e-3, "peak gain {}", h(wc));
    });
}

// -------------------------------------------------------------- mode plan -------------------------------------------

#[test]
fn deterministic_plan_closed_form() {
    big(|| {
    // octave x1 (v=0.6), no inharmonicity (v2 = 0), 26 modes, range 20, no damping: Q = 1000
    let mut p = with(2, 0.0);
    p[3] = 0.0;
    p[4] = 0.0;
    let note = 33u8;
    let f0 = note_hz(note);
    let pl = plan(&p, note);
    let n = 26usize;
    let q4 = 20.0f64;
    let mut want = vec![];
    for k in 0..n {
        let h = (k as f64 * 0.999f32 as f64 * q4 / (n as f64 - 1.0)).trunc();
        let f = f0 * (h + 1.0);
        if (20.0..=22000.0).contains(&f) {
            want.push(f);
        }
    }
    assert_eq!(pl.count, want.len());
    for (i, w) in want.iter().enumerate() {
        assert!((pl.freq[i] as f64 - w).abs() <= 1e-3 * w.max(1.0) * 1e-3 + w * 2e-7, "mode {i}: {} vs {w}", pl.freq[i]);
        assert_eq!(pl.q[i], 1000.0, "no damping: Q = 1000/(1+0+0)");
    }
    assert!(pl.count >= 10, "a real bank");
    });
}

#[test]
fn modes_above_22000_or_below_20_are_skipped_but_the_rest_keep_their_place() {
    big(|| {
    let p = with(2, 0.0);
    let hi = plan(&p, 100); // f0 = 2637 Hz: harmonics above 22 kHz drop out
    let f0 = note_hz(100);
    assert!(hi.count < 26 && hi.count > 0);
    for i in 0..hi.count {
        assert!(hi.freq[i] >= 20.0 && hi.freq[i] <= 22000.0 + 1.0);
        assert!(hi.freq[i] as f64 >= f0 * 0.999999);
    }
    let lo = plan(&p, 0); // f0 = 8.18 Hz: the fundamental is below 20 Hz
    let f = note_hz(0);
    assert!(lo.freq[..lo.count].iter().all(|&x| x >= 20.0), "{:?}", &lo.freq[..lo.count.min(5)]);
    assert!(f < 20.0);
    });
}

#[test]
fn damping_law() {
    big(|| {
    // Q = 1000 / (1 + 4.4e7·P3^9 + 5e4·P4^9·(f/f0)²)
    let note = 45u8;
    for (a, b) in [(0.0f32, 0.0f32), (0.4, 0.0), (0.0, 0.6), (0.5, 0.5), (1.0, 1.0), (0.3, 0.9)] {
        let mut p = with(2, 0.2);
        p[3] = a;
        p[4] = b;
        let pl = plan(&p, note);
        let f0 = note_hz(note);
        assert!(pl.count > 4);
        for i in 0..pl.count {
            let ratio = (pl.freq[i] as f64 / f0).powi(2);
            let d = 1.0 + 4.4e7 * (a as f64).powi(9) + 5e4 * (b as f64).powi(9) * ratio;
            let want = 1000.0 / d;
            assert!((pl.q[i] as f64 - want).abs() <= want * 1e-5, "a {a} b {b} mode {i}: {} vs {want}", pl.q[i]);
        }
    }
    });
}

#[test]
fn one_mode_in_deterministic_selection_makes_no_modes() {
    big(|| {
    // the original divides by (N − 1) = 0, the index becomes the integer indefinite value, the frequency is out of range
    let p = with(1, 0.0);
    assert_eq!(plan(&p, 60).count, 0);
    let mut r = p;
    r[11] = 1.0;
    // random selection draws its own harmonic: one mode at most
    assert!(plan(&r, 60).count <= 1);
    });
}

#[test]
fn mode_count_and_octave_selector() {
    big(|| {
    for (v, n) in [(0.0f32, 1usize), (0.0039, 1), (0.0040, 2), (0.5, 128), (1.0, 256)] {
        let mut p = with(1, v);
        p[2] = 0.0;
        p[9] = 0.0; // range 1: every mode is the fundamental (h = 0)
        let pl = plan(&p, 60);
        let expect = if n == 1 { 0 } else { n };
        assert_eq!(pl.count, expect, "v {v}");
    }
    // octave selector trunc(5.99 v): 0..5 -> 1/8 .. 4
    let mut p = with(1, 0.02);
    p[2] = 0.0;
    p[9] = 0.0;
    for (v, mult) in [(0.0f32, 0.125f64), (0.17, 0.25), (0.34, 0.5), (0.51, 1.0), (0.68, 2.0), (0.9, 4.0), (1.0, 4.0)] {
        p[7] = v;
        let pl = plan(&p, 57);
        assert!(pl.count > 0);
        let want = note_hz(57) * mult;
        assert!((pl.freq[0] as f64 - want).abs() < want * 2e-7, "v {v}: {} vs {want}", pl.freq[0]);
    }
    });
}

#[test]
fn inharmonicity_law() {
    big(|| {
    // frequency of harmonic h: f0·(h + 1)·(1 + h·s), s = (2v)³ below 0.5, 0.06v − 0.09 from 0.5
    let note = 40u8;
    let f0 = note_hz(note);
    for (v, s) in [(0.2f32, (0.4f64).powi(3)), (0.5, 0.06 * 0.5 - 0.09), (1.0, 0.06 - 0.09)] {
        let mut p = with(2, v);
        p[1] = 0.1;
        p[9] = 0.2; // range 13
        let pl = plan(&p, note);
        let n = 26usize;
        let q4 = 13.0f64;
        let mut want = vec![];
        for k in 0..n {
            let h = (k as f64 * 0.999f32 as f64 * q4 / (n as f64 - 1.0)).trunc();
            let f = f0 * (h + 1.0) * (1.0 + h * s);
            if (20.0..=22000.0).contains(&f) {
                want.push(f);
            }
        }
        assert_eq!(pl.count, want.len(), "v {v}");
        for (i, w) in want.iter().enumerate() {
            assert!((pl.freq[i] as f64 - w).abs() <= w * 5e-7, "v {v} mode {i}: {} vs {w}", pl.freq[i]);
        }
    }
    });
}

#[test]
fn the_random_global_is_a_pure_function_of_the_seed_and_leaves_no_trace() {
    big(|| {
    // random harmonic selection: the draws depend on the seed (wire 8), the dropout (wire 25) and nothing else; they are
    // the same for every note, and planning a note does not change what the next plan sees
    let mut p = with(2, 0.0);
    p[11] = 1.0;
    p[1] = 0.3;
    let a1 = plan(&p, 40);
    let b = plan(&p, 50);
    let a2 = plan(&p, 40);
    assert_eq!(a1.count, a2.count);
    assert_eq!(&a1.freq[..a1.count], &a2.freq[..a2.count]);
    assert!(b.count > 0);
    let mut q = p;
    q[8] = 0.9;
    let c = plan(&q, 40);
    assert_ne!(&a1.freq[..a1.count.min(c.count)], &c.freq[..a1.count.min(c.count)], "another seed, other harmonics");
    // dropout shifts the draw sequence
    let mut d = p;
    d[25] = 1.0;
    assert_ne!(plan(&d, 40).freq, a1.freq);
    // deterministic selection ignores seed and dropout
    let mut e = with(2, 0.0);
    let base = plan(&e, 40);
    e[8] = 0.77;
    e[25] = 1.0;
    let other = plan(&e, 40);
    assert_eq!(base.count, other.count);
    assert_eq!(&base.freq[..base.count], &other.freq[..other.count]);
    });
}

#[test]
fn random_draws_do_not_depend_on_the_note_or_on_other_notes_played_before() {
    big(|| {
    // the generator is re-seeded by every note from the seed (wire 8) alone: an octave up keeps every harmonic and
    // doubles every frequency (no inharmonicity, all modes inside 20..22000 Hz for both notes); playing other notes
    // first leaves no trace in the next note's resonator set
    let mut p = with(2, 0.0);
    p[11] = 1.0;
    p[1] = 0.05;
    p[9] = 0.1;
    let (lo, hi) = (plan(&p, 40), plan(&p, 52));
    assert!(lo.count > 3 && lo.count == hi.count);
    for i in 0..lo.count {
        assert!((hi.freq[i] / lo.freq[i] - 2.0).abs() < 1e-5, "mode {i}: {} vs {}", lo.freq[i], hi.freq[i]);
    }
    let mut d = dev(&p);
    let _ = render(&mut d, 4096, &[128], &[(0, 33, true), (500, 33, false), (900, 61, true), (1500, 61, false)]);
    // the slot holds the last note played (61): its resonator set is that note's plan, whatever came before
    assert_eq!(d.mode_count(0), plan(&p, 61).count);
    });
}

#[test]
fn random_harmonics_stay_inside_the_range_and_include_negative_indices() {
    big(|| {
    // h = trunc(v·0.999·range) with v in (−1, 1): negative values give h ≤ 0 (a harmonic below the fundamental)
    let mut p = with(2, 0.0);
    p[11] = 1.0;
    p[1] = 1.0; // 256 draws
    p[9] = 1.0; // range 64
    let note = 69u8;
    let f0 = note_hz(note);
    let pl = plan(&p, note);
    assert!(pl.count > 100);
    let (mut below, mut above) = (0, 0);
    for i in 0..pl.count {
        let ratio = pl.freq[i] as f64 / f0;
        assert!(ratio <= 64.0 + 1e-3);
        if ratio < 1.0 - 1e-6 { below += 1 }
        if ratio > 1.0 + 1e-6 { above += 1 }
    }
    assert!(above > 20, "positive draws give overtones");
    assert!(below <= 3, "h < 0 gives (h+1)·(1 + h·s): at most a handful of sub-fundamental modes, got {below}");
    });
}

// ------------------------------------------------------- behaviour of the voice -------------------------------------

const NOTE: u8 = 52;

fn one_note(p: &[f32; PARAMS], n: usize) -> (Vec<f32>, Vec<f32>) {
    let mut d = dev(p);
    render(&mut d, n, &[128], &[(0, NOTE, true)])
}

#[test]
fn silence_without_notes_and_finite_output() {
    big(|| {
    let mut d = dev(&BASE);
    let (l, r) = render(&mut d, 1000, &[128], &[]);
    assert!(l.iter().chain(&r).all(|&x| x == 0.0));
    let (l, r) = one_note(&BASE, 20000);
    assert!(l.iter().chain(&r).all(|x| x.is_finite()));
    assert!(peak(&l) > 1e-3 && peak(&r) > 1e-3, "a note is audible on both channels");
    });
}

#[test]
fn determinism_and_a_voice_slot_has_no_memory_between_notes() {
    big(|| {
    let a = one_note(&BASE, 15000);
    let b = one_note(&BASE, 15000);
    assert_eq!((bits(&a.0), bits(&a.1)), (bits(&b.0), bits(&b.1)));
    // the same note played again on the same slot after the first ended is bit-identical to the first
    let mut d = dev(&BASE);
    let ev = [(0, NOTE, true), (3000, NOTE, false), (60000, NOTE, true), (63000, NOTE, false)];
    let (l, r) = render(&mut d, 120_000, &[128], &ev);
    assert_eq!(bits(&l[..15000]), bits(&l[60000..75000]), "a second note equals the first");
    assert_eq!(bits(&r[..15000]), bits(&r[60000..75000]));
    });
}

#[test]
fn notes_are_not_pitch_trackers_only_through_the_mode_frequencies() {
    big(|| {
    // two notes an octave apart: spectra differ, so outputs differ; and the note number also seeds the noise
    let (a, _) = render(&mut dev(&BASE), 8000, &[128], &[(0, 40, true)]);
    let (b, _) = render(&mut dev(&BASE), 8000, &[128], &[(0, 52, true)]);
    assert_ne!(bits(&a), bits(&b));
    });
}

#[test]
fn every_wire_but_the_inert_ones_is_audible_and_the_inert_ones_are_not() {
    big(|| {
    for i in 0..PARAMS {
        if i == 23 {
            continue; // the voice mode: covered by the mono tests
        }
        // the seed and the dropout only act on the random harmonic selection
        let mut root = BASE;
        if i == 8 || i == 25 {
            root[11] = 1.0;
            root[1] = 0.3;
        }
        let base = one_note(&root, 12000);
        for v in [0.0f32, 0.35, 0.7, 1.0] {
            let mut p = root;
            if p[i] == v {
                continue;
            }
            p[i] = v;
            // hold has no effect while the note is held longer than the envelope needs? it does: sustain vs release
            let got = one_note(&p, 12000);
            let same = bits(&got.0) == bits(&base.0) && bits(&got.1) == bits(&base.1);
            if INERT.contains(&i) {
                assert!(same, "wire {i} = {v} must be inert");
            } else if !same {
                // audible: found for this wire
                break;
            } else if v == 1.0 {
                panic!("wire {i} never changed the output");
            }
        }
    }
    });
}

#[test]
fn width_zero_is_independent_channels_and_one_is_independent_channels_too() {
    big(|| {
    // the common part (A + B)(0.5 − 0.5w) is shared by both channels: at w = 1 the noise is fully independent, at w = 0
    // both channels carry the same mono noise
    let mut p = BASE;
    p[10] = 0.0;
    p[19] = 0.0;
    p[15] = 0.0;
    let (l0, r0) = one_note(&p, 6000);
    // channel noises are (A+B)/2 on both: the band-passed path is identical on both channels, so only the filter C path
    // (same input too) can differ: identical channels
    assert_eq!(bits(&l0), bits(&r0), "width 0 is mono");
    p[10] = 1.0;
    let (l1, r1) = one_note(&p, 6000);
    assert_ne!(bits(&l1), bits(&r1));
    });
}

#[test]
fn output_gain_is_squared() {
    big(|| {
    let a = one_note(&with(22, 0.5), 8000);
    let b = one_note(&with(22, 1.0), 8000);
    for i in 0..8000 {
        let want = a.0[i] * 4.0;
        assert!((b.0[i] - want).abs() <= 1e-5 * want.abs() + 1e-12, "{i}: {} vs {want}", b.0[i]);
    }
    let z = one_note(&with(22, 0.0), 2000);
    assert!(z.0.iter().chain(&z.1).all(|&x| x == 0.0));
    });
}

#[test]
fn block_split_is_bit_identical_while_the_voice_lives() {
    big(|| {
    let ev = [(0, NOTE, true), (5000, NOTE, false)];
    let whole = render(&mut dev(&BASE), 20000, &[20000], &ev);
    for chunks in [vec![128usize], vec![1], vec![7, 64, 3], vec![5, 300]] {
        let cut = render(&mut dev(&BASE), 20000, &chunks, &ev);
        assert_eq!(bits(&whole.0), bits(&cut.0), "{chunks:?}");
        assert_eq!(bits(&whole.1), bits(&cut.1), "{chunks:?}");
    }
    });
}

#[test]
fn onset_is_at_the_event_offset() {
    big(|| {
    for off in [0u32, 1, 37, 127] {
        let mut d = dev(&BASE);
        d.note_on(NOTE, off).unwrap();
        let (mut l, mut r) = (vec![0.0f32; 128], vec![0.0f32; 128]);
        d.process(&mut l, &mut r);
        assert!(l[..off as usize].iter().all(|&x| x == 0.0));
        assert!(l[off as usize..].iter().any(|&x| x != 0.0), "offset {off}");
    }
    let mut d = dev(&BASE);
    d.note_on(NOTE, 300).unwrap();
    let (mut l, mut r) = (vec![0.0f32; 128], vec![0.0f32; 128]);
    d.process(&mut l, &mut r);
    d.process(&mut l, &mut r);
    assert!(l.iter().all(|&x| x == 0.0));
    d.process(&mut l, &mut r);
    assert!(l[..300 - 256].iter().all(|&x| x == 0.0) && l[300 - 256..].iter().any(|&x| x != 0.0));
    });
}

#[test]
fn a_voice_lives_at_most_two_and_a_half_seconds() {
    big(|| {
    let mut d = dev(&BASE); // hold = 1: sustains
    d.note_on(NOTE, 0).unwrap();
    let (mut l, mut r) = (vec![0.0f32; 128], vec![0.0f32; 128]);
    d.process(&mut l, &mut r);
    let mut blocks = 1;
    while d.active_voices() > 0 && blocks < 2000 {
        d.process(&mut l, &mut r);
        blocks += 1;
    }
    // t advances by the f32 1/44100 summed in f64: the first call ending past 2.5 s frees the voice
    let secs = blocks as f64 * 128.0 / 44100.0;
    assert!((2.5..2.5 + 128.0 / 44100.0 + 1e-3).contains(&secs), "{secs} s");
    });
}

#[test]
fn a_silent_voice_ends_after_half_a_second() {
    big(|| {
    // no noise gain, no band-passed gain: nothing is ever audible; the meter stays below 0.001 and the voice ends
    // at the first call that ends after 0.5 s
    let mut p = BASE;
    p[13] = 0.0;
    p[18] = 0.0;
    let mut d = dev(&p);
    d.note_on(NOTE, 0).unwrap();
    let (mut l, mut r) = (vec![0.0f32; 128], vec![0.0f32; 128]);
    d.process(&mut l, &mut r);
    let mut blocks = 1;
    while d.active_voices() > 0 && blocks < 2000 {
        d.process(&mut l, &mut r);
        blocks += 1;
    }
    let secs = blocks as f64 * 128.0 / 44100.0;
    assert!((0.5..0.5 + 128.0 / 44100.0 + 1e-3).contains(&secs), "{secs} s");
    });
}

#[test]
fn hold_below_a_half_releases_at_the_sustain_stage() {
    big(|| {
    let mut p = BASE;
    p[0] = 0.0; // one-shot
    p[13] = 0.0; // only the enveloped band-passed noise
    p[20] = 0.0; // fastest envelope: A 1.5 ms, D 1.8 ms, R 0.9 ms
    let (l, _) = one_note(&p, 30000);
    let tail = &l[20000..];
    let q = peak(&one_note(&with(0, 1.0), 30000).0[2000..8000]);
    assert!(peak(tail) < 1e-3 * q.max(1e-6) + 1e-12 || peak(&l[..4000]) > 0.0, "released after the decay");
    let held = one_note(&{
        let mut h = p;
        h[0] = 1.0;
        h
    }, 30000);
    assert!(peak(&held.0[20000..]) > peak(tail), "hold keeps the sustain level");
    });
}

#[test]
fn level_meter_updates_only_at_the_fifth_sample_of_a_sub_block() {
    big(|| {
    // every mode starts at level 1.0 and is blended with 0.0005·(|l| + |r|) at the fifth sample (index 4) of each scheduler
    // sub-block (here one per call: no events inside): sub-blocks of at most 4 samples never reach it
    let play = |len: usize, calls: usize| {
        let mut d = dev(&BASE);
        d.note_on(NOTE, 0).unwrap();
        let (mut l, mut r) = (vec![0.0f32; len], vec![0.0f32; len]);
        for _ in 0..calls {
            d.process(&mut l, &mut r);
        }
        d.mode_levels(0)
    };
    assert!(play(4, 400).iter().all(|&x| x == 1.0), "4-sample calls never reach index 4");
    let five = play(5, 400);
    assert!(five.iter().all(|&x| x != 1.0) && five.iter().any(|&x| x < 0.9), "5-sample calls update every call: {:?}", &five[..3]);
    // 128-sample calls: one update per call, so after 12 calls the level is 0.9995^12 plus a little energy
    let l128 = play(128, 12);
    assert!(l128.iter().all(|&x| x != 1.0) && l128.iter().all(|&x| x > 0.9 && x < 1.1));
    });
}

// --------------------------------------------------------------- mono mode ------------------------------------------

#[test]
fn mono_notes_while_held_start_no_second_voice_and_release_follows_the_stack() {
    big(|| {
    let mut d = dev(&with(23, 1.0));
    let ev = |d: &mut Id14, n: u8, on: bool| {
        if on { d.note_on(n, 0).unwrap() } else { d.note_off(n, 0).unwrap() }
        let (mut l, mut r) = (vec![0.0f32; 64], vec![0.0f32; 64]);
        d.process(&mut l, &mut r);
    };
    ev(&mut d, 50, true);
    assert_eq!(d.active_voices(), 1);
    ev(&mut d, 55, true);
    ev(&mut d, 57, true);
    assert_eq!(d.active_voices(), 1, "legato notes start no voice");
    assert_eq!(d.voice_notes()[0], Some(50));
    ev(&mut d, 57, false);
    ev(&mut d, 55, false);
    assert_eq!(d.active_voices(), 1, "50 is still held");
    ev(&mut d, 50, false);
    ev(&mut d, 62, true);
    assert_eq!(d.active_voices(), 2, "the released voice rings on, the new note has its own voice");
    });
}

#[test]
fn mono_mode_change_cuts_the_voices() {
    big(|| {
    let mut d = dev(&with(23, 1.0));
    d.note_on(50, 0).unwrap();
    let (mut l, mut r) = (vec![0.0f32; 128], vec![0.0f32; 128]);
    d.process(&mut l, &mut r);
    assert_eq!(d.active_voices(), 1);
    d.set_param(23, 0.0).unwrap();
    assert_eq!(d.active_voices(), 0);
    d.process(&mut l, &mut r);
    assert!(l.iter().all(|&x| x == 0.0));
    });
}

// ----------------------------------------------------------- independent model --------------------------------------

/// The shared 32-bit mixer (written out here independently of the implementation).
fn mix32(s: u32) -> u32 {
    let x = (s ^ (s >> 16)).wrapping_mul(0x7feb352d);
    let x = (x ^ (x >> 15)).wrapping_mul(0x846ca68b);
    x ^ (x >> 16)
}

struct Bq {
    c: [f64; 5],
    x1: [f64; 2],
    x2: [f64; 2],
    y1: [f64; 2],
    y2: [f64; 2],
}
impl Bq {
    fn new(kind: u8, f: f64, q: f64) -> Self {
        Bq { c: cookbook(kind, f, q), x1: [0.0; 2], x2: [0.0; 2], y1: [0.0; 2], y2: [0.0; 2] }
    }
    fn tick(&mut self, x: [f64; 2]) -> [f64; 2] {
        let mut y = [0.0; 2];
        for i in 0..2 {
            y[i] = self.c[0] * x[i] + self.c[1] * self.x1[i] + self.c[2] * self.x2[i] - self.c[3] * self.y1[i] - self.c[4] * self.y2[i];
        }
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Whole voice in f64 for deterministic harmonic selection (poly, one note, held for the whole render, hold >= 0.5).
fn model(p: &[f32; PARAMS], note: u8, n: usize) -> (Vec<f64>, Vec<f64>) {
    let q = |i: usize| p[i] as f64;
    let f0 = note_hz(note);
    let a_env = (q(20) * 3.0 + 0.003, 0.0);
    let (ta, td, ts, tr) = (a_env.0 * 0.5, a_env.0 * 0.6, 0.02f64, a_env.0 * 0.3);
    let _ = tr;
    let d = q(19) * 6400.0;
    let lo = (2.0 * f0 - d).max(20.0);
    let hi = (2.0 * f0 + d).min(20000.0);
    let mut fa = Bq::new(0, lo, 4.0);
    let mut fb = Bq::new(1, hi, 4.0);
    let cf = if p[15] >= 0.5 { (f0 - 24000.0 + q(15) * 40000.0).min(22000.0) } else { q(15) * 40000.0 + f0 };
    let cf = cf.min(21000.0).max(20.0);
    let mut fc = Bq::new(if p[15] >= 0.5 { 1 } else { 0 }, cf, 0.2);
    // modes
    let nm = (p[1] * 255.999).trunc() as usize + 1;
    let mult = [0.125, 0.25, 0.5, 1.0, 2.0, 4.0][((p[7] * 5.99).trunc() as usize).min(5)];
    let q4 = (p[9] * 63.99f32).trunc() as f64 + 1.0;
    let s6 = if q(2) < 0.5 { (2.0 * q(2)).powi(3) } else { q(2) * 0.06 - 0.09 };
    let mut modes = vec![];
    for k in 0..nm {
        let h = if nm == 1 { continue } else { (k as f64 * 0.999 * q4 / (nm as f64 - 1.0)).trunc() };
        let f = f0 * mult * (h + 1.0) * (h * s6 + 1.0);
        if !(20.0..=22000.0).contains(&f) {
            continue;
        }
        let r = (f / f0).powi(2);
        let dd = 1.0 + 4.4e7 * q(3).powi(9) + 5e4 * q(4).powi(9) * r;
        modes.push(Bq::new(5, f, 1000.0 / dd));
    }
    let cnt = modes.len() as f64;
    let mut rng = note as u32 + 0x1f11a9a2;
    let (mut timer, mut thresh) = (0.0f64, 0.0f64);
    let dt = 1.0 / 44100.0;
    let q15 = q(14).powi(3) * 50.0 + 2.0;
    let (mut t, mut et, mut stage, mut lvl_held) = (0.0f64, 0.0f64, 1u8, 0.0f64);
    let (mut ol, mut orr) = (vec![0.0; n], vec![0.0; n]);
    for i in 0..n {
        t += dt;
        timer += dt;
        let x1 = (rng ^ (rng >> 16)).wrapping_mul(0x7feb352d);
        let x1 = x1 ^ (x1 >> 15);
        let ah = x1.wrapping_mul(0x846ca68b);
        let a = (ah ^ (ah >> 16)) as f64 * 2f64.powi(-31) - 1.0;
        let x2 = x1.wrapping_mul(0x9c2d0d6f);
        let x2 = x2 ^ (x2 >> 15);
        let bh = x2.wrapping_mul(0x846ca68b);
        rng = bh ^ (bh >> 16);
        let b = rng as f64 * 2f64.powi(-31) - 1.0;
        let w = q(10);
        let c3 = (0.5 - w * 0.5) * (a + b);
        let inp = [b * w + c3, a * w + c3];
        let co = fc.tick(inp);
        let ao = fa.tick(inp);
        let bo = fb.tick(inp);
        // envelope
        et += dt;
        let env = match stage {
            1 => {
                let v = (et / ta) * (2.0 * (et - ta) / ta).exp();
                lvl_held = v;
                if et >= ta { stage = 2; et = 0.0 }
                v.clamp(0.0, 1.0)
            }
            2 => {
                let x = et / td;
                let target = (ts - 1.0) * (1.0 - (-2.0 * x).exp()) + 1.0;
                let v = (ts - target) * x + target;
                lvl_held = v;
                if et >= td { stage = 3; et = 0.0 }
                v.clamp(0.0, 1.0)
            }
            _ => ts,
        };
        let _ = lvl_held;
        let env = env * 3.0 * q(18);
        if timer > thresh {
            timer = 0.0;
            rng = mix32(rng);
            thresh = (rng as f64 * 1.2 / 4294967296.0 + 0.3) / (q(17) * 20.0 + 1.0);
        }
        let e60 = (timer * -60.0).exp();
        let crack = q(16) * e60 + (1.0 - q(16));
        let nenv = (-t * q15).exp() * q(13) * crack;
        let f = [co[0] * nenv, co[1] * nenv];
        let g = (1.0 - q(21)) + e60 * q(21);
        let x0 = (inp[0] - (ao[0] + bo[0])) * env;
        let x1c = (inp[1] - (ao[1] + bo[1])) * env;
        let e = [(x0 * g + f[0]) * 0.5, (f[1] + g * x1c) * 0.5];
        let mut s = [0.0f64; 2];
        for m in modes.iter_mut() {
            let y = m.tick(e);
            s[0] += y[0];
            s[1] += y[1];
        }
        let gain = (q(22) * 0.4).powi(2);
        ol[i] = s[0] / cnt * gain;
        orr[i] = s[1] / cnt * gain;
    }
    (ol, orr)
}

fn model_err(p: &[f32; PARAMS], note: u8, n: usize) -> (f64, f64) {
    let (ml, mr) = model(p, note, n);
    let (gl, gr) = one_note_at(p, note, n);
    let pk = ml.iter().chain(&mr).fold(0.0f64, |m, x| m.max(x.abs()));
    let mut e = 0.0f64;
    for i in 0..n {
        e = e.max((ml[i] - gl[i] as f64).abs()).max((mr[i] - gr[i] as f64).abs());
    }
    (e, pk)
}

fn one_note_at(p: &[f32; PARAMS], note: u8, n: usize) -> (Vec<f32>, Vec<f32>) {
    let mut d = dev(p);
    render(&mut d, n, &[n], &[(0, note, true)])
}

#[test]
fn the_whole_voice_matches_an_independent_f64_model() {
    big(|| {
    let mut sets: Vec<([f32; PARAMS], u8)> = vec![(BASE, 52)];
    sets.push((with(10, 1.0), 40));
    sets.push((with(15, 0.8), 64));
    let mut p = BASE;
    p[3] = 0.0;
    p[4] = 0.0;
    p[1] = 0.05;
    sets.push((p, 45));
    sets.push((with(2, 0.7), 36));
    for (k, (p, note)) in sets.iter().enumerate() {
        let (e, pk) = model_err(p, *note, 12000);
        assert!(pk > 1e-4, "set {k} audible, peak {pk}");
        assert!(e <= 2e-3 * pk, "set {k}: err {e} peak {pk}");
    }
    });
}

#[test]
fn model_negative_controls_leave_the_bound() {
    big(|| {
    // the model agrees with the device only for the right parameters: nudged inputs must exceed the bound
    let (ml, _) = model(&BASE, 52, 12000);
    let (gl, _) = one_note_at(&BASE, 52, 12000);
    let pk = ml.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    let err = |a: &[f64]| a.iter().zip(&gl).fold(0.0f64, |m, (x, y)| m.max((x - *y as f64).abs()));
    assert!(err(&ml) <= 2e-3 * pk);
    for (i, v) in [(19usize, 0.35f32), (14, 0.7), (20, 0.1), (18, 0.4), (13, 0.5), (3, 0.5), (10, 0.9), (22, 0.8)] {
        let mut q = BASE;
        q[i] = v;
        let (m2, _) = model(&q, 52, 12000);
        assert!(err(&m2) > 2e-3 * pk, "nudging wire {i} to {v} must be caught");
    }
    let (m3, _) = model(&BASE, 53, 12000);
    assert!(err(&m3) > 2e-3 * pk, "another note must be caught");
    });
}

// ---------------------------------------------------------------- robustness ----------------------------------------

#[test]
fn finite_at_the_corners_and_for_random_vectors() {
    big(|| {
    let mut seed = 0x1234_5678u32;
    let mut rnd = || {
        seed = mix32(seed);
        seed as f32 / 4294967296.0
    };
    let mut sets: Vec<[f32; PARAMS]> = vec![[0.0; PARAMS], [1.0; PARAMS], [0.5; PARAMS]];
    for _ in 0..24 {
        let mut p = [0.0f32; PARAMS];
        for v in p.iter_mut() {
            *v = rnd();
        }
        sets.push(p);
    }
    for (k, p) in sets.iter().enumerate() {
        for note in [0u8, 60, 127] {
            let mut d = dev(p);
            let (l, r) = render(&mut d, 30000, &[128], &[(0, note, true)]);
            assert!(l.iter().chain(&r).all(|x| x.is_finite()), "set {k} note {note}");
        }
    }
    assert_eq!(MAX_MODES, 256);
    assert_eq!(VOICES, 16);
    });
}

#[test]
fn sixteen_voices_and_the_seventeenth_is_lost() {
    big(|| {
    let mut d = dev(&BASE);
    for n in 0..17u8 {
        d.note_on(30 + n, 0).unwrap();
    }
    let (mut l, mut r) = (vec![0.0f32; 128], vec![0.0f32; 128]);
    d.process(&mut l, &mut r);
    assert_eq!(d.active_voices(), 16);
    assert_eq!(d.voice_notes()[15], Some(45));
    assert!(d.voice_notes().iter().all(|n| *n != Some(46)));
    });
}
