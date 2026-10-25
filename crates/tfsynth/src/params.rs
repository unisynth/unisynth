//! The 112-value instrument patch: indices in patch/song order, the public parameter names (for preset
//! compatibility) and the normalized → physical mappings of the behavioural spec §5. Values are `p ∈ [0, 1]`.

/// Values per instrument patch.
pub const COUNT: usize = 112;

pub const GAIN: usize = 0;
pub const GEN_BANDWIDTH: usize = 1;
pub const GEN_HARMONICS: usize = 2;
pub const GEN_DAMP: usize = 3;
pub const GEN_MOD: usize = 4;
pub const GEN_VOLUME: usize = 5;
pub const GEN_PAN: usize = 6;
pub const GEN_SLOP: usize = 7;
pub const GEN_OCTAVE: usize = 8;
pub const GEN_GLIDE: usize = 9;
pub const GEN_DETUNE: usize = 10;
pub const GEN_FREQ: usize = 11;
pub const GEN_POLY: usize = 12;
pub const GEN_DRIVE: usize = 13;
pub const GEN_UNISON: usize = 14;
pub const GEN_SPREAD: usize = 15;
pub const GEN_SCALE: usize = 16;
pub const NOISE_AMOUNT: usize = 17;
pub const NOISE_FREQ: usize = 18;
pub const NOISE_BW: usize = 19;
pub const LP_ON: usize = 20;
pub const LP_CUT: usize = 21;
pub const LP_RES: usize = 22;
pub const HP_ON: usize = 23;
pub const HP_CUT: usize = 24;
pub const HP_RES: usize = 25;
/// Attack, decay, sustain, release, slope of envelope 1 (envelope 2 at `ENV2`).
pub const ENV1: usize = 26;
pub const ENV2: usize = 31;
/// Rate, depth, shape, sync of LFO 1 (LFO 2 at `LFO2`).
pub const LFO1: usize = 36;
pub const LFO2: usize = 40;
/// Ten (source, amount, target) triplets; only the first 8 are evaluated.
pub const MM_FIRST: usize = 44;
pub const EFFECT_FIRST: usize = 74;
pub const DIST_AMOUNT: usize = 84;
pub const CHORUS_RATE: usize = 85;
pub const CHORUS_DEPTH: usize = 86;
pub const DELAY_LEFT: usize = 87;
pub const DELAY_RIGHT: usize = 88;
pub const DELAY_DECAY: usize = 89;
pub const REV_ROOM: usize = 90;
pub const REV_DAMP: usize = 91;
pub const REV_WET: usize = 92;
pub const REV_WIDTH: usize = 93;
pub const FLANGER_LFO: usize = 94;
pub const FLANGER_FREQ: usize = 95;
pub const FLANGER_AMP: usize = 96;
pub const FLANGER_WET: usize = 97;
pub const CHORUS_GAIN: usize = 98;
pub const FORMANT_TYPE: usize = 99;
pub const FORMANT_WET: usize = 100;
pub const EQ_LOW: usize = 101;
pub const EQ_MID: usize = 102;
pub const EQ_HIGH: usize = 103;
pub const BEND_UP: usize = 104;
pub const BEND_DOWN: usize = 105;
pub const BP_ON: usize = 106;
pub const BP_CUT: usize = 107;
pub const BP_Q: usize = 108;
pub const NT_ON: usize = 109;
pub const NT_CUT: usize = 110;
pub const NT_Q: usize = 111;

