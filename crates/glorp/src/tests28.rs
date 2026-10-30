//! Synthetic-only tests for ID 28 (virtual-analog unison oscillator bank through a 4-pole ladder filter, on the shared
//! 16-voice scheduler). No song data, recorded tap or proprietary vector is used: the expectations are closed forms,
//! structural properties of the scheduler, and an independent model (ideal sine, exact `tan` / `exp`, f64 filter)
//! written from the behaviour described in research/glorp-devices.md §ID 28.
use crate::id21::Adsr;
use crate::id28::{
    check, increment, note_freq, pan_gains, partial_freq, unison_count, Error, Id28, MAX_NOTE, MAX_UNISON, PARAMS, QUEUE,
    VOICES,
};

/// Made-up musical settings (not the song's): saw, 4 unison voices, mild detune / spread, 5 ms attack, open filter.
const BASE: [f32; PARAMS] = [
    0.75, 0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, // P0..P8 oscillator
    0.002, 0.05, 0.7, 0.05, // P9..P12 amplitude ADSR
    0.8, 0.1, 0.2, 0.5, 0.0, // P13 level, P14 voices, P15 detune, P16 spread, P17 mode
    0.002, 0.05, 0.6, 0.05, // P18..P21 filter ADSR
    0.6, 0.2, 0.5, 0.0, // P22 cutoff, P23 resonance, P24 envelope amount, P25 key tracking
];

fn dev(p: &[f32; PARAMS]) -> Id28 {
    Id28::new(p).unwrap()
}

fn with(i: usize, v: f32) -> [f32; PARAMS] {
    let mut p = BASE;
    p[i] = v;
    p
}

