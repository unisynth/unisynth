//! `.tfm` songs and loose Tunefish patches as unified `synth-mix` songs (design D10).
//!
//! One track per non-empty instrument holds the unified instrument and its insert effects in slot order; a master track
//! applies the `.tfm` player's output scaling: each instrument is mixed at ×11 (the gain curve at 1.0), the bus at
//! `2000 · volume 0.1`, and the unified player's 16-bit output is the f32 mix times 32 768.

use crate::model::{Dev, Ev, SongM, Trk};
use crate::{tf, ConvError, Converted};
use tfsynth::params::COUNT;
use tfsynth::tfm::Tfm;

const RATE: f32 = 44_100.0;
const BLOCK: u32 = 512;
/// `.tfm` bus gain: `1000 · 2 · volume 0.1`, as a share of the 16-bit full scale.
const MASTER: f32 = 200.0 / 32_768.0;
/// Each instrument's gain into the bus.
const INSTRUMENT: f32 = 11.0;
/// The most blocks a song may span so its frames fit the format's `i32`.
const MAX_BLOCKS: u64 = (i32::MAX as u64) / BLOCK as u64;

/// One instrument for the song builder: its patch and its events as `(frame, note, Some(velocity) | None)`.
struct Part<'a> {
    patch: &'a [f32],
    events: Vec<(u64, u8, Option<u8>)>,
}

/// Block start frames of the legacy player's firing: the player advances an `f32` clock by one block per call and fires
/// the events with `t < clock + block`; reproduced step by step so the rounding matches.
struct Clock {
    time: f32,
    block: u64,
}

impl Clock {
    fn new() -> Clock {
        Clock { time: 0.0, block: 0 }
    }

    /// The block (as its first frame) that fires an event at `t` seconds; times must not decrease.
    fn frame(&mut self, t: f32) -> Result<u64, ConvError> {
        while t >= self.time + BLOCK as f32 / RATE {
            self.time += BLOCK as f32 / RATE;
            self.block += 1;
            if self.block > MAX_BLOCKS {
                return Err(ConvError::Input("the song is longer than the song format can hold".into()));
            }
        }
        Ok(self.block * BLOCK as u64)
    }
}

fn events_to_lane(events: &[(u64, u8, Option<u8>)]) -> Vec<Ev> {
    let mut prev = 0u64;
    events
        .iter()
        .map(|&(f, n, v)| {
            let d = (f - prev) as i32;
            prev = f;
            (d, n, v)
        })
        .collect()
}

fn song(tempo: i32, parts: &[Part], length: f64, warnings: &mut Vec<String>) -> SongM {
    let mut devices = vec![];
    let mut lanes = vec![];
    let mut tracks = vec![];
    for (n, part) in parts.iter().enumerate() {
        let p = part.patch;
        for w in tf::warnings(p) {
            warnings.push(format!("instrument {}: {w}", n + 1));
        }
        let mut list = vec![devices.len()];
        devices.push(tf::instrument_device(p, tf::capacity(p)));
        for (_, d) in tf::insert_chain(p) {
            list.push(devices.len());
            devices.push(d);
        }
        lanes.push(events_to_lane(&part.events));
        tracks.push(Trk { volume: 1.0, receives: vec![], devices: list, lane: n, autos: vec![] });
    }
    // the master: every instrument at ×11, then the bus gain
    lanes.push(vec![]);
    let receives = (0..parts.len()).map(|i| (i, 0, INSTRUMENT)).collect();
    tracks.push(Trk { volume: MASTER, receives, devices: vec![], lane: parts.len(), autos: vec![] });
    SongM { tempo, rate: 44_100, length, devices: devices.into_iter().collect::<Vec<Dev>>(), lanes, tracks }
}

