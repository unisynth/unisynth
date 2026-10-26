//! The instrument parameter layout (kind 64): 184 values in groups, the defaults and (host-side) the
//! names and laws per index. Design D3. A chunk stores only the values that differ from [`DEFAULTS`].
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
//! ```
//!
//! Everything except the sample indices is normalized 0..1.

/// Number of values in a patch.
pub const COUNT: usize = EXT + ext::LEN;

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

/// Oscillator types (`choice(p, 5)` of [`o::TYPE`]).
pub mod osc_type {
    pub const OFF: usize = 0;
    pub const PM: usize = 1;
    pub const BLIT: usize = 2;
    pub const WAVETABLE: usize = 3;
    pub const SAMPLER: usize = 4;
    pub const COUNT: usize = 5;
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
/// Filter types (`choice(p, 9)` of [`f::TYPE`]).
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
}

/// Envelopes: 4 × [`e::LEN`].
pub const ENV: usize = FILT + FILT_SLOTS * f::LEN;
pub const ENV_SLOTS: usize = 4;
pub mod e {
    pub const MODE: usize = 0;
    pub const ATTACK: usize = 1;
    pub const DECAY: usize = 2;
    pub const SUSTAIN: usize = 3;
    pub const RELEASE: usize = 4;
    pub const SLOPE: usize = 5;
    pub const LEN: usize = 6;
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

/// Mod matrix: 12 × [`m::LEN`].
pub const MOD: usize = LFO + LFO_SLOTS * l::LEN;
pub const MOD_SLOTS: usize = 12;
pub mod m {
    pub const SOURCE: usize = 0;
    pub const TARGET: usize = 1;
    pub const AMOUNT: usize = 2;
    pub const OPERATION: usize = 3;
    pub const LEN: usize = 4;
}

/// Sample index group: one raw value per oscillator slot, the GSM sample number (1-based, 0 = none) or DLS wave index
/// of a sampler slot. Not normalized (0..65535, whole numbers) and structural (read from the chunk only).
pub const SMP: usize = MOD + MOD_SLOTS * m::LEN;
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
/// Index of field `fld` of mod slot `i`.
pub const fn modslot(i: usize, fld: usize) -> usize {
    MOD + i * m::LEN + fld
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

#[cfg(test)]
mod tests;
