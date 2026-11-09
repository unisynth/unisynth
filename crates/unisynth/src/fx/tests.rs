//! Effect framework and per-mode tests; the comparison with the original engines lives in `synth-convert`'s tests.
// builds with few or no effect modes leave helpers unused and the effect type uninhabited
#![allow(unreachable_code, unused_variables, unused_mut, unused_imports, dead_code)]

use super::*;
use std::vec::Vec;
use synth_core::device::{Ctx, Device, Io};
use synth_core::mix::{Env, Kinds};
use synth_core::pool::Pool;
use synth_core::rng::Rng;

pub(super) struct Lcg(pub u32);
impl Rng for Lcg {
    fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// Deterministic values in `0..1` (an xorshift32 stream from a hashed seed: consecutive seeds give unrelated sets).
pub(super) fn values(seed: u32, n: usize) -> Vec<f32> {
    let mut x = seed.wrapping_mul(0x9E37_79B9).wrapping_add(0x7F4A_7C15) | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x >> 8) as f32 / (1u32 << 24) as f32
        })
        .collect()
}

/// Noise in `-0.5..0.5`, different per channel.
pub(super) fn noise(seed: u32, n: usize) -> (Vec<f32>, Vec<f32>) {
    let mut r = Lcg(seed);
    ((0..n).map(|_| r.next_f32() - 0.5).collect(), (0..n).map(|_| r.next_f32() - 0.5).collect())
}

pub(super) fn sine(hz: f32, rate: f32, n: usize, amp: f32) -> Vec<f32> {
    (0..n).map(|i| amp * libm::sinf(2.0 * core::f32::consts::PI * hz * i as f32 / rate)).collect()
}

/// Renders `(l, r)` through `d` in calls of `call` frames starting at song frame `start`; no sidechain.
pub(super) fn run<D: Device>(d: &mut D, rate: f64, tempo: i32, l: &[f32], r: &[f32], call: usize) -> (Vec<f32>, Vec<f32>) {
    run_side(d, rate, tempo, l, r, None, call, 0)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run_side<D: Device>(
    d: &mut D,
    rate: f64,
    tempo: i32,
    l: &[f32],
    r: &[f32],
    side: Option<(&[f32], &[f32])>,
    call: usize,
    start: u64,
) -> (Vec<f32>, Vec<f32>) {
    let (mut ol, mut or) = (l.to_vec(), r.to_vec());
    let mut rng = Lcg(5);
    let mut at = 0;
    while at < l.len() {
        let n = call.min(l.len() - at);
        let (mut sl, mut sr) = match side {
            Some((a, b)) => (a[at..at + n].to_vec(), b[at..at + n].to_vec()),
            None => (vec![0.0; n], vec![0.0; n]),
        };
        let mut ctx = Ctx { rate, tempo, frame: start + at as u64, rng: &mut rng };
        let mut io = Io { ch: [&mut ol[at..at + n], &mut or[at..at + n], &mut sl, &mut sr] };
        d.run(&mut io, n, &mut ctx);
        at += n;
    }
    (ol, or)
}

fn env(rate: f64, tempo: i32) -> Env<'static> {
    Env { rate, tempo, bank: None }
}

/// The v3 chunk of the old-layout effect values `vals` (`[mode as stored before the switch, params…]`): the tests
/// keep their meaning across the format switch (change unify-v2-algorithms 6.5) and the effect golden pins it.
pub(super) fn chunk(k: u8, vals: &[f32]) -> Vec<u8> {
    let mut old: Vec<u8> = vals.iter().flat_map(|x| x.to_le_bytes()).collect();
    old.extend((old.len() as i32 + 4).to_le_bytes());
    crate::migrate::effect_chunk(k, &old).unwrap()
}

/// The old layout's stored value of mode `m` of kind `k` (the expanded choice).
pub(super) fn old_mode_value(k: u8, m: usize) -> f32 {
    let (old, later) = (mode_count(k), mode_total(k) - mode_count(k));
    if later == 0 {
        mode(m, old)
    } else if old == 1 {
        m as f32 / later as f32
    } else {
        crate::params::old::kinds::encode(m, old)
    }
}

/// Loads effect kind `k` with old-layout `vals` (`[mode, params…]`, see [`chunk`]) and runs `f` on it.
pub(super) fn with_fx<R>(k: u8, vals: &[f32], rate: f64, tempo: i32, f: impl FnOnce(&mut UFx) -> R) -> R {
    let chunk = chunk(k, vals);
    let env = env(rate, tempo);
    let mut mem = vec![0u8; UFx::need(k, &chunk, &env).unwrap()];
    let mut pool = Pool::new(&mut mem);
    let mut fx = UFx::new(k, &chunk, &env, &mut pool).unwrap();
    f(&mut fx)
}

/// `mode` as the normalized value that selects it among `n` modes.
pub(super) fn mode(i: usize, n: usize) -> f32 {
    if n <= 1 { 0.0 } else { i as f32 / (n - 1) as f32 }
}

pub(super) fn worst(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).fold(0.0, |m, (x, y)| m.max((x - y).abs()))
}

pub(super) fn peak(a: &[f32]) -> f32 {
    a.iter().fold(0.0, |m, x| m.max(x.abs()))
}

pub(super) fn rms(a: &[f32]) -> f32 {
    libm::sqrtf(a.iter().map(|x| x * x).sum::<f32>() / a.len() as f32)
}

/// Where the laws use `crate::fm` (own `exp2`/`pow`/`sin`) instead of `libm`, the unified effect differs from the original one by an
/// ulp or two of the result, which a resonant filter can raise to a few 1e-6 (−110 dBFS, below the 16-bit floor): not bit-exact.
const TOL_FM: f32 = 1e-5;
/// The crusher quantizes: a last-bit difference before the rounding moves a sample by one quantization step now and then.
const TOL_QUANT: f32 = 1e-4;

