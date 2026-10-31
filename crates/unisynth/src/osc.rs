//! Oscillator slots (design D4): the types a slot can be, their per-voice state, the control values they ramp, and the
//! per-sample generators. The phase-mod operator is Falcon's: a sine blended with `Square35` (`sin 3φ/3 + sin 5φ/5`),
//! self-feedback, and a send into the next slot's phase. The band-limited saw/pulse is Slaughter's integrated BLIT:
//! a sinc impulse train (one impulse per period for the saw, a second one a pulse width later for the pulse)
//! integrated by a leaky integrator, with the DC of the impulse train removed.

use crate::module;
use crate::params::{self as pm, o, osc, osc_type, COUNT};
use crate::ramp::Ramp;
use crate::smp;
use crate::tables::sin_turns;
#[cfg(feature = "us-osc-wt")]
pub use crate::wt::Wt;
/// Stands in for the wavetable state when the module is left out of the build (never instantiated as a slot kind).
#[cfg(not(feature = "us-osc-wt"))]
pub struct Wt;
#[cfg(not(feature = "us-osc-wt"))]
impl Wt {
    pub const fn new() -> Self {
        Wt
    }
}
use synth_core::mix::LoadError;

/// What a slot is, fixed at load (`o::TYPE` is a structural parameter).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Off,
    Pm,
    Blit,
    /// Sample player: a GSM sample or a GM.DLS wave.
    Sampler,
    /// Spectral wavetable: needs a [`crate::wt::Wt`] from the pool.
    #[cfg(feature = "us-osc-wt")]
    Wt,
    /// V2's oscillator (change add-v2-import): mono, works on the voice's running sum ([`v2_sample`]).
    V2,
    /// Mini-cauldron's waveforms (change add-cauldron-xm-import): a 16-bit phase, naive output ([`cauldron`]).
    Cauldron,
}

impl Kind {
    /// The kind for an `osc_type`, or the module that is missing from this build or not written yet.
    pub fn from_type(ty: usize) -> Result<Kind, LoadError> {
        let missing = |m| Err(LoadError::UnsupportedModule(m));
        match ty {
            osc_type::OFF => Ok(Kind::Off),
            osc_type::PM if cfg!(feature = "us-osc-pm") => Ok(Kind::Pm),
            osc_type::PM => missing(module::OSC_PM),
            osc_type::SAMPLER if cfg!(feature = "us-osc-sample") => Ok(Kind::Sampler),
            osc_type::BLIT if cfg!(feature = "us-osc-blit") => Ok(Kind::Blit),
            osc_type::BLIT => missing(module::OSC_BLIT),
            osc_type::V2 if cfg!(feature = "us-osc-naive") => Ok(Kind::V2),
            osc_type::V2 => missing(module::OSC_NAIVE),
            osc_type::CAULDRON if cfg!(feature = "us-osc-cauldron") => Ok(Kind::Cauldron),
            osc_type::CAULDRON => missing(module::OSC_CAULDRON),
            #[cfg(feature = "us-osc-wt")]
            osc_type::WAVETABLE => Ok(Kind::Wt),
            #[cfg(not(feature = "us-osc-wt"))]
            osc_type::WAVETABLE => missing(module::OSC_WAVETABLE),
            _ => missing(module::OSC_SAMPLER),
        }
    }

    /// The three slot kinds of a patch.
    pub fn all(values: &[f32; COUNT]) -> Result<[Kind; pm::OSC_SLOTS], LoadError> {
        let mut k = [Kind::Off; pm::OSC_SLOTS];
        for (i, slot) in k.iter_mut().enumerate() {
            *slot = Kind::from_type(osc_type::decode(values[osc(i, o::TYPE)]))?;
        }
        Ok(k)
    }
}

/// Per-voice state of one slot.
#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub kind: Kind,
    /// Phase in turns.
    pub phase: f64,
    /// The slot's previous output (feedback).
    pub prev: f32,
    /// Saw/pulse: the leaky integrator.
    pub integral: f64,
    /// Sampler: playback position.
    pub smp: smp::State,
    /// Saw/pulse: no note has started it yet, so `phase` is still the unscaled draw.
    pub fresh: bool,
    /// V2: wave counter, noise filter and seed (`v2::Opt`: absent without the V2 voice features).
    pub v2s: crate::v2::Opt<V2Slot>,
    /// Cauldron: the detuned second saw, the phrase frame counter and the noise state.
    pub cd: CauldronSlot,
}

/// A cauldron slot's running state. `phase2` (turns) is the original's second saw: kept across note-ons, as the
/// original keeps it across retriggers. `t` counts phrase frames (the original's frame counter `t`), set at note-on
/// from the note's velocity ([`cauldron_t0`]). `rng` is the noise state, seeded at note-on.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CauldronSlot {
    pub phase2: f64,
    pub t: f64,
    pub rng: u32,
    /// The filter: `y` (runs on across note-ons), `prev` (its output, 0 at a note-on), `v1` (the envelope part of
    /// `c`, latched at a note-on) ([`cauldron_filter`]).
    pub y: f64,
    pub prev: f64,
    pub v1: f64,
    /// The filter settings of the last note-on: the original computes a step's filter at its retrigger, so a later
    /// change (the converter's retrigger byte, free between notes) reaches it only at the next note-on.
    pub flt: Option<CauldronF>,
    /// The resampler: the last four phrase-rate samples (newest last) and the phrase-frame fraction past `h[1]` the
    /// next output sits at ([`cauldron`]).
    pub h: [f32; 4],
    pub frac: f64,
    /// The reconstruction low-pass after the resampler ([`CauldronLp`]).
    pub lp: CauldronLp,
}

/// The resampler's reconstruction filter: below one phrase frame per output frame the cubic leaves images of the
/// phrase-rate signal above its Nyquist (a phrase slowed 17 times by an XM slide ran at a 2 kHz clock and buzzed
/// +20–30 dB above the original's top), where the original's player band-limits. A 4th-order Butterworth low-pass at
/// `0.45·clock` of the output rate (two biquads, transposed direct form II), bypassed at a clock of 1 or more;
/// coefficients follow the clock (`k`, the cutoff they were made for).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CauldronLp {
    pub k: f32,
    pub c: [[f32; 5]; 2],
    pub s: [[f32; 2]; 2],
}

impl CauldronLp {
    /// The cutoff over the output rate for `clock` phrase frames per output frame; 0 = bypass.
    fn cutoff(clock: f64) -> f32 {
        if clock < 1.0 { (0.45 * clock) as f32 } else { 0.0 }
    }

    fn run(&mut self, x: f32, clock: f64) -> f32 {
        let k = Self::cutoff(clock);
        if k == 0.0 {
            self.k = 0.0;
            return x;
        }
        if k != self.k {
            let w = libm::tanf(core::f32::consts::PI * k);
            for (c, q) in self.c.iter_mut().zip([0.541_196_1f32, 1.306_563]) {
                let n = 1.0 / (1.0 + w / q + w * w);
                let b0 = w * w * n;
                *c = [b0, 2.0 * b0, b0, 2.0 * (w * w - 1.0) * n, (1.0 - w / q + w * w) * n];
            }
            self.k = k;
        }
        let mut y = x;
        for (c, s) in self.c.iter().zip(self.s.iter_mut()) {
            let o = c[0] * y + s[0];
            s[0] = c[1] * y - c[3] * o + s[1];
            s[1] = c[2] * y - c[4] * o;
            y = o;
        }
        y
    }
}

/// A cauldron slot's filter for a control segment, per phrase frame (the slot runs at the phrase rate): envelope
/// amount `a` and base `b` (the original's `v1`, `v2`), the envelope's decay `d`, `q1` = 1 − damping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CauldronF {
    pub a: f64,
    pub b: f64,
    pub d: f64,
    pub q1: f64,
}

/// A cauldron slot's settings for a control segment (not ramped): wave 0–5 (saw, square, spike, pulse, noise, fold),
/// voices 0–10, version 2 (`v2`), and `clock`, the phrase frames per output frame (the original's internal rate times
/// the playback ratio, over the song rate).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CauldronT {
    pub wave: u8,
    pub voices: u8,
    pub v2: bool,
    pub clock: f32,
    pub flt: Option<CauldronF>,
    /// The second saw's phase (turns) a non-legato note-on sets, from the slot's `o::PM_OUT` byte; `None` keeps it
    /// running.
    pub phase2: Option<f64>,
    /// Version 2 in a glide (`CD_VERSION` ½): the original's glide stores its pitch register as the signed
    /// interpolation of the words, so a word above 32767 steps the second saw and the spike as `w − 65536`.
    pub glide: bool,
    /// The word byte (`o::FIXED_HZ`, read as such: a cauldron slot always tracks the note): b ≥ 1 makes the pitch
    /// word `rint(word) + b − 128`, the original's integer word, which a note's frequency (fine byte ±0.39 cents:
    /// ±11.6 words at 51199) cannot reach; 0 (`None`) leaves it as the frequency gives it (bends stay smooth).
    pub word: Option<i32>,
    /// The filter envelope's age at a note-on, phrase frames since the last retrigger ([`cauldron_age`], from the
    /// slot's `o::TRACKING` byte); `None` restarts it at the amount.
    pub age: Option<f64>,
}

