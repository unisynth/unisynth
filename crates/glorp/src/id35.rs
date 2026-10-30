//! Glorp effect ID 35: a cascade of up to 256 all-pass stages with scattered centre frequencies, second-order topology
//! only (the only one the song uses). Independent behavioural implementation written from observed behaviour
//! (research/glorp-devices.md §35); no original binary or pseudocode is included. Settings outside the supported
//! subset are **rejected** with an [`Error`], never approximated:
//!
//! | param | meaning | accepted |
//! |---|---|---|
//! | 0 | base centre frequency `p²·19980 + 20` Hz | any |
//! | 1 | Q `max(0.5 + 19.5·p², 0.5)` (all-pass `α = sin w · 0.5 / Q`); its delay topology's feedback is not modelled | any |
//! | 2 | wet mix `(wet − dry)·p + dry` | any |
//! | 3 | stage count `trunc(p·256)`, 0..=256 | any (0 stages = dry) |
//! | 4 | frequency scatter: stage `i` is at `base·2^(u_i·3p)`, `u_i ∈ [−1, 1)` hashed from `i`, clamped to 20..20000 Hz | any |
//! | 5 | topology `trunc(p·2.999)`: 0 delay-line, 1 first-order, 2 second-order | **2 only** (`p ≥ 2/2.999`) |
//!
//! Every value must be finite and within `[0, 1]` (song bytes/255 are); index ≥ 6 is rejected (the original ignores it
//! silently). Fixed 44.1 kHz (the original never reads the sample rate: the stage coefficients bake in
//! `2π/44100`). Per stage an RBJ all-pass `c1 = −2·cos w/(1+α)`, `c2 = (1−α)/(1+α)` in direct form II:
//! `w = x − (c2·v + c1·u)`, `y = (c1·u + v) + c2·w`, then `v ← u`, `u ← w`; f32, scalar order of the reference, no FMA.
//!
//! Behaviour confirmed against the original: parameters are read at the start of every `process` call (coefficients
//! are a pure function of them, recomputed lazily here), nothing is ever reset (stage state is **kept** across
//! parameter and stage-count changes: stages above the live count keep their history and resume from it), note events do nothing, and
//! processing is in place (an input sample is read before its output is written; the original is called with the
//! same buffers for input and output). Not a `synth_core::Device`: its `set_param` cannot report a rejection.
//!
//! Numerics: the original links the C library's `powf`/`sinf`/`cosf`, whose last-ulp results are neither
//! correctly rounded nor `libm`'s. Here `2^e`, `sin w` and `cos w` are the f64 results rounded to f32: that reproduces
//! the original's coefficients bit for bit on most stages (all of them on the checked static parameter sets), and the
//! remaining last-ulp coefficient differences (5 of 152,400 stage instances in the song window) are amplified by a 254-stage
//! narrow-section cascade into at most 3.2e-3 output error (peak 3.4); every other part of the device is bit-exact
//! (`tests/reference35.rs`, `tests/reference35_structure.rs`). Output is therefore tolerance-matched, not bit-exact.
//! The measured bounds depend on the `libm` crate's `pow`/`sin`/`cos` as locked in `Cargo.lock` (0.2.16 when
//! measured), not on an exact version pin; a lockfile bump that shifts a last ulp calls for re-measuring them.

use synth_core::denormal::Guard;

/// Parameter count of the device.
pub const PARAMS: usize = 6;
/// Maximum stage count (`trunc(1.0 · 256)`).
pub const MAX_STAGES: usize = 256;

/// Why a setting is not supported. The device is left unchanged when one is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parameter index ≥ 6.
    Index,
    /// Not finite or outside `[0, 1]`.
    Range,
    /// Topology other than second-order (`trunc(P5·2.999) != 2`): delay-line and first-order are not supported.
    Topology,
}

/// 2π / 44100 as the reference's f32 constant (bits matter; not the correctly rounded quotient).
const W_PER_HZ: f32 = 0.000_142_475_85;
/// 2⁻³¹ (the hash is mapped to `[0, 2)` then shifted by −1).
const HASH_SCALE: f32 = 4.656_612_9e-10;

#[derive(Clone, Debug)]
pub struct Id35 {
    p: [f32; PARAMS],
    /// Per stage `[u_l, u_r, v_l, v_r]` (the two previous `w` values per channel); all 256 are kept.
    st: [[f32; 4]; MAX_STAGES],
    c1: [f32; MAX_STAGES],
    c2: [f32; MAX_STAGES],
    /// Parameters changed since the coefficients were computed.
    dirty: bool,
}

fn validate(v: f32) -> Result<(), Error> {
    if (0.0..=1.0).contains(&v) { Ok(()) } else { Err(Error::Range) }
}

