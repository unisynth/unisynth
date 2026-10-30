//! Synthetic tests for ID 32 (no original data): rejection + atomicity, bypass, exact-dry, block-split identity,
//! delay memory/state persistence, and an independent f64 model of both modes (history-based, no ring buffer).
use crate::id32::{Error, Id32, PARAMS};

const B: usize = 128;

/// Mode A (chorus), 12 lines, mixed, moderate depth/rate; P7 < 0.5. Index: 0 voices, 1 mix, 2 base delay, 3 feedback,
/// 4 depth, 5 rate, 6 stereo alternation, 7 mode, 8 voices (B), 9 rate variation, 10 enable.
const A: [f32; PARAMS] = [0.45, 0.8, 0.2, 0.7, 0.5, 0.15, 0.6, 0.0, 0.5, 0.3, 1.0];
/// Mode B (all-pass chain with feedback), 7 sections.
const BM: [f32; PARAMS] = [0.45, 0.8, 0.3, 0.8, 0.6, 0.15, 0.6, 0.75, 0.5, 0.3, 1.0];

fn with(base: [f32; PARAMS], set: &[(usize, f32)]) -> [f32; PARAMS] {
    let mut p = base;
    for &(i, v) in set {
        p[i] = v;
    }
    p
}

/// Low-frequency stereo test signal (floor flips in the interpolator stay small).
fn signal(n: usize, off: usize) -> (Vec<f32>, Vec<f32>) {
    let f = |hz: f64, a: f64, i: usize| (a * (2.0 * std::f64::consts::PI * hz * (i + off) as f64 / 44100.0).sin()) as f32;
    ((0..n).map(|i| f(220.0, 0.5, i) + f(131.0, 0.2, i)).collect(), (0..n).map(|i| f(330.0, 0.4, i)).collect())
}

fn run(d: &mut Id32, l: &mut [f32], r: &mut [f32]) {
    for (a, b) in l.chunks_mut(B).zip(r.chunks_mut(B)) {
        d.process(a, b).unwrap();
    }
}

fn rejects_index_range_nonfinite_and_leaves_device_unchanged_body() {
    for bad in [-0.001, 1.001, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for i in 0..PARAMS {
            assert_eq!(Id32::new(&with(A, &[(i, bad)])).err(), Some(Error::Range), "new p{i}={bad}");
        }
    }
    let mut d = Id32::new(&A).unwrap();
    let reference = d.clone();
    assert_eq!(d.set_param(PARAMS, 0.5), Err(Error::Index));
    assert_eq!(d.set_param(usize::MAX, 0.5), Err(Error::Index));
    for bad in [-1e-6, 1.0 + 1e-6, f32::NAN, f32::INFINITY] {
        assert_eq!(d.set_param(3, bad), Err(Error::Range));
    }
    let (mut l1, mut r1) = signal(4 * B, 0);
    let (mut l2, mut r2) = (l1.clone(), r1.clone());
    run(&mut d, &mut l1, &mut r1);
    run(&mut reference.clone(), &mut l2, &mut r2);
    assert_eq!((l1, r1), (l2, r2), "a rejected setting must not change the device");
    // boundaries 0 and 1 are valid everywhere (mode B so the base delay cannot trip the buffer check)
    for i in 0..PARAMS {
        for v in [0.0, 1.0] {
            Id32::new(&with(BM, &[(i, v)])).unwrap_or_else(|e| panic!("p{i}={v}: {e:?}"));
        }
    }
}