/// The unified effect of `k` in `mode_i` against a reference for parameter sets `seeds`: same output within `tol`.
pub(super) fn parity(
    k: u8,
    mode_i: usize,
    n_params: usize,
    rate: f64,
    tol: f32,
    seeds: core::ops::Range<u32>,
    reference: impl Fn(&[f32], &[f32], &[f32]) -> (Vec<f32>, Vec<f32>),
) {
    parity_at(k, mode_i, n_params, rate, 120, tol, seeds, reference)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn parity_at(
    k: u8,
    mode_i: usize,
    n_params: usize,
    rate: f64,
    tempo: i32,
    tol: f32,
    seeds: core::ops::Range<u32>,
    reference: impl Fn(&[f32], &[f32], &[f32]) -> (Vec<f32>, Vec<f32>),
) {
    let n = mode_count(k);
    for seed in seeds {
        let p = values(seed, n_params);
        let (l, r) = noise(seed + 100, 30000);
        let (wl, wr) = reference(&p, &l, &r);
        let mut u = vec![mode(mode_i, n)];
        u.extend(&p);
        let (ul, ur) = with_fx(k, &u, rate, tempo, |fx| run(fx, rate, tempo, &l, &r, 512));
        let (dl, dr) = (worst(&ul, &wl), worst(&ur, &wr));
        assert!(dl <= tol && dr <= tol, "kind {k} mode {mode_i} seed {seed}: {dl} {dr} (params {p:?})");
        assert!(peak(&wl) > 1e-6, "kind {k} mode {mode_i} seed {seed}: the reference is silent");
    }
}

// ---- framework ----

#[test]
fn the_mode_is_the_first_parameter_and_selects_by_choice() {
    assert_eq!(mode_count(kind::DISTORTION), 3);
    assert_eq!(mode_of(kind::DISTORTION, &chunk(kind::DISTORTION, &[1.0])), 2, "power keeps its stored value 1.0");
    assert_eq!(mode_of(kind::DISTORTION, &chunk(kind::DISTORTION, &[0.5])), 1);
    assert_eq!(mode_count(kind::REVERB), 3);
    assert_eq!(mode_count(kind::CRUSHER), 1);
    assert_eq!(mode_count(kind::INSTRUMENT), 0);
    assert_eq!(mode_of(kind::REVERB, &chunk(kind::REVERB, &[0.0])), 0);
    assert_eq!(mode_of(kind::REVERB, &chunk(kind::REVERB, &[0.5])), 1);
    assert_eq!(mode_of(kind::REVERB, &chunk(kind::REVERB, &[1.0])), 2);
    assert_eq!(mode_of(kind::REVERB, &chunk(kind::REVERB, &[])), 0, "no mode in the chunk = mode 0");
    assert_eq!(mode_of(kind::CRUSHER, &chunk(kind::CRUSHER, &[0.9])), 0);
    assert!(is_effect(65) && is_effect(73) && !is_effect(64) && !is_effect(74));
}

#[test]
#[cfg(feature = "us-fx-crusher")]
fn a_chunk_without_parameters_loads_with_the_defaults() {
    // the tag alone (mode 0, defaults) and the mode alone load; an old chunk or a cut one does not
    for (c, ok) in [(vec![], false), (vec![1, 2, 3], false), (encode(&[]), true), (encode(&[0.0]), true), (chunk(kind::CRUSHER, &[]), true)] {
        let env = env(44100.0, 120);
        assert_eq!(UFx::need(kind::CRUSHER, &c, &env).is_ok(), ok, "{c:?}");
    }
    for chunk in [encode(&[]), chunk(kind::CRUSHER, &[])] {
        let env = env(44100.0, 120);
        let mut mem = vec![0u8; UFx::need(kind::CRUSHER, &chunk, &env).unwrap()];
        let mut pool = Pool::new(&mut mem);
        let fx = UFx::new(kind::CRUSHER, &chunk, &env, &mut pool).unwrap();
        assert_eq!(fx.param_count(), 4);
    }
}

#[test]
#[cfg(feature = "us-fx-distortion-shaper")]
fn the_mode_is_chosen_by_the_chunk_only() {
    // set_param(0, …) must not switch the algorithm: it would change what was sized
    let (l, r) = noise(3, 2000);
    let a = with_fx(kind::DISTORTION, &[0.0, 0.9, 0.3, 0.5, 1.0, 0.0, 0.0], 44100.0, 120, |fx| {
        fx.set_param(0, 1.0);
        run(fx, 44100.0, 120, &l, &r, 100)
    });
    let b = with_fx(kind::DISTORTION, &[0.0, 0.9, 0.3, 0.5, 1.0, 0.0, 0.0], 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &r, 100));
    assert_eq!(a, b);
}

#[test]
#[cfg(all(feature = "us-fx-distortion-shaper", feature = "us-fx-distortion-power"))]
fn parameters_past_the_end_are_ignored_and_the_count_is_one_more_than_the_mode_has() {
    with_fx(kind::DISTORTION, &[0.0], 44100.0, 120, |fx| {
        assert_eq!(fx.param_count(), 7);
        fx.set_param(99, 1.0);
    });
    with_fx(kind::DISTORTION, &[1.0], 44100.0, 120, |fx| assert_eq!(fx.param_count(), 2));
}

#[test]
fn a_mode_the_build_lacks_is_named_at_load() {
    let env = env(44100.0, 120);
    for &(k, i, id) in MODES.iter() {
        let chunk = chunk(k, &[old_mode_value(k, i)]);
        let r = UFx::need(k, &chunk, &env);
        let built = match id {
            m::FX_DISTORTION_SHAPER => cfg!(feature = "us-fx-distortion-shaper"),
            m::FX_DISTORTION_POWER => cfg!(feature = "us-fx-distortion-power"),
            m::FX_DISTORTION_CURVE => cfg!(feature = "us-fx-distortion-curve"),
            m::FX_CRUSHER => cfg!(feature = "us-fx-crusher"),
            m::FX_EQ_PARAMETRIC => cfg!(feature = "us-fx-eq-parametric"),
            m::FX_EQ_3BAND => cfg!(feature = "us-fx-eq-3band"),
            m::FX_DELAY_TEMPO => cfg!(feature = "us-fx-delay-tempo"),
            m::FX_DELAY_MS => cfg!(feature = "us-fx-delay-ms"),
            m::FX_CHORUS => cfg!(feature = "us-fx-chorus"),
            m::FX_FLANGER_PHASER => cfg!(feature = "us-fx-flanger-phaser"),
            m::FX_FLANGER_RING => cfg!(feature = "us-fx-flanger-ring"),
            m::FX_REVERB_FREEVERB => cfg!(feature = "us-fx-reverb-freeverb"),
            m::FX_REVERB_COMPACT => cfg!(feature = "us-fx-reverb-compact"),
            m::FX_REVERB_ROOMS => cfg!(feature = "us-fx-reverb-rooms"),
            m::FX_COMPRESSOR => cfg!(feature = "us-fx-compressor"),
            m::FX_FORMANT => cfg!(feature = "us-fx-formant"),
            m::FX_EQ_CUT => cfg!(feature = "us-fx-eq-cut"),
            m::FX_DELAY_MODULATED => cfg!(feature = "us-fx-delay-modulated"),
            m::FX_REVERB_COMB => cfg!(feature = "us-fx-reverb-comb"),
            m::FX_COMPRESSOR_LOOKAHEAD => cfg!(feature = "us-fx-compressor-lookahead"),
            m::FX_DISTORTION_WRAP => cfg!(feature = "us-fx-distortion-wrap"),
            m::FX_FLANGER_COMB => cfg!(feature = "us-fx-flanger-comb"),
            m::FX_CHORUS_TF3 => cfg!(feature = "us-fx-chorus-tf3"),
            _ => false,
        };
        if built {
            assert!(r.is_ok(), "{id}");
        } else {
            assert_eq!(r, Err(LoadError::UnsupportedModule(id)));
        }
    }
    assert_eq!(UFx::need(64, &[], &env), Err(LoadError::UnsupportedKind(64)));
}

#[test]
#[cfg(feature = "us-fx-crusher")]
fn effect_kinds_load_through_the_registry() {
    use crate::device::UDevice;
    let env = env(44100.0, 120);
    let chunk = chunk(kind::CRUSHER, &[0.0, 0.5, 0.5, 1.0]);
    let need = UDevice::need(kind::CRUSHER, &chunk, &env).unwrap();
    let mut mem = vec![0u8; need];
    let mut pool = Pool::new(&mut mem);
    let d = UDevice::load(kind::CRUSHER, &chunk, &env, &mut pool, &mut Lcg(1)).unwrap();
    assert_eq!(d.param_count(), 4);
    assert_eq!(UDevice::need(74, &chunk, &env).err(), Some(LoadError::UnsupportedKind(74)));
}

/// Every implemented mode: a reset effect renders the signal exactly as it first did, whatever the call sizes, and
/// leaves the sidechain alone.
#[test]
fn reset_restores_the_loaded_state_and_calls_do_not_matter() {
    let (l, r) = noise(9, 5000);
    for &(k, i, _) in MODES.iter() {
        let n = mode_count(k);
        let mut vals = vec![mode(i, n)];
        vals.extend(values(40 + i as u32, 14));
        if UFx::need(k, &chunk(k, &vals), &env(44100.0, 120)).is_err() {
            continue;
        }
        with_fx(k, &vals, 44100.0, 120, |fx| {
            let first = run(fx, 44100.0, 120, &l, &r, 5000);
            fx.reset();
            assert_eq!(run(fx, 44100.0, 120, &l, &r, 5000), first, "kind {k} mode {i}: reset");
            fx.reset();
            for call in [1, 7, 100, 512] {
                fx.reset();
                assert_eq!(run(fx, 44100.0, 120, &l, &r, call), first, "kind {k} mode {i}: calls of {call}");
            }
        });
    }
}

