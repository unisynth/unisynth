//! Synthetic-only tests for ID 21 (additive recursive sine bank on the shared 16-voice scheduler). No song data,
//! recorded tap or proprietary vector is used here: the expectations are structural properties (scheduling, envelope
//! closed forms, parameter latching, memory bound, determinism) and an independent f64 model of the whole voice
//! written from the behavioural description in `research/glorp-devices.md` §ID 21 (exact `exp`/`ln`/`pow`, a
//! closed-form amplitude law, an ideal sine except for the documented 512-node interpolated table in the oscillator
//! set-up, textbook damped resonators in f64).
use crate::id21::{check, partials, Adsr, Error, Id21, MAX_NOTE, MAX_PARTIALS, PARAMS, QUEUE, VOICES};
use crate::id25::{hash, DT};

/// Song-device-6-like settings (rounded): every parameter in the interior, 4 outer x 9 inner partials.
const SONG6: [f32; PARAMS] = [0.14, 0.224, 0.685, 0.4, 0.08, 0.485, 0.045, 0.305, 0.047, 0.717, 1.0, 1.0, 0.04];
/// Song-device-5-like settings: 6 x 19 partials.
const SONG5: [f32; PARAMS] = [0.0, 0.0237, 0.0, 0.55, 0.055, 0.888, 0.182, 0.178, 0.1, 0.835, 1.0, 1.0, 1.0];
/// A mid-range set where every parameter matters: 6 x 4 partials.
const MID: [f32; PARAMS] = [0.01, 0.02, 0.5, 0.05, 0.05, 0.4, 0.03, 0.31, 0.2, 0.6, 0.3, 0.8, 0.5];

fn dev(p: &[f32; PARAMS]) -> Box<Id21> {
    Box::new(Id21::new(p).unwrap())
}

fn with(base: &[f32; PARAMS], i: usize, v: f32) -> [f32; PARAMS] {
    let mut p = *base;
    p[i] = v;
    p
}

