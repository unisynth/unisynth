//! The shared 16-voice scheduler of the Glorp instruments (poly mode), independent of any voice DSP.
//!
//! Behavioural summary of the original's shared `SynthDevice` (research/glorp-devices.md §Shared synth scheduler):
//!
//! * a 256-slot queue `{kind, delay, note}`; `note_on` / `note_off` take the **first free slot** (none free: the original
//!   drops the event silently, here [`Error::QueueFull`]);
//! * `process(n)` repeats until `n` samples are done: scan the slots **in slot order**; an event with `delay > 0` only
//!   shortens the current sub-block to its delay; an event with `delay == 0` fires now and frees its slot: a note-on
//!   starts the **lowest-index inactive voice** (no stealing, no steal-oldest: with all 16 busy the note is lost), a
//!   note-off releases **every** active voice playing that note; then every active voice renders the sub-block (index
//!   order), every remaining delay shrinks by the sub-block length;
//! * a voice stays "active" until its own render clears it, and it still renders the rest of the sub-block it freed
//!   itself in; a voice is asked to render only while active at the start of the sub-block;
//! * the output buffers are zeroed first, voices accumulate into them.
//!
//! Same-offset events fire in slot order, which is call order unless a slot was freed in between (a new event takes the
//! lowest free slot, so it can overtake an older event with the same remaining delay).
//!
//! Mono mode (voice-mode parameter = 1, only ID 14 exposes it; every other instrument stays poly, `Synth::new` starts
//! poly): see [`Synth::set_mono`]. In mono mode a note-on pushes the note on a stack and marks it held; it starts a voice
//! only when nothing was sounding, later notes only retarget a sounding voice (the original's per-voice slide, not
//! audible for ID 14 and so not modelled); a note-off pops the stack when it releases the top (skipping notes no longer
//! held) and releases every voice once the stack is empty. Quirks kept on purpose: the stack has 128 cells and the
//! 129th cell *is* the depth counter (the 129th push overwrites it), an empty-stack note-off releases every voice when
//! its note equals the `playing` byte as a number (note 0 while idle), and a voice that ended by itself is not replaced
//! while any key is still held.
//!
//! Not modelled: voices per note other than 1 (unison, never set by any ID 14 or other instrument chunk) and the
//! per-voice slide. Notes are 0..=127 (the original's mono tables are 128 wide). Instruments that cannot represent mono
//! mode (every id but 14) MUST have it rejected by their adapter; blocks are limited to `i32::MAX` samples (asserted).

/// Voices of the shared synth.
pub const VOICES: usize = 16;
/// Event queue slots of the shared synth.
pub const QUEUE: usize = 256;
/// Highest accepted note.
pub const MAX_NOTE: u8 = 127;

/// Why an event is refused (nothing was queued).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Note above [`MAX_NOTE`].
    Note,
    /// Offset above `i32::MAX`.
    Offset,
    /// All [`QUEUE`] slots are taken (the original drops the event silently).
    QueueFull,
}

/// A voice of an instrument; `C` is whatever read-only state the instrument shares between its voices.
pub trait Voice<C> {
    /// Holds a slot (set by `start`, cleared by the voice itself in `render`).
    fn active(&self) -> bool;
    /// The note it was started with.
    fn note(&self) -> u8;
    /// Fresh note-on (also resets all voice state).
    fn start(&mut self, note: u8, ctx: &C);
    /// Note-on with the event's velocity. Voices that ignore velocity (every instrument but ID 20) keep this default.
    fn start_vel(&mut self, note: u8, _velocity: u8, ctx: &C) {
        self.start(note, ctx);
    }
    /// Note-off for the voice's note.
    fn release(&mut self);
    /// Adds `l.len()` samples into `l` / `r` (equal lengths).
    fn render(&mut self, ctx: &C, l: &mut [f32], r: &mut [f32]);
}

#[derive(Clone, Copy, Debug)]
struct Event {
    /// 0 empty, 1 note-on, 2 note-off.
    kind: u8,
    delay: i32,
    note: u8,
    vel: u8,
}

/// Mono-mode state: the held flags, the 128-cell note stack whose 129th cell is the depth counter, and the `playing` byte.
#[derive(Clone, Debug)]
struct Mono {
    on: bool,
    playing: bool,
    held: [bool; MAX_NOTE as usize + 1],
    /// Cells `0..128` hold the stack (a note pushed at depth `d` goes to cell `d`), cell 128 is the depth.
    cells: [i32; MONO_CELLS],
}

const MONO_CELLS: usize = MAX_NOTE as usize + 2;
const DEPTH: usize = MONO_CELLS - 1;

impl Mono {
    const IDLE: Mono = Mono { on: false, playing: false, held: [false; MAX_NOTE as usize + 1], cells: [0; MONO_CELLS] };

    fn clear(&mut self) {
        *self = Mono { on: self.on, ..Mono::IDLE };
    }
}

/// Scheduler + the 16 voices.
#[derive(Clone, Debug)]
pub struct Synth<V> {
    voices: [V; VOICES],
    queue: [Event; QUEUE],
    mono: Mono,
}

impl<V> Synth<V> {
    pub fn new(voices: [V; VOICES]) -> Self {
        Synth { voices, queue: [Event { kind: 0, delay: 0, note: 0, vel: 0 }; QUEUE], mono: Mono::IDLE }
    }

    fn enqueue(&mut self, kind: u8, note: u8, vel: u8, offset: u32) -> Result<(), Error> {
        if note > MAX_NOTE {
            return Err(Error::Note);
        }
        if offset > i32::MAX as u32 {
            return Err(Error::Offset);
        }
        let slot = self.queue.iter_mut().find(|e| e.kind == 0).ok_or(Error::QueueFull)?;
        *slot = Event { kind, delay: offset as i32, note, vel };
        Ok(())
    }

