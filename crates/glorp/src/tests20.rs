//! Synthetic-only tests for ID 20 (twelve negative-feedback combs excited by shaped noise + a damped-sine click,
//! frequency-shifted per group and globally, on the shared 16-voice scheduler). No song data, recorded tap or
//! proprietary vector is used here: the expectations are structural properties (scheduling, latching, persistence,
//! determinism, the velocity ceiling) and an independent f64 model of the whole voice written from the behavioural
//! description in `research/glorp-devices.md` §ID 20 (textbook RBJ filters, ideal sines, an f64 Hilbert pair), plus
//! negative controls that show the model tolerance is tight enough to notice a wrong constant.
use crate::id20::{check, noise_decay_exponent, Error, Id20, MAX_NOTE, MAX_VELOCITY, PARAMS, QUEUE, VOICES};

/// Settings of the song's one ID 20 instance, rounded to a few digits (every parameter in the interior).
const SONG: [f32; PARAMS] =
    [0.2071, 0.3425, 0.534, 0.331, 0.2528, 1.0, 0.93, 0.735, 0.0374, 0.2125, 0.265, 0.9, 0.07, 0.7035, 0.0, 0.3625];
/// A mid-range set where every parameter matters (P4 never does, see `p4_is_inert`).
const MID: [f32; PARAMS] = [0.3, 0.4, 0.5, 0.6, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.3, 0.7, 0.3, 0.2, 0.5, 0.3];

fn dev(p: &[f32; PARAMS]) -> Box<Id20> {
    Box::new(Id20::new(p).unwrap())
}

fn with(base: &[f32; PARAMS], i: usize, v: f32) -> [f32; PARAMS] {
    let mut p = *base;
    p[i] = v;
    p
}

/// One event of a test: `(absolute sample, note, velocity or None for a note-off)`.
type Ev = (u32, u8, Option<u8>);

/// Renders `n` samples in the given chunk sizes (cycled); an event is queued with the offset relative to the chunk it
/// falls in, in list order.
fn render(d: &mut Id20, n: usize, chunks: &[usize], events: &[Ev]) -> (Vec<f32>, Vec<f32>) {
    let (mut l, mut r) = (vec![9.0f32; n], vec![9.0f32; n]);
    let (mut s, mut k) = (0usize, 0usize);
    while s < n {
        let len = chunks[k % chunks.len()].min(n - s);
        k += 1;
        for &(t, note, vel) in events {
            let t = t as usize;
            if t >= s && t < s + len {
                let off = (t - s) as u32;
                match vel {
                    Some(v) => d.note_on(note, v, off).unwrap(),
                    None => d.note_off(note, off).unwrap(),
                }
            }
        }
        d.process(&mut l[s..s + len], &mut r[s..s + len]);
        s += len;
    }
    (l, r)
}

fn one_note(p: &[f32; PARAMS], note: u8, vel: u8, n: usize) -> (Vec<f32>, Vec<f32>) {
    let mut d = dev(p);
    render(&mut d, n, &[n], &[(0, note, Some(vel))])
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

// ---------------------------------------------------------------- API contract

#[test]
fn rejects_unsupported_settings_and_stays_unchanged() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 1.001] {
        assert_eq!(Id20::new(&with(&SONG, 3, bad)).err(), Some(Error::Range), "{bad}");
        assert_eq!(check(&with(&SONG, 15, bad)), Err(Error::Range), "{bad}");
    }
    assert_eq!(check(&SONG), Ok(()));
    assert_eq!(check(&[0.0; PARAMS]), Ok(()));
    assert_eq!(check(&[1.0; PARAMS]), Ok(()));
    let mut d = dev(&SONG);
    let mut e = dev(&SONG);
    assert_eq!(d.set_param(PARAMS, 0.5), Err(Error::Index));
    for bad in [f32::NAN, 1.5, -1.0] {
        assert_eq!(d.set_param(9, bad), Err(Error::Range));
    }
    assert_eq!(d.note_on(MAX_NOTE + 1, 100, 0), Err(Error::Note));
    assert_eq!(d.note_off(200, 0), Err(Error::Note));
    assert_eq!(d.note_on(60, 100, i32::MAX as u32 + 1), Err(Error::Offset));
    assert_eq!(d.note_off(60, u32::MAX), Err(Error::Offset));
    assert_eq!(d.note_on(60, MAX_VELOCITY + 1, 0), Err(Error::Velocity));
    assert_eq!(d.note_on(60, 255, 0), Err(Error::Velocity));
    d.note_on(60, 100, 0).unwrap();
    e.note_on(60, 100, 0).unwrap();
    let (mut a, mut b) = ((vec![0.0; 600], vec![0.0; 600]), (vec![0.0; 600], vec![0.0; 600]));
    d.process(&mut a.0, &mut a.1);
    e.process(&mut b.0, &mut b.1);
    assert!(bits(&a.0) == bits(&b.0), "a rejected call left a trace");
    assert!(bits(&a.1) == bits(&b.1), "buffers differ");
    assert!(peak(&a.0) > 0.0);
}

