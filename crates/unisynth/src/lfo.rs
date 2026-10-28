//! LFOs (design D5/D6): a phase in turns advanced by elapsed time, five shapes, two views of the output.

use crate::tables::sin_turns;

/// Shapes (`choice(p, 5)` of `l::SHAPE`).
pub mod shape {
    pub const SINE: usize = 0;
    pub const RAMP_UP: usize = 1;
    pub const RAMP_DOWN: usize = 2;
    pub const SQUARE: usize = 3;
    pub const RANDOM: usize = 4;
    pub const COUNT: usize = 5;
}

/// Steps of the stepped-random shape per cycle.
const RANDOM_STEPS: f64 = 256.0;

/// Uniform `[0, 1)` from a step number (a fixed hash, so the sequence is the same for every voice and target).
#[inline]
fn hash01(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B1) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x >> 8) as f32 * (1.0 / 16_777_216.0)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Lfo {
    /// Phase in turns, `[0, 1)`.
    pub phase: f64,
    /// V2 mode: one-shot (envelope mode) cycle finished, holding at its end; the sample-and-hold seed and the value
    /// it holds (`v2::Opt`: absent without the V2 voice features).
    v2: crate::v2::Opt<(bool, u32)>,
}

/// V2 LFO waveforms (`V2LFO::Mode`).
pub mod v2shape {
    pub const SAW: u8 = 0;
    pub const TRI: u8 = 1;
    pub const PULSE: u8 = 2;
    pub const SIN: u8 = 3;
    pub const S_H: u8 = 4;
}

/// A V2-mode LFO's settings from its raw fields (`V2LFO::set`): waveform, turns per V2 frame, one-shot, gain and DC
/// of its output in 0..1 units (V2's `out/128`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V2P {
    pub shape: u8,
    pub per_frame: f64,
    pub eg: bool,
    pub gain: f32,
    pub dc: f32,
}

impl V2P {
    /// `shape`, `rate`, `amp` (depth), `polarity` and `eg` are raw / 128.
    pub fn new(shape: f32, rate: f32, amp: f32, polarity: f32, eg: f32) -> Self {
        let raw = |x: f32| x * 128.0;
        let (gain, dc) = match raw(polarity) as i32 {
            1 => (-amp, 0.0),
            2 => (amp, -0.5 * amp),
            _ => (amp, 0.0),
        };
        // freq = 0.5·2³¹·calcfreq(rate/128) counts of 2³² per frame
        let per_frame = 0.25 * crate::v2::calcfreq(rate) as f64;
        V2P { shape: (raw(shape) as i32).clamp(0, 7) as u8, per_frame, eg: raw(eg) as i32 != 0, gain, dc }
    }
}

impl Lfo {
    pub fn new(phase: f64) -> Self {
        Lfo { phase: phase - libm::floor(phase), v2: crate::v2::Opt::new((false, 0)) }
    }

    /// V2 key sync: the counter starts at `phase` (raw / 128 of a turn, V2's `ftou32`); the sample-and-hold draws
    /// at its first tick. `seed` is the voice's sample-and-hold seed.
    pub fn sync_v2(&mut self, phase: f32, seed: u32) {
        self.phase = crate::v2::ftou32(phase) as f64 / 4_294_967_296.0;
        self.v2.set((false, crate::v2::urandom(&mut { seed })));
    }

    /// Advances a V2-mode LFO by `dt` seconds (V2 steps `per_frame` turns every frame); a one-shot LFO stops at the
    /// end of its first cycle, a sample-and-hold draws a new value at every wrap.
    pub fn advance_v2(&mut self, dt: f32, p: &V2P) {
        let (held, mut seed) = self.v2.get();
        if held {
            return;
        }
        let ph = self.phase + p.per_frame * (dt * crate::v2::FRAMES_PER_S) as f64;
        if ph >= 1.0 {
            if p.eg {
                self.phase = 1.0 - 1.0 / 4_294_967_296.0;
                self.v2.set((true, seed));
                return;
            }
            let wraps = libm::floor(ph);
            for _ in 0..(wraps as u32).min(4) {
                crate::v2::urandom(&mut seed);
            }
            self.v2.set((false, seed));
            self.phase = ph - wraps;
        } else {
            self.phase = ph;
        }
    }

