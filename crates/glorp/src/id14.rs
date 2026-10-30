//! Glorp instrument ID 14: a noise-excited modal resonator bank (up to 256 band-pass resonators per voice, tuned to a
//! harmonic series with inharmonicity and damping that grows with frequency), on the shared 16-voice `Synth` scheduler,
//! complete for the poly **and mono** voice modes (the song's two instances are mono).
//!
//! Independent behavioural implementation written from observed behaviour (research/glorp-devices.md §ID 14); no
//! original binary, table or pseudocode is included. f32 throughout the voice, operation order follows the reference
//! because the bank sums hundreds of high-Q resonators. Fixed 44.1 kHz.
//!
//! # Parameters (all `0..=1`, **wire indices** = `set_param` order)
//!
//! Latched **at note-on**: 1 mode count `N = trunc(255.999·v) + 1` (1..=256), 2 inharmonicity, 3 / 4 damping, 7 octave
//! selector `trunc(5.99·v)` (×1/8 … ×4), 8 seed `trunc(1024.999·v)`, 9 harmonic range `R = trunc(63.99·v) + 1`, 11 random
//! harmonic selection (`v > 0.5`), 15 tone filter, 19 band-stop width, 20 envelope time, 25 mode dropout. **Live** (read
//! every sample): 0 hold, 10 stereo width, 13 noise level, 14 noise decay rate `50·v³ + 2`, 16 crackle depth, 17 crackle
//! rate (read when a crackle ends), 18 band-passed noise gain, 21 crackle mix, 22 output gain `(0.4·v)²` (read per
//! call). 23 is the voice mode (`trunc(v) = 1` mono; a change cuts every voice and empties the event queue and the note
//! stack), 24 is accepted and ignored, and so are 5, 6 and 12 (stored, never read).
//!
//! # Voice
//!
//! `f0 = 440·2^(note/12 − 5.75)`. Mode `k < N` (a double `t = k` counter) has harmonic `h`: deterministic
//! `trunc(k·0.999·R/(N−1))`, or with random selection `trunc(0.999·u·R)` for the next draw `u` of a small
//! generator shared by the whole device (see below). Its frequency is `f0·oct·(h+1)·(h·s + 1)` with
//! `s = (2·P2)³` below 0.5, `0.06·P2 − 0.09` from 0.5 (all in f64); modes outside `20..=22000` Hz are skipped (so are all
//! modes when `N = 1` in deterministic selection: the index is the integer indefinite value). A mode's Q is
//! `1000/(1 + 4.4e7·P3⁹ + 5e4·P4⁹·(f/f0)²)`. Modes are band-pass resonators with skirt gain `Q`.
//!
//! **The shared generator.** The original draws harmonics from one global linear congruence
//! (`g ← g·0x015A4E35`, value `((g mod 255) / 255)` with C remainder) that every note re-seeds with
//! `255·seed + 0x12F092`, advances once per mode (whatever the mode did), restores at the end and so leaves no trace; a
//! mode is additionally offset by `0xBC769C` when a hash of `(k + 1 + g)` falls below `1 − P25/(k + 2)`. The note itself
//! never enters these draws.
//!
//! Per sample: a stereo noise pair `(B, A)` from the voice's hash chain (seeded by the note), mixed by the width, goes
//! through three filters: a low-pass at `2·f0 − w` (Q 4) and a high-pass at `2·f0 + w` (Q 4), `w = 6400·P19`, whose summed
//! output is *subtracted* from the noise (a band around `2·f0`), and a tone filter (Q 0.2) whose output is scaled by the
//! noise envelope `e^(−t·rate)·P13` and the crackle `P16·e^(−60·timer) + 1 − P16`. The band-passed noise is scaled by the
//! exponential `Adsr` (shared with ID 21 / ID 22: time scale `3·P20 + 0.003`, A ×0.5, D ×0.6, S 0.02, R ×0.3 and the
//! stored level survives from note to note) times `3·P18`; both halves are mixed (`(1 − P21) + P21·e^(−60·timer)`), halved
//! and sent through every resonator; the voice output is the mean of the resonators times `(0.4·P22)²`. A resonator whose
//! level `0.9995·level + 0.0005·(|l| + |r|)` (updated only at **the fifth sample of each scheduler sub-block**, start 1.0; a `process` call is cut into sub-blocks at every pending event) is below
//! 0.001 stops. The voice is freed at the end of the call where `t > 2.5 s`, or `t > 0.5 s` and the largest output of
//! the call was below 0.001. Everything starts fresh at note-on (no state of a previous note survives except the
//! envelope's stored level).
//!
//! Not a `synth_core::Device`.

