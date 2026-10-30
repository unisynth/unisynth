//! The live host (spec synth-live-host, design D3–D5): the song player's [`Mixer`] over an editable
//! [`Sched`] source. Built from the exported blob with the player's device load order and song random source, so
//! from frame 0 without edits it is bit-identical to `Player` for any call sizes. Edits arrive as a new blob and are
//! diffed per track (D4): value changes → `set_param`, structural changes → that track's devices rebuilt, notes or
//! automation → that track's schedule swapped, routing → mixer rebuilt; a different track count reloads.
//!
//! Render calls never allocate: buffers, meters and per-track queues are sized at load; loop wraps and chase run on
//! preallocated storage.

use song::blob::BlobM;
use synth_core::device::{Ctx, Device, EventKind, Io};
use synth_core::mix::{DEFAULT_SUB, Env, Kinds, LoadError, Mixer, Receive, Source, TrackSource};
use synth_core::pool::Pool;
use unisynth::rng::Xorshift;
use unisynth::{UDevice, chunk::Chunk, params};

/// Largest frames per render call.
pub const MAX_FRAMES: usize = 4096;
/// Audition / chase queue capacity per track.
const QUEUE: usize = 128;
/// Effect tails ring this long after stop (unless stopped twice).
const TAIL_SECONDS: u64 = 2;

/// One device with the memory it borrows. Field order = drop order: the device before its arena and chunk.
struct Slot {
    dev: Option<UDevice<'static>>,
    _mem: Box<[u8]>,
    _chunk: Box<[u8]>,
}

impl Slot {
    fn empty() -> Slot {
        Slot {
            dev: None,
            _mem: Box::default(),
            _chunk: Box::default(),
        }
    }

    fn load(
        kind: u8,
        chunk: &[u8],
        env: &Env<'static>,
        rng: &mut Xorshift,
    ) -> Result<Slot, LoadError> {
        let chunk: Box<[u8]> = chunk.into();
        // SAFETY: the boxed bytes never move and live in this slot until after `dev` is dropped (field order).
        let c: &'static [u8] = unsafe { &*(&*chunk as *const [u8]) };
        let need = UDevice::need(kind, c, env)? + 64;
        let mut mem = vec![0u8; need].into_boxed_slice();
        // SAFETY: as above, for the arena the device carves its state from.
        let m: &'static mut [u8] = unsafe { &mut *(&mut *mem as *mut [u8]) };
        let mut pool = Pool::new(m);
        let dev = UDevice::load(kind, c, env, &mut pool, rng)?;
        Ok(Slot {
            dev: Some(dev),
            _mem: mem,
            _chunk: chunk,
        })
    }
}

impl Device for Slot {
    fn param_count(&self) -> usize {
        self.dev.as_ref().map_or(0, Device::param_count)
    }
    fn set_param(&mut self, i: usize, v: f32) {
        if let Some(d) = &mut self.dev {
            d.set_param(i, v)
        }
    }
    fn note_on(&mut self, n: u8, v: u8, o: u32) {
        if let Some(d) = &mut self.dev {
            d.note_on(n, v, o)
        }
    }
    fn note_off(&mut self, n: u8, o: u32) {
        if let Some(d) = &mut self.dev {
            d.note_off(n, o)
        }
    }
    fn all_notes_off(&mut self) {
        if let Some(d) = &mut self.dev {
            d.all_notes_off()
        }
    }
    fn event_lookahead(&self) -> u32 {
        self.dev.as_ref().map_or(0, Device::event_lookahead)
    }
    fn reset(&mut self) {
        if let Some(d) = &mut self.dev {
            d.reset()
        }
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        match &mut self.dev {
            Some(d) => d.run(io, n, ctx),
            None => {}
        }
    }
}

/// The player's automation walk (`synth_core::mix::player::AutoState`) over owned points; bit-identical values.
#[derive(Clone, Debug, PartialEq)]
struct Auto {
    /// Index into the track's devices (for diffs) and the slot it drives.
    in_track: usize,
    slot: usize,
    param: usize,
    /// `(absolute frame, value)` as the blob decodes them.
    pts: Vec<(i32, f32)>,
    idx: usize,
    prev: (i32, f32),
    cur: (i32, f32),
    pos: i64,
}

