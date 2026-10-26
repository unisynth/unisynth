//! A control value that moves linearly over one control segment (design D5). `to` starts a segment from the previous
//! segment's end value, so the value is continuous and errors never accumulate; `retarget` changes the remaining
//! part of a segment from the current value (after an event that splits it).

#[derive(Clone, Copy, Debug, Default)]
pub struct Ramp {
    v: f32,
    dv: f32,
    end: f32,
}

impl Ramp {
    pub const fn new(v: f32) -> Self {
        Ramp { v, dv: 0.0, end: v }
    }

    /// Jumps to `v` and stays there (voice start).
    pub fn set(&mut self, v: f32) {
        *self = Ramp::new(v);
    }

    /// Starts a segment of `frames` (> 0) from the previous segment's end value to `target`.
    pub fn to(&mut self, target: f32, frames: u32) {
        self.v = self.end;
        self.retarget(target, frames);
    }

    /// Re-aims the rest of the segment (`frames` > 0 left) from the current value.
    pub fn retarget(&mut self, target: f32, frames: u32) {
        self.end = target;
        self.dv = (target - self.v) / frames.max(1) as f32;
    }

    /// The value for this frame, then one step.
    #[inline]
    pub fn next(&mut self) -> f32 {
        let r = self.v;
        self.v += self.dv;
        r
    }

    /// The value the segment ends at.
    pub fn end(&self) -> f32 {
        self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaches_the_target_in_the_segment_and_continues_from_it() {
        let mut r = Ramp::new(1.0);
        r.to(3.0, 4);
        let v: Vec<f32> = (0..4).map(|_| r.next()).collect();
        assert_eq!(v, [1.0, 1.5, 2.0, 2.5]);
        r.to(2.0, 2); // starts exactly at the previous end, not at the accumulated float
        assert_eq!((r.next(), r.next()), (3.0, 2.5));
        assert_eq!(r.end(), 2.0);
    }

    #[test]
    fn retarget_continues_from_the_current_value() {
        let mut r = Ramp::new(0.0);
        r.to(8.0, 8);
        r.next();
        r.next();
        r.next(); // v = 3
        r.retarget(4.0, 2);
        assert_eq!((r.next(), r.next()), (3.0, 3.5));
        r.set(7.0);
        assert_eq!((r.next(), r.next(), r.end()), (7.0, 7.0, 7.0));
    }
}
