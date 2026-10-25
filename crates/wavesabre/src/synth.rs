//! Upstream `SynthDevice` + `Envelope`, on `synth_core`'s voice pool and event slots: 256 voices and 256 event slots
//! like upstream, events split rendering, active voices render in index order and add into main L/R (cleared
//! first: instruments overwrite main).

use crate::helpers::{param_to_unisono, param_to_vibrato_freq, pow4};
use synth_core::device::{Ctx, Event, EventKind, Events, Io, Step, MAIN_L, MAIN_R};
use synth_core::rng::Rng;
use synth_core::voice::{Mode, Voice, VoicePool, VoiceState};

pub const MAX_VOICES: usize = 256;
pub const MAX_EVENTS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvState {
    Attack,
    Decay,
    Sustain,
    Release,
    Finished,
}

/// Upstream `Envelope`: times in ms, linear attack, quadratic decay/release, advanced per sample.
#[derive(Clone, Copy, Debug)]
pub struct Envelope {
    pub state: EnvState,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pos: f32,
    release_value: f32,
}

impl Envelope {
    pub fn new() -> Self {
        Envelope {
            state: EnvState::Finished,
            attack: 1.0,
            decay: 5.0,
            sustain: 0.5,
            release: 1.5,
            pos: 0.0,
            release_value: 0.0,
        }
    }

    pub fn set(&mut self, a: f32, d: f32, s: f32, r: f32) {
        self.attack = a;
        self.decay = d;
        self.sustain = s;
        self.release = r;
    }

    pub fn trigger(&mut self) {
        self.state = EnvState::Attack;
        self.pos = 0.0;
    }

    pub fn off(&mut self) {
        self.release_value = self.value();
        self.state = EnvState::Release;
        self.pos = 0.0;
    }

    #[inline]
    pub fn value(&self) -> f32 {
        match self.state {
            EnvState::Attack => self.pos / self.attack,
            EnvState::Decay => {
                let mut f = 1.0 - self.pos / self.decay;
                f *= f;
                1.0 * f + self.sustain * (1.0 - f)
            }
            EnvState::Sustain => self.sustain,
            EnvState::Release => {
                let mut f = 1.0 - self.pos / self.release;
                f *= f;
                self.release_value * f
            }
            EnvState::Finished => 0.0,
        }
    }

    #[inline]
    pub fn next(&mut self, rate: f64) {
        let d = (1000.0 / rate) as f32;
        match self.state {
            EnvState::Attack => {
                self.pos += d;
                if self.pos >= self.attack {
                    self.state = EnvState::Decay;
                    self.pos -= self.attack;
                }
            }
            EnvState::Decay => {
                self.pos += d;
                if self.pos >= self.decay {
                    self.state = EnvState::Sustain;
                }
            }
            EnvState::Release => {
                self.pos += d;
                if self.pos >= self.release {
                    self.state = EnvState::Finished;
                }
            }
            EnvState::Sustain | EnvState::Finished => {}
        }
    }
}

impl Default for Envelope {
    fn default() -> Self {
        Self::new()
    }
}

/// `SynthDevice` settings shared by all instruments (vibrato, rise, slide).
#[derive(Clone, Copy, Debug)]
pub struct Common {
    pub vib_freq: f64,
    pub vib_amount: f32,
    pub rise: f32,
    pub slide: f32,
}

impl Default for Common {
    fn default() -> Self {
        Common { vib_freq: param_to_vibrato_freq(0.0), vib_amount: 0.0, rise: 0.0, slide: 0.0 }
    }
}

/// What a voice sees of its device during note handling.
pub struct WsCx<'c, P> {
    pub p: &'c P,
    pub common: &'c Common,
    pub rate: f64,
    pub rng: &'c mut dyn Rng,
}

/// Random source for paths that never draw (note-offs from `set_param` / `all_notes_off`).
pub struct NoRng;
impl Rng for NoRng {
    fn next_f32(&mut self) -> f32 {
        0.0
    }
}

/// Upstream `SynthDevice::Voice` state: note, unison detune/pan, vibrato phase, legato slide.
#[derive(Clone, Copy, Debug, Default)]
pub struct VoiceBase {
    pub on: bool,
    pub note: u8,
    pub detune: f32,
    pub pan: f32,
    pub vib_phase: f64,
    slide_active: bool,
    slide_delta: f64,
    slide_samples: i32,
    dest: i32,
    current: f64,
}

impl VoiceBase {
    pub fn note_on(&mut self, note: u8, detune: f32, pan: f32) {
        self.on = true;
        self.note = note;
        self.detune = detune;
        self.pan = pan;
        self.current = note as f64;
        self.slide_active = false;
    }

