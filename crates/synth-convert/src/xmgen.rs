//! Plain XMs with a generator map (change add-plain-xm-import): which XM samples an intro synthesized, by which
//! engine with which parameters, and which it stored. Generated samples become notes of the engine's instrument
//! (design D3: an oscillator type per engine, run at the sample's native clock × playback ratio); stored samples play
//! on a sampler. Nothing generated is ever written as sample data.
//!
//! Map format `xm-gen/1` (JSON):
//! `{"format": "xm-gen/1", "intro": "<name>", "samples": [{"instrument": 1, "sample": 0, "kind": "generated",
//! "engine": "<id>", "length": 4096, "params": {...}}, {"instrument": 7, "sample": 0, "kind":
//! "stored"}]}`. Instruments are 1-based as in a tracker, samples 0-based. `params` is the engine's record as the
//! original computed it (an engine needing a time base keeps it there: the playback rate comes from the XM).
//!
//! The note byte contract (design D3), for engines with one note per run of the sample ([`tone_events`]): a note-on
//! carries its start frame in `o::TRACKING` (×4096) and `o::PM_OUT` (×16), and a backward pass of a seekable engine in
//! `o::FEEDBACK` (255), as latched lanes; a note whose bytes differ from the previous note's starts at least
//! `LANE_LEAD` frames after it. Phrase engines define their own events and lanes ([`Engine::events`], [`Engine::lanes`]).
//! Identical generated records share one engine instrument (Fudge's instruments 18, 19 and 24 reuse 11, 13 and 11).

use crate::cauldron::import::{Path, Seg};
use crate::cauldron::{Chain, PEv, PhraseEv, LANE_LEAD, LATCH_WINDOW};
use crate::xm::Xm;
use serde_json::Value;

#[cfg(test)]
mod tests;

/// One XM sample in the map.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// 1-based instrument, 0-based sample.
    pub instrument: usize,
    pub sample: usize,
    pub generated: Option<Generated>,
}

/// How a generated sample was made.
#[derive(Clone, Debug, PartialEq)]
pub struct Generated {
    pub engine: String,
    pub length: u32,
    pub params: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Map {
    pub intro: String,
    pub samples: Vec<Entry>,
}

/// A JSON text is meant as a generator map (not mini-cauldron patch data): it has a `format` key ([`load`] names a
/// format it does not read).
pub fn is_map(json: &str) -> bool {
    serde_json::from_str::<Value>(json).is_ok_and(|v| v.get("format").is_some())
}

pub fn load(json: &str) -> Result<Map, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("generator map: {e}"))?;
    if v.get("format").and_then(Value::as_str) != Some("xm-gen/1") {
        return Err("generator map: `format` is not xm-gen/1".into());
    }
    let list = v.get("samples").and_then(Value::as_array).ok_or("generator map: `samples` is not a list")?;
    let int = |e: &Value, k: &str, i: usize| e.get(k).and_then(Value::as_u64).ok_or(format!("generator map: entry {i}: `{k}` missing"));
    let samples = list
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let generated = match e.get("kind").and_then(Value::as_str) {
                Some("stored") => None,
                Some("generated") => {
                    let engine = e.get("engine").and_then(Value::as_str).ok_or(format!("generator map: entry {i}: `engine` missing"))?.to_string();
                    let length = int(e, "length", i)? as u32;
                    let params = e.get("params").filter(|p| p.is_object()).cloned().ok_or(format!("generator map: entry {i}: `params` missing or not an object"))?;
                    Some(Generated { engine, length, params })
                }
                _ => return Err(format!("generator map: entry {i}: `kind` is neither stored nor generated")),
            };
            let instrument = int(e, "instrument", i)? as usize;
            if instrument == 0 {
                return Err(format!("generator map: entry {i}: instruments count from 1"));
            }
            Ok(Entry { instrument, sample: int(e, "sample", i)? as usize, generated })
        })
        .collect::<Result<_, String>>()?;
    Ok(Map { intro: v.get("intro").and_then(Value::as_str).unwrap_or("").to_string(), samples })
}

