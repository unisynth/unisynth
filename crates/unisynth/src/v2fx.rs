//! V2's effect bodies (change add-v2-import): the compressor, the bass boost, the modulated delay (chorus/flanger and
//! the global stereo delay) and the reverb. Shared by the instrument's channel stage (`chan.rs`) and the `v2` effect
//! modes. Port of V2 by Tammo "kb" Hinrichs (public domain), following v2redux's `v2core.cpp` (CC0) where it corrects
//! ryg's C++ port to the assembly (compressor look-ahead ring length and RMS order, channel gain order).
//! Settings take V2 raw values (`0..128`).

use crate::v2::{calcfreq, ftou32, linfreq, utof23};

// ---- compressor (V2Comp) ----

/// Look-ahead buffer length in frames (stereo).
pub const COMP_DLEN: usize = 5700;
/// RMS window in frames (stereo), a power of two.
pub const COMP_RMSLEN: usize = 8192;
const RMS_SCALE: f32 = 0.011_048_543; // 1/√8192 (the assembly's 24-bit constant)
const PEAK_FALLOFF: f32 = 0.9998;

/// Compressor settings (`V2Comp::set`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CompP {
    /// Off, or on with peak/RMS and mono/stereo detection.
    pub on: bool,
    pub rms: bool,
    pub stereo: bool,
    pub invol: f32,
    pub ratio: f32,
    pub outvol: f32,
    pub attack: f32,
    pub release: f32,
    pub dblen: u32,
}

impl CompP {
    /// `mode` (0 off, 1 peak, 2 RMS), `stereo`, `autogain`, `lookahead` (ms), `threshold`, `ratio`, `attack`,
    /// `release`, `outgain`: raw values.
    #[allow(clippy::too_many_arguments)]
    pub fn new(mode: f32, stereo: f32, autogain: f32, lookahead: f32, threshold: f32, ratio: f32, attack: f32, release: f32, outgain: f32, rate: f32) -> Self {
        let m = mode as i32;
        let mut thresh = 8.0 * calcfreq(threshold / 128.0);
        let invol = 1.0 / thresh;
        if autogain != 0.0 {
            thresh = 1.0;
        }
        CompP {
            on: m == 1 || m == 2,
            rms: m == 2,
            stereo: stereo != 0.0,
            invol,
            ratio: ratio / 128.0,
            outvol: thresh * crate::fm::exp2f((outgain - 64.0) / 16.0),
            attack: crate::fm::exp2f(-attack * 12.0 / 128.0),
            release: crate::fm::exp2f(-release * 16.0 / 128.0),
            dblen: ((lookahead * (rate / 1000.0)) as u32).min(COMP_DLEN as u32 - 1),
        }
    }

    fn mode_bits(&self) -> u8 {
        if !self.on {
            return 4;
        }
        self.rms as u8 | (self.stereo as u8) << 1
    }
}

/// Compressor state with its pool buffers.
pub struct Comp<'a> {
    dbuf: &'a mut [[f32; 2]],
    rmsbuf: &'a mut [[f32; 2]],
    mode: u8,
    dbcnt: u32,
    curgain: [f32; 2],
    peak: [f32; 2],
    rmsval: [f32; 2],
    rmscnt: usize,
}

impl<'a> Comp<'a> {
    /// Pool bytes [`Comp::new`] takes.
    pub const fn need() -> usize {
        synth_core::pool::Pool::need::<[f32; 2]>(COMP_DLEN) + synth_core::pool::Pool::need::<[f32; 2]>(COMP_RMSLEN)
    }

    pub fn new(pool: &mut synth_core::pool::Pool<'a>) -> Result<Self, synth_core::pool::OutOfMemory> {
        Ok(Comp {
            dbuf: pool.slice_with(COMP_DLEN, |_| [0.0; 2])?,
            rmsbuf: pool.slice_with(COMP_RMSLEN, |_| [0.0; 2])?,
            mode: 2, // V2's init: stereo peak (`mode = MODE_BIT_STEREO`), so the first set with another mode resets
            dbcnt: 0,
            curgain: [1.0; 2],
            peak: [0.0; 2],
            rmsval: [0.0; 2],
            rmscnt: 0,
        })
    }

    pub fn clear(&mut self) {
        self.dbuf.iter_mut().for_each(|x| *x = [0.0; 2]);
        self.rmsbuf.iter_mut().for_each(|x| *x = [0.0; 2]);
        self.mode = 2;
        self.dbcnt = 0;
        self.reset();
    }

    fn reset(&mut self) {
        self.peak = [0.0; 2];
        self.rmsval = [0.0; 2];
        self.curgain = [1.0; 2];
        self.rmsbuf.iter_mut().for_each(|x| *x = [0.0; 2]);
        self.rmscnt = 0;
    }

    /// Applies new settings (a mode change resets the detector, as `V2Comp::set` does).
    pub fn set(&mut self, p: &CompP) {
        let m = p.mode_bits();
        if m != self.mode {
            self.mode = m;
            self.reset();
        }
    }

