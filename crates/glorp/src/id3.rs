//! Glorp effect ID 3: a stereo waveshaper / saturator (four shaping modes, optional 2x / 4x averaging "oversampling",
//! input / output gain, dry-wet mix). Independent behavioural implementation written from observed behaviour
//! (research/glorp-devices.md §ID 3); no original binary or pseudocode is included. Settings outside what is supported
//! are **rejected** with an [`Error`], never approximated:
//!
//! | param | meaning | law (u = value) |
//! |---|---|---|
//! | 0 | drive `k` | `u < 0.2`: `u + 0.8`; else `5·(5u − 1)² + 1` (continuous at 0.2, 1..81) |
//! | 1 | ceiling `c` | the shaper input is divided by `c` and its output multiplied by `c` (clip level `c`; `0` is allowed and silences the wet path) |
//! | 2 | fold amount | mode 0 only: overshoot beyond ±1 is reflected with this factor (0 = hard clip) |
//! | 3 | mix | `out = x + (wet − x)·u` |
//! | 4 | mode | `trunc(u · 3.999)`: 0 clip / fold, 1 `×π/2` then hard clip, 2 cubic `y³` then hard clip, 3 soft knee |
//! | 5 | oversampling | `trunc(2u)`: 0 (`u < 0.5`) none, 1 (`0.5 ≤ u < 1`) 2-point average, 2 (`u = 1`) 4-point average |
//! | 6 | input gain | `u ≤ 0.5`: `2u`; else `4u − 1.5` (0..2.5) |
//! | 7 | output gain | same law as P6 |
//! | 8 | knee threshold | mode 3 only |
//! | 9 | knee width `w` | mode 3 only |
//! | 10 | slope above the knee | mode 3 only |
//!
//! Every value must be finite and within `[0, 1]` (song bytes/255 are); index ≥ 11 is rejected (the original ignores it
//! silently). Out-of-range values, which the original would accept as raw floats, are therefore **unsupported**; so is
//! nothing else: every in-range setting, including `c = 0` and `w = 0` (the degenerate divisions follow IEEE exactly as the
//! reference), is modelled. Fixed behaviour, sample-rate independent.
//!
//! Shaper `S(x, k)` (f32, scalar order of the reference, no FMA): `y = x·k / c`; mode 0 with fold `f > 0` replaces an
//! overshoot `y < −1` by `f·(−1 − y) − 1` and `y > 1` by `f·(1 − y) + 1`; mode 1 `y·1.570796`; mode 2 `y·(y·y)`; mode 3
//! on `a = |y|` (the sign of a NaN counts as negative, of 0 as 0) with `t = a − thr`: if `2t < −w` (or NaN) `a` is kept,
//! if `2t > w` the result is `t·slope + thr`, between `a + (t + w/2)²·(slope − 1) / (2w)`, then the sign is restored. Every
//! mode ends in the clamp to `[−1, 1]` (a NaN becomes −1) times `c`.
//!
//! Per channel (the left block is processed entirely before the right one, one `prev` per channel, no state shared
//! between channels): with `x` the input, `a = S(x·g_in, k)·g_out`; no oversampling: `wet = a`; 2x: `wet = (S((prev + x)/2, k) + a)/2`;
//! 4x with `m = (prev + x)/2`: `wet = (a + S(m) + S((prev + m)/2) + S((m + x)/2))/4` (summed in that order). **The
//! sub-sample paths get neither the input nor the output gain** (observed; only the direct path does). Then `out = x + (wet − x)·mix`
//! and `prev = x` (the raw input). The aux pair is neither read nor written. An infinite input yields NaN (`inf − inf`),
//! also for mix 0. A NaN reaching the shaper (a NaN input sample, or the previous sample of an oversampled path on the next
//! sample) is clamped to −1 (times `c`), so only the NaN sample itself comes out NaN (`x + …`), not its successor. Nothing is ever reset; parameters
//! are re-read at the start of each `process` call, so a change applies from the next call and block-split processing is
//! bit-identical. Subnormals are flushed like the reference.
//!
//! Numerics: pure IEEE `+ − × /` on f32 except the knee's `powf(·, 2)`, computed as `x·x` (the f32 square equals the correctly
//! rounded `pow`). Measured bit-exact against the original (`tests/reference3.rs`). Not a `synth_core::Device`: its
//! `set_param` cannot report a rejection.

use synth_core::denormal::Guard;

/// Parameter count of the device.
pub const PARAMS: usize = 11;

/// Why a setting is not supported. The device is left unchanged when one is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parameter index ≥ 11.
    Index,
    /// Not finite or outside `[0, 1]`.
    Range,
}

/// The reference's f32 `π/2` constant (bits `0x3fc90fd8`, not `FRAC_PI_2`).
const PI_HALF: f32 = 1.570_796;
/// Mode scale: `trunc(u · 3.999)`.
const MODE_SCALE: f32 = 3.999;

#[derive(Clone, Debug)]
pub struct Id3 {
    /// P0 drive input (raw value)
    drive: f32,
    /// P1
    ceil: f32,
    /// P2
    fold: f32,
    /// P3
    mix: f32,
    /// P4 decoded
    mode: u8,
    /// P5 decoded
    over: u8,
    /// P6, P7 (raw values)
    gain_in: f32,
    gain_out: f32,
    /// P8, P9, P10
    thr: f32,
    width: f32,
    slope: f32,
    /// last raw input per channel
    prev: [f32; 2],
}

