//! Tunefish 3 songs (Enigma Studio 3 projects) as unified `synth-mix` songs (change add-tf3-import, design D7).
//!
//! The player's row firing is replayed block by block: at each 384-frame block it computes
//! `row = round(t · bpm / 60 · 4)` from `t = frames/44 100` in f32 (rounding half to even, so a row starts half a row
//! early), and when the row changes it fires that row on every sequence track: the first pattern instance covering it,
//! each column's cell. A note on a column first releases the column's previous note (by note number, on its
//! instrument); note 128 only releases; a note without instrument does nothing; velocity −1 plays 128. One track per
//! instrument that plays (the instrument, then its inserts), a master last at ×11 and `5000/32768` (the player's i16
//! scaling). The song stops where the row would wrap to 0.

use crate::e3prj::{Project, Song};
use crate::model::{Auto, Dev, Ev, SongM, Trk};
use crate::tf3::{self, Mode};
use crate::{ConvError, Converted};

const RATE: f32 = 44_100.0;
const BLOCK: u64 = 384;
/// The player's float → i16 factor (`5000 · volume 1`), as a share of the 16-bit full scale.
const MASTER: f32 = 5000.0 / 32_768.0;
/// Each instrument's gain into the bus (the gain curve at 1.0).
const INSTRUMENT: f32 = 11.0;
/// Rows a song may span (a guard against hostile lengths).
const MAX_BLOCKS: u64 = (i32::MAX as u64) / BLOCK;

/// One note event of an instrument: frame, note, velocity (`None` = note-off), effect word (note-ons only).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub frame: u64,
    pub instrument: u32,
    pub note: u8,
    pub velocity: Option<u8>,
    pub effect: u16,
}

/// The row the player is on at block `b` (spec §6.2): f32 time, x87 rounding.
pub fn row_at(block: u64, bpm: u32) -> u32 {
    let t = (block * BLOCK) as f32 / RATE;
    tf3::fi(t * bpm as f32 / 60.0 * 4.0).max(0) as u32
}

/// Replays the player over one pass of `song`: the events in firing order and the frame where the song wraps.
pub fn events(song: &Song, has_instrument: impl Fn(u32) -> bool) -> Result<(Vec<Event>, u64), ConvError> {
    let length = song.length_rows();
    if length == 0 || song.bpm == 0 {
        return Err(ConvError::Input("e3prj: the song has no rows or no tempo".into()));
    }
    let mut out = vec![];
    // the last note of each (sequence track, column): (instrument, note)
    let mut last: std::collections::HashMap<(u32, usize), (u32, u8)> = std::collections::HashMap::new();
    let mut prev_row: Option<u32> = None;
    let mut block = 0u64;
    loop {
        let row = row_at(block, song.bpm);
        if row >= length {
            return Ok((out, block * BLOCK));
        }
        if prev_row != Some(row) {
            prev_row = Some(row);
            let frame = block * BLOCK;
            for track in 0..16u32 {
                let Some(inst) = song.instances.iter().find(|i| i.track == track && {
                    let rows = song.pattern(i.pattern).map_or(0, |p| p.rows);
                    row >= i.row_offset && row < i.row_offset + rows
                }) else {
                    continue;
                };
                let pattern = song.pattern(inst.pattern).ok_or_else(|| ConvError::Input(format!("e3prj: missing pattern {}", inst.pattern)))?;
                let prow = row - inst.row_offset;
                for (col, cells) in pattern.tracks.iter().enumerate() {
                    let Some(c) = cells.iter().find(|c| c.row == prow) else { continue };
                    let release = |out: &mut Vec<Event>, last: &mut std::collections::HashMap<(u32, usize), (u32, u8)>| {
                        if let Some((i, n)) = last.remove(&(track, col)) {
                            out.push(Event { frame, instrument: i, note: n, velocity: None, effect: 0 });
                        }
                    };
                    if c.note != 0 && c.instrument >= 0 {
                        if !has_instrument(c.instrument as u32) {
                            continue; // the player has no such instrument: nothing happens
                        }
                        release(&mut out, &mut last);
                        if c.note != 128 {
                            let note = (c.note - 1) & 0x7f;
                            let vel = if c.velocity < 0 { 127 } else { c.velocity.clamp(1, 127) as u8 };
                            out.push(Event { frame, instrument: c.instrument as u32, note, velocity: Some(vel), effect: c.effect });
                            last.insert((track, col), (c.instrument as u32, note));
                        }
                    } else if c.note == 128 {
                        release(&mut out, &mut last);
                    }
                }
            }
        }
        block += 1;
        if block > MAX_BLOCKS {
            return Err(ConvError::Input("e3prj: the song is longer than the song format can hold".into()));
        }
    }
}

