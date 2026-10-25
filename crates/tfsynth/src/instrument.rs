//! One Tunefish instrument (spec §2.2, §3.7): 16 voices on `synth-core`'s voice pool with oldest-voice stealing and
//! the `GenPoly` limit, instrument-wide pitch bend and mod wheel, and the effects auto-sleep timer. Renders
//! 512-frame blocks.

use crate::fx::Chain;
use crate::params::{COUNT, GEN_POLY, LFO1, LFO2, polyphony};
use crate::rng::Xorshift;
use crate::voice::{NoteCx, Voice, BLOCK};
use synth_core::mix::LoadError;
use synth_core::pool::Pool;
use synth_core::voice::{Steal, VoicePool};

pub const VOICES: usize = 16;
/// Seconds of digital silence after which effects stop processing.
pub const FX_SLEEP: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadPatch;

pub struct Instrument<'a> {
    pub params: [f32; COUNT],
    voices: VoicePool<'a, Voice>,
    lfo_phase: [f32; 2],
    wheel: f32,
    rng: Xorshift,
    /// Seconds of silent output with no voice rendering (auto-sleep).
    pub idle: f32,
    tmp: [[f32; BLOCK]; 2],
    fx: Chain<'a>,
}

impl<'a> Instrument<'a> {
    /// Pool bytes for an instrument whose effect pool holds `fx` bytes (see [`Chain::need`]).
    pub fn need(fx: usize) -> usize {
        VoicePool::<Voice>::need(VOICES) + Pool::need::<u8>(fx)
    }

    /// Voices and an effect pool of `fx` bytes, both from `pool`.
    pub fn new(pool: &mut Pool<'a>, fx: usize) -> Result<Self, LoadError> {
        let fx = Chain::new(Pool::new(pool.slice::<u8>(fx)?));
        let mut voices = VoicePool::new(pool, VOICES, |_| Voice::new())?;
        voices.steal = Steal::Oldest;
        Ok(Instrument {
            params: [0.0; COUNT],
            voices,
            lfo_phase: [0.0; 2],
            wheel: 0.0,
            rng: Xorshift::new(),
            idle: 0.0,
            tmp: [[0.0; BLOCK]; 2],
            fx,
        })
    }

    /// Fresh-load state (patch kept): voices, LFO phases, wheel, bend, random source, sleep timer, effect state.
    pub fn reset(&mut self) {
        self.voices.clear_state();
        self.voices.voices_mut().iter_mut().for_each(|v| *v = Voice::new());
        self.lfo_phase = [0.0; 2];
        self.wheel = 0.0;
        self.rng = Xorshift::new();
        self.idle = 0.0;
        self.fx.reset();
    }

    /// Loads a 112-value patch; any other length is rejected and nothing changes.
    pub fn set_patch(&mut self, p: &[f32]) -> Result<(), BadPatch> {
        let p: &[f32; COUNT] = p.try_into().map_err(|_| BadPatch)?;
        self.params = *p;
        Ok(())
    }

    fn cx(&mut self) -> (&mut VoicePool<'a, Voice>, NoteCx<'_>) {
        let phase = [
            if self.params[LFO1 + 3] < 0.5 { self.lfo_phase[0] } else { 0.0 },
            if self.params[LFO2 + 3] < 0.5 { self.lfo_phase[1] } else { 0.0 },
        ];
        (&mut self.voices, NoteCx { params: &self.params, lfo_phase: phase, rng: &mut self.rng })
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
        if velocity == 0 {
            return self.note_off(note);
        }
        let limit = polyphony(self.params[GEN_POLY]);
        let (voices, mut cx) = self.cx();
        voices.limit = limit;
        voices.note_on(note, velocity, &mut cx);
    }

    pub fn note_off(&mut self, note: u8) {
        let (voices, mut cx) = self.cx();
        voices.note_off(note, &mut cx);
    }

    /// Releases every held voice (tails keep sounding).
    pub fn all_notes_off(&mut self) {
        let (voices, mut cx) = self.cx();
        voices.all_notes_off(&mut cx);
    }

    /// Pitch bend b ∈ [−1, 1] on all voices, held or not.
    pub fn pitch_bend(&mut self, b: f32) {
        self.voices.voices_mut().iter_mut().for_each(|v| v.bend = b);
    }

    /// Mod wheel in [0, 1] (mod source 5), from the next block.
    pub fn mod_wheel(&mut self, w: f32) {
        self.wheel = w;
    }

    pub fn voices(&self) -> &[Voice] {
        self.voices.voices()
    }

    /// Renders one block: voices, then the effect chain unless asleep, then the silence timer. Flush-to-zero is on
    /// for the call (as in the reference; it also keeps decaying filter and effect state off slow subnormals).
    pub fn render(&mut self, out: &mut [[f32; BLOCK]; 2]) {
        let _ftz = synth_core::denormal::Guard::new();
        self.render_voices(out);
        if !self.asleep() {
            let [l, r] = out;
            self.fx.run(&self.params, &mut self.rng, l, r);
        }
        self.track_silence(out);
    }

    pub fn fx(&self) -> &Chain<'a> {
        &self.fx
    }

    /// Overwrites `out` with the sum of the active voices (no effects).
    pub fn render_voices(&mut self, out: &mut [[f32; BLOCK]; 2]) {
        out[0].fill(0.0);
        out[1].fill(0.0);
        let p = &self.params;
        for v in self.voices.voices_mut().iter_mut().filter(|v| v.active()) {
            self.idle = 0.0;
            self.lfo_phase = v.render(p, self.wheel, &mut self.tmp, out);
        }
        self.voices.tick();
    }

    /// Advances the auto-sleep timer from this block's output level (mean |x| of L and R averaged < 1e-5).
    pub fn track_silence(&mut self, out: &[[f32; BLOCK]; 2]) {
        let mean = |c: &[f32; BLOCK]| c.iter().map(|v| libm::fabsf(*v)).sum::<f32>() / BLOCK as f32;
        if libm::fabsf((mean(&out[0]) + mean(&out[1])) / 2.0) < 1e-5 {
            self.idle += BLOCK as f32 / 44100.0;
        }
    }

    pub fn asleep(&self) -> bool {
        self.idle >= FX_SLEEP
    }
}
