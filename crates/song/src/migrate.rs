//! `song/1` → `song/2` and `us-patch/1` → `us-patch/2` (change unify-v2-algorithms, task 6.6). A `/1` file names and
//! encodes values in the old unisynth layout; [`song`] and [`device`] bring them to v3 once, on load, with the same
//! value rules as the chunk migration (`unisynth::migrate`). A `/2` file is never migrated.
//!
//! - Instrument values: old names (before or after the functional renames of 1.3) → the old patch →
//!   `unisynth::migrate::values` → the v3 values that differ from the defaults, by v3 name, in index order.
//! - Instrument lanes: for each point the whole patch with the lane's value is migrated, and every v3 field that moves
//!   gets a lane at the old lane's ticks (a split choice becomes one lane per changed setting). A lane on a field with
//!   no v3 form is an error naming it.
//! - Effects: the mode as the kind's plain choice; the 7-bit modes' `era` turns into what it decided (cut EQ: the DC
//!   filter; comb reverb below version 4: the low cut off; delay and compressor: dropped); the old chunk trailer is
//!   dropped (v3 chunks end in their version tag).

use crate::model::{Device, Lane, Point, Song, Values};
use crate::Issue;
use unisynth::migrate as um;
use unisynth::params::{self as p3, old as po};

/// The old layout's index of `name` (its pre-1.3 or post-1.3 name).
fn old_index(name: &str) -> Option<usize> {
    (0..po::COUNT).find(|&i| po::name(i) == name || p3::rename::OLD[i].0 == name)
}

fn err(path: String, message: String) -> Issue {
    Issue { path, message }
}

/// An instrument's old values (by name) as the old patch.
fn old_patch(v: &Values, path: &str) -> Result<[f32; po::COUNT], Vec<Issue>> {
    let mut old = po::DEFAULTS;
    let mut issues = vec![];
    for (n, x) in v.iter() {
        match old_index(n) {
            Some(i) => old[i] = x,
            None => issues.push(err(format!("{path}/values/{}", crate::validate::seg(n)), "unknown parameter".to_string())),
        }
    }
    if issues.is_empty() { Ok(old) } else { Err(issues) }
}

/// v3 values that differ from the defaults, by name, in index order.
fn v3_values(p: &[f32; p3::COUNT]) -> Values {
    Values((0..p3::COUNT).filter(|&i| p[i].to_bits() != p3::DEFAULTS[i].to_bits()).map(|i| (p3::name(i), p[i])).collect())
}

/// A `/1` device → v3 (instrument values; effect mode, `era`, trailer). Returns the old patch of an instrument (its
/// lanes migrate against it).
pub fn device(d: &mut Device, path: &str) -> Result<Option<[f32; po::COUNT]>, Vec<Issue>> {
    if d.kind == unisynth::kind::INSTRUMENT {
        let old = old_patch(&d.values, path)?;
        let new = um::values(&old).map_err(|e| vec![err(format!("{path}/values"), e.0)])?;
        d.values = v3_values(&new);
        return Ok(Some(old));
    }
    if !unisynth::fx::is_effect(d.kind) {
        return Ok(None);
    }
    d.trailer = None;
    let k = d.kind;
    let mode = um::old_fx_mode(k, d.values.get("mode").unwrap_or(0.0));
    let mut v = Values::default();
    if mode != 0 {
        v.set("mode", unisynth::fx::mode_value(k, mode));
    }
    let era = d.values.get("era").map(|x| unisynth::law::count(x, 0, 6));
    for (n, x) in d.values.iter() {
        if n != "mode" && n != "era" {
            v.set(n, x);
        }
    }
    use unisynth::kind;
    match (k, mode) {
        (kind::EQ, 2) if era.is_some_and(|e| e < 6) => v.set("dc_filter", 0.0),
        (kind::REVERB, 3) if era.is_some_and(|e| e < 4) => v.set("low_cut", 0.0),
        _ => {}
    }
    d.values = v;
    Ok(None)
}