/// Events are `(absolute sample, note, velocity, on)`; `chunks` are cycled to cut the render.
fn render(d: &mut Id28, n: usize, chunks: &[usize], events: &[(u32, u8, u8, bool)]) -> (Vec<f32>, Vec<f32>) {
    let (mut l, mut r) = (vec![9.0f32; n], vec![9.0f32; n]);
    let (mut s, mut k) = (0usize, 0usize);
    while s < n {
        let len = chunks[k % chunks.len()].min(n - s);
        k += 1;
        for &(t, note, vel, on) in events {
            let t = t as usize;
            if t >= s && t < s + len {
                let off = (t - s) as u32;
                if on { d.note_on(note, vel, off).unwrap() } else { d.note_off(note, off).unwrap() }
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

fn rms_diff(a: &[f32], b: &[f32]) -> f64 {
    let n = a.len().min(b.len());
    (a[..n].iter().zip(&b[..n]).map(|(x, y)| ((x - y) as f64).powi(2)).sum::<f64>() / n as f64).sqrt()
}

fn rms(a: &[f32]) -> f64 {
    (a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / a.len() as f64).sqrt()
}

struct Xs(u64);
impl Xs {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

// ---------------------------------------------------------------- API contract

#[test]
fn rejects_unsupported_calls_and_stays_unchanged() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 1.001] {
        assert_eq!(Id28::new(&with(3, bad)).err(), Some(Error::Range), "{bad}");
        assert_eq!(check(&with(25, bad)), Err(Error::Range), "{bad}");
    }
    let mut d = dev(&BASE);
    let mut e = dev(&BASE);
    assert_eq!(d.set_param(PARAMS, 0.5), Err(Error::Index));
    for bad in [f32::NAN, 1.5, -1.0, f32::INFINITY] {
        assert_eq!(d.set_param(22, bad), Err(Error::Range));
    }
    assert_eq!(d.note_on(MAX_NOTE + 1, 100, 0), Err(Error::Note));
    assert_eq!(d.note_off(200, 0), Err(Error::Note));
    assert_eq!(d.note_on(60, 100, i32::MAX as u32 + 1), Err(Error::Offset));
    assert_eq!(d.note_off(60, u32::MAX), Err(Error::Offset));
    // refused calls changed nothing: both devices render the same note bit for bit
    let ev = [(0, 60, 127, true)];
    assert_eq!(bits(&render(&mut d, 3000, &[128], &ev).0), bits(&render(&mut e, 3000, &[128], &ev).0));
}

#[test]
fn the_whole_unit_cube_is_accepted_and_tested_vectors_render_finite() {
    // nothing in [0, 1]^26 is refused; finiteness is tested on the corners, the midpoint and 40 sampled vectors (coverage, not proof)
    assert!(check(&[0.0; PARAMS]).is_ok());
    assert!(check(&[1.0; PARAMS]).is_ok());
    let mut rng = Xs(0x28);
    let mut vectors = vec![[0.0f32; PARAMS], [1.0; PARAMS], [0.5; PARAMS]];
    for _ in 0..40 {
        let mut p = [0.0f32; PARAMS];
        for v in p.iter_mut() {
            *v = [0.0, 1.0, rng.next() as f32][(rng.next() * 3.0) as usize % 3];
        }
        vectors.push(p);
    }
    for p in vectors {
        assert!(check(&p).is_ok());
        let mut d = dev(&p);
        let ev = [(0, 0, 255, true), (0, 127, 255, true), (0, 69, 1, true), (4000, 0, 0, false)];
        let (l, r) = render(&mut d, 6000, &[128], &ev);
        assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() < 1.0e4), "{p:?}");
    }
}

// ---------------------------------------------------------------- closed forms

#[test]
fn the_unison_count_is_trunc_31_p14_plus_one() {
    for (p, n) in [(0.0, 1), (0.032, 1), (0.0322, 1), (0.0323, 2), (0.0646, 3), (0.0645, 2), (0.5, 16), (0.9677, 30), (0.9678, 31), (0.97, 31), (0.9999, 31), (1.0, 32)] {
        assert_eq!(unison_count(p), n, "{p}");
    }
    assert_eq!(MAX_UNISON, 32);
}

#[test]
fn the_pan_law_is_linear_and_constant_sum() {
    let g = pan_gains(5, 1.0);
    let want = [(0.0, 2.0), (0.5, 1.5), (1.0, 1.0), (1.5, 0.5), (2.0, 0.0)];
    for i in 0..5 {
        assert!((g[i][0] - want[i].0).abs() < 1e-6 && (g[i][1] - want[i].1).abs() < 1e-6, "{i} {:?}", g[i]);
    }
    // s = 0: every voice centred; the sum of both gains is 2 for every s and N
    for n in [2usize, 7, 32] {
        for s in [0.0f32, 0.3, 0.77, 1.0] {
            let g = pan_gains(n, s);
            for (i, gi) in g.iter().take(n).enumerate() {
                assert!((gi[0] + gi[1] - 2.0).abs() < 1e-5, "{n} {s} {i}");
                if s == 0.0 {
                    assert_eq!(*gi, [1.0, 1.0]);
                }
            }
            // first voice (1 - s, 1 + s), last (1 + s, 1 - s)
            assert!((g[0][0] - (1.0 - s)).abs() < 1e-6 && (g[0][1] - (1.0 + s)).abs() < 1e-6);
            assert!((g[n - 1][0] - (1.0 + s)).abs() < 1e-5 && (g[n - 1][1] - (1.0 - s)).abs() < 1e-5);
        }
    }
    // a single voice ignores the spread
    assert_eq!(pan_gains(1, 1.0)[0], [1.0, 1.0]);
}

#[test]
fn the_spread_modes_are_the_documented_frequency_ratios() {
    let f0 = note_freq(57);
    // bucket k covers [ (k+1)/8.99, (k+2)/8.99 ): P17 at the middle of each (m = -1 is the default)
    let mid = |m: i32| (m as f32 + 1.5) / 8.99;
    let ratio = |m: i32, i: usize| partial_freq(mid(m), f0, 57, i) / f0;
    let near = |a: f32, b: f32| (a - b).abs() <= 3e-6 * b.abs().max(1.0);
    for i in 0..8usize {
        let fi = i as f32;
        assert!(near(ratio(-1, i), 1.0));
        assert!(near(ratio(0, i), if i % 2 == 0 { 1.0 } else { 2.0 }), "oct up {i}");
        let fifth = 2.0f32.powf(7.0 / 12.0);
        assert!(near(ratio(1, i), [1.0, fifth, 2.0][i % 3]), "fifth {i} {}", ratio(1, i));
        assert!(near(ratio(2, i), if i % 2 == 0 { 1.0 } else { 0.5 }), "oct down {i}");
        assert!(near(ratio(3, i), fi + 1.0), "harmonic {i}");
        assert!(near(ratio(4, i), 1.0 / (fi + 1.0)), "subharmonic {i}");
        assert!(near(ratio(5, i), 1.0 / (1.0 + fi / 2.0)), "{i}");
        assert!(near(ratio(6, i), 1.0 + fi / 2.0), "{i}");
        assert!(near(ratio(7, i), (2.0 * fi).max(1.0)), "{i}");
    }
    // bucket edges (f32-exact lower bounds of the table in the research note): just below / at / above
    for (edge, below, above) in [
        (0.11123471f32, -1, 0),
        (0.22246942, 0, 1),
        (0.33370411, 1, 2),
        (0.44493884, 2, 3),
        (0.55617356, 3, 4),
        (0.66740823, 4, 5),
        (0.77864295, 5, 6),
        (0.88987768, 6, 7),
    ] {
        let at = |p: f32| partial_freq(p, f0, 57, 1) / f0;
        let lo = f32::from_bits(edge.to_bits() - 2);
        let hi = f32::from_bits(edge.to_bits() + 2);
        let (rl, rh) = (at(lo), at(hi));
        assert!(near(rl, ratio(below, 1)) && near(rh, ratio(above, 1)), "edge {edge}: {rl} {rh}");
    }
    // P17 = 1 is still the last mode
    assert!(near(partial_freq(1.0, f0, 57, 3) / f0, 6.0));
}

#[test]
fn detune_is_linear_in_semitones_and_the_single_voice_is_detuned_down() {
    let f0 = note_freq(69);
    assert!((f0 - 440.0).abs() < 1e-3);
    let hz = |inc: f32| inc as f64 * 44100.0;
    // N = 1: the lone voice sits P15 semitones DOWN (415.305 Hz for one semitone)
    assert!((hz(increment(f0, 0, 1, 1.0)) - 415.305).abs() < 0.01, "{}", hz(increment(f0, 0, 1, 1.0)));
    assert!((hz(increment(f0, 0, 1, 0.0)) - 440.0).abs() < 1e-3);
    // N = 3: -P15, 0, +P15 semitones
    for (i, semis) in [(0usize, -1.0f64), (1, 0.0), (2, 1.0)] {
        let want = 440.0 * 2f64.powf(semis / 12.0);
        assert!((hz(increment(f0, i, 3, 1.0)) - want).abs() < 0.01, "{i}");
    }
    // N = 2: the ends are -P15 and +P15 (D = 1)
    assert!((hz(increment(f0, 1, 2, 0.5)) - 440.0 * 2f64.powf(0.5 / 12.0)).abs() < 0.01);
    // detune is applied on top of the ratio mode: 2·f0 detuned by +P15
    assert!((hz(increment(2.0 * f0, 2, 3, 1.0)) - 880.0 * 2f64.powf(1.0 / 12.0)).abs() < 0.02);
}

// ---------------------------------------------------------------- scheduler

#[test]
fn queue_is_256_slots_and_note_offs_take_slots() {
    let mut d = dev(&BASE);
    for _ in 0..QUEUE / 2 {
        d.note_on(60, 100, 0).unwrap();
        d.note_off(61, 0).unwrap();
    }
    assert_eq!(d.note_on(60, 100, 0), Err(Error::QueueFull));
    assert_eq!(d.note_off(60, 0), Err(Error::QueueFull));
    // firing the events frees the slots again
    let (mut l, mut r) = (vec![0.0; 128], vec![0.0; 128]);
    d.process(&mut l, &mut r);
    assert!(d.note_on(62, 100, 0).is_ok());
}

#[test]
fn silence_before_the_note_and_onset_at_the_offset() {
    let mut d = dev(&BASE);
    let (l, r) = render(&mut d, 2000, &[2000], &[(300, 60, 127, true)]);
    assert!(l[..300].iter().chain(&r[..300]).all(|&x| x == 0.0));
    assert!(peak(&l[300..2000]) > 1e-3 && peak(&r[300..2000]) > 1e-3);
    for off in [0u32, 1, 37, 127] {
        let mut d = dev(&BASE);
        let (mut l, mut r) = (vec![0.0f32; 128], vec![0.0f32; 128]);
        d.note_on(60, 127, off).unwrap();
        d.process(&mut l, &mut r);
        assert!(l[..off as usize].iter().all(|&x| x == 0.0), "{off}");
    }
}

#[test]
fn the_output_does_not_depend_on_how_the_render_is_cut() {
    // no voice may expire inside a block here (an expiring voice keeps rendering the rest of its sub-block, see
    // `a_voice_renders_the_rest_of_its_sub_block_after_expiring`): release 0.05 s = 2205 samples, 2500 rendered
    let ev = [
        (5, 60, 127, true),
        (90, 64, 80, true),
        (700, 60, 0, false),
        (1500, 67, 1, true),
        (2400, 64, 0, false),
    ];
    let one = {
        let mut d = dev(&BASE);
        render(&mut d, 2500, &[2500], &ev)
    };
    for chunks in [&[128usize][..], &[1], &[7, 129, 3], &[1000, 1], &[63, 65]] {
        let mut d = dev(&BASE);
        let got = render(&mut d, 2500, chunks, &ev);
        assert_eq!(bits(&got.0), bits(&one.0), "{chunks:?}");
        assert_eq!(bits(&got.1), bits(&one.1), "{chunks:?}");
    }
}

#[test]
fn sixteen_voices_and_the_seventeenth_note_is_lost() {
    let mut p = BASE;
    p[12] = 1.0; // long release: a released note keeps its slot
    p[14] = 0.0; // one oscillator per voice keeps the test cheap
    let mut d = dev(&p);
    let ev: Vec<_> = (0..17).map(|i| (0u32, 40 + i as u8, 127u8, true)).collect();
    render(&mut d, 256, &[256], &ev);
    assert_eq!(d.active_voices(), VOICES);
    let notes = d.voice_notes();
    assert!((0..VOICES).all(|i| notes[i] == Some(40 + i as u8)));
    // releasing note 41 frees nothing at once (release 1 s) but the slot is reused by the lowest free voice only when free
    render(&mut d, 64, &[64], &[(0, 41, 0, false)]);
    assert_eq!(d.active_voices(), VOICES);
}

#[test]
fn a_finished_release_frees_the_voice_and_note_off_only_hits_its_note() {
    let mut p = BASE;
    p[12] = 0.0; // release 0: gone at the next sample
    p[14] = 0.0;
    let mut d = dev(&p);
    render(&mut d, 512, &[128], &[(0, 60, 127, true), (0, 62, 127, true)]);
    assert_eq!(d.voice_notes()[..2], [Some(60), Some(62)]);
    render(&mut d, 256, &[128], &[(0, 99, 0, false)]); // nobody plays 99
    assert_eq!(d.active_voices(), 2);
    render(&mut d, 256, &[128], &[(0, 60, 0, false)]);
    assert_eq!(d.voice_notes()[..2], [None, Some(62)]);
    // the freed slot (lowest free) takes the next note
    render(&mut d, 256, &[128], &[(0, 70, 127, true)]);
    assert_eq!(d.voice_notes()[..2], [Some(70), Some(62)]);
}

#[test]
fn a_voice_renders_the_rest_of_its_sub_block_after_expiring() {
    // release 0: the envelope reaches stage 0 at the first sample after the note-off. The voice still renders the rest of
    // its sub-block (the filter rings out, nothing new is excited), and is not rendered in the next one.
    let mut p = BASE;
    p[12] = 0.0;
    p[14] = 0.0;
    p[23] = 1.0; // resonant: a long ring
    p[22] = 0.3;
    let ev = [(0, 60, 127, true), (500, 60, 0, false)];
    let one = render(&mut dev(&p), 1024, &[1024], &ev).0;
    let cut = render(&mut dev(&p), 1024, &[128], &ev).0;
    // up to the end of the 128-sample block that holds the note-off (block 3 = 384..512) both agree
    assert_eq!(bits(&one[..512]), bits(&cut[..512]));
    // the single block keeps the ring-out; the cut render stops at the block end (the next block has no voice)
    assert!(cut[512..].iter().all(|&x| x == 0.0));
    assert!(peak(&one[512..640]) > 0.0 && one[512..].iter().any(|&x| x != 0.0));
    let mut d = dev(&p);
    render(&mut d, 1024, &[128], &ev);
    assert_eq!(d.active_voices(), 0);
}

// ---------------------------------------------------------------- parameter roles

/// Parameters read at note-on (P15 detune, P17 mode and the filter P22..P25 are live).
const LATCHED: [usize; 20] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 18, 19, 20, 21];
const LIVE: [usize; 6] = [15, 17, 22, 23, 24, 25];

