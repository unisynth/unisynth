//! The instrument parameter layout (kind 64, chunk versions 3 and 4): 331 values in module blocks, the defaults and
//! (host-side) the names and laws per index. Change unify-v2-algorithms, design "v3 layout (frozen, task 6.4)"; the
//! layout before it is [`old`] (host-only, for the migration). A chunk stores only the values that differ from
//! [`DEFAULTS`]; indices 0–239 take one byte in a chunk, the rest two.
//!
//! ```text
//! voice     0..16    master pan capacity steal unison uni-detune uni-spread mode slide glide transpose
//!                    velocity start-spread one-shot noise-level env1-at-output
//! vs        16..23   voice settings: amp source, pan stage, DC filter, transpose law, key sync, filter routing, balance
//! osc ×3    23..98   25 each: type tracking coarse fine octave ratio ratio-fine fixed-hz gain mix pan pm-out feedback,
//!                    shape[9] (meaning by type, see `shape`), sample index (raw), combine, law
//! noise     98..102  band-on hp lp width
//! filt ×4   102..126 6 each: type cutoff resonance form oversample law
//! env ×4    126..162 9 each: law attack decay sustain release curve curve-slope sustain-slope amplify
//! lfo ×2    162..178 8 each: shape rate depth sync law phase polarity one-shot
//! mod 1–10  178..218 4 each: source target amount operation
//! ctl       218..221 mod wheel, pitch bend, bend range
//! drive     221..225 mode gain p1 p2
//! quirk     225..230 exact math, integer FM, fixed seed, LCG noise, coarse envelope
//! strip     230..261 channel strip (one block: `is_strip`)
//! mod 11–16 261..285
//! cc        285..291 controllers 2–7
//! speech    291..294 on, select, pitch
//! pts ×3    294..321 9 each (change add-tf3-import): point count, segment modes ×5, layers, layer detune, spread
//! el        321..331 (change add-elevated-import): pitch drop base and rate, then linear and hold per envelope
//! ```
//!
//! Everything except the sample indices is normalized 0..1. Fields on a 7-bit step law store `step / 128`
//! ([`v2_raw`] gives the step).

/// Number of values in a patch.
pub const COUNT: usize = el::ENV + ENV_SLOTS * el::ENV_LEN;

/// A 7-bit step field's step (`128 · p`).
#[inline]
pub fn v2_raw(p: f32) -> f32 {
    128.0 * p
}

/// Value of choice `id` of `n` (`law::host::inv_choice` without the clamp, for the player and const tables).
#[inline]
pub const fn choice_value(id: usize, n: usize) -> f32 {
    if n < 2 { 0.0 } else { id as f32 / (n - 1) as f32 }
}

/// Voice group (`0..16`).
pub mod v {
    pub const MASTER: usize = 0;
    pub const PAN: usize = 1;
    pub const CAPACITY: usize = 2;
    pub const STEAL: usize = 3;
    pub const UNISON: usize = 4;
    pub const UNI_DETUNE: usize = 5;
    pub const UNI_SPREAD: usize = 6;
    pub const MODE: usize = 7;
    pub const SLIDE: usize = 8;
    pub const GLIDE: usize = 9;
    pub const TRANSPOSE: usize = 10;
    pub const VELOCITY: usize = 11;
    /// How far the random start phase of an oscillator reaches, `law::start_spread`; 1 = the whole turn.
    pub const START_SPREAD: usize = 12;
    pub const ONE_SHOT: usize = 13;
    pub const NOISE_LEVEL: usize = 14;
    pub const ENV1_AT_OUTPUT: usize = 15;
    pub const LEN: usize = 16;
}

/// Voice settings (absolute indices).
pub const VS: usize = v::LEN;
pub mod vs {
    /// Choice: 0 every envelope as patched, 1 envelope 1 alone sets the amplitude (and the voice ends with it).
    pub const AMP_SOURCE: usize = super::VS;
    /// Choice: 0 each slot pans, 1 the voice pans after the drive (slots mono).
    pub const PAN_STAGE: usize = AMP_SOURCE + 1;
    /// Choice: the voice output's DC filter.
    pub const DC_FILTER: usize = AMP_SOURCE + 2;
    /// Choice: transpose in 0 semitones (`law::transpose`), 1 7-bit steps (`step − 64`).
    pub const LAW: usize = AMP_SOURCE + 3;
    /// Choice of 3: a note-on keeps the oscillator phases (free), restarts them, or resets the voice.
    pub const KEYSYNC: usize = AMP_SOURCE + 4;
    /// Choice: filter slots 1–2 in series or in parallel.
    pub const ROUTING: usize = AMP_SOURCE + 5;
    /// Parallel balance, 7-bit step (64 = both full).
    pub const BALANCE: usize = AMP_SOURCE + 6;
    pub const LEN: usize = 7;
    pub mod keysync {
        pub const FREE: usize = 0;
        pub const RESTART: usize = 1;
        pub const RESET: usize = 2;
        pub const COUNT: usize = 3;
    }
}

