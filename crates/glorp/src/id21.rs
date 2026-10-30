//! Glorp instrument ID 21: an additive bank of damped recursive sine resonators on the shared 16-voice
//! `SynthDevice` scheduler, complete for the poly mode the song uses, up to [`MAX_PARTIALS`] partials per voice.
//!
//! Independent behavioural implementation written from observed behaviour (research/glorp-devices.md §ID 21); no
//! original binary, table or pseudocode is included. f32 throughout, operation order follows the reference where it
//! matters for the numbers. Fixed 44.1 kHz.
//!
//! # Scheduler
//!
//! The shared one in [`crate::synth`] (256-slot queue, in-block offsets, lowest free voice, no stealing, a note-off
//! releases every voice of its note). A voice is freed on the sample its envelope reaches stage 0 and renders nothing
//! after it. Velocity does not exist.
//!
//! # Voice
//!
//! **Everything is latched at note-on** (nothing is read from the parameters while a voice renders, so automation only
//! reaches the next note). A voice is `(m+1)·(n+1)` partials, `n = trunc(100·P4)`, `m = trunc(100·P6)`, each a
//! two-pole resonator `y ← c1·y₁ − c2·y₂` driven only by its initial state (so it rings as a sine that decays by
//! `r = e^(−4.5e-8)` per sample: practically undamped), summed per channel with a pan gain and shaped by one ADSR
//! (attack `P0`, decay `P1`, sustain level `P2`, release `P3`, the first three times in seconds, see [`Adsr`]).
//!
//! The randomisation is a stateless hash of the **bit pattern of P7** plus fixed offsets (so P7 is a seed: one ulp
//! changes every draw), `u(h) = h·2⁻³¹ − 1`:
//!
//! ```text
//! per outer j (0..=m), o = j·0x3eeddc:
//!   ratio  = 2^( h(0xfed44b + o) · trunc(100·P5)/(12·2³²) )          partial ratio spread
//!   q      = ratio + (round_half_up(ratio) − ratio)·(2·P10 − 1)      snapping to integer ratios
//!   amp    = 2^( ln(q)·(7.2134752·P9 − 5.7707801) )                  spectral slope (= q^(5·P9 − 4))
//!   semis  = ln(q)·17.31234 + note
//!   gain   = P11 · u(h(0xfed833 + o))·amp·10 / count
//!   per inner i (0..=n), o' = j·0x3eeddd:
//!     detune = u(h(0xfedc1b + o' + 20i)) · 20·P8³ semitones
//!     f      = 440 · 2^( (detune + semis)/12 − 5.75 )                 Hz
//!     phase  = h(0xfee003 + o' + 40i)·(2π/2³⁰) − 4π
//!     pan    = u(h(0xfee3eb + o' + 40i)) · P12;   gain L = (1 + pan)·gain,  gain R = (1 − pan)·gain
//!     resonator: w = f·2π/44100,  c1 = 2r·tsin(π/2 + w),  c2 = r²,  y₁ = tsin(phase),  y₂ = tsin(phase − w)/r
//! ```
//!
//! (`h(x) = lowbias32(P7bits + x)`, every sum wraps at 32 bits; `tsin` is the 512-node interpolated table sine the
//! other Glorp devices use, **so the frequency of a low partial is only as exact as the table's cosine**; reproduced.)
//! Per sample: `L = env · Σₖ y₁ₖ·gLₖ` (partials summed in index order, `k = j·(n+1) + i`), `R` likewise, added to the
//! zeroed output; then the envelope ticks and every resonator steps.
//!
//! # Memory and support
//!
//! The original allocates 10 201 partials per voice (`P4 = P6 = 1`); here a voice holds [`MAX_PARTIALS`] = 512
//! (24 bytes each: the whole device is ≈ 200 KB). A parameter vector whose count exceeds it is refused with
//! [`Error::Partials`] by [`check`] / [`Id21::new`] / [`Id21::set_param`] (judged on the whole resulting vector, so
//! raising P4 and P6 in turn can need the other lowered first). Everything else in `[0, 1]` is modelled.
//! **Not modelled:** the shared device's mono/legato voice mode, voices-per-note ≠ 1 (unison), the in-voice slide,
//! sample rates other than 44.1 kHz, more than [`MAX_PARTIALS`] partials per voice.
//!
//! **Standalone poly-only API.** This type cannot represent the original's mono/legato or unison voice modes (they are
//! not among its 13 parameters) and cannot detect them; an adapter MUST reject a description that selects one before
//! it builds the device (the song chunk has no field for them, see `crate::preflight`).
//!
//! Exponentials, `ln` and `2^x` are the f64 `libm` results rounded once to f32 (an ordinary approximation; the
//! original's C library rounds a small share of arguments differently, **not** reproduced: tolerance parity, not bit
//! parity). Not a `synth_core::Device`: its `set_param` cannot report a rejection.