#[test]
fn live_parameters_act_on_a_sounding_note_and_latched_ones_wait_for_the_next() {
    let mut base = BASE;
    base[14] = 0.3; // several voices so that P15 / P17 are audible
    base[15] = 0.2;
    base[17] = 0.3;
    base[24] = 0.5;
    base[11] = 1.0; // sustain 1: the note keeps sounding
    base[21] = 0.0;
    let ev = [(0, 52, 127, true)];
    let straight = {
        let mut d = dev(&base);
        render(&mut d, 6000, &[128], &ev).0
    };
    for i in LIVE.iter().chain(LATCHED.iter()) {
        let mut d = dev(&base);
        let mut l = vec![0.0f32; 6000];
        let mut r = vec![0.0f32; 6000];
        d.note_on(52, 127, 0).unwrap();
        d.process(&mut l[..2048], &mut r[..2048]);
        let new = if base[*i] > 0.5 { 0.05 } else { 0.95 };
        d.set_param(*i, new).unwrap();
        d.process(&mut l[2048..], &mut r[2048..]);
        let live = LIVE.contains(i);
        assert_eq!(bits(&l[..2048]), bits(&straight[..2048]));
        let same = bits(&l[2048..]) == bits(&straight[2048..]);
        assert_eq!(!same, live, "P{i}: live = {live}");
    }
    // a latched parameter reaches the NEXT note: P14 (voice count), P16 (spread), P13 (level), P0 (waveform)
    for i in [0usize, 13, 14, 16] {
        let mut a = dev(&base);
        let mut b = dev(&base);
        b.set_param(i, if base[i] > 0.5 { 0.05 } else { 0.95 }).unwrap();
        let ev = [(0, 52, 127, true), (3000, 52, 0, false)];
        let ra = render(&mut a, 6000, &[128], &ev);
        let rb = render(&mut b, 6000, &[128], &ev);
        assert_ne!(bits(&ra.0), bits(&rb.0), "P{i}");
    }
}

