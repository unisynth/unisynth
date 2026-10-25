//! Upstream `Slaughter`: 3 integrated-BLIT saw/pulse oscillators + noise, per-voice 2× SVF with mod envelope,
//! amp/mod/pitch envelopes, vibrato, rise, unison, mono-legato slide. Each of the 256 voices draws its 3 oscillator
//! start phases from the song random source at load (upstream voice constructors); phases and integrators run on
//! across notes.

use crate::helpers::{
    fast_sin, note_to_freq, pan_to_scalar_left, pan_to_scalar_right, param_to_frequency, scalar_to_env_value,
    volume_to_scalar,
};
use crate::svf::{Svf, SvfKind};
use crate::synth::{CommonParam, Envelope, EnvState, Synth, VoiceBase, WsCx, MAX_VOICES};
use synth_core::device::{Ctx, Device, Io};
use synth_core::math::clamp;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;
use synth_core::rng::Rng;
use synth_core::voice::{Voice, VoicePool, VoiceState};

#[derive(Clone, Copy)]
struct OscP {
    waveform: f32,
    pulse_width: f32,
    volume: f32,
    coarse: f32,
    fine: f32,
}

pub struct Params {
    osc: [OscP; 3],
    noise: f32,
    filter_kind: SvfKind,
    filter_freq: f32,
    filter_res: f32,
    filter_mod: f32,
    amp: [f32; 4],
    modenv: [f32; 4],
    pitch: [f32; 4],
    pitch_amt: f32,
    master: f32,
}

#[derive(Clone, Copy, Default)]
struct Osc {
    phase: f64,
    integral: f64,
}

impl Osc {
    /// Upstream `Oscillator::Next`: band-limited impulse train (FastSin sinc) integrated with leak 0.998.
    #[inline]
    fn next(&mut self, note: f64, waveform: f32, pulse_width: f32, rate: f64) -> f32 {
        let phase_max = rate * 0.5 / note_to_freq(note);
        let dc = -0.498 / phase_max;
        let mut phase2 = libm::fmod(self.phase + 2.0 * phase_max * pulse_width as f64, phase_max * 2.0) - phase_max;
        self.phase = libm::fmod(self.phase + 1.0, phase_max * 2.0);
        let mut tmp = self.phase - phase_max;
        const EPS: f64 = 0.0000001;
        let blit1 = if tmp > EPS || tmp < -EPS {
            tmp *= 3.141592;
            fast_sin(tmp) / tmp
        } else {
            1.0
        };
        let blit2 = if phase2 > EPS || phase2 < -EPS {
            phase2 *= 3.141592;
            fast_sin(phase2) / phase2
        } else {
            1.0
        };
        self.integral = 0.998 * self.integral + dc * (1.0 - waveform as f64) + blit1 - blit2 * waveform as f64;
        self.integral as f32
    }
}

pub struct SlaughterVoice {
    base: VoiceBase,
    amp: Envelope,
    modenv: Envelope,
    pitch: Envelope,
    osc: [Osc; 3],
    phase0: [f64; 3],
    filter: Svf,
}

impl VoiceState for SlaughterVoice {
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

impl<'c> Voice<WsCx<'c, Params>> for SlaughterVoice {
    fn note_on(&mut self, note: u8, _: u8, detune: f32, pan: f32, cx: &mut WsCx<'c, Params>) {
        self.base.note_on(note, detune, pan);
        let p = cx.p;
        for (e, s) in [(&mut self.amp, p.amp), (&mut self.modenv, p.modenv), (&mut self.pitch, p.pitch)] {
            e.set(s[0], s[1], s[2], s[3]);
            e.trigger();
        }
    }
    fn note_off(&mut self, _: &mut WsCx<'c, Params>) {
        self.amp.off();
        self.modenv.off();
        self.pitch.off();
    }
    fn slide(&mut self, note: u8, cx: &mut WsCx<'c, Params>) {
        self.base.slide(note, cx.common.slide, cx.rate);
    }
}

fn coarse_detune(d: f32) -> f64 {
    libm::floorf(d * 24.99) as f64
}

impl SlaughterVoice {
    fn run(&mut self, p: &Params, c: &crate::synth::Common, l: &mut [f32], r: &mut [f32], ctx: &mut Ctx) {
        let rate = ctx.rate;
        let vib = c.vib_freq / rate;
        self.filter.kind = p.filter_kind;
        self.filter.set_q(p.filter_res);
        let amp = -16.0 * volume_to_scalar(p.master);
        let (pl, pr) = (pan_to_scalar_left(self.base.pan), pan_to_scalar_right(self.base.pan));
        let det = p.osc.map(|o| coarse_detune(o.coarse) + o.fine as f64);
        let vol = p.osc.map(|o| o.volume * o.volume);
        let noise = p.noise * p.noise;
        for (lo, ro) in l.iter_mut().zip(r.iter_mut()) {
            self.filter.set_freq(clamp(
                p.filter_freq + self.modenv.value() * (20000.0 - 20.0) * (p.filter_mod * 2.0 - 1.0),
                0.0,
                20000.0 - 20.0,
            ));
            let base = self.base.get_note()
                + self.base.detune as f64
                + (self.pitch.value() * p.pitch_amt) as f64
                + fast_sin(self.base.vib_phase) * c.vib_amount as f64
                + (c.rise * 24.0) as f64;
            let mut mix = 0.0f32;
            for i in 0..3 {
                if vol[i] > 0.0 {
                    mix += self.osc[i].next(base + det[i], p.osc[i].waveform, p.osc[i].pulse_width, rate) * vol[i];
                }
            }
            if noise > 0.0 {
                mix += ctx.rng.next_f32() * noise;
            }
            let out = self.filter.next(mix, rate) * self.amp.value() * amp;
            *lo += out * pl;
            *ro += out * pr;
            self.amp.next(rate);
            if self.amp.state == EnvState::Finished {
                self.base.on = false;
                break;
            }
            self.base.vib_phase += vib;
            self.modenv.next(rate);
            self.pitch.next(rate);
        }
    }
}

pub struct Slaughter<'a> {
    p: Params,
    synth: Synth<'a, SlaughterVoice>,
}