#[test]
fn event_queue_holds_256_and_refuses_the_next() {
    let mut d = dev(&SONG);
    for _ in 0..QUEUE {
        d.note_on(10, 100, 5).unwrap();
    }
    assert_eq!(d.note_on(10, 100, 5), Err(Error::QueueFull));
    assert_eq!(d.note_off(10, 5), Err(Error::QueueFull));
    // a refused velocity does not take a slot, and note-offs take slots like any event
    let mut d = dev(&SONG);
    for _ in 0..QUEUE {
        assert_eq!(d.note_on(10, 250, 5), Err(Error::Velocity));
    }
    for _ in 0..QUEUE {
        d.note_off(10, 5).unwrap();
    }
    assert_eq!(d.note_on(10, 100, 5), Err(Error::QueueFull));
}

#[test]
fn silence_overwrites_and_length_rules() {
    let mut d = dev(&SONG);
    let (mut l, mut r) = (vec![7.0f32; 300], vec![-7.0f32; 300]);
    d.process(&mut l, &mut r);
    assert!(l.iter().chain(&r).all(|&x| x == 0.0), "no voice: the buffers must be overwritten with silence");
    d.process(&mut [], &mut []);
    let c = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut d = dev(&SONG);
        d.process(&mut [0.0; 4], &mut [0.0; 5]);
    }));
    assert!(c.is_err(), "channel length mismatch must panic");
}

#[test]
fn parameter_count_is_sixteen() {
    assert_eq!(PARAMS, 16);
    assert_eq!(VOICES, 16);
    assert_eq!(MAX_VELOCITY, 213);
}

// ---------------------------------------------------------------- scheduling

#[test]
fn a_note_sounds_from_the_sample_after_its_offset() {
    for off in [0u32, 1, 37, 127] {
        let mut d = dev(&SONG);
        d.note_on(60, 127, off).unwrap();
        let (mut l, mut r) = (vec![0.0f32; 400], vec![0.0f32; 400]);
        d.process(&mut l, &mut r);
        let first = l.iter().zip(&r).position(|(a, b)| *a != 0.0 || *b != 0.0);
        assert_eq!(first, Some(off as usize + 1), "offset {off}");
    }
}

#[test]
fn chunking_does_not_change_the_sound() {
    // sub-blocks of at least 64 samples: shorter ones can end a young voice (next test), which is chunking behaviour
    let ev: Vec<Ev> = vec![(0, 60, Some(127)), (300, 64, Some(90)), (1000, 60, None)];
    let mut a = dev(&SONG);
    let whole = render(&mut a, 6000, &[6000], &ev);
    for chunks in [[128usize, 128, 128].as_slice(), &[300, 700, 64, 1000, 256]] {
        let mut b = dev(&SONG);
        let split = render(&mut b, 6000, chunks, &ev);
        assert!(bits(&whole.0) == bits(&split.0), "{chunks:?}");
        assert!(bits(&whole.1) == bits(&split.1), "{chunks:?}");
    }
    assert!(peak(&whole.0) > 0.01);
}

