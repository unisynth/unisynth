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

    /// Migrates what is still in the old layout (change unify-v2-algorithms): instrument chunks of versions 1–2 with
    /// their lanes, effect chunks without the v3 tag with their lanes. A v3 part stays as it is, so a song migrates once.
    pub fn to_v3(&mut self) -> Result<(), ConvError> {
        use unisynth::{kind, migrate};
        let err = |e: migrate::MigrateError| ConvError::Input(format!("migration to the v3 layout: {e}"));
        let old: Vec<Option<[f32; unisynth::params::old::COUNT]>> = self
            .devices
            .iter()
            .map(|d| (d.kind == kind::INSTRUMENT && migrate::is_old_instrument(&d.chunk)).then(|| migrate::decode_old_chunk(&d.chunk).map(|(_, v, _)| v)).transpose())
            .collect::<Result<_, _>>()
            .map_err(err)?;
        let fx_mode: Vec<Option<f32>> = self
            .devices
            .iter()
            .map(|d| (unisynth::fx::is_effect(d.kind) && migrate::is_old_effect(&d.chunk)).then(|| d.chunk.get(..4).map_or(0.0, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))))
            .collect();
        let mut quirks: Vec<(usize, usize)> = vec![];
        for t in &mut self.tracks {
            let mut autos = Vec::with_capacity(t.autos.len());
            for a in t.autos.drain(..) {
                let dev = t.devices[a.device];
                if let Some(base) = &old[dev] {
                    for (param, points) in migrate::lane(base, a.param, &a.points).map_err(err)? {
                        autos.push(Auto { device: a.device, param, points });
                    }
                    quirks.extend(migrate::lane_quirks(base, a.param, a.points.iter().map(|&(_, b)| b as f32 / 255.0)).into_iter().map(|q| (dev, q)));
                } else if let Some(m) = fx_mode[dev] {
                    let k = self.devices[dev].kind;
                    let param = migrate::effect_param(k, m, a.param).map_err(err)?;
                    let mut a = Auto { param, ..a };
                    if param == 0 {
                        // the mode as the kind's plain choice, point by point (as the chunk's own mode)
                        let byte = |b: u8| (unisynth::fx::mode_value(k, migrate::old_fx_mode(k, b as f32 / 255.0)) * 255.0).round() as u8;
                        a.points.iter_mut().for_each(|p| p.1 = byte(p.1));
                    }
                    autos.push(a);
                } else {
                    autos.push(a);
                }
            }
            t.autos = autos;
        }
        for (i, d) in self.devices.iter_mut().enumerate() {
            if old[i].is_some() {
                d.chunk = migrate::instrument_chunk(&d.chunk).map_err(err)?;
                // the quirks a lane reaches (set on the patch: quirks are fixed per patch)
                let qs: Vec<usize> = quirks.iter().filter(|e| e.0 == i).map(|e| e.1).collect();
                if !qs.is_empty() {
                    let c = unisynth::chunk::Chunk::parse(&d.chunk).map_err(|e| ConvError::Input(format!("migrated chunk: {e:?}")))?;
                    let mut v = c.values();
                    qs.iter().for_each(|&q| v[q] = 1.0);
                    let samples: Vec<&[u8]> = (0..c.sample_count()).filter_map(|k| c.sample_bytes(k)).collect();
                    d.chunk = unisynth::chunk::encode_text(&v, &samples, c.text());
                }
            } else if fx_mode[i].is_some() {
                d.chunk = migrate::effect_chunk(d.kind, &d.chunk).map_err(err)?;
            }
        }
        Ok(())
    }

    /// The song blob, every part in the v3 layout ([`Self::to_v3`]).
    pub fn write(&self) -> Result<Vec<u8>, ConvError> {
        let mut m = self.clone();
        m.to_v3()?;
        Ok(m.write_as_is())
    }

    /// The song blob as it stands (tests of the old layout).
    pub fn write_as_is(&self) -> Vec<u8> {
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
