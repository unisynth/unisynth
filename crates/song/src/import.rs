//! `synth-mix` blob → song (spec "Import from the song blob"). Devices, routing, volumes and automation come back
//! exactly (automation ticks are fractional so sample times survive); notes are rounded to ticks. `import` cuts each
//! lane into patterns (`crate::patterns`); `import_one_clip` keeps one clip per lane placed at tick 0.
//! `apply_meta` reads an import config (`Meta` plus `about` and `tracks`). Untrusted input: all structure is
//! validated by `synth_core::mix::Song::parse`, chunks by `unisynth::chunk::Chunk::parse`; nothing here panics on a
//! blob that parses.

use crate::blob::BlobM;
use crate::export::sample_to_tick;
use crate::model::*;
use crate::tables;
use synth_core::device::EventKind;
use unisynth::{chunk::Chunk, fx, kind, params};

fn device(k: u8, c: &[u8]) -> Result<Device, String> {
    if k <= 39 {
        return Err(format!("kind {k} is a legacy device; convert the song with synth-convert first"));
    }
    let mut d = Device { kind: k, values: Values::default(), samples: vec![], bypass: false, preset: None, trailer: None };
    if k == kind::INSTRUMENT {
        let ch = Chunk::parse(c).map_err(|e| format!("instrument chunk: {e:?}"))?;
        let t = tables::table(k, 0).ok_or("no instrument table")?;
        for (i, v) in ch.values().iter().enumerate() {
            if v.to_bits() != params::DEFAULTS[i].to_bits() {
                d.values.set(&t.params[i].name, *v);
            }
        }
        d.samples = (0..ch.sample_count()).filter_map(|i| ch.sample_bytes(i)).map(crate::b64::encode).collect();
        return Ok(d);
    }
    if !fx::is_effect(k) {
        return Err(format!("kind {k} is not supported"));
    }
    let n = c.len().saturating_sub(4) / 4;
    let f = |i: usize| f32::from_le_bytes([c[4 * i], c[4 * i + 1], c[4 * i + 2], c[4 * i + 3]]);
    let mode = if n > 0 { f(0) } else { 0.0 };
    d.values.set("mode", mode);
    let t = tables::of_device(&d).ok_or_else(|| format!("kind {k}: no table for its mode"))?;
    for (i, p) in t.params.iter().enumerate().skip(1) {
        if i < n {
            d.values.set(&p.name, f(i));
        }
    }
    if c.len() >= 4 {
        let tr = i32::from_le_bytes([c[c.len() - 4], c[c.len() - 3], c[c.len() - 2], c[c.len() - 1]]);
        let canonical = (4 * t.params.len() + 4) as i32;
        if tr != canonical || n != t.params.len() {
            d.trailer = Some(tr);
        }
    }
    Ok(d)
}

/// Lane events → notes in ticks: each note-on pairs with the next note-off of its pitch; notes still open at the end
/// last until `end`.
fn notes(s: &Song, ev: &[(i64, EventKind)], end: i64) -> Vec<Note> {
    let mut open: [Option<(i64, u8)>; 128] = [None; 128];
    let mut out = vec![];
    let tick = |x: i64| sample_to_tick(s, x as f64).round() as i64;
    let mut close = |p: usize, at: i64, open: &mut [Option<(i64, u8)>; 128]| {
        if let Some((st, v)) = open[p].take() {
            let (a, b) = (tick(st), tick(at));
            out.push(Note { start: a, len: (b - a).max(1), pitch: p as u8, vel: v.max(1) });
        }
    };
    for &(at, k) in ev {
        match k {
            EventKind::NoteOn { note, velocity } => {
                let p = note as usize & 127;
                close(p, at, &mut open);
                open[p] = Some((at, velocity));
            }
            EventKind::NoteOff { note } => close(note as usize & 127, at, &mut open),
        }
    }
    for p in 0..128 {
        close(p, end, &mut open);
    }
    out.sort_by_key(|n| (n.start, n.pitch));
    out
}

/// Blob → song with each lane cut into patterns (same export as [`import_one_clip`]).
pub fn import(b: &[u8]) -> Result<Song, String> {
    let mut s = import_one_clip(b)?;
    crate::patterns::split(&mut s);
    Ok(s)
}

