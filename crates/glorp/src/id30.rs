//! Glorp effect ID 30: a granular delay with a comb / all-pass reverb, two filters, a frequency shifter in the
//! feedback path and an optional all-pass cascade (24 parameters). Independent behavioural implementation written from
//! observed behaviour (research/glorp-devices.md §ID 30); no original binary, table or pseudocode is included. f32,
//! scalar operation order of the reference (no FMA) because the order matters for bit parity. Parameters are read once
//! per [`Id30::process`] call (the reference's block granularity); everything else runs per sample. Fixed 44.1 kHz.
//!
//! | param | meaning |
//! |---|---|
//! | 0 | delay time `P0` s (glides `+= (target − t)·0.021` per write from 0), also scales the grain length / scatter |
//! | 1 | feedback gain of the filtered wet signal |
//! | 2 | dry gain, 3 wet gain (`out = wet·P3 + in·P2`) |
//! | 4 | frequency shift of the feedback path (`P4 = 0.5` none; `(2P4−1)⁸` cycles/sample, up below 0.5) |
//! | 5 / 6 | low-pass `P5²·20000+20` Hz / high-pass `P6²·20000+20` Hz on the wet signal (Q 0.7) |
//! | 7 | grain pitch `2^(2·P7−1)` |
//! | 8 | grain streams `trunc(5·P8)`, 0..=5; **0 = plain delay tap** |
//! | 9 | grain start scatter `P9·P0·22050` samples |
//! | 10 | grain length `P0·P10·176400` samples |
//! | 11 | hold: `P11 > 0.5` stops writing the delay line (the line, its smoothed time and its read position freeze) |
//! | 12 / 13 | reverb feedback `min(0.999, P12)^0.2` / damping `min(0.999, P13)` |
//! | 14 | reverb share: `grain·(1−P14) + reverb·P14·2/15` |
//! | 15 | reverb freeze: feedback → `0.999^0.2`, damping → 0 |
//! | 16 | all-pass cascade mix (`P16 ≤ 0.001` = cascade skipped, its state untouched) |
//! | 17 / 18 / 19 | cascade centre `P17²·19980+20` Hz, feedback `1.98·P18 − 0.99`, scatter `3·P19` octaves |
//! | 20 | cascade topology `trunc(2.999·P20)`: **0 = delay-line all-pass only**; 1 / 2 rejected while the cascade is on |
//! | 21 | ping-pong: the feedback is `P21·wet + (1−P21)·swap(wet)` |
//! | 22 | feedback-path mix of the two channels |
//! | 23 | enable: `P23 < 0.5` leaves the block untouched (in place) and freezes all state |
//!
//! Signal flow per sample: grain engine (or the delay tap) reads the main line → reverb → `(1−P14)/P14` mix → low-pass
//! → high-pass = `wet`. The feedback `P1·pingpong(wet)` is added to the input (channel mix P22), the sum goes through
//! the frequency shifter and (P16 > 0.001) the cascade, and is written into the line unless held.
//!
//! **Rejected, never approximated** ([`Error`]; the device is left unchanged): index ≥ 24, non-finite or outside
//! `[0, 1]`, and [`Error::Topology`] — the cascade is switched on (`P16 > 0.001`) with a topology other than the
//! delay-line one (the first-order and second-order variants are not modelled here).
//!
//! Memory: the device borrows ≈ 4 MB (`SLAB_F32`) from the song pool: the 176,400-sample main line, 24 reverb lines and
//! 128 cascade lines. The original gives every reverb line 44,100 samples; only the last 2048 matter (reads are
//! ≤ 1640 samples behind), so those lines keep the original's 44,100-entry index bookkeeping (it decides the
//! f32 rounding of the fractional read position) over a 2048-sample ring. Not modelled: the original's pre-delay
//! (an identity), its four unused diffusers, its unused reverb filters and the reverb's level follower (its input is
//! gated by a parameter nothing sets, so it stays 0).
//!
//! Input is not sanitised: a NaN / inf input sample can poison the delay line / reverb state until [`Id30::reset`]; the
//! original's behaviour on non-finite input is not reference-verified (no such recorded input).
//!
//! Not a `synth_core::Device`: its `set_param` cannot report a rejection.

