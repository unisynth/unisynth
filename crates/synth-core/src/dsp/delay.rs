/// Circular delay line over a fixed-capacity buffer, with an active length ≤ capacity.
/// Taps count samples ago: `at(1)` is the latest write, `at(len)` = `at(0)` = `oldest()`.
pub struct Ring<'a> {
    buf: &'a mut [f32],
    len: usize,
    pos: usize,
}

impl<'a> Ring<'a> {
    /// Capacity and initial length = `buf.len()` (must be ≥ 1).
    pub fn new(buf: &'a mut [f32]) -> Self {
        let len = buf.len();
        Ring { buf, len, pos: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// Sets the active length (clamped to 1..=capacity). A changed length clears the buffer and restarts at 0;
    /// the same length changes nothing.
    pub fn set_len(&mut self, n: usize) {
        let n = n.clamp(1, self.buf.len());
        if n != self.len {
            self.len = n;
            self.pos = 0;
            self.buf[..n].fill(0.0);
        }
    }

    /// Sets the active length (clamped to 1..=capacity), keeping the contents and clamping the position into it.
    pub fn set_len_keep(&mut self, n: usize) {
        self.len = n.clamp(1, self.buf.len());
        self.pos = self.pos.min(self.len - 1);
    }

    /// Zeroes the active samples and restarts at position 0 (length kept).
    pub fn clear(&mut self) {
        self.buf[..self.len].fill(0.0);
        self.pos = 0;
    }

    #[inline]
    pub fn write(&mut self, x: f32) {
        self.buf[self.pos] = crate::denormal::flush(x);
        self.pos += 1;
        if self.pos == self.len {
            self.pos = 0;
        }
    }

    /// The slot written next, i.e. the sample written `len` writes ago.
    #[inline]
    pub fn oldest(&self) -> f32 {
        self.buf[self.pos]
    }

    /// Sample written `k` writes ago (modulo the length).
    #[inline]
    pub fn at(&self, k: usize) -> f32 {
        let k = k % self.len;
        let i = if k <= self.pos { self.pos - k } else { self.pos + self.len - k };
        self.buf[i]
    }

    /// Linear interpolation between `at(floor d)` and `at(floor d + 1)`; `d` ≥ 0.
    #[inline]
    pub fn read_frac(&self, d: f32) -> f32 {
        let k = d as usize;
        let f = d - k as f32;
        let s0 = self.at(k);
        s0 + f * (self.at(k + 1) - s0)
    }
}