/// Renders `n` samples in the given chunk sizes (cycled); `events` = `(absolute sample, note, on)` queued with the
/// offset relative to the chunk they fall in, in list order.
fn render(d: &mut Id21, n: usize, chunks: &[usize], events: &[(u32, u8, bool)]) -> (Vec<f32>, Vec<f32>) {
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

fn max_diff(a: &[f32], b: &[f32]) -> f64 {
    a.iter().zip(b).fold(0.0, |m, (x, y)| m.max((*x as f64 - *y as f64).abs()))
}

// ---------------------------------------------------------------- API contract and memory bound

#[test]
fn rejects_unsupported_settings_and_stays_unchanged() {
    for bad in [f32::NAN, f32::INFINITY, -0.001, 1.001] {
        assert_eq!(Id21::new(&with(&MID, 3, bad)).err(), Some(Error::Range), "{bad}");
        assert_eq!(check(&with(&MID, 11, bad)), Err(Error::Range));
    }
    let mut d = dev(&MID);
    let mut e = dev(&MID);
    assert_eq!(d.set_param(PARAMS, 0.5), Err(Error::Index));
    for bad in [f32::NAN, 1.5, -1.0] {
        assert_eq!(d.set_param(9, bad), Err(Error::Range));
    }
    // too many partials: P6 at 0.5 makes 51 x 6 = 306 (fine); 1.0 makes 101 x 6 = 606 > MAX_PARTIALS
    assert_eq!(d.set_param(6, 1.0), Err(Error::Partials));
    assert_eq!(d.note_on(MAX_NOTE + 1, 0), Err(Error::Note));
    assert_eq!(d.note_off(200, 0), Err(Error::Note));
    assert_eq!(d.note_on(60, i32::MAX as u32 + 1), Err(Error::Offset));
    assert_eq!(d.note_off(60, u32::MAX), Err(Error::Offset));
    d.note_on(60, 0).unwrap();
    e.note_on(60, 0).unwrap();
    let (mut a, mut b) = ((vec![0.0; 900], vec![0.0; 900]), (vec![0.0; 900], vec![0.0; 900]));
    d.process(&mut a.0, &mut a.1);
    e.process(&mut b.0, &mut b.1);
    assert_eq!(bits(&a.0), bits(&b.0), "a rejected call left a trace");
    assert_eq!(bits(&a.1), bits(&b.1));
    assert!(peak(&a.0) > 0.0);
}

#[test]
fn partial_count_law_and_the_memory_bound() {
    let mut p = [0.0f32; PARAMS];
    assert_eq!(partials(&p), 1);
    // (trunc(100·P4) + 1) · (trunc(100·P6) + 1): the original's count, truncating, not rounding
    p[4] = 0.0549;
    p[6] = 0.1829;
    assert_eq!(partials(&p), 6 * 19);
    p[4] = 0.5;
    p[6] = 0.5;
    assert_eq!(partials(&p), 51 * 51);
    p[4] = 1.0;
    p[6] = 1.0;
    assert_eq!(partials(&p), 101 * 101);
    // the bound: exactly MAX_PARTIALS is accepted, one more step is refused, by `new` and by `set_param`
    assert_eq!(MAX_PARTIALS, 512);
    let at = |a: f32, b: f32| with(&with(&MID, 4, a), 6, b);
    assert_eq!(partials(&at(0.3105, 0.1505)), 32 * 16);
    assert_eq!(check(&at(0.3105, 0.1505)), Ok(()));
    assert_eq!(check(&at(0.3105, 0.1605)), Err(Error::Partials));
    assert_eq!(check(&at(0.3205, 0.1505)), Err(Error::Partials));
    assert_eq!(check(&at(1.0, 0.0)), Ok(()), "101 x 1 partials fit");
    assert_eq!(check(&at(1.0, 1.0)), Err(Error::Partials));
    assert_eq!(Id21::new(&at(0.0, 1.0)).map(|_| ()), Ok(()));
    assert_eq!(Id21::new(&at(1.0, 1.0)).err(), Some(Error::Partials));
    let mut d = dev(&at(0.3105, 0.1505));
    assert_eq!(d.set_param(6, 0.1605), Err(Error::Partials));
    assert_eq!(d.set_param(4, 0.3205), Err(Error::Partials));
    assert_eq!(d.set_param(6, 0.14), Ok(()));
    // 16 voices x MAX_PARTIALS x 6 floats plus a little: the whole device stays small and fixed
    let size = core::mem::size_of::<Id21>();
    assert!(size >= VOICES * MAX_PARTIALS * 24, "{size}");
    assert!(size <= 256 * 1024, "{size} bytes");
}

#[test]
fn event_queue_holds_256_and_refuses_the_next() {
    let mut d = dev(&MID);
    for _ in 0..QUEUE {
        d.note_on(10, 5).unwrap();
    }
    assert_eq!(d.note_on(10, 5), Err(Error::QueueFull));
    assert_eq!(d.note_off(10, 5), Err(Error::QueueFull));
}

#[test]
fn silence_overwrites_and_length_rules() {
    let mut d = dev(&MID);
    let (mut l, mut r) = (vec![7.0f32; 300], vec![-7.0f32; 300]);
    d.process(&mut l, &mut r);
    assert!(l.iter().chain(&r).all(|&x| x == 0.0), "the instrument overwrites its output");
    d.process(&mut [], &mut []);
    let err = std::panic::catch_unwind(|| {
        let mut d = Id21::new(&MID).unwrap();
        d.process(&mut [0.0; 4], &mut [0.0; 3]);
    })
    .err()
    .unwrap();
    assert!(err.downcast_ref::<&str>().is_some_and(|m| m.contains("length")) || err.downcast_ref::<String>().is_some_and(|m| m.contains("length")));
}

// ---------------------------------------------------------------- scheduling

#[test]
fn onset_is_offset_accurate() {
    for off in [0u32, 1, 37, 127] {
        let mut d = dev(&SONG5);
        d.note_on(72, off).unwrap();
        let (mut l, mut r) = (vec![1.0f32; 300], vec![1.0f32; 300]);
        d.process(&mut l, &mut r);
        let first = l.iter().position(|&x| x != 0.0).unwrap();
        assert_eq!(first, off as usize);
        assert!(r[..first].iter().all(|&x| x == 0.0));
    }
    let mut d = dev(&SONG5);
    d.note_on(72, 200).unwrap();
    let (mut l, mut r) = (vec![0.0f32; 128], vec![0.0f32; 128]);
    d.process(&mut l, &mut r);
    assert!(l.iter().all(|&x| x == 0.0));
    assert_eq!(d.active_voices(), 0);
    d.process(&mut l, &mut r);
    assert!(l[..72].iter().all(|&x| x == 0.0) && l[72] != 0.0);
}

#[test]
fn block_split_is_bit_identical() {
    let ev = [(0, 60u8, true), (300, 63, true), (301, 60, false), (777, 68, true), (4000, 50, true), (4100, 63, false)];
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
fn polyphony_is_sixteen_and_the_seventeenth_is_lost() {
    // sustain 1: a voice lives until its note-off + release
    let p = with(&with(&MID, 2, 1.0), 3, 0.05);
    let mut d = dev(&p);
    for i in 0..17u8 {
        d.note_on(40 + i, 0).unwrap();
    }
    let (mut l, mut r) = (vec![0.0f32; 256], vec![0.0f32; 256]);
    d.process(&mut l, &mut r);
    assert_eq!(d.active_voices(), 16);
    let notes = d.voice_notes();
    for i in 0..16u8 {
        assert_eq!(notes[i as usize], Some(40 + i), "lowest free voice, in call order");
    }
    // a note-off releases (for 0.05 s) and frees the slot; the lowest free voice is then used again
    d.note_off(43, 0).unwrap();
    let (mut l, mut r) = (vec![0.0f32; 3000], vec![0.0f32; 3000]);
    d.process(&mut l, &mut r);
    assert_eq!(d.active_voices(), 15);
    d.note_on(99, 0).unwrap();
    let (mut l, mut r) = (vec![0.0f32; 128], vec![0.0f32; 128]);
    d.process(&mut l, &mut r);
    assert_eq!(d.voice_notes()[3], Some(99), "the freed slot is reused");
    // the lost note never sounded: removing it changes nothing
    let ev: Vec<(u32, u8, bool)> = (0..16u8).map(|i| (0, 40 + i, true)).collect();
    let mut a = dev(&p);
    let ra = render(&mut a, 2000, &[2000], &ev);
    let mut b = dev(&p);
    let mut ev17 = ev.clone();
    ev17.push((0, 56, true));
    let rb = render(&mut b, 2000, &[2000], &ev17);
    assert_eq!(bits(&ra.0), bits(&rb.0));
}

#[test]
fn a_note_off_releases_every_voice_of_that_note_only() {
    let p = with(&with(&MID, 2, 1.0), 3, 0.01);
    let mut d = dev(&p);
    for n in [60u8, 60, 61] {
        d.note_on(n, 0).unwrap();
    }
    let (mut l, mut r) = (vec![0.0f32; 128], vec![0.0f32; 128]);
    d.process(&mut l, &mut r);
    assert_eq!(d.active_voices(), 3);
    d.note_off(60, 0).unwrap();
    let (mut l, mut r) = (vec![0.0f32; 2000], vec![0.0f32; 2000]);
    d.process(&mut l, &mut r);
    assert_eq!(d.voice_notes().iter().flatten().copied().collect::<Vec<_>>(), vec![61]);
}

// ---------------------------------------------------------------- envelope (closed forms)

fn adsr(a: f32, dd: f32, s: f32, r: f32) -> Adsr {
    let mut e = Adsr::idle();
    e.restart(a, dd, s, r);
    e
}

#[test]
fn envelope_attack_decay_sustain_follow_the_closed_forms() {
    let (a, dd, s) = (0.002f32, 0.004f32, 0.35f32);
    let mut e = adsr(a, dd, s, 0.01);
    let t = |k: usize| k as f64 * DT as f64;
    let mut k = 0usize;
    // attack: v = (t/A)·exp(2(t/A − 1)) until t >= A
    loop {
        k += 1;
        let v = e.tick() as f64;
        let x = t(k) / a as f64;
        if e.stage == 1 {
            let want = x * (2.0 * (x - 1.0)).exp();
            assert!((v - want).abs() < 2e-5, "attack tick {k}: {v} vs {want}");
        } else {
            assert_eq!(e.stage, 2);
            assert_eq!(v, 1.0, "the first sample past the attack is clamped to 1");
            assert!(e.level > 1.0, "the stored level is not clamped");
            break;
        }
        assert!(k < 500);
    }
    // decay: x = t/D, target = 1 + (S−1)(1 − e^(−2x)), v = target + (S − target)·x, from t = DT after the switch
    let mut j = 0usize;
    loop {
        j += 1;
        let v = e.tick() as f64;
        let x = t(j) / dd as f64;
        if e.stage == 2 {
            let tg = 1.0 + (s as f64 - 1.0) * (1.0 - (-2.0 * x).exp());
            let want = tg + (s as f64 - tg) * x;
            assert!((v - want).abs() < 2e-5, "decay tick {j}: {v} vs {want}");
        } else {
            assert_eq!(e.stage, 3);
            break;
        }
        assert!(j < 500);
    }
    for _ in 0..50 {
        assert_eq!(e.tick(), s, "sustain holds exactly S");
    }
}

#[test]
fn envelope_release_starts_from_the_stored_level() {
    // release from sustain: v = S·(1 − x)·e^(−2x), x = t/R, ends at t >= R with 0 and stage 0
    let (s, r) = (0.6f32, 0.003f32);
    let mut e = adsr(0.0, 0.0, s, r);
    for _ in 0..5 {
        e.tick();
    }
    assert_eq!(e.stage, 3);
    e.release();
    assert_eq!(e.stage, 4);
    let mut k = 0usize;
    loop {
        k += 1;
        let v = e.tick() as f64;
        let x = k as f64 * DT as f64 / r as f64;
        if e.stage == 4 {
            let want = s as f64 * (1.0 - x) * (-2.0 * x).exp();
            assert!((v - want).abs() < 2e-5, "release tick {k}: {v} vs {want}");
        } else {
            assert_eq!((e.stage, v, e.level), (0, 0.0, 0.0));
            break;
        }
        assert!(k < 1000);
    }
    // a release during the attack starts from the (unclamped) attack value reached so far
    let mut e = adsr(0.01, 0.1, 0.5, 0.05);
    let mut last = 0.0;
    for _ in 0..100 {
        last = e.tick();
    }
    let lvl = e.level;
    assert_eq!(lvl, last);
    e.release();
    let first = e.tick();
    let x = DT / 0.05;
    assert!((first - lvl * (1.0 - x) * (-2.0 * x).exp()).abs() < 1e-5 * lvl.max(1.0));
}

#[test]
fn envelope_zero_times_take_the_shortcuts() {
    // attack 0: the first tick returns 1 and enters the decay; decay 0: the next tick returns S and sustains
    let mut e = adsr(0.0, 0.0, 0.25, 0.0);
    assert_eq!(e.tick(), 1.0);
    assert_eq!(e.stage, 2);
    assert_eq!(e.tick(), 0.25);
    assert_eq!(e.stage, 3);
    assert_eq!(e.tick(), 0.25);
    // release 0: stage 0 and 0 at once, the stored level is left as it was (the original does not clear it)
    e.release();
    assert_eq!(e.tick(), 0.0);
    assert_eq!(e.stage, 0);
    assert_eq!(e.level, 0.25);
    // a finished envelope stays 0
    assert_eq!(e.tick(), 0.0);
    // a stale level survives into the next note's early release (no reset at note-on)
    e.restart(0.5, 0.5, 0.5, 0.01);
    e.release();
    let v = e.tick();
    assert!(v > 0.2 && v < 0.25, "released from the stale level 0.25: {v}");
}

// ---------------------------------------------------------------- voice behaviour

/// Single partial: P4 = P6 = 0, P5 = 0 (ratio 1), P8 = 0 (no detune), flat envelope, no pan.
fn single(seed: f32) -> [f32; PARAMS] {
    [0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, seed, 0.0, 0.5, 0.5, 1.0, 0.0]
}

fn crossings(x: &[f32]) -> usize {
    x.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count()
}

#[test]
fn the_note_sets_the_pitch_in_semitones() {
    // f = 440·2^((note − 69)/12); high notes only: the oscillator is set up through a 512-node table sine whose
    // error moves low frequencies (documented, reproduced)
    let n = 8000;
    let mut cross = [0usize; 3];
    for (k, note) in [84u8, 96, 108].into_iter().enumerate() {
        let (l, r) = {
            let mut d = dev(&single(0.3));
            render(&mut d, n, &[n], &[(0, note, true)])
        };
        assert!(peak(&l) > 0.05 && l == r, "pan 0: both channels equal, audible");
        cross[k] = crossings(&l);
        let f = 440.0 * 2f64.powf((note as f64 - 69.0) / 12.0);
        let want = 2.0 * f * n as f64 / 44100.0;
        assert!((cross[k] as f64 - want).abs() <= 0.01 * want + 2.0, "note {note}: {} crossings vs {want}", cross[k]);
    }
    assert!(cross[1] > cross[0] && cross[2] > cross[1]);
}

#[test]
fn parameters_are_latched_at_note_on() {
    // Notes: 60 (held 1400 samples, then released), 64 at 2500 (held 1900 samples). A parameter written at sample 1000
    // cannot touch the first note (not even its release, P3) but changes the second one.
    let ev = [(0u32, 60u8, true), (1400, 60, false), (2500, 64, true), (4400, 64, false)];
    let n = 7000;
    let run = |change: Option<(usize, f32)>| {
        let mut d = dev(&MID);
        let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
        let push = |d: &mut Id21, from: usize, to: usize, l: &mut [f32], r: &mut [f32]| {
            for &(t, note, on) in &ev {
                let t = t as usize;
                if t >= from && t < to {
                    let off = (t - from) as u32;
                    if on { d.note_on(note, off).unwrap() } else { d.note_off(note, off).unwrap() }
                }
            }
            d.process(&mut l[from..to], &mut r[from..to]);
        };
        push(&mut d, 0, 1000, &mut l, &mut r);
        if let Some((i, v)) = change {
            d.set_param(i, v).unwrap();
        }
        push(&mut d, 1000, n, &mut l, &mut r);
        (l, r)
    };
    let base = run(None);
    assert!(peak(&base.0[2500..4400]) > 0.0);
    for i in 0..PARAMS {
        let old = MID[i];
        let new = if old < 0.5 { old + 0.4 } else { old - 0.4 };
        let got = run(Some((i, new)));
        assert_eq!(bits(&got.0[..2500]), bits(&base.0[..2500]), "P{i} acted on a note that was already playing");
        assert_eq!(bits(&got.1[..2500]), bits(&base.1[..2500]), "P{i}");
        assert_ne!(bits(&got.0[2500..]), bits(&base.0[2500..]), "P{i} did not reach the next note");
    }
}

#[test]
fn p7_is_a_seed_down_to_the_last_bit() {
    let a = 0.3f32;
    let b = f32::from_bits(a.to_bits() + 1);
    let one = |p: f32| {
        let mut d = dev(&with(&MID, 7, p));
        render(&mut d, 1500, &[1500], &[(0, 60, true)])
    };
    let (x, y) = (one(a), one(b));
    assert_ne!(bits(&x.0), bits(&y.0), "a one-ulp different P7 draws different partials");
    assert!(max_diff(&x.0, &y.0) > 1e-3 * peak(&x.0) as f64);
    assert_eq!(bits(&x.0), bits(&one(a).0), "deterministic");
}

#[test]
fn pan_zero_is_mono_and_the_spread_is_symmetric() {
    let mut d = dev(&with(&MID, 12, 0.0));
    let (l, r) = render(&mut d, 2000, &[2000], &[(0, 60, true)]);
    assert_eq!(bits(&l), bits(&r));
    let mut d = dev(&with(&MID, 12, 1.0));
    let (l, r) = render(&mut d, 2000, &[2000], &[(0, 60, true)]);
    assert_ne!(bits(&l), bits(&r));
    // gain 0 is silent
    let mut d = dev(&with(&MID, 11, 0.0));
    let (l, r) = render(&mut d, 500, &[500], &[(0, 60, true)]);
    assert!(l.iter().chain(&r).all(|&x| x == 0.0));
}

#[test]
fn a_released_voice_is_freed_when_its_release_ends() {
    // flat envelope S = 1, release 0.05 s = 2205 ticks: the voice sounds until the release's last tick and no longer
    let p = with(&with(&with(&MID, 0, 0.0), 1, 0.0), 2, 1.0);
    let p = with(&p, 3, 0.05);
    let mut d = dev(&p);
    d.note_on(60, 0).unwrap();
    let (mut l, mut r) = (vec![0.0f32; 4000], vec![0.0f32; 4000]);
    d.process(&mut l, &mut r);
    assert_eq!(d.active_voices(), 1, "sustains for ever");
    d.note_off(60, 0).unwrap();
    // the envelope's f32 clock reaches 0.05 s after ~2205 ticks
    let (mut l, mut r) = (vec![0.0f32; 2100], vec![0.0f32; 2100]);
    d.process(&mut l, &mut r);
    assert_eq!(d.active_voices(), 1, "still releasing");
    let (mut l, mut r) = (vec![0.0f32; 300], vec![0.0f32; 300]);
    d.process(&mut l, &mut r);
    assert_eq!(d.active_voices(), 0);
    assert!(l[250..].iter().chain(&r[250..]).all(|&x| x == 0.0));
}

#[test]
fn a_full_load_stays_finite() {
    // 16 simultaneous notes of the largest accepted partial count, extreme settings, a second of audio
    let p = [1.0, 1.0, 1.0, 1.0, 0.3105, 1.0, 0.1505, 0.77, 1.0, 1.0, 0.0, 1.0, 1.0];
    let mut d = dev(&p);
    let ev: Vec<(u32, u8, bool)> = (0..16u8).map(|i| (0, 8 * i, true)).chain([(300, 0, false)]).collect();
    let (l, r) = render(&mut d, 44100, &[128], &ev);
    assert!(l.iter().chain(&r).all(|x| x.is_finite()));
    assert!(peak(&l) > 0.0);
    assert_eq!(d.active_voices(), 16, "note 0 is released at 300 for 1 s, so it is still active at 44100");
}

// ---------------------------------------------------------------- independent f64 model

const TAU: f64 = core::f64::consts::TAU;

/// The 512-node interpolated table sine of the oscillator set-up, from exact nodes.
fn tsin(x: f64) -> f64 {
    let c = x / TAU;
    let pos = (c - c.floor()) * 512.0;
    let i = pos.floor();
    let w = pos - i;
    let node = |k: f64| (k * TAU / 512.0).sin();
    node(i) + (node(i + 1.0) - node(i)) * w
}

struct Part {
    c1: f64,
    c2: f64,
    y1: f64,
    y2: f64,
    gl: f64,
    gr: f64,
}

/// The note-on randomisation, from the documented formulas (f64, exact functions). `u(h)` = bits·2⁻³¹ − 1.
fn model_partials(p: &[f32; PARAMS], note: u8) -> Vec<Part> {
    let seed = p[7].to_bits();
    let n = (p[4] * 100.0) as u32;
    let m = (p[6] * 100.0) as u32;
    let count = ((n + 1) * (m + 1)) as f64;
    let spread = ((p[5] * 100.0) as i32) as f64 / (12.0 * 4294967296.0);
    let snap = 2.0 * p[10] as f64 - 1.0;
    let detune = 20.0 * (p[8] as f64).powi(3);
    let u = |h: u32| h as f64 / 2147483648.0 - 1.0;
    let r = (-4.5351474e-8f64).exp();
    let mut out = Vec::new();
    for j in 0..=m {
        let jo = j.wrapping_mul(0x3eed_dc);
        let ratio = (hash(seed.wrapping_add(0xfe_d44b).wrapping_add(jo)) as f64 * spread).exp2();
        let near = (ratio + 0.5).floor();
        let q = ratio + (near - ratio) * snap;
        // amplitude q^(5·P9 − 4) (the spectral slope), semitone offset 12·log2 q
        let amp = q.powf(5.0 * p[9] as f64 - 4.0);
        let semis = 12.0 * q.log2() + note as f64;
        let b = u(hash(seed.wrapping_add(0xfe_d833).wrapping_add(jo))) * amp * 10.0;
        let g = p[11] as f64 * b / count;
        for i in 0..=n {
            let io = j.wrapping_mul(0x3e_eddd);
            let da = u(hash(seed.wrapping_add(0xfe_dc1b).wrapping_add(io).wrapping_add(0x14 * i)));
            let f = 440.0 * ((da * detune + semis) / 12.0 - 5.75).exp2();
            let w = f * TAU / 44100.0;
            let ph = hash(seed.wrapping_add(0xfe_e003).wrapping_add(io).wrapping_add(0x28 * i)) as f64 * (TAU / 1073741824.0) - 4.0 * core::f64::consts::PI;
            let pan = u(hash(seed.wrapping_add(0xfe_e3eb).wrapping_add(io).wrapping_add(0x28 * i))) * p[12] as f64;
            out.push(Part {
                c1: 2.0 * r * tsin(core::f64::consts::FRAC_PI_2 + w),
                c2: r * r,
                y1: tsin(ph),
                y2: tsin(ph - w) / r,
                gl: (1.0 + pan) * g,
                gr: (1.0 - pan) * g,
            });
        }
    }
    assert_eq!(out.len() as f64, count);
    out
}

/// Envelope in f64: one tick per sample, same stage machine as the documented closed forms.
struct MEnv {
    t: f64,
    level: f64,
    st: u8,
    a: f64,
    d: f64,
    s: f64,
    r: f64,
}

impl MEnv {
    fn tick(&mut self) -> f64 {
        let dt = 1.0 / 44100.0;
        self.t += dt;
        let t = self.t;
        let v = match self.st {
            1 => {
                let x = t / self.a;
                let v = x * (2.0 * (x - 1.0)).exp();
                if t >= self.a {
                    self.t = 0.0;
                    self.st = 2;
                }
                self.level = v;
                v
            }
            2 => {
                let x = t / self.d;
                let tg = 1.0 + (self.s - 1.0) * (1.0 - (-2.0 * x).exp());
                let v = tg + (self.s - tg) * x;
                if t >= self.d {
                    self.t = 0.0;
                    self.st = 3;
                }
                self.level = v;
                v
            }
            3 => {
                self.level = self.s;
                self.s
            }
            4 => {
                let x = t / self.r;
                let v = (1.0 - x) * (-2.0 * x).exp() * self.level;
                if t >= self.r {
                    self.st = 0;
                    self.level = 0.0;
                    return 0.0;
                }
                v
            }
            _ => 0.0,
        };
        v.clamp(0.0, 1.0)
    }
}

/// Whole-voice model: notes `(start, note, off)`, `n` samples.
fn model(p: &[f32; PARAMS], notes: &[(usize, u8, Option<usize>)], n: usize) -> Vec<[f64; 2]> {
    let mut out = vec![[0.0f64; 2]; n];
    for &(start, note, off) in notes {
        let mut parts = model_partials(p, note);
        let mut e = MEnv { t: 0.0, level: 0.0, st: 1, a: p[0] as f64, d: p[1] as f64, s: p[2] as f64, r: p[3] as f64 };
        for (k, o) in out.iter_mut().enumerate().skip(start) {
            if off == Some(k) {
                e.st = 4;
                e.t = 0.0;
            }
            let (mut l, mut r) = (0.0, 0.0);
            for q in parts.iter() {
                l += q.y1 * q.gl;
                r += q.y1 * q.gr;
            }
            let env = e.tick();
            o[0] += env * l;
            o[1] += env * r;
            for q in parts.iter_mut() {
                let ny = q.c1 * q.y1 - q.c2 * q.y2;
                q.y2 = q.y1;
                q.y1 = ny;
            }
            if e.st == 0 {
                break;
            }
        }
    }
    out
}

/// `(max abs error, peak)` of both channels against the model.
fn model_error(p: &[f32; PARAMS], notes: &[(usize, u8, Option<usize>)], n: usize) -> (f64, f64) {
    let mut ev = Vec::new();
    for &(s, note, off) in notes {
        ev.push((s as u32, note, true));
        if let Some(o) = off {
            ev.push((o as u32, note, false));
        }
    }
    let mut d = dev(p);
    let (l, r) = render(&mut d, n, &[n], &ev);
    let want = model(p, notes, n);
    let mut err = 0.0f64;
    let mut pk = 0.0f64;
    for i in 0..n {
        err = err.max((l[i] as f64 - want[i][0]).abs()).max((r[i] as f64 - want[i][1]).abs());
        pk = pk.max(want[i][0].abs()).max(want[i][1].abs());
    }
    (err, pk)
}

const SCENARIO: [(usize, u8, Option<usize>); 2] = [(0, 60, Some(1200)), (2000, 72, None)];
const N_MODEL: usize = 3000;
/// Bound relative to the peak (measured worst ~ see `matches_the_independent_f64_model`).
const MODEL_REL: f64 = 2e-3;

#[test]
fn matches_the_independent_f64_model() {
    for (name, p) in [("MID", MID), ("SONG5", SONG5), ("SONG6", SONG6), ("single", single(0.3))] {
        let (err, pk) = model_error(&p, &SCENARIO, N_MODEL);
        eprintln!("{name}: max err {err:.3e} peak {pk:.3}");
        assert!(pk > 0.01, "{name}: the model is silent");
        assert!(err <= MODEL_REL * pk, "{name}: {err} vs peak {pk}");
    }
}

#[test]
fn negative_controls_leave_the_model_tolerance() {
    for i in 0..PARAMS {
        let mut bad = MID;
        bad[i] = (MID[i] + 0.02).min(1.0);
        let mut d = dev(&bad);
        let notes = SCENARIO;
        let mut ev = Vec::new();
        for &(s, note, off) in &notes {
            ev.push((s as u32, note, true));
            if let Some(o) = off {
                ev.push((o as u32, note, false));
            }
        }
        let (l, _) = render(&mut d, N_MODEL, &[N_MODEL], &ev);
        let want = model(&MID, &notes, N_MODEL);
        let pk = want.iter().fold(0.0f64, |m, v| m.max(v[0].abs()));
        let err = l.iter().zip(&want).fold(0.0f64, |m, (a, b)| m.max((*a as f64 - b[0]).abs()));
        assert!(err > 3.0 * MODEL_REL * pk, "P{i} nudged by 0.02 stays inside the model tolerance: {err} vs {pk}");
    }
    // a wrong model detail is caught too: the channels swapped (the pan law is not symmetric per partial)
    let mut d = dev(&SONG5);
    let (l, r) = render(&mut d, N_MODEL, &[N_MODEL], &[(0, 60, true), (1200, 60, false), (2000, 72, true)]);
    let want = model(&SONG5, &SCENARIO, N_MODEL);
    let pk = want.iter().fold(0.0f64, |m, v| m.max(v[0].abs()).max(v[1].abs()));
    let err = (0..N_MODEL).fold(0.0f64, |m, i| m.max((l[i] as f64 - want[i][1]).abs()).max((r[i] as f64 - want[i][0]).abs()));
    assert!(err > 3.0 * MODEL_REL * pk, "swapped channels stay inside the tolerance: {err} vs {pk}");
}
