//! Upstream `SamplePlayer` and the sample voice shared by Specimen (GSM sample in the chunk) and Adultery (GM.DLS
//! wave): amp/mod envelopes, `Svf` with mod-envelope cutoff, velocity/128, pitch relative to note 60 with coarse
//! `floor((c·2−1)·12)` and fine `f·2−1` semitones.

use crate::helpers::{exp2, pan_to_scalar_left, pan_to_scalar_right, param_to_boolean, param_to_frequency,
    scalar_to_env_value, volume_to_scalar};
use crate::svf::{Svf, SvfKind};
use crate::synth::{Common, CommonParam, EnvState, Envelope, Synth, VoiceBase, WsCx, MAX_VOICES};
use synth_core::device::{Ctx, Io};
use synth_core::math::clamp;
use synth_core::mix::LoadError;
use synth_core::pool::Pool;
use synth_core::voice::{Voice, VoicePool, VoiceState};

/// Where sample data lives: decoded f32 in the pool (Specimen) or 8/16-bit PCM read in place from the bank (Adultery).
#[derive(Clone, Copy)]
pub enum Source<'a> {
    F32(&'a [f32]),
    #[cfg(feature = "ws-adultery")]
    Wave(dls::Wave<'a>),
}

impl Source<'_> {
    #[inline]
    pub fn len(&self) -> i32 {
        let n = match self {
            Source::F32(d) => d.len(),
            #[cfg(feature = "ws-adultery")]
            Source::Wave(w) => w.len(),
        };
        n.min(i32::MAX as usize) as i32
    }

    /// `i` in `0..len` (16-bit: `s / 32768`, exactly upstream's `(float)((double)s / 32768.0)`).
    #[inline]
    fn at(&self, i: i32) -> f32 {
        match self {
            Source::F32(d) => d[i as usize],
            #[cfg(feature = "ws-adultery")]
            Source::Wave(w) => w.sample(i as usize),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loop {
    Disabled,
    Repeat,
    PingPong,
}

impl Loop {
    /// `(int)(v · 2)`; out-of-range values behave as no loop (upstream's switch matches nothing).
    pub fn from_param(v: f32) -> Self {
        match (v * 2.0) as i32 {
            1 => Loop::Repeat,
            2 => Loop::PingPong,
            _ => Loop::Disabled,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interp {
    Nearest,
    Linear,
    /// Out-of-range parameter: upstream outputs 0.
    None,
}

impl Interp {
    pub fn from_param(v: f32) -> Self {
        match v as i32 {
            0 => Interp::Nearest,
            1 => Interp::Linear,
            _ => Interp::None,
        }
    }
}

/// `(int)v` → `Some(false)` from sample, `Some(true)` manual, `None` out of range (loop bounds left as they were).
pub fn boundary_from_param(v: f32) -> Option<bool> {
    match v as i32 {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// Upstream `SamplePlayer`.
#[derive(Clone, Copy)]
pub struct Player<'a> {
    pub src: Source<'a>,
    pub active: bool,
    pub sample_start: f32,
    pub reverse: bool,
    pub loop_mode: Loop,
    pub boundary_manual: Option<bool>,
    pub loop_start: f32,
    pub loop_length: f32,
    pub interp: Interp,
    /// `(start, length)` from the sample (Specimen: whole sample; Adultery: wave loop or whole wave).
    pub sample_loop: (i32, i32),
    pos: f64,
    delta: f64,
    loop_s: i32,
    loop_l: i32,
    loop_e: i32,
    rev: bool,
}

impl<'a> Player<'a> {
    pub fn new() -> Self {
        Player {
            src: Source::F32(&[]),
            active: false,
            sample_start: 0.0,
            reverse: false,
            loop_mode: Loop::Repeat,
            boundary_manual: Some(false),
            loop_start: 0.0,
            loop_length: 1.0,
            interp: Interp::Linear,
            sample_loop: (0, 0),
            pos: 0.0,
            delta: 0.0,
            loop_s: 0,
            loop_l: 0,
            loop_e: 0,
            rev: false,
        }
    }

    pub fn calc_pitch(&mut self, note: f64) {
        let d = exp2(note / 12.0);
        self.delta = if self.rev { -d } else { d };
    }

    pub fn init_pos(&mut self) {
        self.rev = self.reverse;
        self.active = true;
        let last = (self.src.len() - 1) as f64;
        let s = self.sample_start as f64;
        self.pos = if self.rev { (1.0 - s) * last } else { s * last };
    }

    pub fn run_prep(&mut self) {
        let len = self.src.len();
        match self.boundary_manual {
            Some(false) => (self.loop_s, self.loop_l) = self.sample_loop,
            Some(true) => {
                self.loop_s = (len as f32 * self.loop_start) as i32;
                self.loop_l = (len as f32 * self.loop_length) as i32;
            }
            None => {}
        }
        if self.loop_l < 1 {
            self.loop_l = 1;
        }
        if self.loop_s >= len {
            self.loop_s = len - 1;
        }
        self.loop_e = self.loop_s.saturating_add(self.loop_l);
        if self.loop_e > len {
            self.loop_e = len;
            self.loop_l = self.loop_e - self.loop_s;
        }
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        let fl = libm::floor(self.pos);
        let fract = self.pos - fl;
        let len = self.src.len();
        // range check in f64: an out-of-range position never wraps into the sample
        if fl < 0.0 || fl >= len as f64 {
            self.active = false;
            return 0.0;
        }
        let i = fl as i32;
        let sample = match self.interp {
            Interp::Nearest => self.src.at(i),
            Interp::Linear => {
                let left = self.src.at(i);
                let mut ri = i + 1;
                if self.loop_mode == Loop::Repeat && ri == self.loop_e {
                    ri = self.loop_s;
                }
                // a degenerate loop (start −1 on an empty-loop edge) reads 0 like an out-of-range right index
                let right = if (0..len).contains(&ri) { self.src.at(ri) } else { 0.0 };
                (left as f64 * (1.0 - fract) + right as f64 * fract) as f32
            }
            Interp::None => 0.0,
        };
        self.pos += self.delta;
        match self.loop_mode {
            Loop::Repeat => {
                // loop_l ≥ 1 after run_prep, so these terminate (one step per loop length, like upstream)
                let (s, e, l) = (self.loop_s as f64, self.loop_e as f64, self.loop_l as f64);
                if self.delta > 0.0 {
                    while self.pos >= e {
                        self.pos -= l;
                    }
                } else if self.pos < s {
                    while self.pos < s {
                        self.pos += l;
                    }
                }
            }
            Loop::PingPong => {
                if self.delta > 0.0 && self.pos >= self.loop_e as f64 {
                    self.pos = (self.loop_e - 1) as f64;
                    self.delta = -self.delta;
                    self.rev = !self.rev;
                } else if self.delta < 0.0 && self.pos < self.loop_s as f64 {
                    self.pos = self.loop_s as f64;
                    self.delta = -self.delta;
                    self.rev = !self.rev;
                }
            }
            Loop::Disabled => {}
        }
        sample
    }
}

impl Default for Player<'_> {
    fn default() -> Self {
        Self::new()
    }
}

/// Settings of a sample instrument (upstream Specimen/Adultery members).
pub struct Params<'a> {
    pub src: Source<'a>,
    pub sample_loop: (i32, i32),
    amp: [f32; 4],
    modenv: [f32; 4],
    sample_start: f32,
    reverse: bool,
    loop_mode: Loop,
    boundary_manual: Option<bool>,
    loop_start: f32,
    loop_length: f32,
    interp: Interp,
    coarse: f32,
    fine: f32,
    filter_kind: SvfKind,
    filter_freq: f32,
    filter_res: f32,
    filter_mod: f32,
    master: f32,
}

/// Parameter roles; each device maps its upstream indices onto these.
#[derive(Clone, Copy)]
#[allow(dead_code)] // single-device builds use a subset
pub enum P {
    Amp(usize),
    Mod(usize),
    SampleStart,
    LoopMode,
    LoopBoundary,
    LoopStart,
    LoopLength,
    Reverse,
    Interp,
    Coarse,
    Fine,
    FilterType,
    FilterFreq,
    FilterRes,
    FilterMod,
    Master,
    Common(CommonParam),
    /// Device-specific (Adultery's sample index): handled by the wrapper's `extra_param`.
    Extra,
}

pub struct SampleVoice<'a> {
    base: VoiceBase,
    amp: Envelope,
    modenv: Envelope,
    player: Player<'a>,
    filter: Svf,
    velocity: f32,
}

impl VoiceState for SampleVoice<'_> {
    fn is_on(&self) -> bool {
        self.base.on
    }
    fn note(&self) -> u8 {
        self.base.note
    }
    fn kill(&mut self) {
        self.base.on = false;
    }
}

impl<'c, 'a> Voice<WsCx<'c, Params<'a>>> for SampleVoice<'a> {
    fn note_on(&mut self, note: u8, velocity: u8, detune: f32, pan: f32, cx: &mut WsCx<'c, Params<'a>>) {
        self.base.note_on(note, detune, pan);
        let p = cx.p;
        self.amp.set(p.amp[0], p.amp[1], p.amp[2], p.amp[3]);
        self.amp.trigger();
        self.modenv.set(p.modenv[0], p.modenv[1], p.modenv[2], p.modenv[3]);
        self.modenv.trigger();
        // the voice keeps this sample until its next note-on (a later sample change never pulls data from under it)
        self.player.src = p.src;
        self.player.sample_loop = p.sample_loop;
        self.prep(p);
        self.calc_pitch(p);
        self.player.init_pos();
        self.velocity = velocity as f32 / 128.0;
    }
    fn note_off(&mut self, _: &mut WsCx<'c, Params<'a>>) {
        self.amp.off();
        self.modenv.off();
    }
    fn slide(&mut self, note: u8, cx: &mut WsCx<'c, Params<'a>>) {
        self.base.slide(note, cx.common.slide, cx.rate);
    }
}

impl<'a> SampleVoice<'a> {
    fn new() -> Self {
        SampleVoice {
            base: VoiceBase::default(),
            amp: Envelope::new(),
            modenv: Envelope::new(),
            player: Player::new(),
            filter: Svf::new(SvfKind::Lowpass),
            velocity: 0.0,
        }
    }

    fn prep(&mut self, p: &Params) {
        let pl = &mut self.player;
        pl.sample_start = p.sample_start;
        pl.loop_start = p.loop_start;
        pl.loop_length = p.loop_length;
        pl.loop_mode = p.loop_mode;
        pl.boundary_manual = p.boundary_manual;
        pl.interp = p.interp;
        pl.reverse = p.reverse;
    }

    #[inline]
    fn calc_pitch(&mut self, p: &Params) {
        let coarse = libm::floor(((p.coarse * 2.0 - 1.0) * 12.0) as f64);
        let note = self.base.get_note() - 60.0 + self.base.detune as f64 + (p.fine * 2.0) as f64 - 1.0 + coarse;
        self.player.calc_pitch(note);
    }

    fn run(&mut self, p: &Params, l: &mut [f32], r: &mut [f32], ctx: &mut Ctx) {
        let rate = ctx.rate;
        self.filter.kind = p.filter_kind;
        self.filter.set_q(p.filter_res);
        self.prep(p);
        self.player.run_prep();
        let amp = volume_to_scalar(p.master);
        let (pl, pr) = (pan_to_scalar_left(self.base.pan), pan_to_scalar_right(self.base.pan));
        for (lo, ro) in l.iter_mut().zip(r.iter_mut()) {
            self.calc_pitch(p);
            self.filter.set_freq(clamp(
                p.filter_freq + self.modenv.value() * (20000.0 - 20.0) * (p.filter_mod * 2.0 - 1.0),
                0.0,
                20000.0 - 20.0,
            ));
            let s = self.player.next();
            if !self.player.active {
                self.base.on = false;
                break;
            }
            let s = self.filter.next(s, rate) * self.amp.value() * self.velocity * amp;
            *lo += s * pl;
            *ro += s * pr;
            self.modenv.next(rate);
            self.amp.next(rate);
            if self.amp.state == EnvState::Finished {
                self.base.on = false;
                break;
            }
        }
    }
}

/// A sample instrument: Specimen and Adultery differ only in their parameter map, defaults and sample source.
pub struct Sampler<'a> {
    pub p: Params<'a>,
    synth: Synth<'a, SampleVoice<'a>>,
}

impl<'a> Sampler<'a> {
    pub fn need() -> usize {
        VoicePool::<SampleVoice>::need(MAX_VOICES)
    }

    /// `adultery`: upstream Adultery defaults (repeat loop, bounds from the sample); else Specimen's (no loop,
    /// manual bounds).
    pub fn new(pool: &mut Pool<'a>, src: Source<'a>, adultery: bool) -> Result<Self, LoadError> {
        let voices = VoicePool::new(pool, MAX_VOICES, |_| SampleVoice::new())?;
        let p = Params {
            src,
            sample_loop: (0, src.len()),
            amp: [1.0; 4],
            modenv: [1.0, 5.0, 1.0, 1.5],
            sample_start: 0.0,
            reverse: false,
            loop_mode: if adultery { Loop::Repeat } else { Loop::Disabled },
            boundary_manual: Some(!adultery),
            loop_start: 0.0,
            loop_length: 1.0,
            interp: Interp::Linear,
            coarse: 0.5,
            fine: 0.5,
            filter_kind: SvfKind::Lowpass,
            filter_freq: 20000.0 - 20.0,
            filter_res: 1.0,
            filter_mod: 0.5,
            master: 0.5,
        };
        Ok(Sampler { p, synth: Synth::new(voices) })
    }

    pub fn set(&mut self, which: P, v: f32) {
        let p = &mut self.p;
        match which {
            P::Amp(i) => p.amp[i] = if i == 2 { v } else { scalar_to_env_value(v) },
            P::Mod(i) => p.modenv[i] = if i == 2 { v } else { scalar_to_env_value(v) },
            P::SampleStart => p.sample_start = v,
            P::LoopMode => p.loop_mode = Loop::from_param(v),
            P::LoopBoundary => p.boundary_manual = boundary_from_param(v),
            P::LoopStart => p.loop_start = v,
            P::LoopLength => p.loop_length = v,
            P::Reverse => p.reverse = param_to_boolean(v),
            P::Interp => p.interp = Interp::from_param(v),
            P::Coarse => p.coarse = v,
            P::Fine => p.fine = v,
            P::FilterType => p.filter_kind = SvfKind::from_param(v),
            P::FilterFreq => p.filter_freq = param_to_frequency(v),
            P::FilterRes => p.filter_res = 1.0 - v,
            P::FilterMod => p.filter_mod = v,
            P::Master => p.master = v,
            P::Common(c) => self.synth.set_common(c, v, &self.p),
            P::Extra => {}
        }
    }

    pub fn note_on(&mut self, note: u8, velocity: u8, offset: u32) {
        self.synth.note_on(note, velocity, offset);
    }
    pub fn note_off(&mut self, note: u8, offset: u32) {
        self.synth.note_off(note, offset);
    }
    pub fn all_notes_off(&mut self) {
        self.synth.all_notes_off(&self.p);
    }
    pub fn reset(&mut self) {
        self.synth.events.clear();
        self.synth.voices.clear_state();
        for v in self.synth.voices.voices_mut() {
            *v = SampleVoice::new();
        }
    }
    pub fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        self.synth.run(&self.p, io, n, ctx, |v, p, _: &Common, l, r, ctx| v.run(p, l, r, ctx));
    }
}

/// Implements `Device` for a wrapper `$t { s: Sampler }` with the parameter map `$map: [P; N]`.
macro_rules! sampler_device {
    ($t:ident, $map:ident) => {
        impl synth_core::device::Device for $t<'_> {
            fn param_count(&self) -> usize {
                $map.len()
            }
            fn set_param(&mut self, i: usize, v: f32) {
                match $map.get(i) {
                    Some(P::Extra) => self.extra_param(i, v),
                    Some(&w) => self.s.set(w, v),
                    None => {}
                }
            }
            fn note_on(&mut self, note: u8, velocity: u8, offset: u32) {
                self.s.note_on(note, velocity, offset);
            }
            fn note_off(&mut self, note: u8, offset: u32) {
                self.s.note_off(note, offset);
            }
            fn all_notes_off(&mut self) {
                self.s.all_notes_off();
            }
            fn reset(&mut self) {
                self.s.reset();
            }
            fn run(&mut self, io: &mut synth_core::device::Io, n: usize, ctx: &mut synth_core::device::Ctx) {
                self.s.run(io, n, ctx);
            }
        }
    };
}
pub(crate) use sampler_device;
