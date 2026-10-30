//! Maths shared by every engine, computed with `libm` so results match on all targets.
//! Engine-specific laws (WaveSabre's `exp2(db/6)` dB curve, its sine table) live in the engine crates.

/// 12-TET, A4 = note 69 = 440 Hz.
pub fn note_to_hz(note: f32) -> f32 {
    440.0 * libm::exp2f((note - 69.0) / 12.0)
}

/// Double-precision variant for engines with f64 phase accumulators.
pub fn note_to_hz_f64(note: f64) -> f64 {
    440.0 * libm::exp2((note - 69.0) / 12.0)
}

/// `10^(db/20)`.
pub fn db_to_gain(db: f32) -> f32 {
    libm::powf(10.0, db / 20.0)
}

/// `20·log10(gain)`.
pub fn gain_to_db(gain: f32) -> f32 {
    20.0 * libm::log10f(gain)
}

/// Equal-power pan, left gain for pan `p` in [0, 1] (0 = left).
pub fn pan_left(p: f32) -> f32 {
    libm::sqrtf(1.0 - p)
}

/// Equal-power pan, right gain.
pub fn pan_right(p: f32) -> f32 {
    libm::sqrtf(p)
}

pub fn clamp(x: f32, min: f32, max: f32) -> f32 {
    if x < min {
        min
    } else if x > max {
        max
    } else {
        x
    }
}

/// `a·(1−t) + b·t`, in this operation order (parity with upstream engines).
pub fn mix(a: f32, b: f32, t: f32) -> f32 {
    a * (1.0 - t) + b * t
}

/// `a + (b−a)·t`, written as one subtraction, one multiplication, one addition, in this order (the form the original
/// Glorp engine writes its lanes in, see `LaneLerp::Delta`). The source requests no fused multiply-add, but Rust does
/// not guarantee the compiler never fuses or reassociates float ops on a given target, so bit-exactness is not
/// promised by the language: revalidate against the Glorp reference whenever the target or toolchain changes.
/// Differs from [`mix`] in the last bit for a few percent of inputs.
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn exp2(x: f32) -> f32 {
    libm::exp2f(x)
}