#[test]
fn every_parameter_changes_the_sound() {
    // a sine-fold base where every parameter has a role: wave 0, stretch (so t > 1 for the gate), fold gain, triangle
    // fold off, shaper on, drive, power curve
    let mut base = BASE;
    base[0] = 0.2;
    base[2] = 0.5;
    base[3] = 0.5;
    base[4] = 0.2;
    base[5] = 0.4;
    base[6] = 0.4;
    base[7] = 0.5;
    base[8] = 0.2;
    base[14] = 0.3;
    base[15] = 0.3;
    base[16] = 0.6;
    base[17] = 0.4;
    base[25] = 0.5;
    base[24] = 0.55; // a small envelope amount: P18..P21 audible, cutoff far from the clamp
    let ev = [(0, 52, 100, true), (2500, 52, 0, false)];
    let reference = {
        let mut d = dev(&base);
        render(&mut d, 6000, &[128], &ev)
    };
    for i in 0..PARAMS {
        let mut p = base;
        p[i] = match i {
            0 => 0.8,  // wave select (the other side of the 0.5 threshold)
            4 => 0.8,  // triangle-fold flag
            8 => 0.8,  // stretch gate
            _ => (base[i] + 0.3).min(1.0),
        };
        let mut d = dev(&p);
        let got = render(&mut d, 6000, &[128], &ev);
        assert_ne!(bits(&got.0), bits(&reference.0), "P{i} does not change the left channel");
    }
}