use crate::id25::{expf, hash, sine_table, DT};
use crate::id34::{sin_with, TABLE_LEN};
use crate::synth::{ActiveNote, Synth, Voice as SynthVoice};
use synth_core::denormal::Guard;

/// Parameter count of the device.
pub const PARAMS: usize = 13;
/// Partials one voice can hold (the original allocates 10 201): `(trunc(100·P4) + 1)·(trunc(100·P6) + 1)` above this
/// is refused.
pub const MAX_PARTIALS: usize = 512;
pub use crate::synth::{MAX_NOTE, QUEUE, VOICES};

/// Why a call is not supported. The device is left unchanged when one is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parameter index ≥ 13.
    Index,
    /// Not finite or outside `[0, 1]`.
    Range,
    /// More than [`MAX_PARTIALS`] partials per voice.
    Partials,
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
const W: f32 = f32::from_bits(0x3915_6592); // 2π/44100
const HALF_PI: f32 = f32::from_bits(0x3fc9_0fdb);
const DAMP: f32 = f32::from_bits(0xb342_c879); // −4.5351474e-8 (the exponent of the per-sample decay)
const P31: f32 = f32::from_bits(0x3000_0000); // 2^-31
const SPREAD: f32 = f32::from_bits(0x2daa_aaab); // 1/(12·2³²)
const PHASE: f32 = f32::from_bits(0x31c9_0fdb); // 2π/2³⁰
const NEG_4PI: f32 = f32::from_bits(0xc149_0fdb); // −12.566371
const SLOPE_A: f32 = f32::from_bits(0x40e6_d4ca); // 7.2134752
const SLOPE_B: f32 = f32::from_bits(0xc0b8_aa3b); // −5.7707801
const SEMI: f32 = f32::from_bits(0x418a_7fac); // 17.31234
const TWELFTH: f32 = f32::from_bits(0x3daa_aaab);

fn pow2(x: f32) -> f32 {
    libm::exp2(x as f64) as f32
}

fn lnf(x: f32) -> f32 {
    libm::log(x as f64) as f32
}

/// The shared exponential envelope (attack `A`, decay `D` to level `S`, sustain, release `R`; times in seconds), one
/// tick per sample on the f32 clock `t += 1/44100`. The level the release starts from is the last stored (unclamped)
/// stage value and **survives from one note to the next** (it is only cleared when a release finishes).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Adsr {
    pub(crate) t: f32,
    pub(crate) level: f32,
    pub(crate) a: f32,
    pub(crate) d: f32,
    pub(crate) s: f32,
    pub(crate) r: f32,
    /// 0 done, 1 attack, 2 decay, 3 sustain, 4 release.
    pub(crate) stage: u8,
}

impl Adsr {
    pub(crate) fn idle() -> Self {
        Adsr { t: 0.0, level: 0.0, a: 0.0, d: 0.0, s: 0.0, r: 0.0, stage: 0 }
    }

    /// Note-on: new times, clock 0, attack. The stored level is kept.
    pub(crate) fn restart(&mut self, a: f32, d: f32, s: f32, r: f32) {
        self.a = a;
        self.d = d;
        self.s = s;
        self.r = r;
        self.stage = 1;
        self.t = 0.0;
    }

    /// Note-off: release stage from the stored level, clock 0.
    pub(crate) fn release(&mut self) {
        self.stage = 4;
        self.t = 0.0;
    }