use crate::id25::{hash, sine_table, Biquad};
use crate::id27::rbj_pass;
use crate::id34::{sin_with, tick, Hist, CHAIN_A, CHAIN_B, TABLE_LEN, TAU};
use synth_core::denormal::Guard;
use synth_core::pool::{OutOfMemory, Pool};

/// Parameter count of the device.
pub const PARAMS: usize = 24;
/// Most grain streams (`trunc(5·1.0)`).
pub const MAX_STREAMS: usize = 5;

const MAIN_SIZE: usize = 176_400;
const REV_SIZE: usize = 44_100;
const REV_CAP: usize = 2_048;
const CASC_SIZE: usize = 2_205;
const STAGES: usize = 128;
const COMBS: usize = 8;
const APS: usize = 4;
/// `f32`s the device borrows from the pool (two per stereo sample).
pub const SLAB_F32: usize = 2 * (MAIN_SIZE + 2 * (COMBS + APS) * REV_CAP + STAGES * CASC_SIZE);

const COMB_TUNE: [f32; COMBS] = [1116.0, 1188.0, 1277.0, 1356.0, 1422.0, 1491.0, 1557.0, 1617.0];
const AP_TUNE: [f32; APS] = [556.0, 441.0, 341.0, 225.0];
const INV_44100: f32 = f32::from_bits(0x37be_37c6);
const SPREAD: f32 = f32::from_bits(0x3a08_b816); // 23 / 44100 = 0.00052154192
const NEG_44100: f32 = -44100.0;
const Q07: f32 = f32::from_bits(0x3f33_3333);
const NEG_2M32: f32 = f32::from_bits(0xaf80_0000);
const HASH_SCALE: f32 = f32::from_bits(0x3000_0000); // 2^-31 as the cascade scatter uses it
const MIN_DELAY: f32 = f32::from_bits(0x383e_37c6); // 2/44100 s
const PI: f32 = f32::from_bits(0x4049_0fdb);
const K0133: f32 = f32::from_bits(0x3e08_8889);
const K0999: f32 = f32::from_bits(0x3f7f_be77);
const K02: f32 = f32::from_bits(0x3e4c_cccd);
const HALF_PI: f32 = 1.570_796_4;

/// Why a setting is not supported. The device is left unchanged when one is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parameter index ≥ 24.
    Index,
    /// Not finite or outside `[0, 1]`.
    Range,
    /// The all-pass cascade is on (`P16 > 0.001`) with a topology other than 0 (delay-line).
    Topology,
    /// The pool is too small ([`SLAB_F32`] floats are needed).
    Memory,
}

impl From<OutOfMemory> for Error {
    fn from(_: OutOfMemory) -> Self {
        Error::Memory
    }
}

fn topology(p20: f32) -> i32 {
    let t = (p20 * 2.999) as i32;
    if t > 2 { 2 } else if t < 0 { 0 } else { t }
}

/// Whether `new` / `set_param` accept this whole parameter vector (cheap: builds nothing).
pub fn check(p: &[f32; PARAMS]) -> Result<(), Error> {
    if p.iter().any(|v| !(0.0..=1.0).contains(v)) {
        return Err(Error::Range);
    }
    if p[16] > 0.001 && topology(p[20]) != 0 {
        return Err(Error::Topology);
    }
    Ok(())
}

/// `cvttss2si`: truncate toward zero, out-of-range and NaN give `i32::MIN`.
fn cvtt(t: f32) -> i32 {
    if t >= -2_147_483_648.0 && t < 2_147_483_648.0 { t as i32 } else { i32::MIN }
}

/// `x` brought into `[0, size)` the way the reference's ring reads do (two conditional additions).
fn wrap_pos(x: f32, size: f32) -> f32 {
    let t = x + size;
    if t >= size {
        x
    } else if t >= 0.0 {
        t
    } else {
        t + size
    }
}

/// One stereo delay line over a slice of the pool slab. `size` is the logical length the position bookkeeping runs
/// over (the original's); `cap` the samples actually stored (a ring, `cap ≤ size`; `cap == size` is the original's
/// buffer).
#[derive(Clone, Copy, Debug)]
struct Line {
    base: usize,
    cap: usize,
    size: i32,
    /// Slot of the newest write (ring) = `pos` when `cap == size`.
    head: usize,
    pos: i32,
    /// Integer read position at the end of the last write (the grain engine's start reference).
    rd: i32,
    time: f32,
    target: f32,
    coef: f32,
}

