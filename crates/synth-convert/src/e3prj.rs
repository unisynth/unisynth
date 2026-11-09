//! Enigma Studio 3 projects (`.e3prj`, XML; change add-tf3-import): the Tunefish 3 songs and instruments a project
//! holds (`research/tunefish3-functional-spec.md` §5, §6.1). Only the elements the converter reads are parsed; every
//! problem is a `ConvError::Input` naming it.

use crate::ConvError;

/// Parameters per Tunefish 3 instrument.
pub const COUNT: usize = 129;

/// Public parameter names in index order (spec §5; projects and presets store values by them).
pub fn names() -> Vec<String> {
    let mut n: Vec<String> = vec!["OscPoints".into()];
    for i in 1..=4 {
        n.extend([format!("OscP{i}Off"), format!("OscP{i}Val"), format!("OscP{i}Int")]);
    }
    n.extend(
        [
            "OscFiInt", "OscVol", "OscFreq", "OscPan", "OscDetun", "OscPoly", "OscUnisn", "OscSpread", "OscSubOsc", "OscGlide",
            "OscDrive", "OscOctave", "OscOffset", "OscSlop", "AddVolume", "AddProfile", "AddOctave", "AddBandw", "AddDamp",
            "AddHarmncs", "AddScale", "AddDrive", "NoiseOn", "NoiseFreq", "NoiseBw", "FltLPOn", "FltLPCut", "FltLPRes", "FltHPOn",
            "FltHPCut", "FltHPRes", "FltBPOn", "FltBPCut", "FltBPQ", "FltNTOn", "FltNTCut", "FltNTQ",
        ]
        .map(String::from),
    );
    for i in 1..=2 {
        n.extend(["Att", "Dec", "Sus", "Rel", "Slp"].map(|f| format!("Env{i}{f}")));
    }
    for i in 1..=2 {
        n.extend(["Rate", "Dep", "Shp", "Sync"].map(|f| format!("Lfo{i}{f}")));
    }
    for i in 1..=9 {
        n.extend([format!("Mm{i}Source"), format!("Mm{i}Mod"), format!("Mm{i}Target")]);
    }
    n.extend(["Mm10Sourc", "Mm10Mod", "Mm10Targe"].map(String::from));
    n.extend((1..=10).map(|i| format!("Effect{i}")));
    n.extend(
        [
            "DistAmnt", "ChrsRate", "ChrsDep", "DelayL", "DelayR", "DelayDc", "RevRoom", "RevDamp", "RevWet", "RevWidth", "FlangLfo",
            "FlangFreq", "FlangAmp", "FlangWet", "GainAmount", "ChrsGain", "FrmtType", "FrmtWet", "EqLow", "EqMid", "EqHigh",
        ]
        .map(String::from),
    );
    debug_assert_eq!(n.len(), COUNT);
    n
}

/// Index of a public parameter name.
pub fn index(name: &str) -> Option<usize> {
    names().iter().position(|n| n == name)
}

/// One tracker cell: note byte (0 empty, 128 note-off, else MIDI note + 1), instrument (−1 none), velocity (−1 = 128),
/// effect word (high byte = MOD slot 1–4, low byte/128 = value).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub row: u32,
    pub note: u8,
    pub instrument: i32,
    pub velocity: i32,
    pub effect: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub id: u32,
    pub rows: u32,
    pub tracks: Vec<Vec<Cell>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
    pub row_offset: u32,
    pub track: u32,
    pub pattern: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    pub id: u32,
    pub name: String,
    pub bpm: u32,
    pub patterns: Vec<Pattern>,
    pub instances: Vec<Instance>,
}

impl Song {
    /// Length in rows: the end of the latest pattern instance.
    pub fn length_rows(&self) -> u32 {
        self.instances.iter().filter_map(|i| self.pattern(i.pattern).map(|p| i.row_offset + p.rows)).max().unwrap_or(0)
    }