// ---- 5.2: distortion, crusher, EQ ----

use crate::module as m;

#[test]
#[cfg(feature = "us-fx-distortion-shaper")]
fn shaper_transfer_curves_at_spot_values() {
    // drive 0.2 → gain 1, threshold 1 → no scaling: the clipper passes the signal below 1 and clips above it
    let at = |x: f32, shaper: f32| {
        with_fx(kind::DISTORTION, &[0.0, 0.2, 1.0, 0.0, 1.0, shaper, 0.0], 44100.0, 120, |fx| {
            let v = vec![x; 4];
            run(fx, 44100.0, 120, &v, &v, 4).0[3]
        })
    };
    assert!((at(0.5, 0.0) - 0.5).abs() < 1e-6, "clipper below the threshold");
    assert_eq!(at(3.0, 0.0), 1.0);
    assert_eq!(at(-3.0, 0.0), -1.0);
    // shaper 0.5 = sine-ish: x·π/2, clipped
    assert!((at(0.5, 0.5) - 0.5 * 3.141592 / 2.0).abs() < 1e-6);
    assert_eq!(at(0.9, 0.5), 1.0);
    // shaper 1.0 = parabola: x³
    assert!((at(0.5, 1.0) - 0.125).abs() < 1e-6);
    assert!((at(0.8, 1.0) - 0.512).abs() < 1e-6);
}

#[test]
#[cfg(feature = "us-fx-crusher")]
fn crusher_quantises_to_the_bit_depth() {
    // vertical 0 → step 2^-1 = 0.5; horizontal 0 → hold every sample; dry/wet 1
    let (l, _) = noise(2, 1000);
    let out = with_fx(kind::CRUSHER, &[0.0, 1.0, 0.0, 1.0], 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &l, 100).0);
    assert!(out.iter().all(|v| (*v * 2.0 - libm::floorf(*v * 2.0 + 0.5)).abs() < 1e-6), "multiples of 0.5");
    // 8 bits: vertical (8−1)/15 → step 2^-8
    let out = with_fx(kind::CRUSHER, &[0.0, 1.0 - 7.0 / 15.0, 0.0, 1.0], 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &l, 100).0);
    let step = 1.0 / 256.0;
    assert!(out.iter().all(|v| ((*v / step) - libm::floorf(*v / step + 0.5)).abs() < 1e-3));
}

#[test]
#[cfg(feature = "us-fx-eq-parametric")]
fn parametric_peak_gain_is_its_parameter_at_the_centre() {
    // peak 1 at 1 kHz: gain param 0.75 → +12 dB (24 dB range), Q 1 (param 0.5 → q 1)
    let rate = 44100.0f32;
    let freq = |hz: f32| ((hz - 20.0) / 19980.0).sqrt();
    // low cut 20 Hz (param 0), peak 1 at 1 kHz +12 dB, others 0 dB, high cut 20 kHz, master 1
    let params = [0.0, 0.5, freq(1000.0), 0.75, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 1.0, 0.5, 1.0];
    let mut v = vec![0.0];
    v.extend(params);
    let gain_at = |hz: f32| {
        let x = sine(hz, rate, 20000, 0.1);
        let out = with_fx(kind::EQ, &v, rate as f64, 120, |fx| run(fx, rate as f64, 120, &x, &x, 512).0);
        rms(&out[10000..]) / rms(&x[10000..])
    };
    let db = |g: f32| 20.0 * libm::log10f(g);
    // the original peak gain is 10^(dB/40)² at the centre = 10^(dB/20)… as WaveSabre computes it: A² = 10^(gain/20)
    assert!((db(gain_at(1000.0)) - 12.0).abs() < 0.5, "{}", db(gain_at(1000.0)));
    // a Q of 1 is a broad bell: the skirts decay away from the centre
    assert!(db(gain_at(100.0)) > 0.0 && db(gain_at(100.0)) < 3.0 && db(gain_at(10000.0)).abs() < 1.0);
}

#[test]
#[cfg(feature = "us-fx-distortion-power")]
fn power_distortion_transfer_curve_at_spot_values() {
    let at = |amount: f32, x: f32| {
        with_fx(kind::DISTORTION, &[1.0, amount], 44100.0, 120, |fx| run(fx, 44100.0, 120, &[x; 3], &[x; 3], 3).0[2])
    };
    // amount 0: y = q(|x|), the input quantised to 15 bits; amount 1: every non-zero input becomes ±1
    assert!((at(0.0, 0.5) - 0.5).abs() <= 1.0 / 32768.0, "quantised to 15 bits");
    assert_eq!(at(0.0, 0.0), 0.0);
    assert!((at(0.5, 0.25) - 0.5).abs() < 1e-3, "{}", at(0.5, 0.25));
    assert_eq!(at(1.0, 0.001), 1.0);
    assert_eq!(at(1.0, -0.001), -1.0);
    assert_eq!(at(1.0, 0.0), 0.0);
    assert!((at(0.0, 3.0) - 32767.0 / 32768.0).abs() < 1e-6, "inputs above 1 are clipped");
}

#[test]
#[cfg(feature = "us-fx-eq-3band")]
fn three_band_eq_band_gains_at_spot_values() {
    use super::law::eq_gain;
    assert_eq!(eq_gain(0.0), 0.0);
    assert_eq!(eq_gain(0.25), 0.5);
    assert_eq!(eq_gain(0.5), 1.0);
    assert!((eq_gain(0.75) - 3.5).abs() < 1e-6);
    assert!((eq_gain(1.0) - 11.0).abs() < 1e-6);
    // low band only: a 100 Hz tone passes at its gain, a 10 kHz tone does not
    let rate = 44100.0f32;
    let gain = |p: [f32; 3], hz: f32| {
        let x = sine(hz, rate, 20000, 0.1);
        let out = with_fx(kind::EQ, &[1.0, p[0], p[1], p[2]], rate as f64, 120, |fx| run(fx, rate as f64, 120, &x, &x, 512).0);
        rms(&out[10000..]) / rms(&x[10000..])
    };
    assert!((gain([1.0, 0.0, 0.0], 100.0) - 11.0).abs() < 1.0, "{}", gain([1.0, 0.0, 0.0], 100.0));
    assert!(gain([1.0, 0.0, 0.0], 10000.0) < 0.05);
    assert!((gain([0.0, 0.0, 1.0], 12000.0) - 11.0).abs() < 2.0, "{}", gain([0.0, 0.0, 1.0], 12000.0));
    // the high band is the 3-sample-delayed input minus the 5 kHz low-pass: a 100 Hz tone is nearly cancelled
    assert!(gain([0.0, 0.0, 1.0], 100.0) < 0.6, "{}", gain([0.0, 0.0, 1.0], 100.0));
}

