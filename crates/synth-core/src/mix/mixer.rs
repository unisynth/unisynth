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

#[cfg(feature = "threads")]
mod par;
#[cfg(feature = "threads")]
pub use par::{Par, SplitSource};

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
pub(crate) struct Bufs<'a> {
    ch: [&'a mut [f32]; 4],
    /// Extra pairs (feature `pairs`): `device::EXT_CH` buffers, outputs then inputs.
    #[cfg(feature = "pairs")]
    ext: [&'a mut [f32]; crate::device::EXT_CH],
    look: u32,
    /// Frames the track's output has been quiet (below `QUIET_LEVEL`) in a row.
    quiet: u32,
    /// Frames the track has skipped its devices since they last ran (they get `Device::wake` before they run again).
    asleep: u32,
    /// Frame of the latest event delivered to the track: with lookahead a device holds it until then, awake.
    due: i64,
}

/// A track's output below this peak counts as silence (−120 dBFS).
const QUIET_LEVEL: f32 = 1e-6;
/// The shortest quiet run before a track may sleep, even when its devices have no memory (filters ring a little).
const QUIET_MIN: u32 = 4096;

/// Whether a track may skip its devices in the sub-block starting at `t0`: every event delivered to it is due before
/// `t0` (a device holds an event delivered early until its frame), its input (the receives, already added) is exactly
/// zero, and its output has been quiet for its devices' memory (`need`, the sum of their [`Device::quiet_after`],
/// `None` if one is unknown). Its buffers stay cleared: silence.
#[inline]
pub(crate) fn may_sleep(tb: &Bufs, n: usize, t0: i64, need: Option<u32>) -> bool {
    let Some(need) = need else { return false };
    if tb.due >= t0 || tb.quiet < need.max(QUIET_MIN) {
        return false;
    }
    #[cfg(feature = "pairs")]
    if tb.ext.iter().any(|b| b[..n].iter().any(|&x| x != 0.0)) {
        return false;
    }
    tb.ch.iter().all(|b| b[..n].iter().all(|&x| x == 0.0))
}

/// Whether a device's output (the track's buffers after it ran) reaches `QUIET_LEVEL`: every device's, not only the
/// last one's, since a delay later in the chain holds what an earlier device played while its own output is silent.
#[inline]
pub(crate) fn loud(ch: &[&mut [f32]]) -> bool {
    ch.iter().any(|b| b.iter().any(|x| x.abs() >= QUIET_LEVEL))
}

/// After a sub-block: extends the track's quiet run, or ends it when a device's output was loud (`loud`).
#[inline]
pub(crate) fn note_quiet(tb: &mut Bufs, n: usize, loud: bool) {
    tb.quiet = if loud { 0 } else { tb.quiet.saturating_add(n as u32) };
}

/// The sum of the devices' memories, `None` if one is unknown or its engine refuses the skip
/// ([`Device::quiet_skip`], per device: a song may mix engines).
#[inline]
pub(crate) fn chain_quiet<'d, D: Device + 'd>(devices: impl Iterator<Item = usize>, dev: impl Fn(usize) -> &'d D) -> Option<u32> {
    devices.map(|d| dev(d)).map(|d| if d.quiet_skip() { d.quiet_after() } else { None }).try_fold(0u32, |a, q| q.map(|q| a.saturating_add(q)))
}

/// After the automations of a track that decided to sleep: if one made a device's memory unknown (a parameter that
/// turns silence into sound), the track runs after all, its devices woken here (after the automation, the best left).
/// Returns whether it still sleeps.
#[inline]
pub(crate) fn still_asleep(tb: &mut Bufs, n: usize, need: Option<u32>, mut wake: impl FnMut(u32)) -> bool {
    if need.is_some() {
        return true;
    }
    let slept = tb.asleep.saturating_sub(n as u32);
    if slept > 0 {
        wake(slept);
    }
    tb.asleep = 0;
    false
}

pub struct Mixer<'a> {
    tracks: &'a mut [Bufs<'a>],
    order: &'a mut [usize],
    sub: usize,
    /// Skip quiet tracks ([`Mixer::set_skip_quiet`]).
    skip: bool,
    /// Song frame of the next sub-block.
    frame: u64,
    #[cfg(feature = "threads")]
    levels: par::Levels<'a>,
}

impl<'a> Mixer<'a> {
    /// Pool bytes [`Mixer::new`] needs for `tracks` tracks (upper bound).
    pub fn need(tracks: usize, sub: usize) -> usize {
        let chans = if cfg!(feature = "pairs") { 4 + crate::device::EXT_CH } else { 4 };
        let need = Pool::need::<Bufs>(tracks) + tracks * chans * Pool::need::<f32>(sub.max(1)) + Pool::need::<usize>(tracks)
            + Pool::need::<u8>(tracks);
        #[cfg(feature = "threads")]
        let need = need + par::Levels::need(tracks);
        need
    }