    pub fn pattern(&self, id: u32) -> Option<&Pattern> {
        self.patterns.iter().find(|p| p.id == id)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Instrument {
    pub id: u32,
    pub name: String,
    pub params: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub version: String,
    pub songs: Vec<Song>,
    pub instruments: Vec<Instrument>,
}

impl Project {
    /// The playable song: `id`, or the first with patterns and instances.
    pub fn song(&self, id: Option<u32>) -> Result<&Song, ConvError> {
        let playable = |s: &&Song| !s.patterns.is_empty() && !s.instances.is_empty();
        match id {
            Some(id) => self.songs.iter().filter(playable).find(|s| s.id == id).ok_or_else(|| err(format!("no playable song with id {id}"))),
            None => self.songs.iter().find(playable).ok_or_else(|| err("the project has no playable song".into())),
        }
    }
}

fn err(m: String) -> ConvError {
    ConvError::Input(format!("e3prj: {m}"))
}

// ---------- minimal XML ----------

#[derive(Debug)]
struct El {
    tag: String,
    attrs: Vec<(String, String)>,
    kids: Vec<El>,
}

impl El {
    fn kid(&self, tag: &str) -> Option<&El> {
        self.kids.iter().find(|k| k.tag == tag)
    }
    fn kids<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a El> + 'a {
        self.kids.iter().filter(move |k| k.tag == tag)
    }
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|a| a.0 == name).map(|a| a.1.as_str())
    }
    fn int(&self, name: &str) -> Result<i64, ConvError> {
        let v = self.attr(name).ok_or_else(|| err(format!("<{}> without {name}", self.tag)))?;
        v.trim().parse().map_err(|_| err(format!("<{}> {name}=\"{v}\" is not a whole number", self.tag)))
    }
    fn uint(&self, name: &str, max: i64) -> Result<u32, ConvError> {
        let v = self.int(name)?;
        if v < 0 || v > max {
            return Err(err(format!("<{}> {name}={v} is out of range 0..={max}", self.tag)));
        }
        Ok(v as u32)
    }
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// The element tree of the XML subset Enigma Studio writes (declarations and comments skipped).
fn parse_xml(text: &str) -> Result<El, ConvError> {
    let mut stack = vec![El { tag: "#root".into(), attrs: vec![], kids: vec![] }];
    let mut i = 0;
    while let Some(off) = text[i..].find('<') {
        let start = i + off;
        if text[start..].starts_with("<?") || text[start..].starts_with("<!--") {
            let close = if text[start..].starts_with("<?") { "?>" } else { "-->" };
            i = start + text[start..].find(close).ok_or_else(|| err(format!("unclosed markup at byte {start}")))? + close.len();
            continue;
        }
        // the tag ends at the first `>` outside a quoted value (values may hold a raw `>`)
        let mut quoted = false;
        let end = start
            + text[start..]
                .bytes()
                .position(|c| {
                    if c == b'"' {
                        quoted = !quoted;
                    }
                    c == b'>' && !quoted
                })
                .ok_or_else(|| err(format!("unclosed tag at byte {start}")))?;
        let inner = &text[start + 1..end];
        i = end + 1;
        if let Some(name) = inner.strip_prefix('/') {
            let top = stack.pop().filter(|e| e.tag == name.trim()).ok_or_else(|| err(format!("unexpected </{}> at byte {start}", name.trim())))?;
            stack.last_mut().ok_or_else(|| err("unbalanced tags".into()))?.kids.push(top);
            continue;
        }
        let self_close = inner.ends_with('/');
        let inner = inner.trim_end_matches('/');
        let name_end = inner.find(|c: char| c.is_whitespace()).unwrap_or(inner.len());
        let tag = &inner[..name_end];
        if tag.is_empty() || !tag.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'.') {
            return Err(err(format!("bad tag at byte {start}")));
        }
        let mut attrs = vec![];
        let mut rest = &inner[name_end..];
        loop {
            rest = rest.trim_start();
            if rest.is_empty() {
                break;
            }
            let eq = rest.find('=').ok_or_else(|| err(format!("<{tag}>: attribute without a value")))?;
            let name = rest[..eq].trim().to_string();
            let after = rest[eq + 1..].trim_start();
            let q = after.strip_prefix('"').ok_or_else(|| err(format!("<{tag}> {name}: unquoted value")))?;
            let close = q.find('"').ok_or_else(|| err(format!("<{tag}> {name}: unclosed value")))?;
            attrs.push((name, unescape(&q[..close])));
            rest = &q[close + 1..];
        }
        let el = El { tag: tag.to_string(), attrs, kids: vec![] };
        if self_close {
            stack.last_mut().unwrap().kids.push(el);
        } else {
            stack.push(el);
        }
    }
    if stack.len() != 1 {
        return Err(err(format!("<{}> is not closed", stack.last().unwrap().tag)));
    }
    Ok(stack.pop().unwrap())
}