/// Oscillator slots: 3 × [`o::LEN`] starting at [`OSC`].
pub const OSC: usize = VS + vs::LEN;
pub const OSC_SLOTS: usize = 3;
pub mod o {
    pub const TYPE: usize = 0;
    pub const TRACKING: usize = 1;
    pub const COARSE: usize = 2;
    pub const FINE: usize = 3;
    pub const OCTAVE: usize = 4;
    pub const RATIO: usize = 5;
    pub const RATIO_FINE: usize = 6;
    pub const FIXED_HZ: usize = 7;
    pub const GAIN: usize = 8;
    pub const MIX: usize = 9;
    pub const PAN: usize = 10;
    pub const PM_OUT: usize = 11;
    pub const FEEDBACK: usize = 12;
    /// First of the 9 type-dependent shape values.
    pub const SHAPE: usize = 13;
    pub const SHAPES: usize = 9;
    /// The GSM sample number or DLS wave index of a sampler slot (raw whole number 0..65535, structural).
    pub const SAMPLE: usize = 22;
    /// Choice of 3 ([`super::combine`]): how the slot meets the sum of the earlier slots.
    pub const COMBINE: usize = 23;
    /// Choice: 0 the unisynth laws, 1 7-bit steps (coarse = transpose `step − 64`, fine = detune, gain = volume, all
    /// `step / 128`).
    pub const LAW: usize = 24;
    pub const LEN: usize = 25;
}

/// Slot combine ops (`o::COMBINE`).
pub mod combine {
    pub const ADD: usize = 0;
    pub const MUL: usize = 1;
    pub const SET: usize = 2;
    pub const COUNT: usize = 3;
}

/// Oscillator types (`o::TYPE`, `Choice(25)`): the id is the stored position. Change add-tf3-import expanded the
/// choice from 13 to 25 positions so every earlier type `k` (stored `k/12`) keeps its value at position `2k`; the
/// types it added take odd positions, and the other odd positions are free (`is_gap`, refused at load).
pub mod osc_type {
    pub const OFF: usize = 0;
    pub const PM: usize = 2;
    pub const BLIT: usize = 4;
    pub const WAVETABLE: usize = 6;
    pub const SAMPLER: usize = 8;
    /// The naive generators (`osc::naive`): saw/triangle, pulse, sine, coloured noise, FM (phase-modulated by the
    /// sum of the earlier slots), aux in A and B (the strip's aux busses). Even ids `NAIVE_SAW..=AUX_B`.
    pub const NAIVE_SAW: usize = 10;
    pub const NAIVE_PULSE: usize = 12;
    pub const SINE: usize = 14;
    pub const NOISE: usize = 16;
    pub const FM: usize = 18;
    pub const AUX_A: usize = 20;
    pub const AUX_B: usize = 22;
    /// Clocked oscillator: `shape[CLK_*]` pick wave, voices, version, clock.
    pub const CLOCKED: usize = 24;
    /// Point oscillator (change add-tf3-import): a cycle drawn through control points (`shape::PTS_*`, `pts`).
    pub const POINTS: usize = 1;
    /// PAD table (change add-tf3-import): a static PADsynth table built at load (`shape::PAD_*`).
    pub const PAD: usize = 3;
    /// Pair oscillator (change add-elevated-import): two naive copies at phase ratios `2 − d` and `d`
    /// (`shape::PAIR_*`).
    pub const PAIR: usize = 5;
    pub const COUNT: usize = 25;
    #[inline]
    pub fn decode(p: f32) -> usize {
        crate::law::choice(p, COUNT)
    }
    pub const fn encode(id: usize) -> f32 {
        super::choice_value(id, COUNT)
    }
    /// Whether position `t` holds no type (a free position of the expanded choice).
    #[inline]
    pub const fn is_gap(t: usize) -> bool {
        t >= COUNT || (t % 2 == 1 && t != POINTS && t != PAD && t != PAIR)
    }
    /// Whether type `t` is a naive generator.
    #[inline]
    pub const fn is_naive(t: usize) -> bool {
        t >= NAIVE_SAW && t <= AUX_B && (t - NAIVE_SAW) % 2 == 0
    }
}

