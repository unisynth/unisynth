//! `features`: the cargo features of `unisynth` a converted song needs, so a compo build links only those modules.

use crate::model::SongM;
use crate::{auto, ConvError};
use std::collections::{BTreeMap, HashMap};
use unisynth::needs;

/// The feature names (`us-*`) of the modules the song's devices use, and of every module its automation can switch
/// on (a filter type or envelope mode a lane steps to, noise a lane raises), in module order. Devices that are not
/// unified kinds are an error: convert the song first.
pub fn features(blob: &[u8]) -> Result<Vec<&'static str>, ConvError> {
    Ok(explain(blob, None)?.into_iter().map(|e| e.feature).collect())
}

/// One needed feature, what calls for it and (when a cost table is given) what it adds to the build.
#[derive(Debug, Clone, PartialEq)]
pub struct Need {
    pub feature: &'static str,
    /// One line per cause, `instrument device 3: osc1.type is phase-mod`.
    pub why: Vec<String>,
    /// Marginal code size in bytes, raw and `xz -9e`, from `research/us-size-probe/marginal.sh`.
    pub cost: Option<(i64, i64)>,
}

const EFFECTS: [&str; 9] = ["distortion", "crusher", "EQ", "delay", "chorus", "flanger", "reverb", "compressor", "formant"];

/// The marginal cost of each leaf feature from the `feature,raw,xz` table `marginal.sh` writes (row `all` is the
/// build with every module; a feature's cost is the saving when it is removed).
pub fn read_cost(table: &str) -> HashMap<String, (i64, i64)> {
    let rows: Vec<(&str, i64, i64)> = table
        .lines()
        .skip(1)
        .filter_map(|l| {
            let mut c = l.split(',');
            Some((c.next()?, c.next()?.trim().parse().ok()?, c.next()?.trim().parse().ok()?))
        })
        .collect();
    let Some(all) = rows.iter().find(|r| r.0 == "all") else { return HashMap::new() };
    rows.iter().filter(|r| r.0 != "all").map(|r| (r.0.to_string(), (all.1 - r.1, all.2 - r.2))).collect()
}

/// [`features`] with the causes, in module order. `cost` is the table from [`read_cost`].
pub fn explain(blob: &[u8], cost: Option<&HashMap<String, (i64, i64)>>) -> Result<Vec<Need>, ConvError> {
    let song = SongM::read(blob)?;
    let mut why: BTreeMap<u8, Vec<String>> = BTreeMap::new();
    for (di, d) in song.devices.iter().enumerate() {
        let bad = |what: &str| ConvError::Device(d.kind, format!("{what}; convert the song with `ws` or `tfm` first"));
        if d.kind == unisynth::kind::INSTRUMENT {
            // the span of every automated parameter of this device, over every track that lists it
            let mut spans: HashMap<usize, (f32, f32)> = Default::default();
            for t in &song.tracks {
                for a in t.autos.iter().filter(|a| t.devices.get(a.device) == Some(&di)) {
                    for &(_, b) in &a.points {
                        let x = auto::value(b);
                        let e = spans.entry(a.param).or_insert((x, x));
                        *e = (e.0.min(x), e.1.max(x));
                    }
                }
            }
            let used = needs::instrument_why(&d.chunk, &|i| spans.get(&i).copied())
                .map_err(|e| ConvError::Device(d.kind, format!("not a unified instrument chunk: {e:?}")))?;
            for (id, reasons) in used {
                why.entry(id).or_default().extend(reasons.into_iter().map(|r| format!("instrument device {di}: {r}")));
            }
        } else if unisynth::fx::is_effect(d.kind) {
            if let Some(id) = needs::effect(d.kind, &d.chunk) {
                let name = EFFECTS.get((d.kind - unisynth::kind::INSTRUMENT - 1) as usize).copied().unwrap_or("effect");
                let mode = unisynth::fx::mode_of(d.kind, &d.chunk);
                why.entry(id).or_default().push(format!("device {di}: {name} (kind {}), mode {mode}", d.kind));
            }
        } else {
            return Err(bad("not a unified device"));
        }
    }
    Ok(why
        .into_iter()
        .map(|(id, why)| {
            let feature = needs::feature(id);
            Need { feature, why, cost: cost.and_then(|c| c.get(feature).copied()) }
        })
        .collect())
}

/// Causes printed per feature by `--explain`.
const SHOWN: usize = 3;

fn kb(n: i64) -> String {
    format!("{:.1} K", n as f64 / 1024.0)
}

/// The text of `features --explain`.
pub fn render_text(needs: &[Need]) -> String {
    let mut o = String::new();
    for n in needs {
        o += n.feature;
        if let Some((raw, xz)) = n.cost {
            o += &format!("   +{} raw, +{} xz", kb(raw), kb(xz));
        }
        o += "\n";
        for w in n.why.iter().take(SHOWN) {
            o += &format!("    {w}\n");
        }
        if n.why.len() > SHOWN {
            o += &format!("    … and {} more (`--json` lists all)\n", n.why.len() - SHOWN);
        }
    }
    let known: Vec<_> = needs.iter().filter_map(|n| n.cost).collect();
    if !known.is_empty() {
        o += &format!(
            "marginal cost of the listed features (each measured with every other module present): +{} raw, +{} xz\n",
            kb(known.iter().map(|c| c.0).sum()),
            kb(known.iter().map(|c| c.1).sum())
        );
    }
    o
}

fn quote(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o += "\\\"",
            '\\' => o += "\\\\",
            c if (c as u32) < 0x20 => o += &format!("\\u{:04x}", c as u32),
            c => o.push(c),
        }
    }
    o + "\""
}

/// The JSON of `features --json`: `[{"feature", "why": [...], "raw", "xz"}]` (`raw`/`xz` null without a cost table).
pub fn render_json(needs: &[Need]) -> String {
    let items: Vec<String> = needs
        .iter()
        .map(|n| {
            let why: Vec<String> = n.why.iter().map(|w| quote(w)).collect();
            let (raw, xz) = match n.cost {
                Some((r, x)) => (r.to_string(), x.to_string()),
                None => ("null".into(), "null".into()),
            };
            format!("{{\"feature\":{},\"why\":[{}],\"raw\":{raw},\"xz\":{xz}}}", quote(n.feature), why.join(","))
        })
        .collect();
    format!("[{}]\n", items.join(",\n "))
}
