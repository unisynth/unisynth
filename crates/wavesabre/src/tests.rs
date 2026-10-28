use crate::helpers::*;
use synth_core::rng::Rng;

#[test]
fn random_law_matches_reference_formula() {
    let mut r = WsRng::new();
    // reference: 32-bit wrapping multiply on a signed seed, C truncating remainder, float division
    let mut seed: i64 = 1;
    for _ in 0..5 {
        seed = ((seed * 0x15a4e35) as i32) as i64;
        let want = (seed % 255) as f32 / 255.0;
        assert_eq!(r.next_f32(), want);
    }
    // the range really goes negative once the seed wraps
    let mut r = WsRng::new();
    assert!((0..1000).any(|_| r.next_f32() < 0.0));
}

/// Independent model of upstream FastSin: top 32 mantissa bits of (frac + 1) → 9-bit index + 23-bit fraction.
fn fast_sin_model(x: f64) -> f64 {
    let p = x * (1.0 / (core::f64::consts::PI * 2.0));
    let f = p - p.floor() + 1.0;
    let bits = ((f - 1.0) * 4294967296.0) as u32;
    let index = (bits >> 23) as usize;
    let fract = (bits & 0x7f_ffff) as f64 * (1.0 / 8388608.0);
    let step = (core::f64::consts::PI * 2.0) / 512.0;
    let left = libm::sin(index as f64 * step);
    let right = libm::sin((index + 1) as f64 * step);
    left + (right - left) * fract
}

#[test]
fn fast_sin_matches_model_and_sin() {
    init();
    for i in 0..10_000 {
        let x = -4.0 * core::f64::consts::PI + i as f64 * (8.0 * core::f64::consts::PI / 10_000.0);
        assert_eq!(fast_sin(x), fast_sin_model(x), "x = {x}");
        assert!((fast_sin(x) - libm::sin(x)).abs() < 1e-4, "x = {x}");
        assert!((fast_cos(x) - libm::cos(x)).abs() < 1e-4, "x = {x}");
    }
}

#[test]
fn square_partials() {
    init();
    let x = 0.3;
    assert_eq!(square135(x), fast_sin(x) + fast_sin(x * 3.0) / 3.0 + fast_sin(x * 5.0) / 5.0);
    assert_eq!(square35(x), fast_sin(x * 3.0) / 3.0 + fast_sin(x * 5.0) / 5.0);
}

#[test]
fn parameter_mappings() {
    assert_eq!(note_to_freq(69.0), 440.0);
    assert!((note_to_freq(81.0) - 880.0).abs() < 1e-9);
    assert_eq!(exp2f(3.0), 8.0);
    assert!((exp10f(2.0) - 100.0).abs() < 1e-3);
    assert!((db_to_scalar(6.0) - 2.0).abs() < 1e-6); // upstream dB law: 2^(dB/6)
    assert_eq!(param_to_frequency(0.0), 20.0);
    assert_eq!(param_to_frequency(1.0), 20000.0);
    assert_eq!(param_to_q(0.0), 0.33);
    assert_eq!(param_to_q(0.5), 1.0);
    assert_eq!(param_to_q(1.0), 12.0);
    assert_eq!(param_to_db(0.5, 24.0), 0.0);
    assert_eq!(param_to_db(1.0, 12.0), 12.0);
    assert!((volume_to_scalar(1.0) - 0.16).abs() < 1e-7);
    assert_eq!(scalar_to_env_value(1.0), 5001.0);
    assert_eq!(env_value_to_scalar(5001.0), 1.0);
    assert!(param_to_boolean(0.5) && !param_to_boolean(0.49));
    assert!((param_to_resonance(1.0) - 1.0).abs() < 1e-7);
    assert_eq!(param_to_unisono(0.0), 1);
    assert_eq!(param_to_unisono(1.0), 16);
    assert!((param_to_vibrato_freq(0.0) - 7.0).abs() < 1e-12);
    assert_eq!(pow4(2.0), 16.0);
}

// ---- devices: shared test harness ----

pub(crate) mod harness {
    use synth_core::device::*;
    use synth_core::rng::Rng;

    pub struct Zero;
    impl Rng for Zero {
        fn next_f32(&mut self) -> f32 {
            0.0
        }
    }

    /// Runs `dev` in place on stereo input (main L/R), with an optional sidechain; returns main L/R.
    pub fn run(dev: &mut dyn Device, l: &[f32], r: &[f32], side: Option<(&[f32], &[f32])>) -> (Vec<f32>, Vec<f32>) {
        run_at(dev, l, r, side, 120)
    }

    pub fn run_at(
        dev: &mut dyn Device,
        l: &[f32],
        r: &[f32],
        side: Option<(&[f32], &[f32])>,
        tempo: i32,
    ) -> (Vec<f32>, Vec<f32>) {
        let n = l.len();
        let (mut a, mut b) = (l.to_vec(), r.to_vec());
        let (mut c, mut d) = side.map_or((vec![0.0; n], vec![0.0; n]), |(x, y)| (x.to_vec(), y.to_vec()));
        let mut rng = Zero;
        let mut ctx = Ctx { rate: 44100.0, tempo, frame: 0, rng: &mut rng };
        let mut io = Io { ch: [&mut a, &mut b, &mut c, &mut d] };
        dev.run(&mut io, n, &mut ctx);
        (a, b)
    }

    pub fn sine(freq: f32, n: usize, amp: f32) -> Vec<f32> {
        (0..n).map(|i| amp * libm::sinf(2.0 * core::f32::consts::PI * freq * i as f32 / 44100.0)).collect()
    }

    /// Peak level in dB over the second half of `x`.
    pub fn peak_db(x: &[f32]) -> f32 {
        20.0 * libm::log10f(x[x.len() / 2..].iter().fold(0.0f32, |m, v| m.max(v.abs())))
    }

    /// Little-endian chunk of `params` followed by the i32 chunk size (upstream `GetChunk` layout).
    pub fn chunk(params: &[f32]) -> Vec<u8> {
        let mut c: Vec<u8> = params.iter().flat_map(|p| p.to_le_bytes()).collect();
        let size = (c.len() + 4) as i32;
        c.extend(size.to_le_bytes());
        c
    }
}

mod effect_tests_1 {
    use super::harness::*;
    use crate::*;
    use synth_core::device::Device;
    use synth_core::mix::{Env, Kinds};
    use synth_core::pool::Pool;

