//! Legacy → unified song converter (host only). `convert` takes a `synth-mix` song blob with legacy device kinds
//! and returns one with unified kinds (64–73): same tempo, rate, length, lanes, tracks, receives, volumes and
//! device order; each legacy device becomes one unified device in its place.

pub mod auto;
pub mod capacity;
pub mod cauldron;
pub mod e3prj;
pub mod elevated;
pub mod features;
pub mod fx;
pub mod instr;
pub mod legacy;
pub mod model;
pub mod patch;
pub mod rust;
pub mod tf;
pub mod tf3;
pub mod tf3song;
pub mod tfm;
pub mod v2conv;
pub mod v2m;
pub mod xm;
pub mod xmgen;

use model::{Auto, Dev, SongM};
use std::fmt;

#[derive(Debug)]
pub enum ConvError {
    /// The input is not a song this converter can read.
    Input(String),
    /// A legacy device kind the converter does not know or cannot map yet.
    Device(u8, String),
}

impl fmt::Display for ConvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvError::Input(m) => f.write_str(m),
            ConvError::Device(k, m) => write!(f, "device kind {k}: {m}"),
        }
    }
}

impl std::error::Error for ConvError {}

/// A converted song and what the converter wants to tell the user.
pub struct Converted {
    pub blob: Vec<u8>,
    pub warnings: Vec<String>,
}

/// Converts a legacy `synth-mix` song (WaveSabre kinds 0–12, Tunefish effect kinds 33–39).
pub fn convert_ws(blob: &[u8]) -> Result<Vec<u8>, ConvError> {
    convert(blob).map(|c| c.blob)
}

pub fn convert(blob: &[u8]) -> Result<Converted, ConvError> {
    let mut song = SongM::read(blob)?;
    let mut warnings = vec![];
    let mut devices = Vec::with_capacity(song.devices.len());
    // converted automation lanes of an instrument device, by (track, automation index)
    let mut lanes: std::collections::HashMap<(usize, usize), Vec<(usize, Vec<(i32, u8)>)>> = Default::default();
    // insert effects of Tunefish instruments: (instrument device, [(legacy effect kind, device)])
    let mut pending: Vec<(usize, Vec<(u8, Dev)>)> = vec![];
    for (di, d) in song.devices.iter().enumerate() {
        if legacy::is_wavesabre_instrument(d.kind) || legacy::is_tunefish_instrument(d.kind) {
            let (dev, l, chain) = instrument(&song, di, d, &mut warnings)?;
            lanes.extend(l);
            devices.push(dev);
            if !chain.is_empty() {
                pending.push((di, chain));
            }
        } else {
            devices.push(convert_device(d)?);
        }
    }
    let kinds: Vec<u8> = devices.iter().map(|d| d.kind).collect();
    // the insert chains follow their instrument in every track that lists it
    let mut chains: std::collections::HashMap<usize, Vec<(u8, usize)>> = Default::default();
    for (di, chain) in pending {
        let ids = chain
            .into_iter()
            .map(|(k, dev)| {
                devices.push(dev);
                (k, devices.len() - 1)
            })
            .collect();
        chains.insert(di, ids);
    }
    for (ti, t) in song.tracks.iter_mut().enumerate() {
        let old = std::mem::take(&mut t.devices);
        let mut at = Vec::with_capacity(old.len());
        for &x in &old {
            at.push(t.devices.len());
            t.devices.push(x);
            t.devices.extend(chains.get(&x).into_iter().flatten().map(|&(_, id)| id));
        }
        let mut out: Vec<Auto> = vec![];
        for (ai, a) in std::mem::take(&mut t.autos).into_iter().enumerate() {
            let di = old[a.device];
            let (kind, pos) = (kinds[di], at[a.device]);
            if let Some(l) = lanes.remove(&(ti, ai)) {
                for (param, pts) in l {
                    let mut prev = 0i32;
                    let points = pts
                        .into_iter()
                        .map(|(t, b)| {
                            let d = t.wrapping_sub(prev);
                            prev = t;
                            (d, b)
                        })
                        .collect();
                    out.push(Auto { device: pos, param, points });
                }
            } else if kind == unisynth::kind::INSTRUMENT {
                // a legacy parameter the instrument does not read: nothing to automate
            } else if (unisynth::kind::DISTORTION..=unisynth::kind::FORMANT).contains(&kind) {
                out.push(Auto { device: pos, param: a.param + 1, ..a });
                continue;
            } else {
                out.push(Auto { device: pos, ..a });
                continue;
            }
            // a Tunefish instrument's effect parameters drive its insert effects, whose parameters follow the mode
            for (k, &(tk, _)) in chains.get(&di).into_iter().flatten().enumerate() {
                if let Some(q) = tfsynth::device::effect_params(tk).iter().position(|&j| j == a.param) {
                    out.push(Auto { device: pos + 1 + k, param: q + 1, points: a.points.clone() });
                }
            }
        }
        t.autos = out;
    }
    song.devices = devices;
    Ok(Converted { blob: song.write()?, warnings })
}

