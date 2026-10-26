//! Formant (Tunefish): a 10th-order all-pole vowel filter in f64, mixed linearly with the dry signal. Coefficients:
//! the public-domain "Formant filter" by Alex (alex@smartelectronix.com), musicdsp.org archive #110, designed for
//! 44.1 kHz, soprano vowels A, E, I, O, U — with the A input gain toned down to 3.11044e-06 as suggested in that
//! page's comments (2003-10-04).
//!
//! At another song rate the vowel keeps its formant frequencies in Hz: the filter's poles are moved by the
//! matched-pole mapping `z → z^(44100/rate)` (angles scale, radii keep their decay per second) and the input gain
//! is scaled to keep the DC gain. At 44.1 kHz the table is used as it stands. Parameters after the mode: vowel, wet.

use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

/// Per vowel: input gain, then 10 feedback coefficients.
const VOWELS: [[f64; 11]; 5] = [
    [3.11044e-06, 8.943665402, -36.83889529, 92.01697887, -154.337906, 181.6233289, -151.8651235, 89.09614114, -35.10298511, 8.388101016, -0.923313471],
    [4.36215e-06, 8.90438318, -36.55179099, 91.05750846, -152.422234, 179.1170248, -149.6496211, 87.78352223, -34.60687431, 8.282228154, -0.914150747],
    [3.33819e-06, 8.893102966, -36.49532826, 90.96543286, -152.4545478, 179.4835618, -150.315433, 88.43409371, -34.98612086, 8.407803364, -0.932568035],
    [1.13572e-06, 8.994734087, -37.2084849, 93.22900521, -156.6929844, 184.596544, -154.3755513, 90.49663749, -35.58964535, 8.478996281, -0.929252233],
    [4.09431e-07, 8.997322763, -37.20218544, 93.11385476, -156.2530937, 183.7080141, -153.2631681, 89.59539726, -35.12454591, 8.338655623, -0.910251753],
];

const REF_RATE: f64 = 44100.0;

type C = (f64, f64);

