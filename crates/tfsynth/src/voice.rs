//! One voice (spec §3.1.5, §3.7, §3.8): mod matrix, noise, pitch (bend, slop, glide), the wavetable generator
//! with unison/detune/spread/drive/pan, the four filters, and the gain-curve mix into the instrument bus.

use crate::filter::{Ladder, Notch, Rbj, RbjKind};
use crate::modmx::{dst, ModMatrix};
use crate::noise::Noise;
use crate::params::*;
use crate::rng::Xorshift;
use crate::tables::{tables, SEMITONE};
use crate::wavetable::{band_bins, Shape, Table};
use synth_core::rng::Rng;
use synth_core::voice::{Voice as PoolVoice, VoiceState};

pub const BLOCK: usize = 512;
const RATE: f32 = 44100.0;
pub const MAX_LAYERS: usize = 10;

/// What a voice reads from its instrument at note events.
pub struct NoteCx<'c> {
    pub params: &'c [f32; COUNT],
    /// Instrument LFO phases (free-running approximation; replaced by 0 when the LFO syncs).
    pub lfo_phase: [f32; 2],
    pub rng: &'c mut Xorshift,
}

pub struct Voice {
    note: u8,
    velocity: u8,
    /// Key held (note-off not yet received).
    pub held: bool,
    /// Rendered more than 1.0 total |sample| last block.
    pub sounding: bool,
    /// Blocks rendered since note-on.
    time: u32,
    slop: f32,
    pub freq: f32,
    /// Pitch bend in [−1, 1].
    pub bend: f32,
    pub mm: ModMatrix,
    table: Table,
    mu: f32,
    /// Oscillator phases, `[left, right]` per unison layer.
    phase: [[f32; 2]; MAX_LAYERS],
    last_vol: [f32; 2],
    noise: Noise,
    lp: Ladder,
    hp: Rbj,
    bp: Rbj,
    nt: Notch,
}

impl Voice {
    pub fn new() -> Self {
        Voice {
            note: 0,
            velocity: 0,
            held: false,
            sounding: false,
            time: 0,
            slop: 0.0,
            freq: 0.0,
            bend: 0.0,
            mm: ModMatrix::new(),
            table: Table::new(),
            mu: 0.0,
            phase: [[0.0; 2]; MAX_LAYERS],
            last_vol: [0.0; 2],
            noise: Noise::default(),
            lp: Ladder::default(),
            hp: Rbj::default(),
            bp: Rbj::default(),
            nt: Notch::default(),
        }
    }

    pub fn active(&self) -> bool {
        self.held || self.sounding
    }

    /// Renders one block into `tmp` and mixes it into `out` with the gain curve. Returns the voice's LFO phases
    /// (the instrument keeps the last rendered voice's as its free-running phase).
    pub fn render(&mut self, p: &[f32; COUNT], wheel: f32, tmp: &mut [[f32; BLOCK]; 2], out: &mut [[f32; BLOCK]; 2]) -> [f32; 2] {
        self.time += 1;
        let env_running = self.mm.step(p, wheel);
        let lfo = [self.mm.lfo[0].phase, self.mm.lfo[1].phase];
        let mut velocity = self.velocity as f32 / 128.0;
        if !env_running && !self.held {
            velocity = 0.0; // no envelope routed (or both finished) and the key is up: organ gate closes
        }
        let mm = self.mm; // outputs are fixed for the rest of the block
        let m = |d| mm.get(d);
        let [tl, tr] = tmp;
        self.noise.update(p, velocity, m(dst::NOISE));
        self.noise.run(tl, tr);

        self.pitch(p);
        if self.time % 4 == 1 {
            let shape = Shape {
                harmonics: harmonics((p[GEN_HARMONICS] * m(dst::HARMONICS)).clamp(0.0, 1.0)),
                bandwidth: (p[GEN_BANDWIDTH] * m(dst::BANDWIDTH)).clamp(0.0, 1.0),
                damp: (p[GEN_DAMP] * m(dst::DAMP)).clamp(0.0, 1.0),
                scale: (p[GEN_SCALE] * 4.0 * m(dst::SCALE)).clamp(0.0, 4.0),
                bins: band_bins(self.freq),
            };
            self.table.update(&shape);
            self.table.render(p[GEN_MOD], &mut self.mu);
        }
        self.generate(p, velocity, tl, tr);

        if p[LP_ON] > 0.5 {
            self.lp.set(p[LP_CUT] * m(dst::LP_CUT), p[LP_RES] * m(dst::LP_RES));
            self.lp.run(tl, tr);
        }
        if p[HP_ON] > 0.5 {
            self.hp.set(RbjKind::HighPass, p[HP_CUT] * m(dst::HP_CUT), p[HP_RES] * m(dst::HP_RES));
            self.hp.run(tl, tr);
        }
        if p[BP_ON] > 0.5 {
            self.bp.set(RbjKind::BandPass, p[BP_CUT] * m(dst::BP_CUT), p[BP_Q] * m(dst::BP_Q));
            self.bp.run(tl, tr);
        }
        if p[NT_ON] > 0.5 {
            self.nt.set(p[NT_CUT] * m(dst::NT_CUT), p[NT_Q] * m(dst::NT_Q));
            self.nt.run(tl, tr);
        }

        let g = gain_curve(p[GAIN]);
        let mut level = 0.0f32;
        for c in 0..2 {
            for (o, &v) in out[c].iter_mut().zip(tmp[c].iter()) {
                level += libm::fabsf(v);
                *o += v * g;
            }
        }
        self.sounding = level > 1.0;
        lfo
    }

