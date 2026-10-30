//! Glorp instrument ID 27: a noise-burst-excited pair of filtered comb resonators (one with negative, one with
//! positive feedback) feeding a short bright comb, a saturator and a noise hiss, on the shared 16-voice `SynthDevice`
//! scheduler, complete for the poly mode the song uses.
//!
//! Independent behavioural implementation written from observed behaviour (research/glorp-devices.md §ID 27); no
//! original binary, table or pseudocode is included. f32 throughout, operation order follows the reference where it
//! matters for the numbers. Fixed 44.1 kHz.
//!
//! # Scheduler
//!
//! The shared one in [`crate::synth`]: 256-slot event queue, in-block offsets, lowest free voice, no stealing (a 17th
//! simultaneous note is lost), note-offs take queue slots and are looked up per voice but **do nothing audible**: the
//! voice lives a fixed time whatever happens ([`Id27::note_off`] is accepted and inert).
//!
//! # Voice
//!
//! A voice is a fixed ~2.5 s one-shot. `t` is an f32 clock advanced by `1/44100` per sample; the voice is freed by the
//! sample that makes `t > 2.5` (it still renders that sample). Accumulating `1/44100` in f32 drifts, so the voice
//! lasts 110 349 samples (2.5022 s), not 110 250. The note seeds a hash chain `s₀ = note·1000 + 412412`
//! (`s ← lowbias32(s)` per draw, shared with ID 25); velocity does not exist. The randomisation drawn at note-on
//! (draws `d₁…d₇`, `uₖ = dₖ·2⁻³²`):
//!
//! ```text
//! f      = max(|400·P2 − 350 + 200·u₁³|, 50)         comb tone (Hz): both combs delay 0.5/f seconds
//! hp0    = 20  + 0.3f + 200·u₂ − 2000·P4              high-pass corner inside combs A and B
//! lp0    = 220 + 0.3f + 200·u₂ + 1200·u₃² + 2000·P5   low-pass base inside combs A and B
//! x      = min(u₆⁴, 1);  s = (3 − 2x)·x²              smoothstep, short-comb length / brightness
//! comb C : delay 1/(2050 + 20000·s) s (2..21 samples), loop filter HP(20 + 20000·P10², Q .7) after
//!          LP(20 + 20000·u₇·(P11² + s), Q .7)
//! hiss   : LP(8000 Hz, Q 0.36) on a fresh noise pair
//! ```
//!
//! (draws `d₄`, `d₅` are consumed and unused). Per sample (`t` = voice clock, noise draws `nA` then `nB` from the chain
//! restarted at the seed, `nA` feeds the right channel, `nB` the left):
//!
//! ```text
//! s     = tsin(2π·100·t)                  100 Hz source (512-node interpolated table sine)
//! k     = 110.9 (s < 0)  or  110.9 + 0.01·443.6;   sq = sgn(s)·(1 − e^(−k|s|)) / (1 − e^(−k))
//! exc   = (n + sq)·e^(−200t)              a ~5 ms burst, per channel
//! g     = 0.85 + 0.14·P3;   lpF = lp0 + 2000·P9·e^(−20t)
//! A     : r0 = delayA;  delayA ← exc − g·HP(hp0, Q .7)(LP(lpF, Q .7)(r0))       negative feedback
//! B     : r1 = delayB;  b = HP(LP(r1));  delayB ← exc + g·b                      positive feedback
//! m     = exc + P0·r0 + P1·b              (the comb A tap is unfiltered, the comb B tap filtered)
//! h     = HP₁₀(LP₁₁(m + 0.99·delayC));  delayC ← h
//! o     = (1 − P12)·m + 0.5·P12·h
//! drv   = 10·P7 + 0.001
//! y     = tanh(drv·o)/tanh(drv) + 0.2·e^(−20t)·tanh(10·LP₈ₖ(n'))            n' = a fresh noise pair
//! out  += P8·y
//! ```
//!
//! `tanh(x)` here is the [7/6] Padé `x(378x⁴+17325x²+135135)/(18x⁶+3150x⁴+62370x²+135135)` (**not** the exact tanh: it
//! overshoots for |x| > ~4); `tanh(drv)` uses a true division, the two per-sample ones use a reciprocal estimate with
//! one Newton step in the original (`rcpps`, CPU-specific) and a true division here. The filters are RBJ
//! low-/high-pass biquads (direct form I); the four loop filters are **re-designed every sample** (A and B share the
//! same low-pass and high-pass coefficients). Delay lines: 44100-entry stereo lines read with an f32 position
//! `write − 44100·time` wrapped into `[0, 44100)`, linear interpolation; `time` follows its target with a 1.0 gain
//! (`time ← (target − time) + time`) and, together with the write position, **persists between notes of a voice**
//! (only the contents are cleared at note-on). Only the last 512 samples are ever read (the delays are at most
//! 441 samples), so each line is a 512-sample ring plus the 44100-modulus position bookkeeping.
//!
//! Parameters: P0 comb A tap gain, P1 comb B tap gain, P2 tone, P3 feedback, P4 / P5 loop filter corners, P7 drive,
//! P8 gain, P9 low-pass sweep depth, P10 / P11 comb C filter corners, P12 comb C wet. **P6 is never read.** P2, P4,
//! P5, P10, P11 are read at note-on only (a later change affects the next note); the others are read every sample.
//!
//! # Not modelled / unsupported
//!
//! The shared device's mono/legato voice mode (`+0x348 == 1`, never set by the song or by any chunk), voices-per-note
//! ≠ 1 (unison), the in-voice slide, sample rates other than 44.1 kHz. **Standalone poly-only API.** This type cannot
//! represent those modes, so it cannot accept a chunk or setting that selects one, and nothing here can detect it (the
//! 13 parameters do not carry the voice mode). A future adapter (`Kinds::load` or equivalent) **MUST reject** a device
//! description whose voice mode is mono/legato or whose voices-per-note is not 1 **before** constructing this device.
//!
//! Numerics: `exp` / `sin` / `cos` are f64 `libm` results rounded to f32 (ordinary approximations; the original's C
//! library rounds some arguments differently, see the research doc), so this is **tolerance parity with the original,
//! not bit parity**: the measured errors are listed in `research/glorp-devices.md` §ID 27.
//!
//! Validity: parameters must be finite and within `[0, 1]`; notes ≤ 127; offsets ≤ `i32::MAX`.
//! Not a `synth_core::Device`: its `set_param` cannot report a rejection.

