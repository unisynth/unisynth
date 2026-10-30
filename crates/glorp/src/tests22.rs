//! Synthetic-only tests for ID 22 (circle oscillator through an iterated 3D trig map, on the shared 16-voice
//! scheduler). No song data, recorded tap or proprietary vector is used: the expectations are independent closed forms
//! and an independent f64 model (ideal sine / exponential, written from the behaviour described in
//! research/glorp-devices.md §ID 22), plus structural properties of the scheduler and of the weight-overflow guard.
use crate::id22::{check, frame, iterations, map, shaped, shaper_norm, weight_ratio, Error, Id22, MAX_NOTE, PARAMS, QUEUE, VOICES};

/// Made-up musical settings (not the song's): contractive map (`b = 0.6`), 4 iterations, 10 ms attack.
const BASE: [f32; PARAMS] = [0.5, 0.2, 0.3, 0.3, 0.2, 0.01, 0.05, 0.7, 0.05, 0.5, 0.3, 0.3, 0.2, 0.5, 0.2];

fn dev(p: &[f32; PARAMS]) -> Id22 {
    Id22::new(p).unwrap()
}

fn with(i: usize, v: f32) -> [f32; PARAMS] {
    let mut p = BASE;
    p[i] = v;
    p
}

/// Renders `n` samples in the given chunk sizes (cycled); events are `(absolute sample, note, on)`.
fn render(d: &mut Id22, n: usize, chunks: &[usize], events: &[(u32, u8, bool)]) -> (Vec<f32>, Vec<f32>) {
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

/// Deterministic xorshift for the property tests.
struct Xs(u64);
impl Xs {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next()
    }
}

// ---------------------------------------------------------------- API contract

#[test]
fn rejects_unsupported_settings_and_stays_unchanged() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 1.001] {
        assert_eq!(Id22::new(&with(3, bad)).err(), Some(Error::Range), "{bad}");
        assert_eq!(check(&with(9, bad)), Err(Error::Range), "{bad}");
    }
    let mut d = dev(&BASE);
    let mut e = dev(&BASE);
    assert_eq!(d.set_param(PARAMS, 0.5), Err(Error::Index));
    for bad in [f32::NAN, 1.5, -1.0, f32::INFINITY] {
        assert_eq!(d.set_param(9, bad), Err(Error::Range));
    }
    assert_eq!(d.note_on(MAX_NOTE + 1, 0), Err(Error::Note));
    assert_eq!(d.note_off(200, 0), Err(Error::Note));
    assert_eq!(d.note_on(60, i32::MAX as u32 + 1), Err(Error::Offset));
    assert_eq!(d.note_off(60, u32::MAX), Err(Error::Offset));
    // a weight-overflowing write is refused too (P2 = 0 with P4 = 1 is 500^16)
    assert_eq!(d.set_param(2, 0.0).and_then(|_| d.set_param(4, 1.0)), Err(Error::Weights));
    d.set_param(4, BASE[4]).unwrap();
    d.set_param(2, BASE[2]).unwrap();
    d.note_on(60, 0).unwrap();
    e.note_on(60, 0).unwrap();
    let (mut a, mut b) = ((vec![0.0; 900], vec![0.0; 900]), (vec![0.0; 900], vec![0.0; 900]));
    d.process(&mut a.0, &mut a.1);
    e.process(&mut b.0, &mut b.1);
    assert_eq!(bits(&a.0), bits(&b.0), "rejected calls left no trace");
    assert_eq!(bits(&a.1), bits(&b.1));
    assert!(peak(&a.0) > 0.0);
}

#[test]
fn event_queue_holds_256_and_refuses_the_next() {
    let mut d = dev(&BASE);
    for i in 0..QUEUE {
        d.note_on((i % 128) as u8, 10).unwrap();
    }
    assert_eq!(d.note_on(1, 10), Err(Error::QueueFull));
    assert_eq!(d.note_off(1, 10), Err(Error::QueueFull));
    let (mut l, mut r) = (vec![0.0; 16], vec![0.0; 16]);
    d.process(&mut l, &mut r);
    assert_eq!(d.active_voices(), VOICES);
    d.note_on(1, 0).unwrap();
}

#[test]
fn silent_without_notes_and_overwrites_the_buffer() {
    let mut d = dev(&BASE);
    let (mut l, mut r) = (vec![1.0f32; 300], vec![-2.0f32; 300]);
    d.process(&mut l, &mut r);
    assert!(l.iter().chain(r.iter()).all(|&x| x == 0.0), "an instrument writes, it does not add to the input");
}

