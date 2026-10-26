//! Semantic validation of a parsed song: every problem with its JSON pointer and reason (spec "Validation with
//! paths"). Export refuses a song with any issue.

use crate::model::*;
use crate::tables::{self, Table};
use crate::Issue;
use std::collections::{HashMap, HashSet};

struct Cx {
    out: Vec<Issue>,
}

impl Cx {
    fn err(&mut self, path: impl Into<String>, msg: impl Into<String>) {
        self.out.push(Issue { path: path.into(), message: msg.into() });
    }
}

/// JSON-pointer escape of one segment (`~` → `~0`, `/` → `~1`).
pub(crate) fn seg(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

/// Largest song length in ticks whose sample positions fit the blob's i32 times.
fn max_ticks(s: &Song) -> f64 {
    i32::MAX as f64 * s.tempo * TICKS_PER_BEAT as f64 / (60.0 * s.rate as f64)
}

pub fn validate(s: &Song) -> Vec<Issue> {
    let mut cx = Cx { out: vec![] };
    if s.format != FORMAT {
        cx.err("/format", format!("expected \"{FORMAT}\", found \"{}\"", s.format));
    }
    if !(8000..=192_000).contains(&s.rate) {
        cx.err("/rate", format!("sample rate {} out of range 8000..192000", s.rate));
    }
    if !s.tempo.is_finite() || !(20.0..=999.0).contains(&s.tempo) {
        cx.err("/tempo", format!("tempo {} out of range 20..999", s.tempo));
    }
    if !(1..=32).contains(&s.signature[0]) || ![1, 2, 4, 8, 16, 32].contains(&s.signature[1]) {
        cx.err("/signature", format!("time signature {}/{} is not supported", s.signature[0], s.signature[1]));
    }
    if s.ticks_per_beat != TICKS_PER_BEAT {
        cx.err("/ticks_per_beat", format!("ticks per beat must be {TICKS_PER_BEAT}"));
    }
    if let Some(m) = &s.meta {
        for (name, v) in [("author", &m.author), ("description", &m.description), ("copyright", &m.copyright), ("url", &m.url)] {
            if v.trim().is_empty() {
                cx.err(format!("/meta/{name}"), format!("{name} is empty"));
            }
        }
        if !m.url.trim().is_empty() && !(m.url.starts_with("http://") || m.url.starts_with("https://")) {
            cx.err("/meta/url", "url must start with http:// or https://");
        }
        if let Some(from) = &m.adapted_from {
            if from.trim().is_empty() {
                cx.err("/meta/adapted_from", "adapted_from is empty; name the original work");
            }
            if !m.description.to_lowercase().contains("adapted") {
                cx.err("/meta/description", "a work with adapted_from must say in its description that it is adapted");
            }
        }
    }
    if let Some(a) = &s.about {
        about(&mut cx, "/about", a);
    }
    let header_ok = cx.out.iter().all(|i| i.path != "/rate" && i.path != "/tempo");
    if s.length < 0 || (header_ok && s.length as f64 > max_ticks(s)) {
        cx.err("/length", format!("song length {} ticks out of range", s.length));
    }

    let mut clips: HashMap<&str, &Clip> = HashMap::new();
    for (ci, c) in s.clips.iter().enumerate() {
        let p = format!("/clips/{ci}");
        if c.id.is_empty() {
            cx.err(format!("{p}/id"), "clip id is empty");
        } else if clips.insert(&c.id, c).is_some() {
            cx.err(format!("{p}/id"), format!("duplicate clip id \"{}\"", c.id));
        }
        if c.length <= 0 {
            cx.err(format!("{p}/length"), "clip length must be at least 1 tick");
        }
        for (ni, n) in c.notes.iter().enumerate() {
            let np = format!("{p}/notes/{ni}");
            if n.pitch > 127 {
                cx.err(&np, format!("pitch {} out of range 0..127", n.pitch));
            }
            if !(1..=127).contains(&n.vel) {
                cx.err(&np, format!("velocity {} out of range 1..127", n.vel));
            }
            if n.len < 1 {
                cx.err(&np, "note length must be at least 1 tick");
            }
            if n.start < 0 || n.start >= c.length {
                cx.err(&np, format!("note at tick {} is outside the clip (0..{})", n.start, c.length));
            }
        }
        for (name, env) in [("wheel", &c.wheel), ("bend", &c.bend)] {
            points(&mut cx, &format!("{p}/{name}"), env, 0.0, c.length as f64, "outside the clip");
        }
    }

    // track ids
    let all: Vec<(String, &Track)> =
        s.tracks.iter().enumerate().map(|(i, t)| (format!("/tracks/{i}"), t)).chain([("/master".to_string(), &s.master)]).collect();
    let mut ids: HashMap<&str, usize> = HashMap::new();
    for (k, (p, t)) in all.iter().enumerate() {
        if t.id.is_empty() {
            cx.err(format!("{p}/id"), "track id is empty");
        } else if ids.insert(&t.id, k).is_some() {
            cx.err(format!("{p}/id"), format!("duplicate track id \"{}\"", t.id));
        }
    }
    let master = all.len() - 1;
    for (k, (p, t)) in all.iter().enumerate() {
        track(&mut cx, s, p, t, &clips);
        for (ri, r) in t.receives.iter().enumerate() {
            let rp = format!("{p}/receives/{ri}");
            match ids.get(r.from.as_str()) {
                None => cx.err(format!("{rp}/from"), format!("unknown track \"{}\"", r.from)),
                Some(&m) if m == master => cx.err(format!("{rp}/from"), "the master cannot send; route through a bus before the master"),
                Some(&m) if m == k => cx.err(format!("{rp}/from"), "a track cannot receive from itself"),
                _ => {}
            }
            if !r.volume.is_finite() || r.volume < 0.0 {
                cx.err(format!("{rp}/volume"), format!("send volume {} must be a finite number ≥ 0", r.volume));
            }
        }
    }
    cycles(&mut cx, &all, &ids);
    cx.out
}

/// `type` and `description` not blank, no blank optional text.
fn about(cx: &mut Cx, p: &str, a: &About) {
    for (name, v) in [("type", Some(&a.kind)), ("description", Some(&a.description)), ("style", a.style.as_ref()), ("tempo", a.tempo.as_ref()), ("engine", a.engine.as_ref())] {
        if v.is_some_and(|v| v.trim().is_empty()) {
            cx.err(format!("{p}/{name}"), format!("{name} is empty"));
        }
    }
    for (i, v) in a.similar_to.iter().enumerate() {
        if v.trim().is_empty() {
            cx.err(format!("{p}/similar_to/{i}"), "empty name");
        }
    }
}

fn points(cx: &mut Cx, base: &str, pts: &[Point], lo: f64, hi: f64, outside: &str) {
    let mut prev = f64::NEG_INFINITY;
    for (i, pt) in pts.iter().enumerate() {
        let pp = format!("{base}/{i}");
        if !pt.tick.is_finite() || !pt.value.is_finite() {
            cx.err(&pp, "point is not a finite number");
            continue;
        }
        if !(0.0..=1.0).contains(&pt.value) {
            cx.err(&pp, format!("value {} out of range 0..1", pt.value));
        }
        if pt.tick < lo || pt.tick > hi {
            cx.err(&pp, format!("point at tick {} is {outside}", pt.tick));
        }
        if pt.tick < prev {
            cx.err(&pp, "points out of order");
        }
        prev = pt.tick;
    }
}

fn device(cx: &mut Cx, p: &str, d: &Device) -> Option<&'static Table> {
    let k = d.kind;
    if k <= 39 {
        cx.err(format!("{p}/kind"), format!("kind {k} is a legacy device; convert the song with synth-convert first"));
        return None;
    }
    let Some(t) = tables::of_device(d) else {
        cx.err(format!("{p}/kind"), format!("unknown device kind {k}"));
        return None;
    };
    for (name, v) in d.values.iter() {
        let vp = format!("{p}/values/{}", seg(name));
        let Some(par) = t.param(name) else {
            cx.err(vp, "unknown parameter");
            continue;
        };
        let (lo, hi) = par.range();
        if !v.is_finite() || v < lo || v > hi {
            cx.err(vp, format!("value {v} out of range {lo}..{hi}"));
        } else if par.raw && v.fract() != 0.0 {
            cx.err(vp, format!("value {v} must be a whole number"));
        }
    }
    if !d.samples.is_empty() && k != unisynth::kind::INSTRUMENT {
        cx.err(format!("{p}/samples"), "only the instrument holds samples");
    }
    if d.samples.len() > unisynth::chunk::MAX_SAMPLES {
        cx.err(format!("{p}/samples"), format!("at most {} samples", unisynth::chunk::MAX_SAMPLES));
    }
    for (i, b) in d.samples.iter().enumerate() {
        if crate::b64::decode(b).is_none() {
            cx.err(format!("{p}/samples/{i}"), "not valid base64");
        }
    }
    if d.trailer.is_some() && !unisynth::fx::is_effect(k) {
        cx.err(format!("{p}/trailer"), "only effects have a chunk trailer");
    }
    Some(t)
}

