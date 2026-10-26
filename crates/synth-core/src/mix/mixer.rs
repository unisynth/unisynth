//! The per-sub-block track step, shared by the blob [`Player`](super::Player) and the DAW live host (change
//! add-synth-daw, design D3). A [`Source`] supplies the routing (devices, receives, volume per track), the events and
//! the automation values; the [`Mixer`] owns the track buffers and the render order and runs one sub-block:
//!
//! per track in dependency order: deliver events (up to the track's lookahead early) to every device on the track,
//! apply automations, clear its 4 buffers, add receives, run its devices in chain order, scale by the track volume.
//! The last track is the master.

use super::song::Receive;
use super::LoadError;
use crate::device::{Ctx, Device, EventKind, Io};
use crate::pool::Pool;
use crate::rng::Rng;

/// What the mixer renders: one [`TrackSource`] per track. Device indices are song-level (into the `devices` slice
/// passed to [`Mixer::render_sub`]).
pub trait Source {
    type Track<'s>: TrackSource
    where
        Self: 's;
    fn track_count(&self) -> usize;
    /// Track `t` (`t < track_count()`). Must not change source state: the mixer also calls it to read routing.
    fn track(&mut self, t: usize) -> Self::Track<'_>;
}

/// One track of a [`Source`]: routing, events and automation.
pub trait TrackSource {
    /// Song-level device indices of the track, in chain order.
    fn devices(&self) -> impl Iterator<Item = usize> + '_;
    fn receives(&self) -> impl Iterator<Item = Receive> + '_;
    fn volume(&self) -> f32;
    /// The next event (absolute frame, kind), not consumed; `None` at the end.
    fn peek(&self) -> Option<(i64, EventKind)>;
    /// Consumes the event [`TrackSource::peek`] returned.
    fn advance(&mut self);
    /// Steps every automation by a sub-block of `n` frames, calling `set(device, param, value)` for each that has a
    /// value, in automation order.
    fn automate(&mut self, n: usize, set: impl FnMut(usize, usize, f32));
}

/// A track's 4 buffers (main L/R, side L/R) and its event lookahead.
struct Bufs<'a> {
    ch: [&'a mut [f32]; 4],
    look: u32,
}

pub struct Mixer<'a> {
    tracks: &'a mut [Bufs<'a>],
    order: &'a mut [usize],
    sub: usize,
    /// Song frame of the next sub-block.
    frame: u64,
}

impl<'a> Mixer<'a> {
    /// Pool bytes [`Mixer::new`] needs for `tracks` tracks (upper bound).
    pub fn need(tracks: usize, sub: usize) -> usize {
        Pool::need::<Bufs>(tracks) + tracks * 4 * Pool::need::<f32>(sub.max(1)) + Pool::need::<usize>(tracks)
            + Pool::need::<u8>(tracks)
    }

    /// Checks that `src` is safe to mix with `devices` song-level devices: at least one track, device indices in
    /// range, receives from another existing track on channel 0 or 2. A receive cycle feeding the master is rejected by
    /// [`Mixer::new`]. Blob sources are checked by `Song::parse`; other sources (the live host) call this before
    /// [`Mixer::new`], which assumes a checked source.
    pub fn check(src: &mut impl Source, devices: usize) -> Result<(), LoadError> {
        let nt = src.track_count();
        if nt == 0 {
            return Err(LoadError::Malformed);
        }
        for t in 0..nt {
            let st = src.track(t);
            if st.devices().any(|d| d >= devices) || st.receives().any(|r| r.track >= nt || r.track == t || (r.channel != 0 && r.channel != 2)) {
                return Err(LoadError::BadIndex);
            }
        }
        Ok(())
    }

    /// Orders the tracks of `src` and allocates their buffers. `sub` is the sub-block size (≥ 1). Lookaheads start
    /// at 0 (see [`Mixer::update_look`]). The source must be valid: at least one track, receive tracks in range,
    /// channels 0 or 2 (`Song::parse` checks this for blobs).
    pub fn new(src: &mut impl Source, pool: &mut Pool<'a>, sub: usize) -> Result<Self, LoadError> {
        let sub = sub.max(1);
        let nt = src.track_count();
        let order = order(src, pool)?;
        let slots = pool.uninit::<Bufs>(nt)?;
        for slot in slots.iter_mut() {
            slot.write(Bufs { ch: [pool.slice(sub)?, pool.slice(sub)?, pool.slice(sub)?, pool.slice(sub)?], look: 0 });
        }
        // SAFETY: all `nt` slots written above; MaybeUninit<T> has T's layout.
        let tracks = unsafe { &mut *(slots as *mut [core::mem::MaybeUninit<Bufs>] as *mut [Bufs]) };
        Ok(Mixer { tracks, order, sub, frame: 0 })
    }

