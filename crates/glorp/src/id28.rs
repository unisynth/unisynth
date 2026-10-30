//! Glorp instrument ID 28: a unison oscillator bank (up to 32 detuned / spread oscillators per voice, a phase-warped
//! sine-fold or saw waveform, power curve, soft drive and sine shaper) through a four-pole ladder low-pass filter with its
//! own envelope, on the shared 16-voice `Synth` scheduler, complete for the poly mode the song uses.
//!
//! Independent behavioural implementation written from observed behaviour (research/glorp-devices.md §ID 28); no
//! original binary, table or pseudocode is included. f32 throughout, operation order follows the reference because
//! several steps (the exponent-bit power, the fold's integer test, the phase accumulators) are sensitive to rounding.
//! Fixed 44.1 kHz.
//!
//! # Parameters (all `0..=1`)
//!
//! Latched **at note-on** (a sounding note keeps them, the next note uses the new value): P0 waveform (`trunc(2·P0)`: 0 =
//! sine-fold, otherwise saw), P1 phase-warp exponent `1.8·P1 + 0.1`, P2 phase stretch `P2 + 1`, P3 fold gain `10·P3 + 1`,
//! P4 triangle fold (`> 0.5`), P5 power curve exponent `2·P5`, P6 sine shaper (`> 0.001`, gain `20·P6 + 1.3`), P7 drive
//! `k = 16·P7 + 1`, P8 stretch gate (`> 0.5`), P9..P12 amplitude attack / decay / sustain / release, P13 level (divided by
//! the voice count), P14 voice count `trunc(31·P14) + 1` (1..=32), P16 stereo spread (0 for a single voice), P18..P21 filter
//! attack / decay / sustain / release. **Live** (read every sample, automation acts on sounding notes): P15 detune, P17
//! spread mode, P22 cutoff, P23 resonance, P24 envelope amount, P25 key tracking.
//!
//! # Voice
//!
//! Every note restarts all oscillator phases at 0 (no random start phase) and zeroes the ladder state; the two
//! envelopes are the shared exponential `Adsr` of ID 21 / ID 22 (the stored level survives from note to note). The
//! voice is freed on the sample its amplitude envelope reaches stage 0, **and still renders the rest of the sub-block it
//! freed itself in** (silent oscillators, the filter rings out), unlike ID 22.
//!
//! Per sample, oscillator `i < N`, phase `φᵢ` in cycles (an f32 accumulator wrapped by 1024):
//!
//! ```text
//! fᵢ  = f0 · ratio_mode(P17, i)      f0 = 440·2^(note/12 − 5.75)
//! incᵢ = 2^((69 + 12·log2(fᵢ/440) + P15·(i·2/max(N−1, 1) − 1))/12 − 5.75)·440/44100     (so one voice is detuned DOWN by P15)
//! u   = max(frac φᵢ, 1e-10)          t = powbits(u, 1.8·P1 + 0.1)·(P2 + 1)
//! sine-fold:  y = vel·tsin(frac t); z = frac(g·y) (g = 10·P3 + 1), [P4: 1 − z when floor(g·y) is odd]; y = t > ½ ? −z : z
//! saw:        y = vel·(t ≤ ½ ? frac(2t) : −frac(−2t))
//! y   = sign(y)·powbits(|y|, 2·P5);  y = T(k·y)/T(k) (T(x) = x(x² + 27)/(9x² + 27));  [P6: y = tsin((20·P6 + 1.3)·y)];  [P8 ∧ t > 1: y = 1e-4]
//! acc += (env·y)·(gAᵢ, gBᵢ)         gA = 1 − s + 2sD·i, gB = 1 − s + s(2 − 2D·i),  D = 1/max(N − 1, 1)
//! ```
//!
//! `powbits(x, e)` is the reference's cheap power: `x`'s bits as an integer, `(bits − 0x3F800000)·e + 0x3F800000`,
//! reinterpreted. `tsin` is the 512-node table sine of the other Glorp devices (`id34::sin_with`). The mixed pair `acc ·
//! P13/N` runs through the ladder: a four-pole zero-delay-feedback low-pass (`k = 3.88·(0.9·P23² + 0.1)`, cutoff
//! `(20 + 19980·P22²)·2^(P25·(note − 60)/12)·2^(env·(384·P24 − 192)/12)` clamped to 20..20000 Hz twice, `tan` by the Padé
//! `w(1 − w²/15)/(1 − 0.4w²)`), left lane to the first output.
//!
//! # What this crate refuses
//!
//! * Any parameter not finite or outside `[0, 1]` ([`Error::Range`]): every vector inside `[0, 1]^26` is accepted; the
//!   ladder gain `k ≤ 3.88` is below 4, and the tested coverage (corners, midpoint, sampled vectors) renders finite. Nothing
//!   else is refused, so automation lanes are accepted; finiteness is tested, not proven by the sampling.
//! * mono/legato, slide (the original ignores it), other sample rates, notes above 127, offsets above `i32::MAX`.
//!   The song chunk has no field for a voice mode, so the adapter refuses any chunk longer than 26 parameters + size.
//!
//! **Standalone poly-only API.** As for the other instruments nothing here can detect a mono/legato description: a
//! loader MUST reject it before constructing this device.
//!
//! Exactness: `+ − × /`, the table sine and the integer conversions are IEEE-exact; `2^x` and `ln` are the f64 `libm`
//! results rounded to f32 (`2^x` also sets the oscillator increments and the cutoff, a last-bit difference from the original's
//! C library is the suspected, not isolated, source of the residue) and the envelope's `exp` likewise: **near bit parity**, measured 99.29–99.93 %
//! of the whole-song samples bit-identical, worst 2.4e-7 absolute (research doc §ID 28).
//! Not a `synth_core::Device`.

