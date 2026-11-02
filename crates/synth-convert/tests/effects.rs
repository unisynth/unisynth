//! The unified effects against the effects of the original engines: for random parameter sets and noise the output of each
//! mode must be the same (bit for bit unless noted, with the tolerances below where the unified effect uses its own
//! `fm` maths). The converter owns this check because it is the one crate that knows both sides.

use synth_core::device::{Ctx, Device, Io};
use synth_core::mix::{Env, Kinds};
use synth_core::pool::Pool;
use synth_core::rng::Rng;
use unisynth::fx::{mode_count, UFx};
// the legacy chunk layout (values + WaveSabre's size trailer); the unified side migrates it (change unify-v2-algorithms)
use unisynth::migrate::encode_old_fx as encode;
use unisynth::kind;

struct Lcg(u32);
impl Rng for Lcg {
    fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// Deterministic values in `0..1` (an xorshift32 stream from a hashed seed: consecutive seeds give unrelated sets).
fn values(seed: u32, n: usize) -> Vec<f32> {
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
fn noise(seed: u32, n: usize) -> (Vec<f32>, Vec<f32>) {
    let mut r = Lcg(seed);
    ((0..n).map(|_| r.next_f32() - 0.5).collect(), (0..n).map(|_| r.next_f32() - 0.5).collect())
}

/// Renders `(l, r)` through `d` in calls of `call` frames; no sidechain.
fn run<D: Device>(d: &mut D, rate: f64, tempo: i32, l: &[f32], r: &[f32], call: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut ol, mut or) = (l.to_vec(), r.to_vec());
    let mut rng = Lcg(5);
    let mut at = 0;
    while at < l.len() {
        let n = call.min(l.len() - at);
        let (mut sl, mut sr) = (vec![0.0; n], vec![0.0; n]);
        let mut ctx = Ctx { rate, tempo, frame: at as u64, rng: &mut rng };
        let mut io = Io { ch: [&mut ol[at..at + n], &mut or[at..at + n], &mut sl, &mut sr] };
        d.run(&mut io, n, &mut ctx);
        at += n;
    }
    (ol, or)
}

fn env(rate: f64, tempo: i32) -> Env<'static> {
    Env { rate, tempo, bank: None }
}

/// Loads unified effect kind `k` with old-layout `vals` (`[mode as stored before the switch, params…]`, migrated) and
/// runs `f` on it.
fn with_fx<R>(k: u8, vals: &[f32], rate: f64, tempo: i32, f: impl FnOnce(&mut UFx) -> R) -> R {
    let chunk = unisynth::migrate::effect_chunk(k, &encode(vals)).unwrap();
    let env = env(rate, tempo);
    let mut mem = vec![0u8; UFx::need(k, &chunk, &env).unwrap()];
    let mut pool = Pool::new(&mut mem);
    let mut fx = UFx::new(k, &chunk, &env, &mut pool).unwrap();
    f(&mut fx)
}

/// The same with a WaveSabre effect (`params` without a mode).
fn with_ws<R>(k: u8, params: &[f32], rate: f64, tempo: i32, f: impl FnOnce(&mut wavesabre::WsDevice) -> R) -> R {
    let chunk = encode(params);
    let env = env(rate, tempo);
    let mut mem = vec![0u8; wavesabre::WsDevice::need(k, &chunk, &env).unwrap()];
    let mut pool = Pool::new(&mut mem);
    let mut dev = wavesabre::WsDevice::load(k, &chunk, &env, &mut pool, &mut Lcg(1)).unwrap();
    f(&mut dev)
}

/// The same with a Tunefish effect.
fn with_tf<R>(k: u8, params: &[f32], rate: f64, tempo: i32, f: impl FnOnce(&mut tfsynth::device::TfDevice) -> R) -> R {
    let chunk = encode(params);
    let env = env(rate, tempo);
    let mut mem = vec![0u8; tfsynth::device::TfDevice::need(k, &chunk, &env).unwrap()];
    let mut pool = Pool::new(&mut mem);
    let mut dev = tfsynth::device::TfDevice::load(k, &chunk, &env, &mut pool, &mut Lcg(1)).unwrap();
    f(&mut dev)
}

/// `mode` as the normalized value that selects it among `n` modes.
fn mode(i: usize, n: usize) -> f32 {
    if n <= 1 { 0.0 } else { i as f32 / (n - 1) as f32 }
}

fn worst(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).fold(0.0, |m, (x, y)| m.max((x - y).abs()))
}

fn peak(a: &[f32]) -> f32 {
    a.iter().fold(0.0, |m, x| m.max(x.abs()))
}