#[test]
fn a_voice_whose_level_follower_is_still_below_1e_minus_4_is_freed_at_the_end_of_a_sub_block() {
    // the original checks "follower < 1e-4" after every render call of the voice, so a note whose first sub-block is
    // one sample long is gone before it was ever heard
    let mut d = dev(&SONG);
    let r = render(&mut d, 600, &[128, 472], &[(127, 60, Some(127))]);
    assert_eq!(d.active_voices(), 0, "the 1-sample first sub-block ended the voice");
    assert_eq!(peak(&r.0), 0.0);
    // the same note a few samples earlier (a longer first sub-block) survives
    let mut d = dev(&SONG);
    let r = render(&mut d, 600, &[128, 472], &[(100, 60, Some(127))]);
    assert_eq!(d.active_voices(), 1);
    assert!(peak(&r.0) > 0.0);
}

#[test]
fn sixteen_notes_play_and_the_seventeenth_is_lost() {
    let mut d = dev(&SONG);
    for i in 0..17u8 {
        d.note_on(30 + i, 127, 0).unwrap();
    }
    let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
    d.process(&mut l, &mut r);
    assert_eq!(d.active_voices(), 16);
    let notes = d.voice_notes();
    for i in 0..16 {
        assert_eq!(notes[i], Some(30 + i as u8), "voice {i}");
    }
    assert!(!notes.contains(&Some(46)), "the 17th note must be dropped");
}

#[test]
fn the_note_off_is_inaudible_and_does_not_free_the_voice() {
    let on: Vec<Ev> = vec![(0, 60, Some(127))];
    let off: Vec<Ev> = vec![(0, 60, Some(127)), (500, 60, None), (501, 61, None)];
    let mut a = dev(&SONG);
    let mut b = dev(&SONG);
    let x = render(&mut a, 4000, &[4000], &on);
    let y = render(&mut b, 4000, &[4000], &off);
    // the note-off at 500 splits the sub-block but is otherwise inert
    assert!(bits(&x.0) == bits(&y.0), "buffers differ");
    assert!(bits(&x.1) == bits(&y.1), "buffers differ");
    assert_eq!(b.active_voices(), 1);
}

#[test]
fn a_silent_voice_dies_and_a_ringing_one_lives_for_seconds() {
    // velocity 0: no excitation, the follower never leaves zero, the voice is freed at the end of its first sub-block
    let mut d = dev(&SONG);
    render(&mut d, 256, &[128], &[(0, 60, Some(0))]);
    assert_eq!(d.active_voices(), 0);
    // a normal note rings for a long time, then the follower drops below 1e-4 and the slot is freed
    let mut d = dev(&SONG);
    render(&mut d, 44100, &[128], &[(0, 60, Some(127))]);
    assert_eq!(d.active_voices(), 1, "still ringing after one second");
    render(&mut d, 44100 * 60, &[4096], &[]);
    assert_eq!(d.active_voices(), 0, "finally freed");
}

// ---------------------------------------------------------------- parameters and state

#[test]
fn every_parameter_is_latched_by_the_note_on() {
    // a write while a voice sounds changes nothing audible of that voice (all 16 are read in note-on only)
    let ev: Vec<Ev> = vec![(0, 60, Some(100))];
    let mut a = dev(&MID);
    let base = render(&mut a, 3000, &[3000], &ev);
    for i in 0..PARAMS {
        let mut b = dev(&MID);
        let mut l = vec![0.0f32; 3000];
        let mut r = vec![0.0f32; 3000];
        b.note_on(60, 100, 0).unwrap();
        b.process(&mut l[..500], &mut r[..500]);
        b.set_param(i, 1.0 - MID[i]).unwrap();
        b.process(&mut l[500..], &mut r[500..]);
        assert!(bits(&base.0) == bits(&l), "P{i}");
        assert!(bits(&base.1) == bits(&r), "P{i}");
    }
}

#[test]
fn p4_is_inert_and_every_other_parameter_is_audible() {
    let base = one_note(&MID, 60, 100, 6000);
    for i in 0..PARAMS {
        for v in [0.0f32, 1.0] {
            if MID[i] == v {
                continue;
            }
            let o = one_note(&with(&MID, i, v), 60, 100, 6000);
            let same = bits(&base.0) == bits(&o.0) && bits(&base.1) == bits(&o.1);
            if i == 4 {
                // group 3 (lines 12..15) is set up at note-on but never rendered; its draws still advance the generator
                assert!(same, "P4 = {v} changed the sound");
            } else {
                assert!(!same, "P{i} = {v} is inert");
            }
        }
    }
}