use crate::id21::Adsr;
use crate::id25::sine_table;
use crate::id34::{sin_with, TABLE_LEN};
use crate::synth::{ActiveNote, Synth, Voice as SynthVoice};
use synth_core::denormal::Guard;

/// Parameter count of the device.
pub const PARAMS: usize = 26;
/// Oscillators per voice (P14 = 1).
pub const MAX_UNISON: usize = 32;
pub use crate::synth::{MAX_NOTE, QUEUE, VOICES};

/// Why a call is not supported. The device is left unchanged when one is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parameter index ≥ 26.
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

// f32 literals of the reference (bit patterns matter).
const TWELFTH: f32 = f32::from_bits(0x3daa_aaab); // 1/12
const OCT_OFFSET: f32 = f32::from_bits(0xc0b8_0000); // −5.75
const FIFTH_OFFSET: f32 = f32::from_bits(0xc0a5_5555); // −5.1666665 (= −5.75 + 7/12)
const INV_440: f32 = f32::from_bits(0x3b14_f209); // 1/440
const INC_SCALE: f32 = f32::from_bits(0x3c23_77ee); // 440/44100
const SEMI: f32 = f32::from_bits(0x418a_7fac); // 12/ln 2
const INV_127: f32 = f32::from_bits(0x3c01_0204);
const C899: f32 = f32::from_bits(0x410f_d70a); // 8.99
const R09: f32 = f32::from_bits(0x3f66_6666); // 0.9
const C01: f32 = f32::from_bits(0x3dcc_cccd); // 0.1
const K388: f32 = f32::from_bits(0x4078_51ec); // 3.88
const C19980: f32 = f32::from_bits(0x469c_1800);
const C20000: f32 = f32::from_bits(0x469c_4000);
const PI_OVER_SR: f32 = f32::from_bits(0x3895_6592);
const W30: f32 = f32::from_bits(0x3b0c_0f39); // π·30/44100
const PADE_A: f32 = f32::from_bits(0xbd88_8889); // −1/15
const PADE_B: f32 = f32::from_bits(0xbecc_cccd); // −0.4
const EPS4: f32 = f32::from_bits(0x38d1_b717); // 1e-4
const EPS10: f32 = f32::from_bits(0x2edb_e6ff); // 1e-10
const TAU: f32 = f32::from_bits(0x40c9_0fdb);
const PI: f32 = f32::from_bits(0x4049_0fdb);
const BIT_HALF: f32 = f32::from_bits(0xce7e_0000); // −0x3F800000 as f32
const BIT_ONE: f32 = f32::from_bits(0x4e7e_0000); // 0x3F800000 as f32
const P23M: f32 = f32::from_bits(0x3400_0000); // 2^-23
/// Ladder output weights of the reference's mode 1 (`[input, y1, y2, y3, y4]`): the fourth pole.
const LADDER_W: [f32; 5] = [0.0, 0.0, 0.0, 0.0, 1.0];

