//! Fixed-capacity voice pool shared by the engines. Allocation follows upstream WaveSabre `SynthDevice`
//! (unison voices take the first free slots, mono-legato note log, note-off releases matching voices); Tunefish adds
//! a polyphony limit and oldest-voice stealing (age in blocks, advanced by [`VoicePool::tick`]).

use crate::pool::{OutOfMemory, Pool};

/// The context-free part of a voice.
pub trait VoiceState {
    /// Sounding (held or releasing). A voice switches itself off when its release ends.
    fn is_on(&self) -> bool;
    fn note(&self) -> u8;
    /// Silences immediately (mode change).
    fn kill(&mut self);
}

/// Note handling of a voice. `C` is what the engine's voices need from their device (upstream voices hold a pointer
/// to their device): settings, sample rate, the song random source.
pub trait Voice<C: ?Sized>: VoiceState {
    /// (Re)starts the voice. `detune` and `pan` are the unison spread values (`pan` 0..1, 0.5 = centre).
    fn note_on(&mut self, note: u8, velocity: u8, detune: f32, pan: f32, cx: &mut C);
    /// Starts the release.
    fn note_off(&mut self, cx: &mut C);
    /// Mono-legato glide to `note` without retriggering.
    fn slide(&mut self, note: u8, cx: &mut C);
    /// The engine plays mono-legato; `false` compiles the pool's note log out (the pool then plays every mode poly).
    const MONO: bool = true;
    /// The engine plays more than one voice per note; `false` compiles the unison loop down to one voice.
    const UNISON: bool = true;
}

/// What to do when no voice is free.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Steal {
    /// Drop the note (upstream WaveSabre).
    None,
    /// Restart the candidate with the largest age, lowest index on ties (Tunefish).
    Oldest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Poly,
    /// One (unison) voice set; new notes glide, releasing the last note glides back to the latest still held.
    MonoLegato,
}

const NOTES: usize = 128;

pub struct VoicePool<'a, V> {
    voices: &'a mut [V],
    ages: &'a mut [u32],
    mode: Mode,
    pub steal: Steal,
    /// Only voices `0..limit` are allocated (polyphony limit). Defaults to the capacity.
    pub limit: usize,
    /// Voices started per note (≥ 1).
    pub unison: u32,
    /// Detune of the outermost unison voice (`f·detune`, f from 1 down to 0).
    pub detune: f32,
    /// Unison pan spread control (upstream `VoicesPan`): 0.5 = all centred, 1 = full spread, 0 = mirrored.
    pub pan_spread: f32,
    mono_active: bool,
    log: [u8; NOTES],
    count: usize,
    held: [bool; NOTES],
}

impl<'a, V: VoiceState> VoicePool<'a, V> {
    /// `n` voices built by `make(index)` in pool memory.
    pub fn new(pool: &mut Pool<'a>, n: usize, make: impl FnMut(usize) -> V) -> Result<Self, OutOfMemory> {
        Ok(VoicePool {
            voices: pool.slice_with(n, make)?,
            ages: pool.slice(n)?,
            mode: Mode::Poly,
            steal: Steal::None,
            limit: n,
            unison: 1,
            detune: 0.0,
            pan_spread: 0.5,
            mono_active: false,
            log: [0; NOTES],
            count: 0,
            held: [false; NOTES],
        })
    }

    /// Pool bytes `new` needs for `n` voices.
    pub const fn need(n: usize) -> usize {
        Pool::need::<V>(n) + Pool::need::<u32>(n)
    }

    pub fn voices(&self) -> &[V] {
        self.voices
    }

    pub fn voices_mut(&mut self) -> &mut [V] {
        self.voices
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Changing the mode releases everything and silences all voices (upstream `SetVoiceMode`).
    pub fn set_mode<C: ?Sized>(&mut self, mode: Mode, cx: &mut C)
    where
        V: Voice<C>,
    {
        if mode != self.mode {
            self.all_notes_off(cx);
            self.voices.iter_mut().for_each(VoiceState::kill);
            self.mode = mode;
        }
    }

    /// Forgets held notes, mono state and ages without touching the voices (device reset).
    pub fn clear_state(&mut self) {
        self.mono_active = false;
        self.count = 0;
        self.held = [false; NOTES];
        self.ages.fill(0);
    }

    /// Advances voice ages by one block (only needed with [`Steal::Oldest`]).
    pub fn tick(&mut self) {
        self.ages.iter_mut().for_each(|a| *a = a.saturating_add(1));
    }

    pub fn note_on<C: ?Sized>(&mut self, note: u8, velocity: u8, cx: &mut C)
    where
        V: Voice<C>,
    {
        let note = note & 0x7f;
        if V::MONO && self.mode == Mode::MonoLegato {
            self.held[note as usize] = true;
            if self.count < NOTES {
                self.log[self.count] = note;
                self.count += 1;
            }
            if self.mono_active {
                self.voices.iter_mut().filter(|v| v.is_on()).for_each(|v| v.slide(note, cx));
                return;
            }
            self.mono_active = true;
        }
        self.start(note, velocity, cx);
    }

    fn start<C: ?Sized>(&mut self, note: u8, velocity: u8, cx: &mut C)
    where
        V: Voice<C>,
    {
        let limit = self.limit.min(self.voices.len());
        let u = if V::UNISON { self.unison.max(1) } else { 1 };
        let mut j = u;
        let mut k = 0;
        while j > 0 {
            let slot = match (k..limit).find(|&i| !self.voices[i].is_on()) {
                Some(i) => {
                    k = i + 1;
                    i
                }
                None if self.steal == Steal::Oldest && limit > 0 => {
                    k = limit;
                    (0..limit).fold(0, |best, i| if self.ages[i] > self.ages[best] { i } else { best })
                }
                None => break,
            };
            j -= 1;
            let f = j as f32 / if u > 1 { (u - 1) as f32 } else { 1.0 };
            let pan = (f - 0.5) * (self.pan_spread * 2.0 - 1.0) + 0.5;
            self.voices[slot].note_on(note, velocity, f * self.detune, pan, cx);
            self.ages[slot] = 0;
        }
    }

    pub fn note_off<C: ?Sized>(&mut self, note: u8, cx: &mut C)
    where
        V: Voice<C>,
    {
        let note = note & 0x7f;
        match self.mode {
            Mode::MonoLegato if V::MONO => {
                self.held[note as usize] = false;
                if self.count == 0 || self.log[self.count - 1] != note {
                    return;
                }
                while self.count > 0 {
                    let last = self.log[self.count - 1];
                    if self.held[last as usize] {
                        self.voices.iter_mut().filter(|v| v.is_on()).for_each(|v| v.slide(last, cx));
                        break;
                    }
                    self.count -= 1;
                }
                if self.count == 0 {
                    self.release_all(cx);
                }
            }
            _ => {
                self.voices.iter_mut().filter(|v| v.is_on() && v.note() == note).for_each(|v| v.note_off(cx));
            }
        }
    }

    /// Releases every sounding voice and forgets held notes.
    pub fn all_notes_off<C: ?Sized>(&mut self, cx: &mut C)
    where
        V: Voice<C>,
    {
        self.release_all(cx);
    }

    fn release_all<C: ?Sized>(&mut self, cx: &mut C)
    where
        V: Voice<C>,
    {
        self.voices.iter_mut().filter(|v| v.is_on()).for_each(|v| v.note_off(cx));
        self.mono_active = false;
        self.count = 0;
        self.held = [false; NOTES];
    }
}
