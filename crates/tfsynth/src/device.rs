//! `synth-core` `Device` adapters (design D1b): the instrument as `synth-mix` kind 32 and each effect as a standalone
//! device, kinds 33–39. `TfDevice` is the registry enum (`Kinds`); combine it with another engine's registry through
//! `synth_core::mix::Either`. Device chunks are the `synth-mix` convention: `(size − 4)/4` f32 parameters, then the
//! i32 size; missing parameters keep their defaults (0).

use crate::fx::{kind as fx_kind, Chain, Fx};
use crate::instrument::Instrument;
use crate::params::*;
use crate::rng::Xorshift;
use crate::voice::BLOCK;
use synth_core::device::{Ctx, Device, Io, MAIN_L, MAIN_R};
use synth_core::mix::{Env, Kinds, LoadError};
use synth_core::pool::Pool;

/// `synth-mix` kind ids.
pub mod kind {
    pub const INSTRUMENT: u8 = 32;
    pub const DISTORTION: u8 = 33;
    pub const DELAY: u8 = 34;
    pub const CHORUS: u8 = 35;
    pub const FLANGER: u8 = 36;
    pub const REVERB: u8 = 37;
    pub const FORMANT: u8 = 38;
    pub const EQ: u8 = 39;
}

/// Standalone effect parameters: the effect's subset of the 112-value table, in table order.
pub fn effect_params(k: u8) -> &'static [usize] {
    match k {
        kind::DISTORTION => &[DIST_AMOUNT],
        kind::DELAY => &[DELAY_LEFT, DELAY_RIGHT, DELAY_DECAY],
        kind::CHORUS => &[CHORUS_RATE, CHORUS_DEPTH, CHORUS_GAIN],
        kind::FLANGER => &[FLANGER_LFO, FLANGER_FREQ, FLANGER_AMP, FLANGER_WET],
        kind::REVERB => &[REV_ROOM, REV_DAMP, REV_WET, REV_WIDTH],
        kind::FORMANT => &[FORMANT_TYPE, FORMANT_WET],
        kind::EQ => &[EQ_LOW, EQ_MID, EQ_HIGH],
        _ => &[],
    }
}

/// `chunk` parameters as `(index, value)`.
fn chunk_params(chunk: &[u8]) -> impl Iterator<Item = (usize, f32)> + '_ {
    let n = chunk.len().saturating_sub(4) / 4;
    chunk[..n * 4].chunks_exact(4).enumerate().map(|(i, b)| (i, f32::from_le_bytes([b[0], b[1], b[2], b[3]])))
}

/// Queued events (absolute song frames once the next `run` places them).
const QUEUE: usize = 512;

#[derive(Clone, Copy)]
struct Queued {
    frame: u64,
    note: u8,
    /// 0 = note-off.
    velocity: u8,
}

/// The Tunefish instrument on a shared track: 512-frame blocks on the absolute song grid, every event fired at
/// the start of the block that contains it (lookahead 511 guarantees it has arrived).
pub struct TfInstrument<'a> {
    ins: Instrument<'a>,
    queue: [Queued; QUEUE],
    len: usize,
    /// Events queued since the last `run` hold offsets until `run` learns its start frame.
    unplaced: usize,
    block: Option<u64>,
    buf: [[f32; BLOCK]; 2],
}

impl<'a> TfInstrument<'a> {
    fn patch(chunk: &[u8]) -> [f32; COUNT] {
        let mut p = [0.0f32; COUNT];
        chunk_params(chunk).filter(|(i, _)| *i < COUNT).for_each(|(i, v)| p[i] = v);
        p
    }

    pub fn need(chunk: &[u8]) -> usize {
        Instrument::need(Chain::need(&Self::patch(chunk)))
    }

    pub fn new(chunk: &[u8], pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let p = Self::patch(chunk);
        let mut ins = Instrument::new(pool, Chain::need(&p))?;
        ins.set_patch(&p).map_err(|_| LoadError::BadChunk)?;
        Ok(TfInstrument {
            ins,
            queue: [Queued { frame: 0, note: 0, velocity: 0 }; QUEUE],
            len: 0,
            unplaced: 0,
            block: None,
            buf: [[0.0; BLOCK]; 2],
        })
    }

