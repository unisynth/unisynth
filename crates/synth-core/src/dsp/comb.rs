/// Freeverb comb: delay line with a one-pole lowpass in the feedback path.
/// `out = buf[i]; z = out·(1−damp) + z·damp; buf[i] = in + z·feedback`.
pub struct Comb<'a> {
    buf: &'a mut [f32],
    idx: usize,
    z: f32,
    pub feedback: f32,
    pub damp: f32,
}

impl<'a> Comb<'a> {
    /// Delay length = `buf.len()` (must be ≥ 1).
    pub fn new(buf: &'a mut [f32]) -> Self {
        Comb { buf, idx: 0, z: 0.0, feedback: 0.0, damp: 0.0 }
    }

    /// Zeroes the delay line and state (coefficients kept).
    pub fn clear(&mut self) {
        self.buf.fill(0.0);
        self.idx = 0;
        self.z = 0.0;
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        let out = self.buf[self.idx];
        self.z = out * (1.0 - self.damp) + self.z * self.damp;
        self.buf[self.idx] = input + self.z * self.feedback;
        self.idx += 1;
        if self.idx == self.buf.len() {
            self.idx = 0;
        }
        out
    }
}
