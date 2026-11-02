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
    // the quirks first: structural, each its own module (spec "Quirk disabled")
    for q in pm::QUIRK..pm::QUIRK + pm::quirk::LEN {
        let id = crate::module::quirk_module(q);
        if pm::quirk_on(values, q) && !crate::module::compiled(id) {
            return Err(LoadError::UnsupportedModule(id));
        }
    }
    for i in 0..pm::FILT_SLOTS {
        flt::Kind::check(pm::filter_type::decode(values[pm::filt(i, pm::f::TYPE)]), flt::Forms::read(values, i))?;
    }
    // envelope 1 always runs; the others when a matrix slot reads them
    for i in 0..pm::ENV_SLOTS {
        let read = (0..pm::MOD_SLOTS)
            .any(|s| crate::modmx::source::decode(values[pm::modslot(s, pm::m::SOURCE)]) == crate::modmx::source::ENV1 + i);
        if i == 0 || read {
            crate::env::check_settings(values, i)?;
        }
    }
    let routed = (0..pm::MOD_SLOTS).any(|i| {
        crate::modmx::source::decode(values[pm::modslot(i, pm::m::SOURCE)]) != crate::modmx::source::NONE
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
        let src = |s: usize| source::decode(values[pm::modslot(s, pm::m::SOURCE)]);
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
    if !cfg!(feature = "us-matrix-add") {
        uses((0..pm::MOD_SLOTS).any(|s| crate::modmx::needs_add(values, s)), crate::module::MATRIX_ADD)?;
    }
    check_v2(values)
}

/// The V2 modules a patch uses (change add-v2-import; `needs::instrument_why` mirrors this): a V2-mode LFO a slot
/// reads, the voice distortion, parallel filter routing, the V2 voice, the channel stage, the speech stage.
fn check_v2(values: &[f32; COUNT]) -> Result<(), LoadError> {
    use crate::module as m;
    let uses = |on: bool, id: u8| if on { Err(LoadError::UnsupportedModule(id)) } else { Ok(()) };
    let raw = |i: usize| pm::v2_raw(values[i]) as i32;
    if !cfg!(feature = "us-lfo-frame") {
        let read = |k: usize| (0..pm::MOD_SLOTS).any(|s| crate::modmx::source::decode(values[pm::modslot(s, pm::m::SOURCE)]) == crate::modmx::source::LFO1 + k);
        uses((0..pm::LFO_SLOTS).any(|k| read(k) && crate::lfo::LfoP::uses_settings(values, k)), m::LFO_FRAME)?;
    }
    if !cfg!(feature = "us-drive") {
        uses(raw(pm::DRIVE + pm::dist::MODE) != 0, m::DRIVE)?;
    }
    if !cfg!(feature = "us-flt-parallel") {
        uses(law::choice(values[pm::vs::ROUTING], 2) == 1, m::FILTER_PARALLEL)?;
    }
    if !cfg!(feature = "us-voice-out") {
        uses([pm::vs::AMP_SOURCE, pm::vs::PAN_STAGE, pm::vs::DC_FILTER, pm::vs::LAW].iter().any(|&i| law::choice(values[i], 2) == 1), m::VOICE_OUT)?;
    }
    if !cfg!(feature = "us-osc-combine") {
        let slot = |i: usize| pm::osc_type::decode(values[pm::osc(i, pm::o::TYPE)]) != pm::osc_type::OFF && law::choice(values[pm::osc(i, pm::o::COMBINE)], pm::combine::COUNT) != pm::combine::ADD;
        uses((0..pm::OSC_SLOTS).any(slot), m::OSC_COMBINE)?;
    }
    if !cfg!(feature = "us-strip") {
        uses(law::choice(values[pm::ch::ON], 2) == 1 || (0..pm::OSC_SLOTS).any(|i| crate::osc::v2_aux(values, i)), m::STRIP)?;
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
    /// The channel stage (`ch::ON`, `us-strip`).
    chan: Option<crate::chan::Chan<'a>>,
    /// Note-ons so far (`NoteCx::seq`).
    seq: u32,
    /// The Ronan speech stage (`ch::SPEECH` with text in the chunk, `us-speech`), the speech controllers it last
    /// applied (raw) and the chunk's text (for a reset).
    #[cfg(feature = "us-speech")]
    speech: Option<crate::speech::Ronan<'a>>,
    #[cfg(feature = "us-speech")]
    speech_ctl: [i32; 2],
    #[cfg(feature = "us-speech")]
    text: &'a [u8],
    /// The editor's filter taps (`set_taps`).
    #[cfg(feature = "probe")]
    taps: Option<crate::probe::Ring>,
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
        let chan = if crate::chan::on(&values) { crate::chan::Chan::need() } else { 0 };
        Ok(VoicePool::<Voice>::need(n) + if tables > 0 { Pool::need::<Wt>(tables) } else { 0 } + smp::pool_need(&values, &chunk)? + chan)
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
        let chan = if crate::chan::on(&values) { Some(crate::chan::Chan::new(pool)?) } else { None };
        #[cfg(feature = "us-speech")]
        let text = Chunk::parse(chunk)?.text();
        let mut s = UInstrument {
            chan,
            seq: 0,
            #[cfg(feature = "us-speech")]
            speech: speech_of(&values, text, env.rate as f32),
            #[cfg(feature = "us-speech")]
            speech_ctl: speech_ctl(&values),
            #[cfg(feature = "us-speech")]
            text,
            p: values,
            rate: env.rate as f32,
            ctl0: core::array::from_fn(|i| values[pm::CTL + i]),
            kinds,
            seed,
            rng: own,
            voices,
            events: Events::default(),
            #[cfg(feature = "probe")]
            taps: None,
        };
        s.apply_pool_settings();
        Ok(s)
    }

    pub fn voices(&self) -> &[Voice<'a>] {
        self.voices.voices()
    }

    /// Starts (or stops) collecting each filter slot's input and output summed over voices (`probe`).
    #[cfg(feature = "probe")]
    pub fn set_taps(&mut self, on: bool) {
        if on != self.taps.is_some() {
            self.taps = on.then(Default::default);
        }
    }

    /// The sounding voices and the filter taps after the last render call. Reads only.
    #[cfg(feature = "probe")]
    pub fn probe(&self) -> crate::probe::Probe {
        let sh = Shared { p: &self.p, rate: self.rate };
        let voices = self.voices.voices().iter().filter(|v| v.is_on()).map(|v| v.snap(&sh)).collect();
        let on = |s: usize| crate::flt::Kind::of(pm::filter_type::decode(self.p[pm::filt(s, pm::f::TYPE)]), crate::flt::Forms::of(&self.p, s)) != crate::flt::Kind::Off;
        let taps = core::array::from_fn(|s| self.taps.as_ref().filter(|_| on(s)).map(|t| t.read(s)));
        crate::probe::Probe { voices, taps }
    }

    /// Mod wheel `0..1` (mod source 7): the live input; writes the `ctl.wheel` parameter songs can automate.
    pub fn mod_wheel(&mut self, w: f32) {
        self.set_param(pm::ctl::WHEEL, w);
    }

    /// Runs `f` with the pool and a note context (no event position: phase 0).
    fn with_cx<R>(&mut self, f: impl FnOnce(&mut VoicePool<'a, Voice>, &mut NoteCx) -> R) -> R {
        let UInstrument { p, rate, rng, voices, seq, .. } = self;
        let mut cx = NoteCx { sh: Shared { p, rate: *rate }, rng, frame: 0, force: false, seq: *seq };
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

/// The speech stage of a patch: on, with text.
#[cfg(feature = "us-speech")]
fn speech_of<'a>(p: &[f32; COUNT], text: &'a [u8], rate: f32) -> Option<crate::speech::Ronan<'a>> {
    (law::choice(p[pm::speech::ON], 2) == 1 && !text.is_empty()).then(|| crate::speech::Ronan::new(text, rate))
}

/// The speech controllers 4 and 5 (raw values).
#[cfg(feature = "us-speech")]
fn speech_ctl(p: &[f32; COUNT]) -> [i32; 2] {
    [libm::rintf(pm::v2_raw(p[pm::speech::SELECT])) as i32, libm::rintf(pm::v2_raw(p[pm::speech::PITCH])) as i32]
}

/// V2's frame at `rate` (128 samples at 44.1 kHz).
#[cfg(feature = "us-speech")]
fn v2_frame(rate: f32) -> u64 {
    ((128.0 * rate / 44_100.0 + 0.5) as u64).max(1)
}

/// Renders the frames `l.len()` from absolute frame `abs`, ticking the control grid.
/// `base` = frame of `l[0]` in the device call (where the editor's taps write).
#[cfg_attr(not(feature = "probe"), allow(unused_variables))]
#[allow(clippy::too_many_arguments)]
fn render_span(
    voices: &mut VoicePool<Voice>,
    sh: &Shared,
    abs: u64,
    l: &mut [f32],
    r: &mut [f32],
    base: usize,
    #[cfg(feature = "probe")] mut tap: Option<&mut crate::probe::Ring>,
    #[cfg(feature = "us-strip")] aux: &[&mut [f32]],
) {
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
            v.render(
                &mut l[pos..pos + seg],
                &mut r[pos..pos + seg],
                #[cfg(feature = "probe")]
                tap.as_deref_mut().map(|t| (t, base + pos)),
                #[cfg(feature = "us-strip")]
                (aux, base + pos),
            );
        }
        pos += seg;
    }
}