/// Meaning of `o::SHAPE + i` per oscillator type.
pub mod shape {
    /// Phase-mod operator: sine + Square35 blend.
    pub const PM_WAVEFORM: usize = 0;
    /// Band-limited saw/pulse: saw (0) .. pulse (1), pulse width.
    pub const BLIT_WAVEFORM: usize = 0;
    pub const BLIT_PULSE_WIDTH: usize = 1;
    /// Spectral wavetable (Tunefish `Gen*`).
    pub const WT_BANDWIDTH: usize = 0;
    pub const WT_HARMONICS: usize = 1;
    pub const WT_DAMP: usize = 2;
    pub const WT_SCALE: usize = 3;
    pub const WT_MOD: usize = 4;
    pub const WT_DRIVE: usize = 5;
    pub const WT_LAYERS: usize = 6;
    pub const WT_LAYER_DETUNE: usize = 7;
    pub const WT_LAYER_SPREAD: usize = 8;
    /// Sampler.
    pub const SMP_START: usize = 0;
    pub const SMP_LOOP_MODE: usize = 1;
    pub const SMP_BOUNDARY: usize = 2;
    pub const SMP_LOOP_START: usize = 3;
    pub const SMP_LOOP_LENGTH: usize = 4;
    pub const SMP_REVERSE: usize = 5;
    pub const SMP_INTERP: usize = 6;
    pub const SMP_SOURCE: usize = 7;
    /// Naive generators: the colour (saw/triangle break point, pulse width, noise filter, FM depth), `step / 128`.
    pub const NAIVE_COLOR: usize = 1;
    /// Clocked oscillator (change add-cauldron-xm-import): wave `choice(p, 6)` (saw, square, spike, pulse, noise,
    /// fold), voices `round(10p)` (0–10), version `choice(p, 2) + 1`, clock `p·2²⁰` Hz (phrase frames per second: the
    /// original's internal rate times the playback ratio; 0 = one per output frame). The phrase frame at a note-on
    /// comes from the note's velocity (`osc::clocked_t0`).
    pub const CLK_WAVE: usize = 0;
    pub const CLK_VOICES: usize = 1;
    pub const CLK_VERSION: usize = 2;
    pub const CLK_CLOCK: usize = 3;
    /// The note's retrigger byte `round(255p)` = `2k + accent`: the filter's base and amount are compounded by the
    /// slot's `mul` (`o::FEEDBACK`, `4p`) k times and multiplied by the accent (×4 version 1, ×2 version 2). An
    /// automation byte holds it exactly; the converter writes it before each note.
    pub const CLK_RETRIG: usize = 4;
    /// Clocked filter (the original resonant low-pass after the slot, in phrase-frame units): base `c` `p·8`,
    /// envelope amount `p·8` (latched at a note-on), decay per phrase frame `1 + (0.5 − p)/100` (above 1 the
    /// cutoff rises), damping `p`. Base and amount both 0: no filter.
    pub const CLK_F_BASE: usize = 5;
    pub const CLK_F_AMOUNT: usize = 6;
    pub const CLK_F_DECAY: usize = 7;
    pub const CLK_F_DAMP: usize = 8;
    /// Point oscillator (change add-tf3-import): point `k` (0..4) offset at `PTS_POINT + 2k`, value at
    /// `PTS_POINT + 2k + 1`, drive last. Count, segment modes and layers are the appended [`super::pt`] fields.
    /// Shape 7 (point 4's value) is structural for every type (`SMP_SOURCE`): it is not automatable, but the mod
    /// matrix still moves it.
    pub const PTS_POINT: usize = 0;
    pub const PTS_DRIVE: usize = 8;
    /// PAD table (change add-tf3-import): read once at load to build the table (a later change is not applied),
    /// except the drive.
    pub const PAD_BANDWIDTH: usize = 0;
    pub const PAD_SCALE: usize = 1;
    pub const PAD_DAMP: usize = 2;
    pub const PAD_HARMONICS: usize = 3;
    /// `Choice(4)`: single, detune, spread, gauss.
    pub const PAD_PROFILE: usize = 4;
    pub const PAD_DRIVE: usize = 5;
    /// Pair oscillator (change add-elevated-import): waveform (`Choice(3)`: sine, square, saw), phase shift (turns),
    /// detune `d = 2p` (0.5 is `d = 1`), polarity (`Choice(2)`: as is, negated).
    pub const PAIR_WAVE: usize = 0;
    pub const PAIR_SHIFT: usize = 1;
    pub const PAIR_DETUNE: usize = 2;
    pub const PAIR_POLARITY: usize = 3;
}

