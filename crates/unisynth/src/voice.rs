//! One voice (design D4–D5). A voice keeps two copies of its control state: `start`, at the beginning of the current
//! control segment, and `end`, one segment length later. The segment ends on the next absolute 32-frame grid frame
//! (or earlier: a voice that starts mid-block has a short first segment). Control outputs are [`Ramp`]s that move
//! linearly from the values at `start` to the values at `end`. An event that arrives inside a segment (note-off,
//! slide) first advances `start` to the event frame, changes it, and recomputes `end`, so the audio depends only on
//! absolute frames, never on how the caller split its render calls.

use crate::env::{Env, EnvP, Stage};
use crate::lfo::{shape as lfo_shape, Lfo};
use crate::modmx::{target, Matrix, Query, Sources};
use crate::flt;
use crate::noise::{self, Noise};
use crate::osc::{self, Kind, Slot, SlotRamps, SlotT, Wt};
use crate::smp::{self, Sample};
use crate::params::{self as pm, v};
use crate::ramp::Ramp;
use crate::rng::Xorshift;
use synth_core::rng::Rng;
use crate::law;
use synth_core::voice::{Voice as PoolVoice, VoiceState};

/// Control grid in frames (absolute song frames, multiples of this).
/// Frames between wavetable checks (Tunefish rebuilds every 4 blocks of 512).
const WT_CHECK: u32 = 2048;
/// One Tunefish control block: the wavetable shape is read this far ahead (see `wt_build`).
#[cfg(feature = "us-osc-wt")]
const WT_STEP: u32 = 512;

/// Frequency of note 60 (middle C), the sampler's unity pitch.
const MIDDLE_C: f32 = 261.625_56;

pub const CONTROL: u32 = 32;
/// A releasing voice whose output stays below this mean magnitude (about −72 dBFS) for `QUIET_FRAMES` is freed, as the
/// original Tunefish voices are when their block level falls under a threshold; an exponential release otherwise keeps
/// a voice alive for seconds after it can no longer be heard (a fast arpeggio then holds twice the original voices).
const QUIET: f32 = 5e-4;
const QUIET_FRAMES: u32 = 256;
/// The fine control period a voice uses just after its note-on and note-off (see `FINE_SPAN`).
pub const FINE: u32 = 8;
/// Frames after a note-on or note-off during which a voice runs on the fine grid: percussive envelopes last a few
/// milliseconds, and sampling them every 32 frames loses their peak and lengthens their tail (a hat is 2 ms: about
/// 1.7 dB quieter than the original per-sample envelope).
const FINE_SPAN: u32 = 256;

/// The voice-wide pitch offset in semitones: transpose plus the pitch bend.
fn tuning(p: &[f32; pm::COUNT]) -> f32 {
    law::transpose(p[v::TRANSPOSE]) + law::bend_semitones(p[pm::ctl::BEND], p[pm::ctl::BEND_RANGE])
}

/// What every voice call sees of its instrument.
#[derive(Clone, Copy)]
pub struct Shared<'c> {
    pub p: &'c [f32; pm::COUNT],
    pub rate: f32,
}

/// What `VoicePool` hands to note events: the shared view, the instrument's random stream and the position of the
/// event inside its control block.
pub struct NoteCx<'c> {
    pub sh: Shared<'c>,
    pub rng: &'c mut Xorshift,
    /// Absolute song frame of the event.
    pub frame: u64,
    /// An all-notes-off: releases even a one-shot voice.
    pub force: bool,
}

impl NoteCx<'_> {
    /// Position of the event inside its control block.
    pub fn phase(&self) -> u32 {
        (self.frame % CONTROL as u64) as u32
    }
}

#[derive(Clone, Copy, Debug)]
enum Move {
    Snap,
    /// Semitones per frame (> 0).
    Linear(f32),
    /// Time constant in frames (> 0).
    Expo(f32),
}

/// Voice pitch in semitones with its movement towards a target (slide = linear, glide = exponential).
#[derive(Clone, Copy, Debug)]
struct Pitch {
    cur: f32,
    target: f32,
    mv: Move,
}

impl Pitch {
    fn snap(note: f32) -> Self {
        Pitch { cur: note, target: note, mv: Move::Snap }
    }