    /// Sets every track's lookahead to the maximum [`Device::event_lookahead`] of its devices (after loading or
    /// rebuilding devices).
    pub fn update_look<D: Device>(&mut self, devices: &[D], src: &mut impl Source) {
        for (t, b) in self.tracks.iter_mut().enumerate() {
            b.look = src.track(t).devices().map(|d| devices[d].event_lookahead()).max().unwrap_or(0);
        }
    }

    pub fn sub(&self) -> usize {
        self.sub
    }

    /// Song frame of the next sub-block.
    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Clears every buffer and restarts at `frame`.
    pub fn reset(&mut self, frame: u64) {
        for t in self.tracks.iter_mut() {
            for b in t.ch.iter_mut() {
                b.fill(0.0);
            }
        }
        self.frame = frame;
    }

    /// Channel `ch` (0/1 main L/R, 2/3 side L/R) of `track` for the last rendered sub-block.
    pub fn channel(&self, track: usize, ch: usize) -> &[f32] {
        &self.tracks[track].ch[ch]
    }

    /// Frame `i` of the master's (last track's) main L/R in the last rendered sub-block.
    #[inline]
    pub fn master_frame(&self, i: usize) -> (f32, f32) {
        let m = &self.tracks[self.tracks.len() - 1];
        (m.ch[0][i], m.ch[1][i])
    }

    /// Renders one sub-block of every track in dependency order and advances the frame.
    pub fn render_sub<D: Device, S: Source>(&mut self, devices: &mut [D], src: &mut S, rate: f64, tempo: i32, rng: &mut dyn Rng) {
        let n = self.sub;
        let t0 = self.frame as i64;
        for oi in 0..self.order.len() {
            let ti = self.order[oi];
            let mut st = src.track(ti);
            let tb = &mut self.tracks[ti];
            // 1. events (with the track's lookahead)
            let limit = t0 + n as i64 + tb.look as i64;
            while let Some((at, kind)) = st.peek() {
                if at >= limit {
                    break;
                }
                let off = (at - t0) as u32;
                for d in st.devices() {
                    match kind {
                        EventKind::NoteOn { note, velocity } => devices[d].note_on(note, velocity, off),
                        EventKind::NoteOff { note } => devices[d].note_off(note, off),
                    }
                }
                st.advance();
            }
            // 2. automations
            st.automate(n, |d, p, v| devices[d].set_param(p, v));
            // 3. clear + receives
            for b in tb.ch.iter_mut() {
                b[..n].fill(0.0);
            }
            for r in st.receives() {
                let (dst, s) = if r.track < ti {
                    let (a, b) = self.tracks.split_at_mut(ti);
                    (&mut b[0], &a[r.track])
                } else {
                    let (a, b) = self.tracks.split_at_mut(r.track);
                    (&mut a[ti], &b[0])
                };
                for j in 0..2 {
                    for (d, s) in dst.ch[r.channel + j][..n].iter_mut().zip(&s.ch[j][..n]) {
                        *d += s * r.volume;
                    }
                }
            }
            // 4. devices in chain order
            let tb = &mut self.tracks[ti];
            let [a, b, c, d] = &mut tb.ch;
            let mut io = Io { ch: [&mut a[..n], &mut b[..n], &mut c[..n], &mut d[..n]] };
            let mut ctx = Ctx { rate, tempo, frame: self.frame, rng: &mut *rng };
            for di in st.devices() {
                devices[di].run(&mut io, n, &mut ctx);
            }
            // 5. volume
            let volume = st.volume();
            if volume != 1.0 {
                for b in tb.ch.iter_mut() {
                    b[..n].iter_mut().for_each(|x| *x *= volume);
                }
            }
        }
        self.frame += n as u64;
    }
}

/// Upstream single-thread order: repeated passes in index order, rendering each track whose sending tracks are done,
/// until the master (last track) is done. A pass that renders nothing means a receive cycle.
fn order<'a>(src: &mut impl Source, pool: &mut Pool<'a>) -> Result<&'a mut [usize], LoadError> {
    let nt = src.track_count();
    let done = pool.slice::<u8>(nt)?;
    let order = pool.slice::<usize>(nt)?;
    let mut n = 0;
    while done[nt - 1] == 0 {
        let before = n;
        for i in 0..nt {
            if done[i] == 0 && src.track(i).receives().all(|r| done[r.track] != 0) {
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
