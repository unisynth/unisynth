//! Instrument chunk v1 (design D3), little endian, canonical:
//!
//! ```text
//! u8 version = 1
//! u8 n, n × { u8 index, f32 value }          values that differ from DEFAULTS, indices strictly increasing
//! u8 s, s × { u32 size, GSM sample chunk }   header + WAVEFORMATEX + blocks, nothing after the blocks
//! (end of chunk)
//! ```
//!
//! `Chunk::parse` validates everything once, so the views read borrowed bytes without further checks failing. Values
//! are finite and in 0..=1, except the three sampler indices (`params::is_raw`), which are finite and in 0..=65535.

use crate::params::{self as pm, o, osc, osc_type, shape, COUNT, DEFAULTS};
use synth_core::mix::LoadError;

pub const VERSION: u8 = 1;
/// Largest raw sampler index.
pub const MAX_SAMPLE_INDEX: f32 = 65535.0;
/// Entries a sample section can hold (`s` is a byte).
pub const MAX_SAMPLES: usize = 255;

/// A raw sampler index read as a C `(int)v` (negative and NaN → 0): 1-based, 0 = none.
#[inline]
pub fn sample_index(v: f32) -> u32 {
    if v >= 1.0 { v as u32 } else { 0 }
}

#[derive(Clone, Copy)]
struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], LoadError> {
        let end = self.p.checked_add(n).filter(|&e| e <= self.b.len()).ok_or(LoadError::BadChunk)?;
        let s = &self.b[self.p..end];
        self.p = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, LoadError> {
        Ok(self.bytes(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, LoadError> {
        let s = self.bytes(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
}

/// A validated chunk.
#[derive(Clone, Copy, Debug)]
pub struct Chunk<'a> {
    pairs: &'a [u8],
    samples: &'a [u8],
    sample_count: usize,
}

impl<'a> Chunk<'a> {
    pub fn parse(b: &'a [u8]) -> Result<Self, LoadError> {
        let mut r = Rd { b, p: 0 };
        if r.u8()? != VERSION {
            return Err(LoadError::BadChunk);
        }
        let n = r.u8()? as usize;
        let pairs = r.bytes(n * 5)?;
        let mut prev: Option<usize> = None;
        for p in pairs.chunks_exact(5) {
            let i = p[0] as usize;
            if i >= COUNT || prev.is_some_and(|q| i <= q) {
                return Err(LoadError::BadChunk);
            }
            prev = Some(i);
            let v = f32::from_le_bytes([p[1], p[2], p[3], p[4]]);
            let max = if pm::is_raw(i) { MAX_SAMPLE_INDEX } else { 1.0 };
            if !(0.0..=max).contains(&v) {
                return Err(LoadError::BadChunk); // also rejects NaN
            }
        }
        let sample_count = r.u8()? as usize;
        let start = r.p;
        for _ in 0..sample_count {
            let size = r.u32()? as usize;
            let body = r.bytes(size)?;
            Self::check_entry(body)?;
        }
        let samples = &b[start..r.p];
        if r.p != b.len() {
            return Err(LoadError::BadChunk);
        }
        let ch = Chunk { pairs, samples, sample_count };
        let values = ch.values();
        for i in 0..pm::OSC_SLOTS {
            let is_sampler = crate::law::choice(values[osc(i, o::TYPE)], osc_type::COUNT) == osc_type::SAMPLER;
            let gsm = crate::law::choice(values[osc(i, o::SHAPE + shape::SMP_SOURCE)], pm::smp_source::COUNT) == pm::smp_source::GSM;
            if is_sampler && gsm && sample_index(values[pm::smp_index(i)]) as usize > sample_count {
                return Err(LoadError::BadChunk);
            }
        }
        Ok(ch)
    }

    #[cfg(feature = "us-sample-gsm")]
    fn check_entry(body: &[u8]) -> Result<(), LoadError> {
        match gsm::GsmChunk::parse(body)? {
            Some(g) if g.rest.is_empty() => Ok(()),
            _ => Err(LoadError::BadChunk),
        }
    }

    #[cfg(not(feature = "us-sample-gsm"))]
    fn check_entry(_: &[u8]) -> Result<(), LoadError> {
        Err(LoadError::UnsupportedModule(crate::module::SAMPLE_GSM))
    }

    /// All parameter values: the defaults with the chunk's pairs applied.
    pub fn values(&self) -> [f32; COUNT] {
        let mut v = DEFAULTS;
        for (i, x) in self.pairs() {
            v[i] = x;
        }
        v
    }

    /// The `(index, value)` pairs in the chunk, in increasing index order.
    pub fn pairs(&self) -> impl Iterator<Item = (usize, f32)> + 'a {
        self.pairs.chunks_exact(5).map(|p| (p[0] as usize, f32::from_le_bytes([p[1], p[2], p[3], p[4]])))
    }

    pub fn sample_count(&self) -> usize {
        self.sample_count
    }

    /// Bytes of sample `i` (0-based).
    pub fn sample_bytes(&self, i: usize) -> Option<&'a [u8]> {
        let mut r = Rd { b: self.samples, p: 0 };
        for k in 0..self.sample_count {
            let size = r.u32().ok()? as usize;
            let body = r.bytes(size).ok()?;
            if k == i {
                return Some(body);
            }
        }
        None
    }

    /// Sample `i` (0-based), parsed.
    #[cfg(feature = "us-sample-gsm")]
    pub fn sample(&self, i: usize) -> Option<gsm::GsmChunk<'a>> {
        gsm::GsmChunk::parse(self.sample_bytes(i)?).ok().flatten()
    }
}

/// Encodes a patch (host side): only values that differ from the defaults, in index order, then the samples. Each
/// sample is a GSM chunk entry as `Chunk::parse` expects.
#[cfg(any(test, feature = "std"))]
pub fn encode(values: &[f32; COUNT], samples: &[&[u8]]) -> std::vec::Vec<u8> {
    assert!(samples.len() <= MAX_SAMPLES, "a chunk holds at most {MAX_SAMPLES} samples");
    let mut c = std::vec![VERSION, 0];
    let mut n = 0u8;
    for (i, (&v, &d)) in values.iter().zip(DEFAULTS.iter()).enumerate() {
        if v.to_bits() != d.to_bits() {
            c.push(i as u8);
            c.extend(v.to_le_bytes());
            n += 1;
        }
    }
    c[1] = n;
    c.push(samples.len() as u8);
    for s in samples {
        c.extend((s.len() as u32).to_le_bytes());
        c.extend(*s);
    }
    c
}

#[cfg(test)]
mod tests;