/// Noise group.
pub const NOISE: usize = OSC + OSC_SLOTS * o::LEN;
pub mod n {
    pub const BAND_ON: usize = 0;
    pub const HP: usize = 1;
    pub const LP: usize = 2;
    /// Noise width `0..1`: 1 gives each channel its own noise (Tunefish's), 0 the same noise on both (WaveSabre's
    /// Slaughter), in between a mix that keeps the power of each channel.
    pub const WIDTH: usize = 3;
    pub const LEN: usize = 4;
}

/// Sampler source choice (`choice(p, 3)` of `shape[SMP_SOURCE]`).
pub mod smp_source {
    /// A GSM sample of the chunk (`sample index`: 1-based, 0 = none).
    pub const GSM: usize = 0;
    /// A GM.DLS instrument looked up by bank and program; each note-on picks the region for its key and velocity.
    /// `sample index = 1 + drums·16384 + bank·128 + program` (0 = none).
    pub const INSTRUMENT: usize = 1;
    /// One wave of the bank by its index (`(int)v − 1`), as WaveSabre's Adultery plays it.
    pub const WAVE: usize = 2;
    pub const COUNT: usize = 3;
}

/// Filter slots: 4 × [`f::LEN`].
pub const FILT: usize = NOISE + n::LEN;
pub const FILT_SLOTS: usize = 4;
pub mod f {
    pub const TYPE: usize = 0;
    pub const CUTOFF: usize = 1;
    pub const RESONANCE: usize = 2;
    /// Choice: state-variable 0 input-scaled / 1 feedback-scaled resonance; 4-pole 0 feedback before / 1 after the
    /// clip.
    pub const FORM: usize = 3;
    /// Choice: state-variable 0 interpolated / 1 held input; 4-pole 0 one / 1 two steps per sample.
    pub const OVERSAMPLE: usize = 4;
    /// Choice: cutoff and resonance on 0 the Hz law, 1 7-bit steps.
    pub const LAW: usize = 5;
    pub const LEN: usize = 6;
}
/// Filter types (`f::TYPE`, `Choice(11)`): the algorithm and its response.
pub mod filter_type {
    pub const OFF: usize = 0;
    pub const SVF_LP: usize = 1;
    pub const SVF_HP: usize = 2;
    pub const SVF_BP: usize = 3;
    pub const SVF_NOTCH: usize = 4;
    pub const P4_LP: usize = 5;
    pub const RBJ_HP: usize = 6;
    pub const RBJ_BP: usize = 7;
    pub const NOTCH: usize = 8;
    pub const SVF_ALL: usize = 9;
    pub const P4_HP: usize = 10;
    pub const COUNT: usize = 11;
    #[inline]
    pub fn decode(p: f32) -> usize {
        crate::law::choice(p, COUNT)
    }
    pub const fn encode(id: usize) -> f32 {
        super::choice_value(id, COUNT)
    }
}

/// Envelopes: 4 × [`e::LEN`].
pub const ENV: usize = FILT + FILT_SLOTS * f::LEN;
pub const ENV_SLOTS: usize = 4;
pub mod e {
    /// [`env_law`].
    pub const LAW: usize = 0;
    pub const ATTACK: usize = 1;
    pub const DECAY: usize = 2;
    pub const SUSTAIN: usize = 3;
    pub const RELEASE: usize = 4;
    /// [`env_curve`].
    pub const CURVE: usize = 5;
    /// The power curve's exponent.
    pub const CURVE_SLOPE: usize = 6;
    /// The sustain level's drift, 7-bit step (64 = flat).
    pub const SUSTAIN_SLOPE: usize = 7;
    /// Output gain, 7-bit step (128 = 1).
    pub const AMPLIFY: usize = 8;
    pub const LEN: usize = 9;
}