fn pow2(x: f32) -> f32 {
    libm::exp2(x as f64) as f32
}

fn lnf(x: f32) -> f32 {
    libm::log(x as f64) as f32
}

/// `cvttss2si`: truncation, `i32::MIN` for NaN / out of range.
fn cvtt(x: f32) -> i32 {
    if x >= -2_147_483_648.0 && x < 2_147_483_648.0 { x as i32 } else { i32::MIN }
}

/// `floor` as the reference does it: truncate, subtract one when the truncation rounded up.
fn floor_f(x: f32) -> f32 {
    let tr = cvtt(x) as f32;
    if x < tr { tr + -1.0 } else { tr }
}

/// The reference's cheap power (see the module doc).
fn pow_bits(x: f32, e: f32) -> f32 {
    let a = ((x.to_bits() as i32) as f32 + BIT_HALF) * e + BIT_ONE;
    f32::from_bits(cvtt(a) as u32)
}

/// Table lookup of a number `q` in `[1, 2)` that carries the phase in its mantissa (top 9 bits index, low 14 bits
/// interpolate).
fn lookup(table: &[f32; TABLE_LEN], q: f32) -> f32 {
    let bits = q.to_bits();
    let i = ((bits >> 14) & 0x1ff) as usize;
    let w = (((bits & 0x3fff) << 9) as i32) as f32 * P23M;
    let (a, b) = (table[i], table[i + 1]);
    (b - a) * w + a
}

/// Whether `new` accepts these parameters: every vector inside the unit cube.
pub fn check(p: &[f32; PARAMS]) -> Result<(), Error> {
    if p.iter().all(|v| (0.0..=1.0).contains(v)) { Ok(()) } else { Err(Error::Range) }
}

/// Oscillators per voice for P14: `trunc(31·P14) + 1`, 1..=32.
pub(crate) fn unison_count(p14: f32) -> usize {
    let c = cvtt(p14 * 31.0);
    if c < 0 {
        return 1;
    }
    let m = ((c + 1) as f32).min(32.0);
    cvtt(m) as usize
}

/// Fundamental of a note, `440·2^(note/12 − 5.75)`.
pub(crate) fn note_freq(note: u8) -> f32 {
    pow2((note as f32) * TWELFTH + OCT_OFFSET) * 440.0
}

/// Left / right gains of every oscillator: linear, constant sum 2 (`gA = 1 − s + 2sD·i`, `gB = 1 − s + s(2 − 2D·i)`);
/// a single voice ignores the spread.
pub(crate) fn pan_gains(n: usize, spread: f32) -> [[f32; 2]; MAX_UNISON] {
    let s = if n == 1 { 0.0 } else { spread };
    let d = 1.0 / ((n.max(1) - 1) as f32).max(1.0);
    let d2 = d + d;
    let sd = (s + s) * d;
    let base = 1.0 - s;
    let mut g = [[1.0f32; 2]; MAX_UNISON];
    for (i, gi) in g.iter_mut().enumerate().take(n) {
        let fi = i as f32;
        *gi = [fi * sd + base, s * (2.0 - d2 * fi) + base];
    }
    g
}

/// Frequency of oscillator `i` for the spread mode `m = trunc(8.99·P17) − 1` (anything outside 0..=7: all equal).
pub(crate) fn partial_freq(p17: f32, f0: f32, note: u8, i: usize) -> f32 {
    let m = cvtt(p17 * C899).wrapping_sub(1);
    let fi = i as f32;
    match m as u32 {
        0 => if i % 2 == 0 { f0 } else { f0 + f0 },
        1 => match i % 3 {
            0 => f0,
            1 => pow2((note as f32) * TWELFTH + FIFTH_OFFSET) * 440.0,
            _ => f0 + f0,
        },
        2 => if i % 2 == 0 { f0 } else { f0 * 0.5 },
        3 => f0 * (fi + 1.0),
        4 => f0 / (fi + 1.0),
        5 => f0 / (fi * 0.5 + 1.0),
        6 => f0 * (fi * 0.5 + 1.0),
        7 => {
            let x = fi + fi;
            f0 * if x > 1.0 { x } else { 1.0 }
        }
        _ => f0,
    }
}