/// Delivers the call's events and renders the voices between them into `l`/`r` (`n` frames from `frame0`).
#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn play(
    events: &mut Events<MAX_EVENTS>,
    voices: &mut VoicePool<Voice>,
    rng: &mut Xorshift,
    seq: &mut u32,
    sh: Shared,
    frame0: u64,
    n: usize,
    l: &mut [f32],
    r: &mut [f32],
    #[cfg(feature = "probe")] mut tap: Option<&mut crate::probe::Ring>,
    #[cfg(feature = "us-speech")] mut speech: Option<&mut crate::speech::Ronan>,
    #[cfg(feature = "us-strip")] aux: &[&mut [f32]],
) {
    events.run(n, |step| match step {
        Step::Event { at, kind } => {
            let mut cx = NoteCx { sh, rng, frame: frame0 + at as u64, force: false, seq: *seq };
            match kind {
                EventKind::NoteOn { note, velocity } => {
                    // V2 tells the speech stage of every note-on and note-off of its channel
                    #[cfg(feature = "us-speech")]
                    if let Some(s) = speech.as_deref_mut() {
                        s.note_on();
                    }
                    *seq = seq.wrapping_add(1);
                    cx.seq = *seq;
                    voices.note_on(note, velocity, &mut cx)
                }
                EventKind::NoteOff { note } => {
                    #[cfg(feature = "us-speech")]
                    if let Some(s) = speech.as_deref_mut() {
                        s.note_off();
                    }
                    voices.note_off(note, &mut cx)
                }
            }
        }
        Step::Render { start, len } => {
            render_span(
                voices,
                &sh,
                frame0 + start as u64,
                &mut l[start..start + len],
                &mut r[start..start + len],
                start,
                #[cfg(feature = "probe")]
                tap.as_deref_mut(),
                #[cfg(feature = "us-strip")]
                aux,
            );
        }
    });
}