#[test]
fn velocity_scales_the_waveform_and_does_not_exist_for_the_envelope() {
    // the oscillator is multiplied by velocity/127 before the (linear) fold-free shaping: with P5 = 0.5 (power 1),
    // P7 = 0 (soft drive k = 1 is non-linear): compare at small amplitude where T is near-linear
    let mut p = BASE;
    p[7] = 0.0;
    p[14] = 0.0;
    let one = |vel: u8| {
        let mut d = dev(&p);
        render(&mut d, 3000, &[128], &[(0, 60, vel, true)]).0
    };
    let (a, b) = (one(127), one(64));
    assert!(peak(&a) > peak(&b) * 1.05 && peak(&b) > 0.0);
    assert_eq!(one(0).iter().map(|x| x.abs()).fold(0.0, f32::max) < 1e-3, true, "velocity 0 is (nearly) silent");
    assert!(peak(&one(255)) > peak(&a));
}

#[test]
fn unison_voices_with_equal_pitch_and_centre_pan_are_one_voice() {
    // N identical oscillators summed and divided by N (the level P13 / N) = the single oscillator
    let mut single = BASE;
    single[14] = 0.0;
    single[15] = 0.0;
    let mut many = BASE;
    many[15] = 0.0;
    many[16] = 0.0;
    many[17] = 0.0;
    for p14 in [0.2f32, 0.55, 1.0] {
        many[14] = p14;
        let ev = [(0, 55, 127, true), (3000, 55, 0, false)];
        let a = render(&mut dev(&single), 5000, &[128], &ev);
        let b = render(&mut dev(&many), 5000, &[128], &ev);
        let err = rms_diff(&a.0, &b.0) / rms(&a.0);
        assert!(err < 1e-5, "N={} rel {err}", unison_count(p14));
    }
}