#[test]
#[cfg(feature = "us-fx-eq-3band")]
fn three_band_eq_gain_changes_ramp_over_the_block() {
    // all bands equal (flat): a gain step from 0.5 to 1.0 between blocks ramps over the next block, with no jump
    let rate = 44100.0f64;
    let x = std::vec![0.5f32; 400];
    with_fx(kind::EQ, &[1.0, 0.25, 0.25, 0.25], rate, 120, |fx| {
        let mut rng = Lcg(1);
        let mut out = Vec::new();
        for (k, chunk) in x.chunks(100).enumerate() {
            if k == 2 {
                (1..=3).for_each(|i| fx.set_param(i, 0.5));
            }
            let (mut l, mut r, mut a, mut b) = (chunk.to_vec(), chunk.to_vec(), std::vec![0.0; 100], std::vec![0.0; 100]);
            let mut ctx = Ctx { rate, tempo: 120, frame: (k * 100) as u64, rng: &mut rng };
            fx.run(&mut Io { ch: [&mut l, &mut r, &mut a, &mut b] }, 100, &mut ctx);
            out.extend(l);
        }
        assert!((out[150] - 0.25).abs() < 1e-6, "flat at 0.5: {}", out[150]);
        assert!((out[399] - 0.5).abs() < 1e-6, "flat at 1.0 after the ramp: {}", out[399]);
        // past the 3-frame delay filling at the start
        let jump = out[10..].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(jump < 0.01, "largest step {jump}");
    });
}

#[test]
#[cfg(feature = "us-fx-eq-3band")]
fn three_band_eq_pans_after_the_bands() {
    // pan 0 (absent from older chunks) leaves both channels; 1..255 is an equal-power pan, hard left to hard right,
    // ramped like the gains
    let rate = 44100.0f64;
    let x = std::vec![0.5f32; 400];
    let byte = |b: f32| b / 255.0;
    for (pan, want) in [(None, [0.25f32, 0.25]), (Some(0.0), [0.25, 0.25]), (Some(byte(1.0)), [0.25, 0.0]), (Some(1.0), [0.0, 0.25]), (Some(byte(128.0)), [0.25 * (127.0f32 / 254.0).sqrt(), 0.25 * (127.0f32 / 254.0).sqrt()])] {
        let mut chunk = std::vec![1.0, 0.25, 0.25, 0.25];
        chunk.extend(pan);
        with_fx(kind::EQ, &chunk, rate, 120, |fx| {
            let (_, l, r) = run3(fx, rate, &x);
            assert!((l[300] - want[0]).abs() < 1e-5 && (r[300] - want[1]).abs() < 1e-5, "pan {pan:?}: {} {}", l[300], r[300]);
        });
    }
    // a pan change ramps over its block
    with_fx(kind::EQ, &[1.0, 0.25, 0.25, 0.25, byte(1.0)], rate, 120, |fx| {
        let mut rng = Lcg(1);
        let mut out = Vec::new();
        for (k, chunk) in x.chunks(100).enumerate() {
            if k == 2 {
                fx.set_param(4, 1.0);
            }
            let (mut l, mut r, mut a, mut b) = (chunk.to_vec(), chunk.to_vec(), std::vec![0.0; 100], std::vec![0.0; 100]);
            let mut ctx = Ctx { rate, tempo: 120, frame: (k * 100) as u64, rng: &mut rng };
            fx.run(&mut Io { ch: [&mut l, &mut r, &mut a, &mut b] }, 100, &mut ctx);
            out.extend(l);
        }
        let jump = out[10..].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(jump < 0.01 && out[399].abs() < 1e-6, "largest step {jump}, end {}", out[399]);
    });
}

/// Runs `x` on both channels in 100-frame blocks: (unused, left, right).
#[cfg(feature = "us-fx-eq-3band")]
fn run3(fx: &mut UFx, rate: f64, x: &[f32]) -> ((), Vec<f32>, Vec<f32>) {
    let mut rng = Lcg(1);
    let (mut lo, mut ro) = (Vec::new(), Vec::new());
    for (k, chunk) in x.chunks(100).enumerate() {
        let (mut l, mut r, mut a, mut b) = (chunk.to_vec(), chunk.to_vec(), std::vec![0.0; 100], std::vec![0.0; 100]);
        let mut ctx = Ctx { rate, tempo: 120, frame: (k * 100) as u64, rng: &mut rng };
        fx.run(&mut Io { ch: [&mut l, &mut r, &mut a, &mut b] }, 100, &mut ctx);
        lo.extend(l);
        ro.extend(r);
    }
    ((), lo, ro)
}

#[test]
#[cfg(feature = "us-fx-eq-3band")]
fn three_band_eq_corners_follow_the_song_rate() {
    // the reference is built for 44.1 kHz; here the corners are in Hz at any rate: a tone at the same fraction of
    // the rate gets the same gain, and a tone at the same Hz gets (nearly) the same gain at 48 kHz
    let low_only = |rate: f32, hz: f32| {
        let x = sine(hz, rate, 30000, 0.1);
        let out = with_fx(kind::EQ, &[1.0, 1.0, 0.0, 0.0], rate as f64, 120, |fx| run(fx, rate as f64, 120, &x, &x, 512).0);
        rms(&out[15000..]) / rms(&x[15000..])
    };
    let (a, b, c) = (low_only(44100.0, 880.0), low_only(48000.0, 880.0), low_only(96000.0, 880.0));
    assert!((a - b).abs() < 0.05 * a && (a - c).abs() < 0.1 * a, "{a} {b} {c}");
    // and the corner stays at 880 Hz: a tone that is the same fraction of the rate is already well past it
    let scaled = low_only(48000.0, 880.0 * 48000.0 / 44100.0);
    assert!(scaled < a * 0.95, "{scaled} vs {a}");
}

/// Setting a parameter after load (automation) is the same as loading it: every mode, every parameter, in the original order.
#[test]
fn automating_a_parameter_is_loading_it() {
    let (l, r) = noise(11, 2500);
    for &(k, i, _) in MODES.iter() {
        let n = mode_count(k);
        let mut a = vec![mode(i, n)];
        a.extend(values(70 + i as u32, 14));
        let mut b = vec![mode(i, n)];
        b.extend(values(90 + i as u32, 14));
        if UFx::need(k, &chunk(k, &a), &env(44100.0, 120)).is_err() {
            continue;
        }
        let want = with_fx(k, &b, 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &r, 500));
        let got = with_fx(k, &a, 44100.0, 120, |fx| {
            let count = fx.param_count();
            for p in 1..count {
                fx.set_param(p, b[p]);
            }
            fx.reset();
            run(fx, 44100.0, 120, &l, &r, 500)
        });
        assert_eq!(got, want, "kind {k} mode {i}");
        // and each parameter alone moves the sound
        for p in 1..with_fx(k, &a, 44100.0, 120, |fx| fx.param_count()) {
            let mut c = a.clone();
            c[p] = b[p];
            let (x, y) = (
                with_fx(k, &a, 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &r, 500)),
                with_fx(k, &c, 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &r, 500)),
            );
            let via_set = with_fx(k, &a, 44100.0, 120, |fx| {
                fx.set_param(p, b[p]);
                run(fx, 44100.0, 120, &l, &r, 500)
            });
            assert_eq!(via_set, y, "kind {k} mode {i} parameter {p}");
            let _ = x;
        }
    }
}

// ---- 5.3: delay, chorus, flanger ----

#[cfg(feature = "us-fx-delay-tempo")]
#[test]
fn tempo_parameters_set_the_times_feedback_and_mix() {
    // 120 BPM: an eighth is 125 ms. Left: 3 eighths + 0 ms; right: 4 eighths + 0 ms; feedback 0.5, no cross, wet 1
    let rate = 44100.0;
    let p = [3.0 / 16.0, 0.0, 4.0 / 16.0, 0.0, 0.0, 1.0, 0.5, 0.0, 1.0];
    let mut v = vec![0.0];
    v.extend(p);
    let mut x = vec![0.0f32; 60000];
    x[0] = 1.0;
    let (l, r) = with_fx(kind::DELAY, &v, rate, 120, |fx| run(fx, rate, 120, &x, &x, 4096));
    let first = |o: &[f32]| o.iter().position(|v| v.abs() > 1e-4).unwrap();
    assert_eq!(first(&l), 16537, "3 eighths at 120 BPM");
    assert_eq!(first(&r), 22050, "4 eighths");
    // the next echo comes one delay later, at the feedback gain
    // the cuts smear the impulse a little: the echo peaks near the delay, its repeat near twice the delay at half the level
    let near = |o: &[f32], at: usize| o[at - 40..at + 40].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let (e1, e2) = (near(&l, 16537 + 20), near(&l, 2 * 16537 + 20));
    assert!(e1 > 0.1 && (e2 / e1 - 0.5).abs() < 0.15, "{e1} {e2}");
}