/// Envelope time laws (`e::LAW`, `Choice(3)`).
pub mod env_law {
    pub const SECONDS: usize = 0;
    pub const RATE: usize = 1;
    pub const FRAME: usize = 2;
    pub const COUNT: usize = 3;
    pub const fn encode(id: usize) -> f32 {
        super::choice_value(id, COUNT)
    }
}

/// Envelope curves (`e::CURVE`, `Choice(4)`): 0 the law's own (quadratic for seconds, power for rate, exponential
/// for frame).
pub mod env_curve {
    pub const BY_LAW: usize = 0;
    pub const QUADRATIC: usize = 1;
    pub const POWER: usize = 2;
    pub const EXPONENTIAL: usize = 3;
    pub const COUNT: usize = 4;
}

/// LFOs: 2 × [`l::LEN`].
pub const LFO: usize = ENV + ENV_SLOTS * e::LEN;
pub const LFO_SLOTS: usize = 2;
pub mod l {
    /// `Choice(8)` of `lfo::shape`.
    pub const SHAPE: usize = 0;
    pub const RATE: usize = 1;
    pub const DEPTH: usize = 2;
    /// [`super::lfo_sync`].
    pub const SYNC: usize = 3;
    /// Choice: 0 the Hz law, 1 the frame law (rate `¼·calcfreq` turns per frame).
    pub const LAW: usize = 4;
    /// Start phase at a restart, 7-bit step of a turn.
    pub const PHASE: usize = 5;
    /// Choice of 3: positive, negative, bipolar.
    pub const POLARITY: usize = 6;
    /// Choice: one cycle, then hold.
    pub const ONE_SHOT: usize = 7;
    pub const LEN: usize = 8;
}

/// What a note-on does to an LFO (`l::SYNC`, `Choice(3)`).
pub mod lfo_sync {
    /// The phase of an LFO that has run since frame 0 (every voice shares it).
    pub const SONG_TIME: usize = 0;
    /// The voice's LFO runs on from where it was.
    pub const CONTINUE: usize = 1;
    /// Restart at the start phase.
    pub const RESTART: usize = 2;
    pub const COUNT: usize = 3;
}

/// Mod matrix: slots 1–10 at [`MOD`], 11–16 at [`MOD2`] (two-byte); use [`modslot`].
pub const MOD: usize = LFO + LFO_SLOTS * l::LEN;
pub const MOD_SLOTS: usize = 16;
/// Slots in the first block.
pub const MOD_SLOTS1: usize = 10;
pub mod m {
    pub const SOURCE: usize = 0;
    pub const TARGET: usize = 1;
    pub const AMOUNT: usize = 2;
    pub const OPERATION: usize = 3;
    pub const LEN: usize = 4;
}

/// Controllers.
pub const CTL: usize = MOD + MOD_SLOTS1 * m::LEN;
pub mod ctl {
    /// Mod wheel `0..1` (mod source 7); the live input `UInstrument::mod_wheel` writes it.
    pub const WHEEL: usize = super::CTL;
    /// Pitch bend `0..1`, centre `0.5`: every voice's pitch moves by `(bend − 0.5) · 2 · range` semitones.
    pub const BEND: usize = WHEEL + 1;
    /// Bend range `0..24` semitones (`law::bend_range`), default 2.
    pub const BEND_RANGE: usize = BEND + 1;
    pub const LEN: usize = 3;
}

/// Voice drive (after the filters): mode, input gain, param 1, param 2 (7-bit steps).
pub const DRIVE: usize = CTL + ctl::LEN;
pub mod dist {
    pub const MODE: usize = 0;
    pub const GAIN: usize = 1;
    pub const P1: usize = 2;
    pub const P2: usize = 3;
    pub const LEN: usize = 4;
}