#[test]
fn full_spread_of_two_equal_voices_is_mono_and_detune_breaks_it() {
    let mut p = BASE;
    p[14] = 0.0323; // N = 2
    p[15] = 0.0;
    p[16] = 1.0;
    p[17] = 0.0;
    let ev = [(0, 60, 127, true)];
    let (l, r) = render(&mut dev(&p), 3000, &[128], &ev);
    assert_eq!(bits(&l), bits(&r), "0·y + 2·y both ways");
    p[15] = 0.5;
    let (l, r) = render(&mut dev(&p), 3000, &[128], &ev);
    assert_ne!(bits(&l), bits(&r));
    // octave-up alternation puts the 2·f0 voice on the left (last voice, gain 2) and f0 on the right
    let mut q = BASE;
    q[14] = 0.0323;
    q[15] = 0.0;
    q[16] = 1.0;
    q[17] = 0.15; // mode 0: odd voices an octave up
    q[22] = 1.0;
    let (l, r) = render(&mut dev(&q), 6000, &[128], &ev);
    let zc = |v: &[f32]| v.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
    assert!(zc(&l[1000..]) > zc(&r[1000..]) * 3 / 2, "left {} right {}", zc(&l[1000..]), zc(&r[1000..]));
}

#[test]
fn the_filter_cutoff_darkens_the_sound_and_stays_stable() {
    let ev = [(0, 60, 127, true)];
    let bright = {
        let mut p = BASE;
        p[22] = 0.9;
        p[24] = 0.5;
        render(&mut dev(&p), 6000, &[128], &ev).0
    };
    let dark = {
        let mut p = BASE;
        p[22] = 0.1;
        p[24] = 0.5;
        render(&mut dev(&p), 6000, &[128], &ev).0
    };
    let hf = |v: &[f32]| rms(&v.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>());
    assert!(hf(&dark) < 0.5 * hf(&bright));
    // resonance 1, cutoff 1, envelope amount 1, long note: finite and bounded
    let mut p = BASE;
    p[22] = 1.0;
    p[23] = 1.0;
    p[24] = 1.0;
    p[11] = 1.0;
    let l = render(&mut dev(&p), 44100, &[128], &ev).0;
    assert!(l.iter().all(|x| x.is_finite()) && peak(&l) < 50.0);
}