    fn load(kind: u8, params: &[f32]) -> WsDevice<'static> {
        let c: &'static [u8] = Box::leak(chunk(params).into_boxed_slice());
        let mem: &'static mut [u8] = Box::leak(vec![0u8; 1 << 20].into_boxed_slice());
        let mut pool = Pool::new(mem);
        let env = Env { rate: 44100.0, tempo: 120, bank: None };
        WsDevice::load(kind, c, &env, &mut pool, &mut Zero).unwrap()
    }

    #[test]
    fn kinds_ids_and_param_counts() {
        assert_eq!(load(kind::CRUSHER, &[]).param_count(), 3);
        assert_eq!(load(kind::SCISSOR, &[]).param_count(), 6);
        assert_eq!(load(kind::LEVELLER, &[]).param_count(), 14);
        let env = Env { rate: 44100.0, tempo: 120, bank: None };
        assert!(WsDevice::need(99, &[], &env).is_err());
    }

    /// Screwdriver (fork kind 13) against the C++ of qm210/WaveSabre a175dd6 built natively (clang, powf): Robo
    /// Outer Space Epos's four parameter sets plus one where X1 > the default X2 clamps; L = ramp −1.2..1.2, R = a
    /// stepped ramp. Within 2 ulp: only `powf` differs (libm here, Apple's libm in the reference; the intro itself used
    /// MSVC's x87 CRT, so no build reproduces it bit for bit); every branch and clamp matches.
    #[cfg(feature = "ws-screwdriver")]
    #[test]
    fn screwdriver_matches_the_cpp_reference() {
        const N: usize = 601;
        let sets: [[f32; 8]; 5] = [
            [0.1, 0.2, 0.16, 0.065, 0.22, 0.5, 0.265, 0.345],
            [0.171, 0.22, 0.644, 0.465, 0.465, 0.395, 0.47, 0.305],
            [0.1, 0.1, 0.3, 0.3, 0.235, 0.5, 0.5, 0.63],
            [0.11, 0.36, 0.2, 0.24, 0.49, 0.51, 0.37, 0.725],
            [0.7, 0.9, 0.8, 0.05, 0.0, 1.0, 0.9, 0.5],
        ];
        let l: Vec<f32> = (0..N).map(|i| -1.2 + 2.4 * i as f32 / (N - 1) as f32).collect();
        let r: Vec<f32> = (0..N).map(|i| 1.1 * (((i * 37) % N) as f32 / (N - 1) as f32 * 2.0 - 1.0)).collect();
        let want: Vec<u32> =
            include_str!("testdata/screwdriver_ref.txt").lines().map(|h| u32::from_str_radix(h, 16).unwrap()).collect();
        assert_eq!(want.len(), sets.len() * 2 * N);
        assert_eq!(load(kind::SCREWDRIVER, &[]).param_count(), 8);
        for (k, s) in sets.iter().enumerate() {
            let mut d = load(kind::SCREWDRIVER, s);
            let (a, b) = run(&mut d, &l, &r, None);
            let got: Vec<u32> = a.iter().chain(b.iter()).map(|v| v.to_bits()).collect();
            let at = k * 2 * N;
            let ulps = got.iter().zip(&want[at..at + 2 * N]).map(|(g, w)| (*g as i64 - *w as i64).unsigned_abs()).max().unwrap();
            let bad = got.iter().zip(&want[at..at + 2 * N]).filter(|(g, w)| g != w).count();
            assert!(ulps <= 2, "set {k}: {bad} samples differ, up to {ulps} ulp");
        }
    }

    #[test]
    fn short_and_tiny_chunks() {
        // chunk with fewer params than the device: given ones set, rest keep defaults
        let mut d = load(kind::CRUSHER, &[0.5]);
        let x = vec![0.3f32; 64];
        let (y, _) = run(&mut d, &x, &x, None);
        assert_ne!(y, x, "vertical set, dry/wet keeps default 1 → crushed");
        // chunk shorter than the size field: no params read, no panic
        let mut pool_mem = vec![0u8; 1024];
        let mut pool = Pool::new(&mut pool_mem);
        let env = Env { rate: 44100.0, tempo: 120, bank: None };
        assert!(WsDevice::load(kind::CRUSHER, &[1, 2], &env, &mut pool, &mut Zero).is_ok());
    }

    #[test]
    fn crusher_step_and_hold() {
        // vertical 0.5 → step = 1/2^(0.5·15+1) = 2^-8.5; horizontal 0.5 → freq 0.25 → hold every 4th sample
        let mut d = load(kind::CRUSHER, &[0.5, 0.5, 1.0]);
        let x: Vec<f32> = (0..16).map(|i| i as f32 * 0.01).collect();
        let (y, _) = run(&mut d, &x, &x, None);
        let step = 1.0 / libm::powf(2.0, 8.5);
        let q = |v: f32| libm::floorf(v / step + 0.5) * step;
        assert_eq!(&y[..4], &[0.0, 0.0, 0.0, q(x[3])]); // phase reaches 1.0 at the 4th sample
        assert_eq!(&y[4..8], &[q(x[3]); 4][..3].iter().copied().chain([q(x[7])]).collect::<Vec<_>>()[..]);
        // dry/wet 0 → passthrough
        let mut d = load(kind::CRUSHER, &[0.5, 0.5, 0.0]);
        assert_eq!(run(&mut d, &x, &x, None).0, x);
    }

    #[test]
    fn scissor_shapers() {
        // drive 0.2 → driveScalar 1, threshold 0.8, clipper: |v|>0.8 clipped to 0.8
        let mut d = load(kind::SCISSOR, &[0.2, 0.8, 0.0, 1.0, 0.0, 0.0]);
        let clip = |x: f32| (x / 0.8 * 1.0).clamp(-1.0, 1.0) * 0.8;
        let x = vec![0.5, 1.0, -1.5, 0.79];
        assert_eq!(run(&mut d, &x, &x, None).0, vec![clip(0.5), 0.8, -0.8, clip(0.79)]);
        // foldover 0.5: v = 1.25/0.8.. → 1 + (1 − v)·0.5 then clamp
        let mut d = load(kind::SCISSOR, &[0.2, 0.8, 0.5, 1.0, 0.0, 0.0]);
        let v: f32 = 1.0 / 0.8;
        let want = (1.0 + (1.0 - v) * 0.5) * 0.8;
        assert_eq!(run(&mut d, &[1.0], &[1.0], None).0, vec![want]);
        // parabola (type 1.0 → 2): (v/t)^3·t
        let mut d = load(kind::SCISSOR, &[0.2, 0.8, 0.0, 1.0, 1.0, 0.0]);
        let v: f32 = 0.4 / 0.8;
        assert_eq!(run(&mut d, &[0.4], &[0.4], None).0, vec![v * v * v * 0.8]);
    }

    #[test]
    fn scissor_oversampling() {
        // X2 (param 0.5): mean of shaped midpoint and shaped input; X4 (param 1): mean of 4 shaped points
        let x = vec![0.0f32, 2.0];
        let mut d = load(kind::SCISSOR, &[0.2, 0.8, 0.0, 1.0, 0.0, 0.5]);
        assert_eq!(run(&mut d, &x, &x, None).0, vec![0.0, (0.8 + 0.8) * 0.5]);
        let x = vec![0.0f32, 1.0];
        let mut d = load(kind::SCISSOR, &[0.2, 0.8, 0.0, 1.0, 0.0, 1.0]);
        let c = |x: f32| (x / 0.8 * 1.0).clamp(-1.0, 1.0) * 0.8;
        assert_eq!(run(&mut d, &x, &x, None).0[1], (c(0.25) + c(0.5) + c(0.75) + c(1.0)) * 0.25);
    }

    #[test]
    fn leveller_peak_and_cuts() {
        let p = |hz: f32| libm::sqrtf((hz - 20.0) / (20000.0 - 20.0));
        // low cut 20 Hz, peak1 +6 dB at 1 kHz Q≈1, high cut 20 kHz, master 1
        let params = [p(20.0), 0.5, p(1000.0), 0.625, 0.5, p(3000.0), 0.5, 0.5, p(7000.0), 0.5, 0.5, p(20000.0), 0.5, 1.0];
        // 100 Hz: upstream quirk. At the default 20 Hz low cut, 1 − FastCos(w0) is ≈4.3× too large (the 512-entry
        // table's interpolation error ≈2e-5 exceeds 1 − cos ≈ 4e-6), so the high-pass sits at ≈42 Hz with Q ≈ 2.1:
        // analytic +1.41 dB at 100 Hz. Reproduced for parity.
        for (hz, want, tol) in [(1000.0, 6.0, 0.2), (100.0, 1.41, 0.15)] {
            let mut d = load(kind::LEVELLER, &params);
            let x = sine(hz, 44100, 0.5);
            let (y, _) = run(&mut d, &x, &x, None);
            let g = peak_db(&y) - peak_db(&x);
            assert!((g - want).abs() < tol, "{hz} Hz: {g} dB");
        }
        // low cut at 1 kHz attenuates 100 Hz strongly (12 dB/oct, Q 1)
        let mut lc = params;
        lc[0] = p(1000.0);
        lc[3] = 0.5;
        let mut d = load(kind::LEVELLER, &lc);
        let x = sine(100.0, 44100, 0.5);
        assert!(peak_db(&run(&mut d, &x, &x, None).0) - peak_db(&x) < -35.0);
    }
}