#[test]
#[should_panic(expected = "channel length mismatch")]
fn unequal_channel_lengths_panic() {
    dev(&BASE).process(&mut [0.0; 4], &mut [0.0; 5]);
}

// ---------------------------------------------------------------- the weight-overflow guard

/// `Σ_{k=1..n} a^-k` in f64: the weight sum `W` of the map (independent of the implementation).
fn exact_weights(a: f64, n: usize) -> f64 {
    (1..=n).map(|k| a.powi(-(k as i32))).sum()
}

fn a_of(p2: f32) -> f64 {
    p2 as f64 * 10.0 + 0.002
}

#[test]
fn iteration_count_and_weight_ratio_follow_the_parameters() {
    for (p4, n) in [(0.0f32, 1usize), (0.066, 1), (0.07, 2), (0.5, 8), (0.9, 14), (0.94, 15), (1.0, 16)] {
        assert_eq!(iterations(p4), n, "P4 {p4}");
    }
    for p2 in [0.0f32, 0.001, 0.263, 0.5, 1.0] {
        assert!((weight_ratio(p2) as f64 - a_of(p2)).abs() < 1e-5 * a_of(p2).max(1.0), "P2 {p2}");
    }
}

#[test]
fn the_original_overflows_at_small_weight_ratios_and_high_iteration_counts() {
    // n = 15 at P2 = 0 (a = 0.002): W = 500^15 > f32::MAX, the map returns NaN (inf/inf); n = 14 stays finite
    let p = [1.0f32, 0.5, 0.25];
    let o = map(p, weight_ratio(0.0), 1.0, 15);
    assert!(o.iter().all(|v| v.is_nan()), "{o:?}");
    let o = map(p, weight_ratio(0.0), 1.0, 16);
    assert!(o.iter().any(|v| !v.is_finite()), "{o:?}");
    let o = map(p, weight_ratio(0.0), 1.0, 14);
    assert!(o.iter().all(|v| v.is_finite()), "{o:?}");
    // n <= 0 (unreachable from a parameter in [0, 1]) is NaN too: pinned so nobody "fixes" it silently
    let o = map(p, 1.0, 1.0, 0);
    assert!(o.iter().all(|v| v.is_nan()), "{o:?}");
}

#[test]
fn check_accepts_exactly_what_cannot_overflow() {
    // accepted => every input stays finite and within 2.0001 (|d| <= 2 per term, a weighted mean); rejected => the
    // exact weight sum is within 8x of the f32 limit (the guard is conservative by a bounded factor, not arbitrary)
    let mut rng = Xs(0x22);
    let mut accepted = 0;
    let mut rejected = 0;
    let p2s = [0.0f32, 1e-5, 5e-5, 1e-4, 3e-4, 1e-3, 2e-3, 0.01, 0.05, 0.1, 0.3, 0.5, 1.0];
    for &p2 in &p2s {
        for k in 0..=15 {
            let p4 = if k == 15 { 1.0 } else { (k as f32 + 0.3) / 15.0 };
            let n = iterations(p4);
            let mut p = BASE;
            p[2] = p2;
            p[4] = p4;
            match check(&p) {
                Ok(()) => {
                    accepted += 1;
                    for _ in 0..60 {
                        let x = [rng.range(-5.0, 5.0) as f32, rng.range(-5.0, 5.0) as f32, rng.range(-1e5, 1e5) as f32];
                        let b = rng.range(0.0, 3.0) as f32;
                        let o = map(x, weight_ratio(p2), b, n as i32);
                        assert!(o.iter().all(|v| v.is_finite() && v.abs() <= 2.0001), "P2 {p2} n {n} b {b}: {o:?}");
                    }
                }
                Err(e) => {
                    rejected += 1;
                    assert_eq!(e, Error::Weights);
                    assert!(exact_weights(a_of(p2), n) >= f32::MAX as f64 / 8.0, "P2 {p2} n {n} refused far from the edge");
                }
            }
        }
    }
    assert!(accepted > 60 && rejected >= 6, "generator not vacuous: {accepted} accepted, {rejected} rejected");
    // the edge at the smallest ratio: n = 14 passes, n = 15 does not
    assert_eq!(check(&{ let mut p = BASE; p[2] = 0.0; p[4] = 0.92; p }), Ok(()));
    assert_eq!(check(&{ let mut p = BASE; p[2] = 0.0; p[4] = 0.94; p }), Err(Error::Weights));
    // ratios above 1 never overflow, whatever the iteration count
    assert_eq!(check(&{ let mut p = BASE; p[2] = 1.0; p[4] = 1.0; p }), Ok(()));
}