// ---------------------------------------------------------------- independent model

/// The reference's cheap power: the exponent bits of `x` scaled by `e` (a documented, observable waveshaping step).
fn fast_pow(x: f32, e: f32) -> f32 {
    let b = (x.to_bits() as i32) as f32;
    let a = (b + -1_065_353_216.0) * e + 1_065_353_216.0;
    let i = if a >= -2_147_483_648.0 && a < 2_147_483_648.0 { a as i32 } else { i32::MIN };
    f32::from_bits(i as u32)
}

fn frac(x: f64) -> f64 {
    x - x.floor()
}

/// One oscillator sample (`phase` in cycles) with ideal sines.
fn model_osc(phase: f32, p: &[f32; PARAMS], vel: f32) -> f64 {
    let u = (phase - phase.trunc()).max(1e-10);
    let t = (fast_pow(u, p[1] * 1.8 + 0.1) * (p[2] + 1.0)) as f64;
    let v = vel as f64 / 127.0;
    let mut y;
    if (p[0] * 2.0) as i32 == 0 {
        y = (2.0 * std::f64::consts::PI * frac(t)).sin() * v * (p[3] as f64 * 10.0 + 1.0);
        let mut z = frac(y);
        if p[4] > 0.5 && (y.floor() as i64).rem_euclid(2) == 1 {
            z = 1.0 - z;
        }
        y = if t * 2.0 * std::f64::consts::PI > std::f64::consts::PI { -z } else { z };
    } else {
        y = (if t > 0.5 { -frac(-2.0 * t) } else { frac(2.0 * t) }) * v;
    }
    let mag = (y.abs() as f32).max(1e-10);
    let pw = fast_pow(mag, p[5] * 2.0) as f64;
    y = if y < 0.0 { -pw } else { pw };
    let k = (p[7] * 16.0 + 1.0) as f64;
    let tt = |x: f64| x * (x * x + 27.0) / (9.0 * x * x + 27.0);
    y = tt(k * y) / tt(k);
    if p[6] > 0.001 {
        y = (y * (p[6] as f64 * 20.0 + 1.3)).sin(); // the shaper is a sine of radians
    }
    if p[8] > 0.5 && t > 1.0 {
        y = 1e-4;
    }
    y
}

fn model_adsr(p: &[f32; PARAMS], base: usize) -> Adsr {
    let mut e = Adsr::idle();
    e.restart(p[base], p[base + 1], p[base + 2], p[base + 3]);
    e
}