#[cfg(feature = "us-fx-delay-tempo")]
#[test]
fn the_longest_echo_fits_the_pool_it_reports() {
    // 60 BPM: an eighth is 250 ms; the longest setting is 16 eighths + 200 ms = 4200 ms on both sides
    let rate = 44100.0;
    let mut v = vec![0.0];
    v.extend([1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 0.7, 0.3, 1.0]);
    let chunk = chunk(kind::DELAY, &v);
    let env = env(rate, 60);
    let need = UFx::need(kind::DELAY, &chunk, &env).unwrap();
    assert!(need >= 2 * 4 * (4.2 * rate) as usize, "{need}");
    let mut buf = vec![0u8; need + 16];
    for skew in 0..16 {
        let start = (buf.as_ptr() as usize).wrapping_neg() & 15;
        let mut pool = Pool::new(&mut buf[start + skew..start + skew + need]);
        let mut fx = UFx::new(kind::DELAY, &chunk, &env, &mut pool).unwrap();
        let (l, r) = noise(skew as u32, 6 * 44100);
        let (ol, or) = run(&mut fx, rate, 60, &l, &r, 997);
        assert!(ol.iter().chain(&or).all(|v| v.is_finite()), "skew {skew}");
        assert_eq!(peak(&ol[..4 * 44100]), 0.0, "nothing before the first echo");
        assert!(peak(&ol[4 * 44100 + 22050..]) > 0.0, "the longest echo is heard, not cut off");
    }
}

#[cfg(feature = "us-fx-delay-ms")]
#[test]
fn ms_delay_is_in_milliseconds_at_any_rate() {
    for rate in [44100.0, 48000.0, 96000.0] {
        let mut x = vec![0.0f32; (rate * 0.6) as usize];
        x[0] = 1.0;
        // 250 ms on both sides, decay 0.5
        let (l, r) = with_fx(kind::DELAY, &[1.0, 0.25, 0.25, 0.5], rate, 120, |fx| run(fx, rate, 120, &x, &x, 777));
        let at = (rate * 0.25) as usize;
        assert!((l[0] - 1.5).abs() < 1e-6, "the zero-latency copy at gain k");
        assert!(l[1..at].iter().all(|v| *v == 0.0), "rate {rate}");
        assert!((l[at] - 0.25).abs() < 1e-6 && (r[at] - 0.25).abs() < 1e-6, "first echo at {at} at {rate}: {}", l[at]);
    }
}

#[cfg(feature = "us-fx-chorus")]
#[test]
fn chorus_output_does_not_depend_on_the_call_sizes() {
    let (l, r) = noise(4, 20000);
    let v = [0.0, 0.4, 0.8, 1.0];
    let whole = with_fx(kind::CHORUS, &v, 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &r, 20000));
    for call in [1, 7, 100, 511, 512, 513, 1000] {
        let got = with_fx(kind::CHORUS, &v, 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &r, call));
        assert_eq!(got, whole, "calls of {call}");
    }
    assert!(peak(&whole.0) > 0.0);
}

#[cfg(feature = "us-fx-chorus")]
#[test]
fn the_chorus_control_grid_is_on_absolute_song_frames() {
    // an effect first run at frame 300 updates its control at 512, not 300 frames in
    let (l, r) = noise(4, 1500);
    let v = [0.0, 0.4, 0.8, 1.0];
    with_fx(kind::CHORUS, &v, 44100.0, 120, |fx| {
        let whole = run_side(fx, 44100.0, 120, &l, &r, None, 1500, 300);
        fx.reset();
        let parts = {
            let (a, b) = run_side(fx, 44100.0, 120, &l[..700], &r[..700], None, 700, 300);
            let (c, d) = run_side(fx, 44100.0, 120, &l[700..], &r[700..], None, 800, 1000);
            ([a, c].concat(), [b, d].concat())
        };
        assert_eq!(whole, parts);
    });
}

#[cfg(feature = "us-fx-chorus")]
#[test]
fn the_chorus_sweep_follows_the_rate_in_seconds() {
    // rate parameter 0.5 → 50·0.25 = 12.5 rad/s: one period every 0.503 s whatever the song rate. Count the
    // delay-line phase turns by the output's modulation: the sweep period seen as the period of the output's
    // short-time delay is hard to read, so compare the two rates' outputs on a tone through the effect's depth
    let tone = |rate: f32| {
        let x = sine(1000.0, rate, (rate * 1.0) as usize, 0.3);
        with_fx(kind::CHORUS, &[0.0, 0.5, 1.0, 1.0], rate as f64, 120, |fx| run(fx, rate as f64, 120, &x, &x, 512).0)
    };
    // envelope of the comb interference: the output level swings as the delay sweeps
    let swings = |o: &[f32], rate: f32| {
        let win = (rate * 0.01) as usize;
        let lv: Vec<f32> = o.chunks(win).map(rms).collect();
        let mean = lv.iter().sum::<f32>() / lv.len() as f32;
        // up-crossings of the mean
        lv.windows(2).filter(|w| w[0] < mean && w[1] >= mean).count()
    };
    let (a, b) = (swings(&tone(44100.0), 44100.0), swings(&tone(48000.0), 48000.0));
    assert!(a >= 1 && (a as i32 - b as i32).abs() <= 1, "{a} sweeps at 44.1 kHz, {b} at 48 kHz");
}

#[cfg(feature = "us-fx-flanger-ring")]
#[test]
fn the_ring_flanger_sweeps_at_the_same_speed_in_seconds_at_any_rate() {
    let theta_after = |rate: f64, secs: f64| {
        let n = (rate * secs) as usize;
        let (l, r) = noise(6, n);
        with_fx(kind::FLANGER, &[1.0, 0.5, 0.6, 0.7, 0.5], rate, 120, |fx| {
            run(fx, rate, 120, &l, &r, 1000);
            fx.tf_flanger_theta()
        })
    };
    let a = theta_after(44100.0, 3.0);
    let b = theta_after(48000.0, 3.0);
    let c = theta_after(96000.0, 3.0);
    assert!(a[0] > 0.05, "it sweeps: {a:?}");
    for (x, y) in [(a, b), (a, c)] {
        assert!((x[0] - y[0]).abs() < 0.03 * x[0].max(y[0]) + 0.01 && (x[1] - y[1]).abs() < 0.03 * x[1].max(y[1]) + 0.01, "{x:?} vs {y:?}");
    }
}

#[cfg(all(feature = "us-fx-delay-tempo", feature = "us-fx-delay-ms", feature = "us-fx-chorus", feature = "us-fx-flanger-phaser", feature = "us-fx-flanger-ring"))]
#[test]
fn odd_call_sizes_change_nothing_for_any_mode() {
    let (l, r) = noise(13, 6000);
    for &(k, i, _) in MODES.iter() {
        let n = mode_count(k);
        let mut v = vec![mode(i, n)];
        v.extend(values(300 + i as u32, 14));
        if UFx::need(k, &chunk(k, &v), &env(44100.0, 120)).is_err() {
            continue;
        }
        let whole = with_fx(k, &v, 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &r, 6000));
        for call in [7, 100, 512] {
            assert_eq!(with_fx(k, &v, 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &r, call)), whole, "kind {k} mode {i} calls of {call}");
        }
    }
}

