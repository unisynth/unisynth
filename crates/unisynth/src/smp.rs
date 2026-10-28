//! The sampler slot (a port of WaveSabre's `SamplePlayer`, shared by Specimen and Adultery): where the data comes from
//! (a GSM sample of the chunk, decoded into the pool at load, or a wave of the caller's GM.DLS bank, read in place),
//! the loop and interpolation settings of a control segment, and the per-sample playback.
//!
//! The position advances by `delta` sample units per output frame (`2^((note − 60)/12)` at the root note 60), as the
//! the original samplers do: the duration of a sample in seconds scales with the song rate.

use crate::chunk::{sample_index, Chunk};
use crate::law;
use crate::module;
use crate::params::{self as pm, o, osc, osc_type, shape, smp_source, COUNT};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

/// Where a slot's sample data lives.
#[derive(Clone, Copy)]
pub enum Source<'a> {
    /// No sample (index 0, no bank, an index past the bank): the slot is silent.
    None,
    /// Decoded GSM in the pool.
    F32(&'a [f32]),
    /// 8/16-bit PCM read in place from the bank.
    #[cfg(feature = "us-sample-dls")]
    Wave(dls::Wave<'a>),
}

impl Source<'_> {
    #[inline]
    pub fn len(&self) -> i32 {
        let n = match self {
            Source::None => 0,
            Source::F32(d) => d.len(),
            #[cfg(feature = "us-sample-dls")]
            Source::Wave(w) => w.len(),
        };
        n.min(i32::MAX as usize) as i32
    }

    /// Sample `i` in `0..len`.
    #[inline]
    fn at(&self, i: i32) -> f32 {
        match self {
            Source::None => 0.0,
            Source::F32(d) => d[i as usize],
            #[cfg(feature = "us-sample-dls")]
            Source::Wave(w) => w.sample(i as usize),
        }
    }
}

/// A GM.DLS instrument of the bank, resolved at load; every note-on picks its region.
#[cfg(feature = "us-sample-dls")]
#[derive(Clone, Copy)]
pub struct Inst<'a> {
    pub bank: dls::Dls<'a>,
    pub ins: dls::Instrument<'a>,
}

/// Without `us-sample-dls` there are no instruments; the type keeps the voice's signature the same.
#[cfg(not(feature = "us-sample-dls"))]
pub struct Inst<'a>(core::marker::PhantomData<&'a ()>);

/// A slot's source with the loop its data carries (`(start, length)`: the whole sample, or the wave's own loop).
/// A DLS-instrument slot holds the region its voice's last note-on picked.
#[derive(Clone, Copy)]
pub struct Sample<'a> {
    pub src: Source<'a>,
    pub sample_loop: (i32, i32),
}

impl Sample<'_> {
    pub const NONE: Sample<'static> = Sample { src: Source::None, sample_loop: (0, 0) };
}

/// How the region a note-on picked changes a slot's playback: the root note (in semitones, fine tune folded in) and the
/// wave's rate against the song's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adj {
    pub root: f32,
    pub rate: f32,
}

impl Adj {
    /// Unity pitch at note 60, no rate change: GSM samples and DLS waves by index.
    pub const UNITY: Adj = Adj { root: 60.0, rate: 1.0 };
}