/// Where the laws use `unisynth::fm` (its own `exp2`/`pow`/`sin`) instead of `libm`, the unified effect differs from the
/// original by an ulp or two of the result, which a resonant filter can raise to a few 1e-6 (−110 dBFS, below the 16-bit
/// floor): not bit-exact.
const TOL_FM: f32 = 1e-5;
/// The crusher quantizes: a last-bit difference before the rounding moves a sample by one quantization step now and then.
const TOL_QUANT: f32 = 1e-4;

/// The unified effect of `k` in `mode_i` against `reference` for parameter sets `seeds`: same output within `tol`.
fn parity(
    k: u8,
    mode_i: usize,
    n_params: usize,
    rate: f64,
    tol: f32,
    seeds: std::ops::Range<u32>,
    reference: impl Fn(&[f32], &[f32], &[f32]) -> (Vec<f32>, Vec<f32>),
) {
    parity_at(k, mode_i, n_params, rate, 120, tol, seeds, reference)
}

#[allow(clippy::too_many_arguments)]
fn parity_at(
    k: u8,
    mode_i: usize,
    n_params: usize,
    rate: f64,
    tempo: i32,
    tol: f32,
    seeds: std::ops::Range<u32>,
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

#[test]
fn scissor_mode_is_the_wavesabre_scissor() {
    let rate = 44100.0;
    parity(kind::DISTORTION, 0, 6, rate, 0.0, 0..24, |p, l, r| with_ws(wavesabre::kind::SCISSOR, p, rate, 120, |d| run(d, rate, 120, l, r, 512)));
}

#[test]
fn crusher_mode_is_the_wavesabre_crusher() {
    let rate = 44100.0;
    parity(kind::CRUSHER, 0, 3, rate, TOL_QUANT, 0..24, |p, l, r| with_ws(wavesabre::kind::CRUSHER, p, rate, 120, |d| run(d, rate, 120, l, r, 512)));
}

#[test]
fn leveller_mode_is_the_wavesabre_leveller() {
    let rate = 44100.0;
    parity(kind::EQ, 0, 14, rate, TOL_FM, 0..24, |p, l, r| with_ws(wavesabre::kind::LEVELLER, p, rate, 120, |d| run(d, rate, 120, l, r, 512)));
}

#[test]
fn tunefish_distortion_mode_is_the_tunefish_distortion() {
    let rate = 44100.0;
    parity(kind::DISTORTION, 2, 1, rate, 0.0, 0..16, |p, l, r| {
        with_tf(tfsynth::device::kind::DISTORTION, p, rate, 120, |d| run(d, rate, 120, l, r, 512))
    });
}

#[test]
fn tunefish_eq_mode_is_the_tunefish_eq_at_its_own_rate() {
    let rate = 44100.0;
    parity(kind::EQ, 1, 3, rate, 0.0, 0..16, |p, l, r| with_tf(tfsynth::device::kind::EQ, p, rate, 120, |d| run(d, rate, 120, l, r, 512)));
}

#[test]
fn scissor_covers_every_shaper_and_oversampling_and_the_foldover() {
    let rate = 44100.0;
    let (l, r) = noise(7, 3000);
    for shaper in [0.0, 0.5, 1.0] {
        for over in [0.0, 0.5, 1.0] {
            for (drive, fold) in [(0.1, 0.0), (0.6, 0.0), (0.9, 0.7)] {
                let p = [drive, 0.6, fold, 0.8, shaper, over];
                let want = with_ws(wavesabre::kind::SCISSOR, &p, rate, 120, |d| run(d, rate, 120, &l, &r, 300));
                let mut u = vec![0.0];
                u.extend(p);
                let got = with_fx(kind::DISTORTION, &u, rate, 120, |fx| run(fx, rate, 120, &l, &r, 300));
                assert_eq!(got, want, "shaper {shaper} oversampling {over} drive {drive}");
            }
        }
    }
}

#[test]
fn echo_mode_is_the_wavesabre_echo_at_several_tempos() {
    for tempo in [60, 120, 175] {
        parity_at(kind::DELAY, 0, 9, 44100.0, tempo, 0.0, 0..12, |p, l, r| {
            with_ws(wavesabre::kind::ECHO, p, 44100.0, tempo, |d| run(d, 44100.0, tempo, l, r, 512))
        });
    }
}

#[test]
fn tunefish_delay_mode_is_the_tunefish_delay_at_44100() {
    parity(kind::DELAY, 1, 3, 44100.0, 0.0, 0..16, |p, l, r| with_tf(tfsynth::device::kind::DELAY, p, 44100.0, 120, |d| run(d, 44100.0, 120, l, r, 512)));
}

#[test]
fn chorus_is_the_tunefish_chorus_at_44100() {
    parity(kind::CHORUS, 0, 3, 44100.0, 0.0, 0..16, |p, l, r| with_tf(tfsynth::device::kind::CHORUS, p, 44100.0, 120, |d| run(d, 44100.0, 120, l, r, 512)));
}

#[test]
fn twister_mode_is_the_wavesabre_twister() {
    parity(kind::FLANGER, 0, 9, 44100.0, 0.0, 0..16, |p, l, r| with_ws(wavesabre::kind::TWISTER, p, 44100.0, 120, |d| run(d, 44100.0, 120, l, r, 512)));
}

#[test]
fn twister_covers_every_type_and_spread() {
    let rate = 44100.0;
    let (l, r) = noise(8, 4000);
    for ty in [0.0, 0.34, 0.67, 1.0] {
        for spread in [0.0, 0.5, 1.0] {
            let p = [ty, 0.45, 0.6, spread, 0.3, 0.5, 0.05, 0.9, 0.7];
            let want = with_ws(wavesabre::kind::TWISTER, &p, rate, 120, |d| run(d, rate, 120, &l, &r, 400));
            let mut u = vec![0.0];
            u.extend(p);
            let got = with_fx(kind::FLANGER, &u, rate, 120, |fx| run(fx, rate, 120, &l, &r, 400));
            assert_eq!(got, want, "type {ty} spread {spread}");
            assert!(peak(&got.0) > 0.0);
        }
    }
}

#[test]
fn tunefish_flanger_mode_is_the_tunefish_flanger_at_44100() {
    parity(kind::FLANGER, 1, 4, 44100.0, 0.0, 0..16, |p, l, r| with_tf(tfsynth::device::kind::FLANGER, p, 44100.0, 120, |d| run(d, 44100.0, 120, l, r, 512)));
}

#[test]
fn cathedral_mode_is_the_wavesabre_cathedral() {
    parity(kind::REVERB, 0, 8, 44100.0, 0.0, 0..16, |p, l, r| with_ws(wavesabre::kind::CATHEDRAL, p, 44100.0, 120, |d| run(d, 44100.0, 120, l, r, 512)));
    // freeze on, a pre-delay, and the legacy defaults of the rest
    for p in [[1.0, 0.9, 0.2, 0.5, 0.1, 0.9, 0.6, 0.0], [0.0, 0.7, 0.4, 0.8, 0.0, 1.0, 1.0, 0.3], [0.0, 0.5, 0.0, 0.0, 0.3, 0.7, 0.5, 1.0]] {
        let (l, r) = noise(2, 5000);
        let want = with_ws(wavesabre::kind::CATHEDRAL, &p, 44100.0, 120, |d| run(d, 44100.0, 120, &l, &r, 700));
        let mut u = vec![0.0];
        u.extend(p);
        assert_eq!(with_fx(kind::REVERB, &u, 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &r, 700)), want, "{p:?}");
    }
}