use crate::id25::{cosf, expf, hash, sine_table, sinf, Biquad, DT, P31, P32, W_RBJ};
use crate::id34::{sin_with, TABLE_LEN};
use crate::synth::{ActiveNote, Synth, Voice as SynthVoice};
use synth_core::denormal::Guard;

/// Parameter count of the device.
pub const PARAMS: usize = 13;
pub use crate::synth::{MAX_NOTE, QUEUE, VOICES};

/// Why a call is not supported. The device is left unchanged when one is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parameter index ≥ 13.
    Index,
    /// Not finite or outside `[0, 1]`.
    Range,
    /// Note above 127.
    Note,
    /// Offset above `i32::MAX`.
    Offset,
    /// All 256 queue slots are taken (the original drops the event silently).
    QueueFull,
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
const C200: f32 = f32::from_bits(0x3348_0000); // 200·2⁻³²
const C1200: f32 = f32::from_bits(0x2496_0000); // 1200·2⁻⁶⁴
const C20000: f32 = f32::from_bits(0x369c_4000); // 20000·2⁻³²
const K03: f32 = f32::from_bits(0x3e99_999a);
const K07: f32 = f32::from_bits(0x3f33_3333); // Q of the loop filters
const K036: f32 = f32::from_bits(0x3eb8_51ec); // Q of the hiss filter
const K001: f32 = f32::from_bits(0x3c23_d70a);
const K4436: f32 = f32::from_bits(0x43dd_cccd);
const K1109: f32 = f32::from_bits(0x42dd_cccd);
const K099: f32 = f32::from_bits(0x3f7d_70a4);
const K014: f32 = f32::from_bits(0x3e0f_5c29);
const K085: f32 = f32::from_bits(0x3f59_999a);
const K0001: f32 = f32::from_bits(0x3a83_126f);
const W100: f32 = f32::from_bits(0x441d_1463); // 2π·100
const NEG_44100: f32 = -44100.0;
const CAP: i32 = 44100;
/// Ring length per delay line: the longest read lag is 0.5/50 s = 441 samples (+1 for the interpolation).
const RING: usize = 512;