/// Single-voice (N = 1, no detune) model of the whole device on one held note; returns the left channel.
fn model(p: &[f32; PARAMS], note: u8, vel: u8, n: usize) -> Vec<f32> {
    use std::f64::consts::PI;
    let (mut ea, mut ef) = (model_adsr(p, 9), model_adsr(p, 18));
    let f0 = 440.0 * 2f64.powf((note as f64 - 69.0) / 12.0);
    let inc = (f0 / 44100.0) as f32;
    let mut phase = 0.0f32;
    let mut st = [0.0f64; 4];
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let (a, f) = (ea.tick() as f64, ef.tick() as f64);
        let base = (20.0 + 19980.0 * (p[22] as f64).powi(2)) * 2f64.powf(p[25] as f64 * (note as f64 - 60.0) / 12.0);
        let fc = base.clamp(20.0, 20000.0);
        let fc = (fc * 2f64.powf(f * (384.0 * p[24] as f64 - 192.0) / 12.0)).clamp(20.0, 20000.0);
        let g = (PI * fc.max(30.0) / 44100.0).tan();
        let k = 3.88 * (0.9 * (p[23] as f64).powi(2) + 0.1);
        let gg = g / (1.0 + g);
        let beta = |j: usize| gg.powi(3 - j as i32) / (1.0 + g);
        let x = model_osc(phase, p, vel as f32) * a * (p[13] as f64); // pan (1, 1), level P13 / N with N = 1
        let s: f64 = (0..4).map(|j| beta(j) * st[j]).sum();
        let mut v = (x - k * s) / (1.0 + k * gg.powi(4));
        for sj in st.iter_mut() {
            let d = (v - *sj) * gg;
            let y = *sj + d;
            *sj = y + d;
            v = y;
        }
        out.push(v as f32);
        phase += inc;
    }
    out
}

fn model_base() -> [f32; PARAMS] {
    let mut p = BASE;
    p[14] = 0.0; // N = 1
    p[15] = 0.0;
    p[22] = 0.35; // ~2.5 kHz: far from the Padé's inaccurate region
    p[24] = 0.5; // neutral envelope amount
    p
}

#[test]
fn matches_an_independent_model_for_a_single_voice() {
    // (1) saw, (2) sine + fold gain, (3) sine + triangle fold + power curve + shaper + stretch gate
    let mut saw = model_base();
    saw[1] = 0.4;
    saw[2] = 0.3;
    let mut sine = model_base();
    sine[0] = 0.2;
    sine[3] = 0.02;
    let mut tri = model_base();
    tri[0] = 0.2;
    tri[2] = 0.4;
    tri[3] = 0.04;
    tri[4] = 0.9;
    tri[5] = 0.4;
    tri[6] = 0.3;
    tri[7] = 0.6;
    tri[8] = 0.9;
    for (name, p, vel) in [("saw", saw, 127u8), ("sine", sine, 100), ("tri", tri, 100)] {
        let mut d = dev(&p);
        let got = render(&mut d, 4096, &[128], &[(0, 45, vel, true)]).0;
        let want = model(&p, 45, vel, 4096);
        assert!(peak(&want) > 0.05, "{name} silent model");
        let err = rms_diff(&got, &want) / rms(&want);
        assert!(err < 6e-3, "{name}: relative rms error {err}");
    }
}

#[test]
fn the_model_comparison_has_teeth() {
    // nudging any one read parameter by 0.05 leaves the model's tolerance (negative control of the test above)
    let mut p = model_base();
    p[0] = 0.2;
    p[2] = 0.4;
    p[3] = 0.04;
    p[5] = 0.4;
    p[6] = 0.3;
    p[7] = 0.6;
    let want = model(&p, 45, 100, 4096);
    for i in [1usize, 2, 5, 6, 7, 13, 22, 23] {
        let mut q = p;
        q[i] = if q[i] > 0.9 { q[i] - 0.05 } else { q[i] + 0.05 };
        let got = render(&mut dev(&q), 4096, &[128], &[(0, 45, 100, true)]).0;
        let err = rms_diff(&got, &want) / rms(&want);
        assert!(err > 1.5e-2, "P{i} nudge not detected: {err}");
    }
}