/// Applies an import config: the `Meta` fields, plus optional `about` (the song) and `tracks` (`[{name, about?}]`,
/// one per track before the master, in order). Errors name the problem; `s` is unchanged on error.
pub fn apply_meta(s: &mut Song, text: &str) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TrackInfo {
        name: String,
        #[serde(default)]
        about: Option<About>,
    }
    let mut v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let obj = v.as_object_mut().ok_or("expected an object")?;
    let about: Option<About> = obj.remove("about").map(serde_json::from_value).transpose().map_err(|e| format!("about: {e}"))?;
    let tracks: Option<Vec<TrackInfo>> = obj.remove("tracks").map(serde_json::from_value).transpose().map_err(|e| format!("tracks: {e}"))?;
    let meta: Meta = serde_json::from_value(v).map_err(|e| e.to_string())?;
    if let Some(t) = &tracks {
        if t.len() != s.tracks.len() {
            return Err(format!("tracks: {} entries for {} tracks (the master is not listed)", t.len(), s.tracks.len()));
        }
    }
    s.meta = Some(meta);
    s.about = about;
    for (tr, info) in s.tracks.iter_mut().zip(tracks.into_iter().flatten()) {
        tr.name = info.name;
        tr.about = info.about;
    }
    Ok(())
}

/// Blob → song with one clip per lane placed once at tick 0 (shared lanes become linked placements).
pub fn import_one_clip(b: &[u8]) -> Result<Song, String> {
    let m = BlobM::read(b).map_err(|e| format!("not a valid song blob: {e:?}"))?;
    let mut s = Song {
        format: FORMAT.into(),
        meta: None,
        about: None,
        rate: m.rate as u32,
        tempo: m.tempo as f64,
        signature: [4, 4],
        ticks_per_beat: TICKS_PER_BEAT,
        length: 0,
        clips: vec![],
        tracks: vec![],
        master: Track { id: String::new(), name: String::new(), about: None, color: None, volume: 1.0, mute: false, receives: vec![], devices: vec![], placements: vec![], lanes: vec![] },
    };
    let end_sample = (m.length * m.rate as f64) as i64;
    let mut length = sample_to_tick(&s, end_sample as f64).round() as i64;
    // one clip per lane that a track plays (shared lanes become linked placements)
    let mut clip_of = vec![None; m.lanes.len()];
    let nt = m.tracks.len();
    let ids: Vec<String> = (0..nt).map(|i| if i + 1 == nt { "master".to_string() } else { format!("t{}", i + 1) }).collect();
    let mut tracks = vec![];
    for (ti, t) in m.tracks.iter().enumerate() {
        let mut tr = Track {
            id: ids[ti].clone(),
            name: if ti + 1 == nt { "Master".into() } else { format!("Track {}", ti + 1) },
            about: None,
            color: None,
            volume: t.volume,
            mute: false,
            receives: t
                .receives
                .iter()
                .map(|&(from, ch, v)| Receive { from: ids[from].clone(), channel: if ch == 2 { Channel::Side } else { Channel::Main }, volume: v })
                .collect(),
            devices: vec![],
            placements: vec![],
            lanes: vec![],
        };
        for &di in &t.devices {
            let (k, c) = &m.devices[di];
            tr.devices.push(device(*k, c).map_err(|e| format!("track {}, device {di}: {e}", ti + 1))?);
        }
        let lane = &m.lanes[t.lane];
        if !lane.is_empty() {
            let id = match clip_of[t.lane] {
                Some(ref id) => String::clone(id),
                None => {
                    let ns = notes(&s, lane, end_sample.max(lane.last().map_or(0, |e| e.0)));
                    let last = ns.iter().map(|n| n.start + n.len).max().unwrap_or(1);
                    let id = format!("lane{}", t.lane + 1);
                    s.clips.push(Clip { id: id.clone(), length: last.max(length).max(1), notes: ns, wheel: vec![], bend: vec![] });
                    clip_of[t.lane] = Some(id.clone());
                    id
                }
            };
            let len = s.clips.iter().find(|c| c.id == id).map_or(1, |c| c.length);
            length = length.max(len);
            tr.placements.push(Placement { clip: id, start: 0, length: Some(len), looping: false });
        }
        for a in &t.autos {
            let d = &tr.devices[a.device];
            let tab = tables::of_device(d).ok_or("no table")?;
            let p = tab.params.iter().find(|p| p.index == a.param).ok_or_else(|| format!("track {}: automated parameter {} does not exist", ti + 1, a.param))?;
            let points: Vec<Point> = a.points.iter().map(|&(at, v)| Point { tick: sample_to_tick(&s, at as f64), value: v as f32 / 255.0 }).collect();
            if let Some(last) = points.last() {
                length = length.max(last.tick.ceil() as i64);
            }
            tr.lanes.push(Lane { device: a.device, param: p.name.clone(), points });
        }
        tracks.push(tr);
    }
    s.master = tracks.pop().ok_or("song has no tracks")?;
    s.tracks = tracks;
    s.length = length;
    Ok(s)
}