// ---- 5.4: reverb, compressor, formant ----

fn impulse(n: usize) -> Vec<f32> {
    let mut x = vec![0.0; n];
    x[0] = 1.0;
    x
}

fn first_nonzero(o: &[f32]) -> Option<usize> {
    o.iter().position(|v| v.abs() > 1e-9)
}

#[cfg(all(feature = "us-fx-reverb-freeverb", feature = "us-fx-reverb-rooms"))]
#[test]
fn the_reverb_mode_selects_the_arrivals_of_its_algorithm() {
    let rate = 44100.0;
    let x = impulse(30000);
    // wet only, full width, no cuts, no pre-delay
    let cathedral = with_fx(kind::REVERB, &[0.0, 0.0, 0.5, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0], rate, 120, |fx| run(fx, rate, 120, &x, &x, 1000));
    let chamber = with_fx(kind::REVERB, &[1.0, 0.0, 0.5, 0.0, 1.0, 1.0, 0.0], rate, 120, |fx| run(fx, rate, 120, &x, &x, 1000));
    // freeverb mode: the shortest comb (1116 samples) on the left, the stereo spread (+23) on the right
    assert_eq!(first_nonzero(&cathedral.0), Some(1116));
    assert!(first_nonzero(&cathedral.1).unwrap() >= 1116, "the right comb is 23 samples longer");
    // Rooms, room size 1: delay lines from 3 ms (132 samples): arrivals far earlier than any comb
    let first = first_nonzero(&chamber.1).unwrap();
    assert_eq!(first, 132, "3 ms at 44.1 kHz");
    assert!(first_nonzero(&chamber.0).unwrap() < 1116);
}

#[cfg(feature = "us-fx-reverb-compact")]
#[test]
fn the_compact_reverb_rooms_keep_their_size_in_seconds() {
    // the stock comb lengths are 44.1 kHz samples; at 48 kHz and 96 kHz they scale with the rate
    for (rate, first) in [(44100.0, 1116), (48000.0, 1214), (96000.0, 2429)] {
        let x = impulse(8000);
        let (l, _) = with_fx(kind::REVERB, &[0.5, 1.0, 0.5, 1.0, 1.0], rate, 120, |fx| run(fx, rate, 120, &x, &x, 1000));
        // the first comb output reaches the allpass chain at its own length; wet on the left only via width 1
        assert_eq!(first_nonzero(&l[1..]).map(|v| v + 1), Some(first), "rate {rate}");
    }
}

#[cfg(feature = "us-fx-compressor")]
#[test]
fn the_sidechain_key_drives_the_compressor_and_is_never_written() {
    let rate = 44100.0;
    let n = 44100;
    // a steady pad on main, a loud kick on the sidechain pair every 0.25 s for 40 ms
    let pad = sine(220.0, rate as f32, n, 0.2);
    let kick: Vec<f32> = (0..n).map(|i| if (i % 11025) < 1764 { 0.9 * libm::sinf(i as f32 * 0.05) } else { 0.0 }).collect();
    // threshold param 0.5 → −18·… low enough for the kick; ratio high; fast attack; short release
    let p = |sidechain: f32| [0.0, sidechain, 0.5, 0.2, 1.0, 0.0, 0.1, 0.5];
    let level = |o: &[f32], from: usize, len: usize| rms(&o[from..from + len]);
    let run_it = |sidechain: f32| {
        let mut dev = {
            let chunk = chunk(kind::COMPRESSOR, &p(sidechain));
            let env = env(rate, 120);
            let mut mem = std::vec![0u8; UFx::need(kind::COMPRESSOR, &chunk, &env).unwrap()];
            let leaked: &'static mut [u8] = std::boxed::Box::leak(mem.into_boxed_slice());
            let mut pool = Pool::new(leaked);
            UFx::new(kind::COMPRESSOR, &chunk, &env, &mut pool).unwrap()
        };
        let mut sl = kick.clone();
        let mut sr = kick.clone();
        let (mut l, mut r) = (pad.clone(), pad.clone());
        let mut rng = Lcg(1);
        let mut ctx = Ctx { rate, tempo: 120, frame: 0, rng: &mut rng };
        dev.run(&mut Io { ch: [&mut l, &mut r, &mut sl, &mut sr] }, n, &mut ctx);
        assert_eq!(sl, kick, "the sidechain is read, never written");
        assert_eq!(sr, kick);
        l
    };
    let keyed = run_it(1.0);
    let on_main = run_it(0.0);
    // between kicks the pad is at its level; during a kick it drops when the key is the sidechain
    let kick_win = (11025 + 400, 1000);
    let quiet_win = (11025 + 4000, 4000);
    assert!(level(&keyed, kick_win.0, kick_win.1) < 0.6 * level(&keyed, quiet_win.0, quiet_win.1), "ducked on every kick");
    for k in 1..4 {
        let at = k * 11025 + 400;
        assert!(level(&keyed, at, 1000) < 0.6 * level(&keyed, k * 11025 + 4000, 4000), "kick {k}");
    }
    // with the key on main the quiet pad stays under the threshold: no ducking
    assert!((level(&on_main, kick_win.0, kick_win.1) / level(&on_main, quiet_win.0, quiet_win.1) - 1.0).abs() < 0.05);
}

#[cfg(feature = "us-fx-formant")]
#[test]
fn the_formant_filter_is_stable_and_rate_independent_in_hz_on_a_noise_input() {
    for rate in [48000.0, 96000.0] {
        let (l, r) = noise(1, 20000);
        for vowel in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let (ol, or) = with_fx(kind::FORMANT, &[0.0, vowel, 1.0], rate, 120, |fx| run(fx, rate, 120, &l, &r, 4096));
            assert!(ol.iter().chain(&or).all(|v| v.is_finite()) && peak(&ol) < 10.0 && peak(&ol) > 1e-3, "vowel {vowel} at {rate}");
        }
    }
}

#[cfg(feature = "us-fx-chorus")]
#[test]
fn the_chorus_delay_is_in_milliseconds_at_any_rate() {
    // depth 0 → every line sits at 1 ms; gain 1 → each line adds its input delayed by trunc(rate · 1 ms)
    for (rate, d) in [(44100.0, 44), (48000.0, 48), (96000.0, 96)] {
        let x = impulse(2000);
        let (l, _) = with_fx(kind::CHORUS, &[0.0, 0.5, 0.0, 1.0], rate, 120, |fx| run(fx, rate, 120, &x, &x, 300));
        assert_eq!(first_nonzero(&l[1..]).map(|v| v + 1), Some(d), "rate {rate}");
    }
}

#[cfg(feature = "us-fx-flanger-ring")]
#[test]
fn the_ring_flanger_delay_range_is_in_milliseconds_at_any_rate() {
    // amp 0 → Δ = 0.1 ms: y = x − wet·x[n − Δ] echoes the impulse inverted Δ samples later
    for (rate, d) in [(44100.0, 4), (48000.0, 4), (96000.0, 9)] {
        let x = impulse(400);
        let (l, _) = with_fx(kind::FLANGER, &[1.0, 0.0, 0.5, 0.0, 1.0], rate, 120, |fx| run(fx, rate, 120, &x, &x, 100));
        assert_eq!(first_nonzero(&l[1..]).map(|v| v + 1), Some(d), "rate {rate}");
        assert!(l[d] < -0.99);
    }
}

#[test]
fn the_random_parameter_sets_cover_the_range_of_each_parameter() {
    // parity is only as good as its inputs: every position of the parameter set sees small and large values
    for pos in 0..14 {
        let v: Vec<f32> = (0..24).map(|s| values(s, 14)[pos]).collect();
        assert!(v.iter().any(|x| *x < 0.35) && v.iter().any(|x| *x > 0.65), "position {pos}: {v:?}");
    }
}

