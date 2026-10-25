//! Song player: single-threaded, deterministic port of upstream WaveSabre `SongRenderer` / `Track::Run`.
//!
//! Per sub-block of `sub` frames (default 100 = upstream players' step), each track in dependency order:
//! deliver lane events (up to the track's event lookahead early) to every device on the track, apply automations,
//! clear its 4 buffers, add receives, run its devices in chain order, scale by the track volume. The last track is
//! the master. Caller buffers of any size are served from the master's sub-block.

use super::registry::{devices_need, load_devices, Env, Kinds};
use super::song::{LaneEvents, Points, Song, Track};
use super::LoadError;
use crate::device::{Ctx, Device, EventKind, Io};
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

struct TrackState<'a> {
    track: Track<'a>,
    bufs: [&'a mut [f32]; 4],
    lane: LaneEvents<'a>,
    events: LaneEvents<'a>,
    acc: i64,
    next: Option<(i64, EventKind)>,
    look: u32,
    autos: &'a mut [AutoState<'a>],
}

impl TrackState<'_> {
    fn reset(&mut self) {
        self.events = self.lane;
        self.acc = 0;
        self.advance();
        self.autos.iter_mut().for_each(AutoState::reset);
        for b in self.bufs.iter_mut() {
            b.fill(0.0);
        }
    }

    fn advance(&mut self) {
        self.next = self.events.next().map(|(d, k)| {
            self.acc += d as i64;
            (self.acc, k)
        });
    }
}

pub struct Player<'a, K, R> {
    song: Song<'a>,
    devices: &'a mut [K],
    tracks: &'a mut [TrackState<'a>],
    order: &'a mut [usize],
    rng: R,
    rng0: R,
    sub: usize,
    /// Song frame of the next sub-block.
    frame: u64,
    /// Next unread frame in the master's sub-block (`sub` = empty).
    read: usize,
}