/// The sample a slot plays for a note-on of `key` at velocity `vel`, and its adjustments. Only a DLS-instrument slot
/// changes: its region's wave (silent when the instrument has no region for the key, or no wave) with the region's
/// (or wave's) root key, fine tune and loop. The bank's articulation is ignored, and so is `wsmp` gain: banks such as
/// Apple's store attenuations of −20 to −113 dB there and make up for them in the articulation, so applying it alone
/// would silence them.
pub fn pick<'a>(inst: &Inst<'a>, key: u8, vel: u8, song_rate: f32) -> (Sample<'a>, Adj) {
    #[cfg(feature = "us-sample-dls")]
    {
        let hit = inst.ins.region_for(key, vel).and_then(|r| inst.bank.sampler(&r));
        return match hit {
            Some((w, ws)) if !w.is_empty() => {
                let len = Source::Wave(w).len();
                let sample_loop = match ws.looped {
                    Some((st, l)) => (st.min(i32::MAX as u32) as i32, l.min(i32::MAX as u32) as i32),
                    None => (0, len),
                };
                let adj = Adj {
                    root: ws.unity_note as f32 - ws.fine_tune as f32 / 100.0,
                    rate: w.rate as f32 / song_rate,
                };
                (Sample { src: Source::Wave(w), sample_loop }, adj)
            }
            _ => (Sample::NONE, Adj::UNITY),
        };
    }
    #[cfg(not(feature = "us-sample-dls"))]
    {
        let _ = (inst, key, vel, song_rate);
        (Sample::NONE, Adj::UNITY)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Loop {
    #[default]
    Off,
    Repeat,
    PingPong,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Interp {
    #[default]
    Nearest,
    Linear,
}

/// Playback settings of one control segment.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cfg {
    pub mode: Loop,
    pub interp: Interp,
    pub loop_s: i32,
    pub loop_l: i32,
    pub loop_e: i32,
}

impl Cfg {
    /// From the slot's shape values (already through the matrix): loop mode, boundary choice, loop start and length.
    pub fn from_shape(sh: &[f32; pm::o::SHAPES], sample: &Sample) -> Cfg {
        let mode = match law::choice(sh[shape::SMP_LOOP_MODE], 3) {
            1 => Loop::Repeat,
            2 => Loop::PingPong,
            _ => Loop::Off,
        };
        let interp = if law::choice(sh[shape::SMP_INTERP], 2) == 1 { Interp::Linear } else { Interp::Nearest };
        let len = sample.src.len();
        let (mut s, mut l) = if law::choice(sh[shape::SMP_BOUNDARY], 2) == 1 {
            ((len as f32 * sh[shape::SMP_LOOP_START]) as i32, (len as f32 * sh[shape::SMP_LOOP_LENGTH]) as i32)
        } else {
            sample.sample_loop
        };
        if l < 1 {
            l = 1;
        }
        if s >= len {
            s = len - 1;
        }
        let mut e = s.saturating_add(l);
        if e > len {
            e = len;
            l = e - s;
        }
        Cfg { mode, interp, loop_s: s, loop_l: l, loop_e: e }
    }
}

/// Per-voice playback state of one slot.
#[derive(Clone, Copy, Debug)]
pub struct State {
    pos: f64,
    /// Playing backwards.
    rev: bool,
    /// Still inside the sample.
    pub active: bool,
}

impl State {
    /// Play position in source frames (the editor's probe).
    #[cfg(feature = "probe")]
    pub fn pos(&self) -> f64 {
        self.pos
    }

    pub const fn new() -> Self {
        State { pos: 0.0, rev: false, active: false }
    }

    /// Note-on: `start` in `0..=1` of the sample, `reverse` plays from the end.
    pub fn start(&mut self, src: &Source, start: f32, reverse: bool) {
        let len = src.len();
        self.rev = reverse;
        self.active = len > 0;
        let last = (len - 1) as f64;
        let s = start as f64;
        self.pos = if reverse { (1.0 - s) * last } else { s * last };
    }

    /// One output frame: the sample value at the position, then the position moves by `step` (≥ 0) in the playing
    /// direction and the loop is applied. Outside the sample the slot ends (`active` false) and the value is 0.
    #[inline]
    pub fn next(&mut self, src: &Source, step: f32, c: &Cfg) -> f32 {
        if !self.active {
            return 0.0;
        }
        let fl = libm::floor(self.pos);
        let fract = self.pos - fl;
        let len = src.len();
        // range check in f64: an out-of-range position never wraps into the sample
        if fl < 0.0 || fl >= len as f64 {
            self.active = false;
            return 0.0;
        }
        let i = fl as i32;
        let sample = match c.interp {
            Interp::Nearest => src.at(i),
            Interp::Linear => {
                let left = src.at(i);
                let mut ri = i + 1;
                if c.mode == Loop::Repeat && ri == c.loop_e {
                    ri = c.loop_s;
                }
                // a degenerate loop reads 0 like an out-of-range right index
                let right = if (0..len).contains(&ri) { src.at(ri) } else { 0.0 };
                (left as f64 * (1.0 - fract) + right as f64 * fract) as f32
            }
        };
        let delta = if self.rev { -(step as f64) } else { step as f64 };
        self.pos += delta;
        match c.mode {
            Loop::Repeat => {
                // loop_l ≥ 1, so these terminate (one step per loop length, like the original)
                let (s, e, l) = (c.loop_s as f64, c.loop_e as f64, c.loop_l as f64);
                if delta > 0.0 {
                    while self.pos >= e {
                        self.pos -= l;
                    }
                } else if self.pos < s {
                    while self.pos < s {
                        self.pos += l;
                    }
                }
            }
            Loop::PingPong => {
                if delta > 0.0 && self.pos >= c.loop_e as f64 {
                    self.pos = (c.loop_e - 1) as f64;
                    self.rev = !self.rev;
                } else if delta < 0.0 && self.pos < c.loop_s as f64 {
                    self.pos = c.loop_s as f64;
                    self.rev = !self.rev;
                }
            }
            Loop::Off => {}
        }
        sample
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

/// A sampler slot's source (`smp_source`) and its raw index.
pub fn choice_of(v: &[f32; COUNT], slot: usize) -> (usize, u32) {
    (law::choice(v[osc(slot, o::SHAPE + shape::SMP_SOURCE)], smp_source::COUNT), sample_index(v[pm::smp_index(slot)]))
}

fn is_sampler(v: &[f32; COUNT], slot: usize) -> bool {
    osc_type::decode(v[osc(slot, o::TYPE)]) == osc_type::SAMPLER
}

/// Pool bytes the decoded GSM samples of the sampler slots need (each distinct sample once).
pub fn pool_need(v: &[f32; COUNT], chunk: &Chunk) -> Result<usize, LoadError> {
    #[cfg_attr(not(any(feature = "us-sample-gsm", feature = "us-sample-dls")), allow(unused_mut))]
    let mut total = 0;
    for i in 0..pm::OSC_SLOTS {
        if !is_sampler(v, i) {
            continue;
        }
        match choice_of(v, i) {
            (smp_source::WAVE, _) => {
                if !cfg!(feature = "us-sample-dls") {
                    return Err(LoadError::UnsupportedModule(module::SAMPLE_DLS));
                }
            }
            (smp_source::INSTRUMENT, _) => {
                if !cfg!(feature = "us-sample-dls") {
                    return Err(LoadError::UnsupportedModule(module::SAMPLE_DLS));
                }
                // the resolved instrument is a pool record (an upper bound: the slot may find nothing)
                #[cfg(feature = "us-sample-dls")]
                {
                    total += Pool::need::<Inst>(1);
                }
            }
            (_, 0) => {}
            (_, idx) => {
                let seen = (0..i).any(|j| is_sampler(v, j) && choice_of(v, j) == (smp_source::GSM, idx));
                if !seen {
                    #[cfg(feature = "us-sample-gsm")]
                    {
                        let bytes = chunk.sample_bytes(idx as usize - 1).ok_or(LoadError::BadChunk)?;
                        total += gsm::GsmChunk::pool_need(bytes)?;
                    }
                    #[cfg(not(feature = "us-sample-gsm"))]
                    return {
                        let _ = chunk;
                        Err(LoadError::UnsupportedModule(module::SAMPLE_GSM))
                    };
                }
            }
        }
    }
    Ok(total)
}

/// The instruments of the DLS-instrument slots (the pool owns them; voices borrow).
pub type Insts<'a> = [Option<&'a Inst<'a>>; pm::OSC_SLOTS];

/// Resolves every sampler slot's source: GSM samples decoded into the pool (a sample used twice is decoded once), DLS
/// waves read in place from the bank, DLS instruments looked up in the bank (silent when the bank or instrument is
/// missing). Slots that are not samplers get [`Sample::NONE`].
pub fn load<'a>(
    v: &[f32; COUNT],
    chunk: &Chunk<'a>,
    env: &Env<'a>,
    pool: &mut Pool<'a>,
) -> Result<([Sample<'a>; pm::OSC_SLOTS], Insts<'a>), LoadError> {
    let mut out = [Sample::NONE; pm::OSC_SLOTS];
    #[cfg_attr(not(feature = "us-sample-dls"), allow(unused_mut))]
    let mut insts: Insts<'a> = [None; pm::OSC_SLOTS];
    #[cfg(feature = "us-sample-dls")]
    let bank = env.bank.and_then(|b| dls::Dls::parse(b).ok());
    #[cfg(not(feature = "us-sample-dls"))]
    let _ = env;
    for i in 0..pm::OSC_SLOTS {
        if !is_sampler(v, i) {
            continue;
        }
        let (source, idx) = choice_of(v, i);
        if source == smp_source::INSTRUMENT {
            #[cfg(feature = "us-sample-dls")]
            if idx > 0 {
                // a missing bank or instrument is silence, not a load error
                let n = idx - 1;
                let (drums, bank_msb, program) = (n & 16384 != 0, ((n >> 7) & 127) as u8, (n & 127) as u8);
                if let Some((b, ins)) = bank.and_then(|b| b.find(drums, bank_msb, program).map(|ins| (b, ins))) {
                    insts[i] = Some(&*pool.value(Inst { bank: b, ins })?);
                }
            }
            #[cfg(not(feature = "us-sample-dls"))]
            return Err(LoadError::UnsupportedModule(module::SAMPLE_DLS));
        } else if source == smp_source::WAVE {
            #[cfg(feature = "us-sample-dls")]
            if idx > 0 {
                // as a C `(int)v − 1`; no bank or an index past it is silence
                if let Some(w) = bank.and_then(|b| b.wave(idx - 1)) {
                    let len = Source::Wave(w).len();
                    let sample_loop = match w.wsmp.looped {
                        Some((s, l)) => (s.min(i32::MAX as u32) as i32, l.min(i32::MAX as u32) as i32),
                        None => (0, len),
                    };
                    out[i] = Sample { src: Source::Wave(w), sample_loop };
                }
            }
            #[cfg(not(feature = "us-sample-dls"))]
            return Err(LoadError::UnsupportedModule(module::SAMPLE_DLS));
        } else if idx > 0 {
            if let Some(j) = (0..i).find(|&j| is_sampler(v, j) && choice_of(v, j) == (smp_source::GSM, idx)) {
                out[i] = out[j];
                continue;
            }
            #[cfg(feature = "us-sample-gsm")]
            {
                let g = chunk.sample(idx as usize - 1).ok_or(LoadError::BadChunk)?;
                let data = g.decode(pool)?;
                out[i] = Sample { src: Source::F32(data), sample_loop: (0, data.len().min(i32::MAX as usize) as i32) };
            }
            #[cfg(not(feature = "us-sample-gsm"))]
            {
                let _ = (chunk, &pool);
                return Err(LoadError::UnsupportedModule(module::SAMPLE_GSM));
            }
        }
    }
    Ok((out, insts))
}

#[cfg(all(test, feature = "us-env-timed"))] // the tests load default patches
mod tests {
    use super::*;
    use crate::chunk::encode;
    use crate::law::host as inv;

    fn sampler(source_dls: bool, index: f32) -> std::vec::Vec<u8> {
        let mut v = pm::DEFAULTS;
        v[osc(0, o::TYPE)] = inv::inv_choice(osc_type::SAMPLER, osc_type::COUNT);
        v[osc(0, o::SHAPE + shape::SMP_SOURCE)] = if source_dls { 1.0 } else { 0.0 };
        v[pm::smp_index(0)] = index;
        encode(&v, &[])
    }

    #[cfg(feature = "us-osc-sample")]
    #[test]
    fn a_sampler_with_no_sample_is_allowed_and_silent() {
        assert!(crate::instrument::UInstrument::need(&sampler(false, 0.0)).is_ok());
        assert!(crate::instrument::UInstrument::need(&sampler(true, 0.0)).is_ok() || !cfg!(feature = "us-sample-dls"));
    }

    #[cfg(all(feature = "us-osc-sample", not(feature = "us-sample-dls")))]
    #[test]
    fn a_dls_slot_without_the_dls_feature_names_the_module() {
        let c = sampler(true, 1.0);
        assert_eq!(crate::instrument::UInstrument::need(&c).err(), Some(LoadError::UnsupportedModule(module::SAMPLE_DLS)));
    }

    #[cfg(not(feature = "us-osc-sample"))]
    #[test]
    fn a_sampler_without_the_sampler_feature_names_the_module() {
        let c = sampler(false, 0.0);
        assert_eq!(crate::instrument::UInstrument::need(&c).err(), Some(LoadError::UnsupportedModule(module::OSC_SAMPLER)));
    }

    #[test]
    fn loop_bounds_follow_the_shape_values() {
        let mut sh = [0.0f32; pm::o::SHAPES];
        sh[shape::SMP_LOOP_MODE] = 0.5;
        sh[shape::SMP_BOUNDARY] = 1.0;
        sh[shape::SMP_LOOP_START] = 0.25;
        sh[shape::SMP_LOOP_LENGTH] = 0.5;
        let data = [0.0f32; 1000];
        let s = Sample { src: Source::F32(&data), sample_loop: (10, 20) };
        let c = Cfg::from_shape(&sh, &s);
        assert_eq!((c.mode, c.loop_s, c.loop_l, c.loop_e), (Loop::Repeat, 250, 500, 750));
        sh[shape::SMP_BOUNDARY] = 0.0;
        let c = Cfg::from_shape(&sh, &s);
        assert_eq!((c.loop_s, c.loop_l, c.loop_e), (10, 20, 30), "from the sample");
        sh[shape::SMP_LOOP_MODE] = 1.0;
        assert_eq!(Cfg::from_shape(&sh, &s).mode, Loop::PingPong);
        sh[shape::SMP_LOOP_MODE] = 0.0;
        assert_eq!(Cfg::from_shape(&sh, &s).mode, Loop::Off);
        // an empty sample never reads: the state ends at once
        let mut st = State::new();
        st.start(&Source::None, 0.0, false);
        assert!(!st.active);
        assert_eq!(st.next(&Source::None, 1.0, &Cfg::default()), 0.0);
    }
}