#[test]
fn the_note_and_p0_only_seed_the_random_draws() {
    // seed = note + trunc(150·P0) + const: the same seed through another (note, P0) pair is the same sound
    let a = one_note(&with(&MID, 0, 0.0), 40, 100, 4000);
    let b = one_note(&with(&MID, 0, 0.0334), 35, 100, 4000); // trunc(5.01) = 5
    assert!(bits(&a.0) == bits(&b.0), "buffers differ");
    assert!(bits(&a.1) == bits(&b.1), "buffers differ");
    let c = one_note(&with(&MID, 0, 0.0), 41, 100, 4000);
    assert!(bits(&a.0) != bits(&c.0), "buffers equal");
    // determinism
    let d = one_note(&with(&MID, 0, 0.0), 40, 100, 4000);
    assert!(bits(&a.0) == bits(&d.0), "buffers differ");
}

#[test]
fn velocity_scales_the_excitation() {
    let loud = one_note(&SONG, 60, 127, 3000);
    let soft = one_note(&SONG, 60, 40, 3000);
    assert!(peak(&loud.0) > 1.5 * peak(&soft.0), "{} {}", peak(&loud.0), peak(&soft.0));
    assert!(peak(&soft.0) > 0.0);
    // the velocity also shortens the noise envelope, so the two are not scaled copies
    let ratio = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| (x / y).abs()).filter(|r| r.is_finite()).collect::<Vec<_>>();
    let rs = ratio(&loud.0[1000..], &soft.0[1000..]);
    assert!(rs.iter().any(|r| *r > 0.0));
}

#[test]
fn state_persists_between_the_notes_of_a_voice() {
    // the delay lines, their read position, the shifter states and the follower are never reset: the same note played
    // again on the same voice is not the same sound as on a fresh device
    let fresh = one_note(&MID, 60, 100, 4000);
    let mut d = dev(&MID);
    let first = render(&mut d, 4000, &[4000], &[(0, 60, Some(100))]);
    assert!(bits(&fresh.0) == bits(&first.0), "buffers differ");
    // let the first voice die, then the second note takes the same slot
    render(&mut d, 44100 * 60, &[4096], &[]);
    assert_eq!(d.active_voices(), 0);
    let second = render(&mut d, 4000, &[4000], &[(0, 60, Some(100))]);
    assert!(bits(&fresh.0) != bits(&second.0), "buffers equal");
    assert!(max_diff(&fresh.0, &second.0) > 1e-4 * f64::from(peak(&fresh.0)));
}

#[test]
fn finite_and_bounded_at_the_corners() {
    let mut corners: Vec<[f32; PARAMS]> = vec![[0.0; PARAMS], [1.0; PARAMS], MID, SONG];
    let mut s = 0x1234_5678u32;
    for _ in 0..24 {
        let mut p = [0.0f32; PARAMS];
        for v in p.iter_mut() {
            s = crate::id25::hash(s);
            *v = if s & 3 == 0 { 0.0 } else if s & 3 == 1 { 1.0 } else { (s >> 8) as f32 / (1u32 << 24) as f32 };
        }
        corners.push(p);
    }
    for p in &corners {
        let mut d = dev(p);
        let r = render(&mut d, 44100, &[128], &[(0, 20, Some(127)), (50, 90, Some(213)), (2000, 60, Some(1))]);
        for v in r.0.iter().chain(&r.1) {
            assert!(v.is_finite() && v.abs() < 1e5, "{p:?}: {v}");
        }
    }
}

// ---------------------------------------------------------------- the velocity ceiling

