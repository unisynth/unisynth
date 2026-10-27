//! Song player: single-threaded, deterministic port of upstream WaveSabre `SongRenderer` / `Track::Run`.
//!
//! A [`Mixer`] over a [`BlobSource`]: per sub-block of `sub` frames (default 100 = upstream players' step), each
//! track in dependency order: deliver lane events (up to the track's event lookahead early) to every device on the
//! track, apply automations, clear its 4 buffers, add receives, run its devices in chain order, scale by the track
//! volume. The last track is the master. Caller buffers of any size are served from the master's sub-block.

use super::mixer::{Mixer, Source, TrackSource};
use super::registry::{devices_need, load_devices, Env, Kinds};
use super::song::{LaneEvents, Points, Receive, Song, Track};
use super::LoadError;
use crate::device::{Device, EventKind};
use crate::pool::Pool;
use crate::rng::Rng;

/// Upstream players' render step in frames.
pub const DEFAULT_SUB: usize = 100;

struct AutoState<'a> {
    /// Song-level device index.
    device: usize,
    param: usize,
    points: Points<'a>,
    len: usize,
    iter: Points<'a>,
    idx: usize,
    prev: (i32, f32),
    cur: (i32, f32),
    pos: i64,
}

impl<'a> AutoState<'a> {
    fn new(device: usize, param: usize, points: Points<'a>, len: usize) -> Self {
        let mut s =
            AutoState { device, param, points, len, iter: points, idx: 0, prev: (0, 0.0), cur: (0, 0.0), pos: 0 };
        s.reset();
        s
    }

    fn reset(&mut self) {
        self.iter = self.points;
        self.idx = 0;
        self.cur = self.iter.next().unwrap_or((0, 0.0));
        self.prev = self.cur;
        self.pos = 0;
    }

    /// Upstream `Automation::Run`: value for the sub-block starting at `pos`, then `pos += n`.
    fn step(&mut self, n: usize) -> Option<f32> {
        if self.len == 0 {
            self.pos += n as i64; // upstream reads points[-1] here; we skip
            return None;
        }
        while self.idx < self.len && self.cur.0 as i64 <= self.pos {
            self.prev = self.cur;
            self.idx += 1;
            if let Some(p) = self.iter.next() {
                self.cur = p;
            }
        }
        let v = if self.idx >= self.len {
            self.prev.1
        } else if self.idx == 0 {
            self.cur.1
        } else {
            let span = self.cur.0.wrapping_sub(self.prev.0);
            let t = if span > 0 { (self.pos - self.prev.0 as i64) as f32 / span as f32 } else { 0.0 };
            crate::math::mix(self.prev.1, self.cur.1, t)
        };
        self.pos += n as i64;
        Some(v)
    }
}

pub struct TrackSrc<'a> {
    track: Track<'a>,
    lane: LaneEvents<'a>,
    events: LaneEvents<'a>,
    acc: i64,
    next: Option<(i64, EventKind)>,
    autos: &'a mut [AutoState<'a>],
}

impl TrackSrc<'_> {
    fn reset(&mut self) {
        self.events = self.lane;
        self.acc = 0;
        self.advance();
        self.autos.iter_mut().for_each(AutoState::reset);
    }

    fn advance(&mut self) {
        self.next = self.events.next().map(|(d, k)| {
            self.acc += d as i64;
            (self.acc, k)
        });
    }
}

/// The song blob as a mixer [`Source`]: routing from the blob's tracks, events from their lanes (delta times
/// accumulated), automation by the upstream point walk.
pub struct BlobSource<'a> {
    tracks: &'a mut [TrackSrc<'a>],
}

impl<'a> BlobSource<'a> {
    fn need(song: &Song<'a>) -> usize {
        let mut n = Pool::need::<TrackSrc>(song.track_count());
        for t in song.tracks() {
            n += Pool::need::<AutoState>(t.automations().count());
        }
        n
    }

