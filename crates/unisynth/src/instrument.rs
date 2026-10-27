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
    // songs checked on the host (`synth-convert rust`), whose build asserts every needed module at compile time
    if cfg!(feature = "us-trusted") {
        return Ok(());
    }
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
    check_voice(values)
}

/// The optional voice paths a patch uses (`needs::instrument_why` mirrors this): an LFO a mod slot reads, a glide time
/// (or a slide time in mono mode), mono-legato mode, unison above one voice, one-shot, any matrix route.
fn check_voice(values: &[f32; COUNT]) -> Result<(), LoadError> {
    use crate::modmx::source;
    let uses = |on: bool, id: u8| if on { Err(LoadError::UnsupportedModule(id)) } else { Ok(()) };
    let mono = law::choice(values[v::MODE], 2) == 1;
    if !cfg!(feature = "us-voice-lfo") {
        let src = |s: usize| law::choice(values[pm::modslot(s, pm::m::SOURCE)], source::COUNT);
        uses((0..pm::MOD_SLOTS).any(|s| (source::LFO1..source::LFO1 + pm::LFO_SLOTS).contains(&src(s))), crate::module::VOICE_LFO)?;
    }
    if !cfg!(feature = "us-voice-glide") {
        uses(values[v::GLIDE] > 0.0 || (mono && values[v::SLIDE] > 0.0), crate::module::VOICE_GLIDE)?;
    }
    if !cfg!(feature = "us-voice-mono") {
        uses(mono, crate::module::VOICE_MONO)?;
    }
    if !cfg!(feature = "us-voice-unison") {
        uses(law::count(values[v::UNISON], 1, 16) > 1, crate::module::VOICE_UNISON)?;
    }
    if !cfg!(feature = "us-voice-oneshot") {
        uses(law::choice(values[v::ONE_SHOT], 2) == 1, crate::module::VOICE_ONESHOT)?;
    }
    if !cfg!(feature = "us-matrix") {
        uses((0..pm::MOD_SLOTS).any(|s| crate::modmx::routes(values, s)), crate::module::MATRIX)?;
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

/// A patch that uses an optional voice path loads only in a build that has it.
#[cfg(all(test, feature = "us-env-timed"))]
mod voice_gating {
    use super::*;
    use crate::law::host::{inv_choice, inv_count};
    use crate::module as m;
    use crate::modmx::source;

    #[test]
    fn each_voice_path_is_refused_without_its_feature_and_loads_with_it() {
        let cases: [(&[(usize, f32)], u8, bool); 7] = [
            (&[(pm::modslot(4, pm::m::SOURCE), inv_choice(source::LFO1, source::COUNT))], m::VOICE_LFO, cfg!(feature = "us-voice-lfo")),
            (&[(pm::modslot(0, pm::m::SOURCE), inv_choice(source::LFO1 + 1, source::COUNT))], m::VOICE_LFO, cfg!(feature = "us-voice-lfo")),
            (&[(v::GLIDE, 0.2)], m::VOICE_GLIDE, cfg!(feature = "us-voice-glide")),
            (&[(v::MODE, 1.0), (v::SLIDE, 0.2)], m::VOICE_GLIDE, cfg!(feature = "us-voice-glide")),
            (&[(v::MODE, 1.0)], m::VOICE_MONO, cfg!(feature = "us-voice-mono")),
            (&[(v::UNISON, inv_count(2, 1, 16))], m::VOICE_UNISON, cfg!(feature = "us-voice-unison")),
            (&[(v::ONE_SHOT, 1.0)], m::VOICE_ONESHOT, cfg!(feature = "us-voice-oneshot")),
        ];
        for (edits, id, on) in cases {
            let mut p = pm::DEFAULTS;
            edits.iter().for_each(|&(i, x)| p[i] = x);
            let r = check_voice(&p);
            // glide is checked before mono, so a mono patch with a slide time names glide first
            if on {
                assert_ne!(r, Err(LoadError::UnsupportedModule(id)), "{edits:?}");
            } else {
                assert_eq!(r, Err(LoadError::UnsupportedModule(id)), "{edits:?}");
            }
        }
        // the matrix: a slot with a source and a target
        let mut p = pm::DEFAULTS;
        p[pm::modslot(3, pm::m::SOURCE)] = inv_choice(source::VELOCITY, source::COUNT);
        let half = check_voice(&p);
        p[pm::modslot(3, pm::m::TARGET)] = crate::law::host::inv_index(crate::modmx::target::GAIN);
        assert_eq!(half, Ok(()), "a source without a target routes nothing");
        let r = check_voice(&p);
        if cfg!(feature = "us-matrix") { assert_eq!(r, Ok(())) } else { assert_eq!(r, Err(LoadError::UnsupportedModule(m::MATRIX))) }
        // unused paths: an LFO nothing reads, a slide time in poly mode, unison detune at one voice
        let mut p = pm::DEFAULTS;
        p[pm::lfo(0, pm::l::RATE)] = 0.9;
        p[v::SLIDE] = 0.5;
        p[v::UNI_DETUNE] = 0.5;
        assert_eq!(check_voice(&p), Ok(()));
        assert_eq!(check_voice(&pm::DEFAULTS), Ok(()));
    }
}

// the tests play patches with envelopes in both modes
#[cfg(all(test, feature = "us-osc-pm", feature = "us-env-timed", feature = "us-env-rate"))]
mod tests;