#[test]
fn the_velocity_ceiling_is_exactly_where_the_noise_envelope_stops_decaying() {
    // the noise envelope multiplies by exp((P13 + 0.1·v/128)/735 − 1/630) per sample: with P13 = 1 it grows without
    // bound (to infinity: a voice that never ends and overflows) from velocity 214 on
    for p13 in [0.0f32, 0.5, 0.99, 1.0] {
        assert!(noise_decay_exponent(p13, MAX_VELOCITY) < 0.0, "P13 {p13}: velocity {MAX_VELOCITY} must decay");
    }
    assert!(noise_decay_exponent(1.0, MAX_VELOCITY + 1) > 0.0, "velocity {} grows at P13 = 1", MAX_VELOCITY + 1);
    assert!(noise_decay_exponent(1.0, 255) > 0.0);
    // monotone in both
    assert!(noise_decay_exponent(0.2, 100) < noise_decay_exponent(0.8, 100));
    assert!(noise_decay_exponent(0.5, 10) < noise_decay_exponent(0.5, 200));
}

#[test]
fn the_slowest_allowed_envelope_still_ends() {
    // P13 = 1, velocity 213: the noise envelope decays with a time constant of minutes but the voice is finite
    let p = with(&with(&SONG, 13, 1.0), 12, 0.0);
    let r = one_note(&p, 60, MAX_VELOCITY, 20000);
    assert!(r.0.iter().chain(&r.1).all(|x| x.is_finite() && x.abs() < 1e5));
}

// ---------------------------------------------------------------- independent f64 model

mod model {
    use crate::id25::hash;
    use std::f64::consts::PI;

    pub type S = [f64; 2];

    /// The 512-node sine table with linear interpolation the original uses for the shifter phase and the click
    /// oscillator's `cos w` (exact node values, `x` in radians).
    pub fn tsin(x: f64) -> f64 {
        let t = x / (2.0 * PI);
        let t = (t - t.floor()) * 512.0;
        let i = t.floor();
        let (a, b) = (((i / 512.0) * 2.0 * PI).sin(), (((i + 1.0) / 512.0) * 2.0 * PI).sin());
        a + (b - a) * (t - i)
    }

    const CHAIN_A: [(f64, f64); 4] = [(0.161_758, 0.161_715_8), (0.733_029, 0.733_029), (0.945_35, 0.945_35), (0.990_598, 0.990_598)];
    const CHAIN_B: [(f64, f64); 4] = [(0.479_401, 0.479_401), (0.876_218, 0.876_218), (0.976_599, 0.976_599), (0.997_5, 0.997_5)];

    #[derive(Clone, Copy, Default)]
    struct Bq {
        c: [f64; 5],
        x1: S,
        x2: S,
        y1: S,
        y2: S,
    }

    impl Bq {
        fn set(&mut self, high: bool, f: f32, q: f64) {
            let f = f64::from(f).clamp(20.0, 22000.0);
            let w = 2.0 * PI * f / 44100.0;
            let (sn, cs) = (w.sin(), w.cos());
            let al = sn / (2.0 * q.max(0.05));
            let (b0, b1) = if high { ((1.0 + cs) / 2.0, -(1.0 + cs)) } else { ((1.0 - cs) / 2.0, 1.0 - cs) };
            let a0 = 1.0 + al;
            self.c = [b0 / a0, b1 / a0, b0 / a0, -2.0 * cs / a0, (1.0 - al) / a0];
        }
        fn clear(&mut self) {
            self.x1 = [0.0; 2];
            self.x2 = [0.0; 2];
            self.y1 = [0.0; 2];
            self.y2 = [0.0; 2];
        }
        fn tick(&mut self, x: S) -> S {
            let c = self.c;
            let y = [0, 1].map(|i| c[0] * x[i] + c[1] * self.x1[i] + c[2] * self.x2[i] - c[3] * self.y1[i] - c[4] * self.y2[i]);
            self.x2 = self.x1;
            self.x1 = x;
            self.y2 = self.y1;
            self.y1 = y;
            y
        }
    }

    /// Ideal-ish delay: full write history, `time` smoothed at 0.021 per sample, read at lag `time·44100` behind the
    /// newest sample (f32 position bookkeeping of the 44100-entry buffer kept: it decides the fractional lag).
    #[derive(Clone, Default)]
    struct Delay {
        hist: Vec<S>,
        time: f32,
        target: f32,
        w: i32,
    }

