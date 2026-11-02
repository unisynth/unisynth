//! Instrument chunk v3 (change unify-v2-algorithms: the v3 layout; versions 1 and 2 held the old layout and are
//! rejected with `LoadError::UnknownVersion`, the host migrates them: `crate::migrate`). Little endian, canonical:
//!
//! ```text
//! u8 version = 3
//! count n, n × { index, f32 value }          values that differ from DEFAULTS, indices strictly increasing
//! u8 s, s × { u32 size, GSM sample chunk }   header + WAVEFORMATEX + blocks, nothing after the blocks
//! (end of chunk)
//! ```
//!
//! Version 4 is version 3 followed by `u16 len, len bytes` of speech text (`len ≥ 1`); a chunk without text is written
//! as version 3.
//!
//! Escapes (change add-v2-import, design D4), like UTF-8 lead bytes; chunks without them are unchanged:
//! a count byte `c < 0xFF` is `n = c`, `0xFF` is followed by a u16 `n ≥ 255`; an index byte `b < 0xF0` is the index,
//! `0xF0 + k` is followed by a byte `x` for index `240 + 256·k + x` (always ≥ 240, so every index has one form). An
//! escaped count below 255 is `BadChunk`.
//!
//! `Chunk::parse` validates everything once, so the views read borrowed bytes without further checks failing. Values
//! are finite and in 0..=1, except the three sampler indices (`params::is_raw`), which are finite and in 0..=65535.

use crate::params::{self as pm, o, osc, osc_type, shape, COUNT, DEFAULTS};
use synth_core::mix::LoadError;

pub const VERSION: u8 = 3;
/// Version 3 + a speech text section.
pub const VERSION_TEXT: u8 = 4;
/// First index byte that is a bank prefix (design D4).
pub const ESC: u8 = 0xF0;
/// Indices below this are one byte.
pub const SHORT: usize = ESC as usize;
/// Largest encodable index.
pub const MAX_INDEX: usize = SHORT + 16 * 256 - 1;
/// Count byte that announces a u16 count.
pub const COUNT_ESC: u8 = 0xFF;
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
    /// An escaped index (a 2-byte form always decodes to `SHORT` or more, so every index has one form).
    fn index(&mut self) -> Result<usize, LoadError> {
        let b = self.u8()?;
        if b < ESC {
            return Ok(b as usize);
        }
        if !cfg!(feature = "us-chunk-wide") {
            return Err(LoadError::UnsupportedModule(crate::module::CHUNK_WIDE));
        }
        Ok(SHORT + ((b - ESC) as usize) * 256 + self.u8()? as usize)
    }
    /// An escaped pair count; an escaped count below 255 is not canonical.
    fn count(&mut self) -> Result<usize, LoadError> {
        let c = self.u8()?;
        if c < COUNT_ESC {
            return Ok(c as usize);
        }
        if !cfg!(feature = "us-chunk-wide") {
            return Err(LoadError::UnsupportedModule(crate::module::CHUNK_WIDE));
        }
        let s = self.bytes(2)?;
        let n = u16::from_le_bytes([s[0], s[1]]) as usize;
        if n < COUNT_ESC as usize {
            return Err(LoadError::BadChunk);
        }
        Ok(n)
    }
    fn u32(&mut self) -> Result<u32, LoadError> {
        let s = self.bytes(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
}

/// What [`Chunk::values`] starts from: [`DEFAULTS`], with the two-byte block (indices 240 on) stored as zero in a
/// build that cannot read it (no `us-chunk-wide`, `us-matrix-add` or `us-strip`): the strip, controllers 2–7, mod slots 11–16 and
/// the speech come with those modules, so nothing such a build runs reads that block, and a zero run costs almost
/// nothing packed (change unify-v2-algorithms, the size gate of the format switch).
static PLAYER_DEFAULTS: [f32; COUNT] = {
    let mut d = DEFAULTS;
    if !cfg!(any(feature = "us-chunk-wide", feature = "us-matrix-add", feature = "us-strip")) {
        let mut i = SHORT;
        while i < COUNT {
            d[i] = 0.0;
            i += 1;
        }
    }
    d
};

/// A validated chunk.
#[derive(Clone, Copy, Debug)]
pub struct Chunk<'a> {
    /// The encoded pairs (escaped index + f32 each).
    pairs: &'a [u8],
    samples: &'a [u8],
    sample_count: usize,
}

