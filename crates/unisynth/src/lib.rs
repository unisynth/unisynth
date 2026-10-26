//! Unified synth: one slot-based instrument (kind 64) and one merged effect set (kinds 65–73), built from the
//! WaveSabre and Tunefish algorithms. `no_std`, f32, no heap; plays through `synth_core::mix`.
//! Design: `openspec/changes/archive/2026-10-26-add-unified-synth/design.md`; docs: `docs/synth/unisynth.md`.
//! MIT; WaveSabre-derived code keeps its notice (`LICENSE-WaveSabre` in this crate).
#![cfg_attr(not(test), no_std)]

#[cfg(feature = "std")]
extern crate std;

pub mod chunk;
pub mod device;
pub mod env;
pub mod flt;
pub mod fm;
pub mod fx;
pub mod instrument;
pub mod law;
pub mod lfo;
pub mod modmx;
#[cfg(feature = "std")]
pub mod needs;
pub mod noise;
pub mod osc;
pub mod module;
pub mod params;
pub mod ramp;
pub mod smp;
pub mod rng;
pub mod tables;
pub mod voice;
#[cfg(feature = "us-osc-wt")]
pub mod wt;

#[cfg(all(test, feature = "us-osc-pm", feature = "us-env-timed", feature = "us-env-rate"))]
mod testing;

/// Most unison layers of a wavetable slot.
#[cfg(feature = "us-osc-wt")]
pub(crate) const fn wt_layers() -> u32 {
    wt::MAX_LAYERS as u32
}
#[cfg(not(feature = "us-osc-wt"))]
pub(crate) const fn wt_layers() -> u32 {
    1
}

pub use device::UDevice;
pub use instrument::UInstrument;

/// `synth-mix` device kind ids (0–12 and 32–39 belong to other engines).
pub mod kind {
    pub const INSTRUMENT: u8 = 64;
    pub const DISTORTION: u8 = 65;
    pub const CRUSHER: u8 = 66;
    pub const EQ: u8 = 67;
    pub const DELAY: u8 = 68;
    pub const CHORUS: u8 = 69;
    pub const FLANGER: u8 = 70;
    pub const REVERB: u8 = 71;
    pub const COMPRESSOR: u8 = 72;
    pub const FORMANT: u8 = 73;
}

#[cfg(test)]
mod tests {
    use super::kind;

    #[test]
    fn kinds_follow_the_design_and_avoid_the_ranges_of_other_engines() {
        let all = [
            kind::INSTRUMENT,
            kind::DISTORTION,
            kind::CRUSHER,
            kind::EQ,
            kind::DELAY,
            kind::CHORUS,
            kind::FLANGER,
            kind::REVERB,
            kind::COMPRESSOR,
            kind::FORMANT,
        ];
        assert_eq!(all, [64, 65, 66, 67, 68, 69, 70, 71, 72, 73]);
        assert!(all.iter().all(|&k| k > 39));
    }
}
