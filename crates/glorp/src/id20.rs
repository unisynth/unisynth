//! Glorp instrument ID 20: twelve independent negative-feedback comb resonators in three groups, excited by shaped noise
//! plus a decaying sine click, each group frequency-shifted and the sum shifted again, on the shared 16-voice
//! `Synth` scheduler. Complete for the poly mode the song uses, velocity sensitive.
//!
//! Independent behavioural implementation written from observed behaviour (research/glorp-devices.md §ID 20); no
//! original binary, table or pseudocode is included. f32 throughout, operation order follows the reference where it
//! matters for the numbers. Fixed 44.1 kHz.
//!
//! # Scheduler
//!
//! The shared one in [`crate::synth`]: 256-slot event queue, in-block offsets, lowest free voice, no stealing (a 17th
//! simultaneous note is lost), note-offs take queue slots and are looked up per voice but **do nothing audible**.
//! **Velocity exists** here: `vel / 128` scales the excitation and shortens the noise envelope.
//!
//! # Voice
//!
//! **All 16 parameters are read at note-on only**, the render loop reads none. The note and P0 only seed a hash chain
//! `s₀ = note + trunc(150·P0) + 0x765fbb8`; every draw is `s ← lowbias32(s)` (shared with ID 25) and `u = s·2⁻³²`. The
//! chain runs on into the render loop (two noise draws per sample), so the 68 draws of the note-on, including those of
//! the unrendered group 3, decide the noise. Per group `g = 0..4` (groups 0..2 are rendered), per line `k = 0..4`,
//! `j = 4g + k`, four draws:
//!
//! ```text
//! freq_j  = clamp(500 + 14500·P(1+g)² + (u₁ − ½)·5000·P7, 500, 20000)          comb tone (Hz), delay 1/freq_j s
//! fb_j    = lo + (0.999 − lo)·(½ + (u₂ − ½)·P6),   lo = ½ + 0.49·P5             feedback gain, applied negated
//! hp_j    = (0.01 + ½·P8·u₃)²·21960 + 40 Hz, Q 0.7           loop high-pass, then
//! lp_j    = (0.99 − ½·P8·u₄)²·21960 + 40 Hz, Q 0.7           loop low-pass
//! ```
//!
//! and after the four lines one draw `u₅` per group: group shift increment `1 + 0.2·(−0.1·P9·u₅)` (a phase advance just
//! below one cycle per sample = a slow downward drift of up to `0.02·P9·44100` Hz). P4 only feeds group 3 (never
//! rendered): **inert**. Master shift `4000·P15 − 2000` Hz. Envelopes: noise `e^((P13 + 0.1·v)/735 − 1/630)` per sample
//! (`v = vel/128`), click `e^(P12/169.6 − 1/163.3)`; both start at 1. Click oscillator: damped sine at
//! `40 + 7960·P14²` Hz, `r = e^(−1/22.05e6)`, started at phase 0. Noise filters: low-pass `40 + 12460·P11²`,
//! high-pass `40 + 12460·P10²`, Q 1, **their state is never reset**.
//!
//! Per sample (R = first noise draw, L = second): `e = HP(LP(noise·envN))`, `exc = v·(e + envC·click)`, then per line
//! `d = delay_j.read(); delay_j.write(LP_j(HP_j(−fb_j·d)) + exc)`, the **raw** delay output `d` summed per group, each
//! group sum through a Hilbert frequency shifter (the ID 34 one: two 4-section all-pass chains, phase `φ += Δ`, one
//! subtraction of 1 when `φ > 1`) scaled `½`, the three added, halved, shifted by the master shifter and added to the
//! output. Delay lines: 44100-entry stereo buffers read at the f32 position `write − 44100·time` (linear
//! interpolation), `time ← (target − time)·0.021 + time` before each write: **the delay's contents, `time`, write
//! position, the shifters' state and phase, the level follower and the noise filters survive from note to note of a
//! voice** (nothing is cleared at note-on; the loop filters' state is). Only lags up to `1/500 s = 88.2` samples are
//! ever read, so each line is a 128-sample ring plus the 44100-modulus position bookkeeping.
//!
//! **A voice ends when its level follower falls below `1e-4`**, checked after every render call of the voice (the end
//! of every sub-block, not every sample). The follower is a peak-ish tracker of `(|L| + |R|)/2` of the voice output
//! (rise `0.101·(x − s)`, fall `s·(0.99999 − 0.05·0.9³)`), it is **not reset** at note-on, and it is 0 for a fresh
//! voice: a note whose first sub-block is a few samples long (another event right after it, or the last samples of a
//! block) is freed before it is heard. Velocity 0 gives silence and a voice freed at the end of its first sub-block.
//!
//! # Velocity ceiling
//!
//! The noise envelope's per-sample factor is `e^((P13 + 0.1·v)/735 − 1/630)`: from velocity 214 on, with P13 = 1 it is
//! **above one** (the noise grows forever, the voice never ends and the output overflows). [`MAX_VELOCITY`] = 213 is
//! the largest velocity for which the factor is below one whatever P13: [`Id20::note_on`] refuses a higher one
//! ([`Error::Velocity`]). The song format carries velocities up to 255.
//!
//! # Not modelled / unsupported
//!
//! The shared device's mono/legato voice mode, voices-per-note ≠ 1 (unison), the in-voice slide, sample rates other
//! than 44.1 kHz, velocities above 213. **Standalone poly-only API**: it cannot represent those modes and cannot
//! detect them (the 16 parameters do not carry the voice mode); an adapter MUST reject a chunk longer than 16
//! parameters + size.
//!
//! Numerics: `exp` / `sin` / `cos` are f64 `libm` results rounded to f32 (ordinary approximations; the original's C
//! library rounds some arguments one ulp differently, `id25::expf`). Measured against the original: **bit-exact** on all
//! 53 synthetic vectors (every parameter at both ends, polyphony, velocity sweep, level-follower kills), and on the whole
//! song bit-exact except 1,499 of 78,292 blocks (from block 38,967) with a worst error of 1.1e-4 at peak 4.73 (2.4e-5 of
//! it): tolerance parity on a long song, see `research/glorp-devices.md` §ID 20.