#[test]
fn weight_guard_is_order_dependent_for_hand_writes_and_atomic() {
    let mut d = dev(&{ let mut p = BASE; p[2] = 0.5; p[4] = 1.0; p });
    // lowering P2 first would leave n = 16 with a = 0.002: refused, nothing changed
    assert_eq!(d.set_param(2, 0.0), Err(Error::Weights));
    d.set_param(4, 0.5).unwrap();
    d.set_param(2, 0.0).unwrap();
    assert_eq!(d.set_param(4, 1.0), Err(Error::Weights));
    d.set_param(4, 0.9).unwrap();
}

// ---------------------------------------------------------------- the map

/// The constant frame the iteration rotates by, derived here from the three observed numbers only: `u ∝ (−1, −1.2,
/// 0.2)`, `n ∝ u × (0, 1, 0)`, `v = n × u`, `p ← (q.z, q.y, q.x)` with `q = x·n + y·v + z·u`.
fn model_rows() -> [[f64; 3]; 3] {
    let s = 2.48f64.sqrt();
    let u = [-1.0 / s, -1.2 / s, 0.2 / s];
    let w = [-u[2], 0.0, u[0]];
    let l = (w[0] * w[0] + w[2] * w[2]).sqrt();
    let n = [w[0] / l, 0.0, w[2] / l];
    let v = [n[1] * u[2] - n[2] * u[1], n[2] * u[0] - n[0] * u[2], n[0] * u[1] - n[1] * u[0]];
    [[n[2], v[2], u[2]], [n[1], v[1], u[1]], [n[0], v[0], u[0]]]
}

/// Independent f64 model of the map with ideal sine.
fn model_map(p: [f64; 3], a: f64, b: f64, n: usize) -> [f64; 3] {
    let m = model_rows();
    let (mut x, mut y, mut z) = (p[0], p[1], p[2]);
    let (mut num, mut w) = ([0.0f64; 3], 0.0f64);
    for k in 1..=n {
        let q: Vec<f64> = m.iter().map(|r| r[0] * x + r[1] * y + r[2] * z).collect();
        let (nx, ny, nz) = (q[0] + q[2].sin(), q[1] + q[0].sin(), q[2] + q[1].sin());
        let wt = a.powi(k as i32); // weight aᵏ: later iterations dominate for a > 1
        let (sx, sy, sz) = (nx.sin(), ny.sin(), nz.sin());
        let (cx, cy, cz) = (nx.cos(), ny.cos(), nz.cos());
        num[0] += wt * (sx * cy - sz * cz);
        num[1] += wt * (sy * cz - sx * cx);
        num[2] += wt * (cx * sz - sy * cy);
        w += wt;
        (x, y, z) = (nx * b, ny * b, nz * b);
    }
    [num[0] / w, num[1] / w, num[2] / w]
}

#[test]
fn the_frame_is_the_observed_proper_rotation() {
    let f = frame();
    let want = [[-0.980581, 0.149441, 0.127000], [0.0, 0.647576, -0.762001], [-0.196116, -0.747203, -0.635001]];
    for i in 0..3 {
        for j in 0..3 {
            assert!((f[i][j] - want[i][j]).abs() < 2e-6, "[{i}][{j}] {} vs {}", f[i][j], want[i][j]);
        }
    }
    let m = model_rows();
    for i in 0..3 {
        for j in 0..3 {
            assert!((f[i][j] as f64 - m[i][j]).abs() < 3e-7, "derived [{i}][{j}]");
            let dot: f64 = (0..3).map(|k| f[i][k] as f64 * f[j][k] as f64).sum();
            assert!((dot - (i == j) as u8 as f64).abs() < 1e-6, "rows {i},{j} not orthonormal: {dot}");
        }
    }
    let det = f[0][0] as f64 * (f[1][1] as f64 * f[2][2] as f64 - f[1][2] as f64 * f[2][1] as f64)
        - f[0][1] as f64 * (f[1][0] as f64 * f[2][2] as f64 - f[1][2] as f64 * f[2][0] as f64)
        + f[0][2] as f64 * (f[1][0] as f64 * f[2][1] as f64 - f[1][1] as f64 * f[2][0] as f64);
    assert!((det - 1.0).abs() < 1e-6, "det {det}");
}