    fn advance(&mut self, dt: u32) {
        match self.mv {
            Move::Snap => self.cur = self.target,
            Move::Linear(per_frame) => {
                let d = self.target - self.cur;
                let step = per_frame * dt as f32;
                if libm::fabsf(d) <= step {
                    *self = Pitch::snap(self.target);
                } else {
                    self.cur += step.copysign(d);
                }
            }
            Move::Expo(tau) => {
                let d = (self.target - self.cur) * crate::fm::expf(-(dt as f32) / tau);
                if libm::fabsf(d) < 1e-3 {
                    *self = Pitch::snap(self.target);
                } else {
                    self.cur = self.target - d;
                }
            }
        }
    }

    fn slide_to(&mut self, note: f32, frames: f32) {
        self.target = note;
        self.mv = if frames >= 1.0 { Move::Linear(libm::fabsf(note - self.cur) / frames) } else { Move::Snap };
        if matches!(self.mv, Move::Linear(d) if d == 0.0) {
            self.mv = Move::Snap;
        }
    }
}

/// Values fixed at note-on.
#[derive(Clone, Copy, Debug)]
struct Fixed {
    envp: [EnvP; pm::ENV_SLOTS],
    /// Velocity `0..1`.
    vel: f32,
    /// Note `0..1` (mod source 9).
    key: f32,
}

/// The part of the voice that advances with time at control rate.
#[derive(Clone, Copy, Debug)]
struct Ctl {
    pitch: Pitch,
    env: [Env; pm::ENV_SLOTS],
    lfo: [Lfo; pm::LFO_SLOTS],
    mm: Matrix,
}

impl Ctl {
    fn advance(&mut self, dt: u32, sh: &Shared, fx: &Fixed) {
        self.pitch.advance(dt);
        let secs = dt as f32 / sh.rate;
        for (i, e) in self.env.iter_mut().enumerate() {
            e.advance(secs, &fx.envp[i], self.mm.scale(target::ENV_DECAY + i as u8));
        }
        for (k, l) in self.lfo.iter_mut().enumerate() {
            l.advance(secs, law::lfo_rate(sh.p[pm::lfo(k, pm::l::RATE)]));
        }
        self.eval(sh, fx);
    }

    /// One mod matrix pass at the current sources.
    fn eval(&mut self, sh: &Shared, fx: &Fixed) {
        let shape = |k: usize| self.lfo[k].shape(law::choice(sh.p[pm::lfo(k, pm::l::SHAPE)], lfo_shape::COUNT));
        let src = Sources {
            env: core::array::from_fn(|i| self.env[i].value()),
            lfo_shape: [shape(0), shape(1)],
            wheel: sh.p[pm::ctl::WHEEL],
            velocity: fx.vel,
            key: fx.key,
        };
        self.mm.eval(sh.p, &src);
    }

    fn release(&mut self) {
        self.env.iter_mut().for_each(Env::release);
    }
}

/// Control outputs at one control state.
struct Targets {
    /// Voice gain after the oscillators: master, mod, velocity, envelope 1.
    amp: f32,
    slot: [SlotT; pm::OSC_SLOTS],
    /// Noise level (linear) and the pan of the noise.
    noise_level: f32,
    noise_pan: (f32, f32),
    noise_band: bool,
    noise_width: f32,
    noise_lp: [f32; flt::NC],
    noise_hp: [f32; flt::NC],
    /// Kind and coefficients of the filter slots.
    flt: [(flt::Kind, [f32; flt::NC], bool); pm::FILT_SLOTS],
}