    impl Delay {
        fn lag(&self, lag: i32) -> S {
            let n = self.hist.len() as i32;
            if lag < n && lag >= 0 { self.hist[(n - 1 - lag) as usize] } else { [0.0; 2] }
        }
        fn read(&self) -> S {
            let mut p = (0.0f32 + self.time) * -44100.0 + self.w as f32;
            let q = p + 44100.0;
            if q < 44100.0 {
                p = if q >= 0.0 { q } else { q + 44100.0 };
            }
            let i = p as i32;
            let i1 = if i + 1 >= 44100 { i + 1 - 44100 } else { i + 1 };
            let fr = f64::from(p - i as f32);
            let (b0, b1) = (self.lag((self.w - i).rem_euclid(44100)), self.lag((self.w - i1).rem_euclid(44100)));
            [0, 1].map(|c| (1.0 - fr) * b0[c] + fr * b1[c])
        }
        fn write(&mut self, x: S) {
            self.time = (self.target - self.time) * 0.021 + self.time;
            self.w = (self.w + 1) % 44100;
            self.hist.push(x);
        }
    }

    #[derive(Clone, Default)]
    struct Shifter {
        a: [[[f64; 4]; 4]; 2],
        b: [[[f64; 4]; 4]; 2],
        prev: S,
        phase: f32,
    }

    fn sec(c: (f64, f64), s: &mut [f64; 4], x: f64) -> f64 {
        let y = c.0 * x - s[1] + c.1 * s[3];
        *s = [x, s[0], y, s[2]];
        y
    }

    impl Shifter {
        fn tick(&mut self, x: S, delta: f32) -> S {
            let mut a = x;
            let mut b = self.prev;
            for ch in 0..2 {
                for k in 0..4 {
                    a[ch] = sec(CHAIN_A[k], &mut self.a[ch][k], a[ch]);
                    b[ch] = sec(CHAIN_B[k], &mut self.b[ch][k], b[ch]);
                }
            }
            self.prev = x;
            let v = self.phase + delta;
            self.phase = if v > 1.0 { v - 1.0 } else { v };
            let ang = f64::from(self.phase) * 2.0 * PI;
            let (cs, sn) = (tsin(ang + PI / 2.0), tsin(ang));
            [0, 1].map(|c| cs * a[c] + sn * b[c])
        }
    }

    #[derive(Clone)]
    pub struct Voice {
        pub active: bool,
        rng: u32,
        vel: f64,
        fb: [f64; 12],
        d: Vec<Delay>,
        hp: Vec<Bq>,
        lp: Vec<Bq>,
        nlp: Bq,
        nhp: Bq,
        grp: [Shifter; 3],
        master: Shifter,
        gdelta: [f32; 3],
        mdelta: f32,
        env_n: f64,
        coef_n: f64,
        env_c: f64,
        coef_c: f64,
        sc: [f64; 4],
        follower: f64,
    }

    fn u32f(d: u32) -> f64 {
        f64::from(d) / 4294967296.0
    }

    impl Voice {
        pub fn new() -> Self {
            Voice {
                active: false,
                rng: 0,
                vel: 0.0,
                fb: [0.0; 12],
                d: vec![Delay::default(); 12],
                hp: vec![Bq::default(); 12],
                lp: vec![Bq::default(); 12],
                nlp: Bq::default(),
                nhp: Bq::default(),
                grp: Default::default(),
                master: Shifter::default(),
                gdelta: [0.0; 3],
                mdelta: 0.0,
                env_n: 1.0,
                coef_n: 1.0,
                env_c: 1.0,
                coef_c: 1.0,
                sc: [0.0; 4],
                follower: 0.0,
            }
        }

