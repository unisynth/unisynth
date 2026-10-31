//! The instrument parameter layout (kind 64): 260 values in groups, the defaults and (host-side) the
//! names and laws per index. Design D3, V2 part design D4a (change add-v2-import). A chunk stores only the values that
//! differ from [`DEFAULTS`].
//!
//! ```text
//! voice  0..16    master pan capacity steal unison uni-detune uni-spread mode slide glide transpose
//!                 velocity start-spread one-shot noise-level env1-at-output
//! osc ×3 16..82   22 each: type tracking coarse fine octave ratio ratio-fine fixed-hz gain mix pan pm-out
//!                 feedback, then shape[9] (meaning by oscillator type, see `shape`)
//! noise  82..85   band-on hp lp
//! filt ×4 85..97  3 each: type cutoff resonance
//! env ×4 97..121  6 each: mode attack decay sustain release slope
//! lfo ×2 121..129 4 each: shape rate depth sync
//! mod ×12 129..177 4 each: source target amount operation
//! smp ×3 177..180 the GSM sample number or DLS wave index of each sampler slot (raw whole numbers, structural)
//! ctl    180..183 mod wheel, pitch bend, bend range (for the DAW; automatable)
//! ext    183..184 noise width (1 = independent channels, 0 = the same noise on both)
//! v2     184..189 V2 voice, key sync, format version, filter routing, filter balance
//! vdist  189..193 voice distortion: mode gain p1 p2
//! envamp 193..197 V2 envelope amplify ×4
//! lfoext 197..205 2 × (mode phase polarity eg)
//! mod2   205..221 mod slots 13–16
//! cc     221..227 controllers 2–7
//! ch     227..240 channel stage on, volume, boost, fx order, sends ×4, aux receive ×2, speech on, speech ctl 4/5
//! ch     240..260 channel distortion ×4, chorus ×7, compressor ×9   (two-byte indices in the chunk)
//! ```
//!
//! Everything except the sample indices is normalized 0..1. Every V2 field (the appended ones, and the existing fields
//! a V2 type or mode reads) stores `V2 raw / 128`: see [`v2_raw`].

/// Number of values in a patch.
pub const COUNT: usize = CH2 + ch::LEN2;

/// A V2 field's raw value (`128 · p`): the number V2's own code works on.
#[inline]
pub fn v2_raw(p: f32) -> f32 {
    128.0 * p
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

/// Oscillator slots: 3 × [`o::LEN`] starting at [`OSC`].
pub const OSC: usize = v::LEN;
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
    pub const LEN: usize = 22;
}

/// Expanded choices (design D4a): a choice of `n` kinds grows to `2n − 1` stored positions so a stored `k/(n−1)`
/// keeps decoding to kind `k`; later kinds take the odd positions. Ids here are semantic (old kinds keep their ids,
/// later kinds follow); `decode` reads a stored value, `encode` writes one. Encoding an old kind with
/// `inv_choice(k, OLD)` gives the same value as `encode(k)`.
pub mod kinds {
    /// Kind id of a stored value: `old` kinds at the even positions, `later` (in order) at the odd ones, other odd
    /// positions → `none`.
    #[inline]
    pub fn decode(p: f32, old: usize, later: usize, none: usize) -> usize {
        at(crate::law::choice(p, 2 * old - 1), old, later, none)
    }
    /// [`decode`] in a build that may lack the later kinds (`built`: their feature is on). A trusted build
    /// (`us-trusted`: the song's modules were checked on the host) without them only ever reads old kinds, whose
    /// stored values decode with the old law (`choice(p, old)`, the pre-expansion code); every other build keeps the
    /// full decode, so a later kind still fails to load.
    #[inline]
    pub fn decode_in(p: f32, old: usize, later: usize, none: usize, built: bool) -> usize {
        if cfg!(feature = "us-trusted") && !built {
            crate::law::choice(p, old)
        } else {
            decode(p, old, later, none)
        }
    }
    /// Kind id at stored position `c` (`0..2·old − 1`).
    #[inline]
    pub fn at(c: usize, old: usize, later: usize, none: usize) -> usize {
        if c % 2 == 0 {
            c / 2
        } else if c / 2 < later {
            old + c / 2
        } else {
            none
        }
    }
    /// Stored value of kind `id`.
    pub fn encode(id: usize, old: usize) -> f32 {
        let top = (2 * old - 2) as f32;
        let pos = if id < old { 2 * id } else { 2 * (id - old) + 1 };
        pos as f32 / top
    }
}