/// Low-/high-pass RBJ coefficients `[b0, b1, b2, a1, a2]` (divided by a0), Q floored at 0.05, frequency clamped to
/// 20..22000 Hz; operation order follows the reference.
pub(crate) fn rbj_pass(high: bool, freq: f32, q: f32) -> [f32; 5] {
    let f = if freq < 22000.0 { freq } else { 22000.0 };
    let f = if f > 20.0 { f } else { 20.0 };
    let w = f * W_RBJ;
    let (sn, cs) = (sinf(w), cosf(w));
    let qm = if q > 0.05f32 { q } else { 0.05 };
    let alpha = sn / (qm + qm);
    let (b0, b1) = if high { (cs * 0.5 + 0.5, -1.0 - cs) } else { (0.5 * (1.0 - cs), 1.0 - cs) };
    let a1n = -2.0 * cs;
    let a0 = alpha + 1.0;
    let a2n = 1.0 - alpha;
    let r = 1.0 / a0;
    [r * b0, r * b1, r * b0, r * a1n, a2n / a0]
}

/// One stereo delay line. The contents are a 512-sample ring of the most recent writes; `w` / `time` follow the
/// original's 44100-entry buffer bookkeeping and survive a note-on.
#[derive(Clone, Copy, Debug)]
struct Delay {
    ring: [[f32; 2]; RING],
    /// Next write goes to `head`; `n` = writes since the contents were cleared (saturating).
    head: usize,
    n: u32,
    /// Position in the original's 44100-entry buffer (newest sample).
    w: i32,
    time: f32,
    target: f32,
}

impl Delay {
    fn idle() -> Self {
        Delay { ring: [[0.0; 2]; RING], head: 0, n: 0, w: 0, time: 0.0, target: 0.0 }
    }

    /// Note-on: contents cleared, new target time; the read/write position and the current time are kept.
    fn clear(&mut self, target: f32) {
        self.n = 0;
        self.target = target;
    }

    /// Sample `lag` writes ago (0 = newest), zero when it was cleared or fell out of the ring (never read that far).
    fn lag(&self, lag: i32) -> [f32; 2] {
        let lag = lag as u32;
        // only lags up to 442 are ever asked for (tone >= 50 Hz); 44099 is the wrapped neighbour of an exact-position read
        // (interpolation weight 0), which is zero here as well
        debug_assert!(lag < RING as u32 || lag >= (CAP - 2) as u32, "delay read {lag} samples back, beyond the ring");
        if lag < self.n && (lag as usize) < RING { self.ring[(self.head + RING - 1 - lag as usize) % RING] } else { [0.0; 2] }
    }

    /// Value at buffer index `i` of the original's 44100-entry buffer (`w` = newest).
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
        // the original's smoothing gain is 1.0 (set at note-on); the arithmetic is kept as is
        #[allow(clippy::identity_op)]
        {
            self.time = (self.target - self.time) * 1.0 + self.time;
        }
        self.w = if self.w + 1 < CAP { self.w + 1 } else { 0 };
        self.ring[self.head] = x;
        self.head = (self.head + 1) % RING;
        self.n = self.n.saturating_add(1);
    }
}

/// `[7/6]` Padé `tanh` of the original, scalar form (the `tanh(drv)` normaliser: true division).
fn pade_ratio(x: f32) -> f32 {
    let x2 = x * x;
    let num = ((x2 * 378.0 + 17325.0) * x2 + 135135.0) * x;
    let den = ((x2 * 18.0 + 3150.0) * x2 + 62370.0) * x2 + 135135.0;
    num / den
}

/// The two per-sample Padé evaluations of one channel: noise path `x_n = 10·hiss` weighted `w_n = 0.2·e^(−20t)` and the
/// output path `x_o = drv·o` divided by `tanh(drv)`. Returns `(noise, output)`.
fn pade_pair(x_n: f32, w_n: f32, x_o: f32, norm: f32) -> (f32, f32) {
    let poly = |q: f32| (q * 378.0 + 17325.0) * q + 135135.0;
    let den = |q: f32| ((q * 18.0 + 3150.0) * q + 62370.0) * q + 135135.0;
    let (xn, qn) = (x_n * w_n, x_n * x_n);
    let (qo, vo) = (x_o * x_o, x_o);
    // lane 0 (noise): numerator poly(q)·(x·w), denominator den(q); lane 1 (output): poly(q)·x, den(q)·tanh(drv)
    let noise = poly(qn) * xn / den(qn);
    let out = poly(qo) * vo / (den(qo) * norm);
    (noise, out)
}