mod effect_tests_2 {
    use super::harness::*;
    use crate::*;
    use synth_core::device::Device;
    use synth_core::mix::{Env, Kinds};
    use synth_core::pool::Pool;

    fn load_at(kind: u8, params: &[f32], tempo: i32) -> WsDevice<'static> {
        let c: &'static [u8] = Box::leak(chunk(params).into_boxed_slice());
        let env = Env { rate: 44100.0, tempo, bank: None };
        let need = WsDevice::need(kind, c, &env).unwrap();
        let mem: &'static mut [u8] = Box::leak(vec![0u8; need].into_boxed_slice());
        let mut pool = Pool::new(mem);
        WsDevice::load(kind, c, &env, &mut pool, &mut Zero).unwrap()
    }

    fn impulse(n: usize) -> Vec<f32> {
        let mut x = vec![0.0; n];
        x[0] = 1.0;
        x
    }

    fn first_nonzero(x: &[f32]) -> usize {
        x.iter().position(|v| v.abs() > 1e-9).unwrap()
    }

    /// Echo with dry 0, feedback 0, open filters; returns (L, R) impulse responses.
    fn echo_ir(coarse_l: f32, coarse_r: f32, tempo: i32) -> (Vec<f32>, Vec<f32>) {
        let mut d = load_at(kind::ECHO, &[coarse_l, 0.0, coarse_r, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0], tempo);
        let x = impulse(60000);
        run_at(&mut d, &x, &x, None, tempo)
    }

    #[test]
    fn echo_tap_times_follow_tempo() {
        // coarse 3/16 → 3 eighths; 120 BPM → 125 ms per eighth → 375 ms → trunc(16537.5) samples
        let (l, r) = echo_ir(3.0 / 16.0, 4.0 / 16.0, 120);
        assert_eq!(first_nonzero(&l), 16537);
        assert_eq!(first_nonzero(&r), 22050);
        let (l, _) = echo_ir(1.0 / 16.0, 1.0 / 16.0, 60); // 250 ms at 60 BPM
        assert_eq!(first_nonzero(&l), 11025);
    }

    #[test]
    fn echo_worst_case_fits_its_pool() {
        // coarse 16 + fine 200 ms at a slow tempo: loads with exactly `need` bytes and runs
        let (l, _) = {
            let mut d = load_at(kind::ECHO, &[1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0], 40);
            let x = impulse(4);
            run_at(&mut d, &x, &x, None, 40)
        };
        assert!(l.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn chamber_impulse_arrival_per_mode() {
        // shortest path to R: buffer 4 (3 ms) read by j=3 (read before its write → full length);
        // to L: buffer 0 (7 ms) read by j=7 after its write in the same sample → one sample shorter.
        for (mode, mult) in [(0.0f32, 1.0f32), (0.5, 5.0), (1.0, 10.0)] {
            let mut d = load_at(kind::CHAMBER, &[mode, 0.0, 0.0, 1.0, 1.0, 0.0], 120);
            let x = impulse(30000);
            let (l, r) = run(&mut d, &x, &x, None);
            let len = |ms: f32| ((ms * mult) as f64 * 44100.0 / 1000.0) as usize;
            assert_eq!(first_nonzero(&r), len(3.0), "mode {mode}");
            assert_eq!(first_nonzero(&l), len(7.0) - 1, "mode {mode}");
        }
    }

    #[test]
    fn chamber_dry_path_is_quarter_per_channel() {
        // dry/wet 0: each channel sums 4 × input·(1−0) then /4 → input
        let mut d = load_at(kind::CHAMBER, &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0], 120);
        let x = vec![0.5f32; 16];
        let (l, r) = run(&mut d, &x, &x, None);
        assert_eq!(l, x);
        assert_eq!(r, x);
    }

    /// Smasher: threshold −12 dB (param 2/3), ratio 2, fastest attack, gains 0 dB.
    fn smasher(sidechain: f32) -> WsDevice<'static> {
        load_at(kind::SMASHER, &[sidechain, 0.5, 2.0 / 3.0, 0.0, 0.0, 0.5, 0.5], 120)
    }

    #[test]
    fn smasher_compresses_above_threshold_after_lookahead() {
        let main = vec![0.5f32; 400];
        let (l, _) = run(&mut smasher(0.0), &main, &main, None);
        // 2 ms = 88-sample ring, written before it is read → 87 samples of delay
        assert!(l[..87].iter().all(|&v| v == 0.0) && l[87] != 0.0, "lookahead delay");
        // threshold 0.25, peak 0.5: gain = (0.25 + 0.25/2) / 0.5 = 0.75
        assert!((l[399] - 0.375).abs() < 1e-6, "{}", l[399]);
    }

    #[test]
    fn smasher_sidechain_key() {
        let main = vec![0.5f32; 400];
        let key = vec![1.0f32; 400];
        let (l, _) = run(&mut smasher(1.0), &main, &main, Some((&key, &key)));
        // keyed by the louder sidechain: gain = (0.25 + 0.75/2) / 1 = 0.625
        assert!((l[399] - 0.3125).abs() < 1e-6, "{}", l[399]);
        // quiet key below threshold: no reduction
        let quiet = vec![0.1f32; 400];
        let (l, _) = run(&mut smasher(1.0), &main, &main, Some((&quiet, &quiet)));
        assert!((l[399] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn reset_restores_fresh_state() {
        let x = impulse(30000);
        let mut d = load_at(kind::CHAMBER, &[0.0, 0.5, 0.0, 1.0, 1.0, 0.5], 120);
        let first = run(&mut d, &x, &x, None);
        d.reset();
        assert_eq!(run(&mut d, &x, &x, None), first);
    }
}

mod effect_tests_3 {
    use super::harness::*;
    use crate::*;
    use synth_core::device::Device;
    use synth_core::mix::{Env, Kinds};
    use synth_core::pool::Pool;

    fn load(kind: u8, params: &[f32]) -> WsDevice<'static> {
        let c: &'static [u8] = Box::leak(chunk(params).into_boxed_slice());
        let env = Env { rate: 44100.0, tempo: 120, bank: None };
        let need = WsDevice::need(kind, c, &env).unwrap();
        let mem: &'static mut [u8] = Box::leak(vec![0u8; need].into_boxed_slice());
        WsDevice::load(kind, c, &env, &mut Pool::new(mem), &mut Zero).unwrap()
    }

    fn impulse(n: usize) -> Vec<f32> {
        let mut x = vec![0.0; n];
        x[0] = 1.0;
        x
    }

    fn first_nonzero(x: &[f32]) -> usize {
        x.iter().position(|v| v.abs() > 1e-12).unwrap()
    }

    fn rms(x: &[f32]) -> f32 {
        libm::sqrtf(x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32)
    }

    // Cathedral params: Freeze, RoomSize, Damp, Width, LowCutFreq, HighCutFreq, DryWet, PreDelay
    #[test]
    fn cathedral_first_arrival_is_shortest_comb() {
        let mut d = load(kind::CATHEDRAL, &[0.0, 0.5, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0]);
        let x = impulse(4000);
        let (l, r) = run(&mut d, &x, &x, None);
        assert_eq!(first_nonzero(&l), 1116);
        assert_eq!(first_nonzero(&r), 1116 + 23); // right combs are 23 samples longer
    }

    #[test]
    fn cathedral_freeze_holds_energy() {
        let tail = |freeze_after: bool| {
            let mut d = load(kind::CATHEDRAL, &[0.0, 0.5, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0]);
            let x = impulse(8000);
            run(&mut d, &x, &x, None);
            if freeze_after {
                d.set_param(0, 1.0);
            }
            let silence = vec![0.0f32; 88200];
            let (l, _) = run(&mut d, &silence, &silence, None);
            rms(&l[86000..]) / rms(&l[..4410])
        };
        assert!(tail(true) > 0.5, "frozen tail keeps its level: {}", tail(true));
        assert!(tail(false) < 0.01, "unfrozen tail decays: {}", tail(false));
    }

    #[test]
    fn cathedral_predelay_shifts_arrival() {
        // pre-delay 0.1 → 50 ms = 2205-sample ring, written then read → 2204 samples more
        let mut d = load(kind::CATHEDRAL, &[0.0, 0.5, 0.0, 1.0, 0.0, 1.0, 1.0, 0.1]);
        let x = impulse(6000);
        assert_eq!(first_nonzero(&run(&mut d, &x, &x, None).0), 1116 + 2204);
    }

    // Twister params: Type, Amount, Feedback, Spread, VibratoFreq, VibratoAmount, LowCutFreq, HighCutFreq, DryWet
    fn twister_delay(amount: f32) -> usize {
        let mut d = load(kind::TWISTER, &[0.0, amount, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0]);
        let x = impulse(400);
        first_nonzero(&run(&mut d, &x, &x, None).0)
    }

    #[test]
    fn twister_sweep_range_is_2_to_134() {
        assert_eq!(twister_delay(0.0), 2);
        assert_eq!(twister_delay(1.0), 134);
        assert_eq!(twister_delay(0.5), 68);
        // fractional position 2.5: half of the impulse already shows one sample earlier
        assert_eq!(twister_delay(0.5 / 132.0), 1);
    }

    #[test]
    fn twister_modes_and_vibrato_stay_finite() {
        for ty in [0.0, 0.34, 0.67, 1.0] {
            for spread in [0.0, 0.5, 1.0] {
                let mut d = load(kind::TWISTER, &[ty, 0.5, 0.9, spread, 1.0, 1.0, 0.0, 1.0, 0.5]);
                let x = sine(440.0, 20000, 0.8);
                let (l, r) = run(&mut d, &x, &x, None);
                assert!(l.iter().chain(&r).all(|v| v.is_finite() && v.abs() < 100.0), "type {ty} spread {spread}");
            }
        }
    }
}

mod synth_tests {
    use crate::helpers::init;
    use crate::svf::{Svf, SvfKind};
    use crate::synth::{EnvState, Envelope};

    #[test]
    fn envelope_timing_in_ms() {
        let rate = 44100.0;
        let mut e = Envelope::new();
        e.set(10.0, 100.0, 0.25, 50.0);
        e.trigger();
        let mut n = 0;
        while e.state == EnvState::Attack {
            e.next(rate);
            n += 1;
        }
        assert!((441..=442).contains(&n), "{n}"); // 10 ms (f32 step accumulation may need one more)
        let mut n = 0;
        while e.state == EnvState::Decay {
            e.next(rate);
            n += 1;
        }
        assert!((4409..=4411).contains(&n), "{n}");
        assert_eq!(e.value(), 0.25);
        e.off();
        assert_eq!(e.value(), 0.25);
        let mut n = 0;
        while e.state != EnvState::Finished {
            e.next(rate);
            n += 1;
        }
        assert!((2204..=2206).contains(&n), "{n}");
        assert_eq!(e.value(), 0.0);
    }

    #[test]
    fn envelope_curves() {
        let mut e = Envelope::new();
        e.set(2.0, 4.0, 0.5, 4.0);
        e.trigger();
        assert_eq!(e.value(), 0.0);
        // attack is linear; decay quadratic toward sustain: f = (1 − pos/D)², v = f + S(1 − f)
        for _ in 0..44 {
            e.next(44100.0);
        }
        assert!((e.value() - 44.0 * (1000.0 / 44100.0) / 2.0).abs() < 1e-4);
    }

    #[test]
    fn svf_lowpass_response() {
        init();
        let gain = |hz: f32| {
            let mut f = Svf::new(SvfKind::Lowpass);
            f.set_freq(1000.0);
            let mut peak = 0.0f32;
            for i in 0..44100 {
                let y = f.next(libm::sinf(2.0 * core::f32::consts::PI * hz * i as f32 / 44100.0), 44100.0);
                if i > 22050 {
                    peak = peak.max(y.abs());
                }
            }
            20.0 * libm::log10f(peak)
        };
        assert!(gain(100.0).abs() < 0.5);
        assert!(gain(10000.0) < -30.0);
    }
}

mod instrument_tests {
    use super::harness::*;
    use crate::helpers::WsRng;
    use crate::*;
    use synth_core::device::{Ctx, Device, Io};
    use synth_core::mix::{Env, Kinds};
    use synth_core::pool::Pool;
    use synth_core::rng::Rng;

    fn load_rng(kind: u8, params: &[f32], rng: &mut dyn Rng) -> WsDevice<'static> {
        let c: &'static [u8] = Box::leak(chunk(params).into_boxed_slice());
        let env = Env { rate: 44100.0, tempo: 120, bank: None };
        let need = WsDevice::need(kind, c, &env).unwrap();
        let mem: &'static mut [u8] = Box::leak(vec![0u8; need].into_boxed_slice());
        WsDevice::load(kind, c, &env, &mut Pool::new(mem), rng).unwrap()
    }

    /// Renders `n` frames after a note-on at 0 (and optional note-off), with the song rng.
    fn play(d: &mut WsDevice, note: u8, n: usize, off: Option<u32>, rng: &mut dyn Rng) -> Vec<f32> {
        d.note_on(note, 100, 0);
        if let Some(o) = off {
            d.note_off(note, o);
        }
        let (mut a, mut b, mut c, mut e) = (vec![9.0; n], vec![9.0; n], vec![0.0; n], vec![0.0; n]);
        let mut ctx = Ctx { rate: 44100.0, tempo: 120, frame: 0, rng };
        let mut io = Io { ch: [&mut a, &mut b, &mut c, &mut e] };
        d.run(&mut io, n, &mut ctx);
        a
    }

    /// Magnitude of the `hz` component (Goertzel) over `x`.
    fn mag(x: &[f32], hz: f64) -> f64 {
        let w = 2.0 * core::f64::consts::PI * hz / 44100.0;
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for &v in x {
            let s = v as f64 + 2.0 * w.cos() * s1 - s2;
            s2 = s1;
            s1 = s;
        }
        (s1 * s1 + s2 * s2 - 2.0 * w.cos() * s1 * s2).sqrt() / x.len() as f64
    }

    /// Frequency of the strongest component near `around` (±10 Hz, 0.05 Hz steps).
    fn peak_freq(x: &[f32], around: f64) -> f64 {
        let mut best = (0.0, 0.0);
        let mut f = around - 10.0;
        while f <= around + 10.0 {
            let m = mag(x, f);
            if m > best.1 {
                best = (f, m);
            }
            f += 0.05;
        }
        best.0
    }

    // Slaughter param indices (upstream order)
    const SL: usize = 42;
    fn slaughter_params() -> Vec<f32> {
        let mut p = vec![0.0f32; SL];
        p[2] = 1.0; // osc1 volume
        p[1] = 0.5; // pulse width
        p[16] = 0.0; // LP
        p[17] = 1.0; // filter open
        p[18] = 0.0; // resonance param 0 → q 1
        p[19] = 0.5; // no filter mod
        p[20] = 0.0; // amp attack 1 ms
        p[21] = 0.0; // decay 1 ms
        p[22] = 1.0; // sustain 1
        p[23] = 0.0; // release 1 ms
        p[28] = 0.5; // master
        p[31] = 0.5; // voices pan centred
        p[39] = 0.5; // pitch env amount 0
        p
    }

    #[test]
    fn instrument_overwrites_main_and_param_counts() {
        let mut d = load_rng(kind::SLAUGHTER, &slaughter_params(), &mut WsRng::new());
        assert_eq!(d.param_count(), 42);
        let out = play(&mut d, 60, 64, None, &mut WsRng::new());
        assert!(out.iter().all(|v| *v != 9.0), "input in main is replaced");
        let d = load_rng(kind::FALCON, &[], &mut WsRng::new());
        assert_eq!(d.param_count(), 32);
    }

    #[test]
    fn slaughter_pitch_and_saw_spectrum() {
        let mut d = load_rng(kind::SLAUGHTER, &slaughter_params(), &mut WsRng::new());
        let x = play(&mut d, 69, 44100, None, &mut WsRng::new());
        let tail = &x[4410..];
        assert!((peak_freq(tail, 440.0) - 440.0).abs() < 0.5);
        // saw: harmonic n ≈ 1/n → 2nd ≈ −6 dB, 3rd ≈ −9.5 dB
        let h = |n: f64| 20.0 * (mag(tail, 440.0 * n) / mag(tail, 440.0)).log10();
        assert!((h(2.0) + 6.0).abs() < 1.5, "2nd {}", h(2.0));
        assert!((h(3.0) + 9.5).abs() < 1.5, "3rd {}", h(3.0));
    }

    #[test]
    fn slaughter_pulse_50_has_no_even_harmonics() {
        let mut p = slaughter_params();
        p[0] = 1.0; // waveform: pulse
        let mut d = load_rng(kind::SLAUGHTER, &p, &mut WsRng::new());
        let x = play(&mut d, 69, 44100, None, &mut WsRng::new());
        let tail = &x[4410..];
        let h = |n: f64| 20.0 * (mag(tail, 440.0 * n) / mag(tail, 440.0)).log10();
        assert!(h(2.0) < -30.0, "2nd {}", h(2.0));
        assert!((h(3.0) + 9.5).abs() < 1.5, "3rd {}", h(3.0));
    }

    #[test]
    fn slaughter_draws_768_phases_at_load() {
        let mut rng = WsRng::new();
        load_rng(kind::SLAUGHTER, &slaughter_params(), &mut rng);
        let mut reference = WsRng::new();
        for _ in 0..768 {
            reference.next_f32();
        }
        assert_eq!(rng.next_f32(), reference.next_f32());
    }

    #[test]
    fn slaughter_release_ends_voice() {
        let mut d = load_rng(kind::SLAUGHTER, &slaughter_params(), &mut WsRng::new());
        let x = play(&mut d, 60, 4410, Some(441), &mut WsRng::new());
        assert!(x[300] != 0.0);
        assert!(x[441 + 60..].iter().all(|v| *v == 0.0), "1 ms release then silence");
    }

    #[test]
    fn slaughter_mono_legato_slides() {
        let mut p = slaughter_params();
        p[40] = 1.0; // mono legato
        p[41] = 0.0; // instant slide
        let mut d = load_rng(kind::SLAUGHTER, &p, &mut WsRng::new());
        d.note_on(57, 100, 0);
        d.note_on(69, 100, 4410); // slides 220 → 440 Hz on the same voice
        let mut rng = WsRng::new();
        let (mut a, mut b, mut c, mut e) = (vec![0.0; 44100], vec![0.0; 44100], vec![0.0; 44100], vec![0.0; 44100]);
        let mut ctx = Ctx { rate: 44100.0, tempo: 120, frame: 0, rng: &mut rng };
        d.run(&mut Io { ch: [&mut a, &mut b, &mut c, &mut e] }, 44100, &mut ctx);
        assert!((peak_freq(&a[8820..], 440.0) - 440.0).abs() < 0.5);
    }

    // Falcon: Osc1Waveform, Osc1RatioCoarse, Osc1RatioFine, Osc1Feedback, Osc1FeedForward, Osc1 ADSR,
    // Osc2Waveform, Osc2RatioCoarse, Osc2RatioFine, Osc2Feedback, Osc2 ADSR, MasterLevel, VoicesUnisono,
    // VoicesDetune, VoicesPan, VibratoFreq, VibratoAmount, Rise, Pitch ADSR, PitchEnvAmt1/2, VoiceMode, SlideTime
    fn falcon_params(ff: f32, coarse1: f32) -> Vec<f32> {
        let mut p = vec![0.0f32; 32];
        p[1] = coarse1;
        p[2] = 0.5;
        p[4] = ff;
        p[7] = 1.0;
        p[11] = 0.5;
        p[15] = 1.0;
        p[17] = 0.8;
        p[20] = 0.5;
        p[28] = 0.5;
        p[29] = 0.5;
        p
    }

    #[test]
    fn falcon_sine_and_fm_sidebands() {
        let mut d = load_rng(kind::FALCON, &falcon_params(0.0, 0.0), &mut WsRng::new());
        let x = play(&mut d, 69, 44100, None, &mut WsRng::new());
        let tail = &x[4410..];
        assert!((peak_freq(tail, 440.0) - 440.0).abs() < 0.5);
        let rel = |t: &[f32], hz: f64| 20.0 * (mag(t, hz) / mag(t, 440.0)).log10();
        assert!(rel(tail, 880.0) < -40.0, "pure sine");
        // osc1 at ratio 2 (coarse 1/32.99 → floor 1 → ratio 2) feeding osc2: sidebands at 440 ± 880·k
        let mut d = load_rng(kind::FALCON, &falcon_params(0.3, 1.0 / 32.99 + 1e-4), &mut WsRng::new());
        let x = play(&mut d, 69, 44100, None, &mut WsRng::new());
        let tail = &x[4410..];
        assert!(rel(tail, 1320.0) > -30.0, "upper sideband {}", rel(tail, 1320.0));
        assert!(rel(tail, 880.0) < -40.0, "no even component {}", rel(tail, 880.0));
    }

    #[test]
    fn falcon_draws_one_phase_per_note_on() {
        let mut d = load_rng(kind::FALCON, &falcon_params(0.0, 0.0), &mut WsRng::new());
        let mut rng = WsRng::new();
        play(&mut d, 60, 16, None, &mut rng);
        let mut reference = WsRng::new();
        reference.next_f32();
        assert_eq!(rng.next_f32(), reference.next_f32());
    }
}