//! Validity: parameters must be finite and within `[0, 1]`; notes ≤ 127; velocity ≤ 213; offsets ≤ `i32::MAX`.
//! Not a `synth_core::Device`: its `set_param` cannot report a rejection.

use crate::id25::{expf, hash, sine_table, Biquad, P31, P32};
use crate::id27::rbj_pass;
use crate::id34::{sin_with, tick, Hist, CHAIN_A, CHAIN_B, HALF_PI, TABLE_LEN, TAU};
use crate::synth::{ActiveNote, Synth, Voice as SynthVoice};
use synth_core::denormal::Guard;

/// Parameter count of the device.
pub const PARAMS: usize = 16;
pub use crate::synth::{MAX_NOTE, QUEUE, VOICES};
/// Largest accepted velocity: the noise envelope decays for every P13 up to here (see the module doc).
pub const MAX_VELOCITY: u8 = 213;

/// Why a call is not supported. The device is left unchanged when one is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parameter index ≥ 16.
    Index,
    /// Not finite or outside `[0, 1]`.
    Range,
    /// Note above 127.
    Note,
    /// Offset above `i32::MAX`.
    Offset,
    /// All 256 queue slots are taken (the original drops the event silently).
    QueueFull,
    /// Velocity above [`MAX_VELOCITY`]: the noise envelope could grow without bound.
    Velocity,
}

impl From<crate::synth::Error> for Error {
    fn from(e: crate::synth::Error) -> Self {
        match e {
            crate::synth::Error::Note => Error::Note,
            crate::synth::Error::Offset => Error::Offset,
            crate::synth::Error::QueueFull => Error::QueueFull,
        }
    }
}