impl Auto {
    fn new(in_track: usize, slot: usize, param: usize, raw: &[(i64, u8)]) -> Auto {
        let mut t = 0i32;
        let mut last = 0i64;
        let pts = raw
            .iter()
            .map(|&(at, b)| {
                t = t.wrapping_add((at - last) as i32);
                last = at;
                (t, (b as f64 / 255.0) as f32)
            })
            .collect();
        let mut a = Auto {
            in_track,
            slot,
            param,
            pts,
            idx: 0,
            prev: (0, 0.0),
            cur: (0, 0.0),
            pos: 0,
        };
        a.seek(0);
        a
    }

    /// State at `frame` (same as stepping from 0 on any grid: the walk depends only on the position).
    fn seek(&mut self, frame: i64) {
        self.idx = 0;
        self.cur = self.pts.first().copied().unwrap_or((0, 0.0));
        self.prev = self.cur;
        self.pos = frame;
    }

    fn step(&mut self, n: usize) -> Option<f32> {
        let len = self.pts.len();
        if len == 0 {
            self.pos += n as i64;
            return None;
        }
        while self.idx < len && self.cur.0 as i64 <= self.pos {
            self.prev = self.cur;
            self.idx += 1;
            if let Some(&p) = self.pts.get(self.idx) {
                self.cur = p;
            }
        }
        let v = if self.idx >= len {
            self.prev.1
        } else if self.idx == 0 {
            self.cur.1
        } else {
            let span = self.cur.0.wrapping_sub(self.prev.0);
            let t = if span > 0 {
                (self.pos - self.prev.0 as i64) as f32 / span as f32
            } else {
                0.0
            };
            synth_core::math::mix(self.prev.1, self.cur.1, t)
        };
        self.pos += n as i64;
        Some(v)
    }
}

struct TrackLive {
    /// Slot indices in chain order.
    devices: Vec<usize>,
    /// The chain as exported (kind, chunk): diffed on update.
    chain: Vec<(u8, Vec<u8>)>,
    receives: Vec<Receive>,
    volume: f32,
    /// Schedule: `(absolute frame, event)`, frame order.
    events: Vec<(i64, EventKind)>,
    pos: usize,
    autos: Vec<Auto>,
    /// Schedule notes sounding (bit per pitch).
    open: u128,
    /// Audition events, delivered at the next sub-block start before anything else.
    aud: Vec<EventKind>,
    /// Chased note-ons after a seek.
    chase: Vec<EventKind>,
}

/// The editable mixer source: per-track schedules plus transport state for the next sub-block.
struct Sched {
    tracks: Vec<TrackLive>,
    playing: bool,
    /// Start frame of the sub-block being rendered.
    now: i64,
    /// Schedule events at or after this frame are held back (loop end).
    cap: i64,
}

struct TrackView<'s> {
    t: &'s mut TrackLive,
    playing: bool,
    now: i64,
    cap: i64,
}

impl Source for Sched {
    type Track<'s> = TrackView<'s>;
    fn track_count(&self) -> usize {
        self.tracks.len()
    }
    fn track(&mut self, t: usize) -> TrackView<'_> {
        TrackView {
            t: &mut self.tracks[t],
            playing: self.playing,
            now: self.now,
            cap: self.cap,
        }
    }
}