/// Quirks (design D2, the 6.3 survey): choices that reproduce particular source builds, each a module.
pub const QUIRK: usize = DRIVE + dist::LEN;
pub mod quirk {
    /// Exact sine and arctangent instead of the fast approximations (sine and FM generators, the drives).
    pub const EXACT_MATH: usize = super::QUIRK;
    /// The FM generator's phase offset rounded to an integer, out of range = the x87 "integer indefinite".
    pub const INTEGER_FM: usize = EXACT_MATH + 1;
    /// Noise generator seeds start at 0.
    pub const FIXED_SEED: usize = EXACT_MATH + 2;
    /// The old LCG noise generator.
    pub const LCG_NOISE: usize = EXACT_MATH + 3;
    /// 256-sample frames and the year-2000 laws for frame-law envelopes.
    pub const COARSE_ENV: usize = EXACT_MATH + 4;
    pub const LEN: usize = 5;
}

/// Channel strip (design D5 of add-v2-import): one block from [`STRIP`], 7-bit steps unless noted. The first ten
/// fields take one chunk byte, the rest two.
pub const STRIP: usize = QUIRK + quirk::LEN;
pub mod ch {
    /// Choice off/on.
    pub const ON: usize = super::STRIP;
    pub const VOL: usize = ON + 1;
    /// Strip distortion: mode, gain, p1, p2.
    pub const DIST: usize = ON + 2;
    pub const SEND_RVB: usize = ON + 6;
    pub const SEND_DLY: usize = ON + 7;
    pub const BOOST: usize = ON + 8;
    /// Raw 0 distortion then chorus, 1 chorus then distortion.
    pub const FXROUTE: usize = ON + 9;
    pub const SEND_AUXA: usize = ON + 10;
    pub const SEND_AUXB: usize = ON + 11;
    pub const RECV_AUXA: usize = ON + 12;
    pub const RECV_AUXB: usize = ON + 13;
    /// Choice: the strip's DC filters.
    pub const DC_FILTER: usize = ON + 14;
    /// Chorus/flanger: amount, feedback, delay L, delay R, mod rate, mod depth, mod phase.
    pub const CHORUS: usize = ON + 15;
    pub const CHORUS_LEN: usize = 7;
    /// Compressor: mode, couple, auto gain, look-ahead, threshold, ratio, attack, release, out gain.
    pub const COMP: usize = CHORUS + CHORUS_LEN;
    pub const COMP_LEN: usize = 9;
    pub const LEN: usize = 15 + CHORUS_LEN + COMP_LEN;
}
/// Whether index `i` is a channel-strip field.
pub const fn is_strip(i: usize) -> bool {
    i >= STRIP && i < STRIP + ch::LEN
}

/// Mod slots 11–16.
pub const MOD2: usize = STRIP + ch::LEN;

/// Controllers 2–7 (7-bit steps): breath, CC 3–6, volume. CC1 is [`ctl::WHEEL`].
pub const CC: usize = MOD2 + (MOD_SLOTS - MOD_SLOTS1) * m::LEN;
pub const CC_COUNT: usize = 6;
/// Index of controller `n` (2..=7).
pub const fn cc(n: usize) -> usize {
    CC + n - 2
}

/// Speech: on (choice), then the speech controllers 4 (text 0–62 / frame rate 63–127) and 5 (pitch), 7-bit steps: a
/// change applies them.
pub const SPEECH: usize = CC + CC_COUNT;
pub mod speech {
    pub const ON: usize = super::SPEECH;
    pub const SELECT: usize = ON + 1;
    pub const PITCH: usize = ON + 2;
}

/// Point-oscillator fields of each oscillator slot (change add-tf3-import, appended after the v3 layout; two-byte
/// indices): point count `count(1..4)`, the segment modes (`Choice(3)`: step, linear, spline) of the segments ending
/// at points 1–4 and of the last segment, layers `count(1..11)`, layer detune `10p³` Hz (left +, right −) and layer
/// spread `10p⁴` Hz per layer, as the wavetable's. A `pad` slot reads the layer detune.
pub const PTS: usize = SPEECH + 3;
pub mod pt {
    pub const COUNT_: usize = 0;
    pub const MODE: usize = 1;
    pub const MODES: usize = 5;
    pub const LAYERS: usize = 6;
    pub const DETUNE: usize = 7;
    pub const SPREAD: usize = 8;
    pub const LEN: usize = 9;
    /// Segment modes (`Choice(3)`).
    pub const STEP: usize = 0;
    pub const LINEAR: usize = 1;
    pub const SPLINE: usize = 2;
}
/// Index of point-oscillator field `fld` of oscillator slot `i`.
pub const fn pts(i: usize, fld: usize) -> usize {
    PTS + i * pt::LEN + fld
}

