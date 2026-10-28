//! Parallel sub-block step (change add-synth-threads, feature `threads`). Compiled only with the feature: without it
//! nothing of this module exists, so builds without the option are byte-identical to the single-thread build.
//!
//! Tracks are grouped into dependency levels at load: level 1 = rendered tracks without receives, level k = tracks
//! whose senders are all in lower levels, the master alone in the last level. Inside a level tracks are independent,
//! so each one runs its whole track step (events, automations, clear, receives, devices in chain order, volume) as one
//! job on a caller-supplied [`Par`]. Each track writes only its own buffers and devices and reads finished buffers of
//! lower levels, and receives are summed in the same order as the serial step, so the output is bit-identical.
//!
//! `synth-core` creates no threads: the caller supplies the workers through [`Par`].

use super::{Bufs, Mixer, Source, TrackSource};
use crate::device::{Ctx, Device, EventKind, Io};
use crate::mix::LoadError;
use crate::pool::Pool;
use crate::rng::Rng;

/// Fork-join workers supplied by the caller.
pub trait Par {
    /// Calls `job(i)` once for every `i < jobs`, in any order and on any threads, and returns after all calls have
    /// finished.
    fn run(&self, jobs: usize, job: &(dyn Fn(usize) + Sync));
}

/// A [`Source`] whose per-track state can be handed to different threads.
///
/// # Safety
/// Implementors promise: `parts()` has one part per track and `&mut parts()[t]` as a [`TrackSource`] behaves exactly
/// like `track(t)`; a track's `devices()` and `receives()` never change while a [`Mixer`] built from the source is
/// used (the parallel-safety check runs once, at [`Mixer::new`]); `automate` sets only devices listed on its own
/// track. [`Mixer::render_sub_par`] relies on this for exclusive access to each track's devices.
pub unsafe trait SplitSource: Source {
    type Part: Send;
    fn parts(&mut self) -> &mut [Self::Part];
}

/// Dependency levels of the rendered tracks and whether they can run in parallel.
pub(super) struct Levels<'a> {
    /// Rendered tracks grouped by level (upstream order inside a level).
    tracks: &'a mut [usize],
    /// Exclusive end of each level in `tracks`.
    ends: &'a mut [usize],
    /// No device is listed on two rendered tracks.
    disjoint: bool,
}

impl<'a> Levels<'a> {
    pub(super) fn need(tracks: usize) -> usize {
        3 * Pool::need::<usize>(tracks)
    }

    /// Levels of the tracks in `order` (the serial render order, master last).
    pub(super) fn new(src: &mut impl Source, order: &[usize], pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let nt = src.track_count();
        let master = nt - 1;
        // level per track, 0 = not rendered; senders of a rendered track come earlier in `order`
        let lvl = pool.slice::<usize>(nt)?;
        let mut top = 0;
        for &t in order.iter().filter(|&&t| t != master) {
            let l = 1 + src.track(t).receives().map(|r| lvl[r.track]).max().unwrap_or(0);
            lvl[t] = l;
            top = top.max(l);
        }
        lvl[master] = top + 1;
        let tracks = pool.slice::<usize>(order.len())?;
        let ends = pool.slice::<usize>(top + 1)?;
        let mut n = 0;
        for (l, end) in ends.iter_mut().enumerate() {
            for &t in order.iter().filter(|&&t| lvl[t] == l + 1) {
                tracks[n] = t;
                n += 1;
            }
            *end = n;
        }
        let mut disjoint = true;
        for (i, &a) in order.iter().enumerate() {
            let len = src.track(a).devices().count();
            for k in 0..len {
                let d = src.track(a).devices().nth(k);
                for &b in &order[i + 1..] {
                    if src.track(b).devices().any(|e| Some(e) == d) {
                        disjoint = false;
                    }
                }
            }
        }
        Ok(Levels { tracks, ends, disjoint })
    }
}

/// Raw pointer shared with worker jobs. Soundness comes from the disjointness argument in [`Mixer::render_sub_par`].
struct Shared<T>(*mut T);

impl<T> Shared<T> {
    #[inline]
    fn get(&self) -> *mut T {
        self.0
    }
}

// SAFETY: jobs only dereference indices no other job touches at the same time (see `render_sub_par`).
unsafe impl<T: Send> Sync for Shared<T> {}

/// The song random source is never handed to a worker; parallel songs do not draw from it while rendering.
struct NoRng;

impl Rng for NoRng {
    fn next_f32(&mut self) -> f32 {
        debug_assert!(false, "a device declared Kinds::RENDER_RNG = false but draws from the song random source");
        0.0
    }
}

impl<'a> Mixer<'a> {
    /// Whether [`Mixer::render_sub_par`] may run this source's tracks in parallel: no device is listed on two rendered
    /// tracks. The caller must also know that no device draws from the song random source while rendering.
    pub fn parallel_safe(&self) -> bool {
        self.levels.disjoint
    }