    /// A V2-mode LFO's waveform value `0..1` at the current phase (`V2LFO::tick`).
    pub fn shape_v2(&self, shape: u8, native_sin: bool) -> f32 {
        let cntr = (self.phase * 4_294_967_296.0) as u64 as u32;
        match shape {
            v2shape::TRI => crate::v2::utof23((cntr << 1) ^ ((cntr as i32 >> 31) as u32)),
            v2shape::PULSE => crate::v2::utof23((cntr as i32 >> 31) as u32),
            v2shape::SIN => {
                let x = crate::v2::utof23(cntr) * core::f32::consts::TAU;
                (if native_sin { crate::fm::sinf(x) } else { crate::v2::fastsinrc(x) }) * 0.5 + 0.5
            }
            v2shape::S_H => crate::v2::utof23(self.v2.get().1),
            _ => crate::v2::utof23(cntr),
        }
    }

    /// Advances by `dt` seconds at `hz`.
    pub fn advance(&mut self, dt: f32, hz: f32) {
        self.phase += hz as f64 * dt as f64;
        self.phase -= libm::floor(self.phase);
    }

    /// The unipolar shape value `0..1` at the current phase.
    pub fn shape(&self, kind: usize) -> f32 {
        let ph = self.phase;
        match kind {
            shape::SINE => (sin_turns(ph) + 1.0) * 0.5,
            shape::RAMP_UP => ph as f32,
            shape::RAMP_DOWN => 1.0 - ph as f32,
            shape::SQUARE => {
                if ph < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            _ => hash01((ph * RANDOM_STEPS) as u32),
        }
    }
}

/// What a Scale slot sees: `1 − d + d·shape`, `d = (depth·M)²`.
#[inline]
pub fn scale_view(depth: f32, shape: f32) -> f32 {
    let d = depth * depth;
    1.0 - d + d * shape
}

/// What an Offset slot sees: bipolar `d·(2·shape − 1)`.
#[inline]
pub fn offset_view(depth: f32, shape: f32) -> f32 {
    let d = depth * depth;
    d * (2.0 * shape - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_at_their_phases() {
        crate::tables::init();
        let at = |ph: f64, k| Lfo::new(ph).shape(k);
        assert!((at(0.0, shape::SINE) - 0.5).abs() < 1e-6);
        assert!((at(0.25, shape::SINE) - 1.0).abs() < 1e-6);
        assert!(at(0.75, shape::SINE).abs() < 1e-6);
        assert_eq!((at(0.0, shape::RAMP_UP), at(0.5, shape::RAMP_UP)), (0.0, 0.5));
        assert_eq!((at(0.0, shape::RAMP_DOWN), at(0.25, shape::RAMP_DOWN)), (1.0, 0.75));
        assert_eq!((at(0.0, shape::SQUARE), at(0.49, shape::SQUARE), at(0.5, shape::SQUARE)), (1.0, 1.0, 0.0));
        // stepped random: constant within a step, uniform-looking across steps
        assert_eq!(at(0.0, shape::RANDOM), at(0.001, shape::RANDOM));
        let v: Vec<f32> = (0..256).map(|i| at(i as f64 / 256.0 + 1e-4, shape::RANDOM)).collect();
        assert!(v.iter().all(|x| (0.0..1.0).contains(x)));
        let mean = v.iter().sum::<f32>() / 256.0;
        assert!((mean - 0.5).abs() < 0.08, "{mean}");
        assert!(v.windows(2).any(|w| w[0] != w[1]));
    }

    #[test]
    fn advance_runs_in_hz_and_wraps() {
        let mut l = Lfo::new(0.0);
        for _ in 0..1000 {
            l.advance(0.001, 2.5); // 1 s at 2.5 Hz = 2.5 turns
        }
        assert!((l.phase - 0.5).abs() < 1e-6, "{}", l.phase); // dt is f32
        l.advance(10.0, 0.0);
        assert!((l.phase - 0.5).abs() < 1e-6);
        assert!(Lfo::new(-0.25).phase > 0.74 && Lfo::new(3.5).phase == 0.5);
    }

    #[test]
    fn the_two_views() {
        // depth 1: full range; depth 0.5: d = 0.25
        assert_eq!((scale_view(1.0, 0.0), scale_view(1.0, 1.0)), (0.0, 1.0));
        assert_eq!((offset_view(1.0, 0.0), offset_view(1.0, 1.0), offset_view(1.0, 0.5)), (-1.0, 1.0, 0.0));
        assert!((scale_view(0.5, 0.0) - 0.75).abs() < 1e-7);
        assert!((offset_view(0.5, 1.0) - 0.25).abs() < 1e-7);
        assert_eq!(scale_view(0.0, 0.3), 1.0);
    }
}
