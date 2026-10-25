/// One-pole lowpass `z += k·(x − z)`.
#[derive(Clone, Copy, Debug, Default)]
pub struct OnePole {
    pub z: f32,
}

impl OnePole {
    #[inline]
    pub fn lowpass(&mut self, x: f32, k: f32) -> f32 {
        self.z += k * (x - self.z);
        self.z
    }
}