fn mul(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

fn div(a: C, b: C) -> C {
    let d = b.0 * b.0 + b.1 * b.1;
    ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
}

/// `z^10 − Σ c[j]·z^(10−j)` at `z` (Horner).
fn poly(c: &[f64; 11], z: C) -> C {
    let mut p = (1.0, 0.0);
    for j in 1..=10 {
        p = mul(p, z);
        p = (p.0 - c[j], p.1);
    }
    p
}

/// The 10 poles of the filter by Durand–Kerner iteration.
fn poles(c: &[f64; 11]) -> [C; 10] {
    let mut z: [C; 10] = core::array::from_fn(|k| {
        let (mut r, mut x) = ((1.0, 0.0), (0.9, 0.4));
        for _ in 0..k {
            r = mul(r, x);
        }
        x = r;
        x
    });
    for _ in 0..500 {
        let mut worst = 0.0f64;
        for i in 0..10 {
            let mut den = (1.0, 0.0);
            for j in 0..10 {
                if j != i {
                    den = mul(den, (z[i].0 - z[j].0, z[i].1 - z[j].1));
                }
            }
            let d = div(poly(c, z[i]), den);
            z[i] = (z[i].0 - d.0, z[i].1 - d.1);
            worst = worst.max(d.0.abs() + d.1.abs());
        }
        if worst < 1e-14 {
            break;
        }
    }
    z
}

/// The coefficients for `rate`: the table's, or its poles mapped to `rate`.
fn coefficients(vowel: usize, rate: f64) -> [f64; 11] {
    let c = VOWELS[vowel];
    if rate == REF_RATE {
        return c;
    }
    let rho = REF_RATE / rate;
    let mapped = poles(&c).map(|z| {
        let (r, th) = (crate::fm::sqrt(z.0 * z.0 + z.1 * z.1), libm::atan2(z.1, z.0));
        let r = crate::fm::pow(r, rho);
        (r * crate::fm::cos(th * rho), r * crate::fm::sin(th * rho))
    });
    // ∏ (z − p): coefficients of z^10 + a1·z^9 + … (real for conjugate pairs)
    let mut a = [(0.0, 0.0); 11];
    a[0] = (1.0, 0.0);
    for (k, p) in mapped.iter().enumerate() {
        for j in (1..=k + 1).rev() {
            let t = mul(a[j - 1], *p);
            a[j] = (a[j].0 - t.0, a[j].1 - t.1);
        }
    }
    let mut out = c;
    for j in 1..=10 {
        out[j] = -a[j].0;
    }
    // keep the DC gain: gain / (1 − Σ c)
    let dc = |c: &[f64; 11]| 1.0 - c[1..].iter().sum::<f64>();
    out[0] = c[0] * dc(&out) / dc(&c);
    out
}

pub struct Formant {
    vowel: f32,
    wet: f32,
    mem: [[f64; 10]; 2],
    /// Coefficients for `(vowel index, rate)`.
    cache: Option<(usize, f64, [f64; 11])>,
}

impl<'a> Effect<'a> for Formant {
    const PARAMS: usize = 2;
    fn need(_: &Env, _: &Params) -> usize {
        0
    }
    fn new(_: &Env, _: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(Formant { vowel: 0.0, wet: 0.0, mem: [[0.0; 10]; 2], cache: None })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.vowel = v,
            1 => self.wet = v,
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.mem = [[0.0; 10]; 2];
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        let vowel = ((self.vowel * 4.0) as usize).min(4);
        let c = match self.cache {
            Some((v, r, c)) if v == vowel && r == ctx.rate => c,
            _ => {
                let c = coefficients(vowel, ctx.rate);
                self.cache = Some((vowel, ctx.rate, c));
                c
            }
        };
        let wet = self.wet;
        let dry = 1.0 - wet;
        for ch in 0..2 {
            let m = &mut self.mem[ch];
            for v in io.ch[ch][..n].iter_mut() {
                let mut y = c[0] * *v as f64;
                for j in 0..10 {
                    y += c[j + 1] * m[j];
                }
                m.copy_within(0..9, 1);
                m[0] = y;
                *v = *v * dry + y as f32 * wet;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// |H| of a coefficient set at `hz` for `rate`.
    fn mag(c: &[f64; 11], hz: f64, rate: f64) -> f64 {
        let w = 2.0 * core::f64::consts::PI * hz / rate;
        let (mut re, mut im) = (1.0, 0.0);
        for j in 1..=10 {
            re -= c[j] * libm::cos(w * j as f64);
            im += c[j] * libm::sin(w * j as f64);
        }
        c[0] / libm::sqrt(re * re + im * im)
    }

    #[test]
    fn the_pole_solver_reproduces_the_table() {
        for v in 0..5 {
            let c = VOWELS[v];
            let back = coefficients(v, 44100.0 + 1e-6); // a mapping with ρ = 1 − 2·10⁻¹¹
            for j in 1..=10 {
                assert!((back[j] - c[j]).abs() < 1e-4 * c[j].abs().max(1.0), "vowel {v} coefficient {j}: {} vs {}", back[j], c[j]);
            }
            assert!((back[0] / c[0] - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn the_formants_keep_their_frequencies_in_hz_at_any_rate() {
        // F1 and F2 of the soprano vowels at 44.1 kHz (A E I O U), read from the response
        let want: [(f64, f64); 5] = [(800.0, 1150.0), (350.0, 2000.0), (260.0, 2140.0), (450.0, 800.0), (320.0, 700.0)];
        for rate in [44100.0, 48000.0, 96000.0, 22050.0] {
            for (v, &(f1, f2)) in want.iter().enumerate() {
                let c = coefficients(v, rate);
                let peaks: std::vec::Vec<f64> = (10..400)
                    .map(|k| k as f64 * 10.0)
                    .filter(|&f| f < rate / 2.0 * 0.9)
                    .filter(|&f| mag(&c, f, rate) > mag(&c, f - 10.0, rate) && mag(&c, f, rate) > mag(&c, f + 10.0, rate))
                    .collect();
                assert!(peaks.len() >= 2, "vowel {v} at {rate}: {peaks:?}");
                for (got, want) in [(peaks[0], f1), (peaks[1], f2)] {
                    assert!((got - want).abs() <= 0.03 * want + 10.0, "vowel {v} at {rate}: {got} vs {want} ({peaks:?})");
                }
                // the level of the first formant (not the unit-gain table's) stays within 6 dB
                let r44 = mag(&coefficients(v, 44100.0), f1, 44100.0);
                assert!((mag(&c, f1, rate) / r44).log10().abs() < 0.3, "vowel {v} at {rate}");
            }
        }
    }
}