/// An engine that renders generated samples (an oscillator type of the unified synth, design D3).
pub trait Engine: Sync {
    fn id(&self) -> &'static str;
    /// It starts at any frame and runs either way (by replaying from frame 0 at a note-on when its state carries
    /// over, design D4): a backward pass plays the frames in reverse. Otherwise a backward pass plays forwards.
    fn seekable(&self, params: &Value) -> bool;
    /// The instrument values playing the sample at phrase clock `clock` (Hz: sample frames per second at the
    /// track's class, `cauldron::R` × its ratio; bend and transpose carry the rest, as the clocked playback rate).
    fn instrument(&self, params: &Value, clock: f64) -> [f32; unisynth::params::old::COUNT];
    /// A trigger's notes along its path at phrase clock `clock` (Hz): one note per run of the sample by default
    /// ([`tone_events`]); a phrase engine unrolls its own events.
    fn events(&self, params: &Value, _clock: f64, path: &Path) -> Vec<PhraseEv> {
        tone_events(path, self.seekable(params))
    }
    /// The note lane and the per-note byte lanes for a track's events: the byte contract by default ([`note_lanes`]).
    fn lanes(&self, evs: Vec<(i64, PEv)>, values: &mut [f32; unisynth::params::old::COUNT]) -> Result<Lanes, String> {
        note_lanes(evs.into_iter(), values)
    }
}

/// A track's note lane and its automations.
pub type Lanes = (Vec<(i32, u8, Option<u8>)>, Vec<crate::model::Auto>);

/// The largest start frame the byte contract carries.
pub const MAX_POSITION: u32 = 255 * 4096 + 255 * 16;

/// The engines in this build.
pub fn engines() -> Vec<&'static dyn Engine> {
    #[allow(unused_mut)]
    let mut v: Vec<&'static dyn Engine> = vec![];
    #[cfg(test)]
    v.push(&tests::CountFrames);
    v
}

fn engine(id: &str) -> Option<&'static dyn Engine> {
    engines().into_iter().find(|e| e.id() == id)
}

/// The XM's samples against the map: every non-empty sample listed once, nothing listed the XM lacks, every engine
/// available.
pub fn check(map: &Map, xm: &Xm) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for e in &map.samples {
        let ok = e.instrument >= 1 && xm.instruments.get(e.instrument - 1).is_some_and(|i| e.sample < i.samples.len());
        if !ok {
            return Err(format!("generator map: instrument {} sample {} is not in the XM", e.instrument, e.sample));
        }
        if !seen.insert((e.instrument, e.sample)) {
            return Err(format!("generator map: instrument {} sample {} is listed twice", e.instrument, e.sample));
        }
        let len = xm.instruments[e.instrument - 1].samples[e.sample].data.len() as u32;
        if len == 0 {
            return Err(format!("generator map: instrument {} sample {} is empty in the XM", e.instrument, e.sample));
        }
        if let Some(g) = &e.generated {
            if g.length != len {
                return Err(format!("generator map: instrument {} sample {}: length {} but the XM's is {len}", e.instrument, e.sample, g.length));
            }
            if len > MAX_POSITION + 16 {
                return Err(format!("generator map: instrument {} sample {}: {len} frames, past the byte contract's {MAX_POSITION}", e.instrument, e.sample));
            }
        }
    }
    for (i, ins) in xm.instruments.iter().enumerate() {
        for (s, smp) in ins.samples.iter().enumerate() {
            if !smp.data.is_empty() && !seen.contains(&(i + 1, s)) {
                return Err(format!("generator map: instrument {} sample {s} (`{}`) is not in the map", i + 1, smp.name.trim()));
            }
        }
    }
    for e in &map.samples {
        if let Some(g) = &e.generated {
            if engine(&g.engine).is_none() {
                return Err(format!("engine `{}` (instrument {} sample {}) is not available in this build", g.engine, e.instrument, e.sample));
            }
        }
    }
    Ok(())
}

