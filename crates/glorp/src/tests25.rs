//! Synthetic-only tests for ID 25 (percussion synth with the shared 16-voice `SynthDevice` scheduling). No song data,
//! recorded tap or proprietary vector is used here; the expectations are independent closed forms, an independent
//! f64 model of the body path, and structural properties of the voice scheduler (research/glorp-devices.md §ID 25).
use crate::id25::{hash, rbj, soft_limit, Error, Id25, MAX_NOTE, PARAMS, QUEUE, VOICES};

/// The song's own settings of its one ID 25 instance (f32 values of the bytes/255 style chunk).
const SONG: [f32; PARAMS] = [
    0.350744337, 0.573000014, 0.202999994, 0.370000005, 0.745000005, 0.254999995, 0.5, 0.843999982, 0.50999999,
    0.63499999, 0.765999973, 0.379999995, 0.86500001, 0.442999989, 0.5,
];

fn dev(p: &[f32; PARAMS]) -> Id25 {
    Id25::new(p).unwrap()
}

fn with(i: usize, v: f32) -> [f32; PARAMS] {
    let mut p = SONG;
    p[i] = v;
    p
}

/// Renders `n` samples in the given chunk sizes (cycled), queueing every `(absolute sample, note, on)` event with the
/// offset relative to the chunk it falls in, in list order.
fn render(d: &mut Id25, n: usize, chunks: &[usize], events: &[(u32, u8, bool)]) -> (Vec<f32>, Vec<f32>) {
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

// ---------------------------------------------------------------- API contract

#[test]
fn rejects_unsupported_settings_and_stays_unchanged() {
    for bad in [f32::NAN, f32::INFINITY, -0.001, 1.001] {
        assert_eq!(Id25::new(&with(3, bad)).err(), Some(Error::Range), "{bad}");
    }
    let mut d = dev(&SONG);
    let mut e = dev(&SONG);
    assert_eq!(d.set_param(PARAMS, 0.5), Err(Error::Index));
    for bad in [f32::NAN, 1.5, -1.0] {
        assert_eq!(d.set_param(9, bad), Err(Error::Range));
    }
    assert_eq!(d.note_on(MAX_NOTE + 1, 0), Err(Error::Note));
    assert_eq!(d.note_off(200, 0), Err(Error::Note));
    assert_eq!(d.note_on(60, i32::MAX as u32 + 1), Err(Error::Offset));
    assert_eq!(d.note_off(60, u32::MAX), Err(Error::Offset));
    // none of the rejected calls left a trace: both devices render the same note identically
    d.note_on(60, 0).unwrap();
    e.note_on(60, 0).unwrap();
    let (mut a, mut b) = ((vec![0.0; 600], vec![0.0; 600]), (vec![0.0; 600], vec![0.0; 600]));
    d.process(&mut a.0, &mut a.1);
    e.process(&mut b.0, &mut b.1);
    assert_eq!(bits(&a.0), bits(&b.0));
    assert_eq!(bits(&a.1), bits(&b.1));
    assert!(peak(&a.0) > 0.0);
}

#[test]
fn event_queue_holds_256_and_refuses_the_next() {
    let mut d = dev(&SONG);
    for i in 0..QUEUE {
        d.note_on((i % 128) as u8, 10).unwrap();
    }
    assert_eq!(d.note_on(1, 10), Err(Error::QueueFull));
    assert_eq!(d.note_off(1, 10), Err(Error::QueueFull));
    // draining frees every slot again
    let (mut l, mut r) = (vec![0.0; 16], vec![0.0; 16]);
    d.process(&mut l, &mut r);
    assert_eq!(d.active_voices(), VOICES, "256 queued note-ons fill all 16 voices, the rest is dropped");
    d.note_on(1, 0).unwrap();
}

#[test]
fn silent_without_notes_and_overwrites_the_buffer() {
    let mut d = dev(&SONG);
    let (mut l, mut r) = (vec![1.0f32; 300], vec![-2.0f32; 300]);
    d.process(&mut l, &mut r);
    assert!(l.iter().chain(r.iter()).all(|&x| x == 0.0), "an instrument writes, it does not add to the input");
}

#[test]
#[should_panic(expected = "channel length mismatch")]
fn unequal_channel_lengths_panic() {
    dev(&SONG).process(&mut [0.0; 4], &mut [0.0; 5]);
}

#[test]
fn zero_length_block_is_a_no_op() {
    let mut d = dev(&SONG);
    d.note_on(60, 0).unwrap();
    d.process(&mut [], &mut []);
    assert_eq!(d.active_voices(), 0, "events only fire inside a block with samples");
}

// ---------------------------------------------------------------- scheduling

#[test]
fn first_sample_is_at_the_event_offset() {
    for off in [0u32, 1, 37, 127] {
        let mut d = dev(&SONG);
        let (l, r) = render(&mut d, 400, &[400], &[(off, 60, true)]);
        let first = l.iter().position(|&x| x != 0.0).unwrap();
        assert_eq!(first as u32, off);
        assert!(l[..first].iter().chain(r[..first].iter()).all(|&x| x == 0.0));
        assert!(r[first] != 0.0);
    }
}

#[test]
fn rendering_is_independent_of_the_block_split() {
    let ev: Vec<(u32, u8, bool)> = vec![
        (100, 40, true),
        (1500, 45, true),
        (900, 40, false),
        (2999, 45, false),
        (3001, 70, true),
        (3001, 71, true),
        (3000, 70, false),
    ];
    let mut whole = dev(&SONG);
    let a = render(&mut whole, 6000, &[6000], &ev);
    for chunks in [&[1usize][..], &[128], &[127, 129, 3, 1000], &[5000, 1]] {
        let mut d = dev(&SONG);
        let b = render(&mut d, 6000, chunks, &ev);
        assert_eq!(bits(&a.0), bits(&b.0), "{chunks:?}");
        assert_eq!(bits(&a.1), bits(&b.1), "{chunks:?}");
    }
    assert!(peak(&a.0) > 0.0 && peak(&a.1) > 0.0);
}

#[test]
fn seventeenth_simultaneous_note_is_dropped_voices_fill_in_order() {
    let notes: Vec<u8> = (0..17).map(|i| 20 + i).collect();
    let mut d = dev(&SONG);
    for &n in &notes {
        d.note_on(n, 0).unwrap();
    }
    d.process(&mut [0.0], &mut [0.0]);
    let got = d.voice_notes();
    for i in 0..VOICES {
        assert_eq!(got[i], Some(20 + i as u8), "voice {i} takes the i-th note (lowest free voice first)");
    }
    assert_eq!(d.active_voices(), 16);
    // the dropped 17th note changes nothing audible
    let ev16: Vec<_> = notes[..16].iter().map(|&n| (0, n, true)).collect();
    let ev17: Vec<_> = notes.iter().map(|&n| (0, n, true)).collect();
    let a = render(&mut dev(&SONG), 3000, &[3000], &ev16);
    let b = render(&mut dev(&SONG), 3000, &[3000], &ev17);
    assert_eq!(bits(&a.0), bits(&b.0));
    assert_eq!(bits(&a.1), bits(&b.1));
}

/// Number of per-sample envelope ticks until a release of 0.5 s ends, with the envelope clock accumulating in f32 (the
/// reference's per-sample `t += 1/44100f`), written independently of the implementation.
fn release_ticks() -> usize {
    let dt = 2.2675737e-5f32;
    let (mut t, mut k) = (0.0f32, 0usize);
    loop {
        t += dt;
        k += 1;
        if t >= 0.5 {
            return k;
        }
    }
}

#[test]
fn released_voice_is_freed_after_exactly_half_a_second() {
    let off_at = 100usize;
    let mut d = dev(&SONG);
    d.note_on(60, 0).unwrap();
    d.note_off(60, off_at as u32).unwrap();
    let k = release_ticks();
    assert!((22_000..22_200).contains(&k), "0.5 s at 44.1 kHz, got {k}");
    // one sample per call: the voice frees itself at the end of the sample that finishes the release
    let mut freed_at = None;
    for i in 0..off_at + k + 50 {
        d.process(&mut [0.0], &mut [0.0]);
        if freed_at.is_none() && d.active_voices() == 0 {
            freed_at = Some(i);
        }
    }
    assert_eq!(freed_at, Some(off_at + k - 1), "sample {off_at} is the first release tick");
}

#[test]
fn unreleased_voice_never_frees_and_never_goes_non_finite() {
    let mut d = dev(&SONG);
    d.note_on(23, 0).unwrap();
    let (l, r) = render(&mut d, 44_100 * 6, &[128], &[]);
    assert_eq!(d.active_voices(), 1);
    assert_eq!(d.voice_notes()[0], Some(23));
    assert!(l.iter().chain(r.iter()).all(|x| x.is_finite()));
}

#[test]
fn note_off_releases_every_voice_of_that_note_and_only_that_note() {
    let mut d = dev(&SONG);
    for n in [50u8, 51, 50, 52] {
        d.note_on(n, 0).unwrap();
    }
    d.process(&mut [0.0; 10], &mut [0.0; 10]);
    assert_eq!(d.active_voices(), 4);
    d.note_off(50, 0).unwrap();
    d.note_off(99, 0).unwrap(); // not playing: ignored
    let _ = render(&mut d, 44_100, &[1000], &[]);
    let notes: Vec<_> = d.voice_notes().iter().flatten().copied().collect();
    assert_eq!(notes, vec![51, 52], "both retriggered voices of note 50 released, 51/52 and the stray off untouched");
}

#[test]
fn same_offset_events_run_in_queue_order() {
    // off queued before on: the off finds no voice, the new voice survives
    let mut a = dev(&SONG);
    a.note_off(40, 77).unwrap();
    a.note_on(40, 77).unwrap();
    // on queued before off: released
    let mut b = dev(&SONG);
    b.note_on(40, 77).unwrap();
    b.note_off(40, 77).unwrap();
    for d in [&mut a, &mut b] {
        let _ = render(d, 44_100, &[500], &[]);
    }
    assert_eq!(a.active_voices(), 1);
    assert_eq!(b.active_voices(), 0);
}

#[test]
fn offsets_longer_than_a_block_stay_queued() {
    let mut d = dev(&SONG);
    d.note_on(60, 300).unwrap();
    let mut first = None;
    for blk in 0..4 {
        let (mut l, mut r) = (vec![0.0f32; 128], vec![0.0f32; 128]);
        d.process(&mut l, &mut r);
        if first.is_none() {
            first = l.iter().position(|&x| x != 0.0).map(|i| blk * 128 + i);
        }
    }
    assert_eq!(first, Some(300));
}

#[test]
fn retriggered_note_after_release_reuses_the_lowest_free_voice() {
    let mut d = dev(&SONG);
    d.note_on(10, 0).unwrap();
    d.note_on(11, 0).unwrap();
    d.note_on(12, 0).unwrap();
    d.note_off(11, 5).unwrap();
    let _ = render(&mut d, 25_000, &[500], &[]);
    assert_eq!(d.voice_notes()[..3], [Some(10), None, Some(12)]);
    d.note_on(13, 0).unwrap();
    d.process(&mut [0.0], &mut [0.0]);
    assert_eq!(d.voice_notes()[..3], [Some(10), Some(13), Some(12)]);
}

// ---------------------------------------------------------------- parameters

#[test]
fn unread_parameters_are_inert() {
    let ev = [(0u32, 60u8, true)];
    let base = render(&mut dev(&SONG), 6000, &[128], &ev);
    for i in [1usize, 4, 8] {
        for v in [0.0, 0.3, 1.0] {
            let o = render(&mut dev(&with(i, v)), 6000, &[128], &ev);
            assert_eq!(bits(&base.0), bits(&o.0), "P{i}={v}");
            assert_eq!(bits(&base.1), bits(&o.1), "P{i}={v}");
        }
    }
}

#[test]
fn every_read_parameter_changes_the_sound() {
    let ev = [(0u32, 48u8, true)];
    let base = render(&mut dev(&SONG), 8000, &[128], &ev);
    for i in [0usize, 2, 3, 5, 6, 7, 9, 10, 11, 12, 13, 14] {
        let o = render(&mut dev(&with(i, if SONG[i] > 0.5 { 0.0 } else { 1.0 })), 8000, &[128], &ev);
        assert!(bits(&base.0) != bits(&o.0) || bits(&base.1) != bits(&o.1), "P{i} has no effect");
    }
}

#[test]
fn gain_parameter_is_an_exact_power_of_two_scale_below_the_limiter_knee() {
    let ev = [(0u32, 60u8, true)];
    let p = |g| {
        let mut q = SONG;
        q[9] = g;
        q
    };
    let a = render(&mut dev(&p(8e-6)), 5000, &[128], &ev);
    let b = render(&mut dev(&p(4e-6)), 5000, &[128], &ev);
    assert!(peak(&a.0) > 1e-5 && peak(&a.0) * 3.0 < 0.6 * 3.0, "must stay under the limiter knee, peak {}", peak(&a.0));
    for (x, y) in a.0.iter().zip(&b.0).chain(a.1.iter().zip(&b.1)) {
        assert_eq!(x * 0.5, *y);
    }
}

#[test]
fn note_only_matters_through_the_randomised_noise_and_filters() {
    let mut p = SONG;
    p[14] = 0.0; // no randomisation of the filters
    p[12] = 0.0; // no noise phase modulation of the body
    p[7] = 0.0; // none of the layer either
    let a = render(&mut dev(&p), 4000, &[4000], &[(0, 40, true)]);
    let b = render(&mut dev(&p), 4000, &[4000], &[(0, 90, true)]);
    assert_eq!(bits(&a.0), bits(&b.0), "velocity-less percussion: pitch comes from P0, not the note");
    assert_eq!(bits(&a.1), bits(&b.1));
    p[14] = 0.5;
    let c = render(&mut dev(&p), 4000, &[4000], &[(0, 40, true)]);
    let d = render(&mut dev(&p), 4000, &[4000], &[(0, 90, true)]);
    let e = render(&mut dev(&p), 4000, &[4000], &[(0, 40, true)]);
    assert!(bits(&c.0) != bits(&d.0), "the note seeds the filter randomisation");
    assert_eq!(bits(&c.0), bits(&e.0), "deterministic per note");
}

// ---------------------------------------------------------------- DSP pieces

#[test]
fn hash_is_the_chained_32_bit_mixer() {
    assert_eq!(hash(1), 0x6889_90c0);
    assert_eq!(hash(hash(1)), 0x58f5_4975);
    assert_eq!(hash(0x1234_5678), 0xf5e7_1c96);
    assert_eq!(hash(0xffff_ffff), 0x6768_824a);
    assert_eq!(hash(0), 0);
}

#[test]
fn soft_limiter_closed_form() {
    assert_eq!(soft_limit(0.0), 0.0);
    assert_eq!(soft_limit(0.25), 0.25);
    assert_eq!(soft_limit(-0.59), -0.59);
    let knee = |x: f64| x - 0.5625 * (x - 0.6) * (x - 0.6);
    for x in [0.61f32, 0.8, 1.0, 1.2, 1.39] {
        assert!((soft_limit(x) as f64 - knee(x as f64)).abs() < 2e-7, "{x}");
        assert_eq!(soft_limit(-x), -soft_limit(x));
    }
    assert!((soft_limit(1.0) - 0.91).abs() < 1e-6);
    for x in [1.41f32, 2.0, 10.0, 1000.0] {
        let want = 1.0 + 0.1 * (x as f64 - 1.0);
        assert!((soft_limit(x) as f64 - want).abs() < 2e-5 * want, "{x}");
    }
    // continuous at both corners
    assert!((soft_limit(0.6001) - 0.6001).abs() < 1e-3);
    assert!((soft_limit(1.4) - 1.04).abs() < 1e-5);
    assert!(soft_limit(f32::NAN).is_nan());
}

/// The reference's 512-node sine in f64: nodes `sin(2πi/512)`, linear interpolation of the fraction of `x/2π`.
fn tsin64(x: f64) -> f64 {
    let t = x / std::f64::consts::TAU;
    let f = (t - t.floor()) * 512.0;
    let i = f.floor() as usize;
    let node = |i: usize| (std::f64::consts::TAU * i as f64 / 512.0).sin();
    node(i) + (node(i + 1) - node(i)) * (f - i as f64)
}

/// RBJ coefficients; `table` selects the table sine for the shelf types (as the reference does) instead of exact trig.
fn rbj64(kind: u32, f: f64, q: f64, db: f64, table: bool) -> [f64; 5] {
    let w = f.clamp(20.0, 22000.0) * std::f64::consts::TAU / 44100.0;
    let (s, c) = if table && (kind == 7 || kind == 8) { (tsin64(w), tsin64(w + std::f64::consts::FRAC_PI_2)) } else { w.sin_cos() };
    let a = 10f64.powf(db / 40.0);
    let sa = a.sqrt();
    let alpha = s / (2.0 * q.max(0.05));
    let (b0, b1, b2, a0, a1, a2) = match kind {
        0 => ((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha),
        3 => (1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a),
        7 => (
            a * ((a + 1.0) - (a - 1.0) * c + 2.0 * sa * alpha),
            2.0 * a * ((a - 1.0) - (a + 1.0) * c),
            a * ((a + 1.0) - (a - 1.0) * c - 2.0 * sa * alpha),
            (a + 1.0) + (a - 1.0) * c + 2.0 * sa * alpha,
            -2.0 * ((a - 1.0) + (a + 1.0) * c),
            (a + 1.0) + (a - 1.0) * c - 2.0 * sa * alpha,
        ),
        8 => (
            a * ((a + 1.0) + (a - 1.0) * c + 2.0 * sa * alpha),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
            a * ((a + 1.0) + (a - 1.0) * c - 2.0 * sa * alpha),
            (a + 1.0) - (a - 1.0) * c + 2.0 * sa * alpha,
            2.0 * ((a - 1.0) - (a + 1.0) * c),
            (a + 1.0) - (a - 1.0) * c - 2.0 * sa * alpha,
        ),
        _ => unreachable!(),
    };
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}

#[test]
fn biquad_coefficients_follow_the_rbj_cookbook() {
    // (type, freq, Q, dB) as the song's note setup produces them for P14 = 0 and for a few random spreads
    let cases = [
        (7u32, 147.0f32, 0.71f32, 60.0f32),
        (8, 1660.0, 0.71, 40.0),
        (3, 70.0, 8.0, 9.0),
        (0, 2000.0, 0.1, 0.0),
        (7, 206.0, 0.9, 40.0),
        (8, 2000.0, 0.43, 25.0),
        (0, 22000.0, 0.5, 0.0),
        (7, 90.0, 1.2, 70.0),
        (8, 3000.0, 2.0, 12.0),
        (3, 500.0, 0.03, -6.0),
    ];
    for (k, f, q, g) in cases {
        let got = rbj(k, f, q, g);
        // types 7/8 take sin/cos from the reference's 512-node interpolated sine, 0/3 a libm sine
        let want = rbj64(k, f as f64, q as f64, g as f64, true);
        let scale = want.iter().fold(1.0f64, |m, x| m.max(x.abs()));
        for i in 0..5 {
            assert!((got[i] as f64 - want[i]).abs() <= 3e-6 * scale, "type {k} f {f} q {q} g {g} coef {i}: {} vs {}", got[i], want[i]);
        }
        // and the table sine's own error (≤ 1.9e-5 absolute in sin, amplified by the shelf gain) stays within 1e-3
        let exact = rbj64(k, f as f64, q as f64, g as f64, false);
        for i in 0..5 {
            assert!((got[i] as f64 - exact[i]).abs() <= 1e-3 * scale, "type {k} coef {i} vs exact trig");
        }
    }
}

// ---------------------------------------------------------------- independent f64 model of the body path

fn limit64(x: f64) -> f64 {
    let (s, a) = (x.signum(), x.abs());
    let y = if 2.0 * (a - 1.0) < -0.8 {
        a
    } else if 2.0 * (a - 1.0) > 0.8 {
        (a - 1.0) * 0.1 + 1.0
    } else {
        a - 0.5625 * (a - 0.6) * (a - 0.6)
    };
    s * y
}

/// Left channel of one never-released note with the layer and the noise off (P6 = 0, P12 = 0) and no randomisation
/// (P14 = 0): sine body with a sweeping pitch, exponential amplitude, three cascaded RBJ filters, soft limiter, ×3.
/// The envelope clock and the phase accumulate in f32 like the reference's (the f32 phase quantises each step, a
/// systematic effect of the original's state precision); everything else is f64 (the table sine included: the reference's sine is a 512-node interpolation, ≤ 1.9e-5 absolute
/// off the true sine, and the 60 dB shelf amplifies that; `tsin64` is written independently of the implementation).
fn body_model(p: &[f32; PARAMS], n: usize) -> Vec<f64> {
    let dt = 2.2675737e-5f32;
    // coefficients rounded to f32 as stored by the reference (the 60 dB shelf poles sit at |z| ≈ 0.995: they matter)
    let r32 = |c: [f64; 5]| c.map(|x| x as f32 as f64);
    let c = [
        r32(rbj64(7, 147.0, 0.71, 60.0, true)),
        r32(rbj64(8, 1660.0, 0.71, 40.0, true)),
        r32(rbj64(3, 70.0, 8.0, 9.0, true)),
    ];
    let mut st = [[0.0f64; 4]; 3]; // x1 x2 y1 y2
    let (mut t, mut ph) = (0.0f32, 0.0f32);
    let f0 = p[0] as f64 * 70.0 + 20.0;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let td = t as f64;
        let sweep = (-(p[3] as f64 * 8.0 + 1.0) * 20.0 * td).exp();
        let w = ((sweep * 2200.0 * p[10] as f64 + f0) * 0.000142475852) as f32;
        ph += w;
        let env = (-(p[2] as f64 * 4.0 + 1.0) * 10.0 * (td - 0.04).max(0.0)).exp();
        let mut x = tsin64(ph as f64) * env;
        for (cf, s) in c.iter().zip(st.iter_mut()) {
            let y = cf[0] * x + cf[1] * s[0] + cf[2] * s[1] - cf[3] * s[2] - cf[4] * s[3];
            *s = [x, s[0], y, s[2]];
            x = y;
        }
        let y = p[9] as f64 * 0.4 * (0.08 * x);
        out.push(3.0 * limit64(y));
        t += dt;
    }
    out
}

fn body_params(p9: f32) -> [f32; PARAMS] {
    let mut p = SONG;
    p[6] = 0.0;
    p[12] = 0.0;
    p[14] = 0.0;
    p[9] = p9;
    p
}

#[test]
fn body_path_matches_an_independent_f64_model() {
    for p9 in [0.02f32, 0.3, 1.0] {
        let p = body_params(p9);
        let n = 12_000;
        let (l, r) = render(&mut dev(&p), n, &[128], &[(0, 60, true)]);
        let want = body_model(&p, n);
        let pk = want.iter().fold(0.0f64, |m, x| m.max(x.abs()));
        assert!(pk > 1e-3, "model is not silent (peak {pk})");
        let err = l.iter().zip(&want).map(|(a, b)| (*a as f64 - b).abs()).fold(0.0, f64::max);
        eprintln!("P9 {p9}: max error {err:e} vs peak {pk:e} ({:.2e})", err / pk);
        assert!(err <= 2e-2 * pk, "P9 {p9}: max error {err:e} vs peak {pk:e}");
        assert_eq!(bits(&l), bits(&r), "mono body without the layer: identical channels");
    }
}

#[test]
fn body_model_negative_control_nudged_parameters_leave_the_bound() {
    let n = 12_000;
    let base = body_params(0.3);
    let want = body_model(&base, n);
    let pk = want.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    for (i, d) in [(0usize, 1.0 / 255.0), (2, 0.05), (3, 1.0 / 255.0), (10, 1.0 / 255.0)] {
        let mut p = base;
        p[i] = (p[i] + d).min(1.0);
        let (l, _) = render(&mut dev(&p), n, &[128], &[(0, 60, true)]);
        let err = l.iter().zip(&want).map(|(a, b)| (*a as f64 - b).abs()).fold(0.0, f64::max);
        eprintln!("P{i} nudge: {err:e} vs peak {pk:e} ({:.2e})", err / pk);
        assert!(err > 2e-2 * pk, "P{i} nudge not detected: {err:e}");
    }
}