mod sampler_tests {
    use crate::helpers::init;
    use crate::sampler::{Interp, Loop, Player, Source};

    fn ramp(n: usize) -> Vec<f32> {
        (0..n).map(|i| i as f32).collect()
    }

    fn player<'a>(data: &'a [f32], lp: Loop, interp: Interp) -> Player<'a> {
        let mut p = Player::new();
        p.src = Source::F32(data);
        p.sample_loop = (0, data.len() as i32);
        p.loop_mode = lp;
        p.interp = interp;
        p
    }

    #[test]
    fn unity_pitch_plays_every_sample_then_stops() {
        init();
        let d = ramp(8);
        let mut p = player(&d, Loop::Disabled, Interp::Nearest);
        p.calc_pitch(0.0);
        p.init_pos();
        p.run_prep();
        let out: Vec<f32> = (0..8).map(|_| p.next()).collect();
        assert_eq!(out, d);
        assert!(p.active);
        assert_eq!(p.next(), 0.0);
        assert!(!p.active);
    }

    #[test]
    fn octave_up_skips_and_linear_interpolates() {
        init();
        let d = ramp(16);
        let mut p = player(&d, Loop::Disabled, Interp::Linear);
        p.calc_pitch(12.0);
        p.init_pos();
        p.run_prep();
        assert_eq!((0..4).map(|_| p.next()).collect::<Vec<_>>(), vec![0.0, 2.0, 4.0, 6.0]);
        let mut p = player(&d, Loop::Disabled, Interp::Linear);
        p.calc_pitch(-12.0); // half speed: midpoints interpolated
        p.init_pos();
        p.run_prep();
        assert_eq!((0..4).map(|_| p.next()).collect::<Vec<_>>(), vec![0.0, 0.5, 1.0, 1.5]);
    }

    #[test]
    fn repeat_loop_wraps_and_interpolates_across_the_seam() {
        init();
        let d = ramp(8);
        let mut p = player(&d, Loop::Repeat, Interp::Linear);
        p.sample_loop = (4, 4); // loop 4..8
        p.calc_pitch(-12.0);
        p.init_pos();
        p.run_prep();
        let out: Vec<f32> = (0..20).map(|_| p.next()).collect();
        // ... 7.0, then halfway between sample 7 and the loop start (4) = 5.5, then 4.0 ...
        assert_eq!(&out[14..18], &[7.0, 5.5, 4.0, 4.5]);
        assert!(p.active);
    }

    #[test]
    fn ping_pong_bounces() {
        init();
        let d = ramp(6);
        let mut p = player(&d, Loop::PingPong, Interp::Nearest);
        p.sample_loop = (2, 4);
        p.calc_pitch(0.0);
        p.init_pos();
        p.run_prep();
        let mut out = vec![];
        for _ in 0..12 {
            p.calc_pitch(0.0); // the voice recomputes the pitch every sample (direction follows the bounce)
            out.push(p.next());
        }
        assert_eq!(out, vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 5.0, 4.0, 3.0, 2.0, 2.0, 3.0]);
    }

    #[test]
    fn manual_boundary_reverse_and_start() {
        init();
        let d = ramp(10);
        let mut p = player(&d, Loop::Disabled, Interp::Nearest);
        p.reverse = true;
        p.sample_start = 0.0; // reverse starts at the end
        p.calc_pitch(0.0);
        p.init_pos();
        p.calc_pitch(0.0);
        p.run_prep();
        assert_eq!((0..3).map(|_| p.next()).collect::<Vec<_>>(), vec![9.0, 8.0, 7.0]);
        let mut p = player(&d, Loop::Repeat, Interp::Nearest);
        p.boundary_manual = Some(true);
        p.loop_start = 0.5;
        p.loop_length = 0.2; // loop 5..7
        p.sample_start = 0.5;
        p.calc_pitch(0.0);
        p.init_pos();
        p.run_prep();
        assert_eq!(&(0..8).map(|_| p.next()).collect::<Vec<_>>()[..], &[4.0, 5.0, 6.0, 5.0, 6.0, 5.0, 6.0, 5.0][..]);
    }

    #[test]
    fn empty_source_is_silent_and_inactive() {
        init();
        let mut p = player(&[], Loop::Repeat, Interp::Linear);
        p.calc_pitch(0.0);
        p.init_pos();
        p.run_prep();
        assert_eq!(p.next(), 0.0);
        assert!(!p.active);
    }
}