impl Default for CauldronT {
    fn default() -> Self {
        CauldronT { wave: 0, voices: 0, v2: false, clock: 1.0, flt: None, phase2: None, glide: false, word: None, age: None }
    }
}

/// The age byte (a cauldron slot's `o::TRACKING` value, read as such: the slot always tracks the note): 0 (`None`)
/// restarts the filter envelope at a note-on, as a retrigger; b ≥ 1 is `(b − 1)` eighth steps (of 3530 phrase frames)
/// since the last retrigger, a note entering a phrase mid-way (an XM jump, a ping-pong pass's reversed step), so the
/// envelope starts decayed. Without it a late step came out as bright as the first: a low-damped filter (Stash's
/// `Jacef1`, damp 0.05) rang at its cutoff.
pub fn cauldron_age(p: f32) -> Option<f64> {
    match libm::roundf(p * 255.0).clamp(0.0, 255.0) as u32 {
        0 => None,
        b => Some((b - 1) as f64 * (3530.0 / 8.0)),
    }
}

impl CauldronT {
    /// From the slot's shape values (after the matrix) at song rate `rate`, `mul_p` (the slot's `o::FEEDBACK` value:
    /// the filter's per-retrigger factor `4p`) and `phase_p` (its `o::PM_OUT` value: byte b = `round(255p)`, 0 keeps
    /// the second saw running, b ≥ 1 sets it to `(b − 1)/255` turn at a note-on); any stored value clamps into range.
    /// The cauldron slot sends nothing into the next slot's phase.
    pub fn from_shape(sh: &[f32; pm::o::SHAPES], rate: f32, mul_p: f32, phase_p: f32, word_p: f32) -> Self {
        use pm::shape::*;
        let hz = if sh[CD_CLOCK] > 0.0 { sh[CD_CLOCK] * 1_048_576.0 } else { 0.0 };
        let clock = if hz > 0.0 && rate > 0.0 { hz / rate } else { 1.0 };
        let (mut b, mut a) = (sh[CD_F_BASE].max(0.0) as f64 * 8.0, sh[CD_F_AMOUNT].max(0.0) as f64 * 8.0);
        // choice of 3: 0 v1, ½ v2 in a glide, 1 v2 (the stored 0 and 1 keep their meaning)
        let version = crate::law::choice(sh[CD_VERSION], 3);
        let v2 = version > 0;
        // the retrigger byte: k compounding steps (each stored as f32, as the original), then the accent
        let byte = libm::roundf(sh[CD_RETRIG] * 255.0).clamp(0.0, 255.0) as u32;
        let mul = mul_p.max(0.0) as f64 * 4.0;
        for _ in 0..byte >> 1 {
            a = (a * mul) as f32 as f64;
            b = (b * mul) as f32 as f64;
        }
        if byte & 1 == 1 {
            let accent = if v2 { 2.0 } else { 4.0 };
            (a, b) = (a * accent, b * accent);
        }
        let flt = (b > 0.0 || a > 0.0).then(|| CauldronF {
            a,
            b,
            d: 1.0 + (0.5 - sh[CD_F_DECAY] as f64) / 100.0,
            q1: 1.0 - (sh[CD_F_DAMP] as f64).clamp(0.0, 1.0),
        });
        CauldronT {
            wave: crate::law::choice(sh[CD_WAVE], 6) as u8,
            voices: libm::roundf(sh[CD_VOICES] * 10.0).clamp(0.0, 10.0) as u8,
            v2,
            clock,
            flt,
            glide: version == 1,
            age: None,
            word: match libm::roundf(word_p * 255.0).clamp(0.0, 255.0) as i32 {
                0 => None,
                b => Some(b - 128),
            },
            phase2: match libm::roundf(phase_p * 255.0).clamp(0.0, 255.0) as u32 {
                0 => None,
                b => Some((b - 1) as f64 / 255.0),
            },
        }
    }
}

/// A V2 slot's running state: wave counter (one turn = 2³²), noise filter and noise seed; `init` once the seed was
/// set.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V2Slot {
    pub cnt: u32,
    pub nf: crate::v2::Lrc,
    pub nseed: u32,
    pub init: bool,
}

impl Slot {
    /// `phase` is the slot's start phase (turns), drawn once when the voice is built.
    pub const fn new(kind: Kind, phase: f64) -> Self {
        Slot {
            kind,
            phase,
            prev: 0.0,
            integral: 0.0,
            smp: smp::State::new(),
            fresh: true,
            v2s: crate::v2::Opt::new(V2Slot { cnt: 0, nf: crate::v2::Lrc { l: 0.0, b: 0.0 }, nseed: 0, init: false }),
            cd: CauldronSlot { phase2: 0.0, t: 0.0, rng: 0, y: 0.0, prev: 0.0, v1: 0.0, flt: None, h: [0.0; 4], frac: 0.0, lp: CauldronLp { k: 0.0, c: [[0.0; 5]; 2], s: [[0.0; 2]; 2] } },
        }
    }

    /// Cauldron: the phrase frame at this note-on ([`cauldron_t0`]); [`Slot::start`] has reset the phase. The filter
    /// restarts like the original's retrigger: its output memory is 0, the envelope part of `c` is the amount; `y`
    /// runs on.
    pub fn cauldron_start(&mut self, t0: f64, c: &CauldronT) {
        self.cd.t = t0;
        if let Some(p) = c.phase2 {
            self.cd.phase2 = p;
        }
        self.cd.flt = c.flt;
        if let Some(f) = c.flt {
            self.cd.prev = 0.0;
            self.cd.v1 = match c.age {
                Some(n) => (f.a * libm::pow(f.d, n)).min(CD_SATURATE),
                None => f.a,
            };
        }
    }

    /// Voice start. The phase-mod operator restarts from `phase`; the saw/pulse keeps its phase and integrator, which
    /// run on across notes as Slaughter's do.
    ///
    /// `spread` is the fraction of a turn the random start phase reaches (`law::start_phase`): `phase` is the voice's
    /// draw for the operators, and the saw/pulse scales its own draw at its first note, when its pitch is known.
    pub fn start(&mut self, phase: f64, p: f32, inc: f32) {
        match self.kind {
            Kind::Pm if cfg!(feature = "us-osc-pm") => {
                self.phase = crate::law::start_phase(phase, p, inc);
                self.prev = 0.0;
            }
            Kind::Blit if cfg!(feature = "us-osc-blit") && self.fresh => {
                self.phase = crate::law::start_phase(self.phase, p, inc);
                self.fresh = false;
            }
            // the original's retrigger: phase -32766 (16-bit), the second saw runs on; noise seeded from the draw
            Kind::Cauldron if cfg!(feature = "us-osc-cauldron") => {
                self.phase = 32770.0 / 65536.0;
                self.cd.t = 0.0;
                self.cd.rng = ((phase * 4_294_967_296.0) as u64 as u32) | 1;
            }
            _ => {}
        }
    }
}

/// Control values of one slot at a control state.
#[derive(Clone, Copy, Debug, Default)]
pub struct SlotT {
    /// Phase increment per sample, in turns.
    pub inc: f32,
    pub gain: f32,
    pub mix: f32,
    pub pan_l: f32,
    pub pan_r: f32,
    pub pm_out: f32,
    pub feedback: f32,
    /// Phase-mod: weight of `Square35`. Saw/pulse: 0 saw .. 1 pulse.
    pub wf: f32,
    /// Saw/pulse: pulse width (fraction of the period).
    pub pw: f32,
    /// Saw/pulse: integrator leak per sample.
    pub leak: f32,
    /// Wavetable: left/right phase offset per layer, and extra offset per layer, in phase per sample.
    pub det: f32,
    pub spread: f32,
    /// Wavetable: drive before the hard clip.
    pub drive: f32,
    /// Wavetable: unison layers.
    pub layers: usize,
    /// Sampler: loop, interpolation and loop bounds.
    pub smp: smp::Cfg,
    /// V2: waveform, ring, break point, noise filter and the format-version switches.
    pub v2: crate::v2::Opt<V2T>,
    /// Cauldron: wave, voices, version and phrase clock.
    pub cd: CauldronT,
}

