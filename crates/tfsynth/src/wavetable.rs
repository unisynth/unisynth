//! The spectral wavetable generator (spec §3.1): a parametric partial spectrum → "phase" stage → inverse FFT →
//! peak-normalised, DC-free single-cycle table of 512 samples.

use crate::ifft::ifft512;
use crate::tables::{tables, BINS};

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

/// Band-limit heuristic: `K = max(4, trunc(512·(1 − r)³))`, `r = clamp((f − 8)/2000, 0, 1)` for the glided
/// keyboard frequency f (before octave and fixed-frequency mode, which can therefore alias).
pub fn band_bins(freq: f32) -> usize {
    let r = ((freq - 8.0) / 2000.0).clamp(0.0, 1.0);
    let inv = libm::pow((1.0 - r) as f64, 3.0) as f32;
    ((BINS as f32 * inv) as usize).max(4)
}

/// Most partials a spectrum can have (`1 + 64`).
pub const MAX_PARTIALS: usize = 65;

/// Rebuild tolerance for the float spectrum inputs.
const SAME: f32 = 1e-5;

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
        if let Some(b) = &self.built
            && b.harmonics == s.harmonics
            && b.bins == s.bins
            && libm::fabsf(b.damp - s.damp) < SAME
            && libm::fabsf(b.scale - s.scale) < SAME
            && libm::fabsf(b.bandwidth - s.bandwidth) < SAME
        {
            return false;
        }
        let exp = &tables().exp;
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
                (1.0 / libm::pow(h as f64, 1.0 + s.damp as f64)) as f32,
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

    /// The phase stage (spec §3.1.3) into the work buffer, with the reference's table-range quirk: the 4096-period
    /// sine is indexed modulo 512, so both multipliers lie in [0, sin(π/4)]. `g` = GenMod; `mu` is the voice's
    /// drift value, advanced by g³/100 (wrapping at 100). With `mu` ≈ 0 the spectrum passes unchanged.
    pub fn phase_stage(&mut self, g: f32, mu: &mut f32) -> &[f32; 2 * BINS] {
        if libm::fabsf(*mu) < SAME {
            self.work = self.spec;
            return &self.work;
        }
        let t = tables();
        let randomise = libm::fabsf(g) >= SAME;
        for i in 0..BINS {
            let strength = i as f32 / BINS as f32;
            let offset = if randomise { (*mu * BINS as f32) * (1.0 - g * t.rho[i]) } else { 0.0 };
            let s = ((offset * strength) as i32 as u32 % BINS as u32) as usize;
            let c = (s + BINS / 4) % BINS;
            self.work[2 * i] = self.spec[2 * i] * t.sin[s];
            self.work[2 * i + 1] = self.spec[2 * i + 1] * t.sin[c];
        }
        self.work[0] = 1.0;
        self.work[1] = 0.0;
        *mu += libm::pow(g as f64, 3.0) as f32 / 100.0;
        if *mu >= 100.0 {
            *mu -= 100.0;
        }
        &self.work
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
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}