mod sample_instrument_tests {
    use super::harness::chunk;
    use crate::helpers::WsRng;
    use crate::*;
    use synth_core::device::{Ctx, Device, Io};
    use synth_core::mix::{Env, Kinds};
    use synth_core::pool::Pool;

    /// Upstream Specimen/Thunder chunk: header (compressed, uncompressed), WAVEFORMATEX GSM 6.10 (cbSize 2,
    /// 320 samples/block), GSM blocks, then f32 params (Specimen) and the trailing size.
    pub fn gsm_chunk(blocks: &[u8], uncompressed: i32, params: &[f32]) -> Vec<u8> {
        let mut c = vec![];
        c.extend((blocks.len() as i32).to_le_bytes());
        c.extend(uncompressed.to_le_bytes());
        c.extend(0x31u16.to_le_bytes()); // WAVE_FORMAT_GSM610
        c.extend(1u16.to_le_bytes());
        c.extend(44100u32.to_le_bytes());
        c.extend(8957u32.to_le_bytes());
        c.extend(65u16.to_le_bytes());
        c.extend(0u16.to_le_bytes());
        c.extend(2u16.to_le_bytes()); // cbSize
        c.extend(320u16.to_le_bytes());
        c.extend(blocks);
        for p in params {
            c.extend(p.to_le_bytes());
        }
        let n = c.len() as i32 + 4;
        c.extend(n.to_le_bytes());
        c
    }