fn validate(v: f32) -> Result<(), Error> {
    if (0.0..=1.0).contains(&v) { Ok(()) } else { Err(Error::Range) }
}

/// `u ≤ 0.5`: `2u`, else `4u − 1.5` (the reference selects on `0.5 < u`).
fn gain_law(u: f32) -> f32 {
    if 0.5 < u { u * 4.0 + -1.5 } else { u + u }
}

fn drive_law(u: f32) -> f32 {
    if 0.2 > u {
        u + 0.8
    } else {
        let t = u * 5.0 + -1.0;
        t * t * 5.0 + 1.0
    }
}

/// Whether `new` accepts these parameters (cheap: builds nothing). `Err` leaves nothing to undo.
pub fn check(p: &[f32; PARAMS]) -> Result<(), Error> {
    p.iter().try_for_each(|&v| validate(v))
}

impl Id3 {
    /// Builds the device from the 11 normalized parameters (a song chunk's first 11 floats).
    pub fn new(p: &[f32; PARAMS]) -> Result<Self, Error> {
        check(p)?;
        let mut d = Id3 {
            drive: 0.2,
            ceil: 0.8,
            fold: 0.0,
            mix: 1.0,
            mode: 0,
            over: 0,
            gain_in: 0.5,
            gain_out: 0.5,
            thr: 0.2,
            width: 1.0,
            slope: 0.0,
            prev: [0.0; 2],
        };
        for (i, &v) in p.iter().enumerate() {
            d.apply(i, v);
        }
        Ok(d)
    }

    /// Sets one parameter between `process` calls (the oversampling history is kept). On `Err` nothing changed.
    pub fn set_param(&mut self, index: usize, value: f32) -> Result<(), Error> {
        if index >= PARAMS {
            return Err(Error::Index);
        }
        validate(value)?;
        self.apply(index, value);
        Ok(())
    }

    fn apply(&mut self, i: usize, v: f32) {
        match i {
            0 => self.drive = v,
            1 => self.ceil = v,
            2 => self.fold = v,
            3 => self.mix = v,
            4 => self.mode = (v * MODE_SCALE) as u8,
            5 => self.over = (v + v) as u8,
            6 => self.gain_in = v,
            7 => self.gain_out = v,
            8 => self.thr = v,
            9 => self.width = v,
            _ => self.slope = v,
        }
    }

    /// The waveshaper `S(x, k)`.
    fn shape(&self, x: f32, k: f32) -> f32 {
        let mut y = x * k / self.ceil;
        match self.mode {
            0 => {
                if self.fold > 0.0 {
                    // `y < −1` or NaN takes the lower branch (NaN stays NaN)
                    if !(y >= -1.0) {
                        y = self.fold * (-1.0 - y) + -1.0;
                    } else if y > 1.0 {
                        y = self.fold * (1.0 - y) + 1.0;
                    }
                }
            }
            1 => y *= PI_HALF,
            2 => y *= y * y,
            _ => {
                let s: f32 = if y > 0.0 {
                    1.0
                } else if y < 0.0 || y.is_nan() {
                    -1.0
                } else {
                    0.0
                };
                let mut a = y * s;
                let t = a - self.thr;
                let t2 = t + t;
                // skipped when `2t < −w` and when unordered
                if !(t2 < -self.width) && !t2.is_nan() {
                    if t2 > self.width {
                        a = t * self.slope + self.thr;
                    } else {
                        let sq = (t + 0.5 * self.width) * (t + 0.5 * self.width);
                        a += sq * (self.slope + -1.0) / (self.width + self.width);
                    }
                }
                y = a * s;
            }
        }
        // clamp to [−1, 1]; NaN becomes −1
        let c = if y < -1.0 || y.is_nan() {
            -1.0
        } else if y > 1.0 {
            1.0
        } else {
            y
        };
        c * self.ceil
    }

    /// Processes `l` / `r` in place (the aux pair of the original is out of scope). Panics when the lengths differ.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        assert_eq!(l.len(), r.len(), "equal length required");
        let _ftz = Guard::new();
        let k = drive_law(self.drive);
        let gi = gain_law(self.gain_in);
        let go = gain_law(self.gain_out);
        for (c, buf) in [l, r].into_iter().enumerate() {
            for out in buf.iter_mut() {
                let x = *out;
                let direct = self.shape(x * gi, k) * go;
                let wet = match self.over {
                    2 => {
                        let p = self.prev[c];
                        let m = (p + x) * 0.5;
                        let q0 = (p + m) * 0.5;
                        let q2 = (m + x) * 0.5;
                        let (a, b, d) = (self.shape(q0, k), self.shape(m, k), self.shape(q2, k));
                        (((direct + b) + a) + d) * 0.25
                    }
                    1 => {
                        let h = self.shape((self.prev[c] + x) * 0.5, k);
                        (h + direct) * 0.5
                    }
                    _ => direct,
                };
                *out = x + (wet - x) * self.mix;
                self.prev[c] = x;
            }
        }
    }
}