    /// Compresses `l`/`r` in place.
    pub fn render(&mut self, p: &CompP, l: &mut [f32], r: &mut [f32]) {
        if !p.on {
            return;
        }
        let n = l.len();
        // level detection, frame by frame, before the gain stage (V2 fills a level buffer first)
        let mut lv = [[0.0f32; 2]; 128];
        let mut pos = 0;
        while pos < n {
            let len = (n - pos).min(128);
            for i in 0..len {
                let (a, b) = (l[pos + i], r[pos + i]);
                lv[i] = match (p.rms, p.stereo) {
                    (false, false) => {
                        let x = self.peak_of(0.5 * (a + b), 0);
                        [p.invol * x; 2]
                    }
                    (true, false) => {
                        let x = self.rms_of(0.5 * (a + b), 0);
                        self.rmscnt = (self.rmscnt + 1) & (COMP_RMSLEN - 1);
                        [p.invol * x; 2]
                    }
                    (false, true) => [p.invol * self.peak_of(a, 0), p.invol * self.peak_of(b, 1)],
                    (true, true) => {
                        let x = [p.invol * self.rms_of(a, 0), p.invol * self.rms_of(b, 1)];
                        self.rmscnt = (self.rmscnt + 1) & (COMP_RMSLEN - 1);
                        x
                    }
                };
            }
            for ch in 0..2 {
                let buf = if ch == 0 { &mut l[pos..pos + len] } else { &mut r[pos..pos + len] };
                let mut gain = self.curgain[ch];
                let mut dbind = self.dbcnt;
                for (i, x) in buf.iter_mut().enumerate() {
                    let v = p.outvol * self.dbuf[dbind as usize][ch];
                    self.dbuf[dbind as usize][ch] = p.invol * *x;
                    dbind += 1;
                    if dbind > p.dblen {
                        dbind = 0;
                    }
                    let lvl = lv[i][ch];
                    let dgain = if lvl >= 1.0 { 1.0 / (1.0 + p.ratio * (lvl - 1.0)) } else { 1.0 };
                    gain += (if dgain < gain { p.attack } else { p.release }) * (dgain - gain);
                    *x = v * gain;
                }
                self.curgain[ch] = gain;
                if ch == 1 {
                    self.dbcnt = dbind;
                }
            }
            pos += len;
        }
    }

    #[inline]
    fn peak_of(&mut self, x: f32, ch: usize) -> f32 {
        self.peak[ch] = (self.peak[ch] * PEAK_FALLOFF).max(libm::fabsf(x));
        self.peak[ch]
    }

    #[inline]
    fn rms_of(&mut self, x: f32, ch: usize) -> f32 {
        self.rmsval[ch] -= self.rmsbuf[self.rmscnt][ch];
        let sq = x * x;
        self.rmsval[ch] += sq;
        self.rmsbuf[self.rmscnt][ch] = sq;
        libm::sqrtf(self.rmsval[ch].max(0.0)) * RMS_SCALE
    }
}

// ---- bass boost (V2Boost) ----

/// The low shelf at 150 Hz: coefficients, or off.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoostP {
    pub on: bool,
    a1: f32,
    a2: f32,
    b0: f32,
    b1: f32,
    b2: f32,
}

