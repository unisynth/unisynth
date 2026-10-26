//! Owned `synth-mix` song blob (layout in `synth_core::mix::song`): `read` parses and validates through
//! `synth_core::mix::Song`, `write` emits the same layout. Times here are absolute samples; the writer delta-encodes.

use synth_core::device::EventKind;
use synth_core::mix::{LoadError, Song};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlobM {
    pub tempo: i32,
    pub rate: i32,
    /// Seconds.
    pub length: f64,
    /// `(kind, chunk)`.
    pub devices: Vec<(u8, Vec<u8>)>,
    /// Per lane: `(absolute sample, event)`, sample order.
    pub lanes: Vec<Vec<(i64, EventKind)>>,
    pub tracks: Vec<TrackM>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackM {
    pub volume: f32,
    /// `(sending track, channel 0 main | 2 sidechain, volume)`.
    pub receives: Vec<(usize, usize, f32)>,
    /// Song-level device indices in chain order.
    pub devices: Vec<usize>,
    pub lane: usize,
    pub autos: Vec<AutoM>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AutoM {
    /// Index into the track's devices.
    pub device: usize,
    pub param: usize,
    /// `(absolute sample, byte)`.
    pub points: Vec<(i64, u8)>,
}

impl BlobM {
    /// Parses and validates `b` (`Song::parse` checks every index, including automation devices against the track's
    /// device list), so importers and the live host can index without checks.
    pub fn read(b: &[u8]) -> Result<BlobM, LoadError> {
        let s = Song::parse(b)?;
        let lanes = s
            .lanes()
            .map(|l| {
                let mut at = 0i64;
                l.events().map(|(d, k)| {
                    at += d as i64;
                    (at, k)
                }).collect()
            })
            .collect();
        let tracks = s
            .tracks()
            .map(|t| TrackM {
                volume: t.volume,
                receives: t.receives().map(|r| (r.track, r.channel, r.volume)).collect(),
                devices: t.devices().collect(),
                lane: t.lane,
                autos: t
                    .automations()
                    .map(|a| AutoM {
                        device: a.device,
                        param: a.param,
                        points: a.points().map(|(at, v)| (at as i64, (v * 255.0).round() as u8)).collect(),
                    })
                    .collect(),
            })
            .collect();
        Ok(BlobM {
            tempo: s.tempo,
            rate: s.rate,
            length: s.length,
            devices: s.devices().map(|d| (d.kind, d.chunk.to_vec())).collect(),
            lanes,
            tracks,
        })
    }

    pub fn write(&self) -> Vec<u8> {
        let mut w = Vec::new();
        let i = |w: &mut Vec<u8>, v: i32| w.extend(v.to_le_bytes());
        let n = |w: &mut Vec<u8>, v: usize| w.extend((v as i32).to_le_bytes());
        i(&mut w, self.tempo);
        i(&mut w, self.rate);
        w.extend(self.length.to_le_bytes());
        n(&mut w, self.devices.len());
        for (k, c) in &self.devices {
            w.push(*k);
            n(&mut w, c.len());
            w.extend(c);
        }
        n(&mut w, self.lanes.len());
        for l in &self.lanes {
            n(&mut w, l.len());
            let mut prev = 0i64;
            for &(at, k) in l {
                i(&mut w, (at - prev) as i32);
                prev = at;
                match k {
                    EventKind::NoteOn { note, velocity } => w.extend([note & 0x7f, velocity]),
                    EventKind::NoteOff { note } => w.push(note | 0x80),
                }
            }
        }
        n(&mut w, self.tracks.len());
        for t in &self.tracks {
            w.extend(t.volume.to_le_bytes());
            n(&mut w, t.receives.len());
            for &(tr, ch, v) in &t.receives {
                n(&mut w, tr);
                n(&mut w, ch);
                w.extend(v.to_le_bytes());
            }
            n(&mut w, t.devices.len());
            for &d in &t.devices {
                n(&mut w, d);
            }
            n(&mut w, t.lane);
            n(&mut w, t.autos.len());
            for a in &t.autos {
                n(&mut w, a.device);
                n(&mut w, a.param);
                n(&mut w, a.points.len());
                let mut prev = 0i64;
                for &(at, v) in &a.points {
                    i(&mut w, (at - prev) as i32);
                    prev = at;
                    w.push(v);
                }
            }
        }
        w
    }
}