#[test]
fn the_map_matches_the_independent_model() {
    // contractive settings (b <= 0.8, few iterations): the table sine's 2e-5 error is not amplified
    let mut rng = Xs(7);
    let mut worst = 0.0f64;
    for t in 0..300 {
        let p = [rng.range(-5.0, 5.0), rng.range(-5.0, 5.0), rng.range(-5.0, 5.0)];
        let a = rng.range(0.5, 10.0);
        let b = rng.range(0.0, 0.8);
        let n = 1 + (t % 6);
        let got = map([p[0] as f32, p[1] as f32, p[2] as f32], a as f32, b as f32, n as i32);
        let want = model_map(p, a as f64, b, n);
        for c in 0..3 {
            worst = worst.max((got[c] as f64 - want[c]).abs());
        }
    }
    assert!(worst < 2e-4, "worst abs error {worst:e}");
}

#[test]
fn one_iteration_is_the_plain_trig_vector_and_a_equal_one_is_the_plain_mean() {
    // n = 1: the weight cancels, out = d(p'), whatever a is
    let p = [0.7f32, -1.1, 0.3];
    let o1 = map(p, 3.0, 0.5, 1);
    let o2 = map(p, 0.25, 2.0, 1);
    for c in 0..3 {
        assert!((o1[c] - o2[c]).abs() < 1e-6, "{o1:?} {o2:?}");
    }
    // a = 1: the plain mean over the iterations (the model with a = 1)
    let got = map([1.0, 2.0, -0.5], 1.0, 0.7, 5);
    let want = model_map([1.0, 2.0, -0.5], 1.0, 0.7, 5);
    for c in 0..3 {
        assert!((got[c] as f64 - want[c]).abs() < 2e-4);
    }
    // exponential weighting aᵏ: a >> 1 is the last iteration alone, a << 1 the first one alone
    let q = [0.9f32, -0.4, 1.3];
    let first = map(q, 1.0, 0.6, 1);
    let light = map(q, 1e-4, 0.6, 6);
    let heavy = map(q, 1000.0, 0.6, 6);
    let last = model_map([0.9, -0.4, 1.3], 1e9, 0.6, 6);
    for c in 0..3 {
        assert!((light[c] - first[c]).abs() < 2e-3, "a << 1 is the first iteration: {light:?} vs {first:?}");
        assert!((heavy[c] as f64 - last[c]).abs() < 2e-3, "a = 1000 is the last iteration: {heavy:?} vs {last:?}");
    }
}

#[test]
fn the_shaper_is_the_odd_rational_saturator() {
    let t = |x: f64| x * (x * x + 27.0) / (9.0 * x * x + 27.0);
    for d in [0.001f32, 0.05, 0.2, 0.7, 1.001] {
        let td = shaper_norm(d);
        assert!((td as f64 - t(d as f64)).abs() < 1e-6 * t(d as f64).abs().max(1e-3), "T({d})");
        for x in [-30.0f32, -3.0, -0.4, -1e-4, 0.0, 1e-4, 0.4, 3.0, 30.0] {
            let got = shaped(x, 0.6, td) as f64;
            let want = 0.6 * t(x as f64) / t(d as f64);
            assert!((got - want).abs() <= 3e-6 * want.abs() + 1e-9, "x {x} d {d}: {got} vs {want}");
            assert_eq!(shaped(-x, 0.6, td).to_bits(), (-shaped(x, 0.6, td)).to_bits(), "odd: x {x}");
        }
    }
    // it saturates: slope 1 at 0, 1/9 far out
    let td = shaper_norm(1.0);
    assert!(shaped(1000.0, 1.0, td) / shaped(100.0, 1.0, td) < 10.5);
}

// ---------------------------------------------------------------- scheduling

#[test]
fn first_sample_is_at_the_event_offset() {
    for off in [0u32, 1, 37, 127] {
        let mut d = dev(&BASE);
        let (l, r) = render(&mut d, 400, &[400], &[(off, 60, true)]);
        let first = l.iter().position(|&x| x != 0.0).unwrap();
        assert_eq!(first as u32, off);
        assert!(l[..first].iter().chain(r[..first].iter()).all(|&x| x == 0.0));
    }
}

