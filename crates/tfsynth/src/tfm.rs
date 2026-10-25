//! `.tfm` song decoding (spec tf-song, reference §6.1), in place without allocation. Layout, little-endian:
//! u16 instrument count I (≤ 32), u16 tempo, I × u16 event counts, `INST`, I × 112 u8 parameters (`p = byte/100`),
//! `SONG`, per instrument three arrays (u16 row deltas, u8 notes, u8 velocities), `ENDS`. Bytes after `ENDS` are
//! ignored.

use crate::params::{from_byte, COUNT};

pub const MAX_INSTRUMENTS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TfmError {
    /// Shorter than its own counts say.
    Truncated,
    /// A tag is not `INST` / `SONG` / `ENDS`.
    BadTag,
    /// More than 32 instruments.
    TooManyInstruments,
}

#[derive(Clone, Copy, Debug)]
pub struct Tfm<'a> {
    pub tempo: u16,
    count: usize,
    counts: [u16; MAX_INSTRUMENTS],
    params: &'a [u8],
    /// Start of each instrument's event arrays.
    events: [usize; MAX_INSTRUMENTS],
    bytes: &'a [u8],
}

/// One event: absolute row (sum of deltas so far), note, velocity (0 = note-off).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub row: u32,
    pub note: u8,
    pub velocity: u8,
}

fn u16_at(b: &[u8], o: usize) -> Result<u16, TfmError> {
    b.get(o..o + 2).map(|x| u16::from_le_bytes([x[0], x[1]])).ok_or(TfmError::Truncated)
}

fn tag(b: &[u8], o: usize, t: &[u8; 4]) -> Result<(), TfmError> {
    match b.get(o..o + 4) {
        None => Err(TfmError::Truncated),
        Some(x) if x == t => Ok(()),
        Some(_) => Err(TfmError::BadTag),
    }
}

impl<'a> Tfm<'a> {
    pub fn parse(b: &'a [u8]) -> Result<Self, TfmError> {
        let count = u16_at(b, 0)? as usize;
        if count > MAX_INSTRUMENTS {
            return Err(TfmError::TooManyInstruments);
        }
        let tempo = u16_at(b, 2)?;
        let mut counts = [0u16; MAX_INSTRUMENTS];
        for (i, c) in counts.iter_mut().take(count).enumerate() {
            *c = u16_at(b, 4 + 2 * i)?;
        }
        let mut o = 4 + 2 * count;
        tag(b, o, b"INST")?;
        o += 4;
        let params = b.get(o..o + count * COUNT).ok_or(TfmError::Truncated)?;
        o += count * COUNT;
        tag(b, o, b"SONG")?;
        o += 4;
        let mut events = [0usize; MAX_INSTRUMENTS];
        for i in 0..count {
            events[i] = o;
            o += 4 * counts[i] as usize;
        }
        if o > b.len() {
            return Err(TfmError::Truncated);
        }
        tag(b, o, b"ENDS")?;
        Ok(Tfm { tempo, count, counts, params, events, bytes: b })
    }

    pub fn instruments(&self) -> usize {
        self.count
    }

    pub fn event_count(&self, i: usize) -> usize {
        self.counts[i] as usize
    }

    /// Instrument `i`'s patch (`byte / 100`).
    pub fn patch(&self, i: usize) -> [f32; COUNT] {
        let raw = &self.params[i * COUNT..(i + 1) * COUNT];
        core::array::from_fn(|k| from_byte(raw[k]))
    }

    /// Event `k` of instrument `i` with its absolute row (O(k) for the row: players keep a cursor instead).
    pub fn event(&self, i: usize, k: usize, row: u32) -> Event {
        let (o, n) = (self.events[i], self.counts[i] as usize);
        let d = u16::from_le_bytes([self.bytes[o + 2 * k], self.bytes[o + 2 * k + 1]]);
        Event { row: row + d as u32, note: self.bytes[o + 2 * n + k], velocity: self.bytes[o + 3 * n + k] }
    }

    /// Instrument `i`'s events in stored order.
    pub fn events(&self, i: usize) -> impl Iterator<Item = Event> + '_ {
        let mut row = 0;
        (0..self.event_count(i)).map(move |k| {
            let e = self.event(i, k, row);
            row = e.row;
            e
        })
    }
}
