//! Engine oscillator types (change add-fudge-softsynth): generators of a precalc-sample intro played as slots. An
//! engine slot steps `256·hz/rate` engine frames per output frame (design D2) through the clocked slot's resampler;
//! a non-legato note-on seeks to the converter's start frame, forwards or backwards (D4). The state is fixed-size
//! (no heap): the Shaper tone is closed-form in the frame index except its carried shaper value, which a seek recovers
//! by scanning back to the frame that set it; the Sweep drum replays its sweep from frame 0 and serves a backward
//! pass from a 64-frame block of exact forward frames.
//!
//! Fudge's two generators (Alien Sex Clone, 1998), read from the running original: a sine with a two-stage shaper (a
//! 4× drive blend below ±8160, a fold-back above ±16320, a blend toward a value that carries from frame to frame) and
//! a sine with an accelerating downward sweep, the same shaper family and a one-shot envelope. The original ran with
//! the FPU rounding toward zero, so each f32 store and integer conversion truncates ([`f32t`]); the rest is f64 here
//! (x87 80-bit there).

use crate::law::choice;
use crate::params::{o, shape as sp};

/// Frames a Sweep drum's backward pass computes at a time.
pub const BLOCK: usize = 64;
/// Engine frames per cycle of the slot frequency (design D2).
pub const FRAMES_PER_CYCLE: f64 = 256.0;

const TWO_PI: f64 = 6.283185307179586;
const HALF_PI: f64 = 1.570796326794896;
/// The shapers' thresholds: below the low one the drive blend applies, above the high one the fold-back.
const LOW: f64 = 8160.0;
const HIGH: f64 = 16320.0;
const CLIP: f64 = 32385.0;

/// `x` stored to f32 with the rounding toward zero (the original's control word): nearest, then one ulp back when
/// that rounded away from zero.
#[inline]
pub fn f32t(x: f64) -> f32 {
    let r = x as f32;
    if (r as f64).abs() > x.abs() {
        f32::from_bits(r.to_bits() - 1)
    } else {
        r
    }
}

/// Integer conversion toward zero (`fistp` under the same control word).
#[inline]
fn trunc(x: f64) -> i64 {
    x as i64
}

/// x86 signed division (toward zero).
#[inline]
fn idiv(a: i64, b: i64) -> i64 {
    a / b
}

/// Shaper tone settings (`shape::TONE_*`, design D3): carrier and modulator periods in time units, depth (rad), the
/// amplitude `f32(vol·255)`, drive and shape blends, FM on, and the time units per frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToneT {
    pub carrier: f32,
    pub modulator: f32,
    pub depth: f32,
    pub amp: f32,
    pub drive: f32,
    pub shape: f32,
    pub fm: bool,
    pub rate: f32,
}

/// Sweep drum settings (`shape::DRUM_*`, design D3): the amplitude `f32(vol·255)`, start frequency and sweep (rad per
/// frame), the original's sweep acceleration (`−growth`), drive and shape blends, the envelope's start, its fall per
/// frame and its curve (0 linear, 1 `1 − sin`, 2 `cos`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DrumT {
    pub amp: f32,
    pub freq: f32,
    pub sweep: f32,
    pub accel: f32,
    pub drive: f32,
    pub shape: f32,
    pub env: f32,
    pub slope: f64,
    pub curve: u8,
}

/// An engine slot's settings for a control segment (not ramped).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum EngineT {
    #[default]
    None,
    Tone(ToneT),
    Drum(DrumT),
}

/// The laws of design D3, value from stored `p`.
pub mod law {
    pub fn tone_period(p: f32) -> f32 {
        p * 1024.0
    }
    pub fn tone_depth(p: f32) -> f32 {
        p * 64.0
    }
    pub fn tone_volume(p: f32) -> f32 {
        p * 256.0
    }
    pub fn tone_rate(p: f32) -> f32 {
        p * 16.0
    }
    pub fn drum_volume(p: f32) -> f32 {
        p * 512.0
    }
    pub fn drum_freq(p: f32) -> f32 {
        p * 4.0
    }
    pub fn drum_growth(p: f32) -> f32 {
        p / 64.0
    }
    pub fn drum_slope(p: f32) -> f64 {
        (2.0 * p as f64 - 1.0) / 16.0
    }
}

impl ToneT {
    pub fn from_shape(sh: &[f32; o::SHAPES]) -> Self {
        ToneT {
            carrier: law::tone_period(sh[sp::TONE_CARRIER]),
            modulator: law::tone_period(sh[sp::TONE_MODULATOR]),
            depth: law::tone_depth(sh[sp::TONE_DEPTH]),
            amp: f32t(law::tone_volume(sh[sp::TONE_VOLUME]) as f64 * 255.0),
            drive: sh[sp::TONE_DRIVE],
            shape: sh[sp::TONE_SHAPE],
            fm: choice(sh[sp::TONE_FM], 2) == 1,
            rate: law::tone_rate(sh[sp::TONE_RATE]),
        }
    }