/// A V2 slot's settings for a control segment (V2 keeps them for a frame; they are not ramped).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V2T {
    /// V2 mode 0..7 (`V2Osc::Mode`).
    pub mode: u8,
    pub ring: bool,
    /// Break point (colour) as 0.32 fixed point.
    pub brpt: u32,
    pub nffrq: f32,
    pub nfres: f32,
    /// Format version below 6: native sine for the sine and FM modes.
    pub native_sin: bool,
    /// Format version below 3: saw/triangle and pulse are the year-2000 4× oversampled box filter (v2redux
    /// `DELTA_OSC_BOXFILTER`).
    pub box4: bool,
    /// Format version below 5: the older noise (MSVC LCG, its own filter recurrence; `DELTA_NOISE_LCG_MSVC`).
    pub old_noise: bool,
}

/// V2 oscillator modes (`V2Osc::Mode`).
pub mod v2mode {
    pub const OFF: u8 = 0;
    pub const TRI_SAW: u8 = 1;
    pub const PULSE: u8 = 2;
    pub const SIN: u8 = 3;
    pub const NOISE: u8 = 4;
    pub const FM: u8 = 5;
    pub const AUX_A: u8 = 6;
    pub const AUX_B: u8 = 7;
}

/// Whether oscillator slot `i` of patch `p` is a V2 aux oscillator (mode aux A or aux B, format version 6): it reads
/// the aux buses through the instrument's extra input pairs, which only the channel stage's build has (`us-strip`).
pub fn v2_aux(p: &[f32; crate::params::COUNT], i: usize) -> bool {
    use crate::params as pm;
    pm::osc_type::decode(p[pm::osc(i, pm::o::TYPE)]) == pm::osc_type::V2
        && (pm::v2_raw(p[pm::osc(i, pm::o::SHAPE + pm::shape::V2_MODE)]) as i32 & 7) as u8 >= v2mode::AUX_A
        && pm::era(p).0 >= 6
}

/// What a V2 slot does to the voice's running sum this sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum V2Out {
    Add(f32),
    /// Ring mode: multiplies the running sum.
    Mul(f32),
    /// FM and the aux input replace it.
    Set(f32),
}

/// The ramps that carry a slot's control values across a segment.
#[derive(Clone, Copy, Debug, Default)]
pub struct SlotRamps {
    inc: Ramp,
    gain: Ramp,
    mix: Ramp,
    pan_l: Ramp,
    pan_r: Ramp,
    pm_out: Ramp,
    feedback: Ramp,
    wf: Ramp,
    pw: Ramp,
    det: Ramp,
    spread: Ramp,
    drive: Ramp,
    leak: f32,
    layers: usize,
    smp: smp::Cfg,
    v2: crate::v2::Opt<V2T>,
    cd: CauldronT,
}

impl SlotRamps {
    pub fn set(&mut self, t: &SlotT) {
        self.inc.set(t.inc);
        self.gain.set(t.gain);
        self.mix.set(t.mix);
        self.pan_l.set(t.pan_l);
        self.pan_r.set(t.pan_r);
        self.pm_out.set(t.pm_out);
        self.feedback.set(t.feedback);
        self.wf.set(t.wf);
        self.pw.set(t.pw);
        self.det.set(t.det);
        self.spread.set(t.spread);
        self.drive.set(t.drive);
        self.leak = t.leak;
        self.layers = t.layers;
        self.smp = t.smp;
        if cfg!(feature = "us-osc-naive") {
            self.v2 = t.v2;
        }
        self.cd = t.cd;
    }

    pub fn to(&mut self, t: &SlotT, frames: u32) {
        self.inc.to(t.inc, frames);
        self.gain.to(t.gain, frames);
        self.mix.to(t.mix, frames);
        self.pan_l.to(t.pan_l, frames);
        self.pan_r.to(t.pan_r, frames);
        self.pm_out.to(t.pm_out, frames);
        self.feedback.to(t.feedback, frames);
        self.wf.to(t.wf, frames);
        self.pw.to(t.pw, frames);
        self.det.to(t.det, frames);
        self.spread.to(t.spread, frames);
        self.drive.to(t.drive, frames);
        self.leak = t.leak;
        self.layers = t.layers;
        self.smp = t.smp;
        if cfg!(feature = "us-osc-naive") {
            self.v2 = t.v2;
        }
        self.cd = t.cd;
    }

    pub fn retarget(&mut self, t: &SlotT, frames: u32) {
        self.inc.retarget(t.inc, frames);
        self.gain.retarget(t.gain, frames);
        self.mix.retarget(t.mix, frames);
        self.pan_l.retarget(t.pan_l, frames);
        self.pan_r.retarget(t.pan_r, frames);
        self.pm_out.retarget(t.pm_out, frames);
        self.feedback.retarget(t.feedback, frames);
        self.wf.retarget(t.wf, frames);
        self.pw.retarget(t.pw, frames);
        self.det.retarget(t.det, frames);
        self.spread.retarget(t.spread, frames);
        self.drive.retarget(t.drive, frames);
        self.leak = t.leak;
        self.layers = t.layers;
        self.smp = t.smp;
        if cfg!(feature = "us-osc-naive") {
            self.v2 = t.v2;
        }
        self.cd = t.cd;
    }
}

const INV_TAU: f32 = 1.0 / (2.0 * core::f32::consts::PI);

/// One sample of a slot: `(left, right, send)` = its output after mix and pan, and the phase-mod send for the next
/// slot in radians. `pm_in` is the previous slot's send.
#[inline]
pub fn sample(slot: &mut Slot, wt: Option<&mut Wt>, src: &smp::Source, r: &mut SlotRamps, pm_in: f32) -> (f32, f32, f32) {
    let inc = r.inc.next();
    let gain = r.gain.next();
    let mix = r.mix.next();
    let (pl, pr) = (r.pan_l.next(), r.pan_r.next());
    let send = r.pm_out.next();
    let fb = r.feedback.next();
    let wf = r.wf.next();
    let pw = r.pw.next();
    let (det, spread, drive) = (r.det.next(), r.spread.next(), r.drive.next());
    #[cfg(not(feature = "us-osc-wt"))]
    let _ = (wt, det, spread, drive);
    match slot.kind {
        Kind::Pm if cfg!(feature = "us-osc-pm") => {
            let t = slot.phase + ((fb * slot.prev + pm_in) * INV_TAU) as f64;
            let y = (sin_turns(t) + (sin_turns(3.0 * t) * (1.0 / 3.0) + sin_turns(5.0 * t) * 0.2) * wf) * gain;
            slot.prev = y;
            slot.phase += inc as f64;
            slot.phase -= libm::floor(slot.phase);
            (y * mix * pl, y * mix * pr, y * send)
        }
        Kind::Sampler if cfg!(feature = "us-osc-sample") => {
            let y = slot.smp.next(src, inc, &r.smp) * gain;
            (y * mix * pl, y * mix * pr, y * send)
        }
        Kind::Blit if cfg!(feature = "us-osc-blit") => {
            let y = blit(slot, inc as f64, wf as f64, pw as f64, r.leak as f64) * gain;
            (y * mix * pl, y * mix * pr, y * send)
        }
        Kind::Cauldron if cfg!(feature = "us-osc-cauldron") => {
            // its `o::PM_OUT` is the second saw's phase byte: no send
            let _ = send;
            let y = cauldron(slot, inc as f64, &r.cd) * gain;
            (y * mix * pl, y * mix * pr, 0.0)
        }
        #[cfg(feature = "us-osc-wt")]
        Kind::Wt => {
            let (a, b) = match wt {
                Some(wt) => wavetable(wt, inc, det, spread, drive, r.layers),
                None => (0.0, 0.0),
            };
            (a * gain * mix * pl, b * gain * mix * pr, (a + b) * 0.5 * gain * send)
        }
        // a type of a module this build lacks is rejected at load; the guards let the compiler drop its code
        _ => (0.0, 0.0, 0.0),
    }
}

const TWO_32: f32 = 4_294_967_296.0;

/// One sample of a V2 slot (`V2Osc::render*`, v2redux's assembly-faithful versions). `mono` is the voice's running sum
/// (the FM modulator), `aux` the aux A and aux B input pairs at this sample (the bus mixes so far in the frame). The counter advances by the ramped increment
/// as V2's integer `freq`.
#[inline]
/// One V2 sample and the slot's pan gains `(left, right)` (1, 1 in the V2 voice).
pub fn v2_sample(slot: &mut Slot, r: &mut SlotRamps, mono: f32, aux: [(f32, f32); 2]) -> (V2Out, f32, f32) {
    let mut st = slot.v2s.get();
    let (pl, pr) = (r.pan_l.next(), r.pan_r.next());
    let y = v2_step(&mut st, r, mono, aux);
    slot.v2s.set(st);
    (y, pl, pr)
}