/// The v3 lanes an old lane on an instrument becomes (values as stored, no byte rounding).
fn instrument_lane(base: &[f32; po::COUNT], lane: &Lane, path: &str) -> Result<Vec<Lane>, Vec<Issue>> {
    let e = |m: String| vec![err(format!("{path}/param"), m)];
    let i = old_index(&lane.param).ok_or_else(|| e(format!("unknown song/1 parameter {}", lane.param)))?;
    if i == po::v2::VOICE || i == po::v2::ERA {
        return Err(e(format!("a lane on {} has no song/2 form (it is structural)", lane.param)));
    }
    let base_new = um::values(base).map_err(|x| e(x.0))?;
    let per_point: Vec<[f32; p3::COUNT]> = lane
        .points
        .iter()
        .map(|pt| {
            let mut old = *base;
            old[i] = pt.value;
            um::values(&old).map_err(|x| e(format!("lane on {}: {}", lane.param, x.0)))
        })
        .collect::<Result<_, _>>()?;
    let mut out = vec![];
    for j in 0..p3::COUNT {
        // a quirk is fixed per patch: `song` sets the ones a lane reaches (`um::lane_quirks`)
        if per_point.iter().all(|v| v[j] == base_new[j]) || (p3::QUIRK..p3::QUIRK + p3::quirk::LEN).contains(&j) {
            continue;
        }
        if p3::is_structural(j) {
            return Err(e(format!("a lane on {} moves {}, which is structural in song/2", lane.param, p3::name(j))));
        }
        let points = lane.points.iter().zip(&per_point).map(|(pt, v)| Point { tick: pt.tick, value: v[j] }).collect();
        out.push(Lane { device: lane.device, param: p3::name(j), points });
    }
    if out.is_empty() {
        // a lane that moves nothing keeps its name where the name survives
        let new = p3::rename::OLD[i].1;
        if (0..p3::COUNT).any(|j| p3::name(j) == new) {
            out.push(Lane { param: new.to_string(), ..lane.clone() });
        }
    }
    Ok(out)
}

/// A `song/1` song → `song/2` (every device and lane; see the module doc). Leaves a `song/2` song alone.
pub fn song(s: &mut Song) -> Result<(), Vec<Issue>> {
    if s.format != crate::model::FORMAT_V1 {
        return Ok(());
    }
    let mut issues = vec![];
    let n = s.tracks.len();
    for (ti, t) in s.tracks.iter_mut().chain(std::iter::once(&mut s.master)).enumerate() {
        let tp = if ti < n { format!("/tracks/{ti}") } else { "/master".to_string() };
        let mut bases = vec![];
        for (di, d) in t.devices.iter_mut().enumerate() {
            match device(d, &format!("{tp}/devices/{di}")) {
                Ok(b) => bases.push(b),
                Err(e) => {
                    issues.extend(e);
                    bases.push(None);
                }
            }
        }
        let mut lanes = vec![];
        for (li, l) in std::mem::take(&mut t.lanes).into_iter().enumerate() {
            let path = format!("{tp}/lanes/{li}");
            match (bases.get(l.device).copied().flatten(), t.devices.get(l.device)) {
                (Some(base), _) => match instrument_lane(&base, &l, &path) {
                    Ok(v) => {
                        lanes.extend(v);
                        if let Some(i) = old_index(&l.param) {
                            for q in um::lane_quirks(&base, i, l.points.iter().map(|p| p.value)) {
                                t.devices[l.device].values.set(&p3::name(q), 1.0);
                            }
                        }
                    }
                    Err(e) => issues.extend(e),
                },
                (None, Some(d)) if unisynth::fx::is_effect(d.kind) && l.param == "era" => {
                    issues.push(err(format!("{path}/param"), "a lane on an effect's format version has no song/2 form".into()))
                }
                // the mode as the kind's plain choice, point by point (as the device's own mode)
                (None, Some(d)) if unisynth::fx::is_effect(d.kind) && l.param == "mode" => {
                    let k = d.kind;
                    let mut l = l;
                    l.points.iter_mut().for_each(|p| p.value = unisynth::fx::mode_value(k, um::old_fx_mode(k, p.value)));
                    lanes.push(l)
                }
                _ => lanes.push(l),
            }
        }
        t.lanes = lanes;
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    s.format = crate::model::FORMAT.to_string();
    Ok(())
}
