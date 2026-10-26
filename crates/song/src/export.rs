//! Song → `synth-mix` blob (spec "Export to the song blob", design D2). Deterministic: the same song gives the same
//! bytes. Tracks in song order, the master last; one lane per track with every placement flattened; muted tracks
//! export volume 0 and no events; bypassed devices (and their lanes) are left out.

use crate::blob::{AutoM, BlobM, TrackM};
use crate::model::*;
use crate::tables;
use crate::Issue;
use synth_core::device::EventKind;
use unisynth::{chunk, fx, kind, params};

/// Tick → sample (`round(tick·60·rate / (tempo·960))` in f64).
pub fn tick_to_sample(s: &Song, tick: f64) -> i64 {
    (tick * 60.0 * s.rate as f64 / (s.tempo * TICKS_PER_BEAT as f64)).round() as i64
}

/// Sample → tick (exact, f64).
pub fn sample_to_tick(s: &Song, sample: f64) -> f64 {
    sample * s.tempo * TICKS_PER_BEAT as f64 / (60.0 * s.rate as f64)
}

/// The chunk a device exports: kind 64 = unisynth chunk v1 (canonical), effects = `[mode, every parameter]` +
/// trailer (the imported one if it differed, else the byte size).
pub fn chunk(d: &Device) -> Vec<u8> {
    let Some(t) = tables::of_device(d) else { return vec![] };
    if d.kind == kind::INSTRUMENT {
        let mut v = params::DEFAULTS;
        for (name, x) in d.values.iter() {
            if let Some(p) = t.param(name) {
                v[p.index] = x;
            }
        }
        let samples: Vec<Vec<u8>> = d.samples.iter().map(|b| crate::b64::decode(b).unwrap_or_default()).collect();
        let refs: Vec<&[u8]> = samples.iter().map(|s| s.as_slice()).collect();
        return chunk::encode(&v, &refs);
    }
    let vals: Vec<f32> = t.params.iter().map(|p| d.values.get(&p.name).unwrap_or(p.default)).collect();
    let mut c = fx::encode(&vals);
    if let Some(tr) = d.trailer {
        let n = c.len();
        c[n - 4..].copy_from_slice(&tr.to_le_bytes());
    }
    c
}

/// A track's notes as `(start, end, pitch, velocity)` in ticks: every placement flattened (loops repeated, cut at
/// the placement end), sorted by start.
pub fn flatten(s: &Song, t: &Track) -> Vec<(i64, i64, u8, u8)> {
    let mut out = vec![];
    let mut pls: Vec<&Placement> = t.placements.iter().collect();
    pls.sort_by_key(|p| p.start);
    for pl in pls {
        let Some(c) = s.clips.iter().find(|c| c.id == pl.clip) else { continue };
        let end = pl.start.saturating_add(pl.length.unwrap_or(c.length));
        let mut off = pl.start;
        while off < end {
            for n in &c.notes {
                let st = off.saturating_add(n.start);
                if st < end {
                    out.push((st, st.saturating_add(n.len).min(end), n.pitch, n.vel));
                }
            }
            if !pl.looping || c.length <= 0 {
                break;
            }
            off = off.saturating_add(c.length);
        }
    }
    out.sort();
    out
}

/// Notes → lane events in samples. Per pitch one open note: an earlier note is cut at the next start (dropped if
/// it would end before it starts). At one sample: note-offs first, then note-ons, each in pitch order.
fn events(s: &Song, notes: &[(i64, i64, u8, u8)]) -> Vec<(i64, EventKind)> {
    let mut by_pitch: Vec<Vec<(i64, i64, u8)>> = vec![vec![]; 128];
    for &(st, en, p, v) in notes {
        by_pitch[p as usize & 127].push((tick_to_sample(s, st as f64), tick_to_sample(s, en as f64), v));
    }
    let mut ev: Vec<(i64, u8, u8, u8)> = vec![]; // (sample, 0 off | 1 on, pitch, velocity)
    for (p, ns) in by_pitch.iter_mut().enumerate() {
        ns.sort();
        for i in 0..ns.len() {
            let (on, mut off, v) = ns[i];
            if let Some(&(next, _, _)) = ns.get(i + 1) {
                if next <= on {
                    continue; // superseded by a note starting at the same sample
                }
                off = off.min(next);
            }
            off = off.max(on + 1);
            ev.push((on, 1, p as u8, v));
            ev.push((off, 0, p as u8, 0));
        }
    }
    ev.sort();
    ev.into_iter()
        .map(|(at, k, note, velocity)| (at, if k == 1 { EventKind::NoteOn { note, velocity } } else { EventKind::NoteOff { note } }))
        .collect()
}

/// Exports a valid song; returns every validation issue otherwise.
pub fn export(s: &Song) -> Result<Vec<u8>, Vec<Issue>> {
    Ok(export_model(s)?.write())
}

/// [`export`] as an owned blob model (the live host builds its schedules from the same data).
pub fn export_model(s: &Song) -> Result<BlobM, Vec<Issue>> {
    let issues = crate::validate(s);
    if !issues.is_empty() {
        return Err(issues);
    }
    let all: Vec<&Track> = s.tracks.iter().chain([&s.master]).collect();
    let index = |id: &str| all.iter().position(|t| t.id == id).unwrap_or(0);
    let frames = tick_to_sample(s, s.length as f64);
    let mut b = BlobM { tempo: s.tempo.round() as i32, rate: s.rate as i32, length: (frames as f64 + 0.5) / s.rate as f64, ..BlobM::default() };
    for (ti, t) in all.iter().enumerate() {
        // devices (bypassed ones left out) and the map from track device index to exported position
        let mut map = vec![None; t.devices.len()];
        let mut devs = vec![];
        for (di, d) in t.devices.iter().enumerate() {
            if !d.bypass {
                map[di] = Some(devs.len());
                devs.push(b.devices.len());
                b.devices.push((d.kind, chunk(d)));
            }
        }
        let lane = if t.mute { vec![] } else { events(s, &flatten(s, t)) };
        b.lanes.push(lane);
        let mut autos = vec![];
        for l in &t.lanes {
            let (Some(Some(dev)), Some(tab)) = (map.get(l.device), tables::of_device(&t.devices[l.device])) else { continue };
            let Some(p) = tab.param(&l.param) else { continue };
            let points = l.points.iter().map(|pt| (tick_to_sample(s, pt.tick), (pt.value * 255.0).round() as u8)).collect();
            autos.push(AutoM { device: *dev, param: p.index, points });
        }
        b.tracks.push(TrackM {
            volume: if t.mute { 0.0 } else { t.volume },
            receives: t
                .receives
                .iter()
                .map(|r| (index(&r.from), if r.channel == Channel::Side { 2 } else { 0 }, r.volume))
                .collect(),
            devices: devs,
            lane: ti,
            autos,
        });
    }
    Ok(b)
}