#[inline(always)]
fn v2_step(slot: &mut V2Slot, r: &mut SlotRamps, mono: f32, aux: [(f32, f32); 2]) -> V2Out {
    use crate::v2::{fastsin, fastsinrc, utof23};
    let inc = r.inc.next();
    let gain = r.gain.next() * r.mix.next();
    let _ = (r.pm_out.next(), r.feedback.next(), r.wf.next(), r.pw.next());
    let _ = (r.det.next(), r.spread.next(), r.drive.next());
    let t = &r.v2.get();
    let freq = (inc * TWO_32) as i64 as u32;
    let out = |y: f32| if t.ring { V2Out::Mul(y) } else { V2Out::Add(y) };
    match t.mode {
        v2mode::TRI_SAW | v2mode::PULSE if t.box4 => {
            // four sub-steps of a quarter increment, averaged
            let step = freq / 4;
            let col = utof23(t.brpt);
            let bg = gain * 0.25;
            let mut acc = 0.0f32;
            let mut c = slot.cnt;
            for _ in 0..4 {
                if t.mode == v2mode::TRI_SAW {
                    let p = f32::from_bits((c >> 9) | 0x3f80_0000);
                    acc += if c < t.brpt {
                        p * (2.0 / col.max(1e-9)) + (-1.0 - 2.0 / col.max(1e-9))
                    } else {
                        p * (-2.0 / (1.0 - col).max(1e-9)) + (4.0 / (1.0 - col).max(1e-9) - 1.0)
                    };
                } else if c >= t.brpt {
                    acc += 2.0;
                }
                c = c.wrapping_add(step);
            }
            slot.cnt = c;
            out(if t.mode == v2mode::TRI_SAW { acc * bg } else { (acc - 4.0) * bg })
        }
        v2mode::TRI_SAW | v2mode::PULSE => {
            let cnt = slot.cnt;
            let brpt = t.brpt;
            let old_up = cnt.wrapping_sub(freq) < brpt;
            let new_up = cnt < brpt;
            let carry = cnt < freq;
            slot.cnt = cnt.wrapping_add(freq);
            let f = utof23(freq).max(1e-9);
            let col = utof23(brpt);
            let p = utof23(cnt);
            let code = (carry as u8) << 2 | (old_up as u8) << 1 | new_up as u8;
            if t.mode == v2mode::TRI_SAW {
                // the transition cases cancel catastrophically and then divide by f: V2 ran them on the x87 (15-bit
                // exponent); in f32 a low note picks up audible noise, so they run in f64 (as v2redux's portable build)
                let (f, g) = (f as f64, gain as f64);
                let omf = 1.0 - f;
                let rcpf = 1.0 / f;
                let col = col as f64;
                let (c1, c2) = (g / col.max(1e-9), -g / (1.0 - col).max(1e-9));
                let p = p as f64 - col;
                let y = match code {
                    3 => c1 * (p + p - f),
                    0 => c2 * (p + p - f),
                    2 => rcpf * (c2 * (p * p) - c1 * ((p - f) * (p - f))),
                    5 => {
                        let t = p + 1.0 - f;
                        -((c2 * (t * t) - c1 * (p * p)) + g) * rcpf
                    }
                    7 => -((c1 * (omf * (p + p + omf))) + g) * rcpf,
                    4 => -((c2 * (omf * (p + p + omf))) + g) * rcpf,
                    _ => 0.0,
                };
                out((y + g) as f32)
            } else {
                let gdf = gain / f;
                out(match code {
                    3 => gain,
                    0 => -gain,
                    2 => gdf * 2.0 * (col - p) + gain,
                    5 => gdf * 2.0 * p - gain,
                    7 => gdf * 2.0 * (col - 1.0) + gain,
                    4 => gdf * 2.0 * col - gain,
                    _ => 0.0,
                })
            }
        }
        v2mode::SIN => {
            let cnt = slot.cnt;
            slot.cnt = cnt.wrapping_add(freq);
            if t.native_sin {
                return out(gain * crate::fm::sinf(utof23(cnt) * core::f32::consts::TAU));
            }
            let mut phase = cnt.wrapping_add(0x4000_0000);
            if phase & 0x8000_0000 != 0 {
                phase = !phase;
            }
            let x = f32::from_bits((phase >> 8) | 0x3f80_0000) * core::f32::consts::PI - 4.712_389;
            out(gain * fastsin(x))
        }
        v2mode::NOISE if t.old_noise => {
            slot.nseed = slot.nseed.wrapping_mul(214_013).wrapping_add(2_531_011);
            let n = f32::from_bits(((slot.nseed & 0xffff) << 7) | 0x4000_0000) - 3.0;
            let (sl, sb, f, r) = (slot.nf.l, slot.nf.b, t.nffrq, t.nfres);
            let bb = sb + sl * f;
            let h = (n - sl) * r - bb;
            let ll = sl + h * f;
            (slot.nf.l, slot.nf.b) = (ll, bb);
            out(gain * (h + bb + ll))
        }
        v2mode::NOISE => {
            let n = crate::v2::frandom(&mut slot.nseed);
            let h = slot.nf.step(n, t.nffrq, t.nfres);
            out(gain * (t.nfres * (slot.nf.l + h) + slot.nf.b))
        }
        v2mode::FM => {
            let cnt = slot.cnt;
            slot.cnt = cnt.wrapping_add(freq);
            let y = if t.native_sin {
                // the x87 `fistp`: round to nearest, and the "integer indefinite" 0x80000000 when out of range (a
                // loud modulator collapses the carrier, as in V2 before version 6)
                let m = libm::rint((mono as f64) * 4.0 * 2_147_483_648.0);
                let modi = if (-2_147_483_648.0..2_147_483_648.0).contains(&m) { m as i32 as u32 } else { 0x8000_0000 };
                gain * crate::fm::sinf(utof23(cnt.wrapping_add(modi)) * core::f32::consts::TAU)
            } else {
                gain * fastsinrc((mono * 2.0 + f32::from_bits((cnt >> 9) | 0x3f80_0000)) * core::f32::consts::TAU)
            };
            if t.ring { V2Out::Mul(y) } else { V2Out::Set(y) }
        }
        v2mode::AUX_A | v2mode::AUX_B => {
            // V2's `renderAux`: the bus's (l + r) · gain · 0.6 (fcgain), replacing the sum or ring-multiplying it
            let (l, r) = aux[(t.mode - v2mode::AUX_A) as usize];
            let a = (l + r) * gain * 0.6;
            V2Out::Set(if t.ring { a * mono } else { a })
        }
        _ => V2Out::Add(0.0),
    }
}

#[cfg(feature = "us-osc-wt")]
/// One sample of the unison wavetable: layer `k` reads the table at `|inc ± det| ± k·spread` on the left/right phase,
/// non-interpolated (as Tunefish does), drives and hard-clips each read, and sums the layers.
#[inline]
fn wavetable(wt: &mut Wt, inc: f32, det: f32, spread: f32, drive: f32, layers: usize) -> (f32, f32) {
    let wave = wt.table.wave();
    let (mut l, mut r) = (0.0f32, 0.0f32);
    let (f1, f2) = (inc + det, inc - det);
    for (k, ph) in wt.phase.iter_mut().take(layers).enumerate() {
        let (fl, fr) = (libm::fabsf(f1 + k as f32 * spread), libm::fabsf(f2 - k as f32 * spread));
        l += (wave[(ph[0] * 511.0) as usize & 511] * drive).max(-1.0).min(1.0);
        r += (wave[(ph[1] * 511.0) as usize & 511] * drive).max(-1.0).min(1.0);
        // a phase past 1 loses its whole turns (a phase of exactly 1 stays, as in Tunefish); the conversion to an
        // integer is the floor of a positive value, without the library call
        ph[0] += fl;
        ph[0] -= if ph[0] > 1.0 { ph[0] as u32 as f32 } else { 0.0 };
        ph[1] += fr;
        ph[1] -= if ph[1] > 1.0 { ph[1] as u32 as f32 } else { 0.0 };
    }
    (l, r)
}

/// `sinc(x) = sin(πx)/(πx)` for `x` in samples.
/// Full scale of the original's 16-bit samples.
const CD_FULL: f64 = 32767.0;

/// Wraps to a signed 16-bit value, as the original's word arithmetic does.
#[inline]
fn i16w(x: i64) -> i64 {
    (x + 32768).rem_euclid(65536) - 32768
}

/// A phase in turns as the original's signed 16-bit phase word.
#[inline]
fn phase16(turns: f64) -> i64 {
    i16w(libm::floor(turns * 65536.0) as i64)
}

