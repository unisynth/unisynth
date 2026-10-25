//! The one interface every instrument and effect implements, so any device can sit on any `mix` track.
//!
//! Dispatch in songs goes through an enum of enabled device kinds (no per-device vtables in the compo exe); the
//! trait keeps engines and tests on one contract. Pitch bend and mod wheel are not part of it: songs never carry
//! them (live-input only), so engines expose them as their own methods.

use crate::rng::Rng;

pub const MAIN_L: usize = 0;
pub const MAIN_R: usize = 1;
pub const SIDE_L: usize = 2;
pub const SIDE_R: usize = 3;

/// The 4 channel buffers of a track, processed in place: `ch[MAIN_L]`, `ch[MAIN_R]`, `ch[SIDE_L]`, `ch[SIDE_R]`.
/// Each slice holds ≥ `n` frames; frames past `n` are untouched.
///
/// Buffer roles (upstream WaveSabre semantics): an instrument **overwrites** main L/R with its output (whatever was
/// there, including receives into main, is replaced); an effect transforms main L/R and may read the sidechain pair
/// as a key input. No device writes the sidechain pair.
pub struct Io<'b> {
    pub ch: [&'b mut [f32]; 4],
}

/// Per-call render context. Replaces upstream globals (sample rate, tempo, random seed).
pub struct Ctx<'r> {
    /// Sample rate in Hz.
    pub rate: f64,
    /// Song tempo in BPM.
    pub tempo: i32,
    /// Song frame of the first frame of this call (absolute grid for block-based devices).
    pub frame: u64,
    /// The song's random source, drawn in track render order. Engines with their own random law keep their own state.
    pub rng: &'r mut dyn Rng,
}

pub trait Device {
    /// Number of normalized parameters. Constant for the device's lifetime.
    fn param_count(&self) -> usize;
    /// Sets parameter `index` to `value` (normally in [0, 1]; not clamped). `index ≥ param_count()` is ignored.
    fn set_param(&mut self, index: usize, value: f32);
    /// Queues a note-on `offset` frames after the first frame of the next `run` call. The velocity is exactly what
    /// the song holds (velocity 0 is still a note-on here; a format whose 0 means note-off, like `.tfm`, is converted
    /// by its decoder/converter, never by the mixer).
    fn note_on(&mut self, _note: u8, _velocity: u8, _offset: u32) {}
    /// Queues a note-off.
    fn note_off(&mut self, _note: u8, _offset: u32) {}
    /// Releases all voices and drops queued events.
    fn all_notes_off(&mut self) {}
    /// Event lookahead `L` in frames, constant for the device's lifetime. In a call covering song frames
    /// `[t, t+n)` the mixer delivers every not-yet-delivered event with frame `< t + n + L` (once each), so a device
    /// rendering internal blocks of `B` frames needs `L = B − 1`. 0 = events only for the current call.
    fn event_lookahead(&self) -> u32 {
        0
    }
    /// Returns to the freshly loaded state. Parameters and values derived from them stay; all dynamic state is
    /// cleared: voices, queued events, delay contents, phases, counters, and any random state the engine owns.
    /// The player calls it on every device, and re-seeds the song random source, before re-rendering from frame 0
    /// on seek.
    fn reset(&mut self);
    /// Renders `n` ≥ 1 frames in place (see [`Io`] for buffer roles).
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    NoteOn { note: u8, velocity: u8 },
    NoteOff { note: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    /// Frames from the first frame of the current `run` call.
    pub offset: u32,
    pub kind: EventKind,
}

/// One step of [`Events::run`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// An event fires at frame `at` of the current call, before frames `at..` are rendered.
    Event { at: usize, kind: EventKind },
    /// Render frames `start .. start + len` of the current call.
    Render { start: usize, len: usize },
}

/// Fixed slot array of pending events (upstream `SynthDevice`: 256 slots, a new event takes the first free slot,
/// a full array drops it, due events fire in slot order). Devices with an event lookahead size `N` for the number
/// of events their window can hold.
pub struct Events<const N: usize> {
    slots: [Option<Event>; N],
}

impl<const N: usize> Default for Events<N> {
    fn default() -> Self {
        Events { slots: [None; N] }
    }
}

impl<const N: usize> Events<N> {
    /// Queues `e` in the first free slot; returns false (and drops it) when all slots are taken.
    pub fn push(&mut self, e: Event) -> bool {
        match self.slots.iter_mut().find(|s| s.is_none()) {
            Some(s) => {
                *s = Some(e);
                true
            }
            None => false,
        }
    }

    pub fn clear(&mut self) {
        self.slots = [None; N];
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    /// Splits a call of `n` frames at event offsets: fires due events (slot order), then renders up to the next
    /// event or the end of the call, repeatedly. Events at offset ≥ `n` stay queued with offsets reduced by `n`.
    /// `n` is a sub-block size (≤ `u32::MAX`).
    pub fn run(&mut self, n: usize, mut step: impl FnMut(Step)) {
        debug_assert!(n <= u32::MAX as usize);
        let mut start = 0;
        while start < n {
            let mut len = n - start;
            for s in self.slots.iter_mut() {
                if let Some(e) = *s {
                    if e.offset == 0 {
                        step(Step::Event { at: start, kind: e.kind });
                        *s = None;
                    } else if (e.offset as usize) < len {
                        len = e.offset as usize;
                    }
                }
            }
            step(Step::Render { start, len });
            for e in self.slots.iter_mut().flatten() {
                e.offset -= len as u32;
            }
            start += len;
        }
    }
}