#[test]
fn rendering_is_independent_of_the_block_split() {
    let ev: Vec<(u32, u8, bool)> =
        vec![(100, 40, true), (1500, 45, true), (900, 40, false), (2999, 45, false), (3001, 70, true), (3001, 71, true), (3000, 70, false)];
    let mut whole = dev(&BASE);
    let a = render(&mut whole, 6000, &[6000], &ev);
    for chunks in [&[1usize][..], &[128], &[127, 129, 3, 1000], &[5000, 1]] {
        let mut d = dev(&BASE);
        let b = render(&mut d, 6000, chunks, &ev);
        assert_eq!(bits(&a.0), bits(&b.0), "{chunks:?}");
        assert_eq!(bits(&a.1), bits(&b.1), "{chunks:?}");
    }
    assert!(peak(&a.0) > 0.0 && peak(&a.1) > 0.0);
}

#[test]
fn seventeenth_simultaneous_note_is_dropped_voices_fill_in_order() {
    let notes: Vec<u8> = (0..17).map(|i| 20 + i).collect();
    let mut d = dev(&BASE);
    for &n in &notes {
        d.note_on(n, 0).unwrap();
    }
    d.process(&mut [0.0], &mut [0.0]);
    let got = d.voice_notes();
    for i in 0..VOICES {
        assert_eq!(got[i], Some(20 + i as u8), "voice {i}");
    }
    assert_eq!(d.active_voices(), 16);
    let ev16: Vec<_> = notes[..16].iter().map(|&n| (0, n, true)).collect();
    let ev17: Vec<_> = notes.iter().map(|&n| (0, n, true)).collect();
    let a = render(&mut dev(&BASE), 3000, &[3000], &ev16);
    let b = render(&mut dev(&BASE), 3000, &[3000], &ev17);
    assert_eq!(bits(&a.0), bits(&b.0));
    assert_eq!(bits(&a.1), bits(&b.1));
}

#[test]
fn a_released_voice_is_freed_when_its_release_ends_and_is_silent_after() {
    // release P8 = 0.05 s: 2205 ticks of the f32 clock; the voice frees itself on the sample that finishes it
    let mut d = dev(&BASE);
    d.note_on(60, 0).unwrap();
    d.note_off(60, 5000).unwrap();
    let (mut freed_at, mut last_nonzero) = (None, 0usize);
    for i in 0..5000 + 2205 + 400 {
        let (mut l, mut r) = ([9.0f32], [9.0f32]);
        d.process(&mut l, &mut r);
        if l[0] != 0.0 || r[0] != 0.0 {
            last_nonzero = i;
        }
        if freed_at.is_none() && d.active_voices() == 0 {
            freed_at = Some(i);
        }
    }
    let f = freed_at.expect("voice never freed");
    assert!((5000 + 2190..5000 + 2230).contains(&f), "freed at {f}");
    assert!(last_nonzero <= f, "nothing is rendered after the voice is freed ({last_nonzero} > {f})");
    // never released: the voice holds its slot (sustain 0.7)
    let mut e = dev(&BASE);
    e.note_on(60, 0).unwrap();
    let (mut l, mut r) = (vec![0.0; 40_000], vec![0.0; 40_000]);
    e.process(&mut l, &mut r);
    assert_eq!(e.active_voices(), 1);
    assert!(peak(&l[39_000..]) > 0.0);
}

#[test]
fn note_off_releases_every_voice_of_that_note_only() {
    let mut d = dev(&BASE);
    let ev = [(0u32, 50u8, true), (0, 50, true), (0, 52, true), (3000, 50, false)];
    render(&mut d, 3500, &[3500], &ev);
    assert_eq!(d.voice_notes()[..3], [Some(50), Some(50), Some(52)]);
    render(&mut d, 4000, &[4000], &[]);
    assert_eq!(d.voice_notes()[..3], [None, None, Some(52)]);
    assert_eq!(d.active_voices(), 1);
}

#[test]
fn a_voice_slot_is_reused_and_the_envelope_level_survives_between_notes() {
    // a note released early ends; the next note on the same slot restarts the clock but not the stored level
    let ev = [(0u32, 60u8, true), (2000, 60, false), (9000, 61, true)];
    let mut a = dev(&BASE);
    let one = render(&mut a, 12_000, &[12_000], &ev);
    assert!(peak(&one.0[9000..]) > 0.0);
    let mut b = dev(&BASE);
    let two = render(&mut b, 12_000, &[128], &ev);
    assert_eq!(bits(&one.0), bits(&two.0));
}