/// Phase increment (cycles per sample) of oscillator `i` of `n` at frequency `fi` with detune `p15`: symmetric
/// `±P15` semitones, and a lone voice sits at `−P15`.
pub(crate) fn increment(fi: f32, i: usize, n: usize, p15: f32) -> f32 {
    let d = 1.0 / ((n.max(1) - 1) as f32).max(1.0);
    let d2 = d + d;
    let l = lnf(fi * INV_440) * SEMI;
    let mut x = (i as f32) * d2;
    x += -1.0;
    x *= p15;
    x += 69.0;
    x += l;
    x *= TWELFTH;
    x += OCT_OFFSET;
    pow2(x) * INC_SCALE
}

/// `clamp(x, 20, 20000)` as the reference does it (a NaN becomes 20000).
fn clamp_hz(x: f32) -> f32 {
    let m = if x < C20000 { x } else { C20000 };
    if x < 20.0 { 20.0 } else { m }
}

#[derive(Clone, Copy, Debug)]
struct Voice {
    active: bool,
    note: u8,
    // latched at note-on
    n: usize,
    f0: f32,
    wave: i32,
    warp: f32,
    stretch: f32,
    drive: f32,
    inv_t: f32,
    fold: f32,
    power: f32,
    shaper_gain: f32,
    vel: f32,
    level: f32,
    tri: bool,
    shape: bool,
    gate: bool,
    pan: [[f32; 2]; MAX_UNISON],
    // dynamic
    phase: [f32; MAX_UNISON],
    inc: [f32; MAX_UNISON],
    /// `(P15 bits, P17 bits)` the increments were computed for.
    inc_key: Option<(u32, u32)>,
    /// `(P25 bits, 2^(P25/12·(note − 60)))`.
    key_track: Option<(u32, f32)>,
    amp: Adsr,
    flt: Adsr,
    /// Four poles, left / right.
    state: [[f32; 2]; 4],
}

impl Voice {
    fn idle() -> Self {
        Voice {
            active: false,
            note: 0,
            n: 1,
            f0: 0.0,
            wave: 0,
            warp: 0.0,
            stretch: 0.0,
            drive: 0.0,
            inv_t: 0.0,
            fold: 0.0,
            power: 0.0,
            shaper_gain: 0.0,
            vel: 0.0,
            level: 0.0,
            tri: false,
            shape: false,
            gate: false,
            pan: [[0.0; 2]; MAX_UNISON],
            phase: [0.0; MAX_UNISON],
            inc: [0.0; MAX_UNISON],
            inc_key: None,
            key_track: None,
            amp: Adsr::idle(),
            flt: Adsr::idle(),
            state: [[0.0; 2]; 4],
        }
    }

    /// Note-on: oscillator phases and ladder state to 0, both envelopes restarted (stored levels kept), parameters latched.
    fn note_start(&mut self, note: u8, velocity: u8, p: &[f32; PARAMS]) {
        self.amp.restart(p[9], p[10], p[11], p[12]);
        self.flt.restart(p[18], p[19], p[20], p[21]);
        self.state = [[0.0; 2]; 4];
        self.phase = [0.0; MAX_UNISON];
        self.note = note;
        self.f0 = note_freq(note);
        let n = unison_count(p[14]);
        self.n = n;
        self.wave = cvtt(p[0] + p[0]);
        self.warp = p[1] * 1.8 + C01;
        self.stretch = p[2] + 1.0;
        let k = p[7] * 16.0 + 1.0;
        self.drive = k;
        self.fold = p[3] * 10.0 + 1.0;
        self.shaper_gain = p[6] * 20.0 + 1.3;
        self.power = p[5] + p[5];
        self.inv_t = if k > EPS4 {
            let k2 = k * k;
            (k2 * 9.0 + 27.0) / ((k2 + 27.0) * k)
        } else {
            1.0
        };
        self.vel = (velocity as f32) * INV_127;
        self.level = p[13] / (n as f32);
        self.tri = 0.5 < p[4];
        self.shape = 0.001 < p[6];
        self.gate = 0.5 < p[8];
        self.pan = pan_gains(n, p[16]);
        self.inc_key = None;
        self.key_track = None;
        self.active = true;
    }