/// `32767·u⁷` (the original's spike shaper, per-op in f64 instead of x87 24-bit precision).
#[inline]
fn spike7(u: f64) -> f64 {
    let u2 = u * u;
    let u4 = u2 * u2;
    let u6 = u4 * u2;
    u6 * CD_FULL * u
}

/// The spike of phase word `p` with the sign branch: falling `(32767 − p)/32767` for `p > 0`, rising `p/32767` below.
#[inline]
fn spike_signed(p: i64) -> f64 {
    let x = if p > 0 { CD_FULL - p as f64 } else { p as f64 };
    spike7(x / CD_FULL)
}

/// The spike wave (`0x411264` / `0x402B26`). Only the last of the original's voice loop survives (a quirk the patches
/// were tuned to): the base spike plus one copy offset by `t·V·pitch/128` (negated for even `V` in version 1; in
/// version 2 `t·V` is truncated to 16 bits and the base spike has no sign branch, so its negative half clips).
fn spike(t: f64, ph: i64, pitch: f64, voices: u8, v2: bool) -> i64 {
    let base = libm::rint(if v2 { spike7((CD_FULL - ph as f64) / CD_FULL) } else { spike_signed(ph) }) as i64;
    if voices == 0 {
        return base;
    }
    let k = voices as i64;
    let (t, pi) = (libm::floor(t) as i64, (libm::rint(pitch) as i64) & 0xFFFF_FFFF);
    let tk = if v2 { (t * k) & 0xFFFF } else { (t * k) & 0xFFFF_FFFF };
    let mut off = ((tk * pi) >> 7) & 0xFFFF_FFFF;
    if !v2 && k & 1 == 0 {
        off = (-off) & 0xFFFF_FFFF;
    }
    libm::rint(spike_signed(i16w(ph + off)) + base as f64) as i64
}

/// One output sample of a cauldron slot. The slot runs at its phrase clock (the original's rate times the playback
/// ratio: `clock` phrase frames per output frame) and is resampled to the output rate with a 4-point cubic
/// (Catmull-Rom), so its naive waveforms alias as the original's do at its own rate, then band-limited at the clock's
/// Nyquist ([`CauldronLp`]) when the clock is below the output rate. Latency: 2 phrase frames (and the low-pass's). `inc`
/// is the phase step in turns per output frame; the original pitch word is `inc·65536/clock`.
fn cauldron(slot: &mut Slot, inc: f64, c: &CauldronT) -> f32 {
    let clock = c.clock as f64;
    let pitch = inc * 65536.0 / clock;
    let pitch = match c.word {
        Some(d) => libm::rint(pitch) + d as f64,
        None => pitch,
    };
    slot.cd.frac += clock;
    while slot.cd.frac >= 1.0 {
        slot.cd.frac -= 1.0;
        let x = cauldron_frame(slot, pitch, c) as f32;
        let h = &mut slot.cd.h;
        *h = [h[1], h[2], h[3], x];
    }
    let ([a, b, cc, d], f) = (slot.cd.h, slot.cd.frac as f32);
    let y = b + 0.5 * f * (cc - a + f * (2.0 * a - 5.0 * b + 4.0 * cc - d + f * (3.0 * (b - cc) + d - a)));
    slot.cd.lp.run(y, clock)
}

/// One phrase frame of a cauldron slot (the original's frame: oscillator, then the filter), in output scale (16-bit
/// value / 32768), from the phase before this frame's step. `pitch` is the original pitch word.
pub(crate) fn cauldron_frame(slot: &mut Slot, pitch: f64, c: &CauldronT) -> f64 {
    // the pitch register: a glide holds the signed word (the phase steps the same; the second saw and spike do not)
    let pitch = if c.glide && pitch > 32767.5 { pitch - 65536.0 } else { pitch };
    let ph = phase16(slot.phase);
    let mut out = ph;
    if c.voices != 0 {
        // the second saw steps by the rounded detuned pitch word, as `fistp(pitch·1.02)`
        let cd = &mut slot.cd;
        cd.phase2 += libm::rint(pitch * 1.02) / 65536.0;
        cd.phase2 -= libm::floor(cd.phase2);
        out += phase16(cd.phase2);
    }
    // version 1 has saw, spike and noise; its other wave values play the saw
    let wave = if c.v2 || c.wave == 2 || c.wave == 4 { c.wave } else { 0 };
    out = match wave {
        1 => {
            if out > 0 {
                30000
            } else {
                -30000
            }
        }
        2 => spike(slot.cd.t, ph, pitch, c.voices, c.v2),
        3 => {
            if out > 31000 {
                30000
            } else {
                0
            }
        }
        4 => {
            let mut x = slot.cd.rng;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            slot.cd.rng = x;
            ((x & 0xFF_FFFF) % 64001) as i64 - 32000
        }
        5 if out > 0 => CD_FULL as i64 - out,
        _ => out,
    };
    if c.v2 {
        out = out.clamp(-32767, 32767);
    }
    slot.phase += pitch / 65536.0;
    slot.phase -= libm::floor(slot.phase);
    slot.cd.t += 1.0;
    let x = out as f64 / 32768.0;
    match slot.cd.flt {
        Some(f) => cauldron_filter(&mut slot.cd, x, &f),
        None => x,
    }
}

/// The phrase frame of a cauldron note-on from its velocity: `(velocity − 1)` eighth steps of 3530 phrase frames (the
/// converter writes step starts exactly; velocity 0 or 1 is the phrase start).
pub fn cauldron_t0(velocity: u8) -> f64 {
    velocity.saturating_sub(1) as f64 * (3530.0 / 8.0)
}

/// The original's resonant low-pass, one phrase frame: `v1 ·= d`, `c = v1 + b`, `y = (1 − q)·(y − c·(x + prev))`,
/// output `prev = y + prev` (DC gain −1, as the original). The original stores `y` and `prev` as 32-bit integers of
/// 16-bit samples (`fistp`), so a rising cutoff that turns the filter unstable saturates them instead of running
/// off to infinity: `CD_SATURATE` (2³¹/2¹⁵) in these units, and an envelope grown past any use stops growing.
pub(crate) fn cauldron_filter(cd: &mut CauldronSlot, x: f64, f: &CauldronF) -> f64 {
    cd.v1 = (cd.v1 * f.d).min(CD_SATURATE);
    let c = cd.v1 + f.b;
    let sat = |v: f64| if v.is_nan() { -CD_SATURATE } else { v.clamp(-CD_SATURATE, CD_SATURATE) };
    cd.y = sat(f.q1 * (cd.y - c * (x + cd.prev)));
    cd.prev = sat(cd.prev + cd.y);
    cd.prev
}

/// The original filter's integer range in sample units: 2³¹ over a full scale of 2¹⁵.
pub const CD_SATURATE: f64 = 65536.0;

#[inline]
fn sinc(x: f64) -> f64 {
    if x > 1e-7 || x < -1e-7 {
        sin_turns(x * 0.5) as f64 / (core::f64::consts::PI * x)
    } else {
        1.0
    }
}