    /// Keyboard pitch: note → Hz, linear-in-Hz bend, slop, per-block glide (state per voice, never reset).
    fn pitch(&mut self, p: &[f32; COUNT]) {
        let base = tables().note_hz[(self.note & 0x7f) as usize];
        let (mut up, mut down) = (base, base);
        for _ in 0..bend_semitones(p[BEND_UP]) {
            up *= SEMITONE;
        }
        let inv = 1.0 / SEMITONE;
        for _ in 0..bend_semitones(p[BEND_DOWN]) {
            down *= inv;
        }
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let mut f = lerp(down, base, (self.bend + 1.0).clamp(0.0, 1.0));
        f = lerp(f, up, self.bend.clamp(0.0, 1.0));
        f += self.slop * cube(p[GEN_SLOP]) * 8.0;
        let glide = p[GEN_GLIDE];
        if glide > 0.0 && self.freq > 0.0 {
            self.freq += (f - self.freq) / (glide * 10.0 + 1.0);
        } else {
            self.freq = f;
        }
    }

    /// Unison table playback (spec §3.1.5): non-interpolated reads, drive + hard clip, per-block linear volume ramp
    /// from the previous block's gains, linear balance pan. Adds into `l`/`r`.
    fn generate(&mut self, p: &[f32; COUNT], velocity: f32, l: &mut [f32], r: &mut [f32]) {
        let mut vol = p[GEN_VOLUME] * 4.0 * velocity;
        let held = self.last_vol[0].max(self.last_vol[1]);
        if held.max(vol) <= 0.0 {
            return;
        }
        let mm = self.mm;
        let m = |d| mm.get(d);
        let fixed = p[GEN_FREQ];
        let key_track = fixed < 0.00001;
        vol *= m(dst::VOLUME);
        let detune = p[GEN_DETUNE] * m(dst::DETUNE);
        let drive = p[GEN_DRIVE] * m(dst::DRIVE);
        let spread = p[GEN_SPREAD] * m(dst::SPREAD);
        let pan = (p[GEN_PAN] * m(dst::PAN)).clamp(0.0, 1.0);
        let fm = m(dst::FREQ);
        if held.max(vol) < 0.001 {
            return;
        }
        let detune = cube(detune) * 10.0;
        let drive = drive * 32.0 + 1.0;
        let spread = libm::pow(spread as f64, 4.0) as f32 / RATE * 10.0;
        let oct = octave_factor(p[GEN_OCTAVE]);
        let (mut f1, mut f2) = if key_track {
            (self.freq * oct + detune, self.freq * oct - detune)
        } else {
            let f = libm::pow(fixed as f64, 2.0) as f32 * 1000.0;
            (f, f)
        };
        f1 = f1 * fm / RATE;
        f2 = f2 * fm / RATE;
        let vol = vol * 0.1;
        let target = [
            if pan <= 0.5 { vol } else { vol * (1.0 - (pan - 0.5) * 2.0) },
            if pan >= 0.5 { vol } else { vol * (pan * 2.0) },
        ];
        let wave = self.table.wave();
        for layer in self.phase[..unison(p[GEN_UNISON])].iter_mut() {
            let mut v = self.last_vol;
            let dv = [(target[0] - v[0]) / BLOCK as f32, (target[1] - v[1]) / BLOCK as f32];
            f1 = libm::fabsf(f1);
            f2 = libm::fabsf(f2);
            for (ol, or) in l.iter_mut().zip(r.iter_mut()) {
                let a = wave[(layer[0] * 511.0) as usize];
                let b = wave[(layer[1] * 511.0) as usize];
                *ol += (a * drive).clamp(-1.0, 1.0) * v[0];
                *or += (b * drive).clamp(-1.0, 1.0) * v[1];
                layer[0] += f1;
                while layer[0] > 1.0 {
                    layer[0] -= 1.0;
                }
                layer[1] += f2;
                while layer[1] > 1.0 {
                    layer[1] -= 1.0;
                }
                v[0] += dv[0];
                v[1] += dv[1];
            }
            f1 += spread;
            f2 -= spread;
        }
        self.last_vol = target;
    }
}

fn cube(x: f32) -> f32 {
    libm::pow(x as f64, 3.0) as f32
}

impl Default for Voice {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceState for Voice {
    fn is_on(&self) -> bool {
        self.active()
    }
    fn note(&self) -> u8 {
        self.note
    }
    fn kill(&mut self) {
        self.held = false;
        self.sounding = false;
    }
}

impl<'c> PoolVoice<NoteCx<'c>> for Voice {
    /// Hard restart (also on a stolen voice): envelopes from 0, LFO phases, slop, oscillator phases, drift, noise
    /// offsets — drawn in that order. Filter state and glide frequency are kept.
    fn note_on(&mut self, note: u8, velocity: u8, _: f32, _: f32, cx: &mut NoteCx<'c>) {
        self.note = note;
        self.velocity = velocity;
        self.slop = cx.rng.range(-1.0, 1.0);
        self.held = true;
        self.time = 0;
        self.last_vol = [0.0; 2];
        self.mm.note_on(cx.lfo_phase);
        for layer in self.phase.iter_mut() {
            let base = cx.rng.next_f32();
            let right = base + cx.rng.next_f32() * 0.1;
            // fix: the reference lets the right phase start above 1 (an out-of-table first read); wrap it
            *layer = [base, if right >= 1.0 { right - 1.0 } else { right }];
        }
        self.mu = cx.rng.range(0.0, 100.0);
        self.noise.offset = [cx.rng.below(32768) as usize, cx.rng.below(32768) as usize];
    }

    /// Releases only a held voice (a stolen voice now plays another note and is not matched by the pool).
    fn note_off(&mut self, _: &mut NoteCx<'c>) {
        if self.held {
            self.held = false;
            self.mm.note_off();
        }
    }

    fn slide(&mut self, _: u8, _: &mut NoteCx<'c>) {}
}