impl Line {
    fn new(base: usize, cap: usize, size: usize) -> Self {
        Line { base, cap, size: size as i32, head: 0, pos: 0, rd: 0, time: 0.0, target: 0.1, coef: 0.02 }
    }

    /// Stereo sample written at logical index `v` (`0 ≤ v < size`), zero when it has fallen out of a ring.
    fn get(&self, slab: &[f32], v: i32) -> [f32; 2] {
        let slot = if self.cap == self.size as usize {
            v as usize
        } else {
            let age = (self.pos - v).rem_euclid(self.size) as usize;
            if age >= self.cap {
                return [0.0; 2];
            }
            (self.head + self.cap - age) % self.cap
        };
        let i = self.base + 2 * slot;
        [slab[i], slab[i + 1]]
    }

    /// Linear interpolation at the float position `p` (as the reference: `frac·b[i+1] + (1−frac)·b[i]`).
    fn at(&self, slab: &[f32], p: f32) -> [f32; 2] {
        let iv = cvtt(p);
        let size = self.size;
        // a position rounded up to `size` reads index 0 (the original reads one past its buffer there)
        if iv < 0 || iv >= 2 * size {
            return [0.0; 2];
        }
        let i0 = if iv >= size { iv - size } else { iv };
        let i2 = if i0 + 1 >= size { i0 + 1 - size } else { i0 + 1 };
        let frac = p - iv as f32;
        let (b1, b2) = (self.get(slab, i0), self.get(slab, i2));
        let w = 1.0 - frac;
        [frac * b2[0] + w * b1[0], frac * b2[1] + w * b1[1]]
    }

    /// Read `offset` seconds before the smoothed delay.
    fn read(&self, slab: &[f32], offset: f32) -> [f32; 2] {
        let x = (offset + self.time) * NEG_44100 + self.pos as f32;
        self.at(slab, wrap_pos(x, self.size as f32))
    }

    /// Glide the delay time, advance, store, and remember the integer read position.
    fn write(&mut self, slab: &mut [f32], x: [f32; 2]) {
        let c = self.coef + 0.001;
        let c = if c < 1.0 { c } else { 1.0 };
        self.time = (self.target - self.time) * c + self.time;
        self.pos += 1;
        if self.pos >= self.size {
            self.pos = 0;
        }
        self.head = (self.head + 1) % self.cap;
        let i = self.base + 2 * self.head;
        slab[i] = x[0];
        slab[i + 1] = x[1];
        let rp = wrap_pos(self.time * NEG_44100 + self.pos as f32, self.size as f32);
        self.rd = cvtt(rp);
    }

    /// Schroeder all-pass: `w = x − g·d`, `y = d + g·w`, `w` written.
    fn allpass(&mut self, slab: &mut [f32], x: [f32; 2], offset: f32, g: f32) -> [f32; 2] {
        let d = self.read(slab, offset);
        let w = [x[0] - d[0] * g, x[1] - d[1] * g];
        let y = [g * w[0] + d[0], g * w[1] + d[1]];
        self.write(slab, w);
        y
    }
}

/// One grain: `[age, length, start position, age step]`.
type Grain = [f32; 4];

pub struct Id30<'a> {
    slab: &'a mut [f32],
    p: [f32; PARAMS],
    main: Line,
    comb: [[Line; COMBS]; 2],
    ap: [[Line; APS]; 2],
    casc: [Line; STAGES],
    s1: [f32; COMBS],
    s2: [f32; COMBS],
    grains: [[Grain; 2]; 16],
    cur: [u8; 16],
    rng: u32,
    lp: Biquad,
    hp: Biquad,
    phase: f32,
    prev: [f32; 2],
    st: [[[Hist; 2]; 4]; 2],
    table: [f32; TABLE_LEN],
}

impl<'a> Id30<'a> {
    /// Builds the device from the 24 normalized parameters (a song chunk's first 24 floats), borrowing its lines from
    /// `pool` ([`SLAB_F32`] floats, zeroed).
    pub fn new(p: &[f32; PARAMS], pool: &mut Pool<'a>) -> Result<Self, Error> {
        check(p)?;
        let slab = pool.slice::<f32>(SLAB_F32)?;
        let mut d = Id30 {
            slab,
            p: *p,
            main: Line::new(0, MAIN_SIZE, MAIN_SIZE),
            comb: [[Line::new(0, 1, 1); COMBS]; 2],
            ap: [[Line::new(0, 1, 1); APS]; 2],
            casc: [Line::new(0, 1, 1); STAGES],
            s1: [0.0; COMBS],
            s2: [0.0; COMBS],
            grains: [[[0.0; 4]; 2]; 16],
            cur: [0; 16],
            rng: 0,
            lp: Biquad::default(),
            hp: Biquad::default(),
            phase: 0.0,
            prev: [0.0; 2],
            st: [[[[0.0; 4]; 2]; 4]; 2],
            table: sine_table(),
        };
        d.rebuild();
        Ok(d)
    }

