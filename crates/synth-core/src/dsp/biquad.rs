/// Normalised biquad coefficients: `y = c1·x + c2·x1 + c3·x2 − c4·y1 − c5·y2`
/// (`c1..c3 = b0..b2 / a0`, `c4, c5 = a1, a2 / a0`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Coefs {
    pub c1: f32,
    pub c2: f32,
    pub c3: f32,
    pub c4: f32,
    pub c5: f32,
}

impl Coefs {
    pub fn from_ba(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        Coefs { c1: b0 / a0, c2: b1 / a0, c3: b2 / a0, c4: a1 / a0, c5: a2 / a0 }
    }

    /// RBJ cookbook lowpass from `cos ω0` and `α`. Engines pass their own sine/cosine (parity).
    pub fn lowpass(cos_w0: f32, alpha: f32) -> Self {
        Self::from_ba((1.0 - cos_w0) / 2.0, 1.0 - cos_w0, (1.0 - cos_w0) / 2.0, 1.0 + alpha, -2.0 * cos_w0, 1.0 - alpha)
    }

    /// RBJ cookbook highpass.
    pub fn highpass(cos_w0: f32, alpha: f32) -> Self {
        Self::from_ba((1.0 + cos_w0) / 2.0, -(1.0 + cos_w0), (1.0 + cos_w0) / 2.0, 1.0 + alpha, -2.0 * cos_w0, 1.0 - alpha)
    }

    /// RBJ cookbook peaking EQ, `a = 10^(gain_db/40)`.
    pub fn peak(cos_w0: f32, alpha: f32, a: f32) -> Self {
        Self::from_ba(1.0 + alpha * a, -2.0 * cos_w0, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos_w0, 1.0 - alpha / a)
    }

    /// `(sin ω0, cos ω0)` via `libm` for `freq` Hz at `rate`.
    pub fn w0(freq: f32, rate: f32) -> (f32, f32) {
        libm::sincosf(2.0 * core::f32::consts::PI * freq / rate)
    }

    pub fn lowpass_hz(freq: f32, q: f32, rate: f32) -> Self {
        let (s, c) = Self::w0(freq, rate);
        Self::lowpass(c, s / (2.0 * q))
    }

    pub fn highpass_hz(freq: f32, q: f32, rate: f32) -> Self {
        let (s, c) = Self::w0(freq, rate);
        Self::highpass(c, s / (2.0 * q))
    }

    pub fn peak_hz(freq: f32, q: f32, gain_db: f32, rate: f32) -> Self {
        let (s, c) = Self::w0(freq, rate);
        Self::peak(c, s / (2.0 * q), libm::powf(10.0, gain_db / 40.0))
    }
}

/// Direct Form I biquad state.
#[derive(Clone, Copy, Debug, Default)]
pub struct Biquad {
    pub coefs: Coefs,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let c = &self.coefs;
        let y = c.c1 * x + c.c2 * self.x1 + c.c3 * self.x2 - c.c4 * self.y1 - c.c5 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}