    /// Frame `i`'s value before the shaper.
    fn raw(&self, i: i64) -> i64 {
        let t = i as f64 * self.rate as f64;
        let m = f32t(libm::sin(TWO_PI / self.modulator as f64 * t) * self.depth as f64) as f64;
        let ph = if self.fm { TWO_PI / (self.carrier as f64 + m + f32t(t) as f64 * f32t(1e-12) as f64) * t } else { m };
        trunc(libm::sin(ph) * self.amp as f64)
    }

    /// The drive stage of a raw value: the value after it and the carried value it sets, if any.
    fn drive_stage(&self, v: i64) -> (i64, Option<i64>) {
        let mut v = v;
        let mut set = None;
        if (v as f64).abs() < LOW {
            set = Some(v << 2);
            let d = self.drive as f64;
            v = trunc((1.0 - d) * v as f64 + d * (v * 4) as f64);
        }
        if v as f64 > HIGH {
            set = Some(idiv((16320 - idiv(v - 16320, 4)) * 127, 80));
        } else if (v as f64) < -HIGH {
            set = Some(idiv((-16320 - idiv(v + 16320, 4)) * 127, 80));
        }
        (v, set)
    }

    /// Frame `i` from the carried value before it: the output (16-bit scale) and the carried value after it.
    fn frame(&self, i: i64, carried: i64) -> (i64, i64) {
        let (v, set) = self.drive_stage(self.raw(i));
        let c = set.unwrap_or(carried);
        let s = self.shape as f64;
        (trunc((1.0 - s) * v as f64 + s * c as f64).clamp(-CLIP as i64, CLIP as i64), c)
    }

    /// The carried value before frame `i`: set by the last frame before it that sets one (frame 0 always does, as
    /// `sin 0 = 0` is below the low threshold), 0 before frame 0.
    pub fn carried_before(&self, i: i64) -> i64 {
        let mut j = i - 1;
        while j >= 0 {
            if let (_, Some(c)) = self.drive_stage(self.raw(j)) {
                return c;
            }
            j -= 1;
        }
        0
    }
}

/// A Sweep drum's running values before a frame: frequency, sweep and the carried shaper value.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DrumState {
    pub freq: f32,
    pub sweep: f32,
    pub carried: f32,
}

impl DrumT {
    pub fn from_shape(sh: &[f32; o::SHAPES]) -> Self {
        DrumT {
            amp: f32t(law::drum_volume(sh[sp::DRUM_VOLUME]) as f64 * 255.0),
            freq: law::drum_freq(sh[sp::DRUM_FREQ]),
            sweep: sh[sp::DRUM_SWEEP],
            accel: -law::drum_growth(sh[sp::DRUM_GROWTH]),
            drive: sh[sp::DRUM_DRIVE],
            shape: sh[sp::DRUM_SHAPE],
            env: sh[sp::DRUM_ENV],
            slope: law::drum_slope(sh[sp::DRUM_SLOPE]),
            curve: choice(sh[sp::DRUM_CURVE], 3) as u8,
        }
    }

    /// The state before frame 0.
    pub fn state0(&self) -> DrumState {
        DrumState { freq: self.freq, sweep: self.sweep, carried: 0.0 }
    }

    /// One step of the sweep: the frequency frame `i` plays at, and the state after it.
    #[inline]
    fn step(&self, st: &mut DrumState) {
        st.freq = f32t(st.freq as f64 - st.sweep as f64);
        st.sweep = f32t(st.sweep as f64 - self.accel as f64);
    }

    /// Frame `i`'s drive stage at frequency `freq`: the value after it and the carried value it sets, if any.
    fn drive_stage(&self, i: i64, freq: f32) -> (f64, Option<f32>) {
        let mut s = f32t(libm::sin(i as f64 * freq as f64) * self.amp as f64) as f64;
        let mut set = None;
        if s.abs() < LOW {
            set = Some(f32t(s * 4.0));
            let d = self.drive as f64;
            s = f32t((1.0 - d) * s + d * (s * 4.0)) as f64;
        }
        let k = f32t(0.0125) as f64;
        if s > HIGH {
            set = Some(f32t((HIGH - (s - HIGH) * 0.25) * 127.0 * k));
        } else if s < -HIGH {
            set = Some(f32t((-HIGH - (s + HIGH) * 0.25) * 127.0 * k));
        }
        (s, set)
    }