    pub fn vectors() -> (&'static [u8], Vec<f32>) {
        let g: &'static [u8] = include_bytes!("../../gsm/testdata/gsm_vectors.gsm");
        let pcm = include_bytes!("../../gsm/testdata/gsm_vectors.pcm")
            .chunks_exact(2)
            .map(|b| (i16::from_le_bytes([b[0], b[1]]) as f64 / 32768.0) as f32)
            .collect();
        (g, pcm)
    }

    fn load(kind: u8, c: Vec<u8>, bank: Option<&'static [u8]>) -> WsDevice<'static> {
        let c: &'static [u8] = Box::leak(c.into_boxed_slice());
        let env = Env { rate: 44100.0, tempo: 120, bank };
        let need = WsDevice::need(kind, c, &env).unwrap();
        let mem: &'static mut [u8] = Box::leak(vec![0u8; need].into_boxed_slice());
        WsDevice::load(kind, c, &env, &mut Pool::new(mem), &mut WsRng::new()).unwrap()
    }

    fn play(d: &mut WsDevice, note: u8, vel: u8, n: usize) -> (Vec<f32>, Vec<f32>) {
        d.note_on(note, vel, 0);
        let (mut a, mut b, mut c, mut e) = (vec![9.0; n], vec![9.0; n], vec![0.0; n], vec![0.0; n]);
        let mut rng = WsRng::new();
        let mut ctx = Ctx { rate: 44100.0, tempo: 120, frame: 0, rng: &mut rng };
        d.run(&mut Io { ch: [&mut a, &mut b, &mut c, &mut e] }, n, &mut ctx);
        (a, b)
    }

    #[test]
    fn thunder_plays_decoded_sample_once() {
        let (g, pcm) = vectors();
        let mut d = load(kind::THUNDER, gsm_chunk(g, 5000, &[]), None);
        assert_eq!(d.param_count(), 0);
        let (l, r) = play(&mut d, 30, 1, 6000);
        assert_eq!(&l[..5000], &pcm[..5000], "uncompressed size caps the decoded length");
        assert_eq!(l, r);
        assert!(l[5000..].iter().all(|v| *v == 0.0));
    }

    fn specimen_params() -> Vec<f32> {
        let mut p = vec![0.0f32; 26];
        p[2] = 1.0; // amp sustain
        p[9] = 0.0; // nearest
        p[10] = 0.5; // coarse 0
        p[11] = 0.5; // fine 0
        p[13] = 1.0; // filter open
        p[15] = 0.5;
        p[18] = 1.0;
        p[22] = 0.5; // pan centre
        p[23] = 1.0; // master
        p
    }

    #[test]
    fn specimen_plays_at_note_60_with_velocity_and_master() {
        let (g, pcm) = vectors();
        let mut d = load(kind::SPECIMEN, gsm_chunk(g, 1 << 20, &specimen_params()), None);
        assert_eq!(d.param_count(), 26);
        let (l, r) = play(&mut d, 60, 64, 3000);
        // velocity 64/128, centre pan √0.5, master volume_to_scalar(1); the SVF wide open is near-transparent
        let k = 0.5 * core::f32::consts::FRAC_1_SQRT_2 * crate::helpers::volume_to_scalar(1.0);
        let want = open_lp(pcm.iter().copied());
        let err = (200..3000).map(|i| (l[i] - want[i] * k).abs()).fold(0.0, f32::max) / k;
        assert!(err < 1e-4, "max err {err}");
        assert_eq!(l, r);
    }

    #[test]
    fn specimen_octave_up_halves_duration() {
        let (g, _) = vectors();
        let n = g.len() / 65 * 320;
        let mut d = load(kind::SPECIMEN, gsm_chunk(g, 1 << 20, &specimen_params()), None);
        let (l, _) = play(&mut d, 72, 127, n);
        let last = l.iter().rposition(|v| *v != 0.0).unwrap();
        assert!((n / 2 - 2..n / 2).contains(&last), "{last} vs {}", n / 2);
    }

    #[test]
    fn specimen_without_sample_is_silent() {
        let mut d = load(kind::SPECIMEN, vec![], None);
        let (l, _) = play(&mut d, 60, 100, 64);
        assert!(l.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn malformed_gsm_chunk_is_an_error() {
        let (g, _) = vectors();
        let mut c = gsm_chunk(g, 100, &[]);
        c[0..4].copy_from_slice(&100_000i32.to_le_bytes()); // compressed size past the end
        let c: &'static [u8] = Box::leak(c.into_boxed_slice());
        let env = Env { rate: 44100.0, tempo: 120, bank: None };
        assert!(WsDevice::need(kind::THUNDER, c, &env).is_err());
        assert!(WsDevice::need(kind::SPECIMEN, &c[..10], &env).is_err());
    }

    /// `x` through the instrument's wide-open lowpass (19980 Hz, q 1), as the voice filters before its gains.
    fn open_lp(x: impl Iterator<Item = f32>) -> Vec<f32> {
        let mut f = crate::svf::Svf::new(crate::svf::SvfKind::Lowpass);
        f.set_q(1.0);
        x.map(|v| {
            f.set_freq(20000.0 - 20.0);
            f.next(v, 44100.0)
        })
        .collect()
    }

    fn adultery_params(index: f32) -> Vec<f32> {
        let mut p = vec![0.0f32; 28];
        p[0] = index;
        p[3] = 1.0;
        p[11] = 1.0; // linear
        p[12] = 0.5;
        p[13] = 0.5;
        p[15] = 1.0;
        p[17] = 0.5;
        p[20] = 1.0;
        p[24] = 0.5;
        p[25] = 1.0;
        p
    }

    #[test]
    fn adultery_without_bank_is_silent() {
        let mut d = load(kind::ADULTERY, chunk(&adultery_params(1.0)), None);
        assert_eq!(d.param_count(), 28);
        let (l, _) = play(&mut d, 60, 100, 256);
        assert!(l.iter().all(|v| *v == 0.0));
    }

    fn bank(var: &str, default: Option<&str>) -> Option<&'static [u8]> {
        let p = std::env::var(var).ok().or(default.map(String::from))?;
        std::fs::read(p).ok().map(|b| &*Box::leak(b.into_boxed_slice()))
    }

    /// Upstream walks the Windows gm.dls wave pool in file order from 0x44602; `dls` goes through the pool table.
    #[test]
    fn adultery_wave_order_matches_upstream_walk_on_windows_bank() {
        let Some(b) = bank("GM_DLS", None) else { return };
        let dls = dls::Dls::parse(b).unwrap();
        let mut at = 0x44602usize;
        for i in 0..495 {
            assert_eq!(&b[at..at + 4], b"LIST");
            let size = u32::from_le_bytes(b[at + 4..at + 8].try_into().unwrap()) as usize;
            let w = dls.wave(i).unwrap();
            let off = w.data.as_ptr() as usize - b.as_ptr() as usize;
            assert!(off > at && off + w.data.len() <= at + 8 + size, "wave {i}");
            at += 8 + size;
        }
    }

    #[test]
    fn adultery_plays_bank_wave() {
        let default = "/System/Library/Components/CoreAudio.component/Contents/Resources/gs_instruments.dls";
        let Some(b) = bank("GM_DLS", None).or_else(|| bank("NO_SUCH_VAR", Some(default))) else { return };
        let w = dls::Dls::parse(b).unwrap().wave(0).unwrap();
        let mut d = load(kind::ADULTERY, chunk(&adultery_params(1.0)), Some(b));
        let (l, _) = play(&mut d, 60, 128, 2000);
        let k = core::f32::consts::FRAC_1_SQRT_2 * crate::helpers::volume_to_scalar(1.0);
        let want = open_lp((0..w.len()).map(|i| w.sample(i)));
        let err = (200..2000.min(w.len())).map(|i| (l[i] - want[i] * k).abs()).fold(0.0, f32::max) / k;
        assert!(err < 1e-4, "max err {err}");
        assert!(l[..2000.min(w.len())].iter().any(|v| *v != 0.0));
    }
}

/// Loads every `$WS_SONG_DIR` upstream ref song with the real devices (bank from `$GM_DLS` if set) and renders 2 s
/// from the start and 2 s after a seek to 5 s: every chunk loads and output is finite. Upstream renders
/// `SabreWulf_13` (1.8 s) silent too, so one silent song is allowed.
#[test]
fn real_songs_load_with_all_devices() {
    use crate::helpers::WsRng;
    use crate::WsDevice;
    use synth_core::mix::{Env, Player, Song, DEFAULT_SUB};
    use synth_core::pool::Pool;
    let Ok(dir) = std::env::var("WS_SONG_DIR") else { return };
    let bank = std::env::var("GM_DLS").ok().map(|p| std::fs::read(p).unwrap());
    let (mut n, mut silent) = (0, vec![]);
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "bin") {
            continue;
        }
        let b = std::fs::read(&p).unwrap();
        let s = Song::parse(&b).unwrap();
        let env = Env::for_song(&s, bank.as_deref());
        let need = Player::<WsDevice, WsRng>::required_pool(&s, &env, DEFAULT_SUB)
            .unwrap_or_else(|e| panic!("{}: {e:?}", p.display()));
        let mut mem = vec![0u8; need];
        let mut pool = Pool::new(&mut mem);
        let mut pl = Player::<WsDevice, WsRng>::new(s, &env, &mut pool, WsRng::new(), DEFAULT_SUB)
            .unwrap_or_else(|e| panic!("{}: {e:?}", p.display()));
        let mut out = vec![0.0f32; 2 * 2 * 44100];
        pl.render_f32(&mut out);
        let early = out.iter().any(|v| *v != 0.0);
        assert!(out.iter().all(|v| v.is_finite()), "{}", p.display());
        pl.seek(5 * 44100);
        pl.render_f32(&mut out);
        assert!(out.iter().all(|v| v.is_finite()), "{}", p.display());
        if !early && out.iter().all(|v| *v == 0.0) {
            silent.push(p.file_name().unwrap().to_owned());
        }
        n += 1;
    }
    assert_eq!(n, 52);
    assert!(silent.len() <= 1, "silent: {silent:?}");
}