    /// One oscillator sample at phase `phase` (cycles).
    fn osc(&self, table: &[f32; TABLE_LEN], phase: f32) -> f32 {
        let tr = cvtt(phase) as f32;
        let mut u = phase - tr;
        if u < 0.0 {
            u += 1.0;
        }
        let u = if u > EPS10 { u } else { EPS10 };
        let t = pow_bits(u, self.warp) * self.stretch;
        let mut y;
        if self.wave == 0 {
            let q = (t + 1.0) - floor_f(t);
            y = lookup(table, q) * self.vel;
            let tau_t = t * TAU;
            let y2 = y * self.fold;
            let tr = cvtt(y2) as f32;
            let mut fr = y2 - tr;
            if fr < 0.0 {
                fr += 1.0;
            }
            if self.tri {
                let fl = if y2 < tr { tr + -1.0 } else { tr };
                let h = fl * 0.5;
                let htr = cvtt(h) as f32;
                let hfl = if h < htr { htr + -1.0 } else { htr };
                if fl - (hfl + hfl) > 0.5 {
                    fr = 1.0 - fr;
                }
            }
            y = if PI < tau_t { -fr } else { fr };
        } else {
            let mut f;
            if t > 0.5 {
                let x = t * -2.0;
                f = x - cvtt(x) as f32;
                if f < 0.0 {
                    f += 1.0;
                }
                f = -f;
            } else {
                let x = t + t;
                f = x - cvtt(x) as f32;
                if f < 0.0 {
                    f += 1.0;
                }
            }
            y = f * self.vel;
        }
        // power curve
        let a = f32::from_bits(y.to_bits() & 0x7fff_ffff);
        let a = if a > EPS10 { a } else { EPS10 };
        let pw = pow_bits(a, self.power);
        y = if y < 0.0 { -pw } else { pw };
        // soft drive
        if self.drive > EPS4 {
            let ky = self.drive * y;
            let ky2 = ky * ky;
            y = (((ky2 + 27.0) * ky) * self.inv_t) / (ky2 * 9.0 + 27.0);
        }
        if self.shape {
            y = sin_with(table, y * self.shaper_gain);
        }
        let gated = if t > 1.0 { EPS4 } else { y };
        if self.gate { gated } else { y }
    }

    /// Recomputes the oscillator increments when P15 / P17 moved.
    fn refresh_increments(&mut self, p: &[f32; PARAMS]) {
        let key = (p[15].to_bits(), p[17].to_bits());
        if self.inc_key == Some(key) {
            return;
        }
        for i in 0..self.n {
            let fi = partial_freq(p[17], self.f0, self.note, i);
            self.inc[i] = increment(fi, i, self.n, p[15]);
        }
        self.inc_key = Some(key);
    }

    /// Renders `l.len()` samples, adding into `l` / `r`.
    fn render_samples(&mut self, sh: &Shared<'_>, l: &mut [f32], r: &mut [f32]) {
        let (p, table) = (sh.p, sh.table);
        for i in 0..l.len() {
            self.refresh_increments(p);
            let ea = self.amp.tick();
            let ef = self.flt.tick();
            // cutoff, resonance, ladder coefficients
            let kt = match self.key_track {
                Some((b, v)) if b == p[25].to_bits() => v,
                _ => {
                    let v = pow2((p[25] * TWELFTH) * ((self.note as f32) + -60.0));
                    self.key_track = Some((p[25].to_bits(), v));
                    v
                }
            };
            let base = (p[22] * p[22]) * C19980 + 20.0;
            let fc1 = clamp_hz(kt * base);
            let depth = p[24] * 384.0 + -192.0;
            let fc = clamp_hz(pow2((ef * TWELFTH) * depth) * fc1);
            let r_ = (p[23] * p[23]) * R09 + C01;
            let k = r_ * K388;
            let m = if fc < C20000 { fc } else { C20000 };
            let w = if fc < 30.0 { W30 } else { m * PI_OVER_SR };
            let w2 = w * w;
            let g = ((w2 * PADE_A + 1.0) * w) / (w2 * PADE_B + 1.0);
            let ig = 1.0 / (g + 1.0);
            let gg = g * ig;
            let g2 = gg * gg;
            let g3 = g2 * gg;
            let beta = [ig * g3, (g2 * ig), gg * ig, ig];
            let c = 1.0 / ((gg * k) * g3 + 1.0);
            // oscillators
            let (mut acc_l, mut acc_r) = (0.0f32, 0.0f32);
            for j in 0..self.n {
                let y = self.osc(table, self.phase[j]);
                let ye = y * ea;
                acc_l += ye * self.pan[j][0];
                acc_r += ye * self.pan[j][1];
                let ph = self.phase[j] + self.inc[j];
                self.phase[j] = if ph < 1024.0 { ph } else { ph + -1024.0 };
            }
            let x = [self.level * acc_l, self.level * acc_r];
            // ladder, both lanes
            let mut out = [0.0f32; 2];
            for lane in 0..2 {
                let mut s = 0.0f32;
                for j in 0..4 {
                    s += beta[j] * self.state[j][lane];
                }
                let u = c * (x[lane] - k * s);
                let mut ys = [u, 0.0, 0.0, 0.0, 0.0];
                let mut prev = u;
                for j in 0..4 {
                    let st = self.state[j][lane];
                    let v = gg * (prev - st);
                    let y = st + v;
                    self.state[j][lane] = v + y;
                    ys[j + 1] = y;
                    prev = y;
                }
                let mut o = 0.0f32;
                for j in 0..5 {
                    o += LADDER_W[j] * ys[j];
                }
                out[lane] = o;
            }
            l[i] = out[0] + l[i];
            r[i] = out[1] + r[i];
            if self.amp.stage == 0 {
                self.active = false;
            }
        }
    }
}