use crate::id21::Adsr;
use crate::id25::{cosf, expf, hash, sinf, Biquad, DT, P31, W_RBJ};
use crate::synth::{ActiveNote, Synth, Voice as SynthVoice};
use synth_core::denormal::Guard;

/// Parameter count of the device (wire indices `0..26`; 24 is accepted and ignored).
pub const PARAMS: usize = 26;
/// Resonators per voice (parameter 1 at 1.0).
pub const MAX_MODES: usize = 256;
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
const C255999: f32 = f32::from_bits(0x437f_ffbe); // 255.999
const C599: f32 = f32::from_bits(0x40bf_ae14); // 5.99
const C1025: f32 = f32::from_bits(0x4480_1ff8); // 1024.999
const C6399: f32 = f32::from_bits(0x427f_f5c3); // 63.99
const C0999: f32 = f32::from_bits(0x3f7f_be77); // 0.999
const C0003: f32 = f32::from_bits(0x3b44_9ba6); // 0.003
const C03: f32 = f32::from_bits(0x3e99_999a); // 0.3
const C06: f32 = f32::from_bits(0x3f19_999a); // 0.6
const C002: f32 = f32::from_bits(0x3ca3_d70a); // 0.02, the sustain level
const C02: f32 = f32::from_bits(0x3e4c_cccd); // 0.2, the tone filter Q
const C04: f32 = f32::from_bits(0x3ecc_cccd); // 0.4
const C005: f32 = f32::from_bits(0x3d4c_cccd); // 0.05, Q floor
const C006: f32 = f32::from_bits(0x3d75_c28f); // 0.06
const CM009: f32 = f32::from_bits(0xbdb8_51eb); // −0.09
const C0001: f32 = f32::from_bits(0x3a83_126f); // 0.001
const C09995: f32 = f32::from_bits(0x3f7f_df3b); // 0.9995
const C00005: f32 = f32::from_bits(0x3a03_1400); // 0.00050002337
const C12P32: f32 = f32::from_bits(0x2f99_999a); // 1.2 / 2^32
const RCP255: f32 = f32::from_bits(0x3b80_8081); // 1/255
/// The voice clock step as the f64 the original adds to `t`.
const DT64: f64 = DT as f64;
const OCT: [f64; 6] = [0.125, 0.25, 0.5, 1.0, 2.0, 4.0];
/// The global generator's value when a note-on starts (it is restored at the end of every note-on).
const G_REST: i32 = 1;