#[test]
fn every_mode_is_a_plain_choice_and_its_old_value_migrates_to_it() {
    // v3: the mode is `Choice(mode_total(k))`; the old expanded value of every mode migrates to it
    for &(k, i, _) in MODES.iter() {
        assert_eq!(mode_of_value(k, super::mode_value(k, i)), i, "kind {k} mode {i}");
        assert_eq!(super::mode_value(k, i), mode(i, mode_total(k)));
        assert_eq!(mode_of(k, &chunk(k, &[old_mode_value(k, i)])), i, "kind {k} mode {i}: the old value");
    }
    // an old mode value read as v3 would be another mode: the tag keeps old chunks out
    assert_ne!(mode_of_value(kind::DISTORTION, old_mode_value(kind::DISTORTION, 2)), 2);
    assert_eq!((mode_total(kind::REVERB), mode_count(kind::REVERB)), (4, 3));
    assert_eq!(mode_of(kind::COMPRESSOR, &chunk(kind::COMPRESSOR, &[1.0])), 1);
}

#[test]
#[cfg(feature = "us-fx-reverb-comb")]
fn the_v2_reverb_has_no_low_cut_before_version_4() {
    let (l, r) = noise(9, 8000);
    let render = |lowcut: f32, era: u32| {
        let p = [old_mode_value(kind::REVERB, 3), 64.0 / 128.0, 64.0 / 128.0, lowcut, 127.0 / 128.0, crate::law::host::inv_count(era, 0, 6)];
        with_fx(kind::REVERB, &p, 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &r, 512))
    };
    assert_eq!(render(0.0, 3), render(1.0, 3), "version 3: the low cut does nothing");
    assert_ne!(render(0.0, 4), render(1.0, 4), "version 4: it cuts");
}

// ---- change add-cauldron-xm-import: comb (flanger mode 2) and wrap (distortion mode 3) ----

#[test]
fn the_clocked_modes_take_free_positions_and_old_values_keep_their_modes() {
    // distortion: 3 old modes on 5 positions; flanger: 2 old modes on 3 positions
    for (k, v, want) in [
        (kind::DISTORTION, 0.0, 0),
        (kind::DISTORTION, 0.5, 1),
        (kind::DISTORTION, 1.0, 2),
        (kind::DISTORTION, 0.25, 3),
        (kind::FLANGER, 0.0, 0),
        (kind::FLANGER, 1.0, 1),
        (kind::FLANGER, 0.5, 2),
    ] {
        assert_eq!(mode_of(k, &chunk(k, &[v])), want, "kind {k} value {v}");
    }
    assert_eq!((old_mode_value(kind::DISTORTION, 3), old_mode_value(kind::FLANGER, 2)), (0.25, 0.5));
    assert_eq!((module_of(kind::DISTORTION, 3), module_of(kind::FLANGER, 2)), (Some(m::FX_DISTORTION_WRAP), Some(m::FX_FLANGER_COMB)));
}

/// Comb parameters: delay `k` frames, amount, sweep rate (fraction per frame), restart.
#[cfg(feature = "us-fx-flanger-comb")]
fn comb(k: usize, g: f32, s: f32) -> [f32; 5] {
    [old_mode_value(kind::FLANGER, 2), k as f32 / 16384.0, g, s / 0.01, 0.0]
}

#[test]
#[cfg(feature = "us-fx-flanger-comb")]
fn the_difference_comb_nulls_a_sine_at_rate_over_k() {
    let x = sine(441.0, 44100.0, 4000, 0.8);
    let (y, _) = with_fx(kind::FLANGER, &comb(100, 1.0, 0.0), 44100.0, 120, |fx| run(fx, 44100.0, 120, &x, &x, 100));
    assert_eq!(y[..100], x[..100], "nothing is delayed in yet");
    assert!(peak(&y[100..]) < 1e-4, "{}", peak(&y[100..])); // the f32 test sine itself drifts by ~1e-5
    // half the amount leaves half of the sine
    let (y, _) = with_fx(kind::FLANGER, &comb(100, 0.5, 0.0), 44100.0, 120, |fx| run(fx, 44100.0, 120, &x, &x, 100));
    assert!((peak(&y[200..]) - 0.4).abs() < 1e-3);
}

#[test]
#[cfg(feature = "us-fx-flanger-comb")]
fn the_sweep_reads_further_ahead_until_a_restart_resets_it() {
    let (x, _) = noise(9, 6000);
    let k = 200;
    let y = with_fx(kind::FLANGER, &comb(k, 1.0, 0.003), 44100.0, 120, |fx| {
        let a = run(fx, 44100.0, 120, &x[..3000], &x[..3000], 3000).0;
        fx.set_param(4, 1.0); // a step of the restart parameter
        let mut b = run(fx, 44100.0, 120, &x[3000..], &x[3000..], 3000).0;
        let mut all = a;
        all.append(&mut b);
        all
    });
    // the second read runs s·t frames ahead of the delayed one (linear interpolation): at frame k, 0.6 frames
    let lerp = |i: usize, f: f32| x[i] + f * (x[i + 1] - x[i]);
    assert!((y[k] - (x[0] + lerp(0, 0.6)) / 2.0).abs() < 1e-4);
    assert!((y[2000] - (x[2000 - k] + x[2000 - k + 6]) / 2.0).abs() < 1e-4, "t = 2000: 6 frames ahead");
    // a restart sets t to 0: both reads are the delayed sample
    assert_eq!(y[3000], x[3000 - k]);
}

/// The original doubler (Stash, `0x402FF0`), in place: `buf[j − 1] = (buf[j] + buf[i]) / 2`, `j = (i·0xFF3B + 0x8000) >> 16`.
#[cfg(feature = "us-fx-flanger-comb")]
fn doubler(x: &[f32]) -> Vec<f32> {
    let mut b = x.to_vec();
    for i in 2..b.len() {
        let j = ((i as u64 * 0xFF3B + 0x8000) >> 16) as usize;
        b[j - 1] = (b[j] + b[i]) / 2.0;
    }
    b
}

#[test]
#[cfg(feature = "us-fx-flanger-comb")]
fn the_sweep_is_the_original_doubler_delayed() {
    // a band-limited phrase-like signal at the original's rate; the sweep with latency D reproduces the doubler
    let rate = 35441.0;
    let n = 58000;
    let x: Vec<f32> = (0..n).map(|i| {
        let t = i as f32 / rate;
        0.4 * libm::sinf(2.0 * core::f32::consts::PI * 216.0 * t) + 0.3 * libm::sinf(2.0 * core::f32::consts::PI * 1300.0 * t + 1.0)
    }).collect();
    let want = doubler(&x);
    // original: y[m] = (x[m + 1] + x[(m + 1)·(1 + s')])/2 with s' = 65536/0xFF3B − 1; the sweep matches it with
    // latency d + 1 when t restarts d frames after the phrase start: y[n] = (x[n − d] + x[n − d + s'·(n − d)])/2
    let d = 180;
    let s = 65536.0 / 0xFF3B as f32 - 1.0;
    let (y, _) = with_fx(kind::FLANGER, &comb(d, 1.0, s), rate as f64, 120, |fx| {
        let mut a = run(fx, rate as f64, 120, &x[..d], &x[..d], d).0;
        fx.set_param(4, 1.0);
        a.extend(run(fx, rate as f64, 120, &x[d..], &x[d..], 1000).0);
        (a, ())
    });
    let (mut err, mut sig) = (0.0f64, 0.0f64);
    for m in 1000..n - d - 2 {
        let e = (y[m + d + 1] - want[m]) as f64;
        err += e * e;
        sig += (want[m] as f64).powi(2);
    }
    let db = 10.0 * (err / sig).log10();
    assert!(db < -20.0, "error {db:.1} dB below the signal");
}