/// Read-only state the voices share.
struct Shared<'a> {
    p: &'a [f32; PARAMS],
    table: &'a [f32; TABLE_LEN],
}

impl SynthVoice<Shared<'_>> for Voice {
    fn active(&self) -> bool {
        self.active
    }
    fn note(&self) -> u8 {
        self.note
    }
    fn start(&mut self, note: u8, c: &Shared<'_>) {
        self.note_start(note, 127, c.p);
    }
    fn start_vel(&mut self, note: u8, velocity: u8, c: &Shared<'_>) {
        self.note_start(note, velocity, c.p);
    }
    fn release(&mut self) {
        self.amp.release();
        self.flt.release();
    }
    fn render(&mut self, c: &Shared<'_>, l: &mut [f32], r: &mut [f32]) {
        self.render_samples(c, l, r);
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

/// The ID 28 instrument: 16 voices of up to 32 oscillators on the shared scheduler, 26 parameters.
#[derive(Clone, Debug)]
pub struct Id28 {
    p: [f32; PARAMS],
    table: [f32; TABLE_LEN],
    synth: Synth<Voice>,
}

impl Id28 {
    /// Builds the device from the 26 normalized parameters (a song chunk's first 26 floats). No voice is playing.
    pub fn new(p: &[f32; PARAMS]) -> Result<Self, Error> {
        check(p)?;
        Ok(Id28 { p: *p, table: sine_table(), synth: Synth::new([Voice::idle(); VOICES]) })
    }

    /// Sets one parameter between `process` calls. Sounding voices use the live parameters (P15, P17, P22..P25) from the
    /// next sample and keep the latched ones; the next note-on uses the new values. On `Err` nothing changed.
    pub fn set_param(&mut self, index: usize, value: f32) -> Result<(), Error> {
        if index >= PARAMS {
            return Err(Error::Index);
        }
        let mut next = self.p;
        next[index] = value;
        check(&next)?;
        self.p = next;
        Ok(())
    }

    /// Queues a note-on with a velocity (0..=255, scales the oscillator by `velocity/127`) `offset` samples after the
    /// start of the next `process` call. With every voice busy at the time it fires, it is lost.
    pub fn note_on(&mut self, note: u8, velocity: u8, offset: u32) -> Result<(), Error> {
        self.synth.note_on_vel(note, velocity, offset).map_err(Error::from)
    }

    /// Queues a note-off (releases every voice currently playing `note`).
    pub fn note_off(&mut self, note: u8, offset: u32) -> Result<(), Error> {
        self.synth.note_off(note, offset).map_err(Error::from)
    }

    /// Voices currently holding a slot (playing or releasing).
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
        let sh = Shared { p: &self.p, table: &self.table };
        self.synth.process(&sh, l, r);
    }
}
