//! Glorp instrument ID 25: a pitch-independent percussion synth (sine body + detuned noise-modulated sine layer) on the
//! shared 16-voice `SynthDevice` scheduler, complete for the poly mode the song uses.
//!
//! Independent behavioural implementation written from observed behaviour (research/glorp-devices.md §ID 25); no
//! original binary, table or pseudocode is included. f32 throughout, operation order follows the reference because it
//! matters for parity. Fixed 44.1 kHz.
//!
//! # Scheduler (shared by every Glorp instrument)
//!
//! * A 256-slot event queue `{kind, delay, note}`. `note_on` / `note_off` take the first free slot (**none free ⇒ the
//!   original drops the event silently; here `Err(QueueFull)`**). `process` repeats: scan the slots in slot order; an
//!   event with `delay > 0` only shortens the sub-block to its delay, an event with `delay == 0` fires now (note-on
//!   takes the **lowest-index inactive voice**, no stealing: with all 16 busy it is lost; note-off releases **every**
//!   active voice playing that note) and frees its slot; then every active voice renders the sub-block, every remaining
//!   delay shrinks by the sub-block length. Same-offset events therefore fire in slot (= call) order.
//! * Voices accumulate into zeroed output buffers in index order (`out = 3·limited + out`, left then right).
//!
//! # Voice
//!
//! The envelope is only a lifetime: 1 ms attack, 0.5 s decay to a sustain of 0, **sustain forever**; note-off starts a
//! 0.5 s release (a second note-off restarts it); the voice is freed on the sample the envelope reaches 0, but still
//! renders the rest of that sub-block. A voice never released therefore holds its slot for ever. Velocity is ignored;
//! the note only seeds a hash chain (`seed = note·0x12d687 + 0x765fbdc`, state ← `lowbias32(state)` per draw) that
//! (a) randomises seven biquads at note-on (draw `u = bits·2⁻³² − 0.5`, spread scaled by P14) and (b) feeds two white
//! noise streams per sample, so P14 = 0 plus no noise makes every note sound the same.
//!
//! Per sample (`t` = voice clock, `ph` = body phase, both f32 accumulators; `tc = max(t − 0.04, 0)`):
//!
//! ```text
//! sweep = exp((8·P3+1)·(−20·t))        env = exp((4·P2+1)·(−10·tc))     menv = P12·exp((90·P11−140)·t)
//! ph   += ((2200·sweep)·P10 + (70·P0+20))·2π/44100
//! body  = tsin(ph + menv·noiseLP1.L) · env           (tsin: 512-node linearly interpolated table sine)
//! k     = (4.74·exp((−45·P13−5)·t) + 15.8)·P7
//! layer = shelf8(shelf7( tsin(694.29·t + k·noiseLP2.R), tsin(691.15·t + k·noiseLP2.L) ))        (R, L)
//! mix   = layer·(0.15·exp((−39·P5−1)·tc)·P6) + shelf3(shelf8(shelf7(body)))·(0.08 − 0.08·P6)
//! y     = limit(0.4·P9·mix)   per channel, ×3
//! ```
//!
//! `limit`: |x| < 0.6 identity, 0.6..1.4 a parabolic knee `|x| − 0.5625(|x| − 0.6)²`, above 1.4 slope 0.1. Noise draws:
//! first → R, second → L. Parameters P1, P4, P8 are never read. All 15 parameters are modelled; nothing in range is
//! unsupported. **Unsupported / not modelled:** the shared device's mono/legato voice mode (`+0x348 == 1`, never set
//! by the song or by any chunk), voices-per-note ≠ 1 (unison), the in-voice slide, sample rates other than 44.1 kHz.
//!
//! **Standalone poly-only API.** This type cannot represent the original's mono/legato or unison voice modes, so it
//! cannot accept a chunk or setting that selects one, and nothing here can detect it (the 15 parameters do not carry
//! the voice mode). A future adapter (`Kinds::load` or equivalent) **MUST reject** a device description whose voice
//! mode is mono/legato or whose voices-per-note is not 1 **before** constructing this device; loading such a chunk
//! would otherwise render poly-mode audio silently.
//!
//! Validity: parameters must be finite and within `[0, 1]`; notes ≤ 127; offsets ≤ `i32::MAX`.
//! Not a `synth_core::Device`: its `set_param` cannot report a rejection.