    /// The state of a freshly built device (parameters kept): zeroed lines, the original's initial smoothing and
    /// reverb tunings, grain slots idle.
    pub fn reset(&mut self) {
        self.slab.fill(0.0);
        self.rebuild();
    }

    fn rebuild(&mut self) {
        let mut at = 2 * MAIN_SIZE;
        self.main = Line::new(0, MAIN_SIZE, MAIN_SIZE);
        for side in 0..2 {
            let extra = if side == 0 { 0.0 } else { SPREAD };
            for k in 0..COMBS {
                let mut l = Line::new(at, REV_CAP, REV_SIZE);
                l.target = COMB_TUNE[k] * INV_44100 + if side == 0 { 0.0 } else { extra };
                self.comb[side][k] = l;
                at += 2 * REV_CAP;
            }
        }
        for side in 0..2 {
            let extra = if side == 0 { 0.0 } else { SPREAD };
            for k in 0..APS {
                let mut l = Line::new(at, REV_CAP, REV_SIZE);
                l.target = AP_TUNE[k] * INV_44100 + if side == 0 { 0.0 } else { extra };
                self.ap[side][k] = l;
                at += 2 * REV_CAP;
            }
        }
        for k in 0..STAGES {
            self.casc[k] = Line::new(at, CASC_SIZE, CASC_SIZE);
            at += 2 * CASC_SIZE;
        }
        debug_assert_eq!(at, SLAB_F32);
        self.s1 = [0.0; COMBS];
        self.s2 = [0.0; COMBS];
        self.grains = [[[0.0, 5.0, 0.0, 1.0]; 2]; 16];
        self.cur = [0; 16];
        self.rng = 0x1f11_a9a2;
        self.lp = Biquad::default();
        self.hp = Biquad::default();
        self.phase = 0.0;
        self.prev = [0.0; 2];
        self.st = [[[[0.0; 4]; 2]; 4]; 2];
    }

    /// Sets one parameter between `process` calls (all state is kept). On `Err` nothing changed.
    pub fn set_param(&mut self, index: usize, value: f32) -> Result<(), Error> {
        if index >= PARAMS {
            return Err(Error::Index);
        }
        let mut p = self.p;
        p[index] = value;
        check(&p)?;
        self.p = p;
        Ok(())
    }

    /// Cascade stage targets for the current parameters (delay-line topology): stage `i` at
    /// `2^(u_i·3·P19)·(P17²·19980+20)` Hz clamped to 20..20000, delay `max(1/f, 2/44100)` s.
    fn setup_cascade(&mut self) {
        let p = &self.p;
        let base = (p[17] * p[17]) * 19980.0 + 20.0;
        let spread = p[19] * 3.0;
        for i in 0..STAGES {
            let mut h = (i as u32).wrapping_mul(1_234_567);
            h = (h ^ (h >> 16)).wrapping_mul(0x7feb_352d);
            h = (h ^ (h >> 15)).wrapping_mul(0x846c_a68b);
            h ^= h >> 16;
            let u = (h as f32 * HASH_SCALE) + -1.0;
            let f = (libm::pow(2.0, (u * spread) as f64) as f32) * base;
            let m = if f < 20000.0 { f } else { 20000.0 };
            let f = if f < 20.0 { 20.0 } else { m };
            let t = 1.0 / f;
            self.casc[i].target = if t > MIN_DELAY { t } else { MIN_DELAY };
        }
    }

