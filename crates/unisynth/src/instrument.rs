//! The unified instrument (`synth-mix` kind 64): parameters, a `VoicePool` of [`Voice`]s, event slots and the
//! absolute 32-frame control grid (design D4–D5). It implements `synth_core::device::Device`.

use crate::chunk::Chunk;
use crate::flt;
use crate::osc::{Kind, Wt};
use crate::smp;
use crate::params::{self as pm, v, COUNT};
use crate::rng::Xorshift;
use crate::tables;
use crate::voice::{NoteCx, Shared, Voice, CONTROL, FINE};
use crate::law;
use synth_core::device::{Ctx, Device, Event, EventKind, Events, Io, Step, MAIN_L, MAIN_R};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;
use synth_core::rng::Rng;
use synth_core::voice::{Mode, Steal, VoicePool, VoiceState};

/// Voices an instrument can have (`v::CAPACITY` is `1..=64`).
pub const MAX_VOICES: usize = 64;
/// Pending events (a full array drops new ones, like the original `SynthDevice`).
pub const MAX_EVENTS: usize = 256;

/// Whether a slot kind keeps a pool-backed wavetable.
fn wants_table(kind: Kind) -> bool {
    #[cfg(feature = "us-osc-wt")]
    return kind == Kind::Wt;
    #[cfg(not(feature = "us-osc-wt"))]
    {
        let _ = kind;
        false
    }
}

/// Slots of a patch that need a pool-backed wavetable.
fn wt_slots(kinds: &[Kind; pm::OSC_SLOTS]) -> usize {
    kinds.iter().filter(|k| wants_table(**k)).count()
}

/// The modules a patch asks for that the build lacks: filter types in use, and the noise source when its level is set
/// or routed.
pub(crate) fn check_modules(values: &[f32; COUNT]) -> Result<(), LoadError> {
    for i in 0..pm::FILT_SLOTS {
        flt::Kind::check(law::choice(values[pm::filt(i, pm::f::TYPE)], pm::filter_type::COUNT))?;
    }
    // envelope 1 always runs; the others when a matrix slot reads them
    for i in 0..pm::ENV_SLOTS {
        let read = (0..pm::MOD_SLOTS)
            .any(|s| law::choice(values[pm::modslot(s, pm::m::SOURCE)], crate::modmx::source::COUNT) == crate::modmx::source::ENV1 + i);
        if i == 0 || read {
            crate::env::check(law::choice(values[pm::env(i, pm::e::MODE)], 2) == 1)?;
        }
    }
    let routed = (0..pm::MOD_SLOTS).any(|i| {
        law::choice(values[pm::modslot(i, pm::m::SOURCE)], crate::modmx::source::COUNT) != crate::modmx::source::NONE
            && law::index(values[pm::modslot(i, pm::m::TARGET)]) == crate::modmx::target::NOISE_LEVEL
    });
    if !cfg!(feature = "us-noise") && (values[v::NOISE_LEVEL] > 0.0 || routed) {
        return Err(LoadError::UnsupportedModule(crate::module::NOISE));
    }
    Ok(())
}

fn capacity(values: &[f32; COUNT]) -> usize {
    law::count(values[v::CAPACITY], 1, MAX_VOICES as u32) as usize
}

pub struct UInstrument<'a> {
    p: [f32; COUNT],
    rate: f32,
    /// Wheel, bend and bend range as loaded: what a reset (a seek) restores.
    ctl0: [f32; pm::ctl::LEN],
    kinds: [Kind; pm::OSC_SLOTS],
    seed: u32,
    rng: Xorshift,
    voices: VoicePool<'a, Voice<'a>>,
    events: Events<MAX_EVENTS>,
}

impl<'a> UInstrument<'a> {
    /// Pool bytes `new` needs (upper bound), from the chunk alone.
    pub fn need(chunk: &[u8]) -> Result<usize, LoadError> {
        let values = Chunk::parse(chunk)?.values();
        let kinds = Kind::all(&values)?;
        check_modules(&values)?;
        let n = capacity(&values);
        let tables = n * wt_slots(&kinds);
        let chunk = Chunk::parse(chunk)?;
        Ok(VoicePool::<Voice>::need(n) + if tables > 0 { Pool::need::<Wt>(tables) } else { 0 } + smp::pool_need(&values, &chunk)?)
    }

    /// Builds the instrument from its chunk. One draw of the song random source seeds the instrument's own stream.
    pub fn new(chunk: &'a [u8], env: &Env<'a>, pool: &mut Pool<'a>, rng: &mut dyn Rng) -> Result<Self, LoadError> {
        tables::init();
        let values = Chunk::parse(chunk)?.values();
        let kinds = Kind::all(&values)?;
        check_modules(&values)?;
        let seed = Xorshift::seed_from(rng);
        let mut own = Xorshift::new(seed);
        let n = capacity(&values);
        let (samples, insts) = smp::load(&values, &Chunk::parse(chunk)?, env, pool)?;
        let mut tables = pool.slice_with(n * wt_slots(&kinds), |_| Wt::new())?.into_iter();
        let voices = VoicePool::new(pool, n, |_| {
            let wt = core::array::from_fn(|i| if wants_table(kinds[i]) { tables.next() } else { None });
            Voice::new(kinds, &mut own, wt, samples, insts)
        })?;
        let mut s = UInstrument {
            p: values,
            rate: env.rate as f32,
            ctl0: core::array::from_fn(|i| values[pm::CTL + i]),
            kinds,
            seed,
            rng: own,
            voices,
            events: Events::default(),
        };
        s.apply_pool_settings();
        Ok(s)
    }