// Constants the reference holds as f32 literals (bit patterns matter for the arithmetic).
const K_VEL: f32 = f32::from_bits(0x3c00_0000); // 1/128
const K_SEED: f32 = 150.0;
const K14500: f32 = 14500.0;
const K5000: f32 = 5000.0;
const K049: f32 = f32::from_bits(0x3efa_e148);
const K0499: f32 = f32::from_bits(0x3eff_7cee);
const P33: f32 = f32::from_bits(0x2f00_0000); // 2^-33
const K001: f32 = f32::from_bits(0x3c23_d70a);
const K21960: f32 = 21960.0;
const K099: f32 = f32::from_bits(0x3f7d_70a4);
const K_SHIFT: f32 = f32::from_bits(0xadcc_cccd); // −0.1·2⁻³²
const K02: f32 = f32::from_bits(0x3e4c_cccd);
const K01: f32 = f32::from_bits(0x3dcc_cccd);
const K_N1: f32 = f32::from_bits(0x3ab2_544a); // 1/735
const K_N2: f32 = f32::from_bits(0xbad0_0d01); // −1/630
const K_C1: f32 = f32::from_bits(0x3bc1_30a5); // 1/169.6
const K_C2: f32 = f32::from_bits(0xbbc8_9ed3); // −1/163.3
const K_R: f32 = f32::from_bits(0xb342_c879); // −1/22.05e6
const K_W: f32 = f32::from_bits(0x3915_6592); // 2π/44100
const K_INV_RATE: f32 = f32::from_bits(0x37be_37c6); // 1/44100
const Q07: f32 = f32::from_bits(0x3f33_3333);
/// Rise gain of the level follower: `a³·0.1 + 0.001` with the constructor's `a = 1`.
const FOLLOW_RISE: f32 = K01 + f32::from_bits(0x3a83_126f);
const FOLLOW_FALL_C: f32 = f32::from_bits(0x3f7f_ff58); // 0.99999
const FOLLOW_FALL_K: f32 = f32::from_bits(0xbd4c_cccd); // −0.05
const FOLLOW_B: f32 = f32::from_bits(0x3f66_6666); // 0.9
const FOLLOW_END: f32 = f32::from_bits(0x38d1_b717); // 1e-4
/// Delay time smoothing gain `0.02 + 0.001` (the delay object adds a millisecond-ish constant to its coefficient).
const SMOOTH: f32 = f32::from_bits(0x3ca3_d70a) + f32::from_bits(0x3a83_126f);
const NEG_44100: f32 = -44100.0;
const CAP: i32 = 44100;
/// Ring length per delay line: the longest read lag is `1/500 s` = 88.2 samples (+1 for the interpolation).
const RING: usize = 128;
/// Rendered lines (groups 0..3 of four) and groups.
const LINES: usize = 12;
const GROUPS: usize = 3;

/// The exponent of the noise envelope's per-sample factor `e^x`: decaying while negative. Exposed for the preflight
/// and the tests that pin [`MAX_VELOCITY`].
pub fn noise_decay_exponent(p13: f32, velocity: u8) -> f32 {
    let v = f32::from(velocity) * K_VEL;
    ((v * K01 + p13) * K_N1) + K_N2
}

/// One stereo delay line. The contents are a 128-sample ring of the most recent writes; `w` / `time` follow the
/// original's 44100-entry buffer bookkeeping and **everything survives a note-on**.
#[derive(Clone, Copy, Debug)]
struct Delay {
    ring: [[f32; 2]; RING],
    head: usize,
    n: u32,
    w: i32,
    time: f32,
    target: f32,
}

impl Delay {
    fn idle() -> Self {
        Delay { ring: [[0.0; 2]; RING], head: 0, n: 0, w: 0, time: 0.0, target: 0.1 }
    }

    /// Sample `lag` writes ago (0 = newest), zero when never written or out of the ring (never read that far).
    fn lag(&self, lag: i32) -> [f32; 2] {
        let lag = lag as u32;
        // lags up to 89 are asked for; 44099 is the wrapped neighbour of an exact-position read (weight 0)
        debug_assert!(lag < RING as u32 || lag >= (CAP - 2) as u32, "delay read {lag} samples back, beyond the ring");
        if lag < self.n && (lag as usize) < RING { self.ring[(self.head + RING - 1 - lag as usize) % RING] } else { [0.0; 2] }
    }

    fn at(&self, i: i32) -> [f32; 2] {
        self.lag((self.w - i).rem_euclid(CAP))
    }

    /// Read with offset 0: position `time·(−44100) + w` wrapped into `[0, 44100)`, linear interpolation.
    fn read(&self) -> [f32; 2] {
        let capf = CAP as f32;
        let mut p = (0.0 + self.time) * NEG_44100 + self.w as f32;
        let q = p + capf;
        if q < capf {
            p = if q >= 0.0 { q } else { q + capf };
        }
        let i = p as i32;
        let i1 = if i + 1 >= CAP { i + 1 - CAP } else { i + 1 };
        let fr = p - i as f32;
        let (b0, b1) = (self.at(i), self.at(i1));
        let k = 1.0 - fr;
        [fr * b1[0] + k * b0[0], fr * b1[1] + k * b0[1]]
    }