fn mode_a_delay_beyond_the_buffer_is_rejected_mode_b_is_not_body() {
    // worst case `(0.002 + 0.4·P2³)·(1 + 0.8·P4³)·44100` samples must fit the 2205-sample line (limit 2200)
    let ok = |p2: f32, p4: f32| Id32::new(&with(A, &[(2, p2), (4, p4)]));
    assert!(ok(0.49, 0.0).is_ok());
    assert_eq!(ok(0.5, 0.0).err(), Some(Error::Delay));
    assert!(ok(0.40, 1.0).is_ok());
    assert_eq!(ok(0.41, 1.0).err(), Some(Error::Delay));
    assert_eq!(ok(1.0, 1.0).err(), Some(Error::Delay));
    // mode B uses the base delay only up to 0.0102 s: any P2 is fine
    assert!(Id32::new(&with(BM, &[(2, 1.0), (4, 1.0)])).is_ok());
    // set_param judges the whole resulting vector, atomically
    let mut d = Id32::new(&with(A, &[(2, 0.45)])).unwrap();
    assert_eq!(d.set_param(2, 0.9), Err(Error::Delay));
    assert_eq!(d.set_param(4, 1.0), Err(Error::Delay)); // 0.45 with full depth no longer fits
    d.set_param(7, 1.0).unwrap(); // switching to mode B makes it valid
    d.set_param(4, 1.0).unwrap();
    d.set_param(2, 0.9).unwrap();
    assert_eq!(d.set_param(7, 0.0), Err(Error::Delay)); // and back is not
    d.set_param(7, 0.4999).map_or_else(|e| assert_eq!(e, Error::Delay), |_| panic!("0.4999 is mode A"));
}

fn multi_param_transitions_depend_on_the_order_of_set_param_body() {
    // target: mode A, P2 = 0.45, P4 = 0.5 (valid); start: mode B with a long delay (valid there only)
    let start = with(BM, &[(2, 0.9), (4, 1.0)]);
    let fin = with(A, &[(2, 0.45), (4, 0.5)]);
    Id32::new(&fin).unwrap();
    // wrong order: P7 first sees P2 = 0.9 in mode A. Rejected, and nothing changed (still accepts the right order)
    let mut d = Id32::new(&start).unwrap();
    assert_eq!(d.set_param(7, 0.0), Err(Error::Delay));
    // right order: shorten the delay while still in mode B, then switch mode
    d.set_param(2, 0.45).unwrap();
    d.set_param(4, 0.5).unwrap();
    d.set_param(7, 0.0).unwrap();
    let (mut l, mut r) = signal(2 * B, 0);
    run(&mut d, &mut l, &mut r); // the transitioned device processes (mode A, in-range delay)
    // raising P2 and P4 inside mode A: the intermediate vector must fit too
    let mut d = Id32::new(&with(A, &[(2, 0.2), (4, 0.0)])).unwrap();
    d.set_param(4, 1.0).unwrap(); // 0.2 with full depth fits
    assert_eq!(d.set_param(2, 0.45), Err(Error::Delay)); // 0.45 with full depth does not: P4 had to drop first
    d.set_param(4, 0.5).unwrap();
    d.set_param(2, 0.45).unwrap();
    // reverse direction: A → B needs P7 first, then the long delay
    let mut d = Id32::new(&fin).unwrap();
    assert_eq!(d.set_param(2, 0.9), Err(Error::Delay));
    d.set_param(7, 1.0).unwrap();
    d.set_param(2, 0.9).unwrap();
}

fn stale_smoothed_delay_makes_process_refuse_a_block_without_touching_anything_body() {
    // the base delay glides (0.001 per sample) towards its target in either mode; B → A with an unsettled long delay
    // would read before the line buffer in the original, so `process` refuses the block instead
    let mut d = Id32::new(&with(BM, &[(2, 0.9), (4, 0.0)])).unwrap();
    let (mut l, mut r) = signal(40 * B, 0);
    run(&mut d, &mut l, &mut r);
    d.set_param(2, 0.05).unwrap();
    d.set_param(7, 0.0).unwrap(); // valid in isolation: the target delay is small
    let before = d.clone();
    let (mut l, mut r) = signal(B, 5000);
    let (l0, r0) = (l.clone(), r.clone());
    assert_eq!(d.process(&mut l, &mut r), Err(Error::Delay));
    assert_eq!((&l, &r), (&l0, &r0), "buffers untouched on refusal");
    // state untouched: the same refusal repeats and mode B still renders identically to the saved copy
    assert_eq!(d.process(&mut l, &mut r), Err(Error::Delay));
    d.set_param(7, 1.0).unwrap();
    let mut e = before.clone();
    e.set_param(7, 1.0).unwrap();
    let (mut l1, mut r1) = signal(10 * B, 9000);
    let (mut l2, mut r2) = (l1.clone(), r1.clone());
    run(&mut d, &mut l1, &mut r1);
    run(&mut e, &mut l2, &mut r2);
    assert_eq!((l1, r1), (l2, r2));
    // after the glide settled mode A is accepted again
    let (mut l, mut r) = signal(60 * B, 20000);
    run(&mut d, &mut l, &mut r);
    d.set_param(7, 0.0).unwrap();
    let (mut l, mut r) = signal(B, 0);
    assert_eq!(d.process(&mut l, &mut r), Ok(()));
}