impl<'a> Slaughter<'a> {
    pub fn need(_: &[u8], _: &Env) -> Result<usize, LoadError> {
        Ok(VoicePool::<SlaughterVoice>::need(MAX_VOICES))
    }

    pub fn new(_: &[u8], _: &Env, pool: &mut Pool<'a>, rng: &mut dyn Rng) -> Result<Self, LoadError> {
        let voices = VoicePool::new(pool, MAX_VOICES, |_| {
            let phase0 = [0; 3].map(|_| rng.next_f32() as f64 * 2.0 * 3.141592);
            SlaughterVoice {
                base: VoiceBase::default(),
                amp: Envelope::new(),
                modenv: Envelope::new(),
                pitch: Envelope::new(),
                osc: phase0.map(|phase| Osc { phase, integral: 0.0 }),
                phase0,
                filter: Svf::new(SvfKind::Lowpass),
            }
        })?;
        let osc = OscP { waveform: 0.0, pulse_width: 0.5, volume: 0.0, coarse: 0.0, fine: 0.0 };
        let mut p = Params {
            osc: [osc; 3],
            noise: 0.0,
            filter_kind: SvfKind::Lowpass,
            filter_freq: 20000.0 - 20.0,
            filter_res: 1.0,
            filter_mod: 0.5,
            amp: [1.0, 5.0, 0.5, 1.5],
            modenv: [1.0, 5.0, 1.0, 1.5],
            pitch: [1.0, 5.0, 0.5, 1.5],
            pitch_amt: 0.0,
            master: 0.5,
        };
        p.osc[0].volume = 1.0;
        Ok(Slaughter { p, synth: Synth::new(voices) })
    }
}

impl Device for Slaughter<'_> {
    fn param_count(&self) -> usize {
        42
    }
    fn set_param(&mut self, i: usize, v: f32) {
        let p = &mut self.p;
        match i {
            0..=14 => {
                let o = &mut p.osc[i / 5];
                match i % 5 {
                    0 => o.waveform = v,
                    1 => o.pulse_width = 1.0 - v,
                    2 => o.volume = v,
                    3 => o.coarse = v,
                    _ => o.fine = v,
                }
            }
            15 => p.noise = v,
            16 => p.filter_kind = SvfKind::from_param(v),
            17 => p.filter_freq = param_to_frequency(v),
            18 => p.filter_res = 1.0 - v,
            19 => p.filter_mod = v,
            20..=23 => p.amp[i - 20] = if i == 22 { v } else { scalar_to_env_value(v) },
            24..=27 => p.modenv[i - 24] = if i == 26 { v } else { scalar_to_env_value(v) },
            28 => p.master = v,
            29 => self.synth.set_common(CommonParam::Unisono, v, &self.p),
            30 => self.synth.set_common(CommonParam::Detune, v, &self.p),
            31 => self.synth.set_common(CommonParam::Pan, v, &self.p),
            32 => self.synth.set_common(CommonParam::VibratoFreq, v, &self.p),
            33 => self.synth.set_common(CommonParam::VibratoAmount, v, &self.p),
            34 => self.synth.set_common(CommonParam::Rise, v, &self.p),
            35..=38 => p.pitch[i - 35] = if i == 37 { v } else { scalar_to_env_value(v) },
            39 => p.pitch_amt = (v - 0.5) * 2.0 * 36.0,
            40 => self.synth.set_common(CommonParam::VoiceMode, v, &self.p),
            41 => self.synth.set_common(CommonParam::Slide, v, &self.p),
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
            v.amp = Envelope::new();
            v.modenv = Envelope::new();
            v.pitch = Envelope::new();
            v.osc = v.phase0.map(|phase| Osc { phase, integral: 0.0 });
            v.filter.clear();
        }
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        self.synth.run(&self.p, io, n, ctx, |v, p, c, l, r, ctx| v.run(p, c, l, r, ctx));
    }
}
