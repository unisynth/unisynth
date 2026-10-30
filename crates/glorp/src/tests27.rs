//! Synthetic-only tests for ID 27 (comb-resonator percussion synth on the shared 16-voice scheduler). No song data,
//! recorded tap or proprietary vector is used here: the expectations are structural properties (lifetime, scheduling,
//! parameter latching, determinism) and an independent f64 model of the whole voice written from the behavioural
//! description in `research/glorp-devices.md` §ID 27 (ideal fractional delays, exact `sin`/`exp`, textbook filters).
use crate::id25::{hash, DT};
use crate::id27::{Error, Id27, MAX_NOTE, PARAMS, QUEUE, VOICES};

/// Device-10-like settings (the song's second ID 27 instance, rounded): every parameter in the interior.
const SONG: [f32; PARAMS] = [0.895, 0.225, 0.415, 0.88, 0.0, 0.86, 0.34, 0.4, 1.0, 1.0, 0.075, 0.271, 1.0];
/// A mid-range set where every parameter matters.
const MID: [f32; PARAMS] = [0.5, 0.4, 0.3, 0.5, 0.0, 0.4, 0.5, 0.3, 0.7, 0.5, 0.3, 0.4, 0.5];

fn dev(p: &[f32; PARAMS]) -> Box<Id27> {
    Box::new(Id27::new(p).unwrap())
}

fn with(base: &[f32; PARAMS], i: usize, v: f32) -> [f32; PARAMS] {
    let mut p = *base;
    p[i] = v;
    p
}

