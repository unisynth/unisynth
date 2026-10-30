//! Song-used subset of Glorp effect ID 38 (a multimode filter with pre/post distortion and a cutoff envelope).
//!
//! Independent behavioural implementation written from observed behaviour (research/glorp-devices.md §38); no original binary or pseudocode is included. Supported:
//! RBJ low-pass / high-pass (one biquad stage), post distortion 0/1/2, envelope amount zero. Everything else the
//! 15 parameters can select is **rejected** with an [`Error`], never approximated:
//!
//! | param | meaning | accepted |
//! |---|---|---|
//! | 0 | mode `trunc(p·11.999)` | 0 = low-pass, 1 = high-pass |
//! | 1 | cutoff `p²·19980 + 20` Hz (clamped 20..20000) | any |
//! | 2 | Q `0.5 + 29.5·p²` | any |
//! | 3, 8..=11 | EQ gain, envelope ADSR | any (no effect with the filter modes and a neutral envelope) |
//! | 4 | stages `trunc(p·7.999) + 1` | 1 |
//! | 5 | pre distortion `trunc(p·5.999)` | 0 (none) |
//! | 6 | post distortion `trunc(p·5.999)` | 0 none, 1 rational tanh-like, 2 cubic-rational |
//! | 7 | drive `p²·20 + 0.101` (post types 1, 2) | any |
//! | 12 | envelope amount | exactly 0.5 (neutral: the cutoff is never modulated, so note events cannot matter) |
//! | 13 | wet mix | any |
//! | 14 | output gain `2p` (p ≤ .5) else `4p − 1.5` | any |
//!
//! Every value must be finite and within `[0, 1]` (song bytes/255 are). Fixed 44.1 kHz, 128-frame blocks are the
//! reference's; the device itself is block-size independent (parameters change between `process` calls). Not yet a
//! `synth_core::Device`: its `set_param` cannot report a rejection.
//!
//! Numerics follow the reference in f32 (operation order matters for bit parity). The reference divides with
//! `rcpps` + one Newton step, which is CPU-specific; this uses true division, so distorted output agrees to a
//! few 1e-7, non-distorted output bit for bit on the checked windows (see `tests/reference.rs`).

use synth_core::denormal::Guard;

/// Parameter count of the device.
pub const PARAMS: usize = 15;

/// Why a setting is not supported. The device is left unchanged when one is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parameter index ≥ 15.
    Index,
    /// Not finite or outside `[0, 1]`.
    Range,
    /// Filter mode other than low-pass / high-pass (ladder and the other families).
    Mode,
    /// More than one filter stage.
    Stages,
    /// Pre distortion selected.
    PreDistortion,
    /// Post distortion type above 2.
    PostDistortion,
    /// Envelope amount (param 12) not exactly neutral 0.5.
    Envelope,
}

/// 2π / 44100 as the reference's f32 constant (not the correctly rounded quotient: 2 ulp apart, bits matter).
const W_PER_HZ: f32 = 0.000_142_475_82;

#[derive(Clone, Debug)]
pub struct Id38 {
    p: [f32; PARAMS],
    /// b0, b1, b2 (= b0), a1, a2, all divided by a0.
    c: [f32; 5],
    post: u32,
    gain: f32,
    st: [[f32; 4]; 2],
}

/// Whether `new` / `set_param` accept this whole parameter vector (cheap: builds nothing).
pub fn check(p: &[f32; PARAMS]) -> Result<(), Error> {
    if p.iter().any(|v| !(0.0..=1.0).contains(v)) {
        return Err(Error::Range);
    }
    if ((p[0] * 11.999) as u32) > 1 {
        return Err(Error::Mode);
    }
    if ((p[4] * 7.999) as u32) != 0 {
        return Err(Error::Stages);
    }
    if ((p[5] * 5.999) as u32) != 0 {
        return Err(Error::PreDistortion);
    }
    if ((p[6] * 5.999) as u32) > 2 {
        return Err(Error::PostDistortion);
    }
    if p[12] != 0.5 {
        return Err(Error::Envelope);
    }
    Ok(())
}