/// The start-position bytes of a frame: `(hi, lo)` with `hi·4096 + lo·16` the frame rounded to 16.
pub fn position_bytes(frame: f64) -> Result<(u8, u8), String> {
    let q = (frame.max(0.0) / 16.0).round() as u64;
    let (hi, lo) = (q / 256, q % 256);
    if hi > 255 {
        return Err(format!("start position {frame:.0} past the byte contract's {}", 255 * 4096 + 255 * 16));
    }
    Ok((hi as u8, lo as u8))
}

/// One note per run of the sample: a note-on at each jump (the trigger's start, a loop re-entry, a ping-pong turn)
/// with its start position, an off at the next jump and at the path's end. A backward pass plays backwards when the
/// engine is seekable; otherwise forwards from its lowest frame over the same time (reversed order, as cauldron
/// phrases).
pub fn tone_events(p: &Path, seekable: bool) -> Vec<PhraseEv> {
    let mut out = vec![];
    let joins = |q: &Seg, g: &Seg| (q.t1 - g.t0).abs() < 1e-6 && (q.x1 - g.x0).abs() < 1e-6 && (q.x1 < q.x0) == (g.x1 < g.x0);
    let mut prev: Option<Seg> = None;
    for (i, g) in p.segs.iter().enumerate() {
        let back = g.x1 < g.x0;
        if !prev.is_some_and(|q| joins(&q, g)) {
            if prev.is_some() {
                out.push(PhraseEv { at: g.t0, what: PEv::Off });
            }
            // a non-seekable backward run (one segment per tick) plays forwards from its lowest frame
            let low = || {
                let mut end = i + 1;
                while end < p.segs.len() && joins(&p.segs[end - 1], &p.segs[end]) {
                    end += 1;
                }
                p.segs[i..end].iter().fold(f64::MAX, |m, s| m.min(s.x1))
            };
            let (start, dir) = if back && !seekable { (low(), 0) } else { (g.x0, if back { 255 } else { 0 }) };
            // within the sample, which `check` keeps within the contract
            let (hi, lo) = position_bytes(start.min(MAX_POSITION as f64)).expect("checked");
            out.push(PhraseEv { at: g.t0, what: PEv::Gen { hi, lo, back: dir } });
        }
        prev = Some(*g);
    }
    if prev.is_some() {
        out.push(PhraseEv { at: p.end, what: PEv::Off });
    }
    out
}

/// The note every generated note plays (the engine's pitch is its clock, not the note).
pub const NOTE: u8 = 60;

/// The note lane and the contract's byte lanes (device 0) for generated notes at song frames.
/// A note whose bytes differ from the previous note's less than `LANE_LEAD` frames after it is an error: its bytes
/// would step before the previous note-on reads its own.
pub fn note_lanes(evs: impl Iterator<Item = (i64, PEv)>, values: &mut [f32; unisynth::params::old::COUNT]) -> Result<Lanes, String> {
    use unisynth::params::old::{self as pm, o};
    let (mut hi, mut lo, mut back) = (vec![], vec![], vec![]);
    let (mut lane, mut t) = (vec![], 0i64);
    let mut last: Option<(i64, (u8, u8, u8))> = None;
    for (f, what) in evs {
        match what {
            PEv::Gen { hi: h, lo: l, back: b } => {
                if let Some((g, bytes)) = last {
                    if bytes != (h, l, b) && f - LANE_LEAD <= g {
                        return Err(format!("generated notes {} frames apart at song frame {f}: the byte contract needs {}", f - g, LANE_LEAD + 1));
                    }
                }
                last = Some((f, (h, l, b)));
                hi.push((f - LANE_LEAD, h));
                lo.push((f - LANE_LEAD, l));
                back.push((f - LANE_LEAD, b));
                lane.push(((f - t) as i32, NOTE, Some(127)));
            }
            PEv::Off => lane.push(((f - t) as i32, NOTE, None)),
            other => unreachable!("a generated note is Gen or Off, not {other:?}"),
        }
        t = f;
    }
    let mut autos = vec![];
    for (param, pts) in [(o::TRACKING, hi), (o::PM_OUT, lo), (o::FEEDBACK, back)] {
        let id = pm::osc(0, param);
        let b0 = pts.first().map_or(0, |x| x.1);
        values[id] = b0 as f32 / 255.0;
        if pts.iter().any(|x| x.1 != b0) {
            autos.push(crate::model::Auto { device: 0, param: id, points: crate::cauldron::latched_lane(b0, &pts, LATCH_WINDOW) });
        }
    }
    Ok((lane, autos))
}