/// Biquads 0..3 are the loop filters (coefficients re-designed every sample), 4 / 5 the comb C loop filter, 6 the hiss.
#[derive(Clone, Copy, Debug)]
struct Voice {
    active: bool,
    note: u8,
    t: f32,
    rng: u32,
    hp0: f32,
    lp0: f32,
    bq: [Biquad; 7],
    d: [Delay; 3],
}

impl Voice {
    fn idle() -> Self {
        Voice { active: false, note: 0, t: 0.0, rng: 0, hp0: 0.0, lp0: 0.0, bq: [Biquad::default(); 7], d: [Delay::idle(); 3] }
    }

    /// Note-on: contents of the delays and the filter states are reset, the randomisation is drawn (draw order fixed by
    /// the reference); the delays' `time` and position survive.
    fn note_start(&mut self, note: u8, p: &[f32; PARAMS]) {
        let seed = u32::from(note) * 1000 + 412_412;
        let mut s = seed;
        let mut draw = || {
            s = hash(s);
            s
        };
        let (d1, d2, d3, _d4, _d5, d6, d7) = (draw(), draw(), draw(), draw(), draw(), draw(), draw());
        // tone: u₁² · (200·u₁) − 350 + 400·P2 (single-precision products in this order)
        let x1 = d1 as f32;
        let a1 = x1 * P32;
        let v = (a1 * a1) * (x1 * C200);
        let v = v + -350.0 + p[2] * 400.0;
        let av = v.abs();
        let f = if av > 50.0 { av } else { 50.0 };
        // loop filter bases
        let a2 = (d2 as f32) * C200;
        let b3 = ((d3 as f32) * (d3 as f32)) * C1200;
        self.hp0 = ((K03 * f + a2) + p[4] * -2000.0) + 20.0;
        self.lp0 = ((K03 * f + b3) + p[5] * 2000.0) + (220.0 + a2);
        // short comb: smoothstep of u₆⁴
        let u6 = (d6 as f32) * P32;
        let x = (u6 * u6) * (u6 * u6);
        let x = if x < 1.0 { x } else { 1.0 };
        let sm = (3.0 - (x + x)) * (x * x);
        let half = 0.5 / f;
        self.d[0].clear(half);
        self.d[1].clear(half);
        self.d[2].clear(1.0 / (sm * 20000.0 + 2050.0));
        // comb C loop filter (low-pass then high-pass) and the hiss low-pass
        let f5 = (p[10] * p[10]) * 20000.0 + 20.0;
        let f4 = ((d7 as f32) * C20000) * (p[11] * p[11] + sm) + 20.0;
        self.bq = [Biquad::default(); 7];
        self.bq[4].c = rbj_pass(false, f4, K07);
        self.bq[5].c = rbj_pass(true, f5, K07);
        self.bq[6].c = rbj_pass(false, 8000.0, K036);
        self.active = true;
        self.note = note;
        self.t = 0.0;
        self.rng = seed;
    }