    pub fn voices(&self) -> &[Voice<'a>] {
        self.voices.voices()
    }

    /// Mod wheel `0..1` (mod source 7): the live input; writes the `ctl.wheel` parameter songs can automate.
    pub fn mod_wheel(&mut self, w: f32) {
        self.set_param(pm::ctl::WHEEL, w);
    }

    /// Runs `f` with the pool and a note context (no event position: phase 0).
    fn with_cx<R>(&mut self, f: impl FnOnce(&mut VoicePool<'a, Voice>, &mut NoteCx) -> R) -> R {
        let UInstrument { p, rate, rng, voices, .. } = self;
        let mut cx = NoteCx { sh: Shared { p, rate: *rate }, rng, frame: 0, force: false };
        f(voices, &mut cx)
    }

    fn apply_pool_settings(&mut self) {
        let p = self.p;
        self.voices.unison = law::count(p[v::UNISON], 1, 16);
        self.voices.detune = p[v::UNI_DETUNE];
        self.voices.pan_spread = p[v::UNI_SPREAD];
        self.voices.steal = if law::choice(p[v::STEAL], 2) == 1 { Steal::Oldest } else { Steal::None };
        let mode = if law::choice(p[v::MODE], 2) == 1 { Mode::MonoLegato } else { Mode::Poly };
        self.with_cx(|pool, cx| pool.set_mode(mode, cx));
    }
}

/// Renders the frames `l.len()` from absolute frame `abs`, ticking the control grid.
fn render_span(voices: &mut VoicePool<Voice>, sh: &Shared, abs: u64, l: &mut [f32], r: &mut [f32]) {
    let mut pos = 0;
    while pos < l.len() {
        let at = abs + pos as u64;
        if at % FINE as u64 == 0 {
            for v in voices.voices_mut().iter_mut().filter(|v| v.is_on()) {
                v.tick(sh, at);
            }
            if at % CONTROL as u64 == 0 {
                voices.tick();
            }
        }
        // up to the next tick: the end of the shortest voice segment (a fine one is FINE frames), or the next
        // control boundary when no voice is sounding
        let to_grid = CONTROL - (at % CONTROL as u64) as u32;
        let seg = voices.voices_mut().iter().filter(|v| v.is_on()).map(|v| v.left()).fold(to_grid, u32::min).max(1) as usize;
        let seg = seg.min(l.len() - pos);
        for v in voices.voices_mut().iter_mut().filter(|v| v.is_on()) {
            v.render(&mut l[pos..pos + seg], &mut r[pos..pos + seg]);
        }
        pos += seg;
    }
}

impl Device for UInstrument<'_> {
    fn param_count(&self) -> usize {
        COUNT
    }

    fn set_param(&mut self, i: usize, value: f32) {
        if i >= COUNT || !value.is_finite() || pm::is_structural(i) {
            return;
        }
        let max = if pm::is_raw(i) { crate::chunk::MAX_SAMPLE_INDEX } else { 1.0 };
        self.p[i] = value.max(0.0).min(max);
        if matches!(i, v::UNISON | v::UNI_DETUNE | v::UNI_SPREAD | v::STEAL | v::MODE) {
            self.apply_pool_settings();
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8, offset: u32) {
        self.events.push(Event { offset, kind: EventKind::NoteOn { note, velocity } });
    }

    fn note_off(&mut self, note: u8, offset: u32) {
        self.events.push(Event { offset, kind: EventKind::NoteOff { note } });
    }

    fn all_notes_off(&mut self) {
        self.with_cx(|pool, cx| {
            cx.force = true;
            pool.all_notes_off(cx)
        });
        self.events.clear();
    }

    fn reset(&mut self) {
        self.events.clear();
        self.voices.clear_state();
        let kinds = self.kinds;
        self.rng = Xorshift::new(self.seed);
        let rng = &mut self.rng;
        self.voices.voices_mut().iter_mut().for_each(|v| v.reset(kinds, rng));
        self.p[pm::CTL..pm::CTL + pm::ctl::LEN].copy_from_slice(&self.ctl0); // the controllers only, not `ext`
    }

    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        let _ftz = synth_core::denormal::Guard::new();
        self.rate = ctx.rate as f32;
        io.ch[MAIN_L][..n].fill(0.0);
        io.ch[MAIN_R][..n].fill(0.0);
        let [l, r, ..] = &mut io.ch;
        let UInstrument { p, rate, rng, voices, events, .. } = self;
        let sh = Shared { p, rate: *rate };
        let frame0 = ctx.frame;
        events.run(n, |step| match step {
            Step::Event { at, kind } => {
                let mut cx = NoteCx { sh, rng, frame: frame0 + at as u64, force: false };
                match kind {
                    EventKind::NoteOn { note, velocity } => voices.note_on(note, velocity, &mut cx),
                    EventKind::NoteOff { note } => voices.note_off(note, &mut cx),
                }
            }
            Step::Render { start, len } => {
                render_span(voices, &sh, frame0 + start as u64, &mut l[start..start + len], &mut r[start..start + len]);
            }
        });
    }
}

// the tests play patches with envelopes in both modes
#[cfg(all(test, feature = "us-osc-pm", feature = "us-env-timed", feature = "us-env-rate"))]
mod tests;