fn disabled_below_half_is_an_exact_pass_through_and_freezes_all_state_body() {
    for base in [A, BM] {
        let mut disabled = Id32::new(&with(base, &[(10, 0.4999)])).unwrap();
        let mut skipped = Id32::new(&base).unwrap();
        let (mut l, mut r) = signal(8 * B, 0);
        let (l0, r0) = (l.clone(), r.clone());
        run(&mut disabled, &mut l, &mut r);
        assert_eq!((&l, &r), (&l0, &r0), "bypass leaves the block bit-identical");
        // nothing advanced (smoothed delay, LFO phases, lines): once enabled it equals a device that never saw the blocks
        disabled.set_param(10, 0.5).unwrap();
        let (mut a, mut b) = signal(10 * B, 77);
        let (mut c, mut e) = (a.clone(), b.clone());
        run(&mut disabled, &mut a, &mut b);
        run(&mut skipped, &mut c, &mut e);
        assert_eq!((a, b), (c, e));
    }
}

fn zero_mix_is_the_exact_dry_signal_in_both_modes_body() {
    for base in [A, BM] {
        let mut d = Id32::new(&with(base, &[(1, 0.0)])).unwrap();
        let (mut l, mut r) = signal(20 * B, 0);
        let (l0, r0) = (l.clone(), r.clone());
        run(&mut d, &mut l, &mut r);
        assert_eq!((l, r), (l0, r0));
    }
}

fn block_split_is_bit_identical_in_both_modes_body() {
    for base in [A, BM, with(A, &[(0, 1.0), (6, 1.0), (4, 0.4), (2, 0.3)])] {
        let (mut l1, mut r1) = signal(12 * B, 0);
        let (mut l2, mut r2) = (l1.clone(), r1.clone());
        let mut a = Id32::new(&base).unwrap();
        let mut b = Id32::new(&base).unwrap();
        for (x, y) in l1.chunks_mut(B).zip(r1.chunks_mut(B)) {
            a.process(x, y).unwrap();
        }
        for (x, y) in l2.chunks_mut(32).zip(r2.chunks_mut(32)) {
            b.process(x, y).unwrap();
        }
        assert_eq!((l1, r1), (l2, r2));
    }
}

fn length_mismatch_panics_body() {
    let mut d = Id32::new(&A).unwrap();
    let _ = d.process(&mut [0.0; 4], &mut [0.0; 3]);
}

fn constants_are_the_reference_f32_bit_patterns_body() {
    assert_eq!(10.999f32.to_bits(), 0x412f_fbe7);
    assert_eq!((1.0f32 / 44100.0).to_bits(), 0x37be_37c6); // the correctly rounded 1/44100 is the same constant
    assert_eq!(2.267_573_7e-5f32.to_bits(), 0x37be_37c6);
    assert_eq!(4.535_147_3e-5f32.to_bits(), 0x383e_37c6);
    assert_eq!(std::f32::consts::PI.to_bits(), 0x4049_0fdb);
    assert_eq!((2.0 * std::f32::consts::PI).to_bits(), 0x40c9_0fdb);
}

