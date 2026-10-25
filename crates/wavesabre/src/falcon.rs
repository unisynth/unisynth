//! Upstream `Falcon`: two-operator phase-modulation synth (sine + Square35 blend per operator, operator 1 feedback
//! and feed-forward into operator 2), per-operator envelopes, pitch envelope, vibrato, unison, legato slide. Both
//! operator phases restart from one song random draw at note-on.

use crate::helpers::{
    fast_sin, note_to_freq, pan_to_scalar_left, pan_to_scalar_right, scalar_to_env_value, square35, volume_to_scalar,
};
use crate::synth::{CommonParam, Envelope, EnvState, Synth, VoiceBase, WsCx, MAX_VOICES};
use synth_core::device::{Ctx, Device, Io};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;
use synth_core::rng::Rng;
use synth_core::voice::{Voice, VoicePool, VoiceState};

pub struct Params {
    wf: [f32; 2],
    coarse: [f32; 2],
    fine: [f32; 2],
    feedback: [f32; 2],
    feed_forward: f32,
    env: [[f32; 4]; 2],
    master: f32,
    pitch: [f32; 4],
    pitch_amt: [f32; 2],
}

pub struct FalconVoice {
    base: VoiceBase,
    env: [Envelope; 2],
    pitch: Envelope,
    phase: [f64; 2],
    out: [f64; 2],
}

impl VoiceState for FalconVoice {
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

impl<'c> Voice<WsCx<'c, Params>> for FalconVoice {
    fn note_on(&mut self, note: u8, _: u8, detune: f32, pan: f32, cx: &mut WsCx<'c, Params>) {
        self.base.note_on(note, detune, pan);
        let ph = cx.rng.next_f32() as f64;
        self.phase = [ph, ph];
        let p = cx.p;
        for (e, s) in self.env.iter_mut().zip(p.env) {
            e.set(s[0], s[1], s[2], s[3]);
            e.trigger();
        }
        self.pitch.set(p.pitch[0], p.pitch[1], p.pitch[2], p.pitch[3]);
        self.pitch.trigger();
        self.out = [0.0; 2];
    }
    fn note_off(&mut self, _: &mut WsCx<'c, Params>) {
        self.env[0].off();
        self.env[1].off();
        self.pitch.off();
    }
    fn slide(&mut self, note: u8, cx: &mut WsCx<'c, Params>) {
        self.base.slide(note, cx.common.slide, cx.rate);
    }
}

/// Upstream `ratioScalar`: `1 + floor(coarse·32.99) + ((fine − 0.5)·2)³`.
fn ratio(coarse: f64, fine: f64) -> f64 {
    let f = (fine - 0.5) * 2.0;
    1.0 + libm::floor(coarse * 32.99) + f * f * f
}

impl FalconVoice {
    fn run(&mut self, p: &Params, c: &crate::synth::Common, l: &mut [f32], r: &mut [f32], ctx: &mut Ctx) {
        let rate = ctx.rate;
        let vib = c.vib_freq / rate;
        let fb1 = (p.feedback[0] * p.feedback[0]) as f64 / 2.0;
        let fb2 = (p.feedback[1] * p.feedback[1]) as f64 / 2.0;
        let ff = p.feed_forward * p.feed_forward;
        let master = volume_to_scalar(p.master);
        let (pl, pr) = (pan_to_scalar_left(self.base.pan), pan_to_scalar_right(self.base.pan));
        let r1 = ratio(p.coarse[0] as f64, p.fine[0] as f64);
        let r2 = ratio(p.coarse[1] as f64, p.fine[1] as f64);
        for (lo, ro) in l.iter_mut().zip(r.iter_mut()) {
            let base = self.base.get_note() + self.base.detune as f64 + (c.rise * 24.0) as f64;
            let in1 = self.phase[0] / rate * 2.0 * 3.141592 + self.out[0] * fb1;
            self.out[0] =
                (fast_sin(in1) + square35(in1) * p.wf[0] as f64) * self.env[0].value() as f64 * 13.25;
            let in2 = self.phase[1] / rate * 2.0 * 3.141592 + self.out[1] * fb2 * 13.25 + self.out[0] * ff as f64;
            self.out[1] = (fast_sin(in2) + square35(in2) * p.wf[1] as f64) * self.env[1].value() as f64;
            let out = self.out[1] as f32 * master;
            *lo += out * pl;
            *ro += out * pr;
            self.env[1].next(rate);
            if self.env[1].state == EnvState::Finished {
                self.base.on = false;
                break;
            }
            let pe = self.pitch.value();
            let vibrato = fast_sin(self.base.vib_phase) * c.vib_amount as f64;
            let f1 = note_to_freq(base + (pe * p.pitch_amt[0]) as f64 + vibrato);
            let f2 = note_to_freq(base + (pe * p.pitch_amt[1]) as f64 + vibrato);
            self.phase[0] += f1 * r1;
            self.phase[1] += f2 * r2;
            self.base.vib_phase += vib;
            self.env[0].next(rate);
            self.pitch.next(rate);
        }
    }
}

pub struct Falcon<'a> {
    p: Params,
    synth: Synth<'a, FalconVoice>,
}