    /// Renders `l.len()` samples, adding into `l` / `r`.
    fn render_samples(&mut self, p: &[f32; PARAMS], table: &[f32; TABLE_LEN], l: &mut [f32], r: &mut [f32]) {
        for i in 0..l.len() {
            let t = self.t;
            // two noise draws: the first feeds the right channel, the second the left
            self.rng = hash(self.rng);
            let n_r = (self.rng as f32) * P31 + -1.0;
            self.rng = hash(self.rng);
            let n_l = (self.rng as f32) * P31 + -1.0;
            // 100 Hz saturating "square" burst source
            let s = sin_with(table, t * W100);
            let k = if s < 0.0 { K1109 } else { K001 * K4436 + K1109 };
            let sgn = if s > 0.0 { 1.0f32 } else if s < 0.0 || s.is_nan() { -1.0 } else { 0.0 };
            let sq = ((1.0 - expf(-(s * k).abs())) * sgn) / (1.0 - expf(-k));
            let en = expf(t * -200.0);
            let exc_r = (n_r + sq) * en;
            let exc_l = (n_l + sq) * en;
            let g = p[3] * K014 + K085;
            let sweep = expf(t * -20.0);
            let lp_f = (sweep * 2000.0) * p[9] + self.lp0;
            let lp = rbj_pass(false, lp_f, K07);
            let hp = rbj_pass(true, self.hp0, K07);
            self.bq[0].c = lp;
            self.bq[2].c = lp;
            self.bq[1].c = hp;
            self.bq[3].c = hp;
            let r0 = self.d[0].read();
            let r1 = self.d[1].read();
            let a0 = self.bq[0].tick(r0);
            let a = self.bq[1].tick(a0);
            self.d[0].write([exc_l - a[0] * g, exc_r - a[1] * g]);
            let ta = [p[0] * r0[0], r0[1] * p[0]];
            let b2 = self.bq[2].tick(r1);
            let b = self.bq[3].tick(b2);
            self.d[1].write([g * b[0] + exc_l, b[1] * g + exc_r]);
            let m_r = (b[1] * p[1] + exc_r) + ta[1];
            let m_l = (p[1] * b[0] + exc_l) + ta[0];
            let c = self.d[2].read();
            let x4 = self.bq[4].tick([c[0] * K099 + m_l, c[1] * K099 + m_r]);
            let h = self.bq[5].tick(x4);
            self.d[2].write(h);
            let dry = 1.0 - p[12];
            let wet = p[12] * 0.5;
            let o_r = h[1] * wet + m_r * dry;
            let o_l = wet * h[0] + dry * m_l;
            // saturator + hiss
            let drv = p[7] * 10.0 + K0001;
            let norm = pade_ratio(drv);
            self.rng = hash(self.rng);
            let h_r = (self.rng as f32) * P31 + -1.0;
            self.rng = hash(self.rng);
            let h_l = (self.rng as f32) * P31 + -1.0;
            let hz = self.bq[6].tick([h_l, h_r]);
            let wn = sweep * 0.2;
            let (nz_r, o2_r) = pade_pair(hz[1] * 10.0, wn, o_r * drv, norm);
            let (nz_l, o2_l) = pade_pair(hz[0] * 10.0, wn, o_l * drv, norm);
            let y_r = o2_r + nz_r;
            let y_l = o2_l + nz_l;
            l[i] = y_l * p[8] + l[i];
            r[i] = y_r * p[8] + r[i];
            self.t = t + DT;
            if !(2.5 >= self.t) {
                self.active = false;
            }
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
        self.note_start(note, c.0);
    }
    fn release(&mut self) {}
    fn render(&mut self, c: &Shared<'_>, l: &mut [f32], r: &mut [f32]) {
        self.render_samples(c.0, c.1, l, r);
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

/// The ID 27 instrument: 16 voices on the shared scheduler, 13 parameters. About 200 KB inline (16 voices × 3 delay
/// rings): put it in a `Box` / `static`, and give debug-build test threads a large stack.
#[derive(Clone, Debug)]
pub struct Id27 {
    p: [f32; PARAMS],
    table: [f32; TABLE_LEN],
    synth: Synth<Voice>,
}

fn validate(v: f32) -> Result<(), Error> {
    if (0.0..=1.0).contains(&v) { Ok(()) } else { Err(Error::Range) }
}

/// Whether `new` accepts these parameters (cheap: builds nothing). `Err` leaves nothing to undo.
pub fn check(p: &[f32; PARAMS]) -> Result<(), Error> {
    p.iter().try_for_each(|&v| validate(v))
}

impl Id27 {
    /// Builds the device from the 13 normalized parameters (a song chunk's first 13 floats). No voice is playing.
    pub fn new(p: &[f32; PARAMS]) -> Result<Self, Error> {
        check(p)?;
        Ok(Id27 { p: *p, table: sine_table(), synth: Synth::new([Voice::idle(); VOICES]) })
    }

    /// Sets one parameter between `process` calls. P2, P4, P5, P10, P11 are latched by the next note-on (a running
    /// voice keeps the values it started with); all others act on the very next sample. On `Err` nothing changed.
    pub fn set_param(&mut self, index: usize, value: f32) -> Result<(), Error> {
        if index >= PARAMS {
            return Err(Error::Index);
        }
        validate(value)?;
        self.p[index] = value;
        Ok(())
    }

    /// Queues a note-on `offset` samples after the start of the next `process` call (velocity does not exist for this
    /// instrument). With every voice busy at the time it fires, it is lost.
    pub fn note_on(&mut self, note: u8, offset: u32) -> Result<(), Error> {
        self.synth.note_on(note, offset).map_err(Error::from)
    }

    /// Queues a note-off. Inaudible (a voice runs its fixed 2.5 s); it only takes a queue slot, as in the original.
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
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let _ftz = Guard::new();
        self.synth.process(&(&self.p, &self.table), l, r);
    }
}