fn an_impulse_echoes_after_the_glided_base_delay_body() {
    // P4 = 0 (no depth), feedback 0, mix 1, base delay 0.002 s = 88.2 samples; each of the 2 lines echoes once,
    // the stereo weights (1 ± P6) split the energy between channels
    let p = with(A, &[(0, 0.0), (1, 1.0), (2, 0.0), (3, 0.5), (4, 0.0), (6, 0.0), (9, 0.0)]);
    let mut d = Id32::new(&p).unwrap();
    let mut zl = vec![0.0f32; 400 * B];
    let mut zr = zl.clone();
    run(&mut d, &mut zl, &mut zr); // let the delay glide settle (time constant 1000 samples)
    let mut l = vec![0.0f32; 4 * B];
    l[0] = 1.0;
    let mut r = vec![0.0f32; 4 * B];
    run(&mut d, &mut l, &mut r);
    let peak = (0..l.len()).max_by(|&a, &b| l[a].abs().total_cmp(&l[b].abs())).unwrap();
    assert!((88..=90).contains(&peak), "echo at {peak}");
    let energy: f32 = l.iter().sum();
    // 2 lines · (2.25/2) scale · unit impulse (the interpolation weights sum to 1), weights 1 + 0 each lane
    assert!((energy - 2.25).abs() < 1e-3, "echo area {energy}");
    assert!(l[..peak - 2].iter().all(|&x| x == 0.0), "nothing before the echo");
}

// ----- independent f64 model: every line keeps its whole push history (no ring buffer, no index arithmetic) -----

#[derive(Clone, Default)]
struct Line {
    h: Vec<[f64; 2]>,
    cur: f64,
    tgt: f64,
}

impl Line {
    fn v(&self, n: usize) -> [f64; 2] {
        self.h.len().checked_sub(1 + n).map_or([0.0; 2], |i| self.h[i])
    }
    fn read(&self) -> [f64; 2] {
        let d = self.cur * 44100.0;
        let n = d.floor();
        let f = d - n;
        let (a, b) = (self.v(n as usize), self.v(n as usize + 1));
        [(1.0 - f) * a[0] + f * b[0], (1.0 - f) * a[1] + f * b[1]]
    }
    fn push(&mut self, x: [f64; 2]) {
        self.cur = self.tgt;
        self.h.push(x);
    }
}

struct Model {
    lines: Vec<Line>,
    /// LFO phases accumulate in f32 like the original: an f64 accumulator would drift away from it over a few
    /// thousand samples and the delay modulation (up to hundreds of samples) would not stay comparable.
    phase: [f32; 16],
    /// The base-delay glide `d += (target − d)·0.001` also runs in f32: it stalls ~500 ulp short of the target, which an
    /// f64 model would miss (a few 1e-4 of output error over a second).
    dsm: f32,
    st: [f64; 2],
}

/// The sine of the model is the reference's table sine (it defines the modulation); f64 only for the delay lines.
fn tsin(x: f32) -> f64 {
    static TABLE: std::sync::OnceLock<[f32; crate::id34::TABLE_LEN]> = std::sync::OnceLock::new();
    crate::id34::sin_with(TABLE.get_or_init(crate::id34::make_table), x) as f64
}