    /// Pitch bend in [−1, 1] and mod wheel in [0, 1] (live control; songs never carry them).
    pub fn instrument(&mut self) -> &mut Instrument<'a> {
        &mut self.ins
    }

    fn push(&mut self, note: u8, velocity: u8, offset: u32) {
        if self.len < QUEUE {
            self.queue[self.len] = Queued { frame: offset as u64, note, velocity };
            self.len += 1;
            self.unplaced += 1;
        }
    }

    /// Fires queued events with frame < `end`, in arrival order, and drops them.
    fn fire_before(&mut self, end: u64) {
        let mut kept = 0;
        for i in 0..self.len {
            let e = self.queue[i];
            if e.frame < end {
                if e.note != 0 {
                    if e.velocity == 0 {
                        self.ins.note_off(e.note);
                    } else {
                        self.ins.note_on(e.note, e.velocity);
                    }
                }
            } else {
                self.queue[kept] = e;
                kept += 1;
            }
        }
        self.len = kept;
    }
}

impl Device for TfInstrument<'_> {
    fn param_count(&self) -> usize {
        COUNT
    }
    fn set_param(&mut self, index: usize, value: f32) {
        if index < COUNT {
            self.ins.params[index] = value;
        }
    }
    fn note_on(&mut self, note: u8, velocity: u8, offset: u32) {
        self.push(note, velocity, offset); // velocity 0 = note-off for Tunefish (.tfm semantics, design D1b)
    }
    fn note_off(&mut self, note: u8, offset: u32) {
        self.push(note, 0, offset);
    }
    fn all_notes_off(&mut self) {
        self.len = 0;
        self.unplaced = 0;
        self.ins.all_notes_off();
    }
    fn event_lookahead(&self) -> u32 {
        BLOCK as u32 - 1
    }
    fn reset(&mut self) {
        self.ins.reset();
        self.len = 0;
        self.unplaced = 0;
        self.block = None;
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        for e in &mut self.queue[self.len - self.unplaced..self.len] {
            e.frame += ctx.frame;
        }
        self.unplaced = 0;
        for i in 0..n {
            let f = ctx.frame + i as u64;
            let b = f / BLOCK as u64;
            if self.block != Some(b) {
                self.fire_before((b + 1) * BLOCK as u64);
                self.ins.render(&mut self.buf);
                self.block = Some(b);
            }
            let k = (f % BLOCK as u64) as usize;
            io.ch[MAIN_L][i] = self.buf[0][k];
            io.ch[MAIN_R][i] = self.buf[1][k];
        }
    }
}

/// One effect kind as a standalone device. Per-sample effects run at the caller's `n`; the chorus keeps its
/// block-rate control on a 512-frame grid counted across calls, so no output depends on the caller's block size.
pub struct TfEffect<'a> {
    k: u8,
    params: [f32; COUNT],
    fx: Fx<'a>,
    rng: Xorshift,
    frames: u64,
}

impl<'a> TfEffect<'a> {
    fn fx_kind(k: u8) -> u8 {
        k - kind::DISTORTION + fx_kind::DISTORTION
    }

    pub fn need(k: u8) -> usize {
        Fx::need(Self::fx_kind(k))
    }

    pub fn new(k: u8, chunk: &[u8], pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let mut rng = Xorshift::new();
        let fx = Fx::create(Self::fx_kind(k), pool, &mut rng)?;
        if matches!(fx, Fx::None) {
            return Err(LoadError::UnsupportedKind(k));
        }
        let mut d = TfEffect { k, params: [0.0; COUNT], fx, rng, frames: 0 };
        chunk_params(chunk).for_each(|(i, v)| d.set_param(i, v));
        Ok(d)
    }
}

