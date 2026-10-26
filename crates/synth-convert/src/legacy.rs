//! Legacy instrument chunks: the device kinds, their parameter counts and constructor defaults, and the decoder that
//! reads a chunk the way the legacy `set_params` does (`(size − 4)/4` little-endian f32 values after the sample for
//! Specimen, none for Thunder).

use tfsynth::device::kind as t;
use wavesabre::kind as w;

/// Instrument kinds this module converts.
pub fn is_wavesabre_instrument(kind: u8) -> bool {
    matches!(kind, w::FALCON | w::SLAUGHTER | w::THUNDER | w::ADULTERY | w::SPECIMEN)
}

pub fn is_tunefish_instrument(kind: u8) -> bool {
    kind == t::INSTRUMENT
}

pub fn name(kind: u8) -> &'static str {
    match kind {
        w::FALCON => "Falcon",
        w::SLAUGHTER => "Slaughter",
        w::THUNDER => "Thunder",
        w::ADULTERY => "Adultery",
        w::SPECIMEN => "Specimen",
        t::INSTRUMENT => "Tunefish",
        _ => "instrument",
    }
}

/// Number of parameters of a WaveSabre instrument.
pub fn param_count(kind: u8) -> usize {
    match kind {
        w::FALCON => 32,
        w::SLAUGHTER => 42,
        w::THUNDER => 0,
        w::SPECIMEN => 26,
        w::ADULTERY => 28,
        t::INSTRUMENT => tfsynth::params::COUNT,
        _ => 0,
    }
}

/// Legacy parameter indices the capacity analysis reads.
pub struct Roles {
    pub unison: Option<usize>,
    pub mode: Option<usize>,
    /// Release of the envelope that ends the voice.
    pub release: Option<usize>,
}

pub fn roles(kind: u8) -> Roles {
    let (unison, mode, release) = match kind {
        w::FALCON => (Some(18), Some(30), Some(16)),
        w::SLAUGHTER => (Some(29), Some(40), Some(23)),
        w::SPECIMEN => (Some(20), Some(24), Some(3)),
        w::ADULTERY => (Some(22), Some(26), Some(4)),
        _ => (None, None, None),
    };
    Roles { unison, mode, release }
}

/// Legacy envelope defaults as 0..1 parameters: attack 1 ms, decay 5 ms, release 1.5 ms.
const D5: f32 = 0.028_284_27; // sqrt((5 − 1)/5000)
const R15: f32 = 0.01; // sqrt((1.5 − 1)/5000)

/// Parameter defaults of the legacy constructor (what a short chunk leaves).
pub fn defaults(kind: u8) -> Vec<f32> {
    let mut d = vec![0.0; param_count(kind)];
    match kind {
        w::FALCON => {
            d[2] = 0.5;
            d[11] = 0.5;
            d[7] = 1.0;
            d[13..17].copy_from_slice(&[0.0, D5, 0.75, R15]);
            d[17] = 0.8;
            d[20] = 0.5;
            d[24..28].copy_from_slice(&[0.0, D5, 0.5, R15]);
            d[28] = 0.5;
            d[29] = 0.5;
        }
        w::SLAUGHTER => {
            d[1] = 0.5;
            d[2] = 1.0;
            d[6] = 0.5;
            d[11] = 0.5;
            d[16] = 0.0;
            d[17] = 1.0;
            d[19] = 0.5;
            d[20..24].copy_from_slice(&[0.0, D5, 0.5, R15]);
            d[24..28].copy_from_slice(&[0.0, D5, 1.0, R15]);
            d[28] = 0.5;
            d[31] = 0.5;
            d[35..39].copy_from_slice(&[0.0, D5, 0.5, R15]);
            d[39] = 0.5;
        }
        w::SPECIMEN | w::ADULTERY => {
            let b = if kind == w::ADULTERY { 1 } else { 0 };
            d[b..b + 4].copy_from_slice(&[0.0, 0.0, 1.0, 0.0]); // amp: 1 ms, 1 ms, sustain 1, 1 ms
            let (loop_mode, interp, coarse, fcut, fmod, mod0, pan, master) = if kind == w::ADULTERY {
                (6, 11, 12, 15, 17, 18, 24, 25)
            } else {
                (5, 9, 10, 13, 15, 16, 22, 23)
            };
            d[loop_mode] = if kind == w::ADULTERY { 0.5 } else { 0.0 }; // Adultery repeats, Specimen does not
            d[interp] = 1.0;
            d[coarse] = 0.5;
            d[coarse + 1] = 0.5;
            d[fcut] = 1.0;
            d[fmod] = 0.5;
            d[mod0..mod0 + 4].copy_from_slice(&[0.0, D5, 1.0, R15]);
            d[pan] = 0.5;
            d[master] = 0.5;
            // loop length: the whole sample (Specimen's bounds are manual, Adultery's come from the sample)
            d[if kind == w::ADULTERY { 9 } else { 7 }] = 1.0;
        }
        _ => {}
    }
    d
}

/// A decoded legacy instrument.
#[derive(Clone, Debug)]
pub struct Legacy {
    pub kind: u8,
    /// One value per legacy parameter (defaults where the chunk is short).
    pub params: Vec<f32>,
    /// The GSM sample (Specimen, Thunder): the chunk's sample section as a unified sample entry.
    pub sample: Option<Vec<u8>>,
    /// Decoded samples of that sample (Thunder's voice length).
    pub sample_frames: usize,
}

/// Decodes a chunk of `kind`. A non-finite value reads as 0, what upstream's `(int)` decodes make of NaN.
pub fn decode(kind: u8, chunk: &[u8]) -> Result<Legacy, String> {
    let mut params = defaults(kind);
    let (mut sample, mut frames) = (None, 0);
    let mut rest = chunk;
    if matches!(kind, w::SPECIMEN | w::THUNDER) {
        match gsm::GsmChunk::parse(chunk).map_err(|e| format!("bad sample: {e:?}"))? {
            Some(g) => {
                let n = chunk.len() - g.rest.len();
                sample = Some(chunk[..n].to_vec());
                frames = g.samples;
                rest = g.rest;
            }
            None => rest = &[],
        }
    }
    if kind == w::THUNDER {
        return Ok(Legacy { kind, params, sample, sample_frames: frames });
    }
    let n = rest.len().saturating_sub(4) / 4;
    for (i, b) in rest[..n * 4].chunks_exact(4).enumerate().take(params.len()) {
        let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        params[i] = if v.is_finite() { v } else { 0.0 };
    }
    Ok(Legacy { kind, params, sample, sample_frames: frames })
}