impl Model {
    fn new() -> Self {
        Model { lines: vec![Line::default(); 16], phase: [0.0; 16], dsm: 0.0, st: [0.0; 2] }
    }
    fn process(&mut self, p: &[f32; PARAMS], l: &mut [f32], r: &mut [f32]) {
        let pf = *p;
        let p: Vec<f64> = p.iter().map(|&x| x as f64).collect();
        if p[10] < 0.5 {
            return;
        }
        let tau = 2.0 * std::f32::consts::PI;
        let c = p[2].powi(3);
        let target = 0.4 * ((pf[2] * pf[2]) * pf[2]) + 0.002;
        let fb = 2.0 * p[3] - 1.0;
        let g = p[3].min(0.999);
        let d = c * 0.005 + 0.0001;
        let na = (2 * (p[0] * 12.0) as usize + 2).min(16);
        let nb = (((p[8] * 10.999) as usize) + 2).min(16);
        let rate = ((pf[5] * pf[5]) * 15040.0) * pf[5] * 2.267_573_7e-5f32;
        for k in 0..l.len() {
            self.dsm = (target - self.dsm) * 0.001 + self.dsm;
            let dsm = self.dsm as f64;
            let x = [l[k] as f64, r[k] as f64];
            let out;
            if p[7] < 0.5 {
                let mut acc = [0.0; 2];
                for i in 0..na {
                    let fi = i as f32;
                    let m = tsin(fi) as f32 * pf[9] + 1.0; // the per-line factor is defined by the table sine
                    let arg = fi * std::f32::consts::PI / na as f32 + self.phase[i];
                    self.lines[i].tgt = dsm * (1.0 + 0.8 * p[4].powi(3) * tsin(arg));
                    let tap = self.lines[i].read();
                    self.lines[i].push([tap[0] * fb + x[0], tap[1] * fb + x[1]]);
                    self.phase[i] += m * rate;
                    if self.phase[i] >= tau {
                        self.phase[i] -= tau;
                    }
                    let (hi, lo) = (1.0 + p[6], 1.0 - p[6]);
                    let w = if i % 2 == 0 { [hi, lo] } else { [lo, hi] };
                    acc[0] += tap[0] * w[0];
                    acc[1] += tap[1] * w[1];
                }
                let k1 = p[1] * 2.25 / na as f64;
                out = [k1 * acc[0] + (1.0 - p[1]) * x[0], k1 * acc[1] + (1.0 - p[1]) * x[1]];
            } else {
                let mut cur = [self.st[0] * fb + x[0], self.st[1] * fb + x[1]];
                for i in 0..nb {
                    let fi = i as f32;
                    let arg = fi * std::f32::consts::PI / nb as f32 + self.phase[i];
                    self.lines[i].tgt = ((tsin(arg) * p[4] + 1.0) * d).max(1.0 / 22050.0);
                    let tap = self.lines[i].read();
                    let w = [cur[0] - tap[0] * g, cur[1] - tap[1] * g];
                    let y = [g * w[0] + tap[0], g * w[1] + tap[1]];
                    self.lines[i].push(w);
                    cur = y;
                    self.phase[i] += (fi * (0.3 * (1.0 / nb as f32)) + 1.0) * rate;
                    if self.phase[i] >= tau {
                        self.phase[i] -= tau;
                    }
                }
                self.st = cur;
                out = [p[1] * cur[0] + (1.0 - p[1]) * x[0], p[1] * cur[1] + (1.0 - p[1]) * x[1]];
            }
            l[k] = out[0] as f32;
            r[k] = out[1] as f32;
        }
    }
}

/// Runs the device and the model over the same blocks, `sets[b]` being the parameter vector of block `b`
/// (applied through `set_param`); returns the max abs difference.
fn compare(sets: &[[f32; PARAMS]], off: usize) -> f64 {
    let mut d = Id32::new(&sets[0]).unwrap();
    let mut m = Model::new();
    let mut worst = 0.0f64;
    let (il, ir) = signal(sets.len() * B, off);
    for (b, p) in sets.iter().enumerate() {
        for (i, &v) in p.iter().enumerate() {
            d.set_param(i, v).unwrap();
        }
        let (mut l, mut r) = (il[b * B..(b + 1) * B].to_vec(), ir[b * B..(b + 1) * B].to_vec());
        let (mut ml, mut mr) = (l.clone(), r.clone());
        d.process(&mut l, &mut r).unwrap();
        m.process(p, &mut ml, &mut mr);
        for i in 0..B {
            worst = worst.max((l[i] - ml[i]).abs() as f64).max((r[i] - mr[i]).abs() as f64);
        }
    }
    worst
}

fn mode_a_matches_the_independent_model_body() {
    let sets = vec![A; 300];
    let e = compare(&sets, 0);
    assert!(e < 1e-4, "max abs err {e}");
    // maximum voices, full stereo alternation, strong rate variation
    let hard = with(A, &[(0, 1.0), (6, 1.0), (9, 1.0), (5, 0.3), (4, 0.35), (2, 0.3), (3, 0.9)]);
    let e = compare(&vec![hard; 300], 11);
    assert!(e < 1e-4, "max abs err {e}");
    // negative feedback, minimum voices
    let neg = with(A, &[(0, 0.0), (3, 0.05), (5, 0.4)]);
    let e = compare(&vec![neg; 300], 5);
    assert!(e < 1e-4, "max abs err {e}");
}