    /// Queues a note-on `offset` samples after the start of the next `process` call.
    pub fn note_on(&mut self, note: u8, offset: u32) -> Result<(), Error> {
        self.enqueue(1, note, 0, offset)
    }

    /// Queues a note-on carrying a velocity for the voices that read it ([`Voice::start_vel`]).
    pub fn note_on_vel(&mut self, note: u8, velocity: u8, offset: u32) -> Result<(), Error> {
        self.enqueue(1, note, velocity, offset)
    }

    /// Queues a note-off.
    pub fn note_off(&mut self, note: u8, offset: u32) -> Result<(), Error> {
        self.enqueue(2, note, 0, offset)
    }

    /// Whether the scheduler is in the original's mono (voice mode 1) note-stack mode.
    pub fn is_mono(&self) -> bool {
        self.mono.on
    }

    /// Depth cell of the mono note stack (introspection).
    pub fn mono_depth(&self) -> usize {
        self.mono.cells[DEPTH] as usize
    }

    /// Switches the voice mode. A change (the original compares the new mode with the stored one) empties the event
    /// queue and the note stack and `stop`s every voice at once (the notes are cut, not released); the same mode again
    /// does nothing.
    pub fn set_mono<F: FnMut(&mut V)>(&mut self, mono: bool, mut stop: F) {
        if self.mono.on == mono {
            return;
        }
        self.mono = Mono { on: mono, ..Mono::IDLE };
        for e in self.queue.iter_mut() {
            e.kind = 0;
        }
        for v in self.voices.iter_mut() {
            stop(v);
        }
    }

    /// Direct access for introspection.
    pub fn voices(&self) -> &[V; VOICES] {
        &self.voices
    }

    /// Zeroes `l` / `r`, fires due events and renders. Panics when the lengths differ.
    pub fn process<C>(&mut self, ctx: &C, l: &mut [f32], r: &mut [f32])
    where
        V: Voice<C>,
    {
        assert_eq!(l.len(), r.len(), "channel length mismatch");
        // delays are i32 (like the original's) and shrink by the sub-block length: a longer block would wrap them
        assert!(l.len() <= i32::MAX as usize, "block longer than i32::MAX samples");
        l.fill(0.0);
        r.fill(0.0);
        let total = l.len();
        let mut pos = 0;
        while pos < total {
            let mut cur = total - pos;
            for s in 0..QUEUE {
                let e = self.queue[s];
                if e.kind == 0 {
                    continue;
                }
                if e.delay != 0 {
                    cur = cur.min(e.delay as usize);
                    continue;
                }
                match (e.kind, self.mono.on) {
                    (1, false) => self.start_voice(e.note, e.vel, ctx),
                    (1, true) => self.mono_on(e.note, e.vel, ctx),
                    (_, false) => {
                        for v in self.voices.iter_mut().filter(|v| v.active() && v.note() == e.note) {
                            v.release();
                        }
                    }
                    (_, true) => self.mono_off::<C>(e.note),
                }
                self.queue[s].kind = 0;
            }
            for v in self.voices.iter_mut().filter(|v| v.active()) {
                v.render(ctx, &mut l[pos..pos + cur], &mut r[pos..pos + cur]);
            }
            for e in self.queue.iter_mut().filter(|e| e.kind != 0) {
                e.delay -= cur as i32;
            }
            pos += cur;
        }
    }

    fn start_voice<C>(&mut self, note: u8, vel: u8, ctx: &C)
    where
        V: Voice<C>,
    {
        if let Some(v) = self.voices.iter_mut().find(|v| !v.active()) {
            v.start_vel(note, vel, ctx);
        }
    }

    /// Mono note-on: held flag, push; start a voice only when nothing was sounding.
    fn mono_on<C>(&mut self, note: u8, vel: u8, ctx: &C)
    where
        V: Voice<C>,
    {
        let m = &mut self.mono;
        m.held[note as usize] = true;
        let d = m.cells[DEPTH] as usize;
        m.cells[d] = note as i32;
        m.cells[DEPTH] += 1;
        if !m.playing {
            m.playing = true;
            self.start_voice(note, vel, ctx);
        }
    }

    /// Mono note-off: only the stack top pops (past notes no longer held); an empty stack releases every voice.
    fn mono_off<C>(&mut self, note: u8)
    where
        V: Voice<C>,
    {
        let m = &mut self.mono;
        m.held[note as usize] = false;
        let depth = m.cells[DEPTH];
        // the cell the original compares with: the stack top, or (empty stack) the `playing` byte read as a number
        let top = if depth > 0 { m.cells[depth as usize - 1] } else { m.playing as i32 };
        if note as i32 != top {
            return;
        }
        let mut d = depth;
        while d > 0 {
            let t = m.cells[d as usize - 1];
            if (0..=MAX_NOTE as i32).contains(&t) && m.held[t as usize] {
                m.cells[DEPTH] = d;
                return;
            }
            d -= 1;
            m.cells[DEPTH] = d;
        }
        m.clear();
        for v in self.voices.iter_mut().filter(|v| v.active()) {
            v.release();
        }
    }

    pub fn active_voices(&self) -> usize
    where
        V: ActiveNote,
    {
        self.voices.iter().filter(|v| v.is_active()).count()
    }

    pub fn voice_notes(&self) -> [Option<u8>; VOICES]
    where
        V: ActiveNote,
    {
        core::array::from_fn(|i| self.voices[i].is_active().then(|| self.voices[i].held_note()))
    }
}

/// Context-free view of a voice for introspection (`Voice<C>` needs a context type to be named).
pub trait ActiveNote {
    fn is_active(&self) -> bool;
    fn held_note(&self) -> u8;
}