    /// One grain-engine step: the sum of the live grains / streams, or the plain delay tap with no stream.
    fn grains(&mut self, streams: usize, scale: f32, scatter: f32, length: f32) -> [f32; 2] {
        if streams == 0 {
            return self.main.read(self.slab, 0.0);
        }
        let p7 = self.p[7];
        let size = self.main.size;
        let size_f = size as f32;
        let wpos = self.main.pos as f32;
        let dur = (scale * length) * 176_400.0;
        let scat = (scatter * scale) * 22_050.0;
        let base = self.main.rd as f32;
        let inv = 1.0 / size_f;
        let mut acc = [0.0f32; 2];
        for s in 0..streams {
            for g in self.grains[s].iter_mut() {
                g[0] += g[3];
            }
            let c = self.cur[s] as usize;
            let o = 1 - c;
            let pitch = libm::exp2((p7 + p7 + -1.0) as f64) as f32;
            let x = self.grains[s][c][2] + self.grains[s][c][0];
            let r = floor_wrap(x, inv, size_f);
            if r >= wpos && r - wpos < 11_025.0 {
                let v = self.draw(scat, base);
                self.grains[s][c] = [0.0, dur, v, pitch];
                self.cur[s] = o as u8;
            }
            if self.grains[s][c][1] * 0.5 < self.grains[s][c][0] {
                let v = self.draw(scat, base);
                self.grains[s][o] = [0.0, dur, v, pitch];
                self.cur[s] = o as u8;
            }
            for k in 0..2 {
                let g = self.grains[s][k];
                if g[0] > g[1] {
                    continue;
                }
                let w = sin_with(&self.table, (g[0] * PI) / g[1]);
                let x = floor_wrap(g[0] + g[2], inv, size_f);
                let iv = cvtt(x);
                let i0 = if iv >= size { iv - size } else { iv };
                let i2 = if i0 + 1 >= size { i0 + 1 - size } else { i0 + 1 };
                if i0 < 0 || i0 >= size {
                    continue;
                }
                let frac = x - iv as f32;
                let (b1, b2) = (self.main.get(self.slab, i0), self.main.get(self.slab, i2));
                let q = 1.0 - frac;
                acc[0] += w * (frac * b2[0] + q * b1[0]);
                acc[1] += w * (frac * b2[1] + q * b1[1]);
            }
        }
        let n = 1.0 / streams as f32;
        [n * acc[0], n * acc[1]]
    }

    /// A grain start position: `(0.5 − u)·scatter + read position`, `u` ∈ [0, 1) from the shared hash chain.
    fn draw(&mut self, scat: f32, base: f32) -> f32 {
        self.rng = hash(self.rng);
        ((self.rng as f32 * NEG_2M32) + 0.5) * scat + base
    }

    /// Eight parallel low-pass-feedback combs per side, four all-passes per side, the width mix and the gain 6.
    fn reverb(&mut self, x: [f32; 2], fb: f32, damp: f32) -> [f32; 2] {
        let mut acc = [0.0f32; 2];
        for k in 0..COMBS {
            let a = self.comb[0][k].read(self.slab, 0.0);
            let b = self.comb[1][k].read(self.slab, 0.0);
            let s1 = (self.s1[k] - a[0]) * damp + a[0];
            let s2 = (self.s2[k] - b[1]) * damp + b[1];
            self.s1[k] = s1;
            self.s2[k] = s2;
            let f1 = s1 * fb;
            self.comb[0][k].write(self.slab, [f1 + x[0], x[1] + f1]);
            let f2 = fb * s2;
            self.comb[1][k].write(self.slab, [f2 + x[0], x[1] + f2]);
            acc[0] += b[1];
            acc[1] += a[0];
        }
        acc = [acc[0] * 0.125, acc[1] * 0.125];
        let (mut yl, mut yr) = (0.0, 0.0);
        for j in 0..APS {
            let l = self.ap[0][j].allpass(self.slab, [acc[1], acc[1]], 0.0, fb);
            let r = self.ap[1][j].allpass(self.slab, [acc[0], acc[0]], 0.0, fb);
            acc = [r[1], l[0]];
            yl = l[0];
            yr = r[1];
        }
        // width 1.0: wet1 = 0.5·1 + 0.5, wet2 = 0.5 − 0.5·1; dry 0, gain 6 (the level follower stays 0)
        let (w1, w2) = (1.0f32, 0.0f32);
        let wet = [yl * w1 + yr * w2, yl * w2 + yr * w1];
        [0.0 * x[0] + 6.0 * wet[0], 0.0 * x[1] + 6.0 * wet[1]]
    }