/// Elevated fields (change add-elevated-import, appended after the point fields; two-byte indices): the voice's pitch
/// drop, `drop.base` (`20 000·p` Hz) and `drop.rate` (`1 − k = 10^(−6(1 − p))` per sample, 0 = off), then each
/// envelope's `linear` (`Choice(2)`) and `hold` (`5000·p²` ms) (design D3, D4).
pub const EL: usize = PTS + OSC_SLOTS * pt::LEN;
pub mod el {
    pub const DROP_BASE: usize = super::EL;
    pub const DROP_RATE: usize = DROP_BASE + 1;
    /// First envelope field; envelope `i`'s field `fld` is [`super::el_env`].
    pub const ENV: usize = DROP_BASE + 2;
    pub const LINEAR: usize = 0;
    pub const HOLD: usize = 1;
    pub const ENV_LEN: usize = 2;
}
/// Index of Elevated envelope field `fld` (`el::LINEAR`, `el::HOLD`) of envelope `i`.
pub const fn el_env(i: usize, fld: usize) -> usize {
    el::ENV + i * el::ENV_LEN + fld
}

/// First two-byte index of the chunk escape (`chunk::SHORT`).
pub const TWO_BYTE: usize = 240;
const _: () = assert!(ch::FXROUTE == TWO_BYTE - 1 && SPEECH + 3 == 294, "the frozen v3 layout (design 6.4)");
const _: () = assert!(EL == 321, "the add-tf3-import layout (design D3)");
const _: () = assert!(COUNT == 331, "the add-elevated-import layout (design D3, D4)");

/// Index of the sample index of oscillator slot `i`.
pub const fn smp_index(i: usize) -> usize {
    osc(i, o::SAMPLE)
}

/// Index of field `fld` of oscillator slot `i`.
pub const fn osc(i: usize, fld: usize) -> usize {
    OSC + i * o::LEN + fld
}
/// Index of field `fld` of filter slot `i`.
pub const fn filt(i: usize, fld: usize) -> usize {
    FILT + i * f::LEN + fld
}
/// Index of field `fld` of envelope `i`.
pub const fn env(i: usize, fld: usize) -> usize {
    ENV + i * e::LEN + fld
}
/// Index of field `fld` of LFO `i`.
pub const fn lfo(i: usize, fld: usize) -> usize {
    LFO + i * l::LEN + fld
}
/// Index of field `fld` of mod slot `i` (0..16).
pub const fn modslot(i: usize, fld: usize) -> usize {
    if i < MOD_SLOTS1 {
        MOD + i * m::LEN + fld
    } else {
        MOD2 + (i - MOD_SLOTS1) * m::LEN + fld
    }
}

/// Whether parameter `i` is stored raw (not normalized): the sample index of each oscillator slot.
pub const fn is_raw(i: usize) -> bool {
    i >= OSC && i < NOISE && (i - OSC) % o::LEN == o::SAMPLE
}

/// Whether parameter `i` shapes the instrument's memory or its modules and is read from the chunk only: voice
/// capacity, each oscillator type, each sampler source and index, the quirks. `set_param` ignores these.
pub const fn is_structural(i: usize) -> bool {
    if i == v::CAPACITY || is_raw(i) || (i >= QUIRK && i < QUIRK + quirk::LEN) {
        return true;
    }
    if i < OSC || i >= NOISE {
        return false;
    }
    let fld = (i - OSC) % o::LEN;
    fld == o::TYPE || fld == o::SHAPE + shape::SMP_SOURCE
}

/// Whether value `i` takes effect only when the instrument loads, given the patch `v`: the structural parameters and a
/// PAD slot's table inputs (bandwidth, scale, damp, harmonics, profile; the table is built at load). A host rebuilds
/// the device when one changes.
pub fn is_load_time(v: &[f32; COUNT], i: usize) -> bool {
    if is_structural(i) {
        return true;
    }
    if i < OSC || i >= NOISE {
        return false;
    }
    let (slot, fld) = ((i - OSC) / o::LEN, (i - OSC) % o::LEN);
    fld >= o::SHAPE + shape::PAD_BANDWIDTH
        && fld <= o::SHAPE + shape::PAD_PROFILE
        && osc_type::decode(v[osc(slot, o::TYPE)]) == osc_type::PAD
}