fn convert_device(d: &Dev) -> Result<Dev, ConvError> {
    if let Some((k, mode)) = fx::target(d.kind) {
        return Ok(Dev { kind: k, chunk: fx::chunk(k, mode, &d.chunk) });
    }
    Err(ConvError::Device(
        d.kind,
        match d.kind {
            64..=73 => "already a unified device".into(),
            _ => "unknown device kind".into(),
        },
    ))
}

type Lanes = Vec<((usize, usize), Vec<(usize, Vec<(i32, u8)>)>)>;

/// Insert effects of a Tunefish instrument: `(legacy effect kind, unified device)` in slot order.
type Chain = Vec<(u8, Dev)>;

/// Converts instrument device `di`: its patch and the lanes of every automation that drives it.
fn instrument(song: &SongM, di: usize, d: &Dev, warnings: &mut Vec<String>) -> Result<(Dev, Lanes, Chain), ConvError> {
    let bad = |m: String| ConvError::Device(d.kind, m);
    let name = legacy::name(d.kind);
    let leg = legacy::decode(d.kind, &d.chunk).map_err(|e| bad(format!("{name} {di}: {e}")))?;
    let n = leg.params.len();
    // the tracks using this device and their automations of it
    let mut users = vec![];
    for (ti, t) in song.tracks.iter().enumerate() {
        for (pos, &x) in t.devices.iter().enumerate() {
            if x == di {
                users.push((ti, pos));
            }
        }
    }
    let autos: Vec<(usize, usize, &Auto)> = users
        .iter()
        .flat_map(|&(ti, pos)| {
            song.tracks[ti].autos.iter().enumerate().filter(move |(_, a)| a.device == pos).map(move |(ai, a)| (ti, ai, a))
        })
        .filter(|(_, _, a)| a.param < n && !a.points.is_empty())
        .collect();
    let mut automated = vec![false; n];
    for (_, _, a) in &autos {
        automated[a.param] = true;
    }
    let tunefish = legacy::is_tunefish_instrument(d.kind);
    if tunefish {
        // the mod matrix and the effect slots are the layout of the patch: they cannot move during a song
        for (_, _, a) in &autos {
            let slot = a.param.wrapping_sub(tfsynth::params::MM_FIRST) / 3;
            let routing = (tfsynth::params::MM_FIRST..tfsynth::params::EFFECT_FIRST).contains(&a.param) && slot < tfsynth::modmx::LIVE;
            if (routing && (a.param - tfsynth::params::MM_FIRST) % 3 != 1)
                || (tfsynth::params::EFFECT_FIRST..tfsynth::params::EFFECT_FIRST + tfsynth::fx::SLOTS).contains(&a.param)
            {
                return Err(bad(format!(
                    "{name} {di}: parameter {} (a mod slot's source or target, or an effect slot's kind) is automated; \
                     the unified instrument cannot change its routing during a song",
                    a.param
                )));
            }
        }
    }
    // capacity
    let roles = legacy::roles(d.kind);
    let reach = |idx: Option<usize>, f: &dyn Fn(f32) -> f32| -> f32 {
        let Some(i) = idx else { return 0.0 };
        let mut m = f(leg.params[i]);
        for (_, _, a) in autos.iter().filter(|(_, _, a)| a.param == i) {
            for &(_, b) in &a.points {
                m = m.max(f(auto::value(b)));
            }
        }
        m
    };
    let unison = reach(roles.unison, &|p| ((p.max(0.0) * 15.0) as u32).saturating_add(1) as f32) as u32;
    let unison = unison.max(1);
    let legato = roles.mode.is_some_and(|i| leg.params[i] as i32 == 1 && !automated[i]);
    let release_ms = reach(roles.release, &|p| p * p * 5000.0 + 1.0);
    let rate = song.rate as f64;
    let life = if d.kind == wavesabre::kind::THUNDER {
        capacity::Life::Fixed(leg.sample_frames as i64)
    } else {
        capacity::Life::Note { tail: (release_ms as f64 * rate / 1000.0).ceil().min(1e12) as i64 }
    };
    let (mut cap, mut uncapped) = (1, 1);
    let lanes: Vec<&[model::Ev]> = users.iter().map(|&(ti, _)| song.lanes[song.tracks[ti].lane].as_slice()).collect();
    if tunefish {
        // the voice limit, stealing the oldest
        cap = reach(Some(tfsynth::params::GEN_POLY), &|p| tfsynth::params::polyphony(p) as f32) as u32;
        uncapped = cap;
        for w in tf::warnings(&leg.params) {
            warnings.push(format!("device {di} ({name}): {w}"));
        }
    } else if !lanes.is_empty() {
        (cap, uncapped) = capacity::analyse(&lanes, &life, unison, legato);
    }
    if unison > 16 && !tunefish {
        warnings.push(format!(
            "device {di} ({name}): the unison parameter asks for {unison} voices per note, the unified instrument has at most 16"
        ));
    }
    if uncapped > capacity::MAX {
        warnings.push(format!(
            "device {di} ({name}): the song needs up to {uncapped} voices, the unified instrument has {}; \
             the oldest voices will be stolen",
            capacity::MAX
        ));
    }
    let cx = instr::Cx { automated, capacity: cap };
    let patch = instr::build(&leg, &cx).finish();
    let samples: Vec<&[u8]> = leg.sample.iter().map(|s| s.as_slice()).collect();
    let chunk = unisynth::migrate::encode_old(&patch, &samples, &[]);
    // automation lanes
    let mut out: Lanes = vec![];
    let choices = instr::discrete_shapes(d.kind);
    let mut driver: std::collections::HashMap<usize, usize> = Default::default();
    for (ti, ai, a) in autos {
        let eval = |v: f32| {
            let mut p = leg.params.clone();
            p[a.param] = v;
            instr::build(&legacy::Legacy { params: p, ..leg.clone() }, &cx).finish()
        };
        let probe = |b: u8| eval(auto::value(b));
        let affected = auto::affected(&probe);
        let mut t = 0i32;
        let pts: Vec<(i32, u8)> = a
            .points
            .iter()
            .map(|&(dt, b)| {
                t = t.wrapping_add(dt);
                (t, b)
            })
            .collect();
        for &j in &affected {
            if let Some(&other) = driver.get(&j) {
                if other != a.param {
                    warnings.push(format!(
                        "device {di} ({name}): parameters {other} and {} both drive unified parameter {j}; \
                         their automations are not combined",
                        a.param
                    ));
                }
            }
            driver.insert(j, a.param);
        }
        let l = auto::convert(&pts, &affected, &choices, &eval);
        out.push(((ti, ai), l.into_iter().map(|l| (l.param, l.points)).collect()));
    }
    let chain = if tunefish { tf::insert_chain(&leg.params) } else { vec![] };
    Ok((Dev { kind: unisynth::kind::INSTRUMENT, chunk }, out, chain))
}