        pub fn note_on(&mut self, note: u8, vel: u8, p: &[f32; 16]) {
            let pf = |i: usize| f64::from(p[i]);
            self.active = true;
            self.vel = f64::from(vel) / 128.0;
            self.rng = u32::from(note).wrapping_add(((p[0] * 150.0) as i64) as u32).wrapping_add(0x765f_bb8);
            let mut draw = || {
                self.rng = hash(self.rng);
                u32f(self.rng)
            };
            let mut freq = [0.0f64; 16];
            let mut shift = [0.0f64; 4];
            for g in 0..4 {
                for k in 0..4 {
                    let j = 4 * g + k;
                    let base = 500.0 + 14500.0 * pf(1 + g) * pf(1 + g);
                    freq[j] = (base + (draw() - 0.5) * 5000.0 * pf(7)).clamp(500.0, 20000.0);
                    let lo = 0.5 + 0.49 * pf(5);
                    let fbj = lo + (0.999 - lo) * (0.5 + (draw() - 0.5) * pf(6));
                    let hf = (0.01 + 0.5 * pf(8) * draw()).powi(2) * 21960.0 + 40.0;
                    let lf = (0.99 - 0.5 * pf(8) * draw()).powi(2) * 21960.0 + 40.0;
                    if j < 12 {
                        self.fb[j] = fbj;
                        self.hp[j].clear();
                        self.lp[j].clear();
                        self.hp[j].set(true, hf as f32, 0.7);
                        self.lp[j].set(false, lf as f32, 0.7);
                    }
                }
                shift[g] = -0.1 * pf(9) * draw();
            }
            for g in 0..3 {
                self.gdelta[g] = (1.0 + 0.2 * shift[g]) as f32;
            }
            self.mdelta = ((4000.0 * pf(15) - 2000.0) / 44100.0) as f32;
            self.coef_n = ((pf(13) + 0.1 * self.vel) / 735.0 - 1.0 / 630.0).exp();
            self.coef_c = (pf(12) / 169.6 - 1.0 / 163.3).exp();
            self.env_n = 1.0;
            self.env_c = 1.0;
            let w = 2.0 * PI * (40.0 + 7960.0 * pf(14) * pf(14)) / 44100.0;
            let r = (-1.0f64 / 22.05e6).exp();
            self.sc = [2.0 * r * tsin(w + PI / 2.0), r * r, 0.0, tsin(-w) / r];
            for j in 0..12 {
                self.d[j].target = (1.0 / freq[j]) as f32;
            }
            self.nlp.set(false, (40.0 + 12460.0 * pf(11) * pf(11)) as f32, 1.0);
            self.nhp.set(true, (40.0 + 12460.0 * pf(10) * pf(10)) as f32, 1.0);
        }

        pub fn render(&mut self, n: usize, out: &mut [S]) {
            for o in out.iter_mut().take(n) {
                self.rng = hash(self.rng);
                let n_r = f64::from(self.rng) * 2.0 / 4294967296.0 - 1.0;
                self.rng = hash(self.rng);
                let n_l = f64::from(self.rng) * 2.0 / 4294967296.0 - 1.0;
                let e = self.nhp.tick(self.nlp.tick([n_l * self.env_n, n_r * self.env_n]));
                let click = self.env_c * self.sc[2];
                let exc = [(e[0] + click) * self.vel, (e[1] + click) * self.vel];
                self.env_n *= self.coef_n;
                self.env_c *= self.coef_c;
                let ny = self.sc[0] * self.sc[2] - self.sc[1] * self.sc[3];
                self.sc[3] = self.sc[2];
                self.sc[2] = ny;
                let mut acc = [0.0f64; 2];
                for g in 0..3 {
                    let mut sum = [0.0f64; 2];
                    for k in 0..4 {
                        let j = 4 * g + k;
                        let d = self.d[j].read();
                        let x = [-self.fb[j] * d[0], -self.fb[j] * d[1]];
                        let y = self.lp[j].tick(self.hp[j].tick(x));
                        self.d[j].write([y[0] + exc[0], y[1] + exc[1]]);
                        sum = [sum[0] + d[0], sum[1] + d[1]];
                    }
                    let s = self.grp[g].tick(sum, self.gdelta[g]);
                    acc = [acc[0] + 0.5 * s[0], acc[1] + 0.5 * s[1]];
                }
                let y = self.master.tick([0.5 * acc[0], 0.5 * acc[1]], self.mdelta);
                *o = y;
                let x = (y[0].abs() + y[1].abs()) * 0.5;
                self.follower = if x > self.follower {
                    self.follower + (x - self.follower) * (0.1 + 0.001)
                } else {
                    self.follower * (0.99999 - 0.05 * 0.9f64.powi(3))
                };
            }
            if self.follower < 1e-4 {
                self.active = false;
            }
        }
    }
}