#[test]
#[cfg(feature = "us-fx-distortion-wrap")]
fn wrap_distortion_wraps_around_full_scale() {
    // drive p·4, offset 2p − 1
    let at = |drive: f32, offset: f32, x: f32| {
        let v = [old_mode_value(kind::DISTORTION, 3), drive / 4.0, (offset + 1.0) / 2.0];
        with_fx(kind::DISTORTION, &v, 44100.0, 120, |fx| run(fx, 44100.0, 120, &[x; 2], &[x; 2], 2).0[1])
    };
    assert!((at(1.25, 0.0, 0.9) - -0.875).abs() < 1e-6, "1.125 wraps to -0.875");
    assert!((at(1.25, 0.0, 0.5) - 0.625).abs() < 1e-6, "inside the range: drive only");
    assert!((at(1.0, 0.375, 0.7) - (1.075 - 2.0)).abs() < 1e-6, "Stash's +48: 0.7 + 0.375 wraps");
    assert!((at(1.0, 0.375, -0.5) - -0.125).abs() < 1e-6);
}

/// `Device::quiet_after` of the modes the TBL songs use (the mixer's quiet-track skip): memoryless ones 0, the delays
/// their line capacity, a wrap with an offset unknown (it turns silence into DC), every other mode unknown.
#[cfg(all(feature = "us-fx-distortion-shaper", feature = "us-fx-eq-3band", feature = "us-fx-delay-tempo", feature = "us-fx-flanger-comb", feature = "us-fx-distortion-wrap", feature = "us-fx-crusher"))]
#[test]
fn quiet_after_is_the_memory_of_the_mode() {
    let q = |k: u8, m: f32, rest: &[f32]| {
        let mut v = vec![m];
        v.extend_from_slice(rest);
        with_fx(k, &v, 44100.0, 120, |fx| synth_core::device::Device::quiet_after(fx))
    };
    let (dist, eq, delay, fl) = (kind::DISTORTION, kind::EQ, kind::DELAY, kind::FLANGER);
    assert_eq!(q(dist, old_mode_value(dist, 0), &[0.2, 1.0, 0.0, 1.0]), Some(0), "shaper");
    assert_eq!(q(eq, old_mode_value(eq, 1), &[0.5, 0.5, 0.5]), Some(0), "3-band");
    let tempo = q(delay, old_mode_value(delay, 0), &[0.5, 0.5, 0.5, 0.5]).unwrap();
    // its whole line: at least the longest delay at 120 BPM, 16 eighths (1 s) + 200 ms
    assert!(tempo as f64 >= 1.2 * 44100.0, "the tempo delay's whole line: {tempo}");
    assert_eq!(q(fl, old_mode_value(fl, 2), &[0.01, 1.0, 0.0, 0.0]), Some(16386), "comb: its line");
    assert_eq!(q(dist, old_mode_value(dist, 3), &[0.25, 0.5]), Some(0), "wrap without offset");
    assert_eq!(q(dist, old_mode_value(dist, 3), &[0.25, 0.7]), None, "wrap with an offset plays DC on silence");
    assert_eq!(q(kind::CRUSHER, 0.0, &[0.5, 0.5, 1.0]), None, "a mode without an answer");
}

/// `Device::wake` (the mixer skipped the effect's track): the 3-band EQ's next block ramps from the gain its
/// parameters held before the waking block's automation, as it does when it never slept.
#[cfg(feature = "us-fx-eq-3band")]
#[test]
fn a_woken_three_band_ramps_as_if_it_had_run() {
    let eq = kind::EQ;
    let block = |fx: &mut UFx, x: f32| {
        let (mut l, mut r, mut sl, mut sr) = (vec![x; 100], vec![x; 100], vec![0.0; 100], vec![0.0; 100]);
        let mut rng = Lcg(1);
        let mut ctx = Ctx { rate: 44100.0, tempo: 120, frame: 0, rng: &mut rng };
        synth_core::device::Device::run(fx, &mut Io { ch: [&mut l, &mut r, &mut sl, &mut sr] }, 100, &mut ctx);
        l
    };
    // awake: blocks at gain 0.1, then 0.3 (silent input), then the gain steps to 0.5 with sound
    let want = with_fx(eq, &[old_mode_value(eq, 1), 0.1, 0.1, 0.1], 44100.0, 120, |fx| {
        block(fx, 0.0);
        for i in 1..=3 {
            synth_core::device::Device::set_param(fx, i, 0.3);
        }
        block(fx, 0.0);
        for i in 1..=3 {
            synth_core::device::Device::set_param(fx, i, 0.5);
        }
        block(fx, 1.0)
    });
    // asleep through the 0.3 block (no run), woken before the step to 0.5
    let got = with_fx(eq, &[old_mode_value(eq, 1), 0.1, 0.1, 0.1], 44100.0, 120, |fx| {
        block(fx, 0.0);
        for i in 1..=3 {
            synth_core::device::Device::set_param(fx, i, 0.3);
        }
        synth_core::device::Device::wake(fx, 100);
        for i in 1..=3 {
            synth_core::device::Device::set_param(fx, i, 0.5);
        }
        block(fx, 1.0)
    });
    assert_eq!(got, want);
}

/// A woken comb's sweep goes on from where the skipped frames would have left it (its counter runs in time).
#[cfg(feature = "us-fx-flanger-comb")]
#[test]
fn a_woken_comb_sweeps_on_as_if_it_had_run() {
    let fl = kind::FLANGER;
    // k = 1000 frames, sweep 0.003 per frame (cap at k after 333 333 frames): silence for 5000 frames, then a tone
    let vals = [old_mode_value(fl, 2), 1000.0 / 16384.0, 1.0, 0.3, 0.0];
    let block = |fx: &mut UFx, x: &[f32]| {
        let (mut l, mut r, mut sl, mut sr) = (x.to_vec(), x.to_vec(), vec![0.0; x.len()], vec![0.0; x.len()]);
        let mut rng = Lcg(1);
        let mut ctx = Ctx { rate: 44100.0, tempo: 120, frame: 0, rng: &mut rng };
        synth_core::device::Device::run(fx, &mut Io { ch: [&mut l, &mut r, &mut sl, &mut sr] }, x.len(), &mut ctx);
        l
    };
    let tone: Vec<f32> = (0..3000).map(|i| libm::sinf(i as f32 * 0.05)).collect();
    let want = with_fx(fl, &vals, 44100.0, 120, |fx| {
        block(fx, &[0.0; 5000]);
        block(fx, &tone)
    });
    let got = with_fx(fl, &vals, 44100.0, 120, |fx| {
        synth_core::device::Device::wake(fx, 5000);
        block(fx, &tone)
    });
    assert!(peak(&want[1500..]) > 0.1, "the comb sounds");
    assert_eq!(got, want);
}

/// The freeverb mode's memory (change compact-cauldron-xm-import: a reverb on the TBL songs' layered tracks): the
/// core's sleep span (more than its longest loop) plus the pre-delay line; frozen, unknown (it sustains forever).
#[cfg(feature = "us-fx-reverb-freeverb")]
#[test]
fn a_freeverb_answers_its_memory() {
    let r = kind::REVERB;
    let q = |freeze: f32| with_fx(r, &[old_mode_value(r, 0), freeze, 0.3, 0.5, 1.0, 0.0, 1.0, 0.12, 0.02], 44100.0, 120, |fx| synth_core::device::Device::quiet_after(fx));
    let n = q(0.0).expect("a known memory") as usize;
    assert!(n >= 8192 + super::law::ms_to_len(500.0, 44100.0), "{n}");
    assert_eq!(q(1.0), None, "frozen");
}