use crate::id34::{make_table, sin_with, TABLE_LEN};
use crate::synth::{ActiveNote, Synth, Voice as SynthVoice};
use synth_core::denormal::Guard;

/// Parameter count of the device.
pub const PARAMS: usize = 15;
pub use crate::synth::{MAX_NOTE, QUEUE, VOICES};

/// Why a call is not supported. The device is left unchanged when one is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parameter index ≥ 15.
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

// Constants the reference holds as f32 literals; the bit patterns matter.
pub(crate) const DT: f32 = f32::from_bits(0x37be_37c6); // 1/44100 as the voice clock step
const W_RUN: f32 = f32::from_bits(0x3915_6592); // 2π/44100 in the voice loop
pub(crate) const W_RBJ: f32 = f32::from_bits(0x3915_6590); // 2π/44100 in the biquad setup
const GAIN_K: f32 = f32::from_bits(0x3daa_152d); // log2(10)/40
const HALF_PI: f32 = f32::from_bits(0x3fc9_0fdb);
const NEG_004: f32 = f32::from_bits(0xbd23_d70a);
const K474: f32 = f32::from_bits(0x4097_ae15);
const K158: f32 = f32::from_bits(0x417c_cccd);
const HZ_R: f32 = f32::from_bits(0x442d_92b0); // 694.29199
const HZ_L: f32 = f32::from_bits(0x442c_c9a0); // 691.15039
const K015: f32 = f32::from_bits(0x3e19_999a);
const K008: f32 = f32::from_bits(0x3da3_d70a);
const K04: f32 = f32::from_bits(0x3ecc_cccd);
const K08: f32 = f32::from_bits(0x3f4c_cccd);
const KM06: f32 = f32::from_bits(0xbf19_999a);
const K01: f32 = f32::from_bits(0x3dcc_cccd);
const K071: f32 = f32::from_bits(0x3f35_c28f);
const K09: f32 = f32::from_bits(0x3f66_6666);
const K043: f32 = f32::from_bits(0x3edc_28f6);
const K005: f32 = f32::from_bits(0x3d4c_cccd);
pub(crate) const P32: f32 = f32::from_bits(0x2f80_0000); // 2^-32
const P26: f32 = f32::from_bits(0x3270_0000); // 2^-26
pub(crate) const P31: f32 = f32::from_bits(0x3000_0000); // 2^-31

// Exponentials: the f64 `libm` result rounded once to f32 (`expf(x)` = `exp(x)`, `2^e` = `exp2(e)`), an ordinary
// library approximation. The original's C library (Wine's msvcrt) rounds differently in a small share of arguments;
// that is **not** reproduced, because the provenance of an earlier table + polynomial scheme that matched it bit for
// bit could not be documented as independent (it equals the published musl / ARM optimized-routines `expf` data), so
// it was removed (research doc §ID 25, "Exponential provenance"). The cost is a measured, bounded loss of parity.
pub(crate) fn expf(x: f32) -> f32 {
    libm::exp(x as f64) as f32
}

fn pow2(x: f32) -> f32 {
    libm::exp2(x as f64) as f32
}

// sin / cos: the f64 result rounded to f32 is the closest simple match (0.14 % of arguments differ by 1 ulp).
pub(crate) fn sinf(x: f32) -> f32 {
    libm::sin(x as f64) as f32
}
pub(crate) fn cosf(x: f32) -> f32 {
    libm::cos(x as f64) as f32
}

/// The 512-node sine table of the reference. Built like `id34`'s (`sinf(i·2π/512)`), plus the one measured difference:
/// the original's node 305 is one ulp further from zero than the correctly rounded value (every other node equals
/// it), which the 60 dB shelves of this voice amplify to ~1e-4 relative after a few thousand samples.
pub(crate) fn sine_table() -> [f32; TABLE_LEN] {
    let mut t = make_table();
    if t[305] < 0.0 {
        t[305] = f32::from_bits(t[305].to_bits() + 1);
    }
    t
}