    fn write(&mut self, x: [f32; 2]) {
        self.time = (self.target - self.time) * SMOOTH + self.time;
        self.w = if self.w + 1 < CAP { self.w + 1 } else { 0 };
        self.ring[self.head] = x;
        self.head = (self.head + 1) % RING;
        self.n = self.n.saturating_add(1);
    }
}

/// The Hilbert single-sideband shifter of ID 34 run once per sample with a given phase increment.
#[derive(Clone, Copy, Debug)]
struct Shifter {
    /// `st[chain][section][channel]`.
    st: [[[Hist; 2]; 4]; 2],
    prev: [f32; 2],
    phase: f32,
}

impl Shifter {
    fn idle() -> Self {
        Shifter { st: [[[[0.0; 4]; 2]; 4]; 2], prev: [0.0; 2], phase: 0.0 }
    }

    fn tick(&mut self, table: &[f32; TABLE_LEN], x: [f32; 2], inc: f32) -> [f32; 2] {
        let mut a = x;
        let mut b = self.prev;
        for (ch, (a, b)) in a.iter_mut().zip(b.iter_mut()).enumerate() {
            for k in 0..4 {
                *a = tick(&CHAIN_A[k], &mut self.st[0][k][ch], *a);
                *b = tick(&CHAIN_B[k], &mut self.st[1][k][ch], *b);
            }
        }
        self.prev = x;
        let v = inc + self.phase;
        self.phase = if 1.0 < v { v + -1.0 } else { v };
        let arg = self.phase * TAU;
        let c = sin_with(table, HALF_PI + arg);
        let s = sin_with(table, arg);
        [c * a[0] + s * b[0], c * a[1] + s * b[1]]
    }
}

/// One voice: everything the note-on latches plus the state that outlives notes.
#[derive(Clone, Copy, Debug)]
struct Voice {
    active: bool,
    note: u8,
    vel: f32,
    rng: u32,
    fb: [f32; LINES],
    d: [Delay; LINES],
    hp: [Biquad; LINES],
    lp: [Biquad; LINES],
    nlp: Biquad,
    nhp: Biquad,
    group: [Shifter; GROUPS],
    master: Shifter,
    group_inc: [f32; GROUPS],
    master_hz: f32,
    env_n: f32,
    coef_n: f32,
    env_c: f32,
    coef_c: f32,
    /// Click oscillator: `[2r·cos w, r², y, y₋₁]`.
    click: [f32; 4],
    follower: f32,
}

impl Voice {
    fn idle() -> Self {
        Voice {
            active: false,
            note: 0,
            vel: 0.0,
            rng: 0,
            fb: [0.0; LINES],
            d: [Delay::idle(); LINES],
            hp: [Biquad::default(); LINES],
            lp: [Biquad::default(); LINES],
            nlp: Biquad::default(),
            nhp: Biquad::default(),
            group: [Shifter::idle(); GROUPS],
            master: Shifter::idle(),
            group_inc: [0.0; GROUPS],
            master_hz: 0.0,
            env_n: 1.0,
            coef_n: 1.0,
            env_c: 1.0,
            coef_c: 1.0,
            click: [0.0; 4],
            follower: 0.0,
        }
    }