fn minss(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

fn maxss(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

/// `cvttss2si`: truncation, `i32::MIN` for NaN / out of range.
fn cvtt(x: f32) -> i32 {
    if x >= -2_147_483_648.0 && x < 2_147_483_648.0 { x as i32 } else { i32::MIN }
}

/// `cvttsd2si`.
fn cvttd(x: f64) -> i32 {
    if x >= -2_147_483_648.0 && x < 2_147_483_648.0 { x as i32 } else { i32::MIN }
}

fn cube(x: f32) -> f32 {
    libm::pow(x as f64, 3.0) as f32
}

/// Whether `new` accepts these parameters: every vector inside the unit cube.
pub fn check(p: &[f32; PARAMS]) -> Result<(), Error> {
    if p.iter().all(|v| (0.0..=1.0).contains(v)) { Ok(()) } else { Err(Error::Range) }
}

/// RBJ biquad `[b0, b1, b2, a1, a2] / a0` of type 0 low-pass, 1 high-pass or 5 band-pass (skirt gain = Q), in the
/// reference's operation order: frequency clamped to `20..=22000`, Q floored at 0.05.
pub(crate) fn design(kind: u8, freq: f32, q: f32) -> [f32; 5] {
    let w = maxss(minss(freq, 22000.0), 20.0) * W_RBJ;
    let (sn, cs) = (sinf(w), cosf(w));
    let qm = maxss(q, C005);
    let alpha = sn / (qm + qm);
    let (b, a1n) = match kind {
        0 => {
            let b0 = 0.5 * (1.0 - cs);
            ([b0, 1.0 - cs, b0], -2.0 * cs)
        }
        1 => {
            let b0 = cs * 0.5 + 0.5;
            ([b0, -1.0 - cs, b0], cs * -2.0)
        }
        5 => ([qm * alpha, 0.0, (-qm) * alpha], -2.0 * cs),
        _ => unreachable!("only the three filter types the voice uses"),
    };
    let a0 = alpha + 1.0;
    let a2n = 1.0 - alpha;
    let r = 1.0 / a0;
    [r * b[0], r * b[1], r * b[2], r * a1n, a2n / a0]
}

fn biquad(c: [f32; 5]) -> Biquad {
    let mut b = Biquad::default();
    b.c = c;
    b
}

/// The global generator: `g ← g·0x015A4E35`, value `(g mod 255)/255` (C remainder of the signed value).
fn rand_step(g: u32) -> (u32, f32) {
    let s = g.wrapping_mul(0x015a_4e35);
    (s, ((s as i32) % 255) as f32 * RCP255)
}

/// The same generator applied to `seed + g` without changing `g`.
fn hash_rand(seed: i32, g: u32) -> f32 {
    rand_step((seed as u32).wrapping_add(g)).1
}

/// The resonator set of one note: `count` modes with frequency and Q (outside `20..=22000` skipped).
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    pub count: usize,
    pub freq: [f32; MAX_MODES],
    pub q: [f32; MAX_MODES],
}

/// Mode frequencies and Q for `note` under the parameters `p` (what a note-on builds).
pub fn plan(p: &[f32; PARAMS], note: u8) -> Plan {
    let mut out = Plan { count: 0, freq: [0.0; MAX_MODES], q: [0.0; MAX_MODES] };
    let f0 = libm::exp2(note as f64 * 0.083_333_333_333_333_33 + -5.75) * 440.0;
    let n = cvtt(p[1] * C255999) + 1;
    let oct = OCT[(cvtt(p[7] * C599).max(0) as usize).min(5)];
    let range = cvtt(p[9] * C6399) + 1;
    let seed = cvtt(p[8] * C1025);
    let s6 = {
        let v = p[2];
        (if v < 0.5 { cube(v + v) } else { v * C006 + CM009 }) as f64
    };
    let random = p[11] > 0.5;
    let (d10, d11) = (p[3] as f64, p[4] as f64);
    let drop = p[25] as f64;
    let recip = 1.0 / ((n - 1) as f64);
    let mut g = ((seed as u32) << 8).wrapping_sub(seed as u32).wrapping_add(0x12_f092);
    for k in 0..n {
        let gk = g;
        let r = hash_rand(k + G_REST, gk);
        let thr = drop / (-2.0 - k as f64) + 1.0;
        let gt = if thr > r as f64 { gk.wrapping_add(0xbc_769c) } else { gk };
        let h = if random {
            let v = rand_step(gt).1;
            cvtt((v * C0999) * range as f32)
        } else {
            cvttd(((k as f64) * C0999 as f64) * (range as f32 as f64) * recip)
        };
        g = gk.wrapping_mul(0x015a_4e35);
        let hp1 = h.wrapping_add(1);
        let fm = f0 * oct * (hp1 as f64) * ((h as f64) * s6 + 1.0);
        if fm > 22000.0 || !(fm >= 20.0) {
            continue;
        }
        let ratio = ((fm * fm) / (f0 * f0)) as f32;
        let ratio = maxss(ratio, 0.0) as f64;
        // the reference multiplies in this order; the grouping matters in the last bit
        let lo = (d10 * d10) * (d10 * d10);
        let lo = (lo * lo) * (d10 * 44_000_000.0);
        let e = d11 * d11;
        let e4 = (e * e) * (e * e);
        let hi = ((d11 * 50_000.0) * e4) * ratio;
        let q = (1000.0 / (hi + (lo + 1.0))) as f32;
        let i = out.count;
        out.freq[i] = fm as f32;
        out.q[i] = q;
        out.count += 1;
    }
    out
}

#[derive(Clone, Copy, Debug)]
struct Voice {
    active: bool,
    note: u8,
    /// Seconds since note-on (f64 sum of the f32 step), the crackle timer and its threshold (f32).
    t: f64,
    timer: f32,
    thresh: f32,
    rng: u32,
    adsr: Adsr,
    /// Largest output magnitude of the current call (pre-gain).
    meter: f32,
    count: usize,
    /// Band-stop low-pass / high-pass and the tone filter.
    lp: Biquad,
    hp: Biquad,
    tone: Biquad,
    modes: [Biquad; MAX_MODES],
    level: [f32; MAX_MODES],
}

impl Voice {
    fn idle() -> Self {
        Voice {
            active: false,
            note: 0,
            t: 0.0,
            timer: 0.0,
            thresh: 0.0,
            rng: 0,
            adsr: Adsr::idle(),
            meter: 0.0,
            count: 0,
            lp: Biquad::default(),
            hp: Biquad::default(),
            tone: Biquad::default(),
            modes: [Biquad::default(); MAX_MODES],
            level: [0.0; MAX_MODES],
        }
    }

    /// Note-on: builds the three filters, the envelope times and the resonator bank; the generator state is the note.
    fn note_start(&mut self, note: u8, p: &[f32; PARAMS]) {
        let f0 = libm::exp2(note as f64 * 0.083_333_333_333_333_33 + -5.75) * 440.0;
        self.active = true;
        self.note = note;
        self.t = 0.0;
        self.timer = 0.0;
        self.thresh = 0.0;
        self.meter = 0.0;
        self.rng = (note as u32).wrapping_add(0x1f11_a9a2);
        // envelope: x = 3·P20 + 0.003; A 0.5x, D 0.6x, S 0.02, R 0.3x; the stored level is kept
        let x = p[20] * 3.0 + C0003;
        self.adsr.restart(x * 0.5, x * C06, C002, x * C03);
        // band-stop pair around 2·f0
        let wd = p[19] * 6400.0;
        let f2 = (f0 + f0) as f32;
        let hi = minss(f2 + wd, 20000.0);
        let lo = maxss(f2 - wd, 20.0);
        self.lp = biquad(design(0, lo, 4.0));
        self.hp = biquad(design(1, hi, 4.0));
        // tone filter: low-pass at v·40000 + f0 below 0.5, high-pass at f0 − 24000 + v·40000 from 0.5
        let v = p[15] as f64;
        let (kind, f) = if 0.5 <= p[15] {
            (1u8, minss(((f0 + -24000.0) + v * 40000.0) as f32, 22000.0))
        } else {
            (0u8, (v * 40000.0 + f0) as f32)
        };
        let f = if f < 20.0 { 20.0 } else { minss(21000.0, f) };
        self.tone = biquad(design(kind, f, C02));
        let pl = plan(p, note);
        self.count = pl.count;
        for i in 0..pl.count {
            self.modes[i] = biquad(design(5, pl.freq[i], pl.q[i]));
            self.level[i] = 1.0;
        }
    }

    fn render_samples(&mut self, p: &[f32; PARAMS], l: &mut [f32], r: &mut [f32]) {
        let q15 = cube(p[14]) * 50.0 + 2.0;
        let gain = {
            let x = p[22] * C04;
            x * x
        };
        for i in 0..l.len() {
            self.t += DT64;
            self.timer += DT;
            // noise pair from the voice's hash chain: two draws per sample
            let s = self.rng;
            let x = (s ^ (s >> 16)).wrapping_mul(0x7feb_352d);
            let x1 = x ^ (x >> 15);
            let ah = x1.wrapping_mul(0x846c_a68b);
            let a = (ah ^ (ah >> 16)) as f32 * P31 + -1.0;
            let y = x1.wrapping_mul(0x9c2d_0d6f);
            let y = y ^ (y >> 15);
            let bh = y.wrapping_mul(0x846c_a68b);
            self.rng = bh ^ (bh >> 16);
            let b = self.rng as f32 * P31 + -1.0;
            let sum = b + a;
            let w = p[10];
            let c3 = (0.5 - w * 0.5) * sum;
            let inp = [b * w + c3, a * w + c3];
            let tone = self.tone.tick(inp);
            let lo = self.lp.tick(inp);
            let hi = self.hp.tick(inp);
            let band = [lo[0] + hi[0], lo[1] + hi[1]];
            let env = (self.adsr.tick() * 3.0) * p[18];
            if p[0] < 0.5 && self.adsr.stage == 3 {
                self.adsr.release();
            }
            let tv = if self.timer > self.thresh {
                self.timer = 0.0;
                self.rng = hash(self.rng);
                self.thresh = (self.rng as f32 * C12P32 + C03) / (p[17] * 20.0 + 1.0);
                0.0
            } else {
                self.timer
            };
            let e60 = expf(tv * -60.0);
            let crack = p[16] * e60 + (1.0 - p[16]);
            let nenv = expf(-(self.t as f32) * q15) * p[13] * crack;
            let f = [nenv * tone[0], nenv * tone[1]];
            let (mut o0, mut o1) = (0.0f32, 0.0f32);
            if self.count > 0 {
                let x1 = (inp[1] - band[1]) * env;
                let x0 = (inp[0] - band[0]) * env;
                let g = (1.0 - p[21]) + e60 * p[21];
                let e = [(x0 * g + f[0]) * 0.5, (f[1] + g * x1) * 0.5];
                let (mut s0, mut s1) = (0.0f32, 0.0f32);
                for m in 0..self.count {
                    if 0.001 > self.level[m] {
                        continue;
                    }
                    let y = self.modes[m].tick(e);
                    if i == 4 {
                        self.level[m] = self.level[m] * C09995 + (y[0].abs() + y[1].abs()) * C00005;
                    }
                    s0 += y[0];
                    s1 += y[1];
                }
                let inv = 1.0 / self.count as f32;
                o1 = s1 * inv;
                o0 = inv * s0;
            }
            let (a1, a0) = (o1.abs(), o0.abs());
            let m = maxss(maxss(a1, a0), self.meter);
            self.meter = m;
            l[i] += o0 * gain;
            r[i] += o1 * gain;
        }
        if 2.5 < self.t || (self.t > 0.5 && !(self.meter >= C0001)) {
            self.active = false;
        }
        self.meter = 0.0;
    }
}

struct Shared<'a> {
    p: &'a [f32; PARAMS],
}

