//! One voice (design D4–D5). A voice keeps two copies of its control state: `start`, at the beginning of the current
//! control segment, and `end`, one segment length later. The segment ends on the next absolute 32-frame grid frame
//! (or earlier: a voice that starts mid-block has a short first segment). Control outputs are [`Ramp`]s that move
//! linearly from the values at `start` to the values at `end`. An event that arrives inside a segment (note-off,
//! slide) first advances `start` to the event frame, changes it, and recomputes `end`, so the audio depends only on
//! absolute frames, never on how the caller split its render calls.

use crate::env::{Env, EnvP, Stage};
use crate::lfo::{sync as lfo_sync, Lfo, LfoP};
use crate::modmx::{target, Matrix, Query, Sources};
use crate::flt;
use crate::noise::{self, Noise};
use crate::osc::{self, Kind, Slot, SlotRamps, SlotT, Wt};
use crate::pts::Pts;
use crate::smp::{self, Sample};
use crate::params::{self as pm, v};
use crate::ramp::Ramp;
use crate::rng::Xorshift;
use crate::v2::Opt;
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

/// The voice-wide pitch offset in semitones: transpose (on the voice's 7-bit step law, `step − 64`) plus the pitch
/// bend.
fn tuning(p: &[f32; pm::COUNT]) -> f32 {
    let tr = if VoiceSet::of(p).steps { pm::v2_raw(p[v::TRANSPOSE]) - 64.0 } else { law::transpose(p[v::TRANSPOSE]) };
    tr + law::bend_semitones(p[pm::ctl::BEND], p[pm::ctl::BEND_RANGE])
}

/// The voice output settings (design D6): `vs::AMP_SOURCE`, `PAN_STAGE`, `DC_FILTER`, `LAW` (`us-voice-out`).
#[derive(Clone, Copy)]
struct VoiceSet {
    /// The amplitude is envelope 1 alone (not master × velocity × envelope 1).
    env_amp: bool,
    /// The pan comes once, after the voice drive, on the mono sum (not per slot).
    pan_after: bool,
    /// The output DC filter.
    dc: bool,
    /// The transpose is on the 7-bit step law.
    steps: bool,
}

impl VoiceSet {
    #[inline]
    fn of(p: &[f32; pm::COUNT]) -> VoiceSet {
        let on = |i: usize| cfg!(feature = "us-voice-out") && law::choice(p[i], 2) == 1;
        VoiceSet { env_amp: on(pm::vs::AMP_SOURCE), pan_after: on(pm::vs::PAN_STAGE), dc: on(pm::vs::DC_FILTER), steps: on(pm::vs::LAW) }
    }
}