    /// Checks that `src` is safe to mix with `devices` song-level devices: at least one track, device indices in
    /// range, receives from another existing track on channel 0 or 2 (feature `pairs`: a valid `device::recv` field). A
    /// receive cycle feeding the master is rejected by
    /// [`Mixer::new`]. Blob sources are checked by `Song::parse`; other sources (the live host) call this before
    /// [`Mixer::new`], which assumes a checked source.
    pub fn check(src: &mut impl Source, devices: usize) -> Result<(), LoadError> {
        let nt = src.track_count();
        if nt == 0 {
            return Err(LoadError::Malformed);
        }
        for t in 0..nt {
            let st = src.track(t);
            if st.devices().any(|d| d >= devices)
                || st.receives().any(|r| r.track >= nt || r.track == t || !crate::device::recv::valid(r.channel, cfg!(feature = "pairs")))
            {
                return Err(LoadError::BadIndex);
            }
        }
        Ok(())
    }

    /// Orders the tracks of `src` and allocates their buffers. `sub` is the sub-block size (≥ 1). Lookaheads start
    /// at 0 (see [`Mixer::update_look`]). The source must be valid: at least one track, receive tracks in range,
    /// channels 0 or 2, or with feature `pairs` a valid `device::recv` field (`Song::parse` checks this for blobs).
    pub fn new(src: &mut impl Source, pool: &mut Pool<'a>, sub: usize) -> Result<Self, LoadError> {
        let sub = sub.max(1);
        let nt = src.track_count();
        let order = order(src, pool)?;
        #[cfg(feature = "threads")]
        let levels = par::Levels::new(src, order, pool)?;
        let slots = pool.uninit::<Bufs>(nt)?;
        for slot in slots.iter_mut() {
            slot.write(Bufs {
                ch: [pool.slice(sub)?, pool.slice(sub)?, pool.slice(sub)?, pool.slice(sub)?],
                #[cfg(feature = "pairs")]
                ext: {
                    let mut e: [&mut [f32]; crate::device::EXT_CH] = Default::default();
                    for b in e.iter_mut() {
                        *b = pool.slice(sub)?;
                    }
                    e
                },
                look: 0,
                quiet: 0,
                asleep: 0,
                due: i64::MIN,
            });
        }
        // SAFETY: all `nt` slots written above; MaybeUninit<T> has T's layout.
        let tracks = unsafe { &mut *(slots as *mut [core::mem::MaybeUninit<Bufs>] as *mut [Bufs]) };
        Ok(Mixer {
            tracks,
            order,
            sub,
            skip: true,
            frame: 0,
            #[cfg(feature = "threads")]
            levels,
        })
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

    /// Whether a track whose devices all report their memory ([`Device::quiet_after`]) skips its chain while it is
    /// quiet: no events, no input, and silent output for that long. On by default; the output is the same, but for
    /// what a skipped device would have played below −120 dBFS. Off renders every track every sub-block.
    pub fn set_skip_quiet(&mut self, on: bool) {
        if on != self.skip {
            // a quiet run counted before the switch says nothing about what played while it was off
            for t in self.tracks.iter_mut() {
                t.quiet = 0;
            }
        }
        self.skip = on;
    }

    /// Clears every buffer and restarts at `frame`.
    pub fn reset(&mut self, frame: u64) {
        for t in self.tracks.iter_mut() {
            t.quiet = 0;
            t.asleep = 0;
            t.due = i64::MIN;
            for b in t.ch.iter_mut() {
                b.fill(0.0);
            }
            #[cfg(feature = "pairs")]
            for b in t.ext.iter_mut() {
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
    ///
    /// Not inlined (add-synth-daw task 1.4): it brings the WaveSabre size probe back to before the Mixer split
    /// (`ws-size-probe` slaughter: 25 710 → 25 522 B `.text`, pre-split 25 426 + later V2/shrink changes); the
    /// unified-synth probes grow (demo song +272 B, all modules +2.9 KB). One call per sub-block, no measurable speed cost.
    #[inline(never)]
    pub fn render_sub<D: Device, S: Source>(&mut self, devices: &mut [D], src: &mut S, rate: f64, tempo: i32, rng: &mut dyn Rng) {
        let n = self.sub;
        // a constant `false` from the engine drops the skip path; a track asleep when the skip is switched off is still
        // woken
        let capable = devices.first().is_some_and(|d| d.quiet_skip());
        let skip = self.skip && capable;
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
                if capable {
                    tb.due = tb.due.max(at);
                }
            }
            // 2. clear + receives
            for b in tb.ch.iter_mut() {
                b[..n].fill(0.0);
            }
            #[cfg(feature = "pairs")]
            for b in tb.ext.iter_mut() {
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
                add_receive(dst, s, r.channel, r.volume, n);
            }
            // 3. sleep (skip step 5) or wake; then the automations (a sleeping track's too: its parameters stay current)
            let tb = &mut self.tracks[ti];
            let sleep = skip && may_sleep(tb, n, t0, chain_quiet(st.devices(), |d| &devices[d]));
            if capable {
                if tb.asleep > 0 && !sleep {
                    for d in st.devices() {
                        devices[d].wake(tb.asleep);
                    }
                }
                tb.asleep = if sleep { tb.asleep.saturating_add(n as u32) } else { 0 };
            }
            if devices.first().is_some_and(|d| d.automation().is_ok()) {
                st.automate(n, |d, p, v| devices[d].set_param(p, v));
            }
            let tb = &mut self.tracks[ti];
            if sleep && still_asleep(tb, n, chain_quiet(st.devices(), |d| &devices[d]), |s| st.devices().for_each(|d| devices[d].wake(s))) {
                tb.quiet = tb.quiet.saturating_add(n as u32);
                continue;
            }
            // 4. devices in chain order
            let [a, b, c, d] = &mut tb.ch;
            let mut io = Io { ch: [&mut a[..n], &mut b[..n], &mut c[..n], &mut d[..n]] };
            let mut ctx = Ctx { rate, tempo, frame: self.frame, rng: &mut *rng };
            // the input (receives) enters the devices' memory too
            #[allow(unused_mut)]
            let mut sounded = skip && loud(&io.ch);
            #[cfg(feature = "pairs")]
            {
                sounded = sounded || (skip && tb.ext.iter().any(|b| b[..n].iter().any(|x| x.abs() >= QUIET_LEVEL)));
            }
            #[cfg(feature = "pairs")]
            {
                let mut ext: [&mut [f32]; crate::device::EXT_CH] = Default::default();
                for (e, b) in ext.iter_mut().zip(tb.ext.iter_mut()) {
                    *e = &mut b[..n];
                }
                for di in st.devices() {
                    devices[di].run_ext(&mut io, &mut ext, n, &mut ctx);
                    crate::denormal::flush_io(&mut io.ch);
                    sounded = sounded || (skip && (loud(&io.ch) || loud(&ext)));
                }
            }
            #[cfg(not(feature = "pairs"))]
            for di in st.devices() {
                devices[di].run(&mut io, n, &mut ctx);
                crate::denormal::flush_io(&mut io.ch);
                sounded = sounded || (skip && loud(&io.ch));
            }
            // 5. volume
            scale(tb, st.volume(), n);
            if skip {
                note_quiet(tb, n, sounded);
            }
        }
        self.frame += n as u64;
    }
}

/// Scales a track's outputs (main, sidechain and the extra output pairs) by its volume.
#[inline]
pub(crate) fn scale(tb: &mut Bufs, volume: f32, n: usize) {
    if volume == 1.0 {
        return;
    }
    for b in tb.ch.iter_mut() {
        b[..n].iter_mut().for_each(|x| *x *= volume);
    }
    #[cfg(feature = "pairs")]
    for b in tb.ext[..2 * crate::device::OUT_PAIRS].iter_mut() {
        b[..n].iter_mut().for_each(|x| *x *= volume);
    }
}

/// Adds a receive from `s` into `dst`: the selected pair of the sending track into the selected receiving pair.
#[inline]
pub(crate) fn add_receive(dst: &mut Bufs, s: &Bufs, channel: usize, volume: f32, n: usize) {
    use crate::device::recv;
    for j in 0..2 {
        #[cfg(feature = "pairs")]
        let src: &[f32] = match recv::from(channel) {
            0 => &s.ch[j],
            k => &s.ext[2 * (k - 1) + j],
        };
        #[cfg(not(feature = "pairs"))]
        let src: &[f32] = &s.ch[j];
        let to = recv::to(channel);
        #[cfg(feature = "pairs")]
        let d: &mut [f32] = if to >= 4 { &mut dst.ext[2 * crate::device::OUT_PAIRS + (to - 4) + j] } else { &mut dst.ch[to + j] };
        #[cfg(not(feature = "pairs"))]
        let d: &mut [f32] = &mut dst.ch[to + j];
        for (d, s) in d[..n].iter_mut().zip(&src[..n]) {
            *d += s * volume;
        }
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