/// Reads a project. Instrument values start at 0 (the player's start; the editor stores every parameter) and take
/// every named value; an unknown parameter name is an error.
pub fn read(text: &str) -> Result<Project, ConvError> {
    let root = parse_xml(text)?;
    let doc = root.kid("enigma").ok_or_else(|| err("not an Enigma Studio project (no <enigma>)".into()))?;
    let names = names();
    let mut songs = vec![];
    if let Some(list) = doc.kid("songs") {
        for s in list.kids("song") {
            let mut patterns = vec![];
            for p in s.kid("patterns").into_iter().flat_map(|x| x.kids("pattern")) {
                let (id, rows, count) = (p.uint("patternid", 65535)?, p.uint("rowcount", 255)?, p.uint("trackcount", 32)?);
                let mut tracks = vec![];
                for t in p.kid("tracks").into_iter().flat_map(|x| x.kids("track")) {
                    let mut cells = vec![];
                    for n in t.kids("ne") {
                        let c = Cell {
                            row: n.uint("r", 255)?,
                            note: n.uint("n", 255)? as u8,
                            instrument: n.int("i")?.clamp(-1, 255) as i32,
                            velocity: n.int("v")?.clamp(-1, 255) as i32,
                            effect: n.uint("e", 65535)? as u16,
                        };
                        if c.row >= rows {
                            return Err(err(format!("pattern {id}: event at row {} of {rows}", c.row)));
                        }
                        cells.push(c);
                    }
                    tracks.push(cells);
                }
                if tracks.len() != count as usize {
                    return Err(err(format!("pattern {id}: trackcount {count} but {} tracks", tracks.len())));
                }
                patterns.push(Pattern { id, rows, tracks });
            }
            let mut instances = vec![];
            for i in s.kid("patterninstances").into_iter().flat_map(|x| x.kids("patterninstance")) {
                let inst = Instance { row_offset: i.uint("rowoffset", 65535)?, track: i.uint("track", 15)?, pattern: i.uint("patternid", 65535)? };
                if !patterns.iter().any(|p: &Pattern| p.id == inst.pattern) && !patterns.is_empty() {
                    return Err(err(format!("pattern instance at row {} on sequence track {} names missing pattern {}", inst.row_offset, inst.track, inst.pattern)));
                }
                instances.push(inst);
            }
            songs.push(Song { id: s.uint("id", u32::MAX as i64)?, name: s.attr("name").unwrap_or("").into(), bpm: s.uint("bpm", 1000)?, patterns, instances });
        }
    }
    let mut instruments = vec![];
    if let Some(list) = doc.kid("instruments") {
        for el in list.kids("instrument") {
            let id = el.uint("id", 31)?;
            let mut params = vec![0.0f32; COUNT];
            for p in el.kid("parameters").into_iter().flat_map(|x| x.kids("param")) {
                let name = p.attr("name").unwrap_or("");
                let k = names.iter().position(|n| n == name).ok_or_else(|| err(format!("instrument {id}: unknown parameter \"{name}\"")))?;
                let v = p.attr("value").unwrap_or("");
                params[k] = v.trim().parse().ok().filter(|x: &f32| x.is_finite()).ok_or_else(|| err(format!("instrument {id}: {name}=\"{v}\" is not a number")))?;
            }
            instruments.push(Instrument { id, name: el.attr("name").unwrap_or("").into(), params });
        }
    }
    Ok(Project { version: doc.attr("version").unwrap_or("").into(), songs, instruments })
}

/// One preset file (`String, Short` header, then `name;value` lines): 129 values from 0.
pub fn read_preset(text: &str) -> Result<Vec<f32>, ConvError> {
    let names = names();
    let mut params = vec![0.0f32; COUNT];
    for line in text.lines().skip(1) {
        let Some((name, value)) = line.split_once(';') else { continue };
        let k = names.iter().position(|n| n == name.trim()).ok_or_else(|| err(format!("preset: unknown parameter \"{}\"", name.trim())))?;
        params[k] = value.trim().parse().ok().filter(|x: &f32| x.is_finite()).ok_or_else(|| err(format!("preset: {name}=\"{value}\" is not a number")))?;
    }
    Ok(params)
}