impl TrackSource for TrackView<'_> {
    fn devices(&self) -> impl Iterator<Item = usize> + '_ {
        self.t.devices.iter().copied()
    }
    fn receives(&self) -> impl Iterator<Item = Receive> + '_ {
        self.t.receives.iter().copied()
    }
    fn volume(&self) -> f32 {
        self.t.volume
    }
    fn peek(&self) -> Option<(i64, EventKind)> {
        if let Some(&e) = self.t.aud.first() {
            return Some((self.now, e));
        }
        if let Some(&e) = self.t.chase.first() {
            return Some((self.now, e));
        }
        if !self.playing {
            return None;
        }
        self.t
            .events
            .get(self.t.pos)
            .copied()
            .filter(|e| e.0 < self.cap)
    }
    fn advance(&mut self) {
        if !self.t.aud.is_empty() {
            self.t.aud.remove(0);
        } else if !self.t.chase.is_empty() {
            self.t.chase.remove(0);
        } else if let Some(&(_, e)) = self.t.events.get(self.t.pos) {
            self.t.pos += 1;
            match e {
                EventKind::NoteOn { note, .. } => self.t.open |= 1u128 << (note & 127),
                EventKind::NoteOff { note } => self.t.open &= !(1u128 << (note & 127)),
            }
        }
    }
    fn automate(&mut self, n: usize, mut set: impl FnMut(usize, usize, f32)) {
        if !self.playing {
            return;
        }
        for a in self.t.autos.iter_mut() {
            if let Some(v) = a.step(n) {
                set(a.slot, a.param, v);
            }
        }
    }
}

/// The mixer with the memory it borrows (drop order: mixer first).
struct MixerBox {
    mixer: Mixer<'static>,
    _mem: Box<[u8]>,
}

/// What an update did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edit {
    /// Parameter values set on existing devices.
    pub params: usize,
    /// Tracks whose devices were rebuilt.
    pub rebuilt: usize,
    /// Tracks whose notes or automation were swapped.
    pub schedules: usize,
    /// The mixer was rebuilt (routing changed).
    pub mixer: bool,
    /// Everything was reloaded (track count, rate or tempo changed).
    pub reload: bool,
}

pub struct Host {
    slots: Vec<Slot>,
    sched: Sched,
    mix: Option<MixerBox>,
    rate: i32,
    tempo: i32,
    bank: Option<&'static [u8]>,
    _bank: Option<Box<[u8]>>,
    /// Song random source (devices draw from it while loading and rendering), as in the player.
    rng: Xorshift,
    /// Seeds for devices rebuilt by edits (D4).
    host_rng: Xorshift,
    playing: bool,
    /// Transport position while stopped.
    stop_pos: u64,
    looping: Option<(u64, u64)>,
    /// Current sub-block: start frame, frames valid (up to the loop end), next unread.
    block: u64,
    valid: usize,
    read: usize,
    wrap: bool,
    idle: u64,
    silenced: bool,
    out: Vec<f32>,
    peaks: Vec<[f32; 2]>,
    load: f32,
    /// The track whose instrument the editor watches (`watch`), and the probe's flat output.
    watch: Option<usize>,
    probe_out: Vec<f32>,
}

impl Default for Host {
    fn default() -> Self {
        Self::new()
    }
}

/// Milliseconds from a monotonic clock (the JS host provides it on wasm32).
#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        fn host_now_ms() -> f64;
    }
    unsafe { host_now_ms() }
}

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> f64 {
    use std::sync::OnceLock;
    static T0: OnceLock<std::time::Instant> = OnceLock::new();
    T0.get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
        * 1000.0
}

impl Host {
    pub fn new() -> Host {
        Host {
            slots: vec![],
            sched: Sched {
                tracks: vec![],
                playing: false,
                now: 0,
                cap: i64::MAX,
            },
            mix: None,
            rate: 44100,
            tempo: 120,
            bank: None,
            _bank: None,
            rng: Xorshift::new(song::render::SEED),
            host_rng: Xorshift::new(0x5eed),
            playing: false,
            stop_pos: 0,
            looping: None,
            block: 0,
            valid: 0,
            read: 0,
            wrap: false,
            idle: 0,
            silenced: true,
            out: vec![0.0; MAX_FRAMES * 2],
            peaks: vec![],
            load: 0.0,
            watch: None,
            probe_out: vec![],
        }
    }

    /// GM.DLS bytes for sampler slots (takes effect at the next load or device rebuild).
    pub fn set_bank(&mut self, bank: Vec<u8>) {
        let b = bank.into_boxed_slice();
        // SAFETY: the box lives in `self._bank` until replaced; devices that borrow it are rebuilt by the next load.
        self.bank = Some(unsafe { &*(&*b as *const [u8]) });
        self._bank = Some(b);
    }

    /// The GM bank set by [`Host::set_bank`].
    pub fn bank_bytes(&self) -> Option<&[u8]> {
        self.bank
    }

