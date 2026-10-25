//! Upstream `Thunder` (kind 2, deprecated): plays its GSM sample once from the start at unity pitch, mono to both
//! sides, no envelope or gain; note-off does nothing. No parameters. Chunk layout as Specimen without params.

use crate::gsm::GsmChunk;
use crate::synth::{Synth, WsCx, MAX_VOICES};
use synth_core::device::{Ctx, Device, Io};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;
use synth_core::rng::Rng;
use synth_core::voice::{Voice, VoicePool, VoiceState};

pub struct Params<'a> {
    data: &'a [f32],
}

#[derive(Default)]
pub struct ThunderVoice {
    on: bool,
    note: u8,
    pos: usize,
}

impl VoiceState for ThunderVoice {
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

impl<'c, 'a> Voice<WsCx<'c, Params<'a>>> for ThunderVoice {
    fn note_on(&mut self, note: u8, _: u8, _: f32, _: f32, _: &mut WsCx<'c, Params<'a>>) {
        self.on = true;
        self.note = note;
        self.pos = 0;
    }
    fn note_off(&mut self, _: &mut WsCx<'c, Params<'a>>) {}
    /// Upstream slides only move the pitch, which Thunder ignores.
    fn slide(&mut self, note: u8, _: &mut WsCx<'c, Params<'a>>) {
        self.note = note;
    }
}

pub struct Thunder<'a> {
    p: Params<'a>,
    synth: Synth<'a, ThunderVoice>,
}

impl<'a> Thunder<'a> {
    pub fn need(chunk: &[u8], _: &Env) -> Result<usize, LoadError> {
        Ok(VoicePool::<ThunderVoice>::need(MAX_VOICES) + GsmChunk::pool_need(chunk)?)
    }

    pub fn new(chunk: &'a [u8], _: &Env, pool: &mut Pool<'a>, _: &mut dyn Rng) -> Result<Self, LoadError> {
        let data = match GsmChunk::parse(chunk)? {
            Some(g) => g.decode(pool)?,
            None => &[],
        };
        let voices = VoicePool::new(pool, MAX_VOICES, |_| ThunderVoice::default())?;
        Ok(Thunder { p: Params { data }, synth: Synth::new(voices) })
    }
}

impl Device for Thunder<'_> {
    fn param_count(&self) -> usize {
        0
    }
    fn set_param(&mut self, _: usize, _: f32) {}
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
        self.synth.voices.voices_mut().iter_mut().for_each(|v| *v = ThunderVoice::default());
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        self.synth.run(&self.p, io, n, ctx, |v, p, _, l, r, _| {
            for (lo, ro) in l.iter_mut().zip(r.iter_mut()) {
                let Some(&s) = p.data.get(v.pos) else {
                    v.on = false;
                    break;
                };
                *lo += s;
                *ro += s;
                v.pos += 1;
            }
        });
    }
}