impl Device for TfEffect<'_> {
    fn param_count(&self) -> usize {
        effect_params(self.k).len()
    }
    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(&i) = effect_params(self.k).get(index) {
            self.params[i] = value;
        }
    }
    fn reset(&mut self) {
        self.rng = Xorshift::new();
        self.fx.clear(&mut self.rng);
        self.frames = 0;
    }
    fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
        let [l, r, ..] = &mut io.ch;
        let (l, r) = (&mut l[..n], &mut r[..n]);
        match &mut self.fx {
            #[cfg(feature = "tf-fx-chorus")]
            Fx::Chorus(c) => {
                let mut i = 0;
                while i < n {
                    let at = (self.frames % BLOCK as u64) as usize;
                    if at == 0 {
                        c.control(&self.params);
                    }
                    let len = (n - i).min(BLOCK - at);
                    c.process(&mut l[i..i + len], &mut r[i..i + len]);
                    i += len;
                    self.frames += len as u64;
                }
            }
            fx => fx.run(&self.params, l, r),
        }
    }
}

/// Registry enum for `synth-mix` (kinds 32–39; effect kinds only when their `tf-fx-*` feature is on).
pub enum TfDevice<'a> {
    Instrument(TfInstrument<'a>),
    Effect(TfEffect<'a>),
}

fn effect_enabled(k: u8) -> bool {
    (kind::DISTORTION..=kind::EQ).contains(&k) && {
        let fk = TfEffect::fx_kind(k);
        cfg!(feature = "tf-fx-distortion") && fk == fx_kind::DISTORTION
            || cfg!(feature = "tf-fx-delay") && fk == fx_kind::DELAY
            || cfg!(feature = "tf-fx-chorus") && fk == fx_kind::CHORUS
            || cfg!(feature = "tf-fx-flanger") && fk == fx_kind::FLANGER
            || cfg!(feature = "tf-fx-reverb") && fk == fx_kind::REVERB
            || cfg!(feature = "tf-fx-formant") && fk == fx_kind::FORMANT
            || cfg!(feature = "tf-fx-eq") && fk == fx_kind::EQ
    }
}

impl<'a> Kinds<'a> for TfDevice<'a> {
    fn need(k: u8, chunk: &'a [u8], _: &Env<'a>) -> Result<usize, LoadError> {
        match k {
            kind::INSTRUMENT => Ok(TfInstrument::need(chunk)),
            k if effect_enabled(k) => Ok(TfEffect::need(k)),
            k => Err(LoadError::UnsupportedKind(k)),
        }
    }

    fn load(k: u8, chunk: &'a [u8], env: &Env<'a>, pool: &mut Pool<'a>, _: &mut dyn synth_core::rng::Rng)
        -> Result<Self, LoadError> {
        Self::need(k, chunk, env)?;
        Ok(match k {
            kind::INSTRUMENT => TfDevice::Instrument(TfInstrument::new(chunk, pool)?),
            k => TfDevice::Effect(TfEffect::new(k, chunk, pool)?),
        })
    }
}

macro_rules! each {
    ($self:ident, $d:ident => $e:expr) => {
        match $self {
            TfDevice::Instrument($d) => $e,
            TfDevice::Effect($d) => $e,
        }
    };
}

impl Device for TfDevice<'_> {
    fn param_count(&self) -> usize {
        each!(self, d => d.param_count())
    }
    fn set_param(&mut self, index: usize, value: f32) {
        each!(self, d => d.set_param(index, value))
    }
    fn note_on(&mut self, note: u8, velocity: u8, offset: u32) {
        each!(self, d => d.note_on(note, velocity, offset))
    }
    fn note_off(&mut self, note: u8, offset: u32) {
        each!(self, d => d.note_off(note, offset))
    }
    fn all_notes_off(&mut self) {
        each!(self, d => d.all_notes_off())
    }
    fn event_lookahead(&self) -> u32 {
        each!(self, d => d.event_lookahead())
    }
    fn reset(&mut self) {
        each!(self, d => d.reset())
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        each!(self, d => d.run(io, n, ctx))
    }
}