/// Public parameter names, in patch order (index 10 really is spelled `GetDetn`).
pub const NAMES: [&str; COUNT] = [
    "Gain", "GenBandw", "GenHarmn", "GenDamp", "GenMod", "GenVol", "GenPan", "GenSlop", "GenOct", "GenGlid",
    "GetDetn", "GenFreq", "GenPoly", "GenDriv", "GenUnis", "GenSprd", "GenScale", "NoiseOn", "NoiseFreq", "NoiseBw",
    "FltLPOn", "FltLPCut", "FltLPRes", "FltHPOn", "FltHPCut", "FltHPRes",
    "Env1Att", "Env1Dec", "Env1Sus", "Env1Rel", "Env1Slp", "Env2Att", "Env2Dec", "Env2Sus", "Env2Rel", "Env2Slp",
    "Lfo1Rate", "Lfo1Dep", "Lfo1Shp", "Lfo1Sync", "Lfo2Rate", "Lfo2Dep", "Lfo2Shp", "Lfo2Sync",
    "Mm1Source", "Mm1Mod", "Mm1Target", "Mm2Source", "Mm2Mod", "Mm2Target", "Mm3Source", "Mm3Mod", "Mm3Target",
    "Mm4Source", "Mm4Mod", "Mm4Target", "Mm5Source", "Mm5Mod", "Mm5Target", "Mm6Source", "Mm6Mod", "Mm6Target",
    "Mm7Source", "Mm7Mod", "Mm7Target", "Mm8Source", "Mm8Mod", "Mm8Target", "Mm9Source", "Mm9Mod", "Mm9Target",
    "Mm10Sourc", "Mm10Mod", "Mm10Targe",
    "Effect1", "Effect2", "Effect3", "Effect4", "Effect5", "Effect6", "Effect7", "Effect8", "Effect9", "Effect10",
    "DistAmnt", "ChrsRate", "ChrsDep", "DelayL", "DelayR", "DelayDc", "RevRoom", "RevDamp", "RevWet", "RevWidth",
    "FlangLfo", "FlangFreq", "FlangAmp", "FlangWet", "ChrsGain", "FrmtType", "FrmtWet", "EqLow", "EqMid", "EqHigh",
    "OscPWup", "OscPWdw", "FltBPOn", "FltBPCut", "FltBPQ", "FltNTOn", "FltNTCut", "FltNTQ",
];

/// `round(x) = floor(x + 0.5)` (the engine's rounding for enumerated parameters).
#[inline]
pub fn round(x: f32) -> f32 {
    libm::floorf(x + 0.5)
}

/// `.tfm` stores `trunc(p·100)`; the loader maps back with `byte / 100`.
#[inline]
pub fn from_byte(b: u8) -> f32 {
    b as f32 / 100.0
}

/// Voice output gain curve: `(2g)²` up to 0.5, then `1 + 20(g − 0.5)` (0..11).
#[inline]
pub fn gain_curve(g: f32) -> f32 {
    if g <= 0.5 {
        let v = g * 2.0;
        v * v
    } else {
        (g - 0.5) * 20.0 + 1.0
    }
}

/// Octave multiplier `2^(4 − round(8p))`.
#[inline]
pub fn octave_factor(p: f32) -> f32 {
    const F: [f32; 9] = [16.0, 8.0, 4.0, 2.0, 1.0, 0.5, 0.25, 0.125, 0.0625];
    F[(round(p * 8.0) as usize).min(8)]
}

/// Unison layers `round(9p) + 1` (1..10).
#[inline]
pub fn unison(p: f32) -> usize {
    (round(p * 9.0) as usize).min(9) + 1
}

/// Polyphony `trunc(15p + 1)` (1..16).
#[inline]
pub fn polyphony(p: f32) -> usize {
    ((p * 15.0 + 1.0) as usize).clamp(1, 16)
}

/// Partial count `1 + min(trunc(64η), 64)` (1..65).
#[inline]
pub fn harmonics(eta: f32) -> usize {
    1 + ((eta * 64.0) as usize).min(64)
}

/// Mod-slot amount: `2p` up to 0.5, then `1 + 9(2p − 1)²` (1..10).
#[inline]
pub fn mod_amount(p: f32) -> f32 {
    if p <= 0.5 {
        p * 2.0
    } else {
        let m = (p - 0.5) * 2.0;
        1.0 + m * m * 9.0
    }
}

/// Mod source `round(14p)`: 0 none, 1 LFO1, 2 LFO2, 3 ADSR1, 4 ADSR2, 5 mod wheel, 6.. reserved.
#[inline]
pub fn mod_source(p: f32) -> u8 {
    round(p * 14.0) as u8
}

/// Mod target `round(39p)` (table in `modmx`).
#[inline]
pub fn mod_target(p: f32) -> u8 {
    round(p * 39.0) as u8
}

/// Effect slot type `round(10p)`: 0 none, 1..7 kinds, 8..10 none.
#[inline]
pub fn effect_kind(p: f32) -> u8 {
    round(p * 10.0) as u8
}

/// LFO shape `round(4p)`.
#[inline]
pub fn lfo_shape(p: f32) -> u8 {
    round(p * 4.0) as u8
}

/// Pitch-bend range in semitones, `trunc(12p + 1)` (1..13).
#[inline]
pub fn bend_semitones(p: f32) -> u32 {
    (p * 12.0 + 1.0) as u32
}

/// EQ band gain: `2p` up to 0.5, then `1 + 10(2p − 1)²`.
#[inline]
pub fn eq_gain(p: f32) -> f32 {
    if p <= 0.5 {
        p * 2.0
    } else {
        let m = (p - 0.5) * 2.0;
        libm::powf(m, 2.0) * 10.0 + 1.0
    }
}
