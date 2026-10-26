//! An owned, editable `synth-mix` song: read with `synth_core::mix::Song` (which validates everything), written back
//! in the same binary layout. Automation points keep their byte values.

use crate::ConvError;
use synth_core::device::EventKind;
use synth_core::mix::Song;

#[derive(Clone, Debug, PartialEq)]
pub struct Dev {
    pub kind: u8,
    pub chunk: Vec<u8>,
}

/// A lane event: `(delta frames, note, Some(velocity) for a note-on or None for a note-off)`.
pub type Ev = (i32, u8, Option<u8>);

/// One automation: `device` indexes the track's device list; points are `(delta frames, value byte)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Auto {
    pub device: usize,
    pub param: usize,
    pub points: Vec<(i32, u8)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Trk {
    pub volume: f32,
    /// `(source track, channel 0 main | 2 sidechain, volume)`.
    pub receives: Vec<(usize, usize, f32)>,
    /// Song-level device indices in chain order.
    pub devices: Vec<usize>,
    pub lane: usize,
    pub autos: Vec<Auto>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SongM {
    pub tempo: i32,
    pub rate: i32,
    pub length: f64,
    pub devices: Vec<Dev>,
    pub lanes: Vec<Vec<Ev>>,
    pub tracks: Vec<Trk>,
}

impl SongM {
    /// Decodes a song blob; every structural problem is a [`ConvError::Input`] naming it.
    pub fn read(blob: &[u8]) -> Result<SongM, ConvError> {
        let s = Song::parse(blob).map_err(|e| ConvError::Input(format!("not a valid synth-mix song: {e:?}")))?;
        let devices = s.devices().map(|d| Dev { kind: d.kind, chunk: d.chunk.to_vec() }).collect();
        let lanes = s
            .lanes()
            .map(|l| {
                l.events()
                    .map(|(d, e)| match e {
                        EventKind::NoteOn { note, velocity } => (d as i32, note, Some(velocity)),
                        EventKind::NoteOff { note } => (d as i32, note, None),
                    })
                    .collect()
            })
            .collect();
        let tracks = s
            .tracks()
            .map(|t| Trk {
                volume: t.volume,
                receives: t.receives().map(|r| (r.track, r.channel, r.volume)).collect(),
                devices: t.devices().collect(),
                lane: t.lane,
                autos: t
                    .automations()
                    .map(|a| {
                        let mut prev = 0i32;
                        let points = a
                            .points()
                            .map(|(t, v)| {
                                let d = t.wrapping_sub(prev);
                                prev = t;
                                (d, (v as f64 * 255.0).round() as u8)
                            })
                            .collect();
                        Auto { device: a.device, param: a.param, points }
                    })
                    .collect(),
            })
            .collect();
        Ok(SongM { tempo: s.tempo, rate: s.rate, length: s.length, devices, lanes, tracks })
    }

    pub fn write(&self) -> Vec<u8> {
        let mut b = vec![];
        let i = |b: &mut Vec<u8>, v: i32| b.extend(v.to_le_bytes());
        i(&mut b, self.tempo);
        i(&mut b, self.rate);
        b.extend(self.length.to_le_bytes());
        i(&mut b, self.devices.len() as i32);
        for d in &self.devices {
            b.push(d.kind);
            i(&mut b, d.chunk.len() as i32);
            b.extend(&d.chunk);
        }
        i(&mut b, self.lanes.len() as i32);
        for l in &self.lanes {
            i(&mut b, l.len() as i32);
            for &(d, n, v) in l {
                i(&mut b, d);
                match v {
                    Some(v) => b.extend([n, v]),
                    None => b.push(n | 0x80),
                }
            }
        }
        i(&mut b, self.tracks.len() as i32);
        for t in &self.tracks {
            b.extend(t.volume.to_le_bytes());
            i(&mut b, t.receives.len() as i32);
            for &(s, c, v) in &t.receives {
                i(&mut b, s as i32);
                i(&mut b, c as i32);
                b.extend(v.to_le_bytes());
            }
            i(&mut b, t.devices.len() as i32);
            for &d in &t.devices {
                i(&mut b, d as i32);
            }
            i(&mut b, t.lane as i32);
            i(&mut b, t.autos.len() as i32);
            for a in &t.autos {
                i(&mut b, a.device as i32);
                i(&mut b, a.param as i32);
                i(&mut b, a.points.len() as i32);
                for &(d, v) in &a.points {
                    i(&mut b, d);
                    b.push(v);
                }
            }
        }
        b
    }
}