/// Converts a `.tfm` song. `tail` seconds are added after the last event for the release tails.
pub fn convert_tfm(blob: &[u8], tail: f64) -> Result<Converted, ConvError> {
    let t = Tfm::parse(blob).map_err(|e| ConvError::Input(format!("not a valid .tfm song: {e:?}")))?;
    if t.tempo == 0 {
        return Err(ConvError::Input("the .tfm tempo is 0".into()));
    }
    let secs_per_row = 1.0 / ((t.tempo as u32 * 4) as f32 / 60.0);
    let patches: Vec<[f32; COUNT]> = (0..t.instruments()).map(|i| t.patch(i)).collect();
    let mut parts = vec![];
    let mut last = 0.0f32;
    for i in 0..t.instruments() {
        if t.event_count(i) == 0 {
            continue;
        }
        let mut clock = Clock::new();
        let mut events = vec![];
        for e in t.events(i) {
            let at = e.row as f32 * secs_per_row;
            let frame = clock.frame(at)?;
            last = last.max(at);
            // note 0 is ignored, velocity 0 is a note-off
            if e.note != 0 {
                events.push((frame, e.note & 0x7f, (e.velocity != 0).then_some(e.velocity)));
            }
        }
        parts.push(Part { patch: &patches[i], events });
    }
    let mut warnings = vec![];
    let song = song(t.tempo as i32, &parts, last as f64 + tail, &mut warnings);
    Ok(Converted { blob: song.write(), warnings })
}

/// A loose Tunefish patch (112 values) played by `phrase`: `(block, note, velocity)` with velocity 0 = note-off, the
/// phrase shape of the `tfsynth` synthetic patches. Events land at the start of their block.
pub fn convert_patch(patch: &[f32], phrase: &[(u32, u8, u8)], tempo: i32, length: f64) -> Result<Converted, ConvError> {
    if patch.len() != COUNT {
        return Err(ConvError::Input(format!("a Tunefish patch has {COUNT} values, got {}", patch.len())));
    }
    if phrase.windows(2).any(|w| w[1].0 < w[0].0) {
        return Err(ConvError::Input("the phrase is not in block order".into()));
    }
    if phrase.iter().any(|e| e.0 as u64 > MAX_BLOCKS) {
        return Err(ConvError::Input("the phrase is longer than the song format can hold".into()));
    }
    let events = phrase
        .iter()
        .filter(|e| e.1 != 0)
        .map(|&(b, n, v)| (b as u64 * BLOCK as u64, n & 0x7f, (v != 0).then_some(v)))
        .collect();
    let mut warnings = vec![];
    let song = song(tempo, &[Part { patch, events }], length, &mut warnings);
    Ok(Converted { blob: song.write(), warnings })
}

// ---- .tf4m: the 2013 Tunefish 4 of Enigma Studio 4 (Brain Control's "Turtles all the way down") ----

/// Parameters per instrument in a `.tf4m`: the final layout minus the pitch wheel and band-pass/notch filters
/// (8 values at the end), plus six generator values the 2013 engine declares but never reads.
pub const TF4M_COUNT: usize = 110;
/// 2013 mod targets (`round(16p)`) → final ids: NONE BW DAMP NUMH VOL FREQ PAN DET SPREAD DRIVE NOISE LPCUT LPRES HPCUT
/// HPRES A1DEC A2DEC. PAN is declared but never applied in 2013, so it maps to none.
const TF4M_TARGETS: [u8; 17] = [0, 1, 2, 3, 5, 6, 0, 8, 9, 10, 11, 12, 13, 14, 15, 20, 21];