    /// Frame `i` (16-bit scale) from the state before it; leaves the state before frame `i + 1`.
    pub fn frame(&self, i: i64, st: &mut DrumState) -> f32 {
        self.step(st);
        let (s, set) = self.drive_stage(i, st.freq);
        if let Some(c) = set {
            st.carried = c;
        }
        let sh = self.shape as f64;
        let s = f32t((1.0 - sh) * s + sh * st.carried as f64) as f64;
        let s = s.clamp(-CLIP, CLIP);
        let e = (f32t(self.env as f64 - i as f64 * self.slope) as f64).clamp(0.0, 1.0);
        let y = match self.curve {
            0 => s * e,
            1 => (1.0 - libm::sin(HALF_PI - e * HALF_PI)) * s,
            _ => libm::cos(HALF_PI - e * HALF_PI) * s,
        };
        trunc(f32t(y) as f64) as f32
    }

    /// The state before frame `i`: the sweep replayed from frame 0 without its waveform (two f32 steps a frame), then
    /// the carried value from the last frame before `i` that set one, among the last [`BLOCK`] frames' frequencies
    /// (frame 0 always sets it); a longer gap replays every frame.
    pub fn state_at(&self, i: i64) -> DrumState {
        let mut st = self.state0();
        let mut ring = [0.0f32; BLOCK];
        for j in 0..i.max(0) {
            self.step(&mut st);
            ring[j as usize % BLOCK] = st.freq;
        }
        for j in (0.max(i - BLOCK as i64)..i).rev() {
            if let (_, Some(c)) = self.drive_stage(j, ring[j as usize % BLOCK]) {
                return DrumState { carried: c, ..st };
            }
        }
        if i <= 0 {
            return st;
        }
        let mut st = self.state0();
        for j in 0..i {
            self.frame(j, &mut st);
        }
        st
    }
}

/// An engine slot's running state: the next frame `i`, the direction, the tone's carried value or the drum's state,
/// and the drum's backward block (frames `lo..lo + BLOCK`, 16-bit integers as the engine makes them; `lo < 0`:
/// empty).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EngineSlot {
    pub i: i64,
    pub back: bool,
    pub carried: i64,
    pub drum: DrumState,
    pub lo: i64,
    pub block: [i16; BLOCK],
}

impl EngineSlot {
    pub const fn new() -> Self {
        EngineSlot { i: 0, back: false, carried: 0, drum: DrumState { freq: 0.0, sweep: 0.0, carried: 0.0 }, lo: -1, block: [0; BLOCK] }
    }

    /// Seeks to frame `start`, forwards or backwards (a non-legato note-on): the state is the one a run from frame 0
    /// has there.
    pub fn seek(&mut self, e: &EngineT, start: i64, back: bool) {
        self.i = start.max(0);
        self.back = back;
        self.lo = -1;
        match e {
            EngineT::Tone(t) => self.carried = t.carried_before(self.i),
            EngineT::Drum(d) if !back => self.drum = d.state_at(self.i),
            _ => {}
        }
    }

    /// The frame at `i` (output scale, /32768), then `i` moves one frame in the slot's direction (backwards it holds
    /// at frame 0).
    pub fn next(&mut self, e: &EngineT) -> f32 {
        let i = self.i;
        let y = match e {
            EngineT::None => 0.0,
            EngineT::Tone(t) => {
                let carried = if self.back { t.carried_before(i) } else { self.carried };
                let (y, c) = t.frame(i, carried);
                self.carried = c;
                y as f32
            }
            EngineT::Drum(d) if self.back => {
                if self.lo < 0 || i < self.lo || i >= self.lo + BLOCK as i64 {
                    self.lo = (i - (BLOCK as i64 - 1)).max(0);
                    let mut st = d.state_at(self.lo);
                    for (k, y) in self.block.iter_mut().enumerate() {
                        *y = d.frame(self.lo + k as i64, &mut st) as i16;
                    }
                }
                self.block[(i - self.lo) as usize] as f32
            }
            EngineT::Drum(d) => d.frame(i, &mut self.drum),
        };
        self.i = if self.back { (i - 1).max(0) } else { i + 1 };
        y * (1.0 / 32768.0)
    }
}

impl Default for EngineSlot {
    fn default() -> Self {
        Self::new()
    }
}

/// The note-on bytes of design D3 (add-plain-xm-import): the start frame `TRACKING·4096 + PM_OUT·16` and the backward
/// pass `FEEDBACK ≥ 128`, each byte `round(255p)`.
pub fn start_of(tracking: f32, pm_out: f32, feedback: f32) -> (i64, bool) {
    let b = |p: f32| libm::roundf(p * 255.0).clamp(0.0, 255.0) as i64;
    (b(tracking) * 4096 + b(pm_out) * 16, b(feedback) >= 128)
}

#[cfg(test)]
mod tests;