// ---------------------------------------------------------------- parameters

/// Note-on at 0, note-off at 3000, 12000 samples: every parameter's effect shows.
fn scenario(p: &[f32; PARAMS]) -> (Vec<f32>, Vec<f32>) {
    render(&mut dev(p), 12_000, &[12_000], &[(0, 57, true), (3000, 57, false)])
}

#[test]
fn every_parameter_is_read_and_changes_the_output() {
    let base = scenario(&BASE);
    for i in 0..PARAMS {
        if i == 10 || i == 11 {
            continue; // handled below: P10 lives behind the P11 switch
        }
        let nudged = if BASE[i] + 0.1 <= 1.0 { BASE[i] + 0.1 } else { BASE[i] - 0.1 };
        let o = scenario(&with(i, nudged));
        assert_ne!(bits(&o.0), bits(&base.0), "P{i} is inert");
    }
    // P11 below 0.5: P10 is inert, P3 acts as a ramp rate; at / above 0.5: P10 is the LFO depth, P3 its rate
    let lo = with(11, 0.2);
    assert_eq!(bits(&scenario(&{ let mut p = lo; p[10] = 0.9; p }).0), bits(&scenario(&lo).0), "P10 inert below 0.5");
    let hi = with(11, 0.6);
    assert_ne!(bits(&scenario(&{ let mut p = hi; p[10] = 0.9; p }).0), bits(&scenario(&hi).0), "P10 audible at P11 >= 0.5");
    assert_ne!(bits(&scenario(&lo).0), bits(&scenario(&hi).0));
}

#[test]
fn the_z_mode_switch_is_at_exactly_one_half_inclusive() {
    let a = scenario(&with(11, 0.499_999_94));
    let b = scenario(&with(11, 0.3));
    assert_eq!(bits(&a.0), bits(&b.0), "just below 0.5 is the ramp mode");
    let c = scenario(&with(11, 0.5));
    let d = scenario(&with(11, 0.9));
    assert_eq!(bits(&c.0), bits(&d.0), "0.5 is the LFO mode");
    assert_ne!(bits(&a.0), bits(&c.0));
}

#[test]
fn envelope_times_and_sustain_are_latched_at_note_on_but_the_rest_is_live() {
    let run = |tweak: &dyn Fn(&mut Id22)| {
        let mut d = dev(&BASE);
        d.note_on(57, 0).unwrap();
        let (mut l, mut r) = (vec![0.0f32; 128], vec![0.0f32; 128]);
        d.process(&mut l, &mut r); // the note is playing now
        tweak(&mut d);
        d.note_off(57, 2000).unwrap();
        let (mut l2, mut r2) = (vec![0.0f32; 6000], vec![0.0f32; 6000]);
        d.process(&mut l2, &mut r2);
        l2
    };
    let plain = run(&|_| {});
    for i in [5usize, 6, 7, 8] {
        let o = run(&|d| d.set_param(i, 0.9).unwrap());
        assert_eq!(bits(&o), bits(&plain), "P{i} (envelope) is latched at note-on");
    }
    for i in [0usize, 1, 2, 3, 4, 9, 12, 13, 14] {
        let nudged = if BASE[i] + 0.1 <= 1.0 { BASE[i] + 0.1 } else { BASE[i] - 0.1 };
        let o = run(&|d| d.set_param(i, nudged).unwrap());
        assert_ne!(bits(&o), bits(&plain), "P{i} acts on a sounding voice");
    }
    // the next note-on picks the new envelope up
    let mut d = dev(&BASE);
    d.set_param(5, 0.5).unwrap();
    let slow = render(&mut d, 600, &[600], &[(0, 57, true)]);
    let fast = render(&mut dev(&BASE), 600, &[600], &[(0, 57, true)]);
    assert!(peak(&slow.0) < peak(&fast.0), "a 0.5 s attack is far quieter than a 10 ms one after 600 samples");
}

