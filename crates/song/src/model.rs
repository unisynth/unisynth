//! The `song/1` source model (serde, unknown fields rejected). Times are ticks (960 per beat); values are normalized
//! 0..1 keyed by parameter name (raw sampler indices 0..65535). See `schema/song.schema.json`.

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const FORMAT: &str = "song/1";
pub const TICKS_PER_BEAT: u32 = 960;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Song {
    pub format: String,
    /// Who made the song and under which terms; a work adapted from another says so (see [`Meta`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
    /// What the song is (genre, style, tempo); not part of the blob.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<About>,
    /// Sample rate in Hz.
    pub rate: u32,
    /// Beats per minute (one tempo for the whole song).
    pub tempo: f64,
    /// Time signature, display only: `[beats per bar, beat unit]`.
    pub signature: [u32; 2],
    pub ticks_per_beat: u32,
    /// Song length in ticks.
    pub length: i64,
    pub clips: Vec<Clip>,
    pub tracks: Vec<Track>,
    /// The end of the graph: receives from tracks, may hold effects, never sends.
    pub master: Track,
}

/// Authorship of a song. Not part of the blob (the player never sees it); it travels with the JSON source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub author: String,
    /// What the song is. An adapted work (`adapted_from` set) must say it is adapted here.
    pub description: String,
    pub copyright: String,
    /// SPDX identifier of the terms the song may be used under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Where the song or its original comes from (`http`/`https`).
    pub url: String,
    /// The original work this song was converted or arranged from (title, format, authors).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapted_from: Option<String>,
}

/// A description of a song or a track for people and tools (the DAW, a librarian, an LLM). Not part of the blob.
/// For a song `type` is the genre; for a track its role (kick, bass, lead, pad, …).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct About {
    #[serde(rename = "type")]
    pub kind: String,
    /// Sound or musical style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// Song: BPM and feel; track: rhythm (for example "16th-note arpeggio", "four on the floor").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tempo: Option<String>,
    /// The synth engine and how it makes the sound (for example "Falcon: 2-operator FM").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    /// Real-world instruments it resembles (synthesizers, drum machines, artists for a song).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub similar_to: Vec<String>,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    pub id: String,
    /// Length in ticks; notes start inside it.
    pub length: i64,
    pub notes: Vec<Note>,
    /// Mod-wheel envelope (tick within the clip, value 0..1), flattened into `ctl.wheel` automation on export.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wheel: Vec<Point>,
    /// Pitch-bend envelope (centre 0.5), flattened into `ctl.bend` automation on export.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bend: Vec<Point>,
}

/// A note: `[start tick, length ticks, MIDI pitch, velocity]`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "(i64, i64, u8, u8)", into = "(i64, i64, u8, u8)")]
pub struct Note {
    pub start: i64,
    pub len: i64,
    pub pitch: u8,
    pub vel: u8,
}

impl From<(i64, i64, u8, u8)> for Note {
    fn from((start, len, pitch, vel): (i64, i64, u8, u8)) -> Self {
        Note { start, len, pitch, vel }
    }
}
impl From<Note> for (i64, i64, u8, u8) {
    fn from(n: Note) -> Self {
        (n.start, n.len, n.pitch, n.vel)
    }
}

/// An automation or envelope point: `[tick, value]`. Ticks may be fractional (import keeps exact sample times).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(from = "(f64, f32)")]
pub struct Point {
    pub tick: f64,
    pub value: f32,
}

impl From<(f64, f32)> for Point {
    fn from((tick, value): (f64, f32)) -> Self {
        Point { tick, value }
    }
}
impl Serialize for Point {
    /// `[tick, value]`; a whole tick is written as an integer so saved files keep `[3840, 0.5]`.
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeTuple;
        let mut t = s.serialize_tuple(2)?;
        if self.tick.fract() == 0.0 && self.tick.abs() < 9.0e15 {
            t.serialize_element(&(self.tick as i64))?;
        } else {
            t.serialize_element(&self.tick)?;
        }
        t.serialize_element(&self.value)?;
        t.end()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Track {
    pub id: String,
    pub name: String,
    /// What the track is (role, sound, similar instruments); not part of the blob.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<About>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Linear gain applied after the device chain.
    pub volume: f32,
    /// Muted tracks export with volume 0 and no events.
    pub mute: bool,
    pub receives: Vec<Receive>,
    pub devices: Vec<Device>,
    pub placements: Vec<Placement>,
    pub lanes: Vec<Lane>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Main,
    Side,
}

/// A post-fader send from track `from` into this track (main input or sidechain).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receive {
    pub from: String,
    pub channel: Channel,
    pub volume: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    /// Device kind: 64 instrument, 65–73 effects.
    pub kind: u8,
    /// Values that differ from the table default, keyed by parameter name. Effects: `mode` selects the parameter set.
    pub values: Values,
    /// GSM sample entries (base64), kind 64 only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<String>,
    /// Bypassed devices are kept in the source and left out of the export.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
    /// Preset name shown in the editor (not exported).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Effect chunk trailer when an imported chunk's differs from the canonical one (its byte size).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trailer: Option<i32>,
}

/// One placement of a clip on a track. A looping placement repeats the clip every clip length until its end; a
/// non-looping one plays it once, cut at its end. `length` defaults to the clip length.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub clip: String,
    pub start: i64,
    pub length: Option<i64>,
    #[serde(rename = "loop")]
    pub looping: bool,
}

/// Automation of one device parameter on a track (at most one lane per device and parameter).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lane {
    /// Index into the track's devices.
    pub device: usize,
    pub param: String,
    pub points: Vec<Point>,
}

/// Parameter values keyed by name, in file order (so a preset can store them sorted by index).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Values(pub Vec<(String, f32)>);

impl Values {
    pub fn get(&self, name: &str) -> Option<f32> {
        self.0.iter().find(|(n, _)| n == name).map(|e| e.1)
    }

    /// Sets `name` (appends when absent).
    pub fn set(&mut self, name: &str, v: f32) {
        match self.0.iter_mut().find(|(n, _)| n == name) {
            Some(e) => e.1 = v,
            None => self.0.push((name.to_string(), v)),
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.0.retain(|(n, _)| n != name);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, f32)> {
        self.0.iter().map(|(n, v)| (n.as_str(), *v))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Serialize for Values {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            m.serialize_entry(k, v)?;
        }
        m.end()
    }
}

impl<'de> Deserialize<'de> for Values {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Values;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an object of parameter name → number")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Values, A::Error> {
                let mut out = Vec::new();
                while let Some((k, v)) = m.next_entry::<String, f32>()? {
                    if out.iter().any(|(n, _): &(String, f32)| *n == k) {
                        return Err(serde::de::Error::custom(format!("duplicate parameter {k}")));
                    }
                    out.push((k, v));
                }
                Ok(Values(out))
            }
        }
        d.deserialize_map(V)
    }
}