impl Device for UInstrument<'_> {
    fn param_count(&self) -> usize {
        COUNT
    }

    /// Silent once no voice sounds and no event is queued; a channel stage or speech (V2: their own delays, reverb
    /// and a speech clock) keeps the track awake.
    fn quiet_after(&self) -> Option<u32> {
        #[cfg(feature = "us-speech")]
        if self.speech.is_some() {
            return None;
        }
        (self.chan.is_none() && self.events.is_empty() && !self.voices.voices().iter().any(|v| v.is_on())).then_some(0)
    }

    fn set_param(&mut self, i: usize, value: f32) {
        if i >= COUNT || !value.is_finite() || pm::is_structural(i) {
            return;
        }
        let max = if pm::is_raw(i) { crate::chunk::MAX_SAMPLE_INDEX } else { 1.0 };
        self.p[i] = value.max(0.0).min(max);
        #[cfg(feature = "us-speech")]
        if i == pm::speech::SELECT || i == pm::speech::PITCH {
            // a change of a speech controller is V2's controller message on the speech channel
            let now = speech_ctl(&self.p);
            if let Some(sp) = self.speech.as_mut() {
                for k in 0..2 {
                    if now[k] != self.speech_ctl[k] {
                        sp.set_ctl(4 + k as u32, now[k].max(0) as u32);
                    }
                }
            }
            self.speech_ctl = now;
        }
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
        #[cfg(feature = "us-speech")]
        {
            self.speech = speech_of(&self.p, self.text, self.rate);
            self.speech_ctl = speech_ctl(&self.p);
        }
        let kinds = self.kinds;
        self.rng = Xorshift::new(self.seed);
        let rng = &mut self.rng;
        self.voices.voices_mut().iter_mut().for_each(|v| v.reset(kinds, rng));
        self.p[pm::CTL..pm::CTL + pm::ctl::LEN].copy_from_slice(&self.ctl0); // the controllers only, not `ext`
        if let Some(c) = self.chan.as_mut() {
            c.reset();
        }
        self.seq = 0;
    }

    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        self.render(io, &mut [], n, ctx);
    }

    /// With the channel stage, the extra pairs carry its sends (outputs 1–4: reverb, delay — mono mixes on both
    /// channels — aux A, aux B) and its aux receive (inputs 1–2). Without it, the default (`run`) applies.
    #[cfg(feature = "us-strip")]
    fn run_ext(&mut self, io: &mut Io, ext: &mut [&mut [f32]], n: usize, ctx: &mut Ctx) {
        self.render(io, ext, n, ctx);
    }
}