    /// Frequency shifter step (the ID 34 structure, phase increment fixed for the call).
    fn shift(&mut self, x: [f32; 2], inc: f32) -> [f32; 2] {
        let mut a = x;
        let mut b = self.prev;
        for ch in 0..2 {
            for k in 0..4 {
                a[ch] = tick(&CHAIN_A[k], &mut self.st[0][k][ch], a[ch]);
                b[ch] = tick(&CHAIN_B[k], &mut self.st[1][k][ch], b[ch]);
            }
        }
        self.prev = x;
        let v = self.phase + inc;
        self.phase = if 1.0 < v { v + -1.0 } else { v };
        let arg = self.phase * TAU;
        let c = sin_with(&self.table, HALF_PI + arg);
        let s = sin_with(&self.table, arg);
        [c * a[0] + s * b[0], c * a[1] + s * b[1]]
    }

    /// Processes a stereo block in place. `l` and `r` must have the same length (panics otherwise). The parameters are
    /// fixed for the whole call; all state continues from the previous call. `P23 < 0.5` leaves the block untouched.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        assert_eq!(l.len(), r.len(), "channel length mismatch");
        let _ftz = Guard::new();
        let p = self.p;
        if 0.5 > p[23] {
            return;
        }
        self.main.target = p[0];
        let streams = (p[8] * 5.0) as usize;
        let f1 = (p[5] * p[5]) * 20000.0 + 20.0;
        let f2 = (p[6] * p[6]) * 20000.0 + 20.0;
        self.lp.c = rbj_pass(false, f1, Q07);
        self.hp.c = rbj_pass(true, f2, Q07);
        // reverb feedback / damping (frozen: feedback of 1 clamped to 0.999, no damping)
        let freeze = 0.5 < p[15];
        let room = if freeze { 1.0 } else { p[12] };
        let room = if room < 0.0 { 0.0 } else if K0999 < room { K0999 } else { room };
        let fb = libm::pow(room as f64, K02 as f64) as f32;
        let damp = if freeze {
            0.0
        } else if p[13] < 0.0 {
            0.0
        } else if K0999 < p[13] {
            K0999
        } else {
            p[13]
        };
        let cascade = 0.001 < p[16];
        let g = 1.98 * p[18] + -0.99;
        if cascade {
            self.setup_cascade();
        }
        let delta = {
            let x = (p[4] + p[4]) + -1.0;
            let x2 = x * x;
            let x4 = x2 * x2;
            let x8 = x4 * x4;
            if p[4] < 0.5 { -x8 } else { x8 }
        };
        let inc = if p[4] == 0.0 { 1.0 } else { delta + 1.0 };
        let hold = p[11] > 0.5;
        let mix = 1.0 - p[14];
        let rev = p[14] * K0133;
        for i in 0..l.len() {
            let (a, b) = (l[i], r[i]);
            let gr = self.grains(streams, p[0], p[9], p[10]);
            let rv = self.reverb(gr, fb, damp);
            let m = [gr[0] * mix + rv[0] * rev, gr[1] * mix + rv[1] * rev];
            let f = self.hp.tick(self.lp.tick(m));
            // feedback path
            let s = a + b;
            let pp = [p[21] * f[0] + f[1] * (1.0 - p[21]), p[21] * f[1] + f[0] * (1.0 - p[21])];
            let fbk = [p[1] * pp[0], p[1] * pp[1]];
            let q = 1.0 - p[22];
            let t1 = (fbk[0] + b) * q + fbk[1] * p[22];
            let u = [p[22] * (s + fbk[0]), q * (a + fbk[1])];
            let t0 = u[1] + u[0];
            let mut h = self.shift([t0, t1], inc);
            if cascade {
                let mut y = h;
                for k in 0..STAGES {
                    y = self.casc[k].allpass(self.slab, y, 0.0, g);
                }
                let w = 1.0 - p[16];
                h = [p[16] * y[0] + w * h[0], p[16] * y[1] + w * h[1]];
            }
            if !hold {
                self.main.write(self.slab, h);
            }
            l[i] = f[0] * p[3] + a * p[2];
            r[i] = f[1] * p[3] + b * p[2];
        }
    }
}

/// `x` reduced modulo `size` with the reference's `x − floor(x/size)·size` (truncate, correct downward).
fn floor_wrap(x: f32, inv: f32, size: f32) -> f32 {
    let q = x * inv;
    let t = cvtt(q) as f32;
    let fl = if q < t { t + -1.0 } else { t };
    x - fl * size
}