/// Converts a project's song: `tail` seconds after the song's end for the release tails.
pub fn convert(project: &Project, song_id: Option<u32>, mode: Mode, tail: f64) -> Result<Converted, ConvError> {
    let song = project.song(song_id)?;
    let instrument = |id: u32| project.instruments.iter().find(|i| i.id == id);
    let (evs, end) = events(song, |id| instrument(id).is_some())?;
    let mut warnings = vec![];
    let mut used: Vec<u32> = evs.iter().filter(|e| e.velocity.is_some()).map(|e| e.instrument).collect();
    used.sort_unstable();
    used.dedup();
    let mut devices: Vec<Dev> = vec![];
    let (mut lanes, mut tracks) = (vec![], vec![]);
    for (t, &id) in used.iter().enumerate() {
        let ins = instrument(id).expect("events only name existing instruments");
        let p = if mode == Mode::Player { tf3::quantise(&ins.params) } else { ins.params.clone() };
        let mut w = vec![];
        let mut list = vec![devices.len()];
        devices.push(tf3::instrument_device(&p, mode, &mut w));
        for d in tf3::insert_chain(&p, mode, &mut w) {
            list.push(devices.len());
            devices.push(d);
        }
        warnings.extend(w.into_iter().map(|m| format!("instrument {id} \"{}\": {m}", ins.name)));
        // the lane, and in editor mode the effect words as controller automation (CC2–CC5 = MOD1–4) at their note
        let mut lane: Vec<Ev> = vec![];
        let (mut prev, mut prev_auto) = (0u64, 0u64);
        let mut autos: Vec<Auto> = vec![];
        for e in evs.iter().filter(|e| e.instrument == id) {
            lane.push(((e.frame - prev) as i32, e.note, e.velocity));
            prev = e.frame;
            let slot = (e.effect >> 8) as usize;
            if e.velocity.is_some() && (1..=4).contains(&slot) {
                if mode != Mode::Editor {
                    warnings.push(format!("instrument {id}: effect words set MOD{slot}, which the Tunefish 3 player leaves out; dropped"));
                    continue;
                }
                let param = unisynth::params::cc(1 + slot);
                let byte = (((e.effect & 0xff) as f32 / 128.0) * 255.0).round().clamp(0.0, 255.0) as u8;
                match autos.iter_mut().find(|a| a.param == param) {
                    Some(a) => a.points.push(((e.frame - prev_auto) as i32, byte)),
                    None => autos.push(Auto { device: 0, param, points: vec![(e.frame as i32 - prev_auto as i32, byte)] }),
                }
                prev_auto = e.frame;
            }
        }
        lanes.push(lane);
        tracks.push(Trk { volume: 1.0, receives: vec![], devices: list, lane: t, autos });
    }
    warnings.dedup();
    lanes.push(vec![]);
    tracks.push(Trk { volume: MASTER, receives: (0..used.len()).map(|i| (i, 0, INSTRUMENT)).collect(), devices: vec![], lane: used.len(), autos: vec![] });
    let length = end as f64 / RATE as f64 + tail;
    let s = SongM { tempo: song.bpm as i32, rate: 44_100, length, devices, lanes, tracks };
    Ok(Converted { blob: s.write()?, warnings })
}

/// One preset (129 values) as a unified instrument chunk.
pub fn preset_chunk(params: &[f32], mode: Mode) -> (Vec<u8>, Vec<String>) {
    let p = if mode == Mode::Player { tf3::quantise(params) } else { params.to_vec() };
    let mut w = vec![];
    let d = tf3::instrument_device(&p, mode, &mut w);
    (d.chunk, w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::e3prj::{Cell, Instance, Instrument, Pattern, COUNT};

    fn cell(row: u32, note: u8, instrument: i32) -> Cell {
        Cell { row, note, instrument, velocity: -1, effect: 0 }
    }

    fn song(cells: Vec<Cell>, bpm: u32) -> Song {
        Song { id: 1, name: "t".into(), bpm, patterns: vec![Pattern { id: 0, rows: 32, tracks: vec![cells] }], instances: vec![Instance { row_offset: 0, track: 0, pattern: 0 }] }
    }

    #[test]
    fn rows_start_half_a_row_early() {
        // 140 bpm: row 16 is due at 1.714 s; the rounded row reaches 16 at 1.661 s, block 191
        let (ev, _) = events(&song(vec![cell(16, 61, 0)], 140), |_| true).unwrap();
        assert_eq!(ev[0].frame, 191 * 384);
        assert_eq!((ev[0].note, ev[0].velocity), (60, Some(127)));
    }

    #[test]
    fn a_note_without_instrument_does_nothing_and_note_128_releases() {
        let (ev, _) = events(&song(vec![cell(0, 61, 3), cell(2, 65, -1), cell(4, 128, -1)], 125), |_| true).unwrap();
        assert_eq!(ev.len(), 2, "{ev:?}");
        assert_eq!((ev[1].instrument, ev[1].note, ev[1].velocity), (3, 60, None));
        assert_eq!(ev[1].frame / 384, (0..).find(|&b| row_at(b, 125) == 4).unwrap());
    }

    #[test]
    fn a_new_note_on_the_column_releases_the_previous_one_first() {
        let (ev, end) = events(&song(vec![cell(0, 61, 0), cell(8, 63, 1)], 120), |_| true).unwrap();
        let at8: Vec<_> = ev.iter().filter(|e| e.frame > 0).map(|e| (e.instrument, e.note, e.velocity)).collect();
        assert_eq!(at8, [(0, 60, None), (1, 62, Some(127))]);
        assert_eq!(row_at(end / 384, 120), 32, "the pass ends where the row reaches the length");
        assert_eq!(row_at(end / 384 - 1, 120), 31);
    }

    #[test]
    fn a_converted_song_has_a_track_per_playing_instrument_and_a_master() {
        let mut params = vec![0.0f32; COUNT];
        params[crate::e3prj::index("OscVol").unwrap()] = 0.5;
        let p = Project {
            version: "3.11b".into(),
            songs: vec![song(vec![cell(0, 61, 0), cell(8, 128, -1), cell(9, 61, 5)], 140)],
            instruments: vec![Instrument { id: 0, name: "a".into(), params: params.clone() }, Instrument { id: 1, name: "b".into(), params }],
        };
        let c = convert(&p, None, Mode::Player, 1.0).unwrap();
        let s = SongM::read(&c.blob).unwrap();
        assert_eq!(s.tracks.len(), 2, "instrument 0 and the master (instrument 5 does not exist, 1 never plays)");
        assert_eq!(s.lanes[0].len(), 2);
        assert!((s.tracks[1].volume - MASTER).abs() < 1e-9 && s.tracks[1].receives == [(0, 0, INSTRUMENT)]);
    }
}