pub struct Voice<'a> {
    on: bool,
    note: u8,
    vel: f32,
    detune: f32,
    pan: f32,
    rng: Xorshift,
    has_pitch: bool,
    fx: Fixed,
    start: Ctl,
    end: Ctl,
    seg_pos: u32,
    seg_len: u32,
    amp: Ramp,
    noise: Noise,
    noise_level: Ramp,
    noise_pan: [Ramp; 2],
    noise_band: bool,
    noise_width: f32,
    flt: [flt::Slot; pm::FILT_SLOTS],
    slots: [Slot; pm::OSC_SLOTS],
    /// Pool-backed wavetable state of the slots that are wavetables.
    wt: [Option<&'a mut Wt>; pm::OSC_SLOTS],
    /// Sample data of the sampler slots (a DLS-instrument slot holds the region its last note-on picked).
    smp: [Sample<'a>; pm::OSC_SLOTS],
    /// The instruments of the DLS-instrument slots.
    insts: smp::Insts<'a>,
    /// Root key, rate and gain of the sample each sampler slot plays.
    adj: [smp::Adj; pm::OSC_SLOTS],
    /// Frames since note-on, and the age of the next wavetable check (every 2048 frames).
    age: u32,
    next_check: u32,
    /// Frames of fine control left (see `FINE_SPAN`).
    fine: u32,
    /// Summed output magnitude and frames of the current quiet check window (see `QUIET`).
    lvl: f32,
    lvl_n: u32,
    ramps: [SlotRamps; pm::OSC_SLOTS],
    #[cfg(test)]
    pub(crate) starts: u32,
}

impl<'a> Voice<'a> {
    /// `rng` draws the slots' start phases (Slaughter draws them once, when its voices are built).
    pub fn new(kinds: [Kind; pm::OSC_SLOTS], rng: &mut Xorshift, wt: [Option<&'a mut Wt>; pm::OSC_SLOTS], smp: [Sample<'a>; pm::OSC_SLOTS], insts: smp::Insts<'a>) -> Self {
        let c = Ctl { pitch: Pitch::snap(0.0), env: [Env::new(); pm::ENV_SLOTS], lfo: [Lfo::new(0.0); pm::LFO_SLOTS], mm: Matrix::new() };
        Voice {
            on: false,
            note: 0,
            vel: 0.0,
            detune: 0.0,
            pan: 0.5,
            rng: Xorshift::new(1),
            has_pitch: false,
            fx: Fixed { envp: [EnvP::DEFAULT; pm::ENV_SLOTS], vel: 0.0, key: 0.0 },
            start: c,
            end: c,
            seg_pos: 0,
            seg_len: 0,
            amp: Ramp::new(0.0),
            noise: Noise::new(),
            noise_level: Ramp::new(0.0),
            noise_pan: [Ramp::new(0.0); 2],
            noise_band: false,
            noise_width: 1.0,
            flt: [flt::Slot::new(); pm::FILT_SLOTS],
            slots: core::array::from_fn(|i| Slot::new(kinds[i], rng.next_f32() as f64)),
            wt,
            smp,
            insts,
            adj: [smp::Adj::UNITY; pm::OSC_SLOTS],
            age: 0,
            next_check: WT_CHECK,
            fine: 0,
            lvl: 0.0,
            lvl_n: 0,
            ramps: [SlotRamps::default(); pm::OSC_SLOTS],
            #[cfg(test)]
            starts: 0,
        }
    }

    /// The pitch (semitones) the voice is heading for or sliding through, at the end of its control segment.
    pub fn pitch(&self) -> f32 {
        self.end.pitch.cur
    }

    /// Value of envelope `i` at the end of the voice's control segment.
    pub fn env_value(&self, i: usize) -> f32 {
        self.end.env[i].value()
    }

    /// LFO `k`'s phase in turns at the end of the voice's control segment.
    pub fn lfo_phase(&self, k: usize) -> f64 {
        self.end.lfo[k].phase
    }

    /// Mod matrix Scale factor and Offset sum aimed at target `t`, at the end of the control segment.
    pub fn mod_scale(&self, t: u8) -> f32 {
        self.end.mm.scale(t)
    }

    pub fn mod_offset(&self, t: u8) -> f32 {
        self.end.mm.offset(t)
    }

    /// The playable cycle of wavetable slot `i`.
    #[cfg(all(test, feature = "us-osc-pm", feature = "us-osc-wt", feature = "us-env-timed", feature = "us-env-rate"))]
    pub(crate) fn wt_wave(&self, i: usize) -> Option<[f32; crate::wt::BINS]> {
        self.wt[i].as_deref().map(|w| *w.table.wave())
    }

    /// Phase of oscillator slot `i` in turns.
    #[cfg(all(test, feature = "us-osc-pm", feature = "us-osc-blit", feature = "us-env-timed", feature = "us-env-rate"))]
    pub(crate) fn slot_phase(&self, i: usize) -> f64 {
        self.slots[i].phase
    }

    pub fn velocity(&self) -> f32 {
        self.vel
    }

    /// Unison detune in semitones.
    pub fn detune(&self) -> f32 {
        self.detune
    }

    /// Control values at control state `c`.
    fn targets(&self, c: &Ctl, sh: &Shared) -> Targets {
        let tab = c.mm.tab();
        let (p, mm) = (sh.p, &tab);
        let key = c.pitch.cur + self.detune + tuning(p) + mm.offset(target::PITCH);
        let sens = p[v::VELOCITY];
        let env1 = if law::choice(p[v::ENV1_AT_OUTPUT], 2) == 1 { c.env[0].value() } else { 1.0 };
        let gain = (law::gain(p[v::MASTER]) * mm.scale(target::GAIN) + mm.offset(target::GAIN)).max(0.0);
        let amp = gain * (1.0 - sens + sens * self.vel) * env1;
        // the unison pan and the voice pan move every slot
        let shift = (self.pan - 0.5) + (p[v::PAN] * mm.scale(target::PAN) + mm.offset(target::PAN) - 0.5);
        // a slot that is off (its type is structural) is never rendered: nothing to compute for it
        let slot = core::array::from_fn(|i| if self.slots[i].kind == Kind::Off { SlotT::default() } else { self.slot_targets(c, mm, sh, i, key, shift) });
        // noise: level through the matrix, the voice's pan, the optional band
        let level = if cfg!(feature = "us-noise") {
            (law::gain(p[v::NOISE_LEVEL]) * mm.scale(target::NOISE_LEVEL) + mm.offset(target::NOISE_LEVEL)).max(0.0)
        } else {
            0.0
        };
        let noise_pan = law::pan((0.5 + shift).max(0.0).min(1.0));
        let cut = |base: f32, t: u8| (law::cutoff((base * mm.scale(t)).max(0.0).min(1.0)) + mm.offset(t)).max(0.0).min(19_980.0);
        let band_p = |k: usize| p[pm::NOISE + k];
        let noise_band = law::choice(band_p(pm::n::BAND_ON), 2) == 1;
        let band = |kind, k, t| if noise_band { flt::coefs(kind, cut(band_p(k), t), noise::BAND_RES, sh.rate) } else { [0.0; flt::NC] };
        let noise_lp = band(noise::LP, pm::n::LP, target::NOISE_LP);
        let noise_hp = band(noise::HP, pm::n::HP, target::NOISE_HP);
        // filter slots: cutoff (Scale on the normalized value, Offset in Hz) and resonance
        let flt = core::array::from_fn(|i| {
            let kind = flt::Kind::of(law::choice(p[pm::filt(i, pm::f::TYPE)], pm::filter_type::COUNT));
            if kind == flt::Kind::Off {
                return (kind, [0.0; flt::NC], false);
            }
            let hz = cut(p[pm::filt(i, pm::f::CUTOFF)], target::filter_cutoff(i));
            let tr = target::filter_resonance(i);
            let res = (p[pm::filt(i, pm::f::RESONANCE)] * mm.scale(tr) + mm.offset(tr)).max(0.0).min(1.0);
            (kind, flt::coefs(kind, hz, res, sh.rate), flt::transparent(kind, hz, res))
        });
        let noise_width = sh.p[pm::ext::NOISE_WIDTH];
        Targets { amp, slot, noise_level: level, noise_pan, noise_band, noise_width, noise_lp, noise_hp, flt }
    }

    /// Control values of oscillator slot `i`. `key` is the voice pitch in semitones, `shift` the voice's pan offset.
    fn slot_targets(&self, _c: &Ctl, mm: &impl Query, sh: &Shared, i: usize, key: f32, shift: f32) -> SlotT {
        let p = sh.p;
        let f = |fld: usize| pm::osc(i, fld);
        let t = |fld: u8| target::osc(i, fld);
        let semis = key
            + law::coarse(p[f(pm::o::COARSE)])
            + law::fine(p[f(pm::o::FINE)])
            + 12.0 * law::octave(p[f(pm::o::OCTAVE)])
            + mm.offset(t(target::OSC_PITCH));
        let ratio = law::ratio(p[f(pm::o::RATIO)]) + law::ratio_fine(p[f(pm::o::RATIO_FINE)]);
        let hz_scale = mm.scale(target::PITCH) * mm.scale(t(target::OSC_PITCH));
        let hz = if law::choice(p[f(pm::o::TRACKING)], 2) == 1 {
            law::fixed_hz(p[f(pm::o::FIXED_HZ)]) * hz_scale
        } else {
            440.0 * crate::fm::exp2f((semis - 69.0) / 12.0) * ratio * hz_scale
        };
        let norm = |fld: u8, v: f32| (v * mm.scale(t(fld))).max(0.0).min(1.0);
        let gain = (law::gain(p[f(pm::o::GAIN)]) * mm.scale(t(target::OSC_GAIN)) + mm.offset(t(target::OSC_GAIN))).max(0.0);
        let mix = (norm(target::OSC_MIX, p[f(pm::o::MIX)]) + mm.offset(t(target::OSC_MIX))).max(0.0).min(1.0);
        let pan = (p[f(pm::o::PAN)] * mm.scale(t(target::OSC_PAN)) + mm.offset(t(target::OSC_PAN)) + shift).max(0.0).min(1.0);
        let (pan_l, pan_r) = law::pan(pan);
        let pm_out = (law::pm_out(norm(target::OSC_PM_OUT, p[f(pm::o::PM_OUT)])) + mm.offset(t(target::OSC_PM_OUT))).max(0.0);
        let feedback = (law::feedback(norm(target::OSC_FEEDBACK, p[f(pm::o::FEEDBACK)])) + mm.offset(t(target::OSC_FEEDBACK))).max(0.0);
        let wf = self.shape_value(mm, sh, i, pm::shape::PM_WAVEFORM);
        let pw = self.shape_value(mm, sh, i, pm::shape::BLIT_PULSE_WIDTH);
        let sv = |k: usize| self.shape_value(mm, sh, i, k);
        // the leak keeps its corner (about 14 Hz) at every rate
        let leak = crate::fm::powf(0.998, 44100.0 / sh.rate);
        let det = law::wt_detune(sv(pm::shape::WT_LAYER_DETUNE)) * hz_scale / sh.rate;
        let spread = law::wt_spread(sv(pm::shape::WT_LAYER_SPREAD)) / sh.rate;
        let drive = law::wt_drive(sv(pm::shape::WT_DRIVE));
        let layers = law::count(p[f(pm::o::SHAPE + pm::shape::WT_LAYERS)], 1, crate::wt_layers());
        // a sampler steps through its data by `2^((note − root)/12)` sample units per frame (root 60 unless a DLS region
        // says otherwise, unity there), whatever the song rate; its fixed-frequency mode is relative to note 60 as well
        let inc = if self.slots[i].kind == Kind::Sampler {
            let a = &self.adj[i];
            if law::choice(p[f(pm::o::TRACKING)], 2) == 1 {
                hz / MIDDLE_C * a.rate
            } else {
                crate::fm::exp2f((semis - a.root) / 12.0) * ratio * hz_scale * a.rate
            }
        } else {
            hz / sh.rate
        };
        let smp = smp::Cfg::from_shape(&core::array::from_fn(|k| self.shape_value(mm, sh, i, k)), &self.smp[i]);
        SlotT { inc, gain, mix, pan_l, pan_r, pm_out, feedback, wf, pw, leak, det, spread, drive, layers: layers as usize, smp }
    }

    /// Type-dependent shape value `k` of slot `i` after the matrix, clamped to `0..=1`.
    fn shape_value(&self, mm: &impl Query, sh: &Shared, i: usize, k: usize) -> f32 {
        let t = target::osc(i, target::OSC_SHAPE + k as u8);
        (sh.p[pm::osc(i, pm::o::SHAPE + k)] * mm.scale(t) + mm.offset(t)).max(0.0).min(1.0)
    }

    /// Rebuilds the wavetable of every wavetable slot (the table is rebuilt only when its spectrum inputs changed; the
    /// phase stage and inverse FFT run every time, as in Tunefish). The shape comes from the control state one
    /// Tunefish block (512 frames at 44.1 kHz) ahead: Tunefish steps its matrix at the start of a block and builds the
    /// table from the stepped values, so a note-on table already sees the attack's first step (an attack-0 envelope
    /// at full level), not the envelope at rest. Built from rest instead, an envelope-scaled harmonics or bandwidth
    /// gave a dull first 2048 frames.
    #[cfg(feature = "us-osc-wt")]
    fn wt_build(&mut self, sh: &Shared) {
        use crate::wt::{band_bins, harmonics, Shape};
        use pm::shape as sp;
        if !self.slots.iter().any(|s| s.kind == Kind::Wt) {
            return;
        }
        let mut ahead = self.end;
        ahead.advance(((WT_STEP as f32 * sh.rate / 44_100.0) as u32).max(1), sh, &self.fx);
        for i in 0..pm::OSC_SLOTS {
            if self.slots[i].kind != Kind::Wt {
                continue;
            }
            let c = &ahead;
            let sv = |k: usize| self.shape_value(&c.mm, sh, i, k);
            let key = c.pitch.cur + self.detune + tuning(sh.p) + c.mm.offset(target::PITCH);
            let shape = Shape {
                harmonics: harmonics(sv(sp::WT_HARMONICS)),
                bandwidth: sv(sp::WT_BANDWIDTH),
                damp: sv(sp::WT_DAMP),
                scale: sv(sp::WT_SCALE) * 4.0,
                bins: band_bins(440.0 * crate::fm::exp2f((key - 69.0) / 12.0), sh.rate),
            };
            let g = sv(sp::WT_MOD);
            if let Some(w) = self.wt[i].as_deref_mut() {
                w.table.update(&shape);
                w.table.render(g, &mut w.mu);
            }
        }
    }

    #[cfg(not(feature = "us-osc-wt"))]
    fn wt_build(&mut self, _: &Shared) {}

    /// Voice reset: every slot and table back to its loaded state, drawing the start phases from `rng` again.
    pub fn reset(&mut self, kinds: [Kind; pm::OSC_SLOTS], rng: &mut Xorshift) {
        let mut wt = core::mem::take(&mut self.wt);
        wt.iter_mut().flatten().for_each(|w| **w = Wt::new());
        let (smp, insts) = (self.smp, self.insts);
        *self = Voice::new(kinds, rng, wt, smp, insts);
    }

    fn ramps_set(&mut self, t: &Targets) {
        self.amp.set(t.amp);
        self.noise_level.set(t.noise_level);
        self.noise_pan[0].set(t.noise_pan.0);
        self.noise_pan[1].set(t.noise_pan.1);
        self.noise_band = t.noise_band;
        self.noise_width = t.noise_width;
        self.noise.lp.set(noise::LP, &t.noise_lp);
        self.noise.hp.set(noise::HP, &t.noise_hp);
        for (f, (k, c, pass)) in self.flt.iter_mut().zip(&t.flt) {
            f.pass(*pass);
            f.set(*k, c);
        }
        for (r, s) in self.ramps.iter_mut().zip(&t.slot) {
            r.set(s);
        }
    }

    fn ramps_to(&mut self, t: &Targets, frames: u32) {
        self.amp.to(t.amp, frames);
        self.noise_level.to(t.noise_level, frames);
        self.noise_pan[0].to(t.noise_pan.0, frames);
        self.noise_pan[1].to(t.noise_pan.1, frames);
        self.noise_band = t.noise_band;
        self.noise_width = t.noise_width;
        self.noise.lp.to(noise::LP, &t.noise_lp, frames);
        self.noise.hp.to(noise::HP, &t.noise_hp, frames);
        for (f, (k, c, pass)) in self.flt.iter_mut().zip(&t.flt) {
            f.pass(*pass);
            f.to(*k, c, frames);
        }
        for (r, s) in self.ramps.iter_mut().zip(&t.slot) {
            r.to(s, frames);
        }
    }

    fn ramps_retarget(&mut self, t: &Targets, frames: u32) {
        self.amp.retarget(t.amp, frames);
        self.noise_level.retarget(t.noise_level, frames);
        self.noise_pan[0].retarget(t.noise_pan.0, frames);
        self.noise_pan[1].retarget(t.noise_pan.1, frames);
        self.noise_band = t.noise_band;
        self.noise_width = t.noise_width;
        self.noise.lp.retarget(noise::LP, &t.noise_lp, frames);
        self.noise.hp.retarget(noise::HP, &t.noise_hp, frames);
        for (f, (k, c, pass)) in self.flt.iter_mut().zip(&t.flt) {
            f.pass(*pass);
            f.retarget(*k, c, frames);
        }
        for (r, s) in self.ramps.iter_mut().zip(&t.slot) {
            r.retarget(s, frames);
        }
    }

    /// Every sampler slot has run out (and there is at least one): a one-shot voice is finished.
    fn samplers_done(&self) -> bool {
        let mut any = false;
        for s in &self.slots {
            if s.kind == Kind::Sampler {
                any = true;
                if s.smp.active {
                    return false;
                }
            }
        }
        any
    }

    /// Starts a control segment of `len` frames from the committed state.
    fn begin_segment(&mut self, sh: &Shared, len: u32) {
        self.start = self.end;
        if self.start.env[0].finished() || (law::choice(sh.p[v::ONE_SHOT], 2) == 1 && self.samplers_done()) {
            self.on = false; // envelope 1 has run out: the voice is free
            return;
        }
        if self.start.env[0].stage() == Stage::Release {
            if self.lvl_n >= QUIET_FRAMES {
                if self.lvl < QUIET * 2.0 * self.lvl_n as f32 {
                    self.on = false; // too quiet to hear and still dying away: the voice is free
                    return;
                }
                (self.lvl, self.lvl_n) = (0.0, 0);
            }
        } else {
            (self.lvl, self.lvl_n) = (0.0, 0);
        }
        self.end.advance(len, sh, &self.fx);
        self.age += len;
        self.fine = self.fine.saturating_sub(len);
        if self.age >= self.next_check {
            self.next_check += WT_CHECK;
            self.wt_build(sh);
        }
        self.seg_pos = 0;
        self.seg_len = len;
        let t = self.targets(&self.end, sh);
        self.ramps_to(&t, len);
    }

    /// Runs on every grid frame for a sounding voice.
    pub fn tick(&mut self, sh: &Shared, frame: u64) {
        if self.seg_pos >= self.seg_len {
            let len = if self.fine > 0 { FINE } else { CONTROL - (frame % CONTROL as u64) as u32 };
            self.begin_segment(sh, len);
        }
    }

    /// Frames left in the current control segment.
    pub fn left(&self) -> u32 {
        self.seg_len.saturating_sub(self.seg_pos)
    }

    /// Changes the control state at the current frame (see the module doc).
    fn edit(&mut self, sh: &Shared, f: impl FnOnce(&mut Ctl)) {
        if self.seg_pos >= self.seg_len {
            f(&mut self.end);
            return;
        }
        if self.seg_pos > 0 {
            self.start.advance(self.seg_pos, sh, &self.fx);
            self.seg_len -= self.seg_pos;
            self.seg_pos = 0;
        }
        f(&mut self.start);
        self.end = self.start;
        self.end.advance(self.seg_len, sh, &self.fx);
        let t = self.targets(&self.end, sh);
        self.ramps_retarget(&t, self.seg_len);
    }

    /// Adds this voice's output for `l.len()` frames (never across a control grid frame).
    pub fn render(&mut self, l: &mut [f32], r: &mut [f32]) {
        for (lo, ro) in l.iter_mut().zip(r.iter_mut()) {
            let amp = self.amp.next();
            let (mut bl, mut br, mut send) = (0.0f32, 0.0f32, 0.0f32);
            for (((slot, ramps), wt), smp) in
                self.slots.iter_mut().zip(self.ramps.iter_mut()).zip(self.wt.iter_mut()).zip(self.smp.iter())
            {
                if slot.kind == Kind::Off {
                    send = 0.0;
                    continue;
                }
                let (sl, sr, next) = osc::sample(slot, wt.as_deref_mut(), &smp.src, ramps, send);
                bl += sl;
                br += sr;
                send = next;
            }
            let level = self.noise_level.next();
            let (pl, pr) = (self.noise_pan[0].next(), self.noise_pan[1].next());
            if cfg!(feature = "us-noise") && level > 0.0 {
                let (nl, nr) = self.noise.next(level, self.noise_band, self.noise_width);
                bl += nl * pl;
                br += nr * pr;
            }
            for f in self.flt.iter_mut() {
                (bl, br) = f.process(bl, br);
            }
            let (ol, or) = (bl * amp, br * amp);
            *lo += ol;
            *ro += or;
            self.lvl += ol.abs() + or.abs();
        }
        self.lvl_n += l.len() as u32;
        self.seg_pos += l.len() as u32;
    }
}

impl VoiceState for Voice<'_> {
    fn is_on(&self) -> bool {
        self.on
    }
    fn note(&self) -> u8 {
        self.note
    }
    fn kill(&mut self) {
        self.on = false;
    }
}