#[test]
fn tunefish_reverb_mode_is_the_tunefish_reverb_at_44100() {
    parity(kind::REVERB, 1, 4, 44100.0, 0.0, 0..16, |p, l, r| with_tf(tfsynth::device::kind::REVERB, p, 44100.0, 120, |d| run(d, 44100.0, 120, l, r, 512)));
}

#[test]
fn chamber_mode_is_the_wavesabre_chamber_in_every_room_size() {
    parity(kind::REVERB, 2, 6, 44100.0, 0.0, 0..16, |p, l, r| with_ws(wavesabre::kind::CHAMBER, p, 44100.0, 120, |d| run(d, 44100.0, 120, l, r, 512)));
    for room in [0.0, 0.5, 1.0] {
        let p = [room, 0.6, 0.0, 1.0, 0.8, 0.0];
        let (l, r) = noise(5, 4000);
        let want = with_ws(wavesabre::kind::CHAMBER, &p, 44100.0, 120, |d| run(d, 44100.0, 120, &l, &r, 300));
        let mut u = vec![1.0];
        u.extend(p);
        assert_eq!(with_fx(kind::REVERB, &u, 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &r, 300)), want, "room {room}");
    }
}

#[test]
fn smasher_mode_is_the_wavesabre_smasher() {
    parity(kind::COMPRESSOR, 0, 7, 44100.0, TOL_FM, 0..16, |p, l, r| with_ws(wavesabre::kind::SMASHER, p, 44100.0, 120, |d| run(d, 44100.0, 120, l, r, 512)));
}

#[test]
fn formant_mode_is_the_tunefish_formant_at_44100() {
    parity(kind::FORMANT, 0, 2, 44100.0, 0.0, 0..16, |p, l, r| with_tf(tfsynth::device::kind::FORMANT, p, 44100.0, 120, |d| run(d, 44100.0, 120, l, r, 512)));
    for vowel in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let (l, r) = noise(3, 3000);
        let want = with_tf(tfsynth::device::kind::FORMANT, &[vowel, 1.0], 44100.0, 120, |d| run(d, 44100.0, 120, &l, &r, 500));
        assert_eq!(with_fx(kind::FORMANT, &[0.0, vowel, 1.0], 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &r, 500)), want, "vowel {vowel}");
    }
}
