//! Test-only writer for the song blob (devices, lanes, tracks): synthetic songs for the kinds / player tests. Shared
//! with `tests/` through `#[path]`. No third-party data.

/// A device chunk: the parameters as little-endian f32 followed by the chunk's own byte length (the layout the
/// song files carry).
pub fn chunk(params: &[f32]) -> Vec<u8> {
    let mut v: Vec<u8> = params.iter().flat_map(|p| p.to_le_bytes()).collect();
    let size = (v.len() + 4) as u32;
    v.extend(size.to_le_bytes());
    v
}

pub type Event = (i32, u8, Option<u8>);
/// `(device index within the track, parameter, [(time delta, byte value)])`.
pub type Auto = (i32, i32, Vec<(i32, u8)>);

pub struct Track {
    pub vol: f32,
    pub recv: Vec<(i32, i32, f32)>,
    pub devs: Vec<i32>,
    pub lane: i32,
    pub autos: Vec<Auto>,
}

pub fn track(devs: &[i32], lane: i32) -> Track {
    Track { vol: 1.0, recv: vec![], devs: devs.to_vec(), lane, autos: vec![] }
}

#[derive(Default)]
pub struct Song {
    pub rate: i32,
    pub devices: Vec<(u8, Vec<u8>)>,
    pub lanes: Vec<Vec<Event>>,
    pub tracks: Vec<Track>,
}

impl Song {
    pub fn new() -> Self {
        Song { rate: 44_100, ..Default::default() }
    }

    pub fn device(&mut self, kind: u8, params: &[f32]) -> i32 {
        self.devices.push((kind, chunk(params)));
        self.devices.len() as i32 - 1
    }

    pub fn bytes(&self) -> Vec<u8> {
        let mut w: Vec<u8> = vec![];
        let i = |w: &mut Vec<u8>, v: i32| w.extend(v.to_le_bytes());
        i(&mut w, 174);
        i(&mut w, self.rate);
        w.extend(10.0f64.to_le_bytes());
        i(&mut w, self.devices.len() as i32);
        for (k, c) in &self.devices {
            w.push(*k);
            i(&mut w, c.len() as i32);
            w.extend(c);
        }
        i(&mut w, self.lanes.len() as i32);
        for l in &self.lanes {
            i(&mut w, l.len() as i32);
            for &(d, n, v) in l {
                i(&mut w, d);
                match v {
                    Some(v) => w.extend([n, v]),
                    None => w.push(n | 0x80),
                }
            }
        }
        i(&mut w, self.tracks.len() as i32);
        for t in &self.tracks {
            w.extend(t.vol.to_le_bytes());
            i(&mut w, t.recv.len() as i32);
            for &(s, c, v) in &t.recv {
                i(&mut w, s);
                i(&mut w, c);
                w.extend(v.to_le_bytes());
            }
            i(&mut w, t.devs.len() as i32);
            for &d in &t.devs {
                i(&mut w, d);
            }
            i(&mut w, t.lane);
            i(&mut w, t.autos.len() as i32);
            for (d, p, pts) in &t.autos {
                i(&mut w, *d);
                i(&mut w, *p);
                i(&mut w, pts.len() as i32);
                for &(dt, v) in pts {
                    i(&mut w, dt);
                    w.push(v);
                }
            }
        }
        w
    }
}