#[test]
fn level_gain_and_width_laws() {
    // P9 is the last linear factor: a power of two scales the output exactly
    let a = scenario(&with(9, 0.5));
    let b = scenario(&with(9, 0.25));
    for (x, y) in a.0.iter().zip(&b.0) {
        assert_eq!((x * 0.5).to_bits(), y.to_bits());
    }
    // width 1: L = o0, R = o1 (independent); width 0: both channels are o0 - o1 (identical)
    let w0 = scenario(&with(13, 0.0));
    assert_eq!(bits(&w0.0), bits(&w0.1), "width 0 is mono");
    let w1 = scenario(&with(13, 1.0));
    assert_ne!(bits(&w1.0), bits(&w1.1), "width 1 keeps the channels apart");
    // gain 0 is silent; the shaper then sees 0
    let g0 = scenario(&with(14, 0.0));
    assert!(g0.0.iter().chain(&g0.1).all(|&x| x == 0.0));
}

// ---------------------------------------------------------------- independent model of the whole voice

/// Ideal-math model of one note (f64 clock, ideal `sin`, `exp`, `pow`), all parameters from `p`, envelope times
/// latched as the device does; renders `n` samples, note-off at `off`.
fn model_voice(p: &[f32; PARAMS], note: u8, n: usize, off: usize) -> (Vec<f64>, Vec<f64>) {
    let p: Vec<f64> = p.iter().map(|&v| v as f64).collect();
    let dt = 1.0 / 44100.0;
    let pw = 2f64.powf(note as f64 / 12.0 - 5.75);
    // the phase is an f32 accumulator that is never wrapped (documented behaviour), everything else is ideal
    let w = (2.0 * std::f64::consts::PI * 440.0 / 44100.0 * pw) as f32;
    let mut phi32 = 0.0f32;
    let (ea, ed, es, er) = (p[5], p[6], p[7], p[8]);
    let (mut stage, mut t, mut level) = (1u8, 0.0f64, 0.0f64);
    let nit = (p[4] * 15.0) as usize + 1;
    let a = p[2] * 10.0 + 0.002;
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for i in 0..n {
        if i == off {
            stage = 4;
            t = 0.0;
        }
        // envelope tick
        t += dt;
        let env = match stage {
            1 => {
                let inv = 1.0 / ea;
                let v = inv * t * (2.0 * inv * (t - ea)).exp();
                if t >= ea {
                    t = 0.0;
                    stage = 2;
                }
                level = v;
                v.clamp(0.0, 1.0)
            }
            2 => {
                let x = t / ed;
                let e = (-2.0 * x).exp();
                let target = (es - 1.0) * (1.0 - e) + 1.0;
                let v = (es - target) * x + target;
                if t >= ed {
                    t = 0.0;
                    stage = 3;
                }
                level = v;
                v.clamp(0.0, 1.0)
            }
            3 => {
                level = es;
                es.clamp(0.0, 1.0)
            }
            4 => {
                let x = t / er;
                if t < er {
                    (((1.0 - x) * (-2.0 * x).exp()) * level).clamp(0.0, 1.0)
                } else {
                    stage = 0;
                    0.0
                }
            }
            _ => 0.0,
        };
        let phi = phi32 as f64;
        phi32 += w;
        let z0 = if p[11] < 0.5 { phi * 0.2 * p[3] } else { (phi * 100.0 * p[3] / 440.0 / pw).sin() * 40.0 * p[10] };
        let r0 = 5.0 * p[0];
        let o = model_map([phi.sin() * r0, phi.cos() * r0, z0 * r0], a, p[1] * 3.0, nit);
        let s = (1.0 - p[13]) * (o[0] - o[1]);
        let (cl, cr) = (p[13] * o[0] + s, p[13] * o[1] + s);
        let g = p[14] * 10.0 * env;
        let d = p[12] + 0.001;
        let t_ = |x: f64| x * (x * x + 27.0) / (9.0 * x * x + 27.0);
        l[i] = p[9] * t_(cl * g * d) / t_(d);
        r[i] = p[9] * t_(cr * g * d) / t_(d);
        if stage == 0 && i > off {
            break;
        }
    }
    (l, r)
}