    pub fn slide(&mut self, note: u8, slide: f32, rate: f64) {
        self.slide_active = true;
        self.dest = note as i32;
        let time = 10.0 * pow4(slide as f64);
        self.slide_delta = (note as f64 - self.current) / (rate * time);
        self.slide_samples = (rate * time) as i32;
    }

    /// Upstream `GetNote`, advancing the slide by one sample.
    #[inline]
    pub fn get_note(&mut self) -> f64 {
        if self.slide_active {
            self.current += self.slide_delta;
            self.slide_samples -= 1;
            if self.slide_samples < 0 {
                self.note = self.dest as u8;
                self.slide_active = false;
                self.current = self.dest as f64;
            }
        }
        self.current
    }

    /// Fresh-voice state (device reset).
    pub fn clear(&mut self) {
        *self = VoiceBase::default();
    }
}

/// Instrument core: voice pool, event slots and shared settings.
pub struct Synth<'a, V> {
    pub voices: VoicePool<'a, V>,
    pub events: Events<MAX_EVENTS>,
    pub common: Common,
}

impl<'a, V: VoiceState> Synth<'a, V> {
    pub fn new(pool: VoicePool<'a, V>) -> Self {
        Synth { voices: pool, events: Events::default(), common: Common::default() }
    }

    pub fn note_on(&mut self, note: u8, velocity: u8, offset: u32) {
        self.events.push(Event { offset, kind: EventKind::NoteOn { note, velocity } });
    }

    pub fn note_off(&mut self, note: u8, offset: u32) {
        self.events.push(Event { offset, kind: EventKind::NoteOff { note } });
    }

    /// Upstream `SynthDevice` common parameters, at their per-instrument indices.
    pub fn set_common<P>(&mut self, which: CommonParam, v: f32, p: &P)
    where
        V: for<'c> Voice<WsCx<'c, P>>,
    {
        match which {
            CommonParam::Unisono => self.voices.unison = param_to_unisono(v),
            CommonParam::Detune => self.voices.detune = v,
            CommonParam::Pan => self.voices.pan_spread = v,
            CommonParam::VibratoFreq => self.common.vib_freq = param_to_vibrato_freq(v),
            CommonParam::VibratoAmount => self.common.vib_amount = v,
            CommonParam::Rise => self.common.rise = v,
            CommonParam::Slide => self.common.slide = v,
            CommonParam::VoiceMode => {
                let mode = if (v * 1.0) as i32 == 1 { Mode::MonoLegato } else { Mode::Poly };
                let mut cx = WsCx { p, common: &self.common, rate: 0.0, rng: &mut NoRng };
                self.voices.set_mode(mode, &mut cx);
            }
        }
    }

    pub fn all_notes_off<P>(&mut self, p: &P)
    where
        V: for<'c> Voice<WsCx<'c, P>>,
    {
        let mut cx = WsCx { p, common: &self.common, rate: 0.0, rng: &mut NoRng };
        self.voices.all_notes_off(&mut cx);
        self.events.clear();
    }

    /// Upstream `SynthDevice::Run`: clear main, then fire due events and render active voices in index order.
    pub fn run<P>(
        &mut self,
        p: &P,
        io: &mut Io,
        n: usize,
        ctx: &mut Ctx,
        mut render: impl FnMut(&mut V, &P, &Common, &mut [f32], &mut [f32], &mut Ctx),
    ) where
        V: for<'c> Voice<WsCx<'c, P>>,
    {
        io.ch[MAIN_L][..n].fill(0.0);
        io.ch[MAIN_R][..n].fill(0.0);
        let [l, r, ..] = &mut io.ch;
        let Synth { voices, events, common } = self;
        events.run(n, |s| match s {
            Step::Event { kind, .. } => {
                let mut cx = WsCx { p, common, rate: ctx.rate, rng: &mut *ctx.rng };
                match kind {
                    EventKind::NoteOn { note, velocity } => voices.note_on(note, velocity, &mut cx),
                    EventKind::NoteOff { note } => voices.note_off(note, &mut cx),
                }
            }
            Step::Render { start, len } => {
                for v in voices.voices_mut().iter_mut().filter(|v| v.is_on()) {
                    render(v, p, common, &mut l[start..start + len], &mut r[start..start + len], ctx);
                }
            }
        });
    }
}

/// `SynthDevice` parameters every instrument maps from its own indices.
#[derive(Clone, Copy, Debug)]
pub enum CommonParam {
    Unisono,
    Detune,
    Pan,
    VibratoFreq,
    VibratoAmount,
    Rise,
    VoiceMode,
    Slide,
}