    fn env(&self) -> Env<'static> {
        Env {
            rate: self.rate as f64,
            tempo: self.tempo,
            bank: self.bank,
        }
    }

    /// Loads a song blob (stopped at frame 0). Devices load in song device order from the song random source, as
    /// in the player.
    pub fn load(&mut self, blob: &[u8]) -> Result<(), String> {
        let m = BlobM::read(blob).map_err(|e| format!("not a valid song blob: {e:?}"))?;
        self.mix = None;
        self.sched.tracks.clear();
        self.slots.clear();
        self.rate = m.rate;
        self.tempo = m.tempo;
        self.rng = Xorshift::new(song::render::SEED);
        let env = self.env();
        for (k, c) in &m.devices {
            let s = Slot::load(*k, c, &env, &mut self.rng)
                .map_err(|e| format!("device kind {k}: {e:?}"))?;
            self.slots.push(s);
        }
        for t in &m.tracks {
            let mut tr = TrackLive {
                devices: t.devices.clone(),
                chain: t.devices.iter().map(|&d| m.devices[d].clone()).collect(),
                receives: receives(t),
                volume: t.volume,
                events: m.lanes[t.lane].clone(),
                pos: 0,
                autos: vec![],
                open: 0,
                aud: Vec::with_capacity(QUEUE),
                chase: Vec::with_capacity(QUEUE),
            };
            tr.autos = t
                .autos
                .iter()
                .map(|a| Auto::new(a.device, t.devices[a.device], a.param, &a.points))
                .collect();
            self.sched.tracks.push(tr);
        }
        self.peaks = vec![[0.0; 2]; self.sched.tracks.len()];
        self.build_mixer()?;
        self.playing = false;
        self.stop_pos = 0;
        self.seek_to(0);
        self.apply_watch();
        Ok(())
    }

    fn build_mixer(&mut self) -> Result<(), String> {
        self.mix = None;
        Mixer::check(&mut self.sched, self.slots.len()).map_err(|e| format!("routing: {e:?}"))?;
        let mut mem =
            vec![0u8; Mixer::need(self.sched.tracks.len(), DEFAULT_SUB) + 64].into_boxed_slice();
        // SAFETY: the arena never moves and is dropped after the mixer (MixerBox field order).
        let m: &'static mut [u8] = unsafe { &mut *(&mut *mem as *mut [u8]) };
        let mut pool = Pool::new(m);
        let mut mixer = Mixer::new(&mut self.sched, &mut pool, DEFAULT_SUB)
            .map_err(|e| format!("routing: {e:?}"))?;
        mixer.update_look(&self.slots, &mut self.sched);
        self.mix = Some(MixerBox { mixer, _mem: mem });
        Ok(())
    }

    /// Applies a new song blob between render calls (D4) and reports what changed.
    pub fn update(&mut self, blob: &[u8]) -> Result<Edit, String> {
        let m = BlobM::read(blob).map_err(|e| format!("not a valid song blob: {e:?}"))?;
        if m.tracks.len() != self.sched.tracks.len()
            || m.rate != self.rate
            || m.tempo != self.tempo
            || self.mix.is_none()
        {
            let (playing, pos, lp) = (self.playing, self.position(), self.looping);
            self.load(blob)?;
            self.looping = lp;
            if playing {
                self.play(pos);
            } else {
                self.stop_pos = pos;
            }
            return Ok(Edit {
                reload: true,
                ..Edit::default()
            });
        }
        let mut e = Edit::default();
        let next = self.mix.as_ref().map_or(0, |m| m.mixer.frame()) as i64;
        let mut look = false;
        for (ti, t) in m.tracks.iter().enumerate() {
            let chain: Vec<(u8, Vec<u8>)> =
                t.devices.iter().map(|&d| m.devices[d].clone()).collect();
            let old = &self.sched.tracks[ti].chain;
            let mut sets = vec![];
            let rebuild = chain.len() != old.len()
                || chain
                    .iter()
                    .zip(old)
                    .enumerate()
                    .any(|(j, (a, b))| match diff(a, b) {
                        Diff::Same => false,
                        Diff::Params(p) => {
                            sets.extend(p.into_iter().map(|(i, v)| (j, i, v)));
                            false
                        }
                        Diff::Rebuild => true,
                    });
            if rebuild {
                self.rebuild_chain(ti, &chain)?;
                e.rebuilt += 1;
                look = true;
            } else {
                for &(j, i, v) in &sets {
                    let s = self.sched.tracks[ti].devices[j];
                    self.slots[s].set_param(i, v);
                }
                e.params += sets.len();
                self.sched.tracks[ti].chain = chain;
            }
            let tr = &mut self.sched.tracks[ti];
            let events = &m.lanes[t.lane];
            let autos: Vec<Auto> = t
                .autos
                .iter()
                .map(|a| Auto::new(a.device, tr.devices[a.device], a.param, &a.points))
                .collect();
            let autos_changed = autos.len() != tr.autos.len()
                || autos
                    .iter()
                    .zip(&tr.autos)
                    .any(|(a, b)| (a.in_track, a.param, &a.pts) != (b.in_track, b.param, &b.pts))
                || autos.iter().zip(&tr.autos).any(|(a, b)| a.slot != b.slot);
            if *events != tr.events || autos_changed {
                if *events != tr.events {
                    for n in 0..128u8 {
                        if tr.open & (1u128 << n) != 0 {
                            for &s in &tr.devices {
                                self.slots[s].note_off(n, 0);
                            }
                        }
                    }
                    tr.open = 0;
                    tr.events = events.clone();
                    // the mixer already delivered events before next + lookahead for the current sub-block
                    let look = tr
                        .devices
                        .iter()
                        .map(|&s| self.slots[s].event_lookahead())
                        .max()
                        .unwrap_or(0) as i64;
                    tr.pos = tr.events.partition_point(|x| x.0 < next + look);
                }
                if autos_changed {
                    tr.autos = autos;
                    for a in tr.autos.iter_mut() {
                        a.seek(next);
                    }
                }
                e.schedules += 1;
            }
            tr.volume = t.volume;
            let rc = receives(t);
            if rc != tr.receives {
                tr.receives = rc;
                e.mixer = true;
            }
        }
        if e.mixer {
            self.build_mixer()?;
            if let Some(mb) = &mut self.mix {
                mb.mixer.reset(next as u64);
            }
            // the rest of the current sub-block lived in the old mixer: continue at its end
            self.block = next as u64;
            self.read = 0;
            self.valid = 0;
            self.wrap = false;
        } else if look {
            if let Some(mb) = &mut self.mix {
                mb.mixer.update_look(&self.slots, &mut self.sched);
            }
        }
        self.apply_watch();
        Ok(e)
    }

    /// New devices for track `ti` (fresh slots seeded from the host random source; old slots freed unless another
    /// track uses them).
    fn rebuild_chain(&mut self, ti: usize, chain: &[(u8, Vec<u8>)]) -> Result<(), String> {
        let env = self.env();
        let mut fresh = vec![];
        for (k, c) in chain {
            fresh.push(
                Slot::load(*k, c, &env, &mut self.host_rng)
                    .map_err(|e| format!("device kind {k}: {e:?}"))?,
            );
        }
        let old = std::mem::take(&mut self.sched.tracks[ti].devices);
        for s in old {
            if !self.sched.tracks.iter().any(|t| t.devices.contains(&s)) {
                self.slots[s] = Slot::empty();
            }
        }
        let mut ids: Vec<usize> = vec![];
        for f in fresh {
            let free = (0..self.slots.len()).find(|&i| {
                self.slots[i].dev.is_none()
                    && !ids.contains(&i)
                    && !self.sched.tracks.iter().any(|t| t.devices.contains(&i))
            });
            match free {
                Some(i) => {
                    self.slots[i] = f;
                    ids.push(i);
                }
                None => {
                    self.slots.push(f);
                    ids.push(self.slots.len() - 1);
                }
            }
        }
        let tr = &mut self.sched.tracks[ti];
        tr.devices = ids;
        tr.chain = chain.to_vec();
        tr.open = 0;
        for a in tr.autos.iter_mut() {
            a.slot = tr.devices.get(a.in_track).copied().unwrap_or(0);
        }
        tr.autos.retain(|a| a.in_track < chain.len());
        Ok(())
    }

    /// Starts playback at `frame` (devices reset, held notes chased; no pre-roll).
    pub fn play(&mut self, frame: u64) {
        self.playing = true;
        self.seek_to(frame);
    }

    /// First stop: notes off, effect tails ring (up to 2 s). Stop while stopped: silence now.
    pub fn stop(&mut self) {
        if self.playing {
            self.stop_pos = self.position();
            self.playing = false;
            self.slots.iter_mut().for_each(Device::all_notes_off);
            for t in self.sched.tracks.iter_mut() {
                t.open = 0;
                t.chase.clear();
            }
            self.idle = 0;
        } else {
            self.silence();
        }
    }

    fn silence(&mut self) {
        self.slots.iter_mut().for_each(Device::reset);
        if let Some(mb) = &mut self.mix {
            let f = mb.mixer.frame();
            mb.mixer.reset(f);
        }
        self.read = self.valid;
        self.silenced = true;
    }

    /// Moves the playhead (while playing: devices reset, held notes chased at the target).
    pub fn seek(&mut self, frame: u64) {
        if self.playing {
            self.seek_to(frame);
        } else {
            self.stop_pos = frame;
        }
    }

    /// Loop range `[start, end)` in frames (`None` = off). The jump happens at the end sample.
    pub fn set_loop(&mut self, range: Option<(u64, u64)>) {
        self.looping = range.filter(|r| r.0 < r.1);
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    /// Frame of the next sample to be rendered.
    pub fn position(&self) -> u64 {
        if !self.playing {
            return self.stop_pos;
        }
        if self.wrap && self.read >= self.valid {
            return self.looping.map_or(self.block + self.read as u64, |l| l.0);
        }
        self.block + self.read as u64
    }

    /// Seek with chase; no allocation (runs inside render at a loop wrap).
    fn seek_to(&mut self, frame: u64) {
        self.slots.iter_mut().for_each(Device::reset);
        if let Some(mb) = &mut self.mix {
            mb.mixer.reset(frame);
        }
        self.block = frame;
        self.read = 0;
        self.valid = 0;
        self.wrap = false;
        self.silenced = false;
        let f = frame as i64;
        for t in self.sched.tracks.iter_mut() {
            t.pos = t.events.partition_point(|e| e.0 < f);
            t.chase.clear();
            t.open = 0;
            if self.playing {
                let mut held: [u8; 128] = [0; 128];
                for &(_, e) in &t.events[..t.pos] {
                    match e {
                        EventKind::NoteOn { note, velocity } => {
                            held[note as usize & 127] = velocity.max(1)
                        }
                        EventKind::NoteOff { note } => held[note as usize & 127] = 0,
                    }
                }
                for (n, &v) in held.iter().enumerate() {
                    if v > 0 && t.chase.len() < t.chase.capacity() {
                        t.chase.push(EventKind::NoteOn {
                            note: n as u8,
                            velocity: v,
                        });
                        t.open |= 1u128 << n;
                    }
                }
            }
            for a in t.autos.iter_mut() {
                a.seek(f);
            }
        }
    }

    /// Audition note on track `track` (outside the schedule; sounds at the next render call, playing or stopped).
    pub fn note_on(&mut self, track: usize, note: u8, velocity: u8) {
        if let Some(t) = self.sched.tracks.get_mut(track) {
            if note < 128 && t.aud.len() < t.aud.capacity() {
                t.aud.push(EventKind::NoteOn {
                    note,
                    velocity: velocity.clamp(1, 127),
                });
                self.idle = 0;
                self.silenced = false;
            }
        }
    }

    pub fn note_off(&mut self, track: usize, note: u8) {
        if let Some(t) = self.sched.tracks.get_mut(track) {
            if note < 128 && t.aud.len() < t.aud.capacity() {
                t.aud.push(EventKind::NoteOff { note });
                self.idle = 0;
            }
        }
    }

    /// Sets parameter `param` of device `device` (index in the track's chain) on `track` from the next sub-block.
    pub fn set_param(&mut self, track: usize, device: usize, param: usize, value: f32) {
        if let Some(&s) = self
            .sched
            .tracks
            .get(track)
            .and_then(|t| t.devices.get(device))
        {
            if value.is_finite() {
                self.slots[s].set_param(param, value);
            }
        }
    }

    /// Per-track peak levels `[L, R]` of the last render call (master last).
    pub fn peaks(&self) -> &[[f32; 2]] {
        &self.peaks
    }

    /// Sounding voices per device slot (instruments; 0 for effects). Diagnostics, allocates: not for the render path.
    pub fn voice_counts(&self) -> Vec<usize> {
        self.slots
            .iter()
            .map(|s| match &s.dev {
                Some(UDevice::Instrument(i)) => i
                    .voices()
                    .iter()
                    .filter(|v| synth_core::voice::VoiceState::is_on(*v))
                    .count(),
                _ => 0,
            })
            .collect()
    }

    /// Watches `track`'s instrument (its first kind-64 device): its filter taps run and `probe` reports its voices.
    /// `None` stops: no instrument collects taps. Survives edits and reloads (by track index).
    pub fn watch(&mut self, track: Option<usize>) {
        self.watch = track;
        self.apply_watch();
    }

    /// The device slot of the watched track's instrument.
    fn watched_slot(&self) -> Option<usize> {
        let t = self.sched.tracks.get(self.watch?)?;
        t.devices.iter().copied().find(|&s| matches!(self.slots[s].dev, Some(UDevice::Instrument(_))))
    }

    fn apply_watch(&mut self) {
        let w = self.watched_slot();
        for (i, s) in self.slots.iter_mut().enumerate() {
            if let Some(UDevice::Instrument(ins)) = &mut s.dev {
                ins.set_taps(Some(i) == w);
            }
        }
    }

    /// The watched instrument's probe, flat (`PROBE_*` layout in lib.rs); empty when nothing is watched. Allocates:
    /// called about every 30 ms between render calls, never inside one.
    pub fn probe(&mut self) -> &[f32] {
        self.probe_out.clear();
        let Some(s) = self.watched_slot() else { return &self.probe_out };
        let Some(UDevice::Instrument(ins)) = &self.slots[s].dev else { return &self.probe_out };
        let p = ins.probe();
        let o = &mut self.probe_out;
        o.extend([1.0, p.voices.len() as f32, p.taps.len() as f32, unisynth::probe::TAP_LEN as f32]);
        let opt = |x: Option<f32>| x.unwrap_or(-1.0);
        for v in &p.voices {
            o.extend([v.id as f32, v.note as f32, v.amp, v.released as u8 as f32, v.age_s]);
            for (lvl, st) in v.env {
                o.extend([lvl, st as u8 as f32]);
            }
            o.extend(v.lfo);
            o.extend(v.smp.map(opt));
            o.extend(v.cutoff.map(|c| opt(c.map(unisynth::probe::cutoff_hz))));
        }
        for t in &p.taps {
            match t {
                Some((i, out)) => {
                    o.push(1.0);
                    o.extend(i);
                    o.extend(out);
                }
                None => o.push(0.0),
            }
        }
        &self.probe_out
    }

    /// The buffer `probe` filled last.
    pub fn probe_ptr(&self) -> *const f32 {
        self.probe_out.as_ptr()
    }

    /// Render time of the last call as a fraction of its real-time duration.
    pub fn load_ratio(&self) -> f32 {
        self.load
    }

    /// Renders `frames` (≤ [`MAX_FRAMES`]) interleaved stereo frames into a buffer the host owns.
    pub fn render(&mut self, frames: usize) -> &[f32] {
        let n = frames.min(MAX_FRAMES);
        let t0 = now_ms();
        self.peaks.iter_mut().for_each(|p| *p = [0.0; 2]);
        if self.mix.is_none() || (self.silenced && !self.playing) {
            self.load = 0.0;
            self.out[..2 * n].fill(0.0);
            return &self.out[..2 * n];
        }
        let _g = synth_core::denormal::Guard::new();
        if self.read < self.valid {
            self.accum_peaks(self.read);
        }
        for i in 0..n {
            if self.read >= self.valid {
                if self.wrap {
                    let start = self.looping.map_or(self.block + self.valid as u64, |l| l.0);
                    self.seek_to(start);
                }
                self.render_block();
            }
            let (l, r) = self
                .mix
                .as_ref()
                .map_or((0.0, 0.0), |m| m.mixer.master_frame(self.read));
            self.out[2 * i] = l;
            self.out[2 * i + 1] = r;
            self.read += 1;
        }
        if !self.playing {
            self.idle += n as u64;
            if self.idle > TAIL_SECONDS * self.rate as u64 {
                self.silence();
            }
        }
        let ms = now_ms() - t0;
        self.load = (ms / 1000.0 / (n.max(1) as f64 / self.rate as f64)) as f32;
        &self.out[..2 * n]
    }

    fn render_block(&mut self) {
        let Some(mb) = &mut self.mix else { return };
        let sub = mb.mixer.sub();
        let t0 = mb.mixer.frame();
        self.block = t0;
        self.valid = sub;
        self.wrap = false;
        self.sched.cap = i64::MAX;
        if self.playing {
            if let Some((_, end)) = self.looping {
                if t0 < end && t0 + sub as u64 >= end {
                    self.valid = (end - t0) as usize;
                    self.wrap = true;
                    self.sched.cap = end as i64;
                }
            }
        }
        self.sched.playing = self.playing;
        self.sched.now = t0 as i64;
        mb.mixer.render_sub(
            &mut self.slots,
            &mut self.sched,
            self.rate as f64,
            self.tempo,
            &mut self.rng,
        );
        self.read = 0;
        self.accum_peaks(0);
    }

    /// Adds frames `from..valid` of the current sub-block to the peaks (meters: the blocks a call touches).
    fn accum_peaks(&mut self, from: usize) {
        let Some(mb) = &self.mix else { return };
        for (t, p) in self.peaks.iter_mut().enumerate() {
            for c in 0..2 {
                let m = mb.mixer.channel(t, c)[from..self.valid]
                    .iter()
                    .fold(0f32, |m, x| m.max(x.abs()));
                p[c] = p[c].max(m);
            }
        }
    }
}

