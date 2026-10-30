//! Unified synth: one slot-based instrument (kind 64) and one merged effect set (kinds 65–73), built from the
//! WaveSabre and Tunefish algorithms. `no_std`, f32, no heap; plays through `synth_core::mix`.
//! Design: `openspec/changes/archive/2026-10-26-add-unified-synth/design.md`; docs: `docs/synth/unisynth.md`.
//! MIT; WaveSabre-derived code keeps its notice (`LICENSE-WaveSabre` in this crate).
#![cfg_attr(not(test), no_std)]

#[cfg(feature = "std")]
extern crate std;

pub mod chan;
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
#[cfg(feature = "probe")]
pub mod probe;
pub mod ramp;
pub mod smp;
#[cfg(feature = "us-speech")]
pub mod speech;
pub mod rng;
pub mod tables;
pub mod v2;
pub mod v2fx;
pub mod voice;
#[cfg(feature = "us-osc-wt")]
pub mod wt;

#[cfg(all(test, feature = "us-osc-pm", feature = "us-env-seconds", feature = "us-env-rate"))]
// the song-level tests that use it are gated on us-matrix or us-voice-glide + us-voice-unison
#[cfg_attr(not(any(feature = "us-matrix", all(feature = "us-voice-glide", feature = "us-voice-unison"))), allow(dead_code))]
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

/// A song with an automation lane loads only in a build with `us-automation`; one without lanes loads in any build.
#[cfg(all(test, feature = "us-osc-pm", feature = "us-env-seconds", feature = "us-env-rate"))]
mod automation_gating {
    use crate::device::UDevice;
    use crate::rng::Xorshift;
    use crate::testing::{song, Track};
    use synth_core::mix::{Env, LoadError, Player, Song};
    use synth_core::pool::Pool;

    fn load(autos: Vec<(i32, i32, Vec<(i32, u8)>)>) -> Result<(), LoadError> {
        let chunk = crate::chunk::encode(&crate::params::DEFAULTS, &[]);
        let mut t = Track::new(&[0], 0);
        t.autos = autos;
        let b = song(44_100, 1.0, &[(crate::kind::INSTRUMENT, chunk)], &[vec![(0, 60, Some(100))]], &[t]);
        let s = Song::parse(&b).unwrap();
        let env = Env::for_song(&s, None);
        let mut mem = vec![0u8; Player::<UDevice, Xorshift>::required_pool(&s, &env, 100)?];
        let mut pool = Pool::new(&mut mem);
        Player::<UDevice, Xorshift>::new(s, &env, &mut pool, Xorshift::new(1), 100).map(|_| ())
    }

    #[test]
    fn a_lane_needs_the_automation_feature() {
        assert_eq!(load(vec![]), Ok(()));
        let r = load(vec![(0, crate::params::v::MASTER as i32, vec![(0, 10), (1000, 200)])]);
        if cfg!(feature = "us-automation") {
            assert_eq!(r, Ok(()));
        } else {
            assert_eq!(r, Err(LoadError::UnsupportedModule(crate::module::AUTOMATION)));
        }
    }
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