    /// Note-on: draws the randomisation (draw order fixed by the reference), re-tunes the loop and noise filters,
    /// restarts the envelopes and the click; delay contents, shifters, follower and noise-filter state are kept.
    fn note_start(&mut self, note: u8, velocity: u8, p: &[f32; PARAMS], table: &[f32; TABLE_LEN]) {
        self.active = true;
        self.note = note;
        self.vel = f32::from(velocity) * K_VEL;
        let seed = u32::from(note).wrapping_add(((p[0] * K_SEED) as i64) as u32).wrapping_add(0x765f_bb8);
        let mut s = seed;
        let mut draw = || {
            s = hash(s);
            s
        };
        let mut shift = [0.0f32; GROUPS];
        for g in 0..4 {
            let pg = p[1 + g];
            let base = pg * pg * K14500 + 500.0;
            for k in 0..4 {
                let j = 4 * g + k;
                let u1 = (draw() as f32) * P32 + -0.5;
                let freq = (u1 * (p[7] * K5000) + base).max(500.0).min(20000.0);
                let u2 = (draw() as f32) * P32 + -0.5;
                let lo = p[5] * K049 + 0.5;
                let fb = (K0499 - p[5] * K049) * (u2 * p[6] + 0.5) + lo;
                let d3 = draw() as f32;
                let hf = p[8] * P33 * d3 + K001;
                let hf = hf * hf * K21960 + 40.0;
                let d4 = draw() as f32;
                let lf = K099 - p[8] * P33 * d4;
                let lf = lf * lf * K21960 + 40.0;
                if j < LINES {
                    self.fb[j] = fb;
                    self.hp[j] = Biquad::default();
                    self.lp[j] = Biquad::default();
                    self.hp[j].c = rbj_pass(true, hf, Q07);
                    self.lp[j].c = rbj_pass(false, lf, Q07);
                    self.d[j].target = 1.0 / freq;
                }
            }
            let u5 = draw() as f32;
            if g < GROUPS {
                shift[g] = p[9] * K_SHIFT * u5;
            }
        }
        self.rng = s;
        for g in 0..GROUPS {
            self.group_inc[g] = shift[g] * K02 + 1.0;
        }
        self.master_hz = p[15] * 4000.0 + -2000.0;
        self.coef_n = expf(noise_decay_exponent(p[13], velocity));
        self.coef_c = expf(p[12] * K_C1 + K_C2);
        self.env_n = 1.0;
        self.env_c = 1.0;
        // damped-sine click: c1 = 2r·cos w, c2 = r², y = sin 0, y₋₁ = sin(−w)/r
        let f = p[14] * p[14] * 7960.0 + 40.0;
        let r = expf(K_R);
        let w = f * K_W;
        let cw = sin_with(table, HALF_PI + w);
        self.click = [cw * (r + r), r * r, sin_with(table, 0.0), sin_with(table, 0.0 - w) / r];
        self.nlp.c = rbj_pass(false, p[11] * p[11] * 12460.0 + 40.0, 1.0);
        self.nhp.c = rbj_pass(true, p[10] * p[10] * 12460.0 + 40.0, 1.0);
    }

    /// Renders `l.len()` samples, adding into `l` / `r`; frees the voice when the follower ends below 1e-4.
    fn render_samples(&mut self, table: &[f32; TABLE_LEN], l: &mut [f32], r: &mut [f32]) {
        for i in 0..l.len() {
            // noise: the first draw feeds the right channel, the second the left
            self.rng = hash(self.rng);
            let n_r = (self.rng as f32) * P31 + -1.0;
            self.rng = hash(self.rng);
            let n_l = (self.rng as f32) * P31 + -1.0;
            let en = self.env_n;
            let e = self.nhp.tick(self.nlp.tick([n_l * en, n_r * en]));
            let y = self.click[2];
            let ec = self.env_c;
            let cy = ec * y;
            let (xr, xl) = (e[1] + cy, cy + e[0]);
            let vel = self.vel;
            self.env_n = en * self.coef_n;
            let (exc_r, exc_l) = (xr * vel, xl * vel);
            self.env_c = ec * self.coef_c;
            let ny = self.click[0] * y - self.click[3] * self.click[1];
            self.click[3] = y;
            self.click[2] = ny;
            let mut acc = [0.0f32; 2];
            for g in 0..GROUPS {
                let mut sum = [0.0f32; 2];
                for k in 0..4 {
                    let j = 4 * g + k;
                    let d = self.d[j].read();
                    let nfb = -self.fb[j];
                    let h = self.hp[j].tick([nfb * d[0], d[1] * nfb]);
                    let o = self.lp[j].tick(h);
                    self.d[j].write([o[0] + exc_l, o[1] + exc_r]);
                    sum = [sum[0] + d[0], sum[1] + d[1]];
                }
                let s = self.group[g].tick(table, sum, self.group_inc[g]);
                acc = [acc[0] + s[0] * 0.5, acc[1] + s[1] * 0.5];
            }
            let m = self.master.tick(table, [acc[0] * 0.5, acc[1] * 0.5], self.master_hz * K_INV_RATE);
            l[i] = m[0] + l[i];
            r[i] = m[1] + r[i];
            // level follower of the voice output
            let x = (m[0].abs() + m[1].abs()) * 0.5;
            self.follower = if x > self.follower {
                (x - self.follower) * FOLLOW_RISE + self.follower
            } else {
                let b = FOLLOW_B;
                (b * b * FOLLOW_FALL_K * b + FOLLOW_FALL_C) * self.follower
            };
        }
        if FOLLOW_END > self.follower {
            self.active = false;
        }
    }
}

/// Read-only state the voices share: the parameters and the sine table.
type Shared<'a> = (&'a [f32; PARAMS], &'a [f32; TABLE_LEN]);