fn receives(t: &song::blob::TrackM) -> Vec<Receive> {
    t.receives
        .iter()
        .map(|&(track, channel, volume)| Receive {
            track,
            channel,
            volume,
        })
        .collect()
}

enum Diff {
    Same,
    /// Parameter index → new value (non-structural changes only).
    Params(Vec<(usize, f32)>),
    Rebuild,
}

/// How device chunk `a` (new) differs from `b` (current).
fn diff(a: &(u8, Vec<u8>), b: &(u8, Vec<u8>)) -> Diff {
    if a == b {
        return Diff::Same;
    }
    if a.0 != b.0 {
        return Diff::Rebuild;
    }
    if a.0 == unisynth::kind::INSTRUMENT {
        let (Ok(x), Ok(y)) = (Chunk::parse(&a.1), Chunk::parse(&b.1)) else {
            return Diff::Rebuild;
        };
        if x.sample_count() != y.sample_count()
            || (0..x.sample_count()).any(|i| x.sample_bytes(i) != y.sample_bytes(i))
        {
            return Diff::Rebuild;
        }
        let (vx, vy) = (x.values(), y.values());
        let mut out = vec![];
        for i in 0..params::COUNT {
            if vx[i].to_bits() != vy[i].to_bits() {
                if params::is_structural(i) {
                    return Diff::Rebuild;
                }
                out.push((i, vx[i]));
            }
        }
        return Diff::Params(out);
    }
    // effect: [mode, params…] + trailer
    if a.1.len() != b.1.len() || a.1.len() < 8 || a.1[..4] != b.1[..4] {
        return Diff::Rebuild;
    }
    let n = (a.1.len() - 4) / 4;
    let f = |c: &[u8], i: usize| {
        f32::from_le_bytes([c[4 * i], c[4 * i + 1], c[4 * i + 2], c[4 * i + 3]])
    };
    Diff::Params(
        (1..n)
            .filter(|&i| f(&a.1, i).to_bits() != f(&b.1, i).to_bits())
            .map(|i| (i, f(&a.1, i)))
            .collect(),
    )
}