impl<'a> Falcon<'a> {
    pub fn need(_: &[u8], _: &Env) -> Result<usize, LoadError> {
        Ok(VoicePool::<FalconVoice>::need(MAX_VOICES))
    }

    pub fn new(_: &[u8], _: &Env, pool: &mut Pool<'a>, _: &mut dyn Rng) -> Result<Self, LoadError> {
        let voices = VoicePool::new(pool, MAX_VOICES, |_| FalconVoice {
            base: VoiceBase::default(),
            env: [Envelope::new(); 2],
            pitch: Envelope::new(),
            phase: [0.0; 2],
            out: [0.0; 2],
        })?;
        let p = Params {
            wf: [0.0; 2],
            coarse: [0.0; 2],
            fine: [0.5; 2],
            feedback: [0.0; 2],
            feed_forward: 0.0,
            env: [[1.0, 1.0, 1.0, 1.0], [1.0, 5.0, 0.75, 1.5]],
            master: 0.8,
            pitch: [1.0, 5.0, 0.5, 1.5],
            pitch_amt: [0.0; 2],
        };
        Ok(Falcon { p, synth: Synth::new(voices) })
    }
}

impl Device for Falcon<'_> {
    fn param_count(&self) -> usize {
        32
    }
    fn set_param(&mut self, i: usize, v: f32) {
        let p = &mut self.p;
        let env = |i: usize, v: f32, sustain: usize| if i == sustain { v } else { scalar_to_env_value(v) };
        match i {
            0 => p.wf[0] = v,
            1 => p.coarse[0] = v,
            2 => p.fine[0] = v,
            3 => p.feedback[0] = v,
            4 => p.feed_forward = v,
            5..=8 => p.env[0][i - 5] = env(i, v, 7),
            9 => p.wf[1] = v,
            10 => p.coarse[1] = v,
            11 => p.fine[1] = v,
            12 => p.feedback[1] = v,
            13..=16 => p.env[1][i - 13] = env(i, v, 15),
            17 => p.master = v,
            18 => self.synth.set_common(CommonParam::Unisono, v, &self.p),
            19 => self.synth.set_common(CommonParam::Detune, v, &self.p),
            20 => self.synth.set_common(CommonParam::Pan, v, &self.p),
            21 => self.synth.set_common(CommonParam::VibratoFreq, v, &self.p),
            22 => self.synth.set_common(CommonParam::VibratoAmount, v, &self.p),
            23 => self.synth.set_common(CommonParam::Rise, v, &self.p),
            24..=27 => p.pitch[i - 24] = env(i, v, 26),
            28 => p.pitch_amt[0] = (v - 0.5) * 2.0 * 36.0,
            29 => p.pitch_amt[1] = (v - 0.5) * 2.0 * 36.0,
            30 => self.synth.set_common(CommonParam::VoiceMode, v, &self.p),
            31 => self.synth.set_common(CommonParam::Slide, v, &self.p),
            _ => {}
        }
    }
    fn note_on(&mut self, note: u8, velocity: u8, offset: u32) {
        self.synth.note_on(note, velocity, offset);
    }
    fn note_off(&mut self, note: u8, offset: u32) {
        self.synth.note_off(note, offset);
    }
    fn all_notes_off(&mut self) {
        self.synth.all_notes_off(&self.p);
    }
    fn reset(&mut self) {
        self.synth.events.clear();
        self.synth.voices.clear_state();
        for v in self.synth.voices.voices_mut() {
            v.base.clear();
            v.env = [Envelope::new(); 2];
            v.pitch = Envelope::new();
            v.phase = [0.0; 2];
            v.out = [0.0; 2];
        }
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        self.synth.run(&self.p, io, n, ctx, |v, p, c, l, r, ctx| v.run(p, c, l, r, ctx));
    }
}