    /// The rendered tracks grouped by dependency level, lowest first; the master alone in the last level.
    pub fn levels(&self) -> impl Iterator<Item = &[usize]> + '_ {
        let ends = &*self.levels.ends;
        (0..ends.len()).map(move |l| &self.levels.tracks[if l == 0 { 0 } else { ends[l - 1] }..ends[l]])
    }

    /// [`Mixer::render_sub`] with the tracks of each dependency level run as jobs on `par`. Output is bit-identical to
    /// the serial step. Requires [`Mixer::parallel_safe`] and devices that never draw from `Ctx::rng` while rendering
    /// (workers get a stub that returns 0).
    pub fn render_sub_par<D: Device + Send, S: SplitSource>(
        &mut self,
        devices: &mut [D],
        src: &mut S,
        rate: f64,
        tempo: i32,
        par: &dyn Par,
    ) where
        for<'x> &'x mut S::Part: TrackSource,
    {
        assert!(self.levels.disjoint, "render_sub_par on a source that is not parallel-safe");
        let parts = src.parts();
        assert!(parts.len() == self.tracks.len());
        let n = self.sub;
        let frame = self.frame;
        // as the serial step: a build without automation (constant `Err`) skips the walk; read here, before any job
        // holds a device
        let automate = devices.first().is_some_and(|d| d.automation().is_ok());
        let parts = Shared(parts.as_mut_ptr());
        let devs = Shared(devices.as_mut_ptr());
        let ndev = devices.len();
        let nt = self.tracks.len();
        let bufs = Shared(self.tracks.as_mut_ptr());
        let mut start = 0;
        for &end in self.levels.ends.iter() {
            let level = &self.levels.tracks[start..end];
            let job = |i: usize| {
                let ti = level[i];
                // SAFETY: `ti` is unique in `level`, so this job is the only one using part `ti`, track buffer `ti`
                // and (disjointness, `parallel_safe`, stable routing and own-track automation promised by the
                // `unsafe trait SplitSource`) the devices listed on track `ti`. Receives read buffers of lower levels,
                // which no job of this level writes. `ti` < track count (`assert` above); `step` range-checks device
                // and receive indices.
                unsafe { step(ti, &mut *parts.get().add(ti), devs.get(), ndev, bufs.get(), nt, n, frame, rate, tempo, automate) }
            };
            if level.len() == 1 {
                job(0);
            } else {
                par.run(level.len(), &job);
            }
            start = end;
        }
        self.frame += n as u64;
    }
}

/// One track's sub-block, as in the serial [`Mixer::render_sub`] (same operations in the same order).
///
/// # Safety
/// No other thread may access track buffer `ti`, the devices listed on track `ti`, or write the buffers of the tracks
/// `ti` receives from, for the duration of the call; `ti < nt`, `bufs` has `nt` tracks and `devs` `ndev` devices.
/// Device and receive indices are range-checked here (release too), so a source that breaks its routing contract
/// panics instead of reaching memory out of bounds.
#[allow(clippy::too_many_arguments)]
unsafe fn step<D: Device, T: TrackSource>(
    ti: usize,
    mut st: T,
    devs: *mut D,
    ndev: usize,
    bufs: *mut Bufs,
    nt: usize,
    n: usize,
    frame: u64,
    rate: f64,
    tempo: i32,
    automate: bool,
) {
    let _g = crate::denormal::Guard::new();
    let dev = |d: usize| {
        assert!(d < ndev, "device index out of range");
        // SAFETY: caller guarantees exclusive access to the devices of this track.
        unsafe { &mut *devs.add(d) }
    };
    // SAFETY: caller guarantees exclusive access to buffer `ti`.
    let tb = unsafe { &mut *bufs.add(ti) };
    let t0 = frame as i64;
    // 1. events (with the track's lookahead)
    let limit = t0 + n as i64 + tb.look as i64;
    while let Some((at, kind)) = st.peek() {
        if at >= limit {
            break;
        }
        let off = (at - t0) as u32;
        for d in st.devices() {
            match kind {
                EventKind::NoteOn { note, velocity } => dev(d).note_on(note, velocity, off),
                EventKind::NoteOff { note } => dev(d).note_off(note, off),
            }
        }
        st.advance();
    }
    // 2. automations
    if automate {
        st.automate(n, |d, p, v| dev(d).set_param(p, v));
    }
    // 3. clear + receives
    for b in tb.ch.iter_mut() {
        b[..n].fill(0.0);
    }
    for r in st.receives() {
        assert!(r.track != ti && r.track < nt, "receive out of range");
        // SAFETY: a sending track is in a lower level and finished; nobody writes it now.
        let s = unsafe { &*bufs.add(r.track) };
        for j in 0..2 {
            for (d, s) in tb.ch[r.channel + j][..n].iter_mut().zip(&s.ch[j][..n]) {
                *d += s * r.volume;
            }
        }
    }
    // 4. devices in chain order
    let [a, b, c, d] = &mut tb.ch;
    let mut io = Io { ch: [&mut a[..n], &mut b[..n], &mut c[..n], &mut d[..n]] };
    let mut rng = NoRng;
    let mut ctx = Ctx { rate, tempo, frame, rng: &mut rng };
    for di in st.devices() {
        dev(di).run(&mut io, n, &mut ctx);
        crate::denormal::flush_io(&mut io.ch);
    }
    // 5. volume
    let volume = st.volume();
    if volume != 1.0 {
        for b in tb.ch.iter_mut() {
            b[..n].iter_mut().for_each(|x| *x *= volume);
        }
    }
}