/// Renders `n` samples in the given chunk sizes (cycled); `events` = `(absolute sample, note, on)` queued with the
/// offset relative to the chunk they fall in, in list order.
fn render(d: &mut Id27, n: usize, chunks: &[usize], events: &[(u32, u8, bool)]) -> (Vec<f32>, Vec<f32>) {
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

fn one_note(p: &[f32; PARAMS], note: u8, n: usize) -> (Vec<f32>, Vec<f32>) {
    let mut d = dev(p);
    render(&mut d, n, &[n], &[(0, note, true)])
}

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

fn peak(v: &[f32]) -> f32 {
    v.iter().fold(0.0, |m, x| m.max(x.abs()))
}

fn max_diff(a: &[f32], b: &[f32]) -> f64 {
    a.iter().zip(b).fold(0.0, |m, (x, y)| m.max((*x as f64 - *y as f64).abs()))
}

/// Samples a voice lives: the f32 clock `t += 1/44100` is checked after each increment, the voice stops with the sample
/// that makes `t > 2.5` (it still renders that sample).
fn lifetime_samples() -> usize {
    let (mut t, mut n) = (0.0f32, 0usize);
    loop {
        t += DT;
        n += 1;
        if !(2.5 >= t) {
            return n;
        }
    }
}

// ---------------------------------------------------------------- API contract

#[test]
fn rejects_unsupported_settings_and_stays_unchanged() {
    for bad in [f32::NAN, f32::INFINITY, -0.001, 1.001] {
        assert_eq!(Id27::new(&with(&SONG, 3, bad)).err(), Some(Error::Range), "{bad}");
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
    d.note_on(60, 0).unwrap();
    e.note_on(60, 0).unwrap();
    let (mut a, mut b) = ((vec![0.0; 600], vec![0.0; 600]), (vec![0.0; 600], vec![0.0; 600]));
    d.process(&mut a.0, &mut a.1);
    e.process(&mut b.0, &mut b.1);
    assert_eq!(bits(&a.0), bits(&b.0), "a rejected call left a trace");
    assert_eq!(bits(&a.1), bits(&b.1));
    assert!(peak(&a.0) > 0.0);
}

#[test]
fn event_queue_holds_256_and_refuses_the_next() {
    let mut d = dev(&SONG);
    for _ in 0..QUEUE {
        d.note_on(10, 5).unwrap();
    }
    assert_eq!(d.note_on(10, 5), Err(Error::QueueFull));
    assert_eq!(d.note_off(10, 5), Err(Error::QueueFull));
    // note-offs take queue slots too (they are events like any other)
    let mut d = dev(&SONG);
    for _ in 0..QUEUE {
        d.note_off(10, 5).unwrap();
    }
    assert_eq!(d.note_on(10, 5), Err(Error::QueueFull));
}

#[test]
fn silence_overwrites_and_length_rules() {
    let mut d = dev(&SONG);
    let (mut l, mut r) = (vec![7.0f32; 300], vec![-7.0f32; 300]);
    d.process(&mut l, &mut r);
    assert!(l.iter().chain(&r).all(|&x| x == 0.0), "the instrument overwrites its output");
    d.process(&mut [], &mut []);
    let err = std::panic::catch_unwind(|| {
        let mut d = Id27::new(&SONG).unwrap();
        d.process(&mut [0.0; 4], &mut [0.0; 3]);
    })
    .err()
    .unwrap();
    assert!(err.downcast_ref::<&str>().is_some_and(|m| m.contains("length")) || err.downcast_ref::<String>().is_some_and(|m| m.contains("length")));
}

// ---------------------------------------------------------------- scheduling and lifetime

#[test]
fn onset_is_offset_accurate() {
    for off in [0u32, 1, 37, 127] {
        let mut d = dev(&SONG);
        d.note_on(12, off).unwrap();
        let (mut l, mut r) = (vec![1.0f32; 300], vec![1.0f32; 300]);
        d.process(&mut l, &mut r);
        let first = l.iter().position(|&x| x != 0.0).unwrap();
        assert_eq!(first, off as usize);
        assert!(r[..first].iter().all(|&x| x == 0.0));
        assert!(r[first] != 0.0, "the first sample already has both channels");
    }
    // an offset beyond the block stays queued for the next call
    let mut d = dev(&SONG);
    d.note_on(12, 200).unwrap();
    let (mut l, mut r) = (vec![0.0f32; 128], vec![0.0f32; 128]);
    d.process(&mut l, &mut r);
    assert!(l.iter().all(|&x| x == 0.0));
    assert_eq!(d.active_voices(), 0);
    d.process(&mut l, &mut r);
    assert!(l[..72].iter().all(|&x| x == 0.0) && l[72] != 0.0);
}

#[test]
fn block_split_is_bit_identical() {
    let ev = [(0, 12u8, true), (300, 63, true), (301, 12, false), (777, 68, true), (4000, 2, true)];
    let (a, b) = {
        let mut d = dev(&MID);
        let a = render(&mut d, 9000, &[9000], &ev);
        let mut d = dev(&MID);
        (a, render(&mut d, 9000, &[1, 127, 128, 3, 500, 64], &ev))
    };
    assert_eq!(bits(&a.0), bits(&b.0));
    assert_eq!(bits(&a.1), bits(&b.1));
    assert!(peak(&a.0) > 0.0);
}

#[test]
fn voice_lives_until_the_f32_clock_passes_two_and_a_half_seconds() {
    let n = lifetime_samples();
    // f32 accumulation of 1/44100 drifts: the clock reaches 2.5 about 100 samples (2 ms) after the ideal 110 250
    assert!((110_250..110_500).contains(&n), "{n} samples is not 2.5 s");
    let mut d = dev(&SONG);
    d.note_on(12, 0).unwrap();
    let (mut l, mut r) = (vec![0.0f32; n - 1], vec![0.0f32; n - 1]);
    d.process(&mut l, &mut r);
    assert_eq!(d.active_voices(), 1, "still sounding one sample before the end");
    let (mut l1, mut r1) = ([9.0f32; 1], [9.0f32; 1]);
    d.process(&mut l1, &mut r1);
    assert_eq!(d.active_voices(), 0, "freed by the sample that passes 2.5 s");
    assert!(l1[0] != 0.0 || r1[0] != 0.0 || SONG[8] == 0.0, "that last sample is still rendered");
    d.process(&mut l, &mut r);
    assert!(l.iter().chain(&r).all(|&x| x == 0.0));
}

#[test]
fn note_off_is_inaudible_and_does_not_free_the_voice() {
    let n = 6000;
    let ev = [(0, 12u8, true), (300, 63, true)];
    let (a, b) = {
        let mut d = dev(&MID);
        let a = render(&mut d, n, &[n], &ev);
        let mut d = dev(&MID);
        let offs = [(0, 12u8, true), (1, 12, false), (300, 63, true), (305, 63, false), (500, 12, false), (900, 99, false)];
        let b = render(&mut d, n, &[n], &offs);
        assert_eq!(d.active_voices(), 2, "a note-off does not end a voice (it lives 2.5 s regardless)");
        (a, b)
    };
    assert_eq!(bits(&a.0), bits(&b.0));
    assert_eq!(bits(&a.1), bits(&b.1));
}

#[test]
fn polyphony_is_sixteen_and_the_seventeenth_is_lost() {
    let mut d = dev(&SONG);
    for i in 0..17u8 {
        d.note_on(30 + i, u32::from(i)).unwrap();
    }
    let (mut l, mut r) = (vec![0.0f32; 400], vec![0.0f32; 400]);
    d.process(&mut l, &mut r);
    assert_eq!(d.active_voices(), VOICES);
    let notes = d.voice_notes();
    for (i, n) in notes.iter().enumerate() {
        assert_eq!(*n, Some(30 + i as u8), "voice {i} plays the i-th note");
    }
    // while all are busy a new note is lost; after the voices expire the lowest free ones are used again
    d.note_on(90, 0).unwrap();
    d.process(&mut l, &mut r);
    assert!(!d.voice_notes().contains(&Some(90)));
    let n = lifetime_samples();
    let mut big = (vec![0.0f32; n], vec![0.0f32; n]);
    d.process(&mut big.0, &mut big.1);
    assert_eq!(d.active_voices(), 0);
    d.note_on(91, 0).unwrap();
    d.note_on(92, 0).unwrap();
    d.process(&mut l, &mut r);
    assert_eq!(&d.voice_notes()[..3], &[Some(91), Some(92), None]);
}

// ---------------------------------------------------------------- parameters and determinism

#[test]
fn only_p6_is_never_read() {
    let base = one_note(&MID, 40, 6000);
    assert!(peak(&base.0) > 0.0 && peak(&base.1) > 0.0);
    for i in 0..PARAMS {
        let alt = one_note(&with(&MID, i, if MID[i] > 0.5 { MID[i] - 0.25 } else { MID[i] + 0.25 }), 40, 6000);
        if i == 6 {
            assert_eq!(bits(&alt.0), bits(&base.0), "P6 must be inert");
            assert_eq!(bits(&alt.1), bits(&base.1));
        } else {
            assert!(max_diff(&alt.0, &base.0) > 1e-4 * peak(&base.0) as f64, "P{i} changed nothing");
        }
    }
}

#[test]
fn note_on_parameters_are_latched_and_the_others_act_at_once() {
    // P2 P4 P5 P10 P11 are read at note-on only; a later change must not touch the running voice
    for i in [2usize, 4, 5, 10, 11] {
        let plain = {
            let mut d = dev(&MID);
            render(&mut d, 4000, &[4000], &[(0, 40, true)])
        };
        let mut d = dev(&MID);
        d.note_on(40, 0).unwrap();
        let (mut l, mut r) = (vec![0.0f32; 4000], vec![0.0f32; 4000]);
        d.process(&mut l[..1000], &mut r[..1000]);
        d.set_param(i, if MID[i] > 0.5 { 0.1 } else { 0.9 }).unwrap();
        d.process(&mut l[1000..], &mut r[1000..]);
        assert_eq!(bits(&l), bits(&plain.0), "P{i} must not affect a running voice");
        // ... but the next note picks it up
        let fresh = one_note(&MID, 40, 4000);
        let (mut l2, mut r2) = (vec![0.0f32; 4000], vec![0.0f32; 4000]);
        let mut d2 = dev(&MID);
        d2.set_param(i, if MID[i] > 0.5 { 0.1 } else { 0.9 }).unwrap();
        d2.note_on(40, 0).unwrap();
        d2.process(&mut l2, &mut r2);
        assert!(max_diff(&l2, &fresh.0) > 1e-4 * peak(&fresh.0) as f64, "P{i} is read at note-on");
    }
    // the per-sample parameters take effect on the very next sample, not before; P3 (feedback gain) only shapes what is
    // written into the combs, so it is heard once the first modified write comes back round (the comb delay, ≤ 441
    // samples)
    for i in [0usize, 1, 3, 7, 8, 9, 12] {
        let plain = one_note(&MID, 40, 4000);
        let mut d = dev(&MID);
        d.note_on(40, 0).unwrap();
        let (mut l, mut r) = (vec![0.0f32; 4000], vec![0.0f32; 4000]);
        d.process(&mut l[..1000], &mut r[..1000]);
        d.set_param(i, if MID[i] > 0.5 { 0.1 } else { 0.9 }).unwrap();
        d.process(&mut l[1000..], &mut r[1000..]);
        assert_eq!(bits(&l[..1000]), bits(&plain.0[..1000]), "P{i} changed the past");
        let window = if i == 3 { 1000..1500 } else { 1000..1001 };
        assert!(max_diff(&l[window.clone()], &plain.0[window]) > 1e-6, "P{i} acts too late");
    }
}

#[test]
fn the_note_only_seeds_the_randomisation() {
    let a = one_note(&MID, 40, 3000);
    let b = one_note(&MID, 40, 3000);
    assert_eq!(bits(&a.0), bits(&b.0), "deterministic");
    let c = one_note(&MID, 41, 3000);
    assert!(max_diff(&a.0, &c.0) > 1e-3 * peak(&a.0) as f64, "another note, another seed");
    // the same note on another voice (second simultaneous note) is the same signal
    let mut d = dev(&MID);
    d.note_on(40, 0).unwrap();
    d.note_on(40, 0).unwrap();
    let (mut l, mut r) = (vec![0.0f32; 3000], vec![0.0f32; 3000]);
    d.process(&mut l, &mut r);
    let double: Vec<f32> = a.0.iter().map(|x| x + x).collect();
    assert!(max_diff(&l, &double) < 1e-6 * peak(&double) as f64, "two identical voices add up");
}

#[test]
fn zero_gain_is_silent_and_notes_need_no_velocity() {
    let z = one_note(&with(&MID, 8, 0.0), 40, 2000);
    assert!(z.0.iter().chain(&z.1).all(|&x| x == 0.0));
}

#[test]
fn a_voice_restarts_cleanly_on_the_same_slot() {
    // after the voice expired, a new note on the (lowest, now free) slot sounds like the first one did
    let n = lifetime_samples();
    let mut d = dev(&MID);
    let first = render(&mut d, 3000, &[3000], &[(0, 40, true)]);
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    d.process(&mut l, &mut r);
    assert_eq!(d.active_voices(), 0);
    let again = render(&mut d, 3000, &[3000], &[(0, 40, true)]);
    // The original keeps the delay lines' write position and smoothed time between notes (the first note of a fresh device
    // starts at position 0 with time 0, the repeat at 22149 with the time of the first note), so the repeat is the same
    // sound but NOT the same bits: bit equality would mean the note-on reset that persistent state.
    let (ma, d_) = (peak(&first.0) as f64, max_diff(&first.0, &again.0));
    assert!(bits(&first.0) != bits(&again.0) && bits(&first.1) != bits(&again.1), "the delay position / time were reset by the note-on");
    assert!(d_ < 1e-3 * ma, "{d_}");
}

/// Notes 1 and 2 expire inside a block (offset 12 of blocks 862 and 1732); a note-off for a note that never sounded
/// right after that sample cuts the sub-block there (what one-sample calls would do). The voice renders the rest of its
/// sub-block after expiring, which moves the delay write position that persists to the next note on the slot: the third
/// note differs (research §ID 27, verified against the original by `tests/reference27.rs`, `n20_*` vectors: 6.9e-3 of the
/// peak, here the same size).
#[test]
fn a_voice_renders_the_rest_of_its_sub_block_after_expiring_and_the_delay_position_persists() {
    const BLOCK: usize = 128;
    let n = lifetime_samples();
    assert_eq!(n % BLOCK, 13, "a note-on at offset 0 expires on offset 12 of its expiry block");
    let expiry_block = n / BLOCK; // 862
    let notes = [(0, 16, true), ((expiry_block + 8) as u32 * BLOCK as u32, 12, true), ((2 * expiry_block + 16) as u32 * BLOCK as u32, 20, true)];
    let cut = |off: usize| -> Vec<(u32, u8, bool)> {
        let mut ev = notes.to_vec();
        ev.extend([((expiry_block * BLOCK + off) as u32, 99, false), (((2 * expiry_block + 8) * BLOCK + off) as u32, 99, false)]);
        ev
    };
    let total = (2 * expiry_block + 16 + 360) * BLOCK;
    let run = |ev: &[(u32, u8, bool)]| render(&mut dev(&SONG), total, &[BLOCK], ev);
    let unsplit = run(&notes);
    let cut13 = run(&cut(13)); // right after the last sample
    let cut12 = run(&cut(12)); // one sample before it: the voice still renders to the end of the block
    // after its expiry sample the unsplit voice keeps rendering (a tiny decaying tail); the cut one renders nothing
    let tail = |o: &(Vec<f32>, Vec<f32>)| (expiry_block * BLOCK + 13..(expiry_block + 1) * BLOCK).map(|i| o.0[i].abs().max(o.1[i].abs())).fold(0.0f32, f32::max);
    assert!(tail(&unsplit) > 0.0, "the voice renders to the end of its sub-block after expiring");
    assert_eq!(tail(&cut13), 0.0, "an expired voice is not rendered in the next sub-block");
    assert_eq!(bits(&cut12.0), bits(&unsplit.0), "a cut before the expiry sample changes nothing");
    assert_eq!(bits(&cut12.1), bits(&unsplit.1));
    // identical up to the expiry sample, then the third note (the write position differs by 115 + 115 samples) differs
    let first = (expiry_block * BLOCK + 13) as usize;
    assert_eq!(bits(&unsplit.0[..first]), bits(&cut13.0[..first]));
    let third = (2 * expiry_block + 16) * BLOCK;
    let (d, pk) = (max_diff(&unsplit.0[third..], &cut13.0[third..]).max(max_diff(&unsplit.1[third..], &cut13.1[third..])), peak(&unsplit.0) as f64);
    assert!(d > 2e-3 * pk, "the delay write position did not persist across the expiry: {d:e} of {pk}");
}

// ---------------------------------------------------------------- independent f64 model

fn pade(x: f64) -> f64 {
    let x2 = x * x;
    x * (x2 * (378.0 * x2 + 17325.0) + 135135.0) / (x2 * (x2 * (18.0 * x2 + 3150.0) + 62370.0) + 135135.0)
}

#[derive(Clone, Copy, Default)]
struct Bq {
    c: [f64; 5],
    x1: [f64; 2],
    x2: [f64; 2],
    y1: [f64; 2],
    y2: [f64; 2],
}

impl Bq {
    /// RBJ cookbook low-pass / high-pass, frequency clamped to 20..22000 Hz.
    fn design(hp: bool, f: f64, q: f64) -> [f64; 5] {
        let w = std::f64::consts::TAU * f.clamp(20.0, 22000.0) / 44100.0;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q.max(0.05));
        let (b0, b1) = if hp { ((1.0 + c) / 2.0, -(1.0 + c)) } else { ((1.0 - c) / 2.0, 1.0 - c) };
        let a0 = 1.0 + alpha;
        [b0 / a0, b1 / a0, b0 / a0, -2.0 * c / a0, (1.0 - alpha) / a0]
    }
    fn new(hp: bool, f: f64, q: f64) -> Self {
        Bq { c: Self::design(hp, f, q), ..Bq::default() }
    }
    fn tick(&mut self, x: [f64; 2]) -> [f64; 2] {
        let c = self.c;
        let mut y = [0.0; 2];
        for i in 0..2 {
            y[i] = c[0] * x[i] + c[1] * self.x1[i] + c[2] * self.x2[i] - c[3] * self.y1[i] - c[4] * self.y2[i];
        }
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Ideal fractional delay: `lag` samples (fractional) behind the newest write, linear interpolation, zero history.
struct Line {
    hist: Vec<[f64; 2]>,
    lag: f64,
}

impl Line {
    fn new(seconds: f64) -> Self {
        Line { hist: Vec::new(), lag: seconds * 44100.0 }
    }
    fn at(&self, k: usize) -> [f64; 2] {
        if k < self.hist.len() { self.hist[self.hist.len() - 1 - k] } else { [0.0; 2] }
    }
    fn read(&self) -> [f64; 2] {
        let (n, fr) = (self.lag.floor() as usize, self.lag.fract());
        let (a, b) = (self.at(n), self.at(n + 1));
        [(1.0 - fr) * a[0] + fr * b[0], (1.0 - fr) * a[1] + fr * b[1]]
    }
    fn write(&mut self, x: [f64; 2]) {
        self.hist.push(x);
    }
}

/// One voice, `n` samples, `[L, R]` per sample. Noise draws come from the same hash chain (documented in the module
/// doc of `id27`), everything else is closed-form f64.
fn model(p: &[f32; PARAMS], note: u8, n: usize) -> Vec<[f64; 2]> {
    let p: Vec<f64> = p.iter().map(|&v| f64::from(v)).collect();
    let u = |h: u32| f64::from(h) / 4294967296.0;
    let mut state = u32::from(note) * 1000 + 412_412;
    let mut dr = [0u32; 7];
    for d in dr.iter_mut() {
        state = hash(state);
        *d = state;
    }
    let f = (400.0 * p[2] - 350.0 + 200.0 * u(dr[0]).powi(3)).abs().max(50.0);
    let hp_base = 20.0 + 0.3 * f + 200.0 * u(dr[1]) - 2000.0 * p[4];
    let lp_base = 220.0 + 0.3 * f + 200.0 * u(dr[1]) + 1200.0 * u(dr[2]).powi(2) + 2000.0 * p[5];
    let x = u(dr[5]).powi(4).min(1.0);
    let s = (3.0 - 2.0 * x) * x * x;
    let (mut d0, mut d1) = (Line::new((0.5 / f) as f32 as f64), Line::new((0.5 / f) as f32 as f64));
    let mut d2 = Line::new((1.0 / (2050.0 + 20000.0 * s)) as f32 as f64);
    let mut b4 = Bq::new(false, 20.0 + 20000.0 * u(dr[6]) * (p[11] * p[11] + s), 0.7);
    let mut b5 = Bq::new(true, 20.0 + 20000.0 * p[10] * p[10], 0.7);
    let mut b6 = Bq::new(false, 8000.0, 0.36);
    let (mut b0, mut b1, mut b2, mut b3) = (Bq::default(), Bq::default(), Bq::default(), Bq::default());
    let mut rng = u32::from(note) * 1000 + 412_412;
    let mut t = 0.0f32;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let td = f64::from(t);
        rng = hash(rng);
        let na = f64::from(rng) / 2147483648.0 - 1.0;
        rng = hash(rng);
        let nb = f64::from(rng) / 2147483648.0 - 1.0;
        let sv = (std::f64::consts::TAU * 100.0 * td).sin();
        let k = if sv >= 0.0 { 110.9 + 443.6 * 0.01 } else { 110.9 };
        let sq = sv.signum() * (sv != 0.0) as u8 as f64 * (1.0 - (-k * sv.abs()).exp()) / (1.0 - (-k).exp());
        let env = (-200.0 * td).exp();
        let exc = [(nb + sq) * env, (na + sq) * env];
        let g = 0.85 + 0.14 * p[3];
        let lp_f = lp_base + 2000.0 * p[9] * (-20.0 * td).exp();
        let (lp, hp) = (Bq::design(false, lp_f, 0.7), Bq::design(true, hp_base, 0.7));
        (b0.c, b2.c, b1.c, b3.c) = (lp, lp, hp, hp);
        let (r0, r1) = (d0.read(), d1.read());
        let a = b1.tick(b0.tick(r0));
        d0.write([exc[0] - g * a[0], exc[1] - g * a[1]]);
        let bq = b3.tick(b2.tick(r1));
        d1.write([exc[0] + g * bq[0], exc[1] + g * bq[1]]);
        let m = [exc[0] + p[0] * r0[0] + p[1] * bq[0], exc[1] + p[0] * r0[1] + p[1] * bq[1]];
        let c = d2.read();
        let h = b5.tick(b4.tick([m[0] + 0.99 * c[0], m[1] + 0.99 * c[1]]));
        d2.write(h);
        let o = [(1.0 - p[12]) * m[0] + 0.5 * p[12] * h[0], (1.0 - p[12]) * m[1] + 0.5 * p[12] * h[1]];
        let drv = 10.0 * p[7] + 0.001;
        rng = hash(rng);
        let nr = f64::from(rng) / 2147483648.0 - 1.0;
        rng = hash(rng);
        let nl = f64::from(rng) / 2147483648.0 - 1.0;
        let nlp = b6.tick([nl, nr]);
        let hiss = 0.2 * (-20.0 * td).exp();
        let y = [0, 1].map(|c| p[8] * (pade(drv * o[c]) / pade(drv) + hiss * pade(10.0 * nlp[c])));
        out.push(y);
        t += DT;
    }
    out
}

fn model_error(p: &[f32; PARAMS], note: u8, n: usize) -> (f64, f64) {
    let (l, r) = one_note(p, note, n);
    let m = model(p, note, n);
    let mut err = 0.0f64;
    let mut pk = 0.0f64;
    for i in 0..n {
        err = err.max((f64::from(l[i]) - m[i][0]).abs()).max((f64::from(r[i]) - m[i][1]).abs());
        pk = pk.max(m[i][0].abs()).max(m[i][1].abs());
    }
    (err, pk)
}

/// The model uses ideal fractional delays; the original (and so `Id27`) positions its taps with an f32 read position of
/// magnitude up to 44100 (resolution 0.004 sample), so the match is ~1e-3 of the peak (measured ≤ 1.5e-3, bound 4e-3), not an ulp.
const MODEL_TOL: f64 = 4e-3;

#[test]
fn matches_the_independent_f64_model() {
    let sets: [(&str, [f32; PARAMS], u8); 7] = [
        ("song10", SONG, 12),
        ("mid", MID, 40),
        ("song10 note 63", SONG, 63),
        ("no drive", with(&MID, 7, 0.0), 40),
        ("full drive", with(&MID, 7, 1.0), 40),
        ("dry", with(&MID, 12, 0.0), 40),
        ("wet", with(&MID, 12, 1.0), 40),
    ];
    for (name, p, note) in sets {
        let (err, pk) = model_error(&p, note, 8000);
        eprintln!("model vs Id27, {name}: max error {err:.3e}, peak {pk:.3}, {:.2e} of peak", err / pk);
        assert!(pk > 0.1, "{name}: model too quiet ({pk})");
        assert!(err <= MODEL_TOL * pk, "{name}: max error {err:e} vs peak {pk:e}");
    }
}

#[test]
fn negative_controls_leave_the_model_tolerance() {
    // a deliberately wrong parameter (the model is evaluated for p, the device for p with one change) must be caught
    for i in [0usize, 1, 3, 7, 8, 9, 12, 2, 4, 5, 10, 11] {
        let p = with(&MID, i, if MID[i] > 0.5 { MID[i] - 0.25 } else { MID[i] + 0.25 });
        let (l, _) = one_note(&p, 40, 8000);
        let m = model(&MID, 40, 8000);
        let pk = m.iter().fold(0.0f64, |a, v| a.max(v[0].abs()));
        let err = l.iter().zip(&m).fold(0.0f64, |a, (x, y)| a.max((f64::from(*x) - y[0]).abs()));
        assert!(err > 2.0 * MODEL_TOL * pk, "P{i}: a 0.25 change stayed inside the model tolerance ({err:e} vs {pk:e})");
    }
}