impl<'a, K: Kinds<'a>, R: Rng + Clone> Player<'a, K, R> {
    /// Pool bytes [`Player::new`] needs (upper bound).
    pub fn required_pool(song: &Song<'a>, env: &Env<'a>, sub: usize) -> Result<usize, LoadError> {
        let nt = song.track_count();
        let mut n = devices_need::<K>(song, env)? + Pool::need::<TrackState>(nt) + Pool::need::<usize>(nt);
        n += Pool::need::<u8>(nt); // dependency flags
        for t in song.tracks() {
            n += 4 * Pool::need::<f32>(sub) + Pool::need::<AutoState>(t.automations().count());
        }
        Ok(n)
    }

    /// Loads `song`: builds devices, validates automation targets, orders tracks, allocates track buffers.
    /// `rng` is the song random source: devices draw from it while loading, and its post-load state is what
    /// seek restores. `sub` is the internal sub-block size (≥ 1).
    pub fn new(song: Song<'a>, env: &Env<'a>, pool: &mut Pool<'a>, mut rng: R, sub: usize) -> Result<Self, LoadError> {
        let sub = sub.max(1);
        let devices = load_devices::<K>(&song, env, pool, &mut rng)?;
        for t in song.tracks() {
            for a in t.automations() {
                let d = t.devices().nth(a.device).ok_or(LoadError::BadIndex)?;
                if a.param >= devices[d].param_count() {
                    return Err(LoadError::BadIndex);
                }
            }
        }
        let nt = song.track_count();
        let order = Self::order(&song, pool)?;
        let slots = pool.uninit::<TrackState>(nt)?;
        let mut built = 0;
        for (slot, t) in slots.iter_mut().zip(song.tracks()) {
            let lane = song.lane(t.lane).ok_or(LoadError::BadIndex)?.events();
            let mut autos = t.automations();
            let autos = pool.slice_with(t.automations().count(), |_| {
                let a = autos.next().unwrap_or_else(|| unreachable!());
                let d = t.devices().nth(a.device).unwrap_or(0);
                AutoState::new(d, a.param, a.points(), a.len())
            })?;
            let mut st = TrackState {
                track: t,
                bufs: [pool.slice(sub)?, pool.slice(sub)?, pool.slice(sub)?, pool.slice(sub)?],
                lane,
                events: lane,
                acc: 0,
                next: None,
                look: t.devices().map(|d| devices[d].event_lookahead()).max().unwrap_or(0),
                autos,
            };
            st.reset();
            slot.write(st);
            built += 1;
        }
        if built != nt {
            return Err(LoadError::Malformed);
        }
        // SAFETY: all `nt` slots written above; MaybeUninit<T> has T's layout.
        let tracks = unsafe { &mut *(slots as *mut [core::mem::MaybeUninit<TrackState>] as *mut [TrackState]) };
        Ok(Player { song, devices, tracks, order, rng0: rng.clone(), rng, sub, frame: 0, read: sub })
    }

    /// Upstream single-thread order: repeated passes in index order, rendering each track whose sending tracks are
    /// done, until the master (last track) is done. A pass that renders nothing means a receive cycle.
    fn order(song: &Song<'a>, pool: &mut Pool<'a>) -> Result<&'a mut [usize], LoadError> {
        let nt = song.track_count();
        let done = pool.slice::<u8>(nt)?;
        let order = pool.slice::<usize>(nt)?;
        let mut n = 0;
        while done[nt - 1] == 0 {
            let before = n;
            for (i, t) in song.tracks().enumerate() {
                if done[i] == 0 && t.receives().all(|r| done[r.track] != 0) {
                    done[i] = 1;
                    order[n] = i;
                    n += 1;
                }
            }
            if n == before {
                return Err(LoadError::BadIndex);
            }
        }
        Ok(&mut order[..n])
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
        self.tracks.iter_mut().for_each(TrackState::reset);
        self.rng = self.rng0.clone();
        self.frame = 0;
        self.read = self.sub;
        let sub = self.sub as u64;
        while self.frame + sub <= frame {
            self.render_sub();
        }
        if frame > self.frame {
            let start = self.frame;
            self.render_sub();
            self.read = (frame - start) as usize;
        }
    }

    #[inline]
    fn next_frame(&mut self) -> (f32, f32) {
        if self.read == self.sub {
            self.render_sub();
            self.read = 0;
        }
        let m = &self.tracks[self.tracks.len() - 1];
        let i = self.read;
        self.read += 1;
        (m.bufs[0][i], m.bufs[1][i])
    }

    fn render_sub(&mut self) {
        let n = self.sub;
        let t0 = self.frame as i64;
        for oi in 0..self.order.len() {
            let ti = self.order[oi];
            // 1. events (with the track's lookahead)
            let st = &mut self.tracks[ti];
            let limit = t0 + n as i64 + st.look as i64;
            while let Some((at, kind)) = st.next {
                if at >= limit {
                    break;
                }
                let off = (at - t0) as u32;
                for d in st.track.devices() {
                    match kind {
                        EventKind::NoteOn { note, velocity } => self.devices[d].note_on(note, velocity, off),
                        EventKind::NoteOff { note } => self.devices[d].note_off(note, off),
                    }
                }
                st.advance();
            }
            // 2. automations
            for a in st.autos.iter_mut() {
                if let Some(v) = a.step(n) {
                    self.devices[a.device].set_param(a.param, v);
                }
            }
            // 3. clear + receives
            for b in st.bufs.iter_mut() {
                b[..n].fill(0.0);
            }
            let track = st.track;
            for r in track.receives() {
                let (dst, src) = if r.track < ti {
                    let (a, b) = self.tracks.split_at_mut(ti);
                    (&mut b[0], &a[r.track])
                } else {
                    let (a, b) = self.tracks.split_at_mut(r.track);
                    (&mut a[ti], &b[0])
                };
                for j in 0..2 {
                    for (d, s) in dst.bufs[r.channel + j][..n].iter_mut().zip(&src.bufs[j][..n]) {
                        *d += s * r.volume;
                    }
                }
            }
            // 4. devices in chain order
            let st = &mut self.tracks[ti];
            let [a, b, c, d] = &mut st.bufs;
            let mut io = Io { ch: [&mut a[..n], &mut b[..n], &mut c[..n], &mut d[..n]] };
            let mut ctx = Ctx { rate: self.song.rate as f64, tempo: self.song.tempo, frame: self.frame, rng: &mut self.rng };
            for di in track.devices() {
                self.devices[di].run(&mut io, n, &mut ctx);
            }
            // 5. volume
            if track.volume != 1.0 {
                for b in st.bufs.iter_mut() {
                    b[..n].iter_mut().for_each(|x| *x *= track.volume);
                }
            }
        }
        self.frame += n as u64;
    }
}