#[test]
fn a_note_matches_the_independent_model() {
    // contractive map settings; ramp mode and LFO mode; held and released
    let sets: [[f32; PARAMS]; 4] = [
        BASE,
        with(11, 0.7),
        { let mut p = BASE; p[1] = 0.1; p[2] = 0.6; p[4] = 0.4; p[12] = 0.6; p[13] = 0.9; p }, // a = 8, n = 7
        { let mut p = BASE; p[0] = 0.9; p[3] = 0.0; p[7] = 0.3; p[14] = 0.5; p },
    ];
    let mut worst = 0.0f64;
    for (k, p) in sets.iter().enumerate() {
        let (note, n, off) = (48 + 7 * k as u8, 9000, 4000);
        let (l, r) = render(&mut dev(p), n, &[n], &[(0, note, true), (off as u32, note, false)]);
        let (ml, mr) = model_voice(p, note, n, off);
        let pk = ml.iter().chain(&mr).fold(0.0f64, |m, v| m.max(v.abs()));
        assert!(pk > 0.01, "set {k}: model too quiet ({pk})");
        let mut w = (0.0f64, 0usize);
        for i in 0..n {
            let e = ((l[i] as f64 - ml[i]).abs() / pk).max((r[i] as f64 - mr[i]).abs() / pk);
            if e > w.0 {
                w = (e, i);
            }
        }
        worst = worst.max(w.0);
    }
    assert!(worst < 2e-3, "worst error relative to the peak: {worst:e}");
}

#[test]
fn negative_control_nudged_parameters_leave_the_model_tolerance() {
    let (note, n, off) = (55u8, 9000usize, 4000usize);
    for i in [0usize, 1, 2, 3, 4, 7, 9, 12, 13, 14] {
        let mut p = BASE;
        p[i] = if p[i] + 0.1 <= 1.0 { p[i] + 0.1 } else { p[i] - 0.1 };
        let (l, _) = render(&mut dev(&p), n, &[n], &[(0, note, true), (off as u32, note, false)]);
        let (ml, mr) = model_voice(&BASE, note, n, off);
        let pk = ml.iter().chain(&mr).fold(0.0f64, |m, v| m.max(v.abs()));
        let worst = (0..n).map(|k| (l[k] as f64 - ml[k]).abs() / pk).fold(0.0, f64::max);
        assert!(worst > 2e-3, "a 0.1 nudge of P{i} stays inside the model tolerance ({worst:e})");
    }
}

// ---------------------------------------------------------------- finiteness over the accepted domain

#[test]
fn accepted_parameters_never_produce_a_non_finite_sample() {
    let mut rng = Xs(2222);
    let mut ran = 0;
    for t in 0..120 {
        let mut p = [0.0f32; PARAMS];
        for v in p.iter_mut() {
            *v = match (rng.next() * 6.0) as u32 {
                0 => 0.0,
                1 => 1.0,
                _ => rng.next() as f32,
            };
        }
        if t % 3 == 0 {
            p[2] = [0.0, 1e-5, 1e-3][t / 3 % 3]; // smallest ratios: the overflow edge
            p[4] = rng.next() as f32;
        }
        if check(&p).is_err() {
            continue;
        }
        ran += 1;
        let mut d = dev(&p);
        let note = (rng.next() * 128.0) as u8;
        let (l, r) = render(&mut d, 30_000, &[512], &[(0, note, true), (20_000, note, false)]);
        assert!(l.iter().chain(&r).all(|v| v.is_finite()), "case {t}: {p:?} note {note}");
    }
    assert!(ran > 60, "generator not vacuous: {ran}");
}

#[test]
fn the_highest_note_with_a_long_ramp_stays_finite() {
    // phase and z ramp grow without bound (no wrap in the original); 6 s at note 127, both z modes
    for p11 in [0.0f32, 1.0] {
        let mut p = with(11, p11);
        p[3] = 1.0;
        p[10] = 1.0;
        p[0] = 1.0;
        p[4] = 0.5;
        let (l, r) = render(&mut dev(&p), 6 * 44_100, &[128], &[(0, 127, true)]);
        assert!(l.iter().chain(&r).all(|v| v.is_finite()), "P11 {p11}");
        assert!(peak(&l) > 0.0);
    }
    let (l, _) = render(&mut dev(&BASE), 4000, &[4000], &[(0, 0, true)]);
    assert!(peak(&l) > 0.0 && l.iter().all(|v| v.is_finite()));
}

#[test]
fn accepted_extreme_corners_render_finite() {
    // every accepted extreme corner renders finite audio
    for mask in 0u32..(1 << 6) {
        let mut p = BASE;
        for (bit, i) in [0usize, 1, 2, 9, 12, 14].into_iter().enumerate() {
            p[i] = if mask >> bit & 1 == 1 { 1.0 } else { 0.0 };
        }
        p[4] = 0.9; // n = 14: accepted even at P2 = 0
        let (l, r) = render(&mut dev(&p), 3000, &[3000], &[(0, 60, true)]);
        assert!(l.iter().chain(&r).all(|v| v.is_finite()), "mask {mask:06b}");
    }
}