    fn new(song: &Song<'a>, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let nt = song.track_count();
        let slots = pool.uninit::<TrackSrc>(nt)?;
        let mut built = 0;
        for (slot, t) in slots.iter_mut().zip(song.tracks()) {
            let lane = song.lane(t.lane).ok_or(LoadError::BadIndex)?.events();
            let mut autos = t.automations();
            let autos = pool.slice_with(t.automations().count(), |_| {
                let a = autos.next().unwrap_or_else(|| unreachable!());
                let d = t.devices().nth(a.device).unwrap_or(0);
                AutoState::new(d, a.param, a.points(), a.len())
            })?;
            let mut st = TrackSrc { track: t, lane, events: lane, acc: 0, next: None, autos };
            st.reset();
            slot.write(st);
            built += 1;
        }
        if built != nt {
            return Err(LoadError::Malformed);
        }
        // SAFETY: all `nt` slots written above; MaybeUninit<T> has T's layout.
        let tracks = unsafe { &mut *(slots as *mut [core::mem::MaybeUninit<TrackSrc>] as *mut [TrackSrc]) };
        Ok(BlobSource { tracks })
    }

    fn reset(&mut self) {
        self.tracks.iter_mut().for_each(TrackSrc::reset);
    }
}

impl<'a> Source for BlobSource<'a> {
    type Track<'s>
        = &'s mut TrackSrc<'a>
    where
        Self: 's;

    fn track_count(&self) -> usize {
        self.tracks.len()
    }

    fn track(&mut self, t: usize) -> &mut TrackSrc<'a> {
        &mut self.tracks[t]
    }
}

#[cfg(feature = "threads")]
// SAFETY: one `TrackSrc` per track; routing comes from the immutable blob; automation devices are resolved within the
// track's own device list (`BlobSource::new`).
unsafe impl<'a> super::SplitSource for BlobSource<'a> {
    type Part = TrackSrc<'a>;

    fn parts(&mut self) -> &mut [TrackSrc<'a>] {
        self.tracks
    }
}

impl TrackSource for &mut TrackSrc<'_> {
    fn devices(&self) -> impl Iterator<Item = usize> + '_ {
        self.track.devices()
    }

    fn receives(&self) -> impl Iterator<Item = Receive> + '_ {
        self.track.receives()
    }

    fn volume(&self) -> f32 {
        self.track.volume
    }

    #[inline]
    fn peek(&self) -> Option<(i64, EventKind)> {
        self.next
    }

    #[inline]
    fn advance(&mut self) {
        TrackSrc::advance(self)
    }

    #[inline]
    fn automate(&mut self, n: usize, mut set: impl FnMut(usize, usize, f32)) {
        for a in self.autos.iter_mut() {
            if let Some(v) = a.step(n) {
                set(a.device, a.param, v);
            }
        }
    }
}

pub struct Player<'a, K, R> {
    song: Song<'a>,
    devices: &'a mut [K],
    src: BlobSource<'a>,
    mixer: Mixer<'a>,
    rng: R,
    rng0: R,
    /// Next unread frame in the master's sub-block (`sub` = empty).
    read: usize,
}

