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

/// Extra output pairs a device may write (V2's reverb, delay, aux A and aux B sends) and extra input pairs it may
/// read (aux A, aux B), feature `pairs` (change add-v2-import, design D6). The mixer clears a track's extra pairs every
/// sub-block; the track's devices share them (a later device's write replaces an earlier one's); a pair nobody writes
/// reads as silence.
pub const OUT_PAIRS: usize = 4;
pub const IN_PAIRS: usize = 2;
/// Buffers of the extra pairs of a track: outputs `2·k`, `2·k + 1` for pair `k + 1`, inputs after them.
pub const EXT_CH: usize = 2 * (OUT_PAIRS + IN_PAIRS);

/// The receive channel field (design D6): byte 0 is the receiving pair (0 main, 2 sidechain, 4 input pair 1, 6 input
/// pair 2), byte 1 the sending pair (0 the track's main output, 1–4 an extra output pair). Old songs hold 0 or 2.
pub mod recv {
    /// Receiving buffer index: 0/2 into `ch`, 4/6 into the extra input pairs.
    pub const fn to(channel: usize) -> usize {
        channel & 0xff
    }
    /// Sending pair: 0 main, 1–4 an extra output pair.
    pub const fn from(channel: usize) -> usize {
        (channel >> 8) & 0xff
    }
    /// A valid channel field.
    pub const fn valid(channel: usize, pairs: bool) -> bool {
        let to = to(channel);
        let from = from(channel);
        if channel >> 16 != 0 {
            return false;
        }
        if !pairs {
            return channel == 0 || channel == 2;
        }
        (to == 0 || to == 2 || to == 4 || to == 6) && from <= super::OUT_PAIRS
    }
    pub const fn encode(from: usize, to: usize) -> usize {
        from << 8 | to
    }
}

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

/// How a lane's value is interpolated between two automation points, **per device** (the device a lane targets
/// decides, not the registry or the song, so devices of different engines coexist in one song, `Either`). The result
/// is not the same float: the forms differ in the last bit for a few percent of inputs, which a device that hashes the
/// bits of a parameter (Glorp ID 21 seeds on P7) turns into other sound.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LaneLerp {
    /// `a·(1−t) + b·t` ([`crate::math::mix`]): upstream WaveSabre's `Mix`, the default of every engine.
    #[default]
    Mix,
    /// `a + (b−a)·t` ([`crate::math::lerp`]): the original Glorp engine.
    Delta,
}

pub trait Device {
    /// Runs with the track's extra pairs (feature `pairs`): `ext[2k..2k+2]` is output pair `k + 1` (cleared by the
    /// mixer before the track's devices run), `ext[8..12]` input pairs 1 and 2 (filled by receives). The default
    /// ignores them.
    #[inline]
    fn run_ext(&mut self, io: &mut Io, _ext: &mut [&mut [f32]], n: usize, ctx: &mut Ctx) {
        self.run(io, n, ctx);
    }
    /// Number of normalized parameters. Constant for the device's lifetime.
    /// Whether this build plays automation lanes: a constant `Err(module id)` (a build for songs without lanes) lets the
    /// compiler drop the mixer's automation walk, and the player refuses a song with lanes with `UnsupportedModule(id)`.
    /// A method, not a const, so `Device` stays dyn-compatible.
    #[inline]
    fn automation(&self) -> Result<(), u8> {
        Ok(())
    }
    /// Interpolation of the lanes that target this device. `Player::new` reads it **once**, from the loaded device, so it
    /// must not change for the device's lifetime; a wrapper that forwards `Device` (like `Either`) must forward it, or
    /// its devices are silently played with the default. A custom `Source` that evaluates lanes itself must honour it
    /// too. Default: [`LaneLerp::Mix`] (every existing engine).
    #[inline]
    fn lane_lerp(&self) -> LaneLerp {
        LaneLerp::Mix
    }
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
    /// Frames of silent input after which this device's output stays silent (below the mixer's quiet level, −120 dBFS)
    /// for as long as its input does, in its current state: its longest memory (a delay line's length), 0 for a device
    /// without memory, and for an instrument 0 only while no voice sounds and no event is queued. `None` (the default)
    /// = unknown: the mixer never puts the device's track to sleep. Used to skip quiet tracks (see
    /// [`crate::mix::Mixer::set_skip_quiet`]); a skipped device's state is frozen, so a device whose output depends on
    /// time alone (a free-running LFO heard through the input) must answer `None`.
    fn quiet_after(&self) -> Option<u32> {
        None
    }
    /// Whether the mixer may skip quiet tracks of this engine at all: a constant `false` (the default) compiles the
    /// skip path out (no [`Device::quiet_after`] calls, no level checks), like [`Device::automation`]. A method, not a
    /// const, so `Device` stays dyn-compatible. The mixer asks the first device of the song.
    fn quiet_skip(&self) -> bool {
        false
    }
    /// Called once when the mixer runs the device's track again after skipping it for `skipped` frames, **before**
    /// that sub-block's automation: the parameters now hold what the skipped sub-blocks set. A device that smooths
    /// parameter changes across calls (a gain ramp from the last applied value) settles that state here, and one that
    /// counts time (a sweep since its restart) advances it by `skipped`, so it resumes as if it had run. Default:
    /// nothing.
    fn wake(&mut self, _skipped: u32) {}
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

    /// Whether a note-on is queued for the first `n` frames.
    pub fn has_note_on(&self, n: usize) -> bool {
        self.slots.iter().flatten().any(|e| (e.offset as usize) < n && matches!(e.kind, EventKind::NoteOn { .. }))
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