/// The generator map as a sample source of the shared flow (`xm::convert`): keys are the generated entries.
pub struct GenSource<'a> {
    pub map: &'a Map,
    /// One per distinct generated record: (record, engine).
    gens: Vec<(&'a Generated, &'static dyn Engine)>,
    /// Per map entry: its key (generated) or None.
    keys: Vec<Option<usize>>,
}

impl<'a> GenSource<'a> {
    /// The source for `xm` ([`check`]ed first).
    pub fn new(map: &'a Map, xm: &Xm) -> Result<Self, String> {
        check(map, xm)?;
        let (mut gens, mut keys): (Vec<(&Generated, &'static dyn Engine)>, Vec<Option<usize>>) = (vec![], vec![]);
        for e in &map.samples {
            keys.push(e.generated.as_ref().map(|g| match gens.iter().position(|(h, _)| *h == g) {
                Some(k) => k,
                None => {
                    gens.push((g, engine(&g.engine).expect("checked")));
                    gens.len() - 1
                }
            }));
        }
        Ok(GenSource { map, gens, keys })
    }
}

impl crate::xm::convert::Source for GenSource<'_> {
    fn ids(&self, xm: &Xm) -> Result<Vec<Vec<Option<(usize, usize)>>>, String> {
        let mut ids: Vec<Vec<Option<(usize, usize)>>> = xm.instruments.iter().map(|i| vec![None; i.samples.len()]).collect();
        for (e, k) in self.map.samples.iter().zip(&self.keys) {
            ids[e.instrument - 1][e.sample] = k.map(|k| (k, 0));
        }
        Ok(ids)
    }
    fn decimation(&self, _key: usize) -> u32 {
        1
    }
    fn echo_ms(&self, _key: usize, _ratio: f64) -> Option<f64> {
        None
    }
    fn chain(&self, _key: usize, _ratio: f64, _rate: f64, _tempo: i32, _slides: bool, _exact: bool) -> Chain {
        Chain { devices: vec![], latency: 0, level: 1.0, restart: None, echo_clamped: None, lowpass: None, lp_stages: 0 }
    }
    fn instrument(&self, key: usize, ratio: f64) -> [f32; unisynth::params::old::COUNT] {
        let (g, e) = self.gens[key];
        e.instrument(&g.params, crate::cauldron::R * ratio)
    }
    fn events(&self, key: usize, _variant: usize, ratio: f64, path: &Path) -> Vec<PhraseEv> {
        let (g, e) = self.gens[key];
        e.events(&g.params, crate::cauldron::R * ratio, path)
    }
    fn lanes(&self, key: usize, evs: Vec<(i64, PEv)>, values: &mut [f32; unisynth::params::old::COUNT]) -> Result<Lanes, String> {
        self.gens[key].1.lanes(evs, values)
    }
    /// Generated: `<label>` (`<label> (<class>)` off the native speed); stored: `<label> (sample)`. The label is the
    /// sample's name, else its instrument's, else `Instrument <n>` (the intros strip their names).
    fn track_name(&self, key: Option<usize>, class: i32, ins: &crate::xm::Instrument, instrument: usize, sample: usize) -> String {
        let label = [ins.samples[sample].name.trim(), ins.name.trim()].into_iter().find(|s| !s.is_empty()).map_or(format!("Instrument {}", instrument + 1), str::to_string);
        match (key, class) {
            (Some(_), 0) => label,
            (Some(_), c) => format!("{label} ({c:+})"),
            (None, _) => format!("{label} (sample)"),
        }
    }
    fn reverses(&self, key: usize) -> bool {
        let (g, e) = self.gens[key];
        !e.seekable(&g.params)
    }
}