/// A `.tf4m` (2013 engine) as a final-layout `.tfm`, with the warnings for what only matches approximately.
/// Translated: parameter indices (2013 index − 6 from the noise on; the unused profile, modulation type, random,
/// sub-oscillator, shape and test values dropped; wheel and BP/notch off), mod sources (`round(4p)` → `/14`), targets
/// (`round(16p)` → `/39`), amounts (2013 `((p − ½)·20)²` → the final law) and reverb width (the 2013 reverb ignores it:
/// width 1). Values stay on the format's 1/100 grid. The song section is the same in both formats.
pub fn tf4m_to_tfm(b: &[u8]) -> Result<(Vec<u8>, Vec<String>), ConvError> {
    let bad = |m: &str| ConvError::Input(format!("not a valid .tf4m song: {m}"));
    let u16_at = |o: usize| b.get(o..o + 2).map(|x| u16::from_le_bytes([x[0], x[1]]) as usize).ok_or_else(|| bad("truncated"));
    let count = u16_at(0)?;
    let o = 4 + 2 * count;
    if b.get(o..o + 4) != Some(b"INST") {
        return Err(bad("no INST section"));
    }
    let song = o + 4 + count * TF4M_COUNT;
    if b.get(song..song + 4) != Some(b"SONG") {
        return Err(bad("no SONG section after 110 values per instrument (a final-format .tfm?)"));
    }
    use tfsynth::params as tp;
    let mut out = b[..o + 4].to_vec();
    let mut warnings = vec![];
    let byte = |v: f32| (v * 100.0).round().clamp(0.0, 255.0) as u8;
    for i in 0..count {
        let old = &b[o + 4 + i * TF4M_COUNT..o + 4 + (i + 1) * TF4M_COUNT];
        let mut n = [0u8; COUNT];
        n[0] = old[0];
        n[1..=4].copy_from_slice(&old[2..=5]);
        n[5..=16].copy_from_slice(&old[7..=18]);
        n[17..=103].copy_from_slice(&old[23..=109]);
        for e in 0..10 {
            let k = tp::MM_FIRST + 3 * e;
            let (src, amt, tgt) = (old[k + 6] as f32 / 100.0, old[k + 7] as f32 / 100.0, old[k + 8] as f32 / 100.0);
            let src = (src * 4.0).round() as usize;
            if src == 0 {
                continue;
            }
            if e >= tfsynth::modmx::LIVE {
                warnings.push(format!("instrument {}: mod slot {} is ignored by the final engine", i + 1, e + 1));
            }
            let m = ((amt - 0.5) * 20.0).powi(2);
            let amount = if m <= 1.0 { m / 2.0 } else { 0.5 + ((m - 1.0) / 9.0).sqrt() / 2.0 };
            let tgt = TF4M_TARGETS[((tgt * 16.0).round() as usize).min(16)];
            n[k] = byte(src as f32 / 14.0);
            n[k + 1] = byte(amount.min(2.55));
            n[k + 2] = byte(tgt as f32 / 39.0);
            if src <= 2 && matches!(tgt, 5 | 14) {
                warnings.push(format!("instrument {}: LFO {src} on {}: the 2013 sine spans −0.5…1.5, the final one 0…1", i + 1, if tgt == 5 { "volume" } else { "high-pass cutoff" }));
            }
        }
        if (0..tfsynth::fx::SLOTS).any(|k| tp::effect_kind(n[tp::EFFECT_FIRST + k] as f32 / 100.0) == 5) {
            n[tp::REV_WIDTH] = 100;
        }
        if n[tp::GEN_MOD] > 0 {
            warnings.push(format!("instrument {}: generator modulation reads a 2013 sine table of another length (timbre differs)", i + 1));
        }
        out.extend_from_slice(&n);
    }
    out.extend_from_slice(&b[song..]);
    Ok((out, warnings))
}

/// Converts a `.tf4m` song (2013 engine): [`tf4m_to_tfm`], then [`convert_tfm`].
pub fn convert_tf4m(blob: &[u8], tail: f64) -> Result<Converted, ConvError> {
    let (tfm, mut warnings) = tf4m_to_tfm(blob)?;
    let mut c = convert_tfm(&tfm, tail)?;
    warnings.append(&mut c.warnings);
    c.warnings = warnings;
    Ok(c)
}