fn topology(p5: f32) -> i32 {
    (p5 * 2.999) as i32
}

fn check_topology(p5: f32) -> Result<(), Error> {
    if topology(p5) == 2 { Ok(()) } else { Err(Error::Topology) }
}

/// Integer finaliser of `i · 1234567` as a uniform value in `[−1, 1)` (f32 steps as the reference does).
/// Rounding scope: `h as f32` rounds the 32-bit hash to a 24-bit mantissa (nearest-even), then `· 2⁻³¹ − 1` is an
/// f32 op, exactly as the reference; so `u` is a quantised value, not the exact `h/2³¹ − 1` that the independent
/// f64 model in `tests35.rs` uses (the model tolerance absorbs that difference; the original's coefficients are
/// matched bit for bit with this f32 form, `tests/reference35_structure.rs`).
fn scatter(i: usize) -> f32 {
    let mut h = (i as u32).wrapping_mul(1_234_567);
    h = (h ^ (h >> 16)).wrapping_mul(0x7feb_352d);
    h = (h ^ (h >> 15)).wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    (h as f32 * HASH_SCALE) + -1.0
}

/// Whether `new` accepts these parameters (cheap: builds nothing).
pub fn check(p: &[f32; PARAMS]) -> Result<(), Error> {
    p.iter().try_for_each(|&v| validate(v))?;
    check_topology(p[5])
}

impl Id35 {
    /// Builds the device from the 6 normalized parameters (a song chunk's first 6 floats).
    pub fn new(p: &[f32; PARAMS]) -> Result<Self, Error> {
        check(p)?;
        Ok(Id35 { p: *p, st: [[0.0; 4]; MAX_STAGES], c1: [0.0; MAX_STAGES], c2: [0.0; MAX_STAGES], dirty: true })
    }

    /// Sets one parameter between `process` calls (stage state is kept). On `Err` nothing changed.
    pub fn set_param(&mut self, index: usize, value: f32) -> Result<(), Error> {
        if index >= PARAMS {
            return Err(Error::Index);
        }
        validate(value)?;
        if index == 5 {
            check_topology(value)?;
        }
        self.p[index] = value;
        self.dirty = true;
        Ok(())
    }

    fn stages(&self) -> usize {
        ((self.p[3] * 256.0) as usize).min(MAX_STAGES)
    }

    fn recompute(&mut self) {
        let base = (self.p[0] * self.p[0]) * 19980.0 + 20.0;
        let inv_q = 1.0 / (((self.p[1] * self.p[1]) * 19.5 + 0.5).max(0.5));
        let spread = self.p[4] * 3.0;
        for i in 0..self.stages() {
            // f64 results rounded to f32 (libm's own `powf` is up to 1 ulp off the original's: 10% of stages, which `cos w`
            // near π/2 shows as coefficient errors up to 14 ulp and ~1e-3 relative output error)
            let f = (libm::pow(2.0, (scatter(i) * spread) as f64) as f32) * base;
            let m = if f < 20000.0 { f } else { 20000.0 };
            let f = if f < 20.0 { 20.0 } else { m };
            let w = f * W_PER_HZ;
            let a = ((libm::sin(w as f64) as f32) * 0.5) * inv_q;
            let inv = 1.0 / (a + 1.0);
            self.c1[i] = ((libm::cos(w as f64) as f32) * -2.0) * inv;
            self.c2[i] = (1.0 - a) * inv;
        }
        self.dirty = false;
    }

    /// Filters a stereo block in place. `l` and `r` must have the same length (panics otherwise). The parameters
    /// are fixed for the whole call; the stage state continues from the previous call.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        assert_eq!(l.len(), r.len(), "channel length mismatch");
        let _ftz = Guard::new();
        if self.dirty {
            self.recompute();
        }
        let n = self.stages();
        let mix = self.p[2];
        for i in 0..l.len() {
            let dry = [l[i], r[i]];
            let mut v = dry;
            for k in 0..n {
                let (c1, c2) = (self.c1[k], self.c2[k]);
                let s = &mut self.st[k];
                let (u, vv) = ([s[0], s[1]], [s[2], s[3]]);
                let mut w = [0.0; 2];
                for ch in 0..2 {
                    let t = c1 * u[ch];
                    w[ch] = v[ch] - ((c2 * vv[ch]) + t);
                    v[ch] = (t + vv[ch]) + c2 * w[ch];
                }
                *s = [w[0], w[1], u[0], u[1]];
            }
            l[i] = (v[0] - dry[0]) * mix + dry[0];
            r[i] = (v[1] - dry[1]) * mix + dry[1];
        }
    }
}