impl Id38 {
    /// Builds the device from the 15 normalized parameters (a song chunk's first 15 floats).
    pub fn new(p: &[f32; PARAMS]) -> Result<Self, Error> {
        check(p)?;
        let mut d = Id38 { p: *p, c: [0.0; 5], post: 0, gain: 0.0, st: [[0.0; 4]; 2] };
        d.derive();
        Ok(d)
    }

    /// Sets one parameter between `process` calls (filter state is kept). On `Err` nothing changed.
    pub fn set_param(&mut self, index: usize, value: f32) -> Result<(), Error> {
        if index >= PARAMS {
            return Err(Error::Index);
        }
        let mut p = self.p;
        p[index] = value;
        check(&p)?;
        self.p = p;
        self.derive();
        Ok(())
    }

    fn derive(&mut self) {
        let p = &self.p;
        let q = (p[2] * p[2] * 29.5 + 0.5).max(0.5);
        let fc = (p[1] * p[1] * 19980.0 + 20.0).clamp(20.0, 20000.0);
        let w = fc * W_PER_HZ;
        let (sn, cs) = (libm::sinf(w), libm::cosf(w));
        let alpha = sn / (q.max(0.05) * 2.0);
        let (a0, a1, a2) = (1.0 + alpha, -2.0 * cs, 1.0 - alpha);
        let (b0, b1) = if (p[0] * 11.999) as u32 == 0 { (0.5 * (1.0 - cs), 1.0 - cs) } else { (cs * 0.5 + 0.5, -1.0 - cs) };
        let r = 1.0 / a0;
        self.c = [b0 * r, b1 * r, b0 * r, a1 * r, a2 / a0];
        self.post = (p[6] * 5.999) as u32;
        self.gain = if p[14] <= 0.5 { 2.0 * p[14] } else { p[14] * 4.0 - 1.5 };
    }

    /// Filters a stereo block in place. `l` and `r` must have the same length (panics otherwise). The auxiliary
    /// (sidechain) pair is not touched by this device and not part of the API.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        assert_eq!(l.len(), r.len(), "channel length mismatch");
        let _ftz = Guard::new();
        let (c, post, drive, mix, gain) = (self.c, self.post, self.p[7], self.p[13], self.gain);
        for (buf, s) in [l, r].into_iter().zip(self.st.iter_mut()) {
            let [mut x1, mut x2, mut y1, mut y2] = *s;
            for v in buf.iter_mut() {
                let x = *v;
                let y = c[0] * x + c[1] * x1 + c[2] * x2 - (c[3] * y1 + c[4] * y2);
                x2 = x1;
                x1 = x;
                y2 = y1;
                y1 = y;
                let wet = if post > 0 { distort(post, y, drive) } else { y };
                *v = ((wet - x) * mix + x) * gain;
            }
            *s = [x1, x2, y1, y2];
        }
    }
}

/// Post distortion, normalised so an input of 1 maps to 1. `k = drive²·20 + 0.101`, `u = k·x`:
/// kind 1: `R(u)/R(k)` with `R(u) = u(378u⁴ + 17325u² + 135135) / (18u⁶ + 3150u⁴ + 62370u² + 135135)`;
/// kind 2: `g(u)/g(k)` with `g(u) = u(u² + 27) / (9u² + 27)`; kind 0: identity. Large |x| overflows to NaN like the
/// reference's unbounded polynomial would.
pub(crate) fn distort(kind: u32, x: f32, drive: f32) -> f32 {
    let k = drive * drive * 20.0 + 0.101;
    let u = k * x;
    let uu = u * u;
    match kind {
        1 => {
            let kk = k * k;
            let t = (k * (((378.0 * kk + 17325.0) * kk) + 135135.0)) / (kk * ((18.0 * kk + 3150.0) * kk + 62370.0) + 135135.0);
            let n = (((378.0 * uu + 17325.0) * uu) + 135135.0) * u;
            let d = ((18.0 * uu + 3150.0) * uu + 62370.0) * uu + 135135.0;
            n / (t * d)
        }
        2 => {
            let kk = k * k;
            let t = ((kk + 27.0) * k) / (kk * 9.0 + 27.0);
            let n = (uu + 27.0) * u;
            let d = uu * 9.0 + 27.0;
            n / (t * d)
        }
        _ => x,
    }
}