/// V2's noise seeds of the three oscillators (quirk `fixed_seed`: 0).
const V2_SEEDS: [u32; 3] = [0xdead_beef, 0xbaad_f00d, 0xd3ad_c0de];

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
    /// Note-on counter of the instrument: the voice with the highest is the newest (the channel stage reads it).
    pub seq: u32,
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
    /// Linear in frequency (the mini-cauldron's glide, change add-cauldron-xm-import): from `from` semitones, `t`
    /// of the way (0..1), advancing `per_frame` per frame.
    Hz { from: f32, t: f32, per_frame: f32 },
    /// Elevated's pitch drop (change add-elevated-import D3): the frequency `b + (f − b)·k^(n+1)` Hz at frame `n`
    /// after note-on, `f` the note's, `b` the base; `l2k = log2 k`.
    #[cfg(feature = "us-voice-drop")]
    Drop { f: f32, b: f32, l2k: f32, n: u32 },
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

    /// A pitch drop from note `note` towards `base` Hz (per-sample factor `exp2(l2k)`), at its note-on frame.
    #[cfg(feature = "us-voice-drop")]
    fn drop(note: f32, base: f32, l2k: f32) -> Self {
        let f = 440.0 * crate::fm::exp2f((note - 69.0) / 12.0);
        let mut p = Pitch { cur: note, target: note, mv: Move::Drop { f, b: base, l2k, n: 0 } };
        p.advance(0);
        p
    }

    fn advance(&mut self, dt: u32) {
        #[cfg(feature = "us-voice-drop")]
        if let Move::Drop { f, b, l2k, n } = self.mv {
            let n = n.saturating_add(dt);
            let hz = (b + (f - b) * crate::fm::exp2f(l2k * (n + 1) as f32)).max(1e-3);
            self.cur = 69.0 + 12.0 * libm::log2f(hz / 440.0);
            self.target = self.cur;
            self.mv = Move::Drop { f, b, l2k, n };
            return;
        }
        if !cfg!(feature = "us-voice-glide") {
            self.cur = self.target; // without glide/slide every move is a snap
            return;
        }
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
            Move::Hz { from, t, per_frame } => {
                let t = t + per_frame * dt as f32;
                if t >= 1.0 {
                    *self = Pitch::snap(self.target);
                } else {
                    let ratio = crate::fm::exp2f((self.target - from) / 12.0);
                    self.cur = from + 12.0 * libm::log2f(1.0 + (ratio - 1.0) * t);
                    self.mv = Move::Hz { from, t, per_frame };
                }
            }
            #[cfg(feature = "us-voice-drop")]
            Move::Drop { .. } => {}
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

    /// A slide linear in frequency over `frames` (the mini-cauldron's glide).
    fn slide_hz_to(&mut self, note: f32, frames: f32) {
        let from = self.cur;
        self.target = note;
        self.mv = if frames >= 1.0 && note != from { Move::Hz { from, t: 0.0, per_frame: 1.0 / frames } } else { Move::Snap };
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
    /// Note number (V2's note source).
    note: f32,
    /// A voice with a clocked slot: transpose and bend are a playback rate (the slot's phrase clock follows them),
    /// so its envelopes run at that rate too (an XM slide resamples the precalc sample, fades and all).
    resample: bool,
}

/// The part of the voice that advances with time at control rate.
#[derive(Clone, Copy, Debug)]
struct Ctl {
    pitch: Pitch,
    env: [Env; pm::ENV_SLOTS],
    lfo: [Lfo; pm::LFO_SLOTS],
    mm: Matrix,
}

/// The parameters with the Add slots of `mm` applied (V2's rule, before any law), or `None` when no Add slot routes.
fn added(mm: &Matrix, p: &[f32; pm::COUNT]) -> Option<[f32; pm::COUNT]> {
    if !cfg!(feature = "us-matrix-add") || !mm.has_adds() {
        return None;
    }
    let mut v = *p;
    mm.apply(&mut v, false);
    Some(v)
}

impl Ctl {
    fn advance(&mut self, dt: u32, sh: &Shared, fx: &Fixed) {
        let eff = added(&self.mm, sh.p);
        let base = sh;
        let sh = &Shared { p: eff.as_ref().unwrap_or(sh.p), rate: sh.rate };
        self.pitch.advance(dt);
        let secs = dt as f32 / sh.rate;
        let env_secs = if fx.resample { secs * crate::fm::exp2f(tuning(sh.p) / 12.0) } else { secs };
        for (i, e) in self.env.iter_mut().enumerate() {
            // V2 re-reads its envelope every frame (the matrix may move it); the other modes keep the note-on values
            let live;
            let ep = if fx.envp[i].live() {
                live = EnvP::read(sh.p, i);
                &live
            } else {
                &fx.envp[i]
            };
            e.advance(env_secs, ep, self.mm.scale(target::ENV_DECAY + i as u8));
        }
        if cfg!(feature = "us-voice-lfo") {
            for (k, l) in self.lfo.iter_mut().enumerate() {
                l.advance(secs, LfoP::rate_of(sh.p, k), LfoP::one_shot_of(sh.p, k));
            }
        }
        self.eval(base, fx);
    }

    /// One mod matrix pass at the current sources.
    fn eval(&mut self, sh: &Shared, fx: &Fixed) {
        // without `us-voice-lfo` no patch that reads an LFO loads (`check_modules`): the value is never used
        let shape = |k: usize| if cfg!(feature = "us-voice-lfo") { self.lfo[k].value(LfoP::shape_of(sh.p, k)) } else { 0.0 };
        let lfo_shape = [shape(0), shape(1)];
        // an Add slot sees the value for the LFO's polarity (positive only without the LFO settings)
        let add_view = |k: usize| {
            if cfg!(feature = "us-lfo-frame") {
                let (gain, dc) = LfoP::view_of(sh.p, k);
                lfo_shape[k] * gain + dc
            } else {
                sh.p[pm::lfo(k, pm::l::DEPTH)] * lfo_shape[k]
            }
        };
        let src = Sources {
            env: core::array::from_fn(|i| self.env[i].value()),
            lfo_shape,
            // only Add slots read these (`us-matrix-add`)
            lfo_add: if cfg!(feature = "us-matrix-add") { core::array::from_fn(add_view) } else { [0.0; pm::LFO_SLOTS] },
            wheel: sh.p[pm::ctl::WHEEL],
            velocity: fx.vel,
            key: fx.key,
            cc: if cfg!(feature = "us-matrix-add") { core::array::from_fn(|k| sh.p[pm::CC + k]) } else { [0.0; pm::CC_COUNT] },
            note: fx.note,
        };
        self.mm.eval(sh.p, &src);
    }

    /// A note-off: every envelope releases, a linear timed one ends at once.
    fn release(&mut self, fx: &Fixed) {
        self.env.iter_mut().zip(&fx.envp).for_each(|(e, ep)| e.note_off(ep));
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
    /// Filter slots 1–2 in parallel: their gains (`v2::ROUTING`, `us-flt-parallel`).
    par: Opt<Option<(f32, f32)>>,
    /// The voice distortion (`us-drive`).
    dist: Opt<crate::v2::DistP>,
    /// The V2 voice output stage: left/right volume and whether the DC filter runs (format version 6).
    v2out: Opt<Option<(f32, f32, bool)>>,
}

/// A filter cutoff in Hz after the matrix: Scale on the normalized value, Offset in Hz.
#[inline(always)]
fn cut_hz(mm: &impl Query, base: f32, t: u8) -> f32 {
    (law::cutoff((base * mm.scale(t)).max(0.0).min(1.0)) + mm.offset(t)).max(0.0).min(19_980.0)
}

/// A clocked restart's declick fade (see [`Declick`]).
#[cfg(feature = "us-osc-clocked")]
const DECLICK_MS: f32 = 1.0;

/// A note-on restarts a clocked voice with its filter output at 0: when it cuts a sounding note, the step from that
/// note's last output frame to the new note's first is carried into the new note and faded out over `len` frames
/// (`DECLICK_MS`); `pending` until the first frame measures it. A note from silence (a fresh voice, or a retrigger
/// after the old note's fade: below `SILENT`) carries nothing, keeping its instant start.
#[cfg(feature = "us-osc-clocked")]
#[derive(Clone, Copy, Default)]
struct Declick {
    last: (f32, f32),
    step: (f32, f32),
    n: u32,
    len: u32,
    pending: bool,
}

/// −80 dBFS: below it a cut note counts as silent.
#[cfg(feature = "us-osc-clocked")]
const SILENT: f32 = 1e-4;

#[cfg(all(test, feature = "us-osc-clocked"))]
#[test]
fn a_declick_carries_only_a_sounding_note() {
    // from silence (last 0) the new note passes unchanged: its instant start stays
    let mut d = Declick::default();
    d.start(4);
    assert_eq!((0..6).map(|i| d.next(i as f32, -(i as f32))).collect::<Vec<_>>(), (0..6).map(|i| (i as f32, -(i as f32))).collect::<Vec<_>>());
    // nor from a faded note (below −80 dB)
    let mut d = Declick { last: (5e-5, -5e-5), ..Declick::default() };
    d.start(4);
    assert_eq!(d.next(0.5, 0.5), (0.5, 0.5));
    // a sounding cut: the step from the old note's last frame to the new note's first fades out linearly, so the
    // join is continuous whatever the new note starts at
    let mut d = Declick { last: (0.8, -0.4), ..Declick::default() };
    d.start(4);
    let got: Vec<(f32, f32)> = (0..6).map(|_| d.next(0.2, 0.2)).collect();
    let want = [(0.8, -0.4), (0.65, -0.25), (0.5, -0.1), (0.35, 0.05), (0.2, 0.2), (0.2, 0.2)];
    assert!(got.iter().zip(want).all(|(g, w)| (g.0 - w.0).abs() < 1e-6 && (g.1 - w.1).abs() < 1e-6), "{got:?}");
}

#[cfg(feature = "us-osc-clocked")]
impl Declick {
    fn start(&mut self, len: u32) {
        self.len = len;
        self.n = 0;
        self.pending = self.last.0.abs().max(self.last.1.abs()) >= SILENT;
    }

    #[inline]
    fn next(&mut self, l: f32, r: f32) -> (f32, f32) {
        if self.pending {
            self.pending = false;
            self.step = (self.last.0 - l, self.last.1 - r);
            self.n = self.len;
        }
        let (mut l, mut r) = (l, r);
        if self.n > 0 {
            let g = self.n as f32 / self.len as f32;
            l += self.step.0 * g;
            r += self.step.1 * g;
            self.n -= 1;
        }
        self.last = (l, r);
        (l, r)
    }
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
    pub(crate) slots: [Slot; pm::OSC_SLOTS],
    /// Pool-backed wavetable state of the slots that are wavetables.
    wt: [Option<&'a mut Wt>; pm::OSC_SLOTS],
    /// Pool-backed per-voice state of the point and PAD slots (change add-tf3-import).
    pts: [Option<&'a mut Pts>; pm::OSC_SLOTS],
    /// The instrument's PAD table of each PAD slot (shared by the voices).
    pads: [Option<&'a [f32]>; pm::OSC_SLOTS],
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
    par: Opt<Option<(f32, f32)>>,
    dist_p: Opt<crate::v2::DistP>,
    dist: Opt<crate::v2::Dist>,
    v2out: Opt<Option<(f32, f32, bool)>>,
    dcf: Opt<crate::v2::Dcf>,
    dcf_r: Opt<f32>,
    /// Note-on order (`NoteCx::seq`).
    pub seq: u32,
    /// A clocked restart's declick (only with `us-osc-clocked`).
    #[cfg(feature = "us-osc-clocked")]
    dk: Declick,
    #[cfg(test)]
    pub(crate) starts: u32,
}

/// Whether a slot kind is a PAD table.
#[inline]
fn is_pad(k: Kind) -> bool {
    #[cfg(feature = "us-osc-pad")]
    return k == Kind::Pad;
    #[cfg(not(feature = "us-osc-pad"))]
    {
        let _ = k;
        false
    }
}

/// Whether a slot kind is a point oscillator.
#[inline]
fn is_pts(k: Kind) -> bool {
    #[cfg(feature = "us-osc-points")]
    return k == Kind::Pts;
    #[cfg(not(feature = "us-osc-points"))]
    {
        let _ = k;
        false
    }
}

impl<'a> Voice<'a> {
    /// `rng` draws the slots' start phases (Slaughter draws them once, when its voices are built).
    pub fn new(
        kinds: [Kind; pm::OSC_SLOTS],
        rng: &mut Xorshift,
        wt: [Option<&'a mut Wt>; pm::OSC_SLOTS],
        pts: [Option<&'a mut Pts>; pm::OSC_SLOTS],
        pads: [Option<&'a [f32]>; pm::OSC_SLOTS],
        smp: [Sample<'a>; pm::OSC_SLOTS],
        insts: smp::Insts<'a>,
    ) -> Self {
        let c = Ctl { pitch: Pitch::snap(0.0), env: [Env::new(); pm::ENV_SLOTS], lfo: [Lfo::new(0.0); pm::LFO_SLOTS], mm: Matrix::new() };
        Voice {
            on: false,
            note: 0,
            vel: 0.0,
            detune: 0.0,
            pan: 0.5,
            rng: Xorshift::new(1),
            has_pitch: false,
            #[cfg(feature = "us-osc-clocked")]
            dk: Declick::default(),
            fx: Fixed { envp: [EnvP::DEFAULT; pm::ENV_SLOTS], vel: 0.0, key: 0.0, note: 0.0, resample: false },
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
            pts,
            pads,
            smp,
            insts,
            adj: [smp::Adj::UNITY; pm::OSC_SLOTS],
            age: 0,
            next_check: WT_CHECK,
            fine: 0,
            lvl: 0.0,
            lvl_n: 0,
            ramps: [SlotRamps::default(); pm::OSC_SLOTS],
            par: Opt::default(),
            dist_p: Opt::default(),
            dist: Opt::default(),
            v2out: Opt::default(),
            dcf: Opt::default(),
            dcf_r: Opt::new(1.0),
            seq: 0,
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
    #[cfg(all(test, feature = "us-osc-pm", feature = "us-osc-wt", feature = "us-env-seconds", feature = "us-env-rate"))]
    pub(crate) fn wt_wave(&self, i: usize) -> Option<[f32; crate::wt::BINS]> {
        self.wt[i].as_deref().map(|w| *w.table.wave())
    }

    /// Phase of oscillator slot `i` in turns.
    #[cfg(all(test, feature = "us-osc-pm", feature = "us-osc-blit", feature = "us-env-seconds", feature = "us-env-rate"))]
    pub(crate) fn slot_phase(&self, i: usize) -> f64 {
        self.slots[i].phase
    }

    /// The probe's snapshot of this voice at the end of its control segment (read only).
    #[cfg(feature = "probe")]
    pub fn snap(&self, sh: &Shared) -> crate::probe::VoiceSnap {
        // the matrix is read directly (not through `added`/`tab`): a second caller of those would change how the
        // compiler inlines them into `targets`, which costs render time even when nothing is watched. Cutoffs that V2
        // Add routes move are reported without that move.
        let (c, p) = (&self.end, sh.p);
        crate::probe::VoiceSnap {
            id: self.seq,
            note: self.note,
            amp: self.amp.end(),
            released: matches!(c.env[0].stage(), Stage::Release | Stage::Finished),
            age_s: self.age as f32 / sh.rate,
            env: core::array::from_fn(|i| (c.env[i].value(), c.env[i].stage())),
            lfo: core::array::from_fn(|k| c.lfo[k].phase as f32),
            smp: core::array::from_fn(|i| {
                (self.slots[i].kind == Kind::Sampler).then(|| {
                    let n = self.smp[i].src.len();
                    if n > 0 { (self.slots[i].smp.pos() / n as f64) as f32 } else { 0.0 }
                })
            }),
            cutoff: core::array::from_fn(|i| match flt::Kind::of(pm::filter_type::decode(p[pm::filt(i, pm::f::TYPE)]), flt::Forms::of(p, i)) {
                k if k == flt::Kind::Off || (flt::Forms::of(p, i).steps && flt::has_steps(k)) => None,
                _ => {
                    // normalized cutoff after the Scale routes and the Offset routes in Hz: `law::cutoff` is applied by
                    // the reader (`probe::cutoff_hz`), so the render path's call of it stays the only one
                    let t = target::filter_cutoff(i);
                    Some(((p[pm::filt(i, pm::f::CUTOFF)] * c.mm.scale(t)).max(0.0).min(1.0), c.mm.offset(t)))
                }
            }),
        }
    }

    /// Applies this voice's channel Add routes to `v` (the channel stage of the newest voice, V2's
    /// `storeChanValues`).
    pub fn apply_channel(&self, v: &mut [f32; pm::COUNT]) {
        self.end.mm.apply_from(v, true, true);
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
        let eff = added(&c.mm, sh.p);
        let sh = &Shared { p: eff.as_ref().unwrap_or(sh.p), rate: sh.rate };
        let tab = c.mm.tab();
        let (p, mm) = (sh.p, &tab);
        let key = c.pitch.cur + self.detune + tuning(p) + mm.offset(target::PITCH);
        let sens = p[v::VELOCITY];
        let env1 = if law::choice(p[v::ENV1_AT_OUTPUT], 2) == 1 { c.env[0].value() } else { 1.0 };
        let gain = (law::gain(p[v::MASTER]) * mm.scale(target::GAIN) + mm.offset(target::GAIN)).max(0.0);
        let vs = VoiceSet::of(p);
        let exact = cfg!(feature = "us-quirk-exact-math") && pm::quirk_on(p, pm::quirk::EXACT_MATH);
        let amp = if vs.env_amp { c.env[0].value() } else { gain * (1.0 - sens + sens * self.vel) * env1 };
        let vpan = p[v::PAN] * mm.scale(target::PAN) + mm.offset(target::PAN);
        // the unison pan and the voice pan move every slot, unless the pan comes after the drive
        let shift = (self.pan - 0.5) + if vs.pan_after { 0.0 } else { vpan - 0.5 };
        let v2out = if vs.pan_after {
            let (l, r) = law::pan((vpan.max(0.0).min(1.0) + self.pan - 0.5).max(0.0).min(1.0));
            Some((l, r, vs.dc))
        } else {
            None
        };
        let raw = |i: usize| pm::v2_raw(p[i]);
        let par = if cfg!(feature = "us-flt-parallel") && law::choice(p[pm::vs::ROUTING], 2) == 1 {
            let x = (raw(pm::vs::BALANCE) - 64.0) / 64.0;
            Some(if x >= 0.0 { (1.0 - x, 1.0) } else { (1.0, 1.0 + x) })
        } else {
            None
        };
        let dist = if cfg!(feature = "us-drive") {
            let d = pm::DRIVE;
            crate::v2::DistP::new(raw(d), raw(d + pm::dist::GAIN), raw(d + pm::dist::P1), raw(d + pm::dist::P2), sh.rate).native_atan(exact)
        } else {
            crate::v2::DistP::default()
        };
        // a slot that is off (its type is structural) is never rendered: nothing to compute for it
        let slot = core::array::from_fn(|i| if self.slots[i].kind == Kind::Off { SlotT::default() } else { self.slot_targets(c, mm, sh, i, key, shift) });
        // noise: level through the matrix, the voice's pan, the optional band
        let level = if cfg!(feature = "us-noise") {
            (law::gain(p[v::NOISE_LEVEL]) * mm.scale(target::NOISE_LEVEL) + mm.offset(target::NOISE_LEVEL)).max(0.0)
        } else {
            0.0
        };
        let noise_pan = law::pan((0.5 + shift).max(0.0).min(1.0));
        let cut = |base: f32, t: u8| cut_hz(mm, base, t);
        let band_p = |k: usize| p[pm::NOISE + k];
        let noise_band = law::choice(band_p(pm::n::BAND_ON), 2) == 1;
        let band = |kind, k, t| if noise_band { flt::coefs(kind, cut(band_p(k), t), noise::BAND_RES, sh.rate, flt::Forms::PLAIN) } else { [0.0; flt::NC] };
        let noise_lp = band(noise::LP, pm::n::LP, target::NOISE_LP);
        let noise_hp = band(noise::HP, pm::n::HP, target::NOISE_HP);
        // filter slots: cutoff (Scale on the normalized value, Offset in Hz) and resonance
        let flt = core::array::from_fn(|i| {
            let forms = flt::Forms::of(p, i);
            let kind = flt::Kind::of(pm::filter_type::decode(p[pm::filt(i, pm::f::TYPE)]), forms);
            if kind == flt::Kind::Off {
                return (kind, [0.0; flt::NC], false);
            }
            #[cfg(feature = "us-flt-feedback")]
            if forms.steps && flt::has_steps(kind) {
                // 7-bit step laws on the stored values (after the Add routes); a Scale route still multiplies them
                let (tc, tr) = (target::filter_cutoff(i), target::filter_resonance(i));
                let cut = (p[pm::filt(i, pm::f::CUTOFF)] * mm.scale(tc)).max(0.0).min(1.0);
                let res = (p[pm::filt(i, pm::f::RESONANCE)] * mm.scale(tr)).max(0.0).min(1.0);
                return (kind, flt::coefs_steps(kind, cut, res, sh.rate, forms), false);
            }
            let hz = cut(p[pm::filt(i, pm::f::CUTOFF)], target::filter_cutoff(i));
            let tr = target::filter_resonance(i);
            let res = (p[pm::filt(i, pm::f::RESONANCE)] * mm.scale(tr) + mm.offset(tr)).max(0.0).min(1.0);
            (kind, flt::coefs(kind, hz, res, sh.rate, forms), forms.plain() && flt::transparent(kind, hz, res))
        });
        let noise_width = sh.p[pm::NOISE + pm::n::WIDTH];
        Targets { amp, slot, noise_level: level, noise_pan, noise_band, noise_width, noise_lp, noise_hp, flt, par: Opt::new(par), dist: Opt::new(dist), v2out: Opt::new(v2out) }
    }

    /// Control values of oscillator slot `i`. `key` is the voice pitch in semitones, `shift` the voice's pan offset.
    /// One path for every slot kind (change unify-v2-algorithms 5.1): the oscillator law sets transpose, detune and
    /// volume, the pan and the combine are shared, the kind adds its own fields. The 7-bit step law (`o::LAW`) is the
    /// naive generators'; the other kinds are on the semitone/Hz laws.
    fn slot_targets(&self, _c: &Ctl, mm: &impl Query, sh: &Shared, i: usize, key: f32, shift: f32) -> SlotT {
        let p = sh.p;
        let f = |fld: usize| pm::osc(i, fld);
        let t = |fld: u8| target::osc(i, fld);
        let naive = cfg!(feature = "us-osc-naive") && self.slots[i].kind == Kind::V2;
        let steps = naive && law::choice(p[f(pm::o::LAW)], 2) == 1;
        let combine = if osc::COMBINE { law::choice(p[f(pm::o::COMBINE)], pm::combine::COUNT) as u8 } else { osc::combine::ADD };
        // the naive generator's waveform and settings; `pitch` is the slot's own transpose + detune in semitones
        let naive_t = |pitch: f32| {
            use crate::v2::{calcfreq, ftou32, linfreq};
            let col = self.shape_value(mm, sh, i, pm::shape::NAIVE_COLOR);
            let quirk = |q: usize, built: bool| built && pm::quirk_on(p, q);
            crate::osc::V2T {
                mode: osc::naive_mode(pm::osc_type::decode(p[f(pm::o::TYPE)])),
                brpt: ftou32(col),
                nffrq: linfreq(sh.rate) * calcfreq((pitch + 64.0) / 128.0),
                nfres: 1.0 - libm::sqrtf(col),
                native_sin: quirk(pm::quirk::EXACT_MATH, cfg!(feature = "us-quirk-exact-math")),
                integer_fm: quirk(pm::quirk::INTEGER_FM, cfg!(feature = "us-quirk-integer-fm")),
                old_noise: quirk(pm::quirk::LCG_NOISE, cfg!(feature = "us-quirk-lcg-noise")),
            }
        };
        // per slot, or (a naive generator with the pan after the drive) once on the voice's mono sum
        let (pan_l, pan_r) = if naive && VoiceSet::of(p).pan_after {
            (1.0, 1.0)
        } else {
            law::pan((p[f(pm::o::PAN)] * mm.scale(t(target::OSC_PAN)) + mm.offset(t(target::OSC_PAN)) + shift).max(0.0).min(1.0))
        };
        let hz_scale = mm.scale(target::PITCH) * mm.scale(t(target::OSC_PITCH));
        if steps {
            // the 7-bit step law (`V2Osc::set` / `chgPitch`): transpose `step − 64`, detune `(step − 64)/128`
            // semitones, volume `step/128`, note 60 = 130.8 Hz
            let raw = |x: f32| x * 128.0;
            let pitch = (raw(p[f(pm::o::COARSE)]) - 64.0) + (raw(p[f(pm::o::FINE)]) - 64.0) / 128.0;
            let semis = key + pitch + mm.offset(t(target::OSC_PITCH));
            let hz = crate::v2::OSC_BASE_HZ * crate::fm::exp2f((semis - 60.0) / 12.0) * hz_scale;
            let gain = (p[f(pm::o::GAIN)] * mm.scale(t(target::OSC_GAIN)) + mm.offset(t(target::OSC_GAIN))).max(0.0);
            let mix = (p[f(pm::o::MIX)] * mm.scale(t(target::OSC_MIX)) + mm.offset(t(target::OSC_MIX))).max(0.0).min(1.0);
            return SlotT { inc: hz / sh.rate, gain, mix, pan_l, pan_r, v2: Opt::new(naive_t(pitch)), combine, ..SlotT::default() };
        }
        let semis = key
            + law::coarse(p[f(pm::o::COARSE)])
            + law::fine(p[f(pm::o::FINE)])
            + 12.0 * law::octave(p[f(pm::o::OCTAVE)])
            + mm.offset(t(target::OSC_PITCH));
        let ratio = law::ratio(p[f(pm::o::RATIO)]) + law::ratio_fine(p[f(pm::o::RATIO_FINE)]);
        // a clocked slot always tracks the note: its `TRACKING` is the age byte, its `FIXED_HZ` the word byte
        let clocked = self.slots[i].kind == Kind::Clocked;
        let hz = if !clocked && law::choice(p[f(pm::o::TRACKING)], 2) == 1 {
            law::fixed_hz(p[f(pm::o::FIXED_HZ)]) * hz_scale
        } else {
            440.0 * crate::fm::exp2f((semis - 69.0) / 12.0) * ratio * hz_scale
        };
        let norm = |fld: u8, v: f32| (v * mm.scale(t(fld))).max(0.0).min(1.0);
        let gain = (law::gain(p[f(pm::o::GAIN)]) * mm.scale(t(target::OSC_GAIN)) + mm.offset(t(target::OSC_GAIN))).max(0.0);
        let mix = (norm(target::OSC_MIX, p[f(pm::o::MIX)]) + mm.offset(t(target::OSC_MIX))).max(0.0).min(1.0);
        if naive {
            // a naive generator on the semitone/Hz laws
            let pitch = law::coarse(p[f(pm::o::COARSE)]) + law::fine(p[f(pm::o::FINE)]) + 12.0 * law::octave(p[f(pm::o::OCTAVE)]);
            return SlotT { inc: hz / sh.rate, gain, mix, pan_l, pan_r, v2: Opt::new(naive_t(pitch)), combine, ..SlotT::default() };
        }
        let pm_out = (law::pm_out(norm(target::OSC_PM_OUT, p[f(pm::o::PM_OUT)])) + mm.offset(t(target::OSC_PM_OUT))).max(0.0);
        let feedback = (law::feedback(norm(target::OSC_FEEDBACK, p[f(pm::o::FEEDBACK)])) + mm.offset(t(target::OSC_FEEDBACK))).max(0.0);
        let wf = self.shape_value(mm, sh, i, pm::shape::PM_WAVEFORM);
        let pw = self.shape_value(mm, sh, i, pm::shape::BLIT_PULSE_WIDTH);
        let sv = |k: usize| self.shape_value(mm, sh, i, k);
        // the leak keeps its corner (about 14 Hz) at every rate
        let leak = crate::fm::powf(0.998, 44100.0 / sh.rate);
        let (det, spread, drive, layers) = if is_pad(self.slots[i].kind) {
            // the PAD table (change add-tf3-import): the layer detune of the point fields, the linear drive
            (law::wt_detune(p[pm::pts(i, pm::pt::DETUNE)]) * hz_scale / sh.rate, 0.0, law::wt_drive(sv(pm::shape::PAD_DRIVE)), 1)
        } else if is_pts(self.slots[i].kind) {
            // the point fields (change add-tf3-import): layer detune and spread on the wavetable's laws, a cubic drive
            let q = |fld: usize| p[pm::pts(i, fld)];
            (
                law::wt_detune(q(pm::pt::DETUNE)) * hz_scale / sh.rate,
                law::wt_spread(q(pm::pt::SPREAD)) / sh.rate,
                law::pts_drive(sv(pm::shape::PTS_DRIVE)),
                law::count(q(pm::pt::LAYERS), 1, crate::pts::MAX_LAYERS as u32),
            )
        } else {
            (
                law::wt_detune(sv(pm::shape::WT_LAYER_DETUNE)) * hz_scale / sh.rate,
                law::wt_spread(sv(pm::shape::WT_LAYER_SPREAD)) / sh.rate,
                law::wt_drive(sv(pm::shape::WT_DRIVE)),
                law::count(p[f(pm::o::SHAPE + pm::shape::WT_LAYERS)], 1, crate::wt_layers()),
            )
        };
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
        let shapes: [f32; pm::o::SHAPES] = core::array::from_fn(|k| self.shape_value(mm, sh, i, k));
        let smp = smp::Cfg::from_shape(&shapes, &self.smp[i]);
        let clk = if cfg!(feature = "us-osc-clocked") && self.slots[i].kind == Kind::Clocked {
            let mut clk = osc::ClockedT::from_shape(&shapes, sh.rate, p[f(pm::o::FEEDBACK)], p[f(pm::o::PM_OUT)], p[f(pm::o::FIXED_HZ)]);
            clk.age = osc::clocked_age(p[f(pm::o::TRACKING)]);
            // transpose and bend are a playback-rate change (an XM pitch slide resamples the whole precalc sample):
            // the phrase clock follows them, so the pitch word stays and the filter and phrase position speed up too
            let tu = tuning(p);
            if tu != 0.0 {
                clk.clock *= crate::fm::exp2f(tu / 12.0);
            }
            clk
        } else {
            osc::ClockedT::default()
        };
        #[cfg(feature = "us-osc-pair")]
        let pair = if self.slots[i].kind == Kind::Pair { osc::PairT::from_shape(&shapes) } else { osc::PairT::default() };
        #[cfg(not(feature = "us-osc-pair"))]
        let pair = osc::PairT::default();
        SlotT { inc, gain, mix, pan_l, pan_r, pm_out, feedback, wf, pw, leak, det, spread, drive, layers: layers as usize, smp, v2: Opt::default(), clk, combine, pair }
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

    /// Rebuilds the cycle of every point slot from the control state at the end of the segment, when its points,
    /// count or modes changed (Tunefish 3 updates its spline once per block).
    #[cfg(feature = "us-osc-points")]
    fn pts_build(&mut self, sh: &Shared) {
        use crate::pts::{Shape, MAX_POINTS};
        for i in 0..pm::OSC_SLOTS {
            if is_pad(self.slots[i].kind) {
                let c = &self.end;
                let key = c.pitch.cur + self.detune + tuning(sh.p) + c.mm.offset(target::PITCH);
                if let Some(p) = self.pts[i].as_deref_mut() {
                    p.decim.set(440.0 * crate::fm::exp2f((key - 69.0) / 12.0), sh.rate);
                }
                continue;
            }
            if self.slots[i].kind != Kind::Pts {
                continue;
            }
            let sv = |k: usize| self.shape_value(&self.end.mm, sh, i, k);
            let c = &self.end;
            let key = c.pitch.cur + self.detune + tuning(sh.p) + c.mm.offset(target::PITCH);
            let hz = 440.0 * crate::fm::exp2f((key - 69.0) / 12.0);
            let s = Shape {
                count: law::count(sh.p[pm::pts(i, pm::pt::COUNT_)], 1, MAX_POINTS as u32) as usize,
                offset: core::array::from_fn(|k| sv(pm::shape::PTS_POINT + 2 * k)),
                value: core::array::from_fn(|k| sv(pm::shape::PTS_POINT + 2 * k + 1)),
                modes: core::array::from_fn(|k| law::choice(sh.p[pm::pts(i, pm::pt::MODE + k)], 3) as u8),
            };
            if let Some(p) = self.pts[i].as_deref_mut() {
                p.update(&s);
                p.decim.set(hz, sh.rate);
            }
        }
    }

    #[cfg(all(not(feature = "us-osc-points"), feature = "us-osc-pad"))]
    fn pts_build(&mut self, sh: &Shared) {
        for i in 0..pm::OSC_SLOTS {
            if is_pad(self.slots[i].kind) {
                let c = &self.end;
                let key = c.pitch.cur + self.detune + tuning(sh.p) + c.mm.offset(target::PITCH);
                if let Some(p) = self.pts[i].as_deref_mut() {
                    p.decim.set(440.0 * crate::fm::exp2f((key - 69.0) / 12.0), sh.rate);
                }
            }
        }
    }

    #[cfg(not(any(feature = "us-osc-points", feature = "us-osc-pad")))]
    fn pts_build(&mut self, _: &Shared) {}

    /// Voice reset: every slot and table back to its loaded state, drawing the start phases from `rng` again.
    pub fn reset(&mut self, kinds: [Kind; pm::OSC_SLOTS], rng: &mut Xorshift) {
        let mut wt = core::mem::take(&mut self.wt);
        wt.iter_mut().flatten().for_each(|w| **w = Wt::new());
        let mut pts = core::mem::take(&mut self.pts);
        pts.iter_mut().flatten().for_each(|p| **p = Pts::new());
        let (pads, smp, insts) = (self.pads, self.smp, self.insts);
        *self = Voice::new(kinds, rng, wt, pts, pads, smp, insts);
    }

    fn ramps_set(&mut self, t: &Targets) {
        self.par = t.par;
        self.dist_p = t.dist;
        self.v2out = t.v2out;
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
        self.par = t.par;
        self.dist_p = t.dist;
        self.v2out = t.v2out;
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
        self.par = t.par;
        self.dist_p = t.dist;
        self.v2out = t.v2out;
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
        if self.start.env[0].finished() || (cfg!(feature = "us-voice-oneshot") && law::choice(sh.p[v::ONE_SHOT], 2) == 1 && self.samplers_done()) {
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
        self.pts_build(sh);
        self.seg_pos = 0;
        self.seg_len = len;
        let t = self.targets(&self.end, sh);
        self.ramps_to(&t, len);
    }

    /// Runs on every grid frame for a sounding voice.
    pub fn tick(&mut self, sh: &Shared, frame: u64) {
        if self.seg_pos >= self.seg_len {
            let len = if self.fine > 0 { FINE - (frame % FINE as u64) as u32 } else { CONTROL - (frame % CONTROL as u64) as u32 };
            // a linear timed envelope 1 ends the segment at its stage corner (off the grid; the instrument ticks a voice
            // whose segment has ended)
            #[cfg(feature = "us-env-linear")]
            let len = self.end.env[0].linear_left(&self.fx.envp[0], sh.rate).map_or(len, |n| len.min(n));
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

    /// Adds this voice's output for `l.len()` frames (never across a control grid frame). `aux` is the device's aux
    /// input (V2 aux oscillators), `None` when it has none.
    /// `tap` (`probe` builds only): the instrument's filter-tap ring for this call and the ring frame of `l[0]`.
    /// `aux` (`us-strip`): the instrument's extra input pairs (aux A l/r, aux B l/r, or none) and the offset of `l[0]`
    /// in them, for the V2 aux oscillators.
    pub fn render(
        &mut self,
        l: &mut [f32],
        r: &mut [f32],
        #[cfg(feature = "probe")] tap: Option<(&mut crate::probe::Ring, usize)>,
        #[cfg(feature = "us-strip")] aux: (&[&mut [f32]], usize),
    ) {
        // the editor's taps: one loop-invariant branch, so the player's loop is unchanged
        #[cfg(feature = "probe")]
        let (mut tap, mut at) = match tap {
            Some((t, a)) => (Some(t), a),
            None => (None, 0),
        };
        #[cfg_attr(not(feature = "us-strip"), allow(unused_variables))]
        for (j, (lo, ro)) in l.iter_mut().zip(r.iter_mut()).enumerate() {
            let amp = self.amp.next();
            let (mut bl, mut br, mut send, mut mono) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for (((((slot, ramps), wt), pts), pad), smp) in self
                .slots
                .iter_mut()
                .zip(self.ramps.iter_mut())
                .zip(self.wt.iter_mut())
                .zip(self.pts.iter_mut())
                .zip(self.pads.iter())
                .zip(self.smp.iter())
            {
                if slot.kind == Kind::Off {
                    send = 0.0;
                    continue;
                }
                // the slot's output before the pan; a naive generator reads the running mono sum (FM) or the aux input
                let (a, b, next) = if cfg!(feature = "us-osc-naive") && slot.kind == Kind::V2 {
                    #[cfg(feature = "us-strip")]
                    let ax = match aux.0 {
                        [al, ar, bl, br] => [(al[aux.1 + j], ar[aux.1 + j]), (bl[aux.1 + j], br[aux.1 + j])],
                        _ => [(0.0, 0.0); 2],
                    };
                    #[cfg(not(feature = "us-strip"))]
                    let ax = [(0.0, 0.0); 2];
                    let y = osc::v2_sample(slot, ramps, mono, ax);
                    (y, y, 0.0)
                } else {
                    osc::sample(slot, wt.as_deref_mut(), pts.as_deref_mut(), *pad, smp, ramps, send)
                };
                // one combine for every slot: add (panned), multiply (ring) or replace
                let (pl, pr) = ramps.pan();
                (bl, br, mono) = osc::mix_in((bl, br, mono), ramps.combine, a, b, pl, pr);
                send = next;
            }
            let level = self.noise_level.next();
            let (pl, pr) = (self.noise_pan[0].next(), self.noise_pan[1].next());
            if cfg!(feature = "us-noise") && level > 0.0 {
                let (nl, nr) = self.noise.next(level, self.noise_band, self.noise_width);
                bl += nl * pl;
                br += nr * pr;
            }
            if let Some((g1, g2)) = self.par.get().filter(|_| cfg!(feature = "us-flt-parallel")) {
                let (a, b) = self.flt[0].process(bl, br);
                let (c, d) = self.flt[1].process(bl, br);
                #[cfg(feature = "probe")]
                if let Some(t) = tap.as_deref_mut() {
                    let m = |x: f32, y: f32| (x + y) * 0.5 * amp;
                    if self.flt[0].kind != flt::Kind::Off {
                        t.add(at, 0, m(bl, br), m(a, b));
                    }
                    if self.flt[1].kind != flt::Kind::Off {
                        t.add(at, 1, m(bl, br), m(c, d));
                    }
                }
                (bl, br) = (a * g1 + c * g2, b * g1 + d * g2);
                #[cfg(feature = "probe")]
                let mut j = 2;
                for f in self.flt[2..].iter_mut() {
                    #[cfg(feature = "probe")]
                    let x1 = (bl, br);
                    (bl, br) = f.process(bl, br);
                    #[cfg(feature = "probe")]
                    {
                        if let Some(t) = tap.as_deref_mut().filter(|_| f.kind != flt::Kind::Off) {
                            t.add(at, j, (x1.0 + x1.1) * 0.5 * amp, (bl + br) * 0.5 * amp);
                        }
                        j += 1;
                    }
                }
            } else {
                #[cfg(feature = "probe")]
                let mut j = 0;
                for f in self.flt.iter_mut() {
                    #[cfg(feature = "probe")]
                    let x1 = (bl, br);
                    (bl, br) = f.process(bl, br);
                    #[cfg(feature = "probe")]
                    {
                        if let Some(t) = tap.as_deref_mut().filter(|_| f.kind != flt::Kind::Off) {
                            t.add(at, j, (x1.0 + x1.1) * 0.5 * amp, (bl + br) * 0.5 * amp);
                        }
                        j += 1;
                    }
                }
            }
            #[cfg(feature = "probe")]
            {
                at += 1;
            }
            if cfg!(feature = "us-drive") && self.dist_p.get().mode != 0 {
                let dp = self.dist_p.get();
                (bl, br) = self.dist.with(|d| d.run(&dp, bl, br));
            }
            let (ol, or) = match self.v2out.get().filter(|_| cfg!(feature = "us-voice-out")) {
                // the V2 voice: DC filter (format version 6) on the mono voice, the envelope, then V2's pan
                Some((lv, rv, dcf)) => {
                    let r = self.dcf_r.get();
                    let m = if dcf { self.dcf.with(|f| f.step(bl, r)) } else { bl };
                    let out = m * amp;
                    (lv * out, rv * out)
                }
                None => (bl * amp, br * amp),
            };
            #[cfg(feature = "us-osc-clocked")]
            let (ol, or) = self.dk.next(ol, or);
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
        #[cfg(feature = "us-osc-clocked")]
        if !self.on {
            self.dk.last = (0.0, 0.0);
        }
        self.on = true;
        self.seq = cx.seq;
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
        #[cfg(feature = "us-voice-drop")]
        let l2k = law::drop_log2k(sh.p[pm::el::DROP_RATE]);
        #[cfg(not(feature = "us-voice-drop"))]
        let l2k = 0.0f32;
        let pitch = if l2k < 0.0 {
            // Elevated's drop restarts from the note at every note-on, whatever the voice played before
            #[cfg(feature = "us-voice-drop")]
            let p = Pitch::drop(n, law::drop_hz(sh.p[pm::el::DROP_BASE]), l2k);
            #[cfg(not(feature = "us-voice-drop"))]
            let p = Pitch::snap(n);
            p
        } else if cfg!(feature = "us-voice-glide") && glide > 0.0 && self.has_pitch {
            let tau = (law::time(glide) * sh.rate).max(1e-3);
            Pitch { cur: self.end.pitch.cur, target: n, mv: Move::Expo(tau) }
        } else {
            Pitch::snap(n)
        };
        self.has_pitch = true;
        // a clocked instrument's timed attack of 0 is instant: the mini-cauldron has no attack
        let instant = cfg!(feature = "us-osc-clocked") && self.slots.iter().any(|s| s.kind == Kind::Clocked);
        // its restart zeroes the filter's output: a note cut while sounding (a ping-pong joint) would drop in one
        // frame, so the step is carried and faded out; a forward retrigger's old note has faded to 0 (no step)
        #[cfg(feature = "us-osc-clocked")]
        if instant {
            self.dk.start((sh.rate * DECLICK_MS / 1000.0).max(1.0) as u32);
        }
        self.fx = Fixed {
            envp: core::array::from_fn(|i| EnvP { instant, ..EnvP::read(sh.p, i) }),
            vel: self.vel,
            key: note as f32 / 127.0,
            note: note as f32,
            resample: instant,
        };
        // V2: key sync none keeps the counters, 1 zeros them, 2 (version 6) also resets the voice's state
        let p = sh.p;
        let sync = if cfg!(feature = "us-osc-naive") || cfg!(feature = "us-voice-out") { law::choice(p[pm::vs::KEYSYNC], pm::vs::keysync::COUNT) } else { 0 };
        let full = cfg!(feature = "us-voice-out") && sync == pm::vs::keysync::RESET;
        let fixed_seed = cfg!(feature = "us-quirk-fixed-seed") && pm::quirk_on(p, pm::quirk::FIXED_SEED);
        for (i, s) in self.slots.iter_mut().enumerate() {
            if !cfg!(feature = "us-osc-naive") || s.kind != Kind::V2 {
                continue;
            }
            s.v2s.with(|s| {
                if !s.init || full {
                    s.nseed = if fixed_seed { 0 } else { V2_SEEDS[i] };
                    s.nf = crate::v2::Lrc::default();
                    s.init = true;
                }
                if sync >= 1 {
                    s.cnt = 0;
                }
            });
        }
        if full {
            self.flt = [flt::Slot::new(); pm::FILT_SLOTS];
            self.dist = Opt::default();
        }
        if cfg!(feature = "us-voice-out") {
            self.dcf = Opt::default();
            self.dcf_r = Opt::new(crate::v2::dcf_r(sh.rate));
        }
        let mut env = [Env::new(); pm::ENV_SLOTS];
        for (i, e) in env.iter_mut().enumerate() {
            let keep = self.fx.envp[i].keeps_level() && !full;
            if keep {
                *e = self.end.env[i]; // the frame law restarts the attack from where the envelope is
            }
            e.trigger(keep);
        }
        let prev_lfo = self.end.lfo;
        let lfo = core::array::from_fn(|k| {
            if !cfg!(feature = "us-voice-lfo") {
                return Lfo::new(0.0);
            }
            if !cfg!(feature = "us-lfo-frame") {
                // without the LFO settings: restart at 0 or song time (what `sync_of` gives then)
                let synced = law::choice(p[pm::lfo(k, pm::l::SYNC)], pm::lfo_sync::COUNT) != pm::lfo_sync::SONG_TIME;
                return Lfo::new(if synced { 0.0 } else { LfoP::rate_of(p, k) * cx.frame as f64 / sh.rate as f64 });
            }
            let (sync, phase0, reseed) = LfoP::sync_of(p, k);
            match sync {
                lfo_sync::RESTART => Lfo::restart(phase0, reseed, if reseed { self.rng.next_u32() } else { 0 }),
                lfo_sync::CONTINUE => prev_lfo[k],
                // song time: the phase of an LFO that has run since frame 0, so every voice shares it
                _ => Lfo::new(phase0 + LfoP::rate_of(p, k) * cx.frame as f64 / sh.rate as f64),
            }
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
        for i in 0..pm::OSC_SLOTS {
            if cfg!(feature = "us-osc-clocked") && self.slots[i].kind == Kind::Clocked {
                // the phrase frame of this note: the converter writes it as the velocity
                let t0 = osc::clocked_t0(libm::roundf(self.vel * 128.0) as u8);
                self.slots[i].clocked_start(t0, &t.slot[i].clk);
            }
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
        // Tunefish 3: the first layer starts at phase 0; every other layer at one shared random pair (its tick-count
        // seed gives them the same draws)
        for (i, p) in self.pts.iter_mut().enumerate() {
            let Some(p) = p.as_deref_mut() else { continue };
            let pair = [self.rng.next_f32(), self.rng.next_f32()];
            for (k, layer) in p.phase.iter_mut().enumerate() {
                // a PAD slot's two phases (layer 0) start at random too
                *layer = if k == 0 && !is_pad(self.slots[i].kind) { [0.0; 2] } else { pair };
            }
            p.decim.clear();
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
        self.pts_build(&sh);
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
        if cfg!(feature = "us-voice-oneshot") && !cx.force && law::choice(cx.sh.p[v::ONE_SHOT], 2) == 1 {
            return;
        }
        let sh = cx.sh;
        let fx = self.fx;
        self.edit(&sh, |c| c.release(&fx));
        self.fine = FINE_SPAN;
    }

    fn slide(&mut self, note: u8, cx: &mut NoteCx<'c>) {
        let sh = cx.sh;
        let frames = law::time(sh.p[v::SLIDE]) * sh.rate;
        self.note = note;
        // a clocked instrument glides linearly in frequency, as the original moves its pitch word
        let hz = cfg!(feature = "us-osc-clocked") && self.slots.iter().any(|s| s.kind == Kind::Clocked);
        if cfg!(feature = "us-voice-glide") && hz {
            self.edit(&sh, |c| c.pitch.slide_hz_to(note as f32, frames));
        } else if cfg!(feature = "us-voice-glide") {
            self.edit(&sh, |c| c.pitch.slide_to(note as f32, frames));
        } else {
            self.edit(&sh, |c| c.pitch = Pitch::snap(note as f32));
        }
    }

    const MONO: bool = cfg!(feature = "us-voice-mono");
    const UNISON: bool = cfg!(feature = "us-voice-unison");
}

#[cfg(all(test, feature = "us-voice-glide"))]
mod pitch_tests {
    use super::*;

    #[test]
    fn a_frequency_slide_is_linear_in_hz() {
        // the mini-cauldron's glide: the pitch word moves linearly, so halfway up an octave is 1.5×, not √2×
        let mut p = Pitch::snap(36.0);
        p.slide_hz_to(48.0, 1000.0);
        p.advance(500);
        assert!((crate::fm::exp2f((p.cur - 36.0) / 12.0) - 1.5).abs() < 1e-3, "{}", p.cur);
        p.advance(499);
        assert!(p.cur < 48.0);
        p.advance(1);
        assert_eq!((p.cur, p.target), (48.0, 48.0));
        // downwards too: halfway down an octave is 0.75×
        let mut p = Pitch::snap(48.0);
        p.slide_hz_to(36.0, 1000.0);
        p.advance(500);
        assert!((crate::fm::exp2f((p.cur - 48.0) / 12.0) - 0.75).abs() < 1e-3, "{}", p.cur);
    }

    #[test]
    fn a_plain_slide_stays_linear_in_semitones() {
        let mut p = Pitch::snap(36.0);
        p.slide_to(48.0, 1000.0);
        p.advance(500);
        assert!((p.cur - 42.0).abs() < 1e-4);
    }
}

#[cfg(all(test, feature = "us-voice-drop"))]
mod drop_tests {
    use super::*;

    fn hz(p: &Pitch) -> f32 {
        440.0 * crate::fm::exp2f((p.cur - 69.0) / 12.0)
    }

    #[test]
    fn a_kick_drop_falls_from_the_note_to_the_base() {
        // note 45 = 110 Hz towards 40 Hz at k = 0.9995 per sample: b + (f − b)·k^(n+1)
        let l2k = libm::log2f(0.9995);
        let mut p = Pitch::drop(45.0, 40.0, l2k);
        assert!((hz(&p) - (40.0 + 70.0 * 0.9995)).abs() < 0.01, "note-on: {}", hz(&p));
        assert!((hz(&p) - 110.0).abs() < 0.5);
        p.advance(441);
        let want = 40.0 + 70.0 * 0.9995f64.powi(442) as f32;
        assert!((hz(&p) - want).abs() < 0.01, "10 ms: {} vs {want}", hz(&p));
        p.advance(44100 - 441);
        assert!((hz(&p) - 40.0).abs() < 0.1, "1 s: {}", hz(&p));
    }
}