    /// Output limited to `[0, 1]` (NaN → 0).
    fn clamp(v: f32) -> f32 {
        if v > 1.0 {
            1.0
        } else if v >= 0.0 {
            v
        } else {
            0.0
        }
    }

    /// One tick; returns the envelope value for this sample. Stage 0 returns 0.
    pub(crate) fn tick(&mut self) -> f32 {
        self.t = DT + self.t;
        let t = self.t;
        match self.stage {
            1 => {
                let v;
                if 0.0 >= self.a {
                    v = 1.0;
                    self.t = 0.0;
                    self.stage = 2;
                } else {
                    let inv = 1.0 / self.a;
                    let e = expf((inv + inv) * (t - self.a));
                    v = (inv * t) * e;
                    if t >= self.a {
                        self.t = 0.0;
                        self.stage = 2;
                    }
                }
                self.level = v;
                Self::clamp(v)
            }
            2 => {
                let v;
                if 0.0 >= self.d {
                    v = self.s;
                    self.t = 0.0;
                    self.stage = 3;
                } else {
                    let x = t / self.d;
                    let e = expf(-2.0 * x);
                    let target = ((self.s + -1.0) * (1.0 - e)) + 1.0;
                    v = (self.s - target) * x + target;
                    if t >= self.d {
                        self.t = 0.0;
                        self.stage = 3;
                    }
                }
                self.level = v;
                Self::clamp(v)
            }
            3 => {
                self.level = self.s;
                Self::clamp(self.s)
            }
            4 => {
                if 0.0 >= self.r {
                    // the stored level is not cleared on this path
                    self.stage = 0;
                    return 0.0;
                }
                let x = t / self.r;
                let e = expf(-2.0 * x);
                let v = ((1.0 - x) * e) * self.level;
                if t < self.r {
                    Self::clamp(v)
                } else {
                    self.level = 0.0;
                    self.stage = 0;
                    0.0
                }
            }
            _ => 0.0,
        }
    }
}

/// One resonator with its pan gains (6 floats).
#[derive(Clone, Copy, Debug, Default)]
struct Partial {
    c1: f32,
    c2: f32,
    y1: f32,
    y2: f32,
    gl: f32,
    gr: f32,
}

/// Number of partials a parameter vector asks for (the original's count, before the [`MAX_PARTIALS`] bound).
pub fn partials(p: &[f32; PARAMS]) -> usize {
    let n = (p[4] * 100.0) as usize;
    let m = (p[6] * 100.0) as usize;
    (n + 1) * (m + 1)
}

