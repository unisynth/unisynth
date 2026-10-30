//! Glorp effect ID 34: a Hilbert (single-sideband) frequency shifter, complete (4 parameters, 1 of them live).
//!
//! Independent behavioural implementation written from observed behaviour (research/glorp-devices.md §34); no
//! original binary, table or pseudocode is included. Signal flow per channel, f32, all arithmetic in the order
//! below because operation order matters for bit parity:
//!
//! * Two chains of four second-order all-pass sections `y = c·x − x₂ + f·y₂` (`x₂`/`y₂` = the section's input/output
//!   two samples ago): chain A takes the input, chain B takes the input delayed by one sample. The coefficients `c` are
//!   the squares of Niemitalo's published polyphase Hilbert coefficients, rounded to 6 digits; the feedback `f` equals
//!   `c` except in the first section of chain A, which uses 0.1617158 (instead of 0.161758; observed, kept).
//! * A phase `φ` advances once per sample by `inc = 1 ± (2·P3 − 1)⁸` (minus for P3 < 0.5), subtracting 1 when it
//!   exceeds 1 (one subtraction at most, so a positive shift grows without bound: see below). `inc` is computed once
//!   per `process` call (the reference's block granularity), so a parameter change takes effect on the next call.
//! * Output `cos(2πφ)·A + sin(2πφ)·B` with a 512-entry sine table and linear interpolation (`table_sin`).
//!
//! The net shift is `shift_cycles_per_sample(P3)` cycles per sample (× sample rate for Hz; the original never reads
//! the rate): P3 < 0.5 shifts **up**, P3 > 0.5 **down**, `|shift| = (2·P3 − 1)⁸` (steep: 0.8549 ≈ 2.84 kHz at
//! 44.1 kHz, ±1 cycle/sample at the extremes). P0..P2 are stored by the original but never read: accepted and ignored.
//!
//! Validity: every value must be finite and within `[0, 1]` (song bytes/255 are); index ≥ 4 is rejected (the original
//! ignores it silently). Limitations: `φ` is an f32 that, for a downward shift (positive increment), keeps growing, so
//! after roughly 2²⁰ samples the phase resolution is coarser than the table step: the original degrades the same way
//! and so does this. Note events are not part of the API (the original's note slots are empty).
//!
//! Not a `synth_core::Device`: its `set_param` cannot report a rejection.

use synth_core::denormal::Guard;

/// Parameter count of the device.
pub const PARAMS: usize = 4;

/// Why a setting is not supported. The device is left unchanged when one is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parameter index ≥ 4.
    Index,
    /// Not finite or outside `[0, 1]`.
    Range,
}

/// Section coefficients `(c, −1, −f)`: `y = c·x + (−1)·x₂ − (−f)·y₂`.
pub(crate) type Section = [f32; 3];

pub(crate) const CHAIN_A: [Section; 4] = [
    [0.161_758, -1.0, -0.161_715_8],
    [0.733_029, -1.0, -0.733_029],
    [0.945_35, -1.0, -0.945_35],
    [0.990_598, -1.0, -0.990_598],
];
pub(crate) const CHAIN_B: [Section; 4] = [
    [0.479_401, -1.0, -0.479_401],
    [0.876_218, -1.0, -0.876_218],
    [0.976_599, -1.0, -0.976_599],
    [0.997_5, -1.0, -0.997_5],
];

/// 2π and 1/2π as the reference's f32 constants; π/2 likewise.
pub(crate) const TAU: f32 = 6.283_185_5;
const INV_TAU: f32 = 0.159_154_94;
pub(crate) const HALF_PI: f32 = 1.570_796_4;
/// Table step 2π/512 as the reference's f32 constant (bits matter: not the correctly rounded quotient).
const TABLE_STEP: f32 = 0.012_271_847;
pub(crate) const TABLE_LEN: usize = 513;

/// One section's history: `[x₁, x₂, y₁, y₂]` for both channels live in [`Id34::st`].
pub(crate) type Hist = [f32; 4];

#[derive(Clone, Debug)]
pub struct Id34 {
    p: [f32; PARAMS],
    phase: f32,
    /// `st[chain][section][channel]`.
    st: [[[Hist; 2]; 4]; 2],
    /// Previous input sample per channel (chain B's one-sample delay).
    prev: [f32; 2],
    table: [f32; TABLE_LEN],
}

fn validate(v: f32) -> Result<(), Error> {
    if (0.0..=1.0).contains(&v) { Ok(()) } else { Err(Error::Range) }
}