/// Whether quirk `q` (a [`quirk`] index) is set.
#[inline]
pub fn quirk_on(p: &[f32; COUNT], q: usize) -> bool {
    p[q] >= 0.5
}

/// Default values (a note on a default patch with one oscillator type set sounds: unity gain, centred, filters off,
/// envelope 1 at full sustain with an instant attack). Every field the v3 layout added defaults to its first option,
/// except the balance and the sustain slope (centre), amplify (1) and the LFO sync (restart).
pub const DEFAULTS: [f32; COUNT] = defaults();

const fn defaults() -> [f32; COUNT] {
    let mut d = [0.0f32; COUNT];
    d[v::MASTER] = 0.25; // gain(0.25) = 1
    d[v::PAN] = 0.5;
    d[v::CAPACITY] = 15.0 / 63.0; // 16 voices
    d[v::STEAL] = 1.0; // steal oldest
    d[v::UNI_SPREAD] = 0.5;
    d[v::TRANSPOSE] = 0.5;
    d[v::VELOCITY] = 1.0;
    d[v::START_SPREAD] = 1.0; // a whole turn: unison voices do not start in phase
    d[v::ENV1_AT_OUTPUT] = 1.0;
    d[vs::BALANCE] = 0.5;
    d[ctl::BEND] = 0.5; // centre
    d[ctl::BEND_RANGE] = 2.0 / 24.0; // two semitones
    d[NOISE + n::LP] = 1.0;
    d[NOISE + n::WIDTH] = 1.0; // independent channels
    // 7-bit parts: the source's initial patch values / 128
    d[DRIVE + dist::GAIN] = 32.0 / 128.0;
    d[DRIVE + dist::P2] = 0.5;
    d[cc(7)] = 127.0 / 128.0; // a channel starts at volume 127
    d[ch::VOL] = 1.0;
    d[ch::DIST + dist::GAIN] = 32.0 / 128.0;
    d[ch::DIST + dist::P1] = 100.0 / 128.0;
    d[ch::DIST + dist::P2] = 0.5;
    let chorus = [64.0, 64.0, 32.0, 32.0, 0.0, 0.0, 64.0];
    let comp = [0.0, 0.0, 1.0, 2.0, 90.0, 32.0, 20.0, 64.0, 64.0];
    let mut k = 0;
    while k < ch::CHORUS_LEN {
        d[ch::CHORUS + k] = chorus[k] / 128.0;
        k += 1;
    }
    k = 0;
    while k < ch::COMP_LEN {
        d[ch::COMP + k] = comp[k] / 128.0;
        k += 1;
    }
    let mut i = 0;
    while i < OSC_SLOTS {
        d[osc(i, o::COARSE)] = 0.5;
        d[osc(i, o::FINE)] = 0.5;
        d[osc(i, o::OCTAVE)] = 0.5;
        d[osc(i, o::RATIO_FINE)] = 0.5;
        d[osc(i, o::GAIN)] = 0.25;
        d[osc(i, o::MIX)] = 1.0;
        d[osc(i, o::PAN)] = 0.5;
        d[osc(i, o::SHAPE + shape::BLIT_PULSE_WIDTH)] = 0.5;
        i += 1;
    }
    i = 0;
    while i < FILT_SLOTS {
        d[filt(i, f::CUTOFF)] = 1.0;
        i += 1;
    }
    i = 0;
    while i < ENV_SLOTS {
        d[env(i, e::DECAY)] = 0.3;
        d[env(i, e::SUSTAIN)] = 1.0;
        d[env(i, e::RELEASE)] = 0.1;
        d[env(i, e::CURVE_SLOPE)] = 0.5;
        d[env(i, e::SUSTAIN_SLOPE)] = 0.5;
        d[env(i, e::AMPLIFY)] = 1.0;
        i += 1;
    }
    i = 0;
    while i < LFO_SLOTS {
        d[lfo(i, l::RATE)] = 0.6;
        d[lfo(i, l::DEPTH)] = 1.0;
        d[lfo(i, l::SYNC)] = 1.0; // restart
        i += 1;
    }
    d
}

#[cfg(any(test, feature = "std"))]
mod host;
#[cfg(any(test, feature = "std"))]
pub use host::{law_of, name, Law};
#[cfg(any(test, feature = "std"))]
pub mod old;
#[cfg(any(test, feature = "std"))]
pub mod rename;

#[cfg(test)]
mod tests;
