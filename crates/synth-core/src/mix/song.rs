//! Zero-copy decoder of the WaveSabre song blob (little endian):
//!
//! ```text
//! tempo i32, rate i32, length f64
//! devices: n i32, n × { kind u8, size i32, chunk[size] }
//! lanes:   n i32, n × { events i32, events × { delta i32, note u8 (bit 7 = note-off), [velocity u8 if note-on] } }
//! tracks:  n i32, n × { volume f32,
//!                       receives i32 × { track i32, channel i32 (0 main | 2 sidechain; feature `pairs`: see
//!                       `device::recv`), volume f32 },
//!                       devices i32 × { device index i32 }, lane i32,
//!                       automations i32 × { device (index into this track's devices) i32, param i32,
//!                                           points i32 × { delta i32 (signed), value u8 } } }
//! ```
//! `parse` validates everything once; the views below then read the borrowed bytes without further checks failing.

use super::LoadError;
use crate::device::EventKind;

#[derive(Clone, Copy, Debug)]
struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], LoadError> {
        let end = self.p.checked_add(n).filter(|&e| e <= self.b.len()).ok_or(LoadError::Malformed)?;
        let s = &self.b[self.p..end];
        self.p = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, LoadError> {
        Ok(self.bytes(1)?[0])
    }
    fn i32(&mut self) -> Result<i32, LoadError> {
        let s = self.bytes(4)?;
        Ok(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn f32(&mut self) -> Result<f32, LoadError> {
        Ok(f32::from_bits(self.i32()? as u32))
    }
    fn f64(&mut self) -> Result<f64, LoadError> {
        let s = self.bytes(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        Ok(f64::from_le_bytes(a))
    }
    /// A count of records of at least `min` bytes each; rejects negative or impossible counts before looping.
    fn count(&mut self, min: usize) -> Result<usize, LoadError> {
        let n = self.i32()?;
        let n = usize::try_from(n).map_err(|_| LoadError::Malformed)?;
        if n.checked_mul(min).is_none_or(|need| need > self.b.len() - self.p) {
            return Err(LoadError::Malformed);
        }
        Ok(n)
    }
    /// An index in `0..len`.
    fn index(&mut self, len: usize) -> Result<usize, LoadError> {
        usize::try_from(self.i32()?).ok().filter(|&i| i < len).ok_or(LoadError::BadIndex)
    }
    /// Non-negative time delta.
    fn delta(&mut self) -> Result<u32, LoadError> {
        u32::try_from(self.i32()?).map_err(|_| LoadError::Malformed)
    }
}

/// A decoded song: header fields plus validated byte sections.
#[derive(Clone, Copy, Debug)]
pub struct Song<'a> {
    pub tempo: i32,
    pub rate: i32,
    /// Length in seconds.
    pub length: f64,
    devices: (usize, &'a [u8]),
    lanes: (usize, &'a [u8]),
    tracks: (usize, &'a [u8]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceRec<'a> {
    pub kind: u8,
    pub chunk: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Receive {
    pub track: usize,
    /// Receiving channel pair: 0 = main, 2 = sidechain (feature `pairs`: [`crate::device::recv`]).
    pub channel: usize,
    pub volume: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Lane<'a> {
    count: usize,
    bytes: &'a [u8],
}

#[derive(Clone, Copy, Debug)]
pub struct Track<'a> {
    pub volume: f32,
    pub lane: usize,
    receives: (usize, &'a [u8]),
    devices: (usize, &'a [u8]),
    automations: (usize, &'a [u8]),
}

#[derive(Clone, Copy, Debug)]
pub struct Automation<'a> {
    /// Index into the track's device list.
    pub device: usize,
    pub param: usize,
    points: (usize, &'a [u8]),
}

impl<'a> Song<'a> {
    pub fn parse(b: &'a [u8]) -> Result<Song<'a>, LoadError> {
        let mut r = Rd { b, p: 0 };
        let tempo = r.i32()?;
        let rate = r.i32()?;
        let length = r.f64()?;
        if tempo <= 0 || rate <= 0 || !length.is_finite() || length < 0.0 {
            return Err(LoadError::Malformed);
        }
        let nd = r.count(5)?;
        let s = r.p;
        for _ in 0..nd {
            r.u8()?;
            let size = r.count(1)?;
            r.bytes(size)?;
        }
        let devices = (nd, &b[s..r.p]);
        let nl = r.count(4)?;
        let s = r.p;
        for _ in 0..nl {
            for _ in 0..r.count(5)? {
                r.delta()?;
                if r.u8()? & 0x80 == 0 {
                    r.u8()?;
                }
            }
        }
        let lanes = (nl, &b[s..r.p]);
        let nt = r.count(20)?;
        if nt == 0 {
            return Err(LoadError::Malformed);
        }
        let s = r.p;
        for _ in 0..nt {
            r.f32()?;
            for _ in 0..r.count(12)? {
                r.index(nt)?;
                let c = r.i32()?;
                if c < 0 || !crate::device::recv::valid(c as usize, cfg!(feature = "pairs")) {
                    return Err(LoadError::BadIndex);
                }
                r.f32()?;
            }
            let td = r.count(4)?;
            for _ in 0..td {
                r.index(nd)?;
            }
            r.index(nl)?;
            for _ in 0..r.count(12)? {
                r.index(td)?;
                usize::try_from(r.i32()?).map_err(|_| LoadError::BadIndex)?;
                for _ in 0..r.count(5)? {
                    r.i32()?; // signed: converters emit points before the song start
                    r.u8()?;
                }
            }
        }
        let tracks = (nt, &b[s..r.p]);
        if r.p != b.len() {
            return Err(LoadError::Malformed);
        }
        Ok(Song { tempo, rate, length, devices, lanes, tracks })
    }

    /// A song from sections that [`Song::parse`] already accepted (generated code: `synth-convert rust`). Nothing is
    /// checked: `tempo`, `rate`, `length` and the `(count, bytes)` sections must come from a parsed song.
    #[doc(hidden)]
    /// The sections must come from a parsed [`Song`] (they are not validated again).
    pub const fn from_sections(tempo: i32, rate: i32, length: f64, sections: [(usize, &'a [u8]); 3]) -> Song<'a> {
        let [devices, lanes, tracks] = sections;
        Song { tempo, rate, length, devices, lanes, tracks }
    }

    /// The `(count, bytes)` device, lane and track sections ([`Song::from_sections`] takes them back).
    pub fn sections(&self) -> [(usize, &'a [u8]); 3] {
        [self.devices, self.lanes, self.tracks]
    }

    pub fn device_count(&self) -> usize {
        self.devices.0
    }
    pub fn lane_count(&self) -> usize {
        self.lanes.0
    }
    pub fn track_count(&self) -> usize {
        self.tracks.0
    }

    pub fn devices(&self) -> impl Iterator<Item = DeviceRec<'a>> + 'a {
        let mut r = Rd { b: self.devices.1, p: 0 };
        (0..self.devices.0).map_while(move |_| {
            let kind = r.u8().ok()?;
            let size = r.i32().ok()? as usize;
            Some(DeviceRec { kind, chunk: r.bytes(size).ok()? })
        })
    }

    pub fn lanes(&self) -> impl Iterator<Item = Lane<'a>> + 'a {
        let mut r = Rd { b: self.lanes.1, p: 0 };
        (0..self.lanes.0).map_while(move |_| {
            let count = r.i32().ok()? as usize;
            let s = r.p;
            for _ in 0..count {
                r.i32().ok()?;
                if r.u8().ok()? & 0x80 == 0 {
                    r.u8().ok()?;
                }
            }
            Some(Lane { count, bytes: &r.b[s..r.p] })
        })
    }

    pub fn lane(&self, i: usize) -> Option<Lane<'a>> {
        self.lanes().nth(i)
    }

    pub fn tracks(&self) -> impl Iterator<Item = Track<'a>> + 'a {
        let mut r = Rd { b: self.tracks.1, p: 0 };
        (0..self.tracks.0).map_while(move |_| {
            let volume = r.f32().ok()?;
            let nr = r.i32().ok()? as usize;
            let receives = (nr, r.bytes(nr * 12).ok()?);
            let nd = r.i32().ok()? as usize;
            let devices = (nd, r.bytes(nd * 4).ok()?);
            let lane = r.i32().ok()? as usize;
            let na = r.i32().ok()? as usize;
            let s = r.p;
            for _ in 0..na {
                r.bytes(8).ok()?;
                let np = r.i32().ok()? as usize;
                r.bytes(np * 5).ok()?;
            }
            Some(Track { volume, lane, receives, devices, automations: (na, &r.b[s..r.p]) })
        })
    }
}

impl<'a> Lane<'a> {
    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    /// `(delta frames from the previous event, event)`.
    pub fn events(&self) -> LaneEvents<'a> {
        LaneEvents { r: Rd { b: self.bytes, p: 0 }, left: self.count }
    }
}

/// Iterator over a lane's events.
#[derive(Clone, Copy, Debug)]
pub struct LaneEvents<'a> {
    r: Rd<'a>,
    left: usize,
}

impl Iterator for LaneEvents<'_> {
    type Item = (u32, EventKind);
    fn next(&mut self) -> Option<Self::Item> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        let r = &mut self.r;
        let delta = r.i32().ok()? as u32;
        let n = r.u8().ok()?;
        let kind = if n & 0x80 == 0 {
            EventKind::NoteOn { note: n, velocity: r.u8().ok()? }
        } else {
            EventKind::NoteOff { note: n & 0x7f }
        };
        Some((delta, kind))
    }
}

impl<'a> Track<'a> {
    pub fn receives(&self) -> impl Iterator<Item = Receive> + 'a {
        let mut r = Rd { b: self.receives.1, p: 0 };
        (0..self.receives.0).map_while(move |_| {
            Some(Receive {
                track: r.i32().ok()? as usize,
                channel: r.i32().ok()? as usize,
                volume: r.f32().ok()?,
            })
        })
    }
    pub fn device_count(&self) -> usize {
        self.devices.0
    }
    /// Song-level device indices, in chain order.
    pub fn devices(&self) -> impl Iterator<Item = usize> + 'a {
        let mut r = Rd { b: self.devices.1, p: 0 };
        (0..self.devices.0).map_while(move |_| Some(r.i32().ok()? as usize))
    }
    pub fn automations(&self) -> impl Iterator<Item = Automation<'a>> + 'a {
        let mut r = Rd { b: self.automations.1, p: 0 };
        (0..self.automations.0).map_while(move |_| {
            let device = r.i32().ok()? as usize;
            let param = r.i32().ok()? as usize;
            let np = r.i32().ok()? as usize;
            Some(Automation { device, param, points: (np, r.bytes(np * 5).ok()?) })
        })
    }
}

impl<'a> Automation<'a> {
    pub fn len(&self) -> usize {
        self.points.0
    }
    pub fn is_empty(&self) -> bool {
        self.points.0 == 0
    }
    /// `(absolute frame, value)` with value = `byte / 255` (computed in f64, as upstream). Times are signed
    /// (points may precede the song start) and accumulate with wrapping, like upstream's `int`.
    pub fn points(&self) -> Points<'a> {
        Points { r: Rd { b: self.points.1, p: 0 }, left: self.points.0, t: 0 }
    }
}

/// Iterator over an automation's points.
#[derive(Clone, Copy, Debug)]
pub struct Points<'a> {
    r: Rd<'a>,
    left: usize,
    t: i32,
}

impl Iterator for Points<'_> {
    type Item = (i32, f32);
    fn next(&mut self) -> Option<Self::Item> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        self.t = self.t.wrapping_add(self.r.i32().ok()?);
        Some((self.t, (self.r.u8().ok()? as f64 / 255.0) as f32))
    }
}
