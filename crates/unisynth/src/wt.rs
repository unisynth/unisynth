//! The spectral wavetable oscillator's tables (a port of the Tunefish generator, rate-general): a parametric partial
//! spectrum → "phase" stage → inverse FFT → peak-normalised, DC-free single cycle of 512 samples, played back by up
//! to 10 unison layers. One [`Wt`] lives in the pool per voice and wavetable slot.

use crate::tables::{exp_profile, rho, sin_at};

/// Table length = spectrum bins (complex).
pub const BINS: usize = 512;
const TAB: usize = 4096;
/// Unison layers at most.
pub const MAX_LAYERS: usize = 10;
/// Most partials a spectrum can have (`1 + 64`).
pub const MAX_PARTIALS: usize = 65;
/// Rebuild tolerance for the float spectrum inputs.
const SAME: f32 = 1e-5;

/// Spectrum inputs after mod-matrix scaling and clamping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    /// Partial count H (1..65).
    pub harmonics: usize,
    /// β ∈ [0, 1]: partial width `0.3 + β·h` bins.
    pub bandwidth: f32,
    /// δ ∈ [0, 1]: partial gain `h^−(1+δ)`.
    pub damp: f32,
    /// s ∈ [0, 4]: partial h centred at bin `1 + s(2h − 1)`.
    pub scale: f32,
    /// Active bins K (band limit); bins ≥ K are zero.
    pub bins: usize,
}

/// Band-limit heuristic: `K = max(4, trunc(512·(1 − r)³))`, `r = clamp((f − 8)/2000, 0, 1)` for the keyboard
/// frequency `f` as it would be at 44.1 kHz (so the band follows the Nyquist frequency at other rates). The octave and
/// fixed-frequency modes are not considered and can therefore alias.
pub fn band_bins(freq: f32, rate: f32) -> usize {
    let f = freq * (44100.0 / rate);
    let r = ((f - 8.0) / 2000.0).max(0.0).min(1.0);
    let inv = crate::fm::pow((1.0 - r) as f64, 3.0) as f32;
    ((BINS as f32 * inv) as usize).max(4)
}

/// Partial count from the normalised parameter: `1 + trunc(64 p)`.
pub fn harmonics(p: f32) -> usize {
    1 + ((p.max(0.0) * 64.0) as usize).min(64)
}

pub struct Table {
    /// Magnitude spectrum, interleaved complex (re = im = A_k before the phase stage).
    spec: [f32; 2 * BINS],
    /// Phase-staged spectrum, then its inverse FFT.
    work: [f32; 2 * BINS],
    /// The playable single cycle (real part, normalised).
    wave: [f32; BINS],
    built: Option<Shape>,
}

impl Table {
    pub const fn new() -> Self {
        Table { spec: [0.0; 2 * BINS], work: [0.0; 2 * BINS], wave: [0.0; BINS], built: None }
    }

    /// Rebuilds the spectrum when `s` differs from the last build (counts exactly, floats beyond 1e-5).
    /// Returns whether it rebuilt.
    pub fn update(&mut self, s: &Shape) -> bool {
        if let Some(b) = &self.built {
            if b.harmonics == s.harmonics
                && b.bins == s.bins
                && libm::fabsf(b.damp - s.damp) < SAME
                && libm::fabsf(b.scale - s.scale) < SAME
                && libm::fabsf(b.bandwidth - s.bandwidth) < SAME
            {
                return false;
            }
        }
        let exp = exp_profile();
        self.spec = [0.0; 2 * BINS];
        let k_max = s.bins.min(BINS);
        // per-partial centre, width and gain, computed once per rebuild
        let h_n = s.harmonics.min(MAX_PARTIALS);
        let mut part = [(0.0f32, 1.0f32, 0.0f32); MAX_PARTIALS];
        for (i, pt) in part.iter_mut().take(h_n).enumerate() {
            let h = i + 1;
            *pt = (
                (2 * h - 1) as f32 * s.scale + 1.0,
                0.3 + s.bandwidth * h as f32,
                (1.0 / crate::fm::pow(h as f64, 1.0 + s.damp as f64)) as f32,
            );
        }
        let scale = (exp.len() - 1) as f32;
        for k in 0..k_max {
            let mut amp = 0.0f32;
            for &(centre, width, gain) in &part[..h_n] {
                let d = libm::fabsf(centre - k as f32) / width;
                if d < 5.0 {
                    amp += exp[(d / 5.0 * scale) as usize] * gain;
                }
            }
            self.spec[2 * k] = amp;
            self.spec[2 * k + 1] = amp;
        }
        self.spec[0] = 1.0;
        self.spec[1] = 0.0;
        self.built = Some(*s);
        true
    }

    pub fn spectrum(&self) -> &[f32; 2 * BINS] {
        &self.spec
    }

    /// The phase stage into the work buffer, with the original table-range quirk: the 4096-period sine is indexed
    /// modulo 512, so both multipliers lie in `[0, sin(π/4)]`. `g` = the mod value; `mu` is the slot's drift value,
    /// advanced by g³/100 (wrapping at 100). With `mu` ≈ 0 the spectrum passes unchanged.
    fn phase_stage(&mut self, g: f32, mu: &mut f32) {
        if libm::fabsf(*mu) < SAME {
            self.work = self.spec;
            return;
        }
        let rho = rho();
        let randomise = libm::fabsf(g) >= SAME;
        for i in 0..BINS {
            let strength = i as f32 / BINS as f32;
            let offset = if randomise { (*mu * BINS as f32) * (1.0 - g * rho[i]) } else { 0.0 };
            let s = ((offset * strength) as i32 as u32 % BINS as u32) as usize;
            let c = (s + BINS / 4) % BINS;
            self.work[2 * i] = self.spec[2 * i] * sin_at(s);
            self.work[2 * i + 1] = self.spec[2 * i + 1] * sin_at(c);
        }
        self.work[0] = 1.0;
        self.work[1] = 0.0;
        *mu += crate::fm::pow(g as f64, 3.0) as f32 / 100.0;
        if *mu >= 100.0 {
            *mu -= 100.0;
        }
    }