/// The shared 32-bit mixer, applied once per draw (the draw is also the next state).
pub(crate) fn hash(s: u32) -> u32 {
    let x = (s ^ (s >> 16)).wrapping_mul(0x7feb_352d);
    let x = (x ^ (x >> 15)).wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

/// Hash-chain generator: every draw replaces the state.
pub(crate) struct Rng(pub(crate) u32);

impl Rng {
    pub(crate) fn draw(&mut self) -> u32 {
        self.0 = hash(self.0);
        self.0
    }
}

/// Biquad coefficients `[b0, b1, b2, a1, a2]` (all divided by a0) for RBJ type 0 low-pass, 3 peaking, 7 low shelf,
/// 8 high shelf, in the reference's operation order. Types 7 and 8 take sin/cos from the 512-node table.
fn rbj_with(kind: u32, freq: f32, q: f32, gain: f32, table: &[f32; TABLE_LEN]) -> [f32; 5] {
    let w = freq.min(22000.0).max(20.0) * W_RBJ;
    let a = pow2(gain * GAIN_K);
    let qm = if q > K005 { q } else { K005 };
    let (b, a1n, a0, a2n);
    match kind {
        0 | 3 => {
            let (sn, cs) = (sinf(w), cosf(w));
            let alpha = sn / (qm + qm);
            if kind == 0 {
                b = [0.5 * (1.0 - cs), 1.0 - cs, 0.5 * (1.0 - cs)];
                a1n = -2.0 * cs;
                a0 = alpha + 1.0;
                a2n = 1.0 - alpha;
            } else {
                let aa = a * alpha;
                b = [aa + 1.0, -2.0 * cs, 1.0 - aa];
                a1n = -2.0 * cs;
                a0 = alpha / a + 1.0;
                a2n = 1.0 - alpha / a;
            }
        }
        7 | 8 => {
            let cs = sin_with(table, HALF_PI + w);
            let sn = sin_with(table, w);
            let sa = libm::sqrtf(a);
            let x2 = 1.0 + a;
            let x3 = -1.0 + a;
            let t1 = x3 * cs;
            let beta = (sa * sn) / qm;
            let y = cs * x2;
            if kind == 7 {
                let x = x2 - t1;
                let s1 = t1 + x2;
                b = [a * (x + beta), (a + a) * (x3 - y), a * (x - beta)];
                a1n = -2.0 * (y + x3);
                a0 = beta + s1;
                a2n = s1 - beta;
            } else {
                let p = t1 + x2;
                let x = x2 - t1;
                b = [a * (p + beta), (-2.0 * a) * (y + x3), a * (p - beta)];
                a1n = 2.0 * (x3 - y);
                a0 = beta + x;
                a2n = x - beta;
            }
        }
        _ => unreachable!("only the four filter types the voice uses"),
    }
    let r = 1.0 / a0;
    [r * b[0], r * b[1], r * b[2], r * a1n, a2n / a0]
}

#[cfg(test)]
pub(crate) fn rbj(kind: u32, freq: f32, q: f32, gain: f32) -> [f32; 5] {
    rbj_with(kind, freq, q, gain, &sine_table())
}

/// Direct-form-I stereo biquad: `y = ((b0·x + b1·x₁) + b2·x₂) − (a1·y₁ + a2·y₂)` per channel.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Biquad {
    pub(crate) c: [f32; 5],
    x1: [f32; 2],
    x2: [f32; 2],
    y1: [f32; 2],
    y2: [f32; 2],
}