impl<'a, K: Kinds<'a>, R: Rng + Clone> Player<'a, K, R> {
    /// Pool bytes [`Player::new`] needs (upper bound).
    pub fn required_pool(song: &Song<'a>, env: &Env<'a>, sub: usize) -> Result<usize, LoadError> {
        Ok(devices_need::<K>(song, env)? + BlobSource::need(song) + Mixer::need(song.track_count(), sub))
    }

    /// Loads `song`: builds devices, validates automation targets, orders tracks, allocates track buffers.
    /// `rng` is the song random source: devices draw from it while loading, and its post-load state is what
    /// seek restores. `sub` is the internal sub-block size (≥ 1).
    pub fn new(song: Song<'a>, env: &Env<'a>, pool: &mut Pool<'a>, mut rng: R, sub: usize) -> Result<Self, LoadError> {
        let devices = load_devices::<K>(&song, env, pool, &mut rng)?;
        for t in song.tracks() {
            for a in t.automations() {
                let d = t.devices().nth(a.device).ok_or(LoadError::BadIndex)?;
                if a.param >= devices[d].param_count() {
                    return Err(LoadError::BadIndex);
                }
            }
        }
        let src = BlobSource::new(&song, pool)?;
        let mut src = src;
        let mut mixer = Mixer::new(&mut src, pool, sub)?;
        mixer.update_look(devices, &mut src);
        let read = mixer.sub();
        Ok(Player { song, devices, src, mixer, rng0: rng.clone(), rng, read })
    }

    pub fn song(&self) -> &Song<'a> {
        &self.song
    }

    pub fn devices(&self) -> &[K] {
        self.devices
    }

    /// Song length in frames (`length · rate`, truncated).
    pub fn length_frames(&self) -> u64 {
        (self.song.length * self.song.rate as f64) as u64
    }

    /// Renders interleaved stereo f32; frames = `out.len() / 2`.
    pub fn render_f32(&mut self, out: &mut [f32]) {
        let _g = crate::denormal::Guard::new();
        for f in out.chunks_exact_mut(2) {
            let (l, r) = self.next_frame();
            f[0] = l;
            f[1] = r;
        }
    }

    /// Renders interleaved stereo i16 (`clamp(trunc(x·32767))`, upstream conversion).
    pub fn render_i16(&mut self, out: &mut [i16]) {
        let _g = crate::denormal::Guard::new();
        for f in out.chunks_exact_mut(2) {
            let (l, r) = self.next_frame();
            f[0] = ((l * 32767.0) as i32).clamp(-32768, 32767) as i16;
            f[1] = ((r * 32767.0) as i32).clamp(-32768, 32767) as i16;
        }
    }

    /// Restores the state at `frame`: resets every device and the song random source, then re-renders silently.
    pub fn seek(&mut self, frame: u64) {
        let _g = crate::denormal::Guard::new();
        self.devices.iter_mut().for_each(Device::reset);
        self.src.reset();
        self.mixer.reset(0);
        self.rng = self.rng0.clone();
        let sub = self.mixer.sub() as u64;
        self.read = sub as usize;
        while self.mixer.frame() + sub <= frame {
            self.render_sub();
        }
        if frame > self.mixer.frame() {
            let start = self.mixer.frame();
            self.render_sub();
            self.read = (frame - start) as usize;
        }
    }

    #[inline]
    fn next_frame(&mut self) -> (f32, f32) {
        if self.read == self.mixer.sub() {
            self.render_sub();
            self.read = 0;
        }
        let i = self.read;
        self.read += 1;
        self.mixer.master_frame(i)
    }

    fn render_sub(&mut self) {
        self.mixer.render_sub(self.devices, &mut self.src, self.song.rate as f64, self.song.tempo, &mut self.rng);
    }
}

/// Multi-threaded rendering (change add-synth-threads): same output as [`Player::render_f32`] /
/// [`Player::render_i16`], with the tracks of each dependency level rendered as jobs on `par`.
#[cfg(feature = "threads")]
impl<'a, K: Kinds<'a> + Send, R: Rng + Clone> Player<'a, K, R> {
    /// Whether the `_par` renders use `par`: no device on two rendered tracks and no device kind that draws from the
    /// song random source while rendering. Otherwise they render on the single-thread path (same output).
    pub fn parallel(&self) -> bool {
        !K::RENDER_RNG && self.mixer.parallel_safe()
    }

    /// [`Player::render_f32`] through `par`.
    pub fn render_f32_par(&mut self, out: &mut [f32], par: &dyn super::Par) {
        let _g = crate::denormal::Guard::new();
        for f in out.chunks_exact_mut(2) {
            let (l, r) = self.next_frame_par(par);
            f[0] = l;
            f[1] = r;
        }
    }

    /// [`Player::render_i16`] through `par`.
    pub fn render_i16_par(&mut self, out: &mut [i16], par: &dyn super::Par) {
        let _g = crate::denormal::Guard::new();
        for f in out.chunks_exact_mut(2) {
            let (l, r) = self.next_frame_par(par);
            f[0] = ((l * 32767.0) as i32).clamp(-32768, 32767) as i16;
            f[1] = ((r * 32767.0) as i32).clamp(-32768, 32767) as i16;
        }
    }

    #[inline]
    fn next_frame_par(&mut self, par: &dyn super::Par) -> (f32, f32) {
        if self.read == self.mixer.sub() {
            if self.parallel() {
                let (rate, tempo) = (self.song.rate as f64, self.song.tempo);
                self.mixer.render_sub_par(self.devices, &mut self.src, rate, tempo, par);
            } else {
                self.render_sub();
            }
            self.read = 0;
        }
        let i = self.read;
        self.read += 1;
        self.mixer.master_frame(i)
    }

    /// The mixer (dependency levels, parallel safety).
    pub fn mixer(&self) -> &Mixer<'a> {
        &self.mixer
    }
}
