/// Freeverb (Schroeder) allpass: `out = buf[i] − in; buf[i] = in + buf[i]·feedback`.
pub struct AllPass<'a> {
    buf: &'a mut [f32],
    idx: usize,
    pub feedback: f32,
}

impl<'a> AllPass<'a> {
    /// Delay length = `buf.len()` (must be ≥ 1).
    pub fn new(buf: &'a mut [f32]) -> Self {
        AllPass { buf, idx: 0, feedback: 0.0 }
    }

    /// Zeroes the delay line and state (coefficients kept).
    pub fn clear(&mut self) {
        self.buf.fill(0.0);
        self.idx = 0;
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        let b = self.buf[self.idx];
        self.buf[self.idx] = crate::denormal::flush(input + b * self.feedback);
        self.idx += 1;
        if self.idx == self.buf.len() {
            self.idx = 0;
        }
        -input + b
    }
}