impl SynthVoice<Shared<'_>> for Voice {
    fn active(&self) -> bool {
        self.active
    }
    fn note(&self) -> u8 {
        self.note
    }
    fn start(&mut self, note: u8, c: &Shared<'_>) {
        self.note_start(note, c.p);
    }
    fn release(&mut self) {
        self.adsr.release();
    }
    fn render(&mut self, c: &Shared<'_>, l: &mut [f32], r: &mut [f32]) {
        self.render_samples(c.p, l, r);
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

/// The ID 14 instrument: 16 voices of up to 256 resonators on the shared scheduler (~240 KB: put it in a `Box` /
/// `static` / pool, and give debug-build test threads a large enough stack), 26 parameters.
#[derive(Clone, Debug)]
pub struct Id14 {
    p: [f32; PARAMS],
    synth: Synth<Voice>,
}

impl Id14 {
    /// Builds the device from the 26 normalized parameters (a song chunk's first 26 floats). No voice is playing.
    pub fn new(p: &[f32; PARAMS]) -> Result<Self, Error> {
        check(p)?;
        let mut synth = Synth::new([Voice::idle(); VOICES]);
        synth.set_mono(cvtt(p[23]) == 1, |v| v.active = false);
        Ok(Id14 { p: *p, synth })
    }

    /// Sets one parameter between `process` calls. Sounding voices use the live parameters from the next sample and keep
    /// the latched ones; the next note-on uses the new values. Parameter 23 (voice mode) cuts every voice and empties
    /// the event queue when the mode changes. On `Err` nothing changed.
    pub fn set_param(&mut self, index: usize, value: f32) -> Result<(), Error> {
        if index >= PARAMS {
            return Err(Error::Index);
        }
        if !(0.0..=1.0).contains(&value) {
            return Err(Error::Range);
        }
        self.p[index] = value;
        if index == 23 {
            self.synth.set_mono(cvtt(value) == 1, |v| v.active = false);
        }
        Ok(())
    }

    /// Queues a note-on (velocity does not exist) `offset` samples after the start of the next `process` call. Poly mode:
    /// with every voice busy at the time it fires it is lost; mono mode: see [`crate::synth::Synth::set_mono`].
    pub fn note_on(&mut self, note: u8, offset: u32) -> Result<(), Error> {
        self.synth.note_on(note, offset).map_err(Error::from)
    }

    /// Queues a note-off (poly: releases every voice playing `note`; mono: pops the note stack).
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

    /// Whether the device is in mono voice mode.
    pub fn is_mono(&self) -> bool {
        self.synth.is_mono()
    }

    /// Resonators of a voice slot (introspection).
    pub fn mode_count(&self, voice: usize) -> usize {
        self.synth.voices()[voice].count
    }

    #[cfg(test)]
    pub(crate) fn mode_levels(&self, voice: usize) -> std::vec::Vec<f32> {
        let v = &self.synth.voices()[voice];
        v.level[..v.count].to_vec()
    }

    /// Renders a stereo block: the buffers are **overwritten** (an instrument, not an effect). `l` and `r` must have
    /// the same length (panics otherwise). Events whose offset lies inside the block fire at that sample, later ones
    /// stay queued for the following calls. The resonators' level meter (and the voice's quiet-stop peak) counts samples **per scheduler sub-block**, not per call: a call with events inside is several sub-blocks.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let _ftz = Guard::new();
        let sh = Shared { p: &self.p };
        self.synth.process(&sh, l, r);
    }
}