impl Biquad {
    pub(crate) fn tick(&mut self, x: [f32; 2]) -> [f32; 2] {
        let c = &self.c;
        let mut y = [0.0f32; 2];
        for i in 0..2 {
            let ff = (c[0] * x[i] + c[1] * self.x1[i]) + c[2] * self.x2[i];
            y[i] = ff - (c[4] * self.y2[i] + c[3] * self.y1[i]);
        }
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Soft limiter applied to each channel before the ×3: identity below 0.6, parabolic knee to 1.4, slope 0.1 above.
pub(crate) fn soft_limit(x: f32) -> f32 {
    let s: f32 = if x > 0.0 || x.is_nan() {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    };
    let a = x * s;
    let u = a + -1.0;
    let d = u + u;
    let m = if d < -K08 || d.is_nan() {
        a
    } else if d > K08 {
        u * K01 + 1.0
    } else {
        let v = a + KM06;
        a + -0.5625 * (v * v)
    };
    m * s
}

#[derive(Clone, Copy, Debug)]
struct Voice {
    active: bool,
    note: u8,
    /// Voice clock and body phase (f32 accumulators).
    t: f32,
    ph: f32,
    rng: u32,
    /// Lifetime envelope: clock and stage (1 attack, 2 decay, 3 sustain, 4 release, 0 done).
    et: f32,
    stage: u8,
    /// Body shelves/peak 0..2, noise low-passes 3..4, layer shelves 5..6.
    bq: [Biquad; 7],
}

impl Voice {
    fn idle() -> Self {
        Voice { active: false, note: 0, t: 0.0, ph: 0.0, rng: 0, et: 0.0, stage: 0, bq: [Biquad::default(); 7] }
    }

    /// Note-on: fresh state and the randomised filter set (draw order fixed by the reference).
    fn note_start(&mut self, note: u8, p: &[f32; PARAMS], table: &[f32; TABLE_LEN]) {
        let q = p[14];
        let mut rng = Rng((note as u32).wrapping_mul(0x12d687).wrapping_add(0x765f_bdc));
        let mut draw = || rng.draw();
        // u = bits·2⁻³² − 0.5 (the bit pattern converted as an unsigned integer)
        let u = |h: u32| (h as f32) * P32 + -0.5;
        let (q5, q15, q50, q2000, q100, q10, q15g, q400, q40, q2500, q025) =
            (5.0 * q, 1.5 * q, 50.0 * q, q * 2000.0, 100.0 * q, 10.0 * q, 15.0 * q, 400.0 * q, 40.0 * q, 2500.0 * q, 0.25 * q);
        // 0: low shelf (type 7); the first frequency draw is scaled by 2⁻²⁶ and offset by −30
        let (h1, h2) = (draw(), draw());
        let f = ((h1 as f32) * P26 + -30.0) * q5 + 147.0;
        let qq = u(h2) * q15 + K071;
        let g = u(draw()) * q50 + 60.0;
        self.bq[0] = Biquad { c: rbj_with(7, f, qq, g, table), ..Biquad::default() };
        // 1: high shelf (type 8)
        let (h4, h5) = (draw(), draw());
        let f = q2000 * u(h4) + 1660.0;
        let qq = q15 * u(h5) + K071;
        let g = u(draw()) * q50 + 40.0;
        self.bq[1] = Biquad { c: rbj_with(8, f, qq, g, table), ..Biquad::default() };
        // 2: peaking (type 3)
        let (h7, h8) = (draw(), draw());
        let f = q100 * u(h7) + 70.0;
        let qq = q10 * u(h8) + 8.0;
        let g = u(draw()) * q15g + 9.0;
        self.bq[2] = Biquad { c: rbj_with(3, f, qq, g, table), ..Biquad::default() };
        // 3, 4: noise low-passes (type 0, 0 dB)
        let f = u(draw()) * q2500 + 2000.0;
        let qq = u(draw()) * q025 + K01;
        self.bq[3] = Biquad { c: rbj_with(0, f, qq, 0.0, table), ..Biquad::default() };
        let f = u(draw()) * q2500 + 2000.0;
        let qq = u(draw()) * q025 + K01;
        self.bq[4] = Biquad { c: rbj_with(0, f, qq, 0.0, table), ..Biquad::default() };
        // 5: low shelf, 6: high shelf (the layer)
        let (h14, h15) = (draw(), draw());
        let f = q400 * u(h14) + 206.0;
        let qq = q15 * u(h15) + K09;
        let g = u(draw()) * q50 + 40.0;
        self.bq[5] = Biquad { c: rbj_with(7, f, qq, g, table), ..Biquad::default() };
        let (h17, h18) = (draw(), draw());
        let f = q2500 * u(h17) + 2000.0;
        let qq = q * u(h18) + K043;
        let g = u(draw()) * q40 + 25.0;
        self.bq[6] = Biquad { c: rbj_with(8, f, qq, g, table), ..Biquad::default() };
        // the reference sets up one more (unused) filter with two draws; they still advance the noise state
        draw();
        draw();
        self.active = true;
        self.note = note;
        self.t = 0.0;
        self.ph = 0.0;
        self.rng = rng.0;
        self.et = 0.0;
        self.stage = 1;
    }

    fn note_release(&mut self) {
        self.stage = 4;
        self.et = 0.0;
    }

    /// Lifetime envelope tick (the level itself is never used).
    fn envelope(&mut self) {
        self.et += DT;
        match self.stage {
            1 if self.et >= 0.001f32 => {
                self.et = 0.0;
                self.stage = 2;
            }
            2 if self.et >= 0.5 => {
                self.et = 0.0;
                self.stage = 3;
            }
            4 if self.et >= 0.5 => self.stage = 0,
            _ => {}
        }
    }

    /// Renders `l.len()` samples, adding into `l` / `r`.
    fn render_samples(&mut self, p: &[f32; PARAMS], table: &[f32; TABLE_LEN], l: &mut [f32], r: &mut [f32]) {
        for i in 0..l.len() {
            let t = self.t;
            let f0 = p[0] * 70.0 + 20.0;
            let sweep = expf((p[3] * 8.0 + 1.0) * (t * -20.0));
            let tc = {
                let x = t + NEG_004;
                if x > 0.0 { x } else { 0.0 }
            };
            let env = expf((p[2] * 4.0 + 1.0) * (tc * -10.0));
            let menv = expf((p[11] * 90.0 + -140.0) * t) * p[12];
            // two noise draws per sample: the first feeds the right channel, the second the left
            self.rng = hash(self.rng);
            let n_r = (self.rng as f32) * P31 + -1.0;
            self.rng = hash(self.rng);
            let n_l = (self.rng as f32) * P31 + -1.0;
            let noise = [n_l, n_r];
            let d = self.bq[3].tick(noise);
            let w = (((sweep * 2200.0) * p[10] + f0) * W_RUN) + self.ph;
            self.ph = w;
            let sa = sin_with(table, menv * d[0] + w);
            let nenv = expf((p[5] * -39.0 + -1.0) * tc);
            let lenv = expf((p[13] * -45.0 + -5.0) * t);
            let e = self.bq[4].tick(noise);
            let k = (lenv * K474 + K158) * p[7];
            let arg_r = t * HZ_R + e[1] * k;
            let arg_l = t * HZ_L + k * e[0];
            let s_r = sin_with(table, arg_r);
            let s_l = sin_with(table, arg_l);
            let lay5 = self.bq[5].tick([s_l, s_r]);
            let lay = self.bq[6].tick(lay5);
            let body = sa * env;
            let b0 = self.bq[0].tick([body, body]);
            let b1 = self.bq[1].tick(b0);
            let b = self.bq[2].tick(b1);
            let k1 = (nenv * K015) * p[6];
            let c2 = K008 - p[6] * K008;
            let g = p[9] * K04;
            let y_l = g * (k1 * lay[0] + c2 * b[0]);
            let y_r = g * (k1 * lay[1] + c2 * b[1]);
            l[i] = soft_limit(y_l) * 3.0 + l[i];
            r[i] = soft_limit(y_r) * 3.0 + r[i];
            self.envelope();
            self.t = t + DT;
            if self.stage == 0 {
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
        self.note_start(note, c.0, c.1);
    }
    fn release(&mut self) {
        self.note_release();
    }
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

/// The ID 25 instrument: 16 voices on the shared scheduler, 15 parameters.
#[derive(Clone, Debug)]
pub struct Id25 {
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

impl Id25 {
    /// Builds the device from the 15 normalized parameters (a song chunk's first 15 floats). No voice is playing.
    pub fn new(p: &[f32; PARAMS]) -> Result<Self, Error> {
        check(p)?;
        Ok(Id25 { p: *p, table: sine_table(), synth: Synth::new([Voice::idle(); VOICES]) })
    }

    /// Sets one parameter between `process` calls (voices keep playing with the new value; filter randomisation is
    /// drawn at note-on from P14 as it is then). On `Err` nothing changed.
    pub fn set_param(&mut self, index: usize, value: f32) -> Result<(), Error> {
        if index >= PARAMS {
            return Err(Error::Index);
        }
        validate(value)?;
        self.p[index] = value;
        Ok(())
    }

    /// Queues a note-on `offset` samples after the start of the next `process` call (velocity does not exist for
    /// this instrument). With every voice busy at the time it fires, it is lost.
    pub fn note_on(&mut self, note: u8, offset: u32) -> Result<(), Error> {
        self.synth.note_on(note, offset).map_err(Error::from)
    }

    /// Queues a note-off (releases every voice currently playing `note`).
    pub fn note_off(&mut self, note: u8, offset: u32) -> Result<(), Error> {
        self.synth.note_off(note, offset).map_err(Error::from)
    }

    /// Voices currently holding a slot (playing, in release, or released earlier in the running sub-block).
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
