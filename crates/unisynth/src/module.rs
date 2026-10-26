//! Module ids carried by `LoadError::UnsupportedModule`: what a patch asked for that this build does not contain.

pub const OSC_PM: u8 = 1;
pub const OSC_BLIT: u8 = 2;
pub const OSC_WAVETABLE: u8 = 3;
pub const OSC_SAMPLER: u8 = 4;
pub const SAMPLE_GSM: u8 = 5;
pub const SAMPLE_DLS: u8 = 6;
pub const NOISE: u8 = 7;
pub const FILTER_SVF: u8 = 8;
pub const FILTER_LADDER: u8 = 9;
pub const FILTER_RBJ: u8 = 10;
pub const FILTER_NOTCH: u8 = 11;
pub const ENV_TIMED: u8 = 12;
pub const ENV_RATE: u8 = 13;
// effect (kind, mode) pairs
pub const FX_DISTORTION_SHAPER: u8 = 16;
pub const FX_DISTORTION_POWER: u8 = 17;
pub const FX_CRUSHER: u8 = 18;
pub const FX_EQ_PARAMETRIC: u8 = 19;
pub const FX_EQ_3BAND: u8 = 20;
pub const FX_DELAY_TEMPO: u8 = 21;
pub const FX_DELAY_MS: u8 = 22;
pub const FX_CHORUS: u8 = 23;
pub const FX_FLANGER_PHASER: u8 = 24;
pub const FX_FLANGER_RING: u8 = 25;
pub const FX_REVERB_FREEVERB: u8 = 26;
pub const FX_REVERB_COMPACT: u8 = 27;
pub const FX_REVERB_ROOMS: u8 = 28;
pub const FX_COMPRESSOR: u8 = 29;
pub const FX_FORMANT: u8 = 30;

/// Readable name (host side: messages, converter).
#[cfg(any(test, feature = "std"))]
pub fn name(id: u8) -> &'static str {
    match id {
        OSC_PM => "phase-mod oscillator",
        OSC_BLIT => "band-limited saw/pulse oscillator",
        OSC_WAVETABLE => "wavetable oscillator",
        OSC_SAMPLER => "sampler oscillator",
        SAMPLE_GSM => "GSM sample source",
        SAMPLE_DLS => "GM.DLS sample source",
        NOISE => "noise source",
        FILTER_SVF => "state-variable filter",
        FILTER_LADDER => "ladder filter",
        FILTER_RBJ => "RBJ filter",
        FILTER_NOTCH => "resonator notch filter",
        ENV_TIMED => "timed envelope",
        ENV_RATE => "rate envelope",
        FX_DISTORTION_SHAPER => "distortion, shaper mode",
        FX_DISTORTION_POWER => "distortion, power mode",
        FX_CRUSHER => "crusher",
        FX_EQ_PARAMETRIC => "EQ, parametric mode",
        FX_EQ_3BAND => "EQ, 3band mode",
        FX_DELAY_TEMPO => "delay, tempo mode",
        FX_DELAY_MS => "delay, ms mode",
        FX_CHORUS => "chorus",
        FX_FLANGER_PHASER => "flanger, phaser mode",
        FX_FLANGER_RING => "flanger, ring mode",
        FX_REVERB_FREEVERB => "reverb, freeverb mode",
        FX_REVERB_COMPACT => "reverb, compact mode",
        FX_REVERB_ROOMS => "reverb, rooms mode",
        FX_COMPRESSOR => "compressor",
        FX_FORMANT => "formant",
        _ => "unknown module",
    }
}