/// Oscillator types (`o::TYPE`, [`kinds`] with 5 old kinds).
pub mod osc_type {
    pub const OFF: usize = 0;
    pub const PM: usize = 1;
    pub const BLIT: usize = 2;
    pub const WAVETABLE: usize = 3;
    pub const SAMPLER: usize = 4;
    /// Kinds of chunk v1 (`inv_choice(k, COUNT)` still encodes them).
    pub const COUNT: usize = 5;
    /// V2 oscillator (change add-v2-import): `shape[V2_MODE]` picks its waveform.
    pub const V2: usize = 5;
    /// Mini-cauldron oscillator (change add-cauldron-xm-import): `shape[CD_*]` pick wave, voices, version, clock.
    pub const CLOCKED: usize = 6;
    pub const ALL: usize = 7;
    #[inline]
    pub fn decode(p: f32) -> usize {
        // a trusted build reads the expanded choice when it has any later kind (its song was checked on the host)
        super::kinds::decode_in(p, COUNT, ALL - COUNT, OFF, cfg!(feature = "us-osc-naive") || cfg!(feature = "us-osc-clocked"))
    }
    pub fn encode(id: usize) -> f32 {
        super::kinds::encode(id, COUNT)
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
    // shape[8] is unused by the sampler (the sample index lives in the `SMP` group: raw values do not mix with
    // normalized shape values)
    /// V2 oscillator: mode raw 0 off, 1 saw/tri, 2 pulse, 3 sine, 4 noise, 5 FM, 6 aux A, 7 aux B; colour; ring (raw
    /// bit 0). Its coarse field is V2's transpose, fine its detune, gain its volume (all raw / 128).
    pub const V2_MODE: usize = 0;
    pub const V2_COLOR: usize = 1;
    pub const V2_RING: usize = 2;
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
}

/// Noise group (`82..85`).
pub const NOISE: usize = OSC + OSC_SLOTS * o::LEN;
pub mod n {
    pub const BAND_ON: usize = 0;
    pub const HP: usize = 1;
    pub const LP: usize = 2;
    pub const LEN: usize = 3;
}

/// Filter slots: 4 × [`f::LEN`].
pub const FILT: usize = NOISE + n::LEN;
pub const FILT_SLOTS: usize = 4;
pub mod f {
    pub const TYPE: usize = 0;
    pub const CUTOFF: usize = 1;
    pub const RESONANCE: usize = 2;
    pub const LEN: usize = 3;
}
/// Filter types (`f::TYPE`, [`kinds`] with 9 old kinds). V2's filters read cutoff and resonance raw / 128.
pub mod filter_type {
    pub const OFF: usize = 0;
    pub const SVF_LP: usize = 1;
    pub const SVF_HP: usize = 2;
    pub const SVF_BP: usize = 3;
    pub const SVF_NOTCH: usize = 4;
    pub const LADDER_LP: usize = 5;
    pub const RBJ_HP: usize = 6;
    pub const RBJ_BP: usize = 7;
    pub const NOTCH: usize = 8;
    pub const COUNT: usize = 9;
    pub const V2_LP: usize = 9;
    pub const V2_BP: usize = 10;
    pub const V2_HP: usize = 11;
    pub const V2_NOTCH: usize = 12;
    pub const V2_ALL: usize = 13;
    pub const V2_MOOG_LP: usize = 14;
    pub const V2_MOOG_HP: usize = 15;
    pub const ALL: usize = 16;
    #[inline]
    pub fn decode(p: f32) -> usize {
        super::kinds::decode_in(p, COUNT, ALL - COUNT, OFF, cfg!(feature = "us-flt-feedback"))
    }
    pub fn encode(id: usize) -> f32 {
        super::kinds::encode(id, COUNT)
    }
}

/// Envelopes: 4 × [`e::LEN`].
pub const ENV: usize = FILT + FILT_SLOTS * f::LEN;
pub const ENV_SLOTS: usize = 4;
pub mod e {
    /// [`env_mode`].
    pub const MODE: usize = 0;
    pub const ATTACK: usize = 1;
    pub const DECAY: usize = 2;
    pub const SUSTAIN: usize = 3;
    pub const RELEASE: usize = 4;
    pub const SLOPE: usize = 5;
    pub const LEN: usize = 6;
}

/// Envelope modes (`e::MODE`, [`kinds`] with 2 old modes).
pub mod env_mode {
    pub const TIMED: usize = 0;
    pub const RATE: usize = 1;
    pub const COUNT: usize = 2;
    /// V2's envelope: attack, decay, sustain, sustain time (`e::SLOPE`), release raw / 128 and [`super::env_amp`].
    pub const V2: usize = 2;
    pub const ALL: usize = 3;
    #[inline]
    pub fn decode(p: f32) -> usize {
        super::kinds::decode_in(p, COUNT, ALL - COUNT, TIMED, cfg!(feature = "us-env-frame"))
    }
    pub fn encode(id: usize) -> f32 {
        super::kinds::encode(id, COUNT)
    }
}

/// LFOs: 2 × [`l::LEN`].
pub const LFO: usize = ENV + ENV_SLOTS * e::LEN;
pub const LFO_SLOTS: usize = 2;
pub mod l {
    pub const SHAPE: usize = 0;
    pub const RATE: usize = 1;
    pub const DEPTH: usize = 2;
    pub const SYNC: usize = 3;
    pub const LEN: usize = 4;
}

/// Mod matrix: slots 1–12 at [`MOD`], 13–16 at [`MOD2`] (appended); use [`modslot`].
pub const MOD: usize = LFO + LFO_SLOTS * l::LEN;
pub const MOD_SLOTS: usize = 16;
/// Slots in the first block (chunk v1).
pub const MOD_SLOTS1: usize = 12;
pub mod m {
    pub const SOURCE: usize = 0;
    pub const TARGET: usize = 1;
    pub const AMOUNT: usize = 2;
    pub const OPERATION: usize = 3;
    pub const LEN: usize = 4;
}

/// Sample index group: one raw value per oscillator slot, the GSM sample number (1-based, 0 = none) or DLS wave index
/// of a sampler slot. Not normalized (0..65535, whole numbers) and structural (read from the chunk only).
pub const SMP: usize = MOD + MOD_SLOTS1 * m::LEN;
/// Sampler source choice (`choice(p, 3)` of `shape[SMP_SOURCE]`). The order keeps every earlier file valid: 0.0 was
/// GSM and 1.0 a GM.DLS wave.
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

/// Controllers, appended for the DAW so every earlier index stays valid.
pub const CTL: usize = SMP + OSC_SLOTS;
pub mod ctl {
    /// Mod wheel `0..1` (mod source 7); the live input `UInstrument::mod_wheel` writes it.
    pub const WHEEL: usize = super::CTL;
    /// Pitch bend `0..1`, centre `0.5`: every voice's pitch moves by `(bend − 0.5) · 2 · range` semitones.
    pub const BEND: usize = WHEEL + 1;
    /// Bend range `0..24` semitones (`law::bend_range`), default 2.
    pub const BEND_RANGE: usize = BEND + 1;
    pub const LEN: usize = 3;
}

/// Values appended after the controllers, so every earlier index stays valid.
pub const EXT: usize = CTL + ctl::LEN;
pub mod ext {
    /// Noise width `0..1`: 1 gives each channel its own noise (Tunefish's), 0 the same noise on both (WaveSabre's
    /// Slaughter), in between a mix that keeps the power of each channel.
    pub const NOISE_WIDTH: usize = super::EXT;
    pub const LEN: usize = 1;
}

/// V2 voice and filter routing (design D4a), appended after `ext`.
pub const V2G: usize = EXT + ext::LEN;
pub mod v2 {
    /// Choice off/on: V2 voice laws and output stage.
    pub const VOICE: usize = super::V2G;
    /// Raw 0 none, 1 oscillators, 2 full.
    pub const KEYSYNC: usize = VOICE + 1;
    /// Source `.v2m` format version 0–6, or 7 = a version 5 file from the 2004 engine (v2redux `eras::kkrieger2004`:
    /// version 5 behaviour with version 6 sine, FM and arctangent). An expanded choice ([`super::kinds`]) over
    /// `count(0, 6)`, so stored versions keep their values. Use [`super::era`].
    pub const ERA: usize = VOICE + 2;
    /// The 2004-engine version 5 (`ERA` id 7).
    pub const ERA_V5_2004: usize = 7;
    /// Filter slots 1–2: raw 0/1 serial, 2 parallel.
    pub const ROUTING: usize = VOICE + 3;
    /// Parallel balance, raw 64 = both full.
    pub const BALANCE: usize = VOICE + 4;
    pub const LEN: usize = 5;
}

/// The V2 era of a patch: `(format version, version 6 sine/FM/arctangent)`.
#[inline]
pub fn era(p: &[f32; COUNT]) -> (u32, bool) {
    match kinds::decode(p[v2::ERA], 7, 1, 6) {
        v2::ERA_V5_2004 => (5, true),
        v => (v as u32, v >= 6),
    }
}

/// Voice distortion (after the filters): V2 mode, input gain, param 1, param 2 (raw / 128).
pub const VDIST: usize = V2G + v2::LEN;
pub mod dist {
    pub const MODE: usize = 0;
    pub const GAIN: usize = 1;
    pub const P1: usize = 2;
    pub const P2: usize = 3;
    pub const LEN: usize = 4;
}

/// V2 envelope amplify of envelope `i` (V2-mode envelopes).
pub const ENV_AMP: usize = VDIST + dist::LEN;
pub const fn env_amp(i: usize) -> usize {
    ENV_AMP + i
}

/// LFO extras: 2 × [`lx::LEN`].
pub const LFO_EXT: usize = ENV_AMP + ENV_SLOTS;
pub mod lx {
    /// Choice: 0 unisynth laws, 1 V2 (shape, rate, depth = amplify, sync read raw / 128).
    pub const MODE: usize = 0;
    /// Start phase (raw / 128 of a turn) at key sync.
    pub const PHASE: usize = 1;
    /// Raw 0 positive, 1 negative, 2 bipolar.
    pub const POLARITY: usize = 2;
    /// Raw nonzero: one cycle, then hold (V2 envelope mode).
    pub const EG: usize = 3;
    pub const LEN: usize = 4;
}
pub const fn lfo_ext(i: usize, fld: usize) -> usize {
    LFO_EXT + i * lx::LEN + fld
}

/// Mod slots 13–16.
pub const MOD2: usize = LFO_EXT + LFO_SLOTS * lx::LEN;

/// Controllers 2–7 (raw / 128): breath, CC 3–6, volume. CC1 is [`ctl::WHEEL`].
pub const CC: usize = MOD2 + (MOD_SLOTS - MOD_SLOTS1) * m::LEN;
pub const CC_COUNT: usize = 6;
/// Index of controller `n` (2..=7).
pub const fn cc(n: usize) -> usize {
    CC + n - 2
}

/// Channel stage (design D5): the one-byte part at [`CH`], the two-byte part at [`CH2`]. V2 laws (raw / 128).
pub const CH: usize = CC + CC_COUNT;
pub mod ch {
    /// Choice off/on.
    pub const ON: usize = super::CH;
    pub const VOL: usize = ON + 1;
    pub const BOOST: usize = ON + 2;
    /// Raw 0 distortion then chorus, 1 chorus then distortion.
    pub const FXROUTE: usize = ON + 3;
    pub const SEND_RVB: usize = ON + 4;
    pub const SEND_DLY: usize = ON + 5;
    pub const SEND_AUXA: usize = ON + 6;
    pub const SEND_AUXB: usize = ON + 7;
    pub const RECV_AUXA: usize = ON + 8;
    pub const RECV_AUXB: usize = ON + 9;
    /// Choice off/on: the Ronan speech stage.
    pub const SPEECH: usize = ON + 10;
    /// The speech controllers 4 (text 0–62 / frame rate 63–127) and 5 (pitch), raw / 128: a change applies them.
    pub const SPEECH_CTL4: usize = ON + 11;
    pub const SPEECH_CTL5: usize = ON + 12;
    pub const LEN: usize = 13;
    /// Channel distortion: mode, gain, p1, p2.
    pub const DIST: usize = super::CH2;
    /// Chorus/flanger: amount, feedback, delay L, delay R, mod rate, mod depth, mod phase.
    pub const CHORUS: usize = DIST + 4;
    pub const CHORUS_LEN: usize = 7;
    /// Compressor: mode, couple, auto gain, look-ahead, threshold, ratio, attack, release, out gain.
    pub const COMP: usize = CHORUS + CHORUS_LEN;
    pub const COMP_LEN: usize = 9;
    pub const LEN2: usize = 4 + CHORUS_LEN + COMP_LEN;
}
/// First two-byte index of the chunk escape (`chunk::SHORT`); the channel stage's second part starts there.
pub const CH2: usize = 240;
const _: () = assert!(CH + ch::LEN == CH2, "the one-byte part ends at the two-byte indices");

/// Index of the sample index of oscillator slot `i`.
pub const fn smp_index(i: usize) -> usize {
    SMP + i
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

/// Whether parameter `i` is stored raw (not normalized): the sampler index of each oscillator slot.
pub const fn is_raw(i: usize) -> bool {
    i >= SMP && i < SMP + OSC_SLOTS
}

/// Whether parameter `i` shapes the instrument's memory and is read from the chunk only: voice capacity, each
/// oscillator type, each sampler source and index. `set_param` ignores these.
pub const fn is_structural(i: usize) -> bool {
    if i == v::CAPACITY || is_raw(i) {
        return true;
    }
    if i < OSC || i >= NOISE {
        return false;
    }
    let fld = (i - OSC) % o::LEN;
    fld == o::TYPE || fld == o::SHAPE + shape::SMP_SOURCE
}

/// Default values (a note on a default patch with one oscillator type set sounds: unity gain, centred, filters off,
/// envelope 1 at full sustain with an instant attack).
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
    d[ctl::BEND] = 0.5; // centre
    d[ctl::BEND_RANGE] = 2.0 / 24.0; // two semitones
    d[ext::NOISE_WIDTH] = 1.0; // independent channels
    // V2 part (design D4a): V2's initial patch values / 128, switches off
    d[v2::ERA] = 1.0; // version 6
    d[v2::BALANCE] = 0.5;
    d[VDIST + dist::GAIN] = 32.0 / 128.0;
    d[VDIST + dist::P2] = 0.5;
    d[cc(7)] = 127.0 / 128.0; // V2 starts every channel at volume 127
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
    d[NOISE + n::LP] = 1.0;
    i = 0;
    while i < FILT_SLOTS {
        d[filt(i, f::CUTOFF)] = 1.0;
        i += 1;
    }
    i = 0;
    while i < ENV_SLOTS {
        d[env_amp(i)] = 1.0;
        d[env(i, e::DECAY)] = 0.3;
        d[env(i, e::SUSTAIN)] = 1.0;
        d[env(i, e::RELEASE)] = 0.1;
        d[env(i, e::SLOPE)] = 0.5;
        i += 1;
    }
    i = 0;
    while i < LFO_SLOTS {
        d[lfo(i, l::RATE)] = 0.6;
        d[lfo(i, l::DEPTH)] = 1.0;
        d[lfo(i, l::SYNC)] = 1.0;
        i += 1;
    }
    d
}

#[cfg(any(test, feature = "std"))]
mod host;
#[cfg(any(test, feature = "std"))]
pub use host::{law_of, name, Law};
#[cfg(any(test, feature = "std"))]
pub mod rename;

#[cfg(test)]
mod tests;