impl UInstrument<'_> {
    /// One render call (`run` / `run_ext`).
    #[inline(always)]
    fn render(&mut self, io: &mut Io, ext: &mut [&mut [f32]], n: usize, ctx: &mut Ctx) {
        let _ftz = synth_core::denormal::Guard::new();
        self.rate = ctx.rate as f32;
        io.ch[MAIN_L][..n].fill(0.0);
        io.ch[MAIN_R][..n].fill(0.0);
        let [l, r, ..] = &mut io.ch;
        let UInstrument {
            p,
            rate,
            rng,
            voices,
            events,
            seq,
            chan,
            #[cfg(feature = "probe")]
            taps,
            #[cfg(feature = "us-speech")]
            speech,
            ..
        } = self;
        #[cfg(feature = "us-speech")]
        let frame0 = ctx.frame;
        // V2 renders a channel (its speech and its strip) only while one of its voices sounds
        let sounding = (cfg!(feature = "us-speech") || cfg!(feature = "us-strip")) && (voices.voices().iter().any(|v| v.is_on()) || events.has_note_on(n));
        let sh = Shared { p, rate: *rate };
        let (l, r) = (&mut l[..n], &mut r[..n]);
        // the editor's taps (`probe` builds only)
        #[cfg(feature = "probe")]
        if let Some(t) = taps.as_mut() {
            t.begin(n);
        }
        // the aux A/B input pairs, which V2's aux oscillators read (`us-strip`: only it brings the extra pairs)
        #[cfg(feature = "us-strip")]
        let aux: &[&mut [f32]] = if ext.len() >= synth_core::device::EXT_CH { &ext[2 * synth_core::device::OUT_PAIRS..] } else { &[] };
        play(
            events,
            voices,
            rng,
            seq,
            sh,
            ctx.frame,
            n,
            l,
            r,
            #[cfg(feature = "probe")]
            taps.as_mut(),
            #[cfg(feature = "us-speech")]
            speech.as_mut(),
            #[cfg(feature = "us-strip")]
            aux,
        );
        #[cfg(feature = "probe")]
        if let Some(t) = taps.as_mut() {
            t.end();
        }
        // the speech stage speaks through the voice sum, one V2 frame at a time on the absolute grid
        #[cfg(feature = "us-speech")]
        if let Some(sp) = speech.as_mut() {
            let f = v2_frame(*rate);
            let mut pos = 0;
            while pos < n {
                let at = frame0 + pos as u64;
                if at % f == 0 {
                    sp.tick(f as usize);
                }
                let len = ((f - at % f) as usize).min(n - pos);
                if sounding {
                    sp.process(&mut l[pos..pos + len], &mut r[pos..pos + len]);
                }
                pos += len;
            }
        }
        if let Some(c) = chan.as_mut().filter(|_| cfg!(feature = "us-strip") && sounding) {
            // the strip's settings follow the parameters with the newest voice's channel routes (once per call: the
            // player's sub-blocks are on the absolute grid)
            let mut cp = *p;
            if let Some(v) = voices.voices().iter().max_by_key(|v| v.seq).filter(|v| v.seq > 0) {
                v.apply_channel(&mut cp);
            }
            let set = crate::chan::ChanP::new(&cp, *rate);
            if ext.len() >= synth_core::device::EXT_CH {
                let (outs, ins) = ext.split_at_mut(2 * synth_core::device::OUT_PAIRS);
                let [rvb, rvb_r, dly, dly_r, aal, aar, abl, abr] = outs else { unreachable!() };
                let [ial, iar, ibl, ibr] = ins else { unreachable!() };
                let sends = crate::chan::Sends { rvb: &mut rvb[..n], dly: &mut dly[..n], aux: [(&mut aal[..n], &mut aar[..n]), (&mut abl[..n], &mut abr[..n])] };
                c.process(&set, &mut l[..n], &mut r[..n], *rate, Some(sends), Some([(&ial[..n], &iar[..n]), (&ibl[..n], &ibr[..n])]));
                // the reverb and delay buses are mono: the same mix on both channels
                rvb_r[..n].copy_from_slice(&rvb[..n]);
                dly_r[..n].copy_from_slice(&dly[..n]);
            } else {
                c.process(&set, &mut l[..n], &mut r[..n], *rate, None, None);
            }
        }
    }
}

/// A patch that uses an optional voice path loads only in a build that has it.
#[cfg(all(test, feature = "us-env-seconds"))]
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
#[cfg(all(test, feature = "us-osc-pm", feature = "us-env-seconds", feature = "us-env-rate"))]
mod tests;