impl BoostP {
    /// `amount`: raw (0 = off).
    pub fn new(amount: f32, rate: f32) -> Self {
        if amount as i32 == 0 {
            return BoostP::default();
        }
        let w = 150.0 * core::f32::consts::TAU / rate;
        let (s, c) = (crate::fm::sinf(w), crate::fm::cosf(w));
        let a = crate::fm::exp2f(amount / 128.0);
        let beta = libm::sqrtf(2.0 * a);
        let bs = beta * s;
        let (am1, ap1) = (a - 1.0, a + 1.0);
        let (cam1, cap1) = (am1 * c, ap1 * c);
        let ia0 = 1.0 / (ap1 + cam1 + bs);
        BoostP {
            on: true,
            b1: 2.0 * a * (am1 - cap1) * ia0,
            a1: -2.0 * (am1 + cap1) * ia0,
            a2: (ap1 + cam1 - bs) * ia0,
            b0: a * (ap1 - cam1 + bs) * ia0,
            b2: a * (ap1 - cam1 - bs) * ia0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Boost {
    x1: [f32; 2],
    x2: [f32; 2],
    y1: [f32; 2],
    y2: [f32; 2],
}

impl Boost {
    pub fn render(&mut self, p: &BoostP, l: &mut [f32], r: &mut [f32]) {
        if !p.on {
            return;
        }
        for (ch, buf) in [l, r].into_iter().enumerate() {
            let (mut xm1, mut xm2, mut ym1, mut ym2) = (self.x1[ch], self.x2[ch], self.y1[ch], self.y2[ch]);
            for x in buf.iter_mut() {
                let xi = *x;
                let y = p.b0 * xi + p.b1 * xm1 + p.b2 * xm2 - p.a1 * ym1 - p.a2 * ym2;
                (ym2, ym1, xm2, xm1) = (ym1, y, xm1, xi);
                *x = y;
            }
            (self.x1[ch], self.x2[ch], self.y1[ch], self.y2[ch]) = (xm1, xm2, ym1, ym2);
        }
    }
}

// ---- modulated delay (V2ModDel) ----

/// Modulated delay settings (`V2ModDel::set`) for a buffer of `len` (a power of two).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ModDelP {
    pub wet: f32,
    pub dry: f32,
    pub fb: f32,
    pub offs: [u32; 2],
    pub mfreq: u32,
    pub mmax: u32,
    pub mphase: u32,
}

impl ModDelP {
    /// `amount`, `feedback`, `left`, `right`, `rate`, `depth`, `phase`: raw values.
    #[allow(clippy::too_many_arguments)]
    pub fn new(amount: f32, feedback: f32, left: f32, right: f32, mrate: f32, depth: f32, phase: f32, len: usize, rate: f32) -> Self {
        let wet = (amount - 64.0) / 64.0;
        let lenscale = ((len - 1) as f32 - 1023.0) / 128.0;
        ModDelP {
            wet,
            dry: 1.0 - libm::fabsf(wet),
            fb: (feedback - 64.0) / 64.0,
            offs: [(left * lenscale) as i32 as u32, (right * lenscale) as i32 as u32],
            mfreq: (linfreq(rate) * 1_973_915.5 * calcfreq(mrate / 128.0)) as i64 as u32,
            mmax: (depth * 1023.0 / 128.0) as i32 as u32,
            mphase: ftou32((phase - 64.0) / 128.0),
        }
    }
}

/// A modulated stereo delay over two pool buffers.
pub struct ModDel<'a> {
    buf: [&'a mut [f32]; 2],
    mask: u32,
    pos: u32,
    mcnt: u32,
}

impl<'a> ModDel<'a> {
    pub const fn need(len: usize) -> usize {
        2 * synth_core::pool::Pool::need::<f32>(len)
    }

    /// `len`: a power of two.
    pub fn new(pool: &mut synth_core::pool::Pool<'a>, len: usize) -> Result<Self, synth_core::pool::OutOfMemory> {
        Ok(ModDel { buf: [pool.slice(len)?, pool.slice(len)?], mask: len as u32 - 1, pos: 0, mcnt: 0 })
    }

    pub fn len(&self) -> usize {
        self.mask as usize + 1
    }

    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|b| b.fill(0.0));
        self.pos = 0;
        self.mcnt = 0;
    }

    #[inline]
    fn chan(&mut self, p: &ModDelP, x: f32, ch: usize, dry: f32) -> f32 {
        let mut counter = self.mcnt.wrapping_add(if ch == 1 { p.mphase } else { 0 });
        counter = if counter < 0x8000_0000 { counter.wrapping_mul(2) } else { 0xffff_ffff - counter.wrapping_mul(2) };
        let offs = counter as u64 * p.mmax as u64;
        let oi = ((offs >> 32) as u32).wrapping_add(p.offs[ch]);
        // the assembly reads idx and idx + 1 and weights by 1 − frac; the right channel's idx is one earlier
        let idx = self.pos.wrapping_sub(oi).wrapping_sub(ch as u32);
        let t = utof23(offs as u32);
        let b = &mut self.buf[ch];
        let (in1, in2) = (b[(idx & self.mask) as usize], b[(idx.wrapping_add(1) & self.mask) as usize]);
        let delayed = in1 + (1.0 - t) * (in2 - in1);
        b[self.pos as usize] = x + delayed * p.fb;
        x * dry + delayed * p.wet
    }

    #[inline]
    fn sample(&mut self, p: &ModDelP, l: f32, r: f32, dry: f32) -> (f32, f32) {
        let o = (self.chan(p, l, 0, dry), self.chan(p, r, 1, dry));
        self.mcnt = self.mcnt.wrapping_add(p.mfreq);
        self.pos = (self.pos + 1) & self.mask;
        o
    }

    /// The channel chorus/flanger, in place (nothing when the effect is dry).
    pub fn render_chan(&mut self, p: &ModDelP, l: &mut [f32], r: &mut [f32]) {
        if p.wet == 0.0 {
            return;
        }
        for (a, b) in l.iter_mut().zip(r.iter_mut()) {
            (*a, *b) = self.sample(p, *a, *b, p.dry);
        }
    }

    /// The global delay: mono input `x`, wet output added to `l`/`r` (nothing when the effect is dry).
    pub fn render_add(&mut self, p: &ModDelP, x: &[f32], l: &mut [f32], r: &mut [f32]) {
        if p.wet == 0.0 {
            return;
        }
        for ((xi, a), b) in x.iter().zip(l.iter_mut()).zip(r.iter_mut()) {
            let v = *xi;
            let (wl, wr) = self.sample(p, v, v, 0.0);
            *a += wl;
            *b += wr;
        }
    }
}