impl<'a> Chunk<'a> {
    pub fn parse(b: &'a [u8]) -> Result<Self, LoadError> {
        let mut r = Rd { b, p: 0 };
        // songs checked on the host (`synth-convert rust`): only the sections are located
        if cfg!(feature = "us-trusted") && !cfg!(feature = "us-chunk-wide") {
            // no escapes: 5-byte pairs after a one-byte count
            let n = b[1] as usize;
            let pairs = &b[2..2 + n * 5];
            return Ok(Chunk { pairs, samples: &b[3 + n * 5..], sample_count: b[2 + n * 5] as usize });
        }
        if cfg!(feature = "us-trusted") {
            r.p = 1;
            let n = r.count()?;
            let start = r.p;
            for _ in 0..n {
                r.index()?;
                r.p += 4;
            }
            let pairs = &b[start..r.p];
            return Ok(Chunk { pairs, samples: &b[r.p + 1..], sample_count: b[r.p] as usize });
        }
        let version = r.u8()?;
        if version != VERSION && version != VERSION_TEXT {
            return Err(LoadError::UnknownVersion);
        }
        let n = r.count()?;
        let start = r.p;
        let mut prev: Option<usize> = None;
        for _ in 0..n {
            let i = r.index()?;
            if i >= COUNT || prev.is_some_and(|q| i <= q) {
                return Err(LoadError::BadChunk);
            }
            prev = Some(i);
            let x = r.bytes(4)?;
            let v = f32::from_le_bytes([x[0], x[1], x[2], x[3]]);
            let max = if pm::is_raw(i) { MAX_SAMPLE_INDEX } else { 1.0 };
            if !(0.0..=max).contains(&v) {
                return Err(LoadError::BadChunk); // also rejects NaN
            }
        }
        let pairs = &b[start..r.p];
        let sample_count = r.u8()? as usize;
        let start = r.p;
        for _ in 0..sample_count {
            let size = r.u32()? as usize;
            let body = r.bytes(size)?;
            Self::check_entry(body)?;
        }
        if version == VERSION_TEXT {
            let s = r.bytes(2)?;
            let len = u16::from_le_bytes([s[0], s[1]]) as usize;
            if len == 0 {
                return Err(LoadError::BadChunk); // a chunk without text is version 1
            }
            r.bytes(len)?;
            if !cfg!(feature = "us-speech") {
                return Err(LoadError::UnsupportedModule(crate::module::SPEECH));
            }
        }
        if r.p != b.len() {
            return Err(LoadError::BadChunk);
        }
        let ch = Chunk { pairs, samples: &b[start..], sample_count };
        let values = ch.values();
        for i in 0..pm::OSC_SLOTS {
            let is_sampler = osc_type::decode(values[osc(i, o::TYPE)]) == osc_type::SAMPLER;
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
        let mut v = PLAYER_DEFAULTS;
        for (i, x) in self.pairs() {
            v[i] = x;
        }
        v
    }

    /// Whether the chunk uses an escape (an index of `SHORT` or more, or 255 or more pairs): module `CHUNK_WIDE`.
    pub fn wide(&self) -> bool {
        let mut n = 0;
        let mut hi = 0;
        for (i, _) in self.pairs() {
            n += 1;
            hi = i;
        }
        n >= COUNT_ESC as usize || hi >= SHORT
    }

    /// The `(index, value)` pairs in the chunk, in increasing index order.
    #[cfg(feature = "us-chunk-wide")]
    pub fn pairs(&self) -> impl Iterator<Item = (usize, f32)> + 'a {
        let mut r = Rd { b: self.pairs, p: 0 };
        core::iter::from_fn(move || {
            let i = r.index().ok()?;
            let x = r.bytes(4).ok()?;
            Some((i, f32::from_le_bytes([x[0], x[1], x[2], x[3]])))
        })
    }

    /// The `(index, value)` pairs: without `us-chunk-wide` parse refuses escapes, so every pair is 5 bytes.
    #[cfg(not(feature = "us-chunk-wide"))]
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

    /// The speech text of a version 2 chunk (empty for version 1). The samples view runs to the end of the chunk, so
    /// the text is found after the last sample entry.
    pub fn text(&self) -> &'a [u8] {
        let mut r = Rd { b: self.samples, p: 0 };
        for _ in 0..self.sample_count {
            let Ok(size) = r.u32() else { return &[] };
            if r.bytes(size as usize).is_err() {
                return &[];
            }
        }
        match r.bytes(2) {
            Ok(s) => r.bytes(u16::from_le_bytes([s[0], s[1]]) as usize).unwrap_or(&[]),
            Err(_) => &[],
        }
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
    encode_text(values, samples, &[])
}

/// [`encode`] with Ronan speech text: version 2 when `text` is not empty.
#[cfg(any(test, feature = "std"))]
pub fn encode_text(values: &[f32; COUNT], samples: &[&[u8]], text: &[u8]) -> std::vec::Vec<u8> {
    assert!(text.len() <= u16::MAX as usize, "speech text longer than 65535 bytes");
    assert!(samples.len() <= MAX_SAMPLES, "a chunk holds at most {MAX_SAMPLES} samples");
    let mut c = encode_pairs(if text.is_empty() { VERSION } else { VERSION_TEXT }, values);
    c.push(samples.len() as u8);
    for s in samples {
        c.extend((s.len() as u32).to_le_bytes());
        c.extend(*s);
    }
    if !text.is_empty() {
        c.extend((text.len() as u16).to_le_bytes());
        c.extend(text);
    }
    c
}

/// The version byte, the count and the pairs (values that differ from the defaults) of a chunk (host side).
#[cfg(any(test, feature = "std"))]
pub fn encode_pairs(version: u8, values: &[f32; COUNT]) -> std::vec::Vec<u8> {
    let mut pairs = std::vec![];
    let mut n = 0usize;
    for (i, (&v, &d)) in values.iter().zip(DEFAULTS.iter()).enumerate() {
        if v.to_bits() != d.to_bits() {
            push_index(&mut pairs, i);
            pairs.extend(v.to_le_bytes());
            n += 1;
        }
    }
    let mut c = std::vec![version];
    push_count(&mut c, n);
    c.extend(pairs);
    c
}

/// Appends an index in its shortest form (host side).
#[cfg(any(test, feature = "std"))]
pub fn push_index(c: &mut std::vec::Vec<u8>, i: usize) {
    assert!(i <= MAX_INDEX, "index {i} is past the encodable range");
    if i < SHORT {
        c.push(i as u8);
    } else {
        c.extend([ESC + ((i - SHORT) / 256) as u8, ((i - SHORT) % 256) as u8]);
    }
}

/// Appends a pair count in its shortest form (host side).
#[cfg(any(test, feature = "std"))]
pub fn push_count(c: &mut std::vec::Vec<u8>, n: usize) {
    assert!(n <= u16::MAX as usize);
    if n < COUNT_ESC as usize {
        c.push(n as u8);
    } else {
        c.push(COUNT_ESC);
        c.extend((n as u16).to_le_bytes());
    }
}

#[cfg(test)]
mod tests;