/// Whether `new` accepts these parameters (cheap: builds nothing). `Err` leaves nothing to undo.
pub fn check(p: &[f32; PARAMS]) -> Result<(), Error> {
    if !p.iter().all(|v| (0.0..=1.0).contains(v)) {
        return Err(Error::Range);
    }
    if partials(p) > MAX_PARTIALS {
        return Err(Error::Partials);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct Voice {
    active: bool,
    note: u8,
    count: u32,
    adsr: Adsr,
    parts: [Partial; MAX_PARTIALS],
}

impl Voice {
    fn idle() -> Self {
        Voice { active: false, note: 0, count: 0, adsr: Adsr::idle(), parts: [Partial::default(); MAX_PARTIALS] }
    }

    /// Note-on: the envelope times and the whole partial set drawn from the parameters as they are now.
    fn note_start(&mut self, note: u8, p: &[f32; PARAMS], table: &[f32; TABLE_LEN]) {
        self.active = true;
        self.note = note;
        self.adsr.restart(p[0], p[1], p[2], p[3]);
        let n = (p[4] * 100.0) as u32;
        let m = (p[6] * 100.0) as u32;
        let count = (n + 1) * (m + 1);
        debug_assert!(count as usize <= MAX_PARTIALS, "check() bounds the partial count");
        self.count = count;
        let seed = p[7].to_bits();
        let draw = |off: u32| hash(seed.wrapping_add(off)) as f32;
        let spread = ((p[5] * 100.0) as i32) as f32 * SPREAD;
        let snap = (p[10] + p[10]) + -1.0;
        let cube = {
            let a = if p[8] > 0.0 { p[8] } else { 0.0 };
            ((a * a) * 20.0) * a
        };
        let slope = p[9] * SLOPE_A + SLOPE_B;
        let r = expf(DAMP);
        let mut k = 0usize;
        for j in 0..=m {
            let o = j.wrapping_mul(0x3e_eddc);
            let o2 = j.wrapping_mul(0x3e_eddd);
            let ratio = pow2(draw(0xfe_d44b_u32.wrapping_add(o)) * spread);
            let near = {
                let x = ratio + 0.5;
                let tr = (x as i32) as f32;
                if x < tr { tr + -1.0 } else { tr }
            };
            let q = ratio + (near - ratio) * snap;
            let l = lnf(q);
            let amp = pow2(l * slope);
            let semis = l * SEMI + note as f32;
            let b = ((draw(0xfe_d833_u32.wrapping_add(o)) * P31 + -1.0) * amp) * 10.0;
            let gain = (p[11] * b) / count as f32;
            for i in 0..=n {
                let da = draw(0xfe_dc1b_u32.wrapping_add(o2).wrapping_add(0x14 * i)) * P31 + -1.0;
                let x = ((da * cube + semis) * TWELFTH) + -5.75;
                let f = pow2(x) * 440.0;
                let phase = draw(0xfe_e003_u32.wrapping_add(o2).wrapping_add(0x28 * i)) * PHASE + NEG_4PI;
                let pan = (draw(0xfe_e3eb_u32.wrapping_add(o2).wrapping_add(0x28 * i)) * P31 + -1.0) * p[12];
                let w = f * W;
                let cosw = sin_with(table, HALF_PI + w);
                self.parts[k] = Partial {
                    c1: (r + r) * cosw,
                    c2: r * r,
                    y1: sin_with(table, phase),
                    y2: sin_with(table, phase - w) / r,
                    gl: (pan + 1.0) * gain,
                    gr: (1.0 - pan) * gain,
                };
                k += 1;
            }
        }
    }

    /// Renders `l.len()` samples, adding into `l` / `r`. Stops at the sample whose envelope tick reaches stage 0.
    fn render_samples(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = self.count as usize;
        for i in 0..l.len() {
            let (mut al, mut ar) = (0.0f32, 0.0f32);
            for q in &self.parts[..n] {
                al += q.y1 * q.gl;
                ar += q.y1 * q.gr;
            }
            let env = self.adsr.tick();
            l[i] = env * al + l[i];
            r[i] = ar * env + r[i];
            for q in &mut self.parts[..n] {
                let y = q.c1 * q.y1 - q.y2 * q.c2;
                q.y2 = q.y1;
                q.y1 = y;
            }
            if self.adsr.stage == 0 {
                self.active = false;
                return;
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
        self.adsr.release();
    }
    fn render(&mut self, _: &Shared<'_>, l: &mut [f32], r: &mut [f32]) {
        self.render_samples(l, r);
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

/// The ID 21 instrument: 16 voices of up to [`MAX_PARTIALS`] partials on the shared scheduler, 13 parameters.
#[derive(Clone, Debug)]
pub struct Id21 {
    p: [f32; PARAMS],
    table: [f32; TABLE_LEN],
    synth: Synth<Voice>,
}

impl Id21 {
    /// Builds the device from the 13 normalized parameters (a song chunk's first 13 floats). No voice is playing.
    pub fn new(p: &[f32; PARAMS]) -> Result<Self, Error> {
        check(p)?;
        Ok(Id21 { p: *p, table: sine_table(), synth: Synth::new([Voice::idle(); VOICES]) })
    }

    /// Sets one parameter between `process` calls. Sounding voices keep what they were started with; the next note-on
    /// uses the new value. The whole resulting vector is judged ([`Error::Partials`]); on `Err` nothing changed.
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

    /// Queues a note-on `offset` samples after the start of the next `process` call (velocity does not exist for
    /// this instrument). With every voice busy at the time it fires, it is lost.
    pub fn note_on(&mut self, note: u8, offset: u32) -> Result<(), Error> {
        self.synth.note_on(note, offset).map_err(Error::from)
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
        self.synth.process(&(&self.p, &self.table), l, r);
    }
}