    /// Phase stage → inverse FFT → wavetable: peak-normalised (floor 1e-5), then the mean removed.
    pub fn render(&mut self, g: f32, mu: &mut f32) {
        self.phase_stage(g, mu);
        ifft512(&mut self.work);
        let peak = (0..BINS).fold(0.0f32, |m, i| m.max(libm::fabsf(self.work[2 * i]))).max(1e-5);
        let inv = 1.0 / peak;
        let mut sum = 0.0f32;
        for i in 0..BINS {
            self.wave[i] = self.work[2 * i] * inv;
            sum += self.wave[i];
        }
        let mean = sum / BINS as f32;
        self.wave.iter_mut().for_each(|v| *v -= mean);
    }

    pub fn wave(&self) -> &[f32; BINS] {
        &self.wave
    }

    /// Test hook: installs a playable cycle.
    #[cfg(test)]
    pub(crate) fn load(&mut self, w: &[f32; BINS]) {
        self.wave = *w;
    }

    /// Forgets the build, so the next `update` rebuilds (voice reset).
    pub fn clear(&mut self) {
        *self = Table::new();
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

/// Everything a voice keeps for one wavetable slot: the table, the unison layers' phases (left, right) and the phase
/// stage's drift value.
pub struct Wt {
    pub table: Table,
    pub phase: [[f32; 2]; MAX_LAYERS],
    pub mu: f32,
}

impl Wt {
    pub const fn new() -> Self {
        Wt { table: Table::new(), phase: [[0.0; 2]; MAX_LAYERS], mu: 0.0 }
    }
}

impl Default for Wt {
    fn default() -> Self {
        Self::new()
    }
}

/// In-place 512-point radix-2 complex FFT with positive exponent (inverse direction), no 1/N scaling.
/// Data is interleaved `[re0, im0, re1, im1, …]`; twiddles come from the shared 4096-point sine table.
pub fn ifft512(x: &mut [f32; 2 * BINS]) {
    for i in 0..BINS {
        let j = (i as u32).reverse_bits() as usize >> (32 - 9);
        if i < j {
            x.swap(2 * i, 2 * j);
            x.swap(2 * i + 1, 2 * j + 1);
        }
    }
    let mut m = 2;
    while m <= BINS {
        let half = m / 2;
        let stride = TAB / m; // twiddle e^{+i·2π·k/m} = table index k·4096/m
        for k in 0..half {
            let idx = k * stride;
            let (wr, wi) = (sin_at(idx + TAB / 4), sin_at(idx));
            let mut a = k;
            while a < BINS {
                let b = a + half;
                let (br, bi) = (x[2 * b], x[2 * b + 1]);
                let tr = br * wr - bi * wi;
                let ti = br * wi + bi * wr;
                let (ar, ai) = (x[2 * a], x[2 * a + 1]);
                x[2 * b] = ar - tr;
                x[2 * b + 1] = ai - ti;
                x[2 * a] = ar + tr;
                x[2 * a + 1] = ai + ti;
                a += m;
            }
        }
        m *= 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_is_peak_normalised_and_dc_free() {
        crate::tables::init();
        let mut t = Table::new();
        t.update(&Shape { harmonics: 20, bandwidth: 0.1, damp: 0.3, scale: 0.5, bins: 300 });
        let mut mu = 0.0;
        t.render(0.0, &mut mu);
        let w = t.wave();
        let mean = w.iter().sum::<f32>() / BINS as f32;
        assert!(mean.abs() < 1e-4, "{mean}");
        let peak = w.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        // peak-normalised before the mean is removed, so the peak moves by at most the mean's size
        assert!(peak > 0.5 && peak < 1.5, "{peak}");
    }

    #[test]
    fn the_spectrum_is_rebuilt_only_when_its_inputs_change() {
        crate::tables::init();
        let mut t = Table::new();
        let s = Shape { harmonics: 8, bandwidth: 0.2, damp: 0.1, scale: 1.0, bins: 200 };
        assert!(t.update(&s));
        assert!(!t.update(&s));
        assert!(!t.update(&Shape { damp: s.damp + 5e-6, ..s }), "below the 1e-5 tolerance");
        for changed in [
            Shape { harmonics: 9, ..s },
            Shape { bandwidth: 0.21, ..s },
            Shape { damp: 0.11, ..s },
            Shape { scale: 1.1, ..s },
            Shape { bins: 199, ..s },
        ] {
            let mut t = Table::new();
            t.update(&s);
            assert!(t.update(&changed), "{changed:?}");
        }
    }

    #[test]
    fn the_band_limit_follows_the_nyquist_frequency() {
        assert_eq!(band_bins(8.0, 44100.0), 512);
        assert_eq!(band_bins(2008.0, 44100.0), 4);
        assert_eq!(band_bins(440.0, 44100.0), 246);
        assert_eq!(band_bins(880.0, 88200.0), band_bins(440.0, 44100.0));
        assert!(band_bins(440.0, 96000.0) > band_bins(440.0, 44100.0));
        assert_eq!(harmonics(0.0), 1);
        assert_eq!(harmonics(1.0), 65);
        assert_eq!(harmonics(0.5), 33);
    }
}