/// One sample of Slaughter's oscillator, in turns instead of samples: the impulse sits at phase ½ and the pulse's
/// second impulse `pw` later, both as a sinc with the period as its lobe length. The leaky integral is the saw
/// (`wf` 0) or the pulse (`wf` 1) and is inverted like Slaughter's output stage (its gain is `−16·…`), so a negative
/// sign here keeps the original polarity when the gain law is positive.
#[inline]
fn blit(slot: &mut Slot, inc: f64, wf: f64, pw: f64, leak: f64) -> f32 {
    let inc = inc.max(1e-9);
    let p2 = slot.phase + pw;
    let p2 = p2 - libm::floor(p2);
    slot.phase += inc;
    slot.phase -= libm::floor(slot.phase);
    let dc = -0.996 * inc; // -0.498 / half period in samples
    slot.integral = leak * slot.integral + dc * (1.0 - wf) + sinc((slot.phase - 0.5) / inc) - sinc((p2 - 0.5) / inc) * wf;
    -slot.integral as f32
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_unstable_rising_filter_saturates_instead_of_running_off() {
        // Stash song0 record 4 held for minutes: the cutoff rises without end (decay 1.00003 per frame)
        use super::*;
        let f = CauldronF { a: 0.3, b: 0.009375, d: 1.0000300407409668, q1: 0.95 };
        let mut cd = CauldronSlot { phase2: 0.0, t: 0.0, rng: 0, y: 0.0, prev: 0.0, v1: f.a, h: [0.0; 4], frac: 0.0, ..Default::default() };
        let mut peak: f64 = 0.0;
        for n in 0..10_000_000u32 {
            let x = if n % 100 < 50 { 0.5 } else { -0.5 };
            let y = cauldron_filter(&mut cd, x, &f);
            assert!(y.is_finite(), "frame {n}");
            peak = peak.max(y.abs());
        }
        assert!(peak <= CD_SATURATE && peak > 1.0, "{peak}");
    }

    use super::*;
    use crate::law::host as inv;

    #[cfg(feature = "us-osc-wt")]
    mod wavetable_unit {
        use super::super::*;

        /// A table that reads back its own index: `wave[i] = i/511`.
        fn ramp() -> Wt {
            let mut w = Wt::new();
            w.table.load(&core::array::from_fn(|i| i as f32 / 511.0));
            w
        }

        #[test]
        fn layers_sum_and_move_at_their_own_rates() {
            let mut w = ramp();
            w.phase = [[0.0; 2]; crate::wt::MAX_LAYERS];
            for (k, ph) in w.phase.iter_mut().enumerate() {
                *ph = [0.1 * k as f32, 0.05 + 0.1 * k as f32];
            }
            let want_l: f32 = (0..3).map(|k| ((0.1 * k as f32 * 511.0) as usize) as f32 / 511.0).sum();
            let want_r: f32 = (0..3).map(|k| (((0.05 + 0.1 * k as f32) * 511.0) as usize) as f32 / 511.0).sum();
            let (l, r) = wavetable(&mut w, 0.01, 0.002, 0.001, 1.0, 3);
            assert!((l - want_l).abs() < 1e-6 && (r - want_r).abs() < 1e-6, "{l} {r} vs {want_l} {want_r}");
            // layer k advances by inc ± det ± k·spread; the right phase gets the opposite signs
            for k in 0..3 {
                let kf = k as f32;
                assert!((w.phase[k][0] - (0.1 * kf + 0.012 + 0.001 * kf)).abs() < 1e-6, "{k}: {}", w.phase[k][0]);
                assert!((w.phase[k][1] - (0.05 + 0.1 * kf + 0.008 - 0.001 * kf)).abs() < 1e-6, "{k}: {}", w.phase[k][1]);
            }
            assert!(w.phase[3][0] == 0.3 && (w.phase[3][1] - 0.35).abs() < 1e-6, "layers past the count stay put");
        }

        #[test]
        fn drive_hard_clips_each_read_and_a_negative_rate_runs_forward() {
            let mut w = Wt::new();
            w.table.load(&core::array::from_fn(|i| if i < 256 { 0.4 } else { -0.4 }));
            w.phase[0] = [0.1, 0.9];
            let (l, r) = wavetable(&mut w, 0.0, 0.0, 0.0, 10.0, 1);
            assert_eq!((l, r), (1.0, -1.0), "0.4 × 10 clips to ±1");
            // inc − det < 0: the right layer runs at |inc − det|, not backwards
            let mut w = ramp();
            w.phase[0] = [0.5, 0.5];
            wavetable(&mut w, 0.01, 0.03, 0.0, 1.0, 1);
            assert!((w.phase[0][0] - 0.54).abs() < 1e-6 && (w.phase[0][1] - 0.52).abs() < 1e-6, "{:?}", w.phase[0]);
        }

        #[test]
        fn a_phase_past_one_wraps() {
            let mut w = ramp();
            w.phase[0] = [0.99, 0.5];
            wavetable(&mut w, 0.05, 0.0, 0.0, 1.0, 1);
            assert!((w.phase[0][0] - 0.04).abs() < 1e-5, "{}", w.phase[0][0]);
        }
    }

    #[test]
    fn sinc_is_the_normalised_sinc() {
        crate::tables::init();
        for x in [0.0, 1e-9, 0.25, 0.5, 1.0, 1.5, 2.0, 7.3, -3.7, 150.25, -199.9] {
            let want = if x == 0.0 { 1.0 } else { (core::f64::consts::PI * x).sin() / (core::f64::consts::PI * x) };
            assert!((sinc(x) - want).abs() < 2e-6, "{x}: {} vs {want}", sinc(x));
        }
    }

    #[test]
    fn kinds_follow_the_type_parameter_and_the_build() {
        let kind = |ty: usize| {
            let mut v = pm::DEFAULTS;
            v[osc(1, o::TYPE)] = inv::inv_choice(ty, osc_type::COUNT);
            Kind::all(&v)
        };
        assert_eq!(Kind::all(&pm::DEFAULTS).unwrap(), [Kind::Off; 3]);
        assert_eq!(kind(osc_type::OFF).unwrap(), [Kind::Off; 3]);
        for (ty, want, on, id) in [
            (osc_type::PM, Kind::Pm, cfg!(feature = "us-osc-pm"), module::OSC_PM),
            (osc_type::BLIT, Kind::Blit, cfg!(feature = "us-osc-blit"), module::OSC_BLIT),
            (osc_type::SAMPLER, Kind::Sampler, cfg!(feature = "us-osc-sample"), module::OSC_SAMPLER),
        ] {
            match kind(ty) {
                Ok(k) => assert!(on && k == [Kind::Off, want, Kind::Off]),
                Err(e) => assert!(!on && e == LoadError::UnsupportedModule(id)),
            }
        }
        // the later kinds, stored at odd positions of the expanded choice
        let later = |ty: usize| {
            let mut v = pm::DEFAULTS;
            v[osc(1, o::TYPE)] = osc_type::encode(ty);
            Kind::all(&v)
        };
        for (ty, want, on, id) in [
            (osc_type::V2, Kind::V2, cfg!(feature = "us-osc-naive"), module::OSC_NAIVE),
            (osc_type::CAULDRON, Kind::Cauldron, cfg!(feature = "us-osc-cauldron"), module::OSC_CAULDRON),
        ] {
            match later(ty) {
                Ok(k) => assert!(on && k == [Kind::Off, want, Kind::Off]),
                Err(e) => assert!(!on && e == LoadError::UnsupportedModule(id)),
            }
        }
        #[cfg(feature = "us-osc-wt")]
        assert_eq!(kind(osc_type::WAVETABLE).unwrap(), [Kind::Off, Kind::Wt, Kind::Off]);
        #[cfg(not(feature = "us-osc-wt"))]
        assert_eq!(kind(osc_type::WAVETABLE).err(), Some(LoadError::UnsupportedModule(module::OSC_WAVETABLE)));
    }

    #[cfg(feature = "us-osc-cauldron")]
    mod cauldron {
        use super::super::*;
        mod vectors {
            include!("osc/cauldron_vectors.rs");
        }
        mod filter_vectors {
            include!("osc/cauldron_filter_vectors.rs");
        }

        /// The settings and pitch word of a slot's phrase-rate generator ([`cauldron_frame`]).
        struct Gen {
            cd: CauldronT,
            pitch: f64,
        }

        /// A cauldron slot after a note-on at phrase frame `t0`.
        fn slot(ver: u8, wave: u8, voices: u8, pitch: u32, t0: u32) -> (Slot, Gen) {
            let mut s = Slot::new(Kind::Cauldron, 0.25);
            s.start(0.25, 0.0, 0.0);
            let cd = CauldronT { wave, voices, v2: ver == 2, clock: 1.0, flt: None, phase2: None, glide: false, word: None, age: None };
            s.cauldron_start(t0 as f64, &cd);
            (s, Gen { cd, pitch: pitch as f64 })
        }

        /// The slot's next phrase frame in the original's 16-bit units (the slot scales by 1/32768).
        fn next(s: &mut Slot, g: &mut Gen) -> i32 {
            (cauldron_frame(s, g.pitch, &g.cd) * 32768.0).round() as i32
        }

        #[test]
        fn every_wave_matches_the_reference_oscillator() {
            for &(name, ver, wave, voices, pitch, t0, want) in vectors::CASES {
                let (mut s, mut r) = slot(ver, wave, voices, pitch, t0);
                // the spike's shaping runs in f64 here and in x87 24-bit precision in the original
                let tol = if wave == 2 { 3 } else { 0 };
                for (n, &w) in want.iter().enumerate() {
                    let got = next(&mut s, &mut r);
                    assert!((got - w).abs() <= tol, "{name} frame {n}: {got} vs {w}");
                }
            }
        }

        #[test]
        fn version_1_plays_a_saw_for_the_waves_it_lacks() {
            for wave in [1u8, 3, 5] {
                let (mut a, mut ra) = slot(1, wave, 0, 799, 0);
                let (mut b, mut rb) = slot(1, 0, 0, 799, 0);
                for _ in 0..200 {
                    assert_eq!(next(&mut a, &mut ra), next(&mut b, &mut rb), "wave {wave}");
                }
            }
        }

        #[test]
        fn a_note_on_resets_the_phase_and_keeps_the_detuned_pair() {
            let (mut s, mut r) = slot(1, 0, 2, 199, 0);
            for _ in 0..1000 {
                next(&mut s, &mut r);
            }
            let pair = s.cd.phase2;
            s.start(0.7, 0.0, 0.0);
            s.cauldron_start(0.0, &CauldronT::default());
            assert_eq!(s.cd.phase2, pair, "the second saw keeps running");
            // the first saw restarts at -32766: the first output is -32766 + the pair's next value
            let p2 = pair + (199.0f64 * 1.02).round() / 65536.0;
            let p2 = ((p2 - libm::floor(p2)) * 65536.0) as i32;
            let p2 = if p2 >= 32768 { p2 - 65536 } else { p2 };
            assert_eq!(next(&mut s, &mut r), -32766 + p2);
        }

        #[test]
        fn noise_is_in_the_original_range_and_repeats_with_the_seed() {
            let run = || {
                let (mut s, mut r) = slot(1, 4, 0, 799, 0);
                (0..4000).map(|_| next(&mut s, &mut r)).collect::<Vec<_>>()
            };
            let a = run();
            assert!(a.iter().all(|&v| (-32000..=32000).contains(&v)));
            assert!(a.iter().any(|&v| v < -20000) && a.iter().any(|&v| v > 20000));
            assert_eq!(a, run(), "same seed, same noise");
        }

        #[test]
        fn shape_fields_decode_with_clamping() {
            let mut sh = [0.0f32; pm::o::SHAPES];
            sh[pm::shape::CD_WAVE] = 0.4; // choice(0.4, 6) = 2: spike
            sh[pm::shape::CD_VOICES] = 0.31; // 3
            sh[pm::shape::CD_VERSION] = 1.0; // v2
            sh[pm::shape::CD_CLOCK] = 35441.16 / 1_048_576.0;
            let t = CauldronT::from_shape(&sh, 44100.0, 0.0, 0.0, 0.0);
            assert_eq!((t.wave, t.voices, t.v2), (2, 3, true));
            assert!((t.clock - 35441.16 / 44100.0).abs() < 1e-5);
            sh[pm::shape::CD_VOICES] = 7.0;
            sh[pm::shape::CD_WAVE] = -1.0;
            sh[pm::shape::CD_CLOCK] = 0.0;
            let t = CauldronT::from_shape(&sh, 44100.0, 0.0, 0.0, 0.0);
            assert_eq!((t.wave, t.voices), (0, 10));
            assert_eq!(t.clock, 1.0, "no clock: one phrase frame per output frame");
            assert_eq!(t.flt, None, "base and amount 0: no filter");
            // version 0.5: v2 in a glide (the pitch register holds the signed word); 1 and 0 are not
            for (p, v2, glide) in [(0.0, false, false), (0.5, true, true), (1.0, true, false)] {
                sh[pm::shape::CD_VERSION] = p;
                let t = CauldronT::from_shape(&sh, 44100.0, 0.0, 0.0, 0.0);
                assert_eq!((t.v2, t.glide), (v2, glide), "version {p}");
            }
        }

        #[test]
        fn the_word_byte_rounds_and_corrects_the_pitch_word() {
            // byte 0: the word follows the frequency (bends stay smooth); b ≥ 1: rint(word) + b − 128
            let sh = [0.0f32; pm::o::SHAPES];
            assert_eq!(CauldronT::from_shape(&sh, 44100.0, 0.0, 0.0, 0.0).word, None);
            assert_eq!(CauldronT::from_shape(&sh, 44100.0, 0.0, 0.0, 130.0 / 255.0).word, Some(2));
            for (word, step) in [(None, 51199.3), (Some(0), 51199.0), (Some(-3), 51196.0)] {
                let mut s = Slot::new(Kind::Cauldron, 0.25);
                s.start(0.25, 0.0, 0.0);
                let cd = CauldronT { word, ..CauldronT::default() };
                s.cauldron_start(0.0, &cd);
                let p0 = s.phase;
                cauldron(&mut s, 51199.3 / 65536.0, &cd);
                let got = (s.phase - p0).rem_euclid(1.0) * 65536.0;
                assert!((got - step).abs() < 1e-6, "{word:?}: {got} vs {step}");
            }
        }

        #[test]
        fn a_glide_steps_the_second_saw_by_the_signed_word() {
            // word 51199 (−14337 signed): the phase steps the same either way; the second saw by fistp(w·1.02) of the
            // register, 52223 unsigned or −14624 signed (the original's glide stores the signed interpolation)
            for (glide, step2) in [(false, 52223i64), (true, -14624)] {
                let (mut s, mut g) = slot(2, 0, 1, 51199, 0);
                g.cd.glide = glide;
                let (p0, q0) = (s.phase, s.cd.phase2);
                next(&mut s, &mut g);
                let turns = |a: f64, b: f64| ((a - b) * 65536.0).round().rem_euclid(65536.0) as i64;
                assert_eq!(turns(s.phase, p0), 51199, "glide {glide}");
                assert_eq!(turns(s.cd.phase2, q0), step2.rem_euclid(65536), "glide {glide}");
            }
        }

        /// Filter vectors: a slot with its filter on, retriggered once.
        fn filtered(ver: u8, wave: u8, voices: u8, pitch: u32, a: f64, b: f64, d: f64, q: f64) -> (Slot, Gen) {
            let cd = CauldronT { wave, voices, v2: ver == 2, clock: 1.0, flt: Some(CauldronF { a, b, d, q1: 1.0 - q }), phase2: None, glide: false, word: None, age: None };
            let mut s = Slot::new(Kind::Cauldron, 0.25);
            s.start(0.25, 0.0, 0.0);
            s.cauldron_start(0.0, &cd);
            (s, Gen { cd, pitch: pitch as f64 })
        }

        #[test]
        fn the_filter_matches_the_reference_filter() {
            for &(name, ver, wave, voices, pitch, a, b, d, q, restart, a2, b2, want) in filter_vectors::CASES {
                let (mut s, mut g) = filtered(ver, wave, voices, pitch, a, b, d, q);
                let peak = want.iter().map(|v| v.abs()).max().unwrap() as f64;
                let mut worst = 0.0f64;
                for (n, &w) in want.iter().enumerate() {
                    if n == restart {
                        // the retrigger: new base and amount, phase reset, prev 0, y runs on
                        g.cd.flt = Some(CauldronF { a: a2, b: b2, ..g.cd.flt.unwrap() });
                        s.start(0.7, 0.0, 0.0);
                        s.cauldron_start(n as f64, &g.cd); // the phrase frame runs on (the converter stores it)
                    }
                    worst = worst.max((cauldron_frame(&mut s, g.pitch, &g.cd) * 32768.0 - w as f64).abs());
                }
                // the original rounds its output to an integer every frame and works in 24-bit precision
                assert!(worst <= 1e-4 * peak, "{name}: worst error {worst:.1} of peak {peak}");
            }
        }

        #[test]
        fn the_filter_keeps_the_settings_of_its_note_on() {
            // the original computes a step's filter at its retrigger: a later change (the converter's next retrigger
            // byte, ramping towards the next note) reaches the filter only at the next note-on
            let (mut s, g) = filtered(1, 0, 2, 211, 0.4, 0.05, 0.9998, 0.05);
            let (mut r, _) = filtered(1, 0, 2, 211, 0.4, 0.05, 0.9998, 0.05);
            let moved = CauldronT { flt: Some(CauldronF { a: 1.6, b: 0.4, ..g.cd.flt.unwrap() }), ..g.cd };
            for _ in 0..2000 {
                assert_eq!(cauldron_frame(&mut s, g.pitch, &moved), cauldron_frame(&mut r, g.pitch, &g.cd));
            }
            // a note-on takes the new settings
            s.start(0.7, 0.0, 0.0);
            s.cauldron_start(2000.0, &moved);
            r.start(0.7, 0.0, 0.0);
            r.cauldron_start(2000.0, &moved);
            for _ in 0..200 {
                assert_eq!(cauldron_frame(&mut s, g.pitch, &g.cd), cauldron_frame(&mut r, g.pitch, &moved));
            }
        }

        /// The slot's output samples (`sample`) at `clock` phrase frames per output frame, pitch word 211.
        fn resampled(clock: f32, n: usize) -> std::vec::Vec<f32> {
            let mut s = Slot::new(Kind::Cauldron, 0.25);
            let cd = CauldronT { wave: 0, voices: 2, v2: false, clock, flt: Some(CauldronF { a: 0.08, b: 0.04, d: 0.999, q1: 0.9 }), phase2: None, glide: false, word: None, age: None };
            s.start(0.25, 0.0, 0.0);
            s.cauldron_start(0.0, &cd);
            let mut r = SlotRamps::default();
            r.set(&SlotT { inc: 211.0 * clock / 65536.0, gain: 1.0, mix: 1.0, pan_l: 1.0, pan_r: 1.0, cd, ..SlotT::default() });
            (0..n).map(|_| sample(&mut s, None, &smp::Source::None, &mut r, 0.0).0).collect()
        }

        #[test]
        fn the_slot_runs_at_its_phrase_clock_and_is_resampled() {
            // at one phrase frame per output frame: the phrase-rate frames, 2 frames late (the cubic's latency)
            let (mut s, mut g) = filtered(1, 0, 2, 211, 0.08, 0.04, 0.999, 0.1);
            let frames: std::vec::Vec<f32> = (0..400).map(|_| cauldron_frame(&mut s, g.pitch, &g.cd) as f32).collect();
            let _ = &mut g;
            let one = resampled(1.0, 402);
            assert_eq!(one[..2], [0.0, 0.0]);
            for n in 0..400 {
                assert!((one[n + 2] - frames[n]).abs() < 1e-6, "frame {n}: {} vs {}", one[n + 2], frames[n]);
            }
            // at half a phrase frame per output frame: the same signal at half speed, band-limited (CauldronLp): its
            // level matches the phrase frames'
            let half = resampled(0.5, 806);
            let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
            let (a, b) = (rms(&half[200..800]), rms(&frames[100..400]));
            assert!((a / b - 1.0).abs() < 0.1, "{a} vs {b}");
        }

        #[test]
        fn the_reconstruction_low_pass_cuts_above_the_clock_s_nyquist() {
            // clock 0.25: cutoff 0.1125 of the output rate. A sine well inside passes, one past the clock's Nyquist
            // (0.125) is cut, and at clock 1 the filter is bypassed
            let gain = |clock: f64, f: f32| {
                let mut lp = CauldronLp::default();
                let x: std::vec::Vec<f32> = (0..8000).map(|n| crate::fm::sinf(2.0 * core::f32::consts::PI * f * n as f32)).collect();
                let y: std::vec::Vec<f32> = x.iter().map(|&v| lp.run(v, clock)).collect();
                let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
                rms(&y[4000..]) / rms(&x[4000..])
            };
            assert!((gain(0.25, 0.03) - 1.0).abs() < 0.02, "{}", gain(0.25, 0.03));
            assert!(gain(0.25, 0.3) < 0.03, "{}", gain(0.25, 0.3));
            assert_eq!(gain(1.0, 0.4), 1.0);
        }

        #[test]
        fn filter_shape_fields_decode() {
            let mut sh = [0.0f32; pm::o::SHAPES];
            sh[pm::shape::CD_CLOCK] = 22050.0 / 1_048_576.0; // half a phrase frame per output frame at 44.1 kHz
            sh[pm::shape::CD_F_BASE] = 0.04 / 8.0;
            sh[pm::shape::CD_F_AMOUNT] = 0.08 / 8.0;
            sh[pm::shape::CD_F_DECAY] = 0.5 + 0.1; // d = 0.999
            sh[pm::shape::CD_F_DAMP] = 0.1;
            let f = CauldronT::from_shape(&sh, 44100.0, 0.0, 0.0, 0.0).flt.unwrap();
            let close = |x: f64, y: f64| (x - y).abs() < 1e-6;
            assert!(close(f.b, 0.04) && close(f.a, 0.08) && close(f.q1, 0.9) && close(f.d, 0.999), "per phrase frame: {f:?}");
            sh[pm::shape::CD_F_DECAY] = 0.49; // d = 1.0001: a rising cutoff
            assert!(CauldronT::from_shape(&sh, 44100.0, 0.0, 0.0, 0.0).flt.unwrap().d > 1.0);
        }

        #[test]
        fn the_phase_byte_sets_the_second_saw_at_a_note_on() {
            // `o::PM_OUT` byte b: 0 keeps the second saw running, b ≥ 1 sets it to (b − 1)/255 turn at a note-on
            let sh = [0.0f32; pm::o::SHAPES];
            let at = |b: u32| CauldronT::from_shape(&sh, 44100.0, 0.0, b as f32 / 255.0, 0.0).phase2;
            assert_eq!(at(0), None);
            assert_eq!(at(1), Some(0.0));
            assert_eq!(at(129), Some(128.0 / 255.0));
            let (mut s, mut g) = slot(1, 0, 2, 199, 0);
            for _ in 0..1000 {
                next(&mut s, &mut g);
            }
            let cd = CauldronT { phase2: Some(0.25), ..g.cd };
            s.start(0.7, 0.0, 0.0);
            s.cauldron_start(0.0, &cd);
            assert_eq!(s.cd.phase2, 0.25);
        }

        #[test]
        fn the_age_byte_starts_the_filter_envelope_decayed() {
            // `o::TRACKING` byte b (a cauldron slot always tracks the note): 0 restarts the envelope at the amount (a
            // retrigger), b ≥ 1 enters a phrase (b − 1) eighth steps after its last retrigger: the envelope already
            // decayed `a·d^n` (a jump into a step, a ping-pong pass's reversed step)
            assert_eq!(cauldron_age(0.0), None);
            assert_eq!(cauldron_age(1.0 / 255.0), Some(0.0));
            assert_eq!(cauldron_age(17.0 / 255.0), Some(16.0 * 3530.0 / 8.0));
            let mut sh = [0.0f32; pm::o::SHAPES];
            sh[pm::shape::CD_F_AMOUNT] = 0.4 / 8.0;
            sh[pm::shape::CD_F_DECAY] = 0.5 + 0.02; // d = 0.9998
            let f = CauldronT::from_shape(&sh, 44100.0, 0.0, 0.0, 0.0).flt.unwrap();
            let (mut s, _) = slot(2, 0, 2, 199, 0);
            s.cauldron_start(0.0, &CauldronT { flt: Some(f), ..CauldronT::default() });
            assert!((s.cd.v1 - f.a).abs() < 1e-12, "a retrigger: {}", s.cd.v1);
            let n = 5.0 * 3530.0;
            s.cauldron_start(n, &CauldronT { flt: Some(f), age: Some(n), ..CauldronT::default() });
            let want = f.a * f.d.powf(n);
            assert!((s.cd.v1 - want).abs() < 1e-9 * f.a, "{} vs {want}", s.cd.v1);
        }

        #[test]
        fn the_retrigger_byte_compounds_base_and_amount() {
            // byte 2k + accent: base and amount times mul^k, each product rounded to f32 as the original stores it,
            // then the accent (×4 in version 1, ×2 in version 2)
            let mut sh = [0.0f32; pm::o::SHAPES];
            sh[pm::shape::CD_F_BASE] = 0.005 / 8.0;
            sh[pm::shape::CD_F_AMOUNT] = 0.01 / 8.0;
            let mul_p = 1.1 / 4.0;
            let (b0, a0, mul) = ((0.005f32 / 8.0) as f64 * 8.0, (0.01f32 / 8.0) as f64 * 8.0, (1.1f32 / 4.0) as f64 * 4.0);
            let at = |byte: u32, v2: bool| {
                let mut sh = sh;
                sh[pm::shape::CD_RETRIG] = byte as f32 / 255.0;
                sh[pm::shape::CD_VERSION] = if v2 { 1.0 } else { 0.0 };
                CauldronT::from_shape(&sh, 44100.0, mul_p, 0.0, 0.0).flt.unwrap()
            };
            let f = at(0, false);
            assert_eq!((f.a, f.b), (a0, b0), "k 0: the stored values");
            let (mut a, mut b) = (a0, b0);
            for _ in 0..5 {
                a = (a * mul) as f32 as f64;
                b = (b * mul) as f32 as f64;
            }
            let f = at(10, false);
            assert_eq!((f.a, f.b), (a, b), "k 5");
            let f = at(11, false);
            assert_eq!((f.a, f.b), (a * 4.0, b * 4.0), "k 5, accent, version 1");
            let f = at(11, true);
            assert_eq!((f.a, f.b), (a * 2.0, b * 2.0), "k 5, accent, version 2");
            assert!(at(255, false).a > at(254, false).a, "the largest byte is k 127 with the accent");
        }

        #[test]
        fn the_phrase_frame_comes_from_the_velocity_in_eighth_steps() {
            assert_eq!(cauldron_t0(1), 0.0);
            assert_eq!(cauldron_t0(1 + 8 * 5), 5.0 * 3530.0, "a step start is exact");
            assert_eq!(cauldron_t0(1 + 8 * 5 + 3), 5.0 * 3530.0 + 3.0 * 3530.0 / 8.0);
            assert_eq!(cauldron_t0(0), 0.0);
        }
    }
}