impl SynthVoice<Shared<'_>> for Voice {
    fn active(&self) -> bool {
        self.active
    }
    fn note(&self) -> u8 {
        self.note
    }
    fn start(&mut self, note: u8, c: &Shared<'_>) {
        self.note_start(note, 0, c.0, c.1);
    }
    fn start_vel(&mut self, note: u8, velocity: u8, c: &Shared<'_>) {
        self.note_start(note, velocity, c.0, c.1);
    }
    fn release(&mut self) {}
    fn render(&mut self, c: &Shared<'_>, l: &mut [f32], r: &mut [f32]) {
        self.render_samples(c.1, l, r);
    }
}

impl ActiveNote for Voice {
    fn is_active(&self) -> bool {
        self.active
    }
    fn held_note(&self) -> u8 {
        self.note
    }
}

/// The ID 20 instrument: 16 voices on the shared scheduler, 16 parameters. About 215 KB inline (16 voices × 12 delay
/// rings): put it in a `Box` / `static`.
#[derive(Clone, Debug)]
pub struct Id20 {
    p: [f32; PARAMS],
    table: [f32; TABLE_LEN],
    synth: Synth<Voice>,
}

fn validate(v: f32) -> Result<(), Error> {
    if (0.0..=1.0).contains(&v) { Ok(()) } else { Err(Error::Range) }
}

/// Whether `new` accepts these parameters (cheap: builds nothing). Every vector inside `[0, 1]^16` is supported.
pub fn check(p: &[f32; PARAMS]) -> Result<(), Error> {
    p.iter().try_for_each(|&v| validate(v))
}

impl Id20 {
    /// Builds the device from the 16 normalized parameters (a song chunk's first 16 floats). No voice is playing.
    pub fn new(p: &[f32; PARAMS]) -> Result<Self, Error> {
        check(p)?;
        Ok(Id20 { p: *p, table: sine_table(), synth: Synth::new([Voice::idle(); VOICES]) })
    }

    /// Sets one parameter between `process` calls. All 16 are latched by the next note-on (a running voice keeps the
    /// values it started with). On `Err` nothing changed.
    pub fn set_param(&mut self, index: usize, value: f32) -> Result<(), Error> {
        if index >= PARAMS {
            return Err(Error::Index);
        }
        validate(value)?;
        self.p[index] = value;
        Ok(())
    }

    /// Queues a note-on `offset` samples after the start of the next `process` call. With every voice busy at the time
    /// it fires, it is lost. `Err(Velocity)` above [`MAX_VELOCITY`].
    pub fn note_on(&mut self, note: u8, velocity: u8, offset: u32) -> Result<(), Error> {
        if velocity > MAX_VELOCITY {
            return Err(Error::Velocity);
        }
        self.synth.note_on_vel(note, velocity, offset).map_err(Error::from)
    }

    /// Queues a note-off. Inaudible (the voice runs until its follower ends); it only takes a queue slot, as in the
    /// original, and splits the sub-block at its offset.
    pub fn note_off(&mut self, note: u8, offset: u32) -> Result<(), Error> {
        self.synth.note_off(note, offset).map_err(Error::from)
    }

    /// Voices currently holding a slot.
    pub fn active_voices(&self) -> usize {
        self.synth.active_voices()
    }

    /// Note held by each voice slot (`None` = free).
    pub fn voice_notes(&self) -> [Option<u8>; VOICES] {
        self.synth.voice_notes()
    }

    /// Renders a stereo block: the buffers are **overwritten** (an instrument, not an effect). `l` and `r` must have
    /// the same length (panics otherwise). Events whose offset lies inside the block fire at that sample, later ones
    /// stay queued for the following calls.
    ///
    /// **The call boundary is part of the model.** A voice is freed when its level follower is below 1e-4 at the end
    /// of a sub-block, and sub-blocks end at the call boundary as well as at event offsets, so the call length decides
    /// *when* a voice expires (a note whose first sub-block is a few samples long is freed unheard). The player and
    /// the reference render in calls of 128 frames, so that is the only size whose expiry matches the original; other
    /// sizes are self-consistent but not reference-equal. Panics when `l` and `r` differ in length or when the block is
    /// longer than `i32::MAX` samples (event delays are `i32`); the buffers are not touched in either case.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let _ftz = Guard::new();
        self.synth.process(&(&self.p, &self.table), l, r);
    }
}