fn mode_b_matches_the_independent_model_body() {
    let e = compare(&vec![BM; 300], 0);
    assert!(e < 5e-4, "max abs err {e}");
    let wide = with(BM, &[(8, 1.0), (4, 1.0), (2, 1.0), (5, 0.5), (3, 0.6)]); // 12 sections, depth 1, longest delays
    let e = compare(&vec![wide; 300], 3);
    assert!(e < 5e-4, "max abs err {e}");
    let min = with(BM, &[(8, 0.0), (3, 0.5), (4, 0.0)]); // 2 sections, no feedback, no depth
    let e = compare(&vec![min; 300], 9);
    assert!(e < 5e-4, "max abs err {e}");
    // P7 = 0.5 is already mode B
    let edge = with(BM, &[(7, 0.5)]);
    let e = compare(&vec![edge; 100], 1);
    assert!(e < 5e-4, "max abs err {e}");
}

fn automation_mode_switches_and_voice_count_changes_keep_line_state_body() {
    // every block changes parameters; lines above the live count are frozen, not reset, and resume later
    let mut sets = Vec::new();
    for b in 0..400usize {
        let t = b as f32 / 400.0;
        let mut p = with(A, &[(2, 0.1 + 0.2 * t), (4, 0.2 + 0.3 * (6.0 * t).sin().abs()), (1, 0.5 + 0.4 * t)]);
        match b / 50 {
            1 => p[0] = 1.0, // 16 lines
            2 => p[0] = 0.0, // 2 lines
            3 => p[7] = 1.0, // mode B
            4 => p[8] = 1.0, // B with 12
            5 => p[0] = 1.0, // A again: all 16 lines resume from their old history
            6 => p[10] = 0.0, // bypassed
            _ => {}
        }
        sets.push(p);
    }
    let e = compare(&sets, 0);
    assert!(e < 5e-4, "max abs err {e}");
}

// Id32 is ~282 KB inline: a debug build copies it a few times, so every test body runs on a roomy stack.
fn big_stack(f: fn()) {
    if let Err(e) = std::thread::Builder::new().stack_size(64 << 20).spawn(f).unwrap().join() {
        std::panic::resume_unwind(e)
    }
}

#[test]
fn rejects_index_range_nonfinite_and_leaves_device_unchanged() {
    big_stack(rejects_index_range_nonfinite_and_leaves_device_unchanged_body);
}

#[test]
fn mode_a_delay_beyond_the_buffer_is_rejected_mode_b_is_not() {
    big_stack(mode_a_delay_beyond_the_buffer_is_rejected_mode_b_is_not_body);
}

#[test]
fn multi_param_transitions_depend_on_the_order_of_set_param() {
    big_stack(multi_param_transitions_depend_on_the_order_of_set_param_body);
}

#[test]
fn stale_smoothed_delay_makes_process_refuse_a_block_without_touching_anything() {
    big_stack(stale_smoothed_delay_makes_process_refuse_a_block_without_touching_anything_body);
}

#[test]
fn disabled_below_half_is_an_exact_pass_through_and_freezes_all_state() {
    big_stack(disabled_below_half_is_an_exact_pass_through_and_freezes_all_state_body);
}

#[test]
fn zero_mix_is_the_exact_dry_signal_in_both_modes() {
    big_stack(zero_mix_is_the_exact_dry_signal_in_both_modes_body);
}

#[test]
fn block_split_is_bit_identical_in_both_modes() {
    big_stack(block_split_is_bit_identical_in_both_modes_body);
}

#[test]
#[should_panic(expected = "channel length mismatch")]
fn length_mismatch_panics() {
    big_stack(length_mismatch_panics_body);
}

#[test]
fn constants_are_the_reference_f32_bit_patterns() {
    big_stack(constants_are_the_reference_f32_bit_patterns_body);
}

#[test]
fn an_impulse_echoes_after_the_glided_base_delay() {
    big_stack(an_impulse_echoes_after_the_glided_base_delay_body);
}

#[test]
fn mode_a_matches_the_independent_model() {
    big_stack(mode_a_matches_the_independent_model_body);
}

#[test]
fn mode_b_matches_the_independent_model() {
    big_stack(mode_b_matches_the_independent_model_body);
}

#[test]
fn automation_mode_switches_and_voice_count_changes_keep_line_state() {
    big_stack(automation_mode_switches_and_voice_count_changes_keep_line_state_body);
}