impl<'a, 'c> PoolVoice<NoteCx<'c>> for Voice<'a> {
    fn note_on(&mut self, note: u8, velocity: u8, detune: f32, pan: f32, cx: &mut NoteCx<'c>) {
        let sh = cx.sh;
        self.on = true;
        self.note = note;
        self.vel = velocity as f32 / 128.0;
        self.detune = detune;
        self.pan = pan;
        for i in 0..pm::OSC_SLOTS {
            if let (Kind::Sampler, Some(inst)) = (self.slots[i].kind, self.insts[i]) {
                (self.smp[i], self.adj[i]) = smp::pick(inst, note, velocity, sh.rate);
            }
        }
        self.rng = Xorshift::new(cx.rng.next_u32());
        let n = note as f32;
        let glide = sh.p[v::GLIDE];
        let pitch = if glide > 0.0 && self.has_pitch {
            let tau = (law::time(glide) * sh.rate).max(1e-3);
            Pitch { cur: self.end.pitch.cur, target: n, mv: Move::Expo(tau) }
        } else {
            Pitch::snap(n)
        };
        self.has_pitch = true;
        self.fx = Fixed {
            envp: core::array::from_fn(|i| EnvP::read(sh.p, i)),
            vel: self.vel,
            key: note as f32 / 127.0,
        };
        let mut env = [Env::new(); pm::ENV_SLOTS];
        env.iter_mut().for_each(Env::trigger);
        let lfo = core::array::from_fn(|k| {
            let synced = law::choice(sh.p[pm::lfo(k, pm::l::SYNC)], 2) == 1;
            let hz = law::lfo_rate(sh.p[pm::lfo(k, pm::l::RATE)]) as f64;
            // free-running: the phase of an LFO that has run since frame 0, so every voice shares it
            Lfo::new(if synced { 0.0 } else { hz * cx.frame as f64 / sh.rate as f64 })
        });
        self.end = Ctl { pitch, env, lfo, mm: Matrix::new() };
        self.end.eval(&sh, &self.fx);
        self.start = self.end;
        let phase = self.rng.next_f32() as f64; // Falcon: one draw sets every operator's phase
        let t = self.targets(&self.end, &sh);
        let spread = sh.p[v::START_SPREAD];
        for (s, st) in self.slots.iter_mut().zip(&t.slot) {
            s.start(phase, spread, st.inc);
        }
        #[cfg(feature = "us-osc-wt")]
        for w in self.wt.iter_mut().flatten() {
            // Tunefish: per layer a left phase and a right phase a little ahead of it, then the drift value
            for layer in w.phase.iter_mut() {
                let base = self.rng.next_f32();
                let right = base + self.rng.next_f32() * 0.1;
                *layer = [base, if right >= 1.0 { right - 1.0 } else { right }];
            }
            w.mu = self.rng.next_f32() * 100.0;
        }
        self.noise.start(self.rng.next_u32());
        for i in 0..pm::OSC_SLOTS {
            if self.slots[i].kind == Kind::Sampler {
                let start = self.shape_value(&self.end.mm, &sh, i, pm::shape::SMP_START);
                let reverse = law::choice(self.shape_value(&self.end.mm, &sh, i, pm::shape::SMP_REVERSE), 2) == 1;
                self.slots[i].smp.start(&self.smp[i].src, start, reverse);
            }
        }
        self.age = 0;
        self.next_check = WT_CHECK;
        self.fine = FINE_SPAN;
        (self.lvl, self.lvl_n) = (0.0, 0);
        self.wt_build(&sh);
        self.ramps_set(&t);
        if cx.frame % FINE as u64 == 0 {
            // the grid tick at this very frame starts the first segment
            self.seg_pos = 0;
            self.seg_len = 0;
        } else {
            self.begin_segment(&sh, FINE - (cx.frame % FINE as u64) as u32);
        }
        #[cfg(test)]
        {
            self.starts += 1;
        }
    }

    fn note_off(&mut self, cx: &mut NoteCx<'c>) {
        if !cx.force && law::choice(cx.sh.p[v::ONE_SHOT], 2) == 1 {
            return;
        }
        let sh = cx.sh;
        self.edit(&sh, |c| c.release());
        self.fine = FINE_SPAN;
    }

    fn slide(&mut self, note: u8, cx: &mut NoteCx<'c>) {
        let sh = cx.sh;
        let frames = law::time(sh.p[v::SLIDE]) * sh.rate;
        self.note = note;
        self.edit(&sh, |c| c.pitch.slide_to(note as f32, frames));
    }
}