/// Test and parity helper: writes a project in Enigma Studio's layout.
pub fn write(p: &Project) -> String {
    use std::fmt::Write;
    let names = names();
    let mut o = format!("<?xml version=\"1.0\"?>\n<enigma version=\"{}\">\n  <songs>\n", p.version);
    for s in &p.songs {
        let _ = writeln!(o, "    <song bpm=\"{}\" id=\"{}\" name=\"{}\">\n      <patterns>", s.bpm, s.id, s.name);
        for pt in &s.patterns {
            let _ = writeln!(o, "        <pattern rowcount=\"{}\" patternid=\"{}\" trackcount=\"{}\">\n          <tracks>", pt.rows, pt.id, pt.tracks.len());
            for t in &pt.tracks {
                o.push_str("            <track>\n");
                for c in t {
                    let _ = writeln!(o, "              <ne i=\"{}\" n=\"{}\" r=\"{}\" v=\"{}\" e=\"{}\"/>", c.instrument, c.note, c.row, c.velocity, c.effect);
                }
                o.push_str("            </track>\n");
            }
            o.push_str("          </tracks>\n        </pattern>\n");
        }
        o.push_str("      </patterns>\n      <patterninstances>\n");
        for i in &s.instances {
            let _ = writeln!(o, "        <patterninstance rowoffset=\"{}\" patternid=\"{}\" track=\"{}\"/>", i.row_offset, i.pattern, i.track);
        }
        o.push_str("      </patterninstances>\n    </song>\n");
    }
    o.push_str("  </songs>\n  <instruments>\n");
    for ins in &p.instruments {
        let _ = writeln!(o, "    <instrument id=\"{}\" name=\"{}\">\n      <parameters>", ins.id, ins.name);
        for (k, v) in ins.params.iter().enumerate() {
            let _ = writeln!(o, "        <param value=\"{v}\" name=\"{}\"/>", names[k]);
        }
        o.push_str("      </parameters>\n    </instrument>\n");
    }
    o.push_str("  </instruments>\n</enigma>\n");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        let mut params = vec![0.0f32; COUNT];
        params[index("OscVol").unwrap()] = 0.5;
        Project {
            version: "3.11b".into(),
            songs: vec![Song {
                id: 1,
                name: "t".into(),
                bpm: 140,
                patterns: vec![Pattern {
                    id: 0,
                    rows: 16,
                    tracks: vec![vec![
                        Cell { row: 0, note: 61, instrument: 0, velocity: -1, effect: 0 },
                        Cell { row: 4, note: 128, instrument: -1, velocity: -1, effect: 0 },
                    ]],
                }],
                instances: vec![Instance { row_offset: 0, track: 0, pattern: 0 }, Instance { row_offset: 16, track: 1, pattern: 0 }],
            }],
            instruments: vec![Instrument { id: 3, name: "bass".into(), params }],
        }
    }

    #[test]
    fn names_are_the_129_public_ones() {
        let n = names();
        assert_eq!(n.len(), COUNT);
        assert_eq!((n[0].as_str(), n[68].as_str(), n[97].as_str(), n[128].as_str()), ("OscPoints", "Mm1Source", "Mm10Targe", "EqHigh"));
    }

    #[test]
    fn a_written_project_reads_back() {
        let p = project();
        assert!(parse_xml("<a v=\"x>y\"/>").unwrap().kids[0].attr("v") == Some("x>y"), "a raw > in a value");
        let back = read(&write(&p)).unwrap();
        assert_eq!(back, p);
        assert_eq!(back.song(None).unwrap().length_rows(), 32);
    }

    #[test]
    fn a_missing_pattern_names_the_instance() {
        let text = write(&project()).replace("rowoffset=\"16\" patternid=\"0\"", "rowoffset=\"16\" patternid=\"9\"");
        let e = read(&text).unwrap_err().to_string();
        assert!(e.contains("missing pattern 9") && e.contains("sequence track 1"), "{e}");
    }

    #[test]
    fn unknown_names_bad_numbers_and_ranges_are_errors() {
        let w = write(&project());
        for (from, to, want) in [
            ("name=\"OscVol\"", "name=\"Nope\"", "unknown parameter \"Nope\""),
            ("value=\"0.5\"", "value=\"x\"", "not a number"),
            ("trackcount=\"1\"", "trackcount=\"2\"", "trackcount 2 but 1"),
            ("r=\"4\"", "r=\"16\"", "row 16 of 16"),
        ] {
            let e = read(&w.replacen(from, to, 1)).unwrap_err().to_string();
            assert!(e.contains(want), "{from} → {e}");
        }
        assert!(read(&w.replace("enigma", "other")).unwrap_err().to_string().contains("no <enigma>"));
        assert!(project().song(Some(9)).is_err());
    }

    #[test]
    fn truncation_at_every_byte_never_panics() {
        let w = write(&project());
        for n in 0..w.len() {
            if w.is_char_boundary(n) {
                let _ = read(&w[..n]);
            }
        }
    }

    #[test]
    fn presets_read_by_name() {
        let p = read_preset("String, Short\nOscPoints;1\nOscVol;0.25\n").unwrap();
        assert_eq!((p[0], p[index("OscVol").unwrap()]), (1.0, 0.25));
        assert!(read_preset("String, Short\nWhat;1\n").is_err());
    }
}

/// Opt-in: the Enigma-Studio-3 checkout's projects read (`$TF3_REF`, default `/tmp/tf3-ref`; absent → passes).
#[cfg(test)]
mod checkout {
    #[test]
    fn transplant_and_the_default_project_read() {
        let dir = std::env::var("TF3_REF").unwrap_or_else(|_| "/tmp/tf3-ref".into());
        for (rel, rows) in [("demos/transplant/transplant.e3prj", 2096), ("binary/default.e3prj", 64)] {
            let Ok(bytes) = std::fs::read(std::path::Path::new(&dir).join(rel)) else { return };
            let p = super::read(&String::from_utf8_lossy(&bytes)).unwrap();
            assert_eq!(p.song(None).unwrap().length_rows(), rows, "{rel}");
        }
    }
}