fn track(cx: &mut Cx, s: &Song, p: &str, t: &Track, clips: &HashMap<&str, &Clip>) {
    if !t.volume.is_finite() || t.volume < 0.0 {
        cx.err(format!("{p}/volume"), format!("volume {} must be a finite number ≥ 0", t.volume));
    }
    if let Some(a) = &t.about {
        about(cx, &format!("{p}/about"), a);
    }
    let tabs: Vec<Option<&Table>> = t.devices.iter().enumerate().map(|(di, d)| device(cx, &format!("{p}/devices/{di}"), d)).collect();
    for (pi, pl) in t.placements.iter().enumerate() {
        let pp = format!("{p}/placements/{pi}");
        let Some(c) = clips.get(pl.clip.as_str()) else {
            cx.err(format!("{pp}/clip"), format!("unknown clip \"{}\"", pl.clip));
            continue;
        };
        if pl.start < 0 {
            cx.err(format!("{pp}/start"), "placement starts before the song");
        }
        let len = pl.length.unwrap_or(c.length);
        if len <= 0 {
            cx.err(format!("{pp}/length"), "placement length must be at least 1 tick");
        }
        if pl.start.saturating_add(len) > s.length {
            cx.err(&pp, format!("placement ends at tick {} after the song ({})", pl.start.saturating_add(len), s.length));
        }
        for (env, ctl) in [(&c.wheel, "ctl.wheel"), (&c.bend, "ctl.bend")] {
            if env.is_empty() {
                continue;
            }
            let inst = t.devices.iter().zip(&tabs).find(|(d, _)| d.kind == unisynth::kind::INSTRUMENT);
            match inst {
                Some((_, Some(tab))) if tab.param(ctl).is_some() => {
                    let di = t.devices.iter().position(|d| d.kind == unisynth::kind::INSTRUMENT).unwrap_or(0);
                    if let Some(li) = t.lanes.iter().position(|l| l.device == di && l.param == ctl) {
                        let what = ctl.trim_start_matches("ctl.");
                        cx.err(format!("{p}/lanes/{li}"), format!("{what} is automated by both the track and clip {}", c.id));
                    }
                }
                Some(_) => cx.err(&pp, format!("clip {} has a {ctl} envelope but the instrument has no {ctl} parameter (unisynth controller block)", c.id)),
                None => cx.err(&pp, format!("clip {} has a {ctl} envelope but the track has no instrument", c.id)),
            }
        }
    }
    let mut seen: HashSet<(usize, &str)> = HashSet::new();
    for (li, l) in t.lanes.iter().enumerate() {
        let lp = format!("{p}/lanes/{li}");
        match tabs.get(l.device) {
            None => cx.err(format!("{lp}/device"), format!("no device {} on this track", l.device)),
            Some(None) => {}
            Some(Some(tab)) => match tab.param(&l.param) {
                None => cx.err(format!("{lp}/param"), format!("unknown parameter \"{}\"", l.param)),
                Some(par) if par.structural => cx.err(format!("{lp}/param"), format!("structural parameter {} cannot be automated", l.param)),
                Some(_) => {
                    if !seen.insert((l.device, &l.param)) {
                        cx.err(&lp, "parameter already automated on this track");
                    }
                }
            },
        }
        points(cx, &format!("{lp}/points"), &l.points, 0.0, s.length as f64, "outside the song");
    }
}

/// A receive cycle: report each receive whose sending track (transitively) receives from the receiving track.
fn cycles(cx: &mut Cx, all: &[(String, &Track)], ids: &HashMap<&str, usize>) {
    let from = |k: usize| all[k].1.receives.iter().filter_map(|r| ids.get(r.from.as_str()).copied());
    let reaches = |a: usize, b: usize| {
        // does track a receive (transitively) from track b?
        let mut stack = vec![a];
        let mut seen = vec![false; all.len()];
        while let Some(x) = stack.pop() {
            for y in from(x) {
                if y == b {
                    return true;
                }
                if !seen[y] {
                    seen[y] = true;
                    stack.push(y);
                }
            }
        }
        false
    };
    for (k, (p, t)) in all.iter().enumerate() {
        for (ri, r) in t.receives.iter().enumerate() {
            if let Some(&m) = ids.get(r.from.as_str()) {
                if m != k && reaches(m, k) {
                    cx.err(format!("{p}/receives/{ri}"), format!("receive cycle: {} and {} feed each other", t.id, r.from));
                }
            }
        }
    }
}