/// `±(2p−1)⁸` as the reference evaluates it, before the `+ 1` that makes it a phase increment: negative (phase runs
/// slower, i.e. the spectrum moves up) for p < 0.5.
fn phase_delta(p3: f32) -> f32 {
    let x = (p3 + p3) + -1.0;
    let x2 = x * x;
    let x4 = x2 * x2;
    let x8 = x4 * x4;
    if p3 < 0.5 { -x8 } else { x8 }
}

/// Signed frequency shift in cycles per sample for parameter 3 in `[0, 1]`: positive = up. Multiply by the sample
/// rate for Hz.
pub fn shift_cycles_per_sample(p3: f32) -> f32 {
    -phase_delta(p3)
}

/// `cvttps2dq`: truncate toward zero, out-of-range and NaN give `i32::MIN`.
fn cvtt(t: f32) -> i32 {
    if t >= -2_147_483_648.0 && t < 2_147_483_648.0 { t as i32 } else { i32::MIN }
}

/// The reference's sine: table lookup with linear interpolation. `x` in radians; the fraction of `x/2π` is read
/// from the mantissa of `frac + 1 ∈ [1, 2)`: top 9 bits index the table, the low 14 bits interpolate.
pub(crate) fn sin_with(table: &[f32; TABLE_LEN], x: f32) -> f32 {
    let t = x * INV_TAU;
    let fl = cvtt(t) as f32;
    let floor = if t < fl { -1.0 + fl } else { fl };
    let bits = ((t + 1.0) - floor).to_bits();
    let i = ((bits >> 14) & 0x1ff) as usize;
    let w = ((bits & 0x3fff) << 9) as f32 * 1.192_092_9e-7;
    let (a, b) = (table[i], table[i + 1]);
    (b - a) * w + a
}

pub(crate) fn make_table() -> [f32; TABLE_LEN] {
    let mut t = [0.0; TABLE_LEN];
    for (i, v) in t.iter_mut().enumerate() {
        *v = libm::sinf(i as f32 * TABLE_STEP);
    }
    t
}

#[cfg(test)]
pub(crate) fn table_sin(x: f32) -> f32 {
    sin_with(&make_table(), x)
}

pub(crate) fn tick(c: &Section, s: &mut Hist, x: f32) -> f32 {
    let y = (c[0] * x + c[1] * s[1]) - c[2] * s[3];
    *s = [x, s[0], y, s[2]];
    y
}

/// Whether `new` accepts these parameters (cheap: builds nothing). `Err` leaves nothing to undo.
pub fn check(p: &[f32; PARAMS]) -> Result<(), Error> {
    p.iter().try_for_each(|&v| validate(v))
}

impl Id34 {
    /// Builds the device from the 4 normalized parameters (a song chunk's first 4 floats).
    pub fn new(p: &[f32; PARAMS]) -> Result<Self, Error> {
        check(p)?;
        Ok(Id34 { p: *p, phase: 0.0, st: [[[[0.0; 4]; 2]; 4]; 2], prev: [0.0; 2], table: make_table() })
    }

    /// Sets one parameter between `process` calls (filter and phase state are kept). On `Err` nothing changed.
    pub fn set_param(&mut self, index: usize, value: f32) -> Result<(), Error> {
        if index >= PARAMS {
            return Err(Error::Index);
        }
        validate(value)?;
        self.p[index] = value;
        Ok(())
    }

    /// Shifts a stereo block in place. `l` and `r` must have the same length (panics otherwise). The shift is fixed
    /// for the whole call; the phase and the all-pass state continue from the previous call.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        assert_eq!(l.len(), r.len(), "channel length mismatch");
        let _ftz = Guard::new();
        let inc = phase_delta(self.p[3]) + 1.0;
        for i in 0..l.len() {
            let x = [l[i], r[i]];
            let mut a = x;
            let mut b = self.prev;
            for (ch, (a, b)) in a.iter_mut().zip(b.iter_mut()).enumerate() {
                for k in 0..4 {
                    *a = tick(&CHAIN_A[k], &mut self.st[0][k][ch], *a);
                    *b = tick(&CHAIN_B[k], &mut self.st[1][k][ch], *b);
                }
            }
            self.prev = x;
            let v = self.phase + inc;
            self.phase = if 1.0 < v { v + -1.0 } else { v };
            let arg = self.phase * TAU;
            let c = sin_with(&self.table, HALF_PI + arg);
            let s = sin_with(&self.table, arg);
            l[i] = c * a[0] + s * b[0];
            r[i] = c * a[1] + s * b[1];
        }
    }
}