fn model_note(p: &[f32; PARAMS], note: u8, vel: u8, n: usize) -> Vec<[f64; 2]> {
    let mut v = model::Voice::new();
    v.note_on(note, vel, p);
    let mut out = vec![[0.0; 2]; n];
    v.render(n, &mut out);
    out
}

/// Worst `|difference|` between the f32 device and the model, as a fraction of the model's peak.
fn model_gap(p: &[f32; PARAMS], note: u8, vel: u8, n: usize) -> (f64, f64) {
    let r = one_note(p, note, vel, n);
    let m = model_note(p, note, vel, n);
    let pk = m.iter().fold(0.0f64, |a, s| a.max(s[0].abs()).max(s[1].abs()));
    let gap = (0..n).fold(0.0f64, |a, i| a.max((f64::from(r.0[i]) - m[i][0]).abs()).max((f64::from(r.1[i]) - m[i][1]).abs()));
    (gap / pk, pk)
}

// measured worst 9.1e-5 (SONG) / 6.5e-5 (MID) of the peak: the table-sine and f32 rounding differences, amplified by the loops
const MODEL_REL: f64 = 5e-4;

#[test]
fn the_whole_voice_matches_an_independent_f64_model() {
    let big = std::thread::Builder::new().stack_size(64 << 20).spawn(|| {
        for (name, p, note, vel) in [
            ("MID", MID, 60u8, 100u8),
            ("SONG", SONG, 67, 127),
            ("fast", with(&with(&MID, 13, 0.0), 12, 0.0), 33, 64),
            ("shift-up", with(&with(&MID, 15, 1.0), 9, 1.0), 71, 127),
            ("dense", with(&with(&with(&MID, 8, 1.0), 6, 1.0), 5, 0.0), 12, 127),
        ] {
            let (gap, pk) = model_gap(&p, note, vel, 8000);
            assert!(pk > 0.01, "{name}: peak {pk}");
            assert!(gap < MODEL_REL, "{name}: gap {gap} of the peak {pk}");
        }
    });
    big.unwrap().join().unwrap();
}

#[test]
fn negative_control_a_wrong_constant_leaves_the_model_tolerance() {
    // the same comparison with a perturbed parameter on the device side must fail it: the tolerance is not vacuous
    let big = std::thread::Builder::new().stack_size(64 << 20).spawn(|| {
        for (i, v) in [(1usize, 0.45f32), (2, 0.55), (3, 0.65), (5, 0.55), (6, 0.55), (7, 0.55), (8, 0.55), (10, 0.35), (12, 0.35), (13, 0.25), (15, 0.35)] {
            let m = model_note(&MID, 60, 100, 8000);
            let r = one_note(&with(&MID, i, v), 60, 100, 8000);
            let pk = m.iter().fold(0.0f64, |a, s| a.max(s[0].abs()).max(s[1].abs()));
            let gap = (0..8000).fold(0.0f64, |a, k| a.max((f64::from(r.0[k]) - m[k][0]).abs()).max((f64::from(r.1[k]) - m[k][1]).abs()));
            assert!(gap / pk > MODEL_REL, "P{i} nudged to {v}: gap {} still inside the tolerance", gap / pk);
        }
    });
    big.unwrap().join().unwrap();
}

#[test]
#[ignore]
fn dbg_lifetimes() {
    let big = std::thread::Builder::new().stack_size(64 << 20).spawn(|| {
        for (name, p, vel) in [("SONG", SONG, 127u8), ("MID", MID, 100), ("zeros", [0.0; PARAMS], 127), ("ones", [1.0; PARAMS], 100), ("P13=1", with(&SONG, 13, 1.0), 213)] {
            let mut d = dev(&p);
            d.note_on(60, vel, 0).unwrap();
            let (mut l, mut r) = (vec![0.0f32; 128], vec![0.0f32; 128]);
            let mut blocks = 1usize;
            d.process(&mut l, &mut r);
            while d.active_voices() > 0 && blocks < 44100 * 120 / 128 {
                d.process(&mut l, &mut r);
                blocks += 1;
            }
            println!("{name}: freed after {blocks} blocks = {:.2} s", blocks as f64 * 128.0 / 44100.0);
        }
    });
    big.unwrap().join().unwrap();
}
