//! FastTracker II module reader for the mini-cauldron import (change add-cauldron-xm-import, task 5.1): header,
//! order list, patterns (unpacked to `rows × channels` cells), instruments with their envelopes and sample headers,
//! stored PCM (delta-decoded to i16; 8-bit samples scaled by 256). Strict: the FT2 2.04 layout (header 276, pattern
//! header 9, packing 0, sample header 40), the linear frequency table, and every count checked against the bytes
//! left, so a short or foreign file is an error naming its cause, never a panic.

pub mod replay;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Xm {
    pub name: String,
    pub channels: u16,
    pub restart: u16,
    /// Linear frequency table (flags bit 0); the Amiga table is rejected.
    pub linear: bool,
    /// Ticks per row and beats per minute at the start.
    pub speed: u16,
    pub bpm: u16,
    pub order: Vec<u8>,
    pub patterns: Vec<Pattern>,
    pub instruments: Vec<Instrument>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pattern {
    pub rows: u16,
    /// Row-major: `cells[row · channels + channel]`.
    pub cells: Vec<Cell>,
}

/// One pattern cell: note 1–96 (97 = key off, 0 none), instrument 1-based (0 none), volume column byte, effect and
/// its parameter as stored.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cell {
    pub note: u8,
    pub instrument: u8,
    pub volume: u8,
    pub effect: u8,
    pub param: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Instrument {
    pub name: String,
    /// Sample index per note 1–96 (0-based within `samples`).
    pub keymap: [u8; 96],
    pub vol_env: Env,
    pub pan_env: Env,
    /// Auto-vibrato type, sweep, depth, rate.
    pub vibrato: [u8; 4],
    pub fadeout: u16,
    pub samples: Vec<Sample>,
}

/// An envelope: (tick, value) points, sustain and loop point indices, flags (bit 0 on, 1 sustain, 2 loop).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Env {
    pub points: Vec<(u16, u16)>,
    pub sustain: u8,
    pub loop_start: u8,
    pub loop_end: u8,
    pub flags: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub name: String,
    /// Loop start and length in samples (not bytes).
    pub loop_start: u32,
    pub loop_len: u32,
    pub volume: u8,
    pub finetune: i8,
    /// 0 none, 1 forward, 2 ping-pong.
    pub loop_kind: u8,
    pub bits16: bool,
    pub pan: u8,
    pub relative: i8,
    /// Decoded PCM; 8-bit samples as `byte · 256`.
    pub data: Vec<i16>,
}

struct R<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> R<'a> {
    fn take(&mut self, n: usize, what: &str) -> Result<&'a [u8], String> {
        if self.b.len() - self.p < n {
            return Err(format!("truncated at byte {} ({what}: {n} bytes, {} left)", self.p, self.b.len() - self.p));
        }
        self.p += n;
        Ok(&self.b[self.p - n..self.p])
    }
    fn u8(&mut self, what: &str) -> Result<u8, String> {
        Ok(self.take(1, what)?[0])
    }
    fn u16(&mut self, what: &str) -> Result<u16, String> {
        let s = self.take(2, what)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self, what: &str) -> Result<u32, String> {
        let s = self.take(4, what)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn text(&mut self, n: usize, what: &str) -> Result<String, String> {
        let s = self.take(n, what)?;
        let end = s.iter().position(|&c| c == 0).unwrap_or(n);
        Ok(s[..end].iter().map(|&c| c as char).collect::<String>().trim_end().to_string())
    }
}

pub fn parse(b: &[u8]) -> Result<Xm, String> {
    let mut r = R { b, p: 0 };
    if r.take(17, "id")? != b"Extended Module: " {
        return Err("not an XM file (no \"Extended Module: \" id)".into());
    }
    let name = r.text(20, "song name")?;
    if r.u8("id byte")? != 0x1A {
        return Err("not an XM file (byte 37 is not 0x1A)".into());
    }
    r.take(20, "tracker name")?;
    let version = r.u16("version")?;
    if version != 0x0104 {
        return Err(format!("XM version {version:#06x} (only 0x0104)"));
    }
    let header = r.u32("header size")?;
    if header != 276 {
        return Err(format!("XM header size {header} (only 276)"));
    }
    let len = r.u16("song length")? as usize;
    let restart = r.u16("restart")?;
    let channels = r.u16("channels")?;
    let (npat, nins) = (r.u16("pattern count")? as usize, r.u16("instrument count")? as usize);
    let linear = r.u16("flags")? & 1 == 1;
    if !linear {
        return Err("Amiga frequency table (only the linear table)".into());
    }
    let (speed, bpm) = (r.u16("speed")?, r.u16("bpm")?);
    if len > 256 || channels == 0 || channels > 32 {
        return Err(format!("song length {len} or channel count {channels} out of range"));
    }
    let order = r.take(256, "order table")?[..len].to_vec();
    let mut patterns = Vec::with_capacity(npat.min(256));
    for i in 0..npat {
        let hl = r.u32("pattern header")?;
        if hl != 9 {
            return Err(format!("pattern {i} header size {hl} (only 9)"));
        }
        if r.u8("packing")? != 0 {
            return Err(format!("pattern {i}: packing type is not 0"));
        }
        let rows = r.u16("rows")?;
        if rows == 0 || rows > 256 {
            return Err(format!("pattern {i}: {rows} rows"));
        }
        let size = r.u16("pattern size")? as usize;
        let data = r.take(size, "pattern data")?;
        patterns.push(Pattern { rows, cells: unpack(data, rows as usize * channels as usize).map_err(|e| format!("pattern {i}: {e}"))? });
    }
    let mut instruments = Vec::with_capacity(nins.min(256));
    for i in 0..nins {
        instruments.push(instrument(&mut r).map_err(|e| format!("instrument {}: {e}", i + 1))?);
    }
    Ok(Xm { name, channels, restart, linear, speed, bpm, order, patterns, instruments })
}

/// FT2's packed cells: a byte with bit 7 set flags the fields that follow; otherwise it is the note of a full cell.
/// An empty pattern (no data) is all blank.
fn unpack(d: &[u8], n: usize) -> Result<Vec<Cell>, String> {
    if d.is_empty() {
        return Ok(vec![Cell::default(); n]);
    }
    let mut r = R { b: d, p: 0 };
    let mut cells = Vec::with_capacity(n);
    while r.p < d.len() {
        let f = r.u8("cell")?;
        let (flags, first) = if f & 0x80 != 0 { (f & 0x1F, None) } else { (0x1F, Some(f)) };
        let mut field = |bit: u8, what: &str| -> Result<u8, String> {
            if bit == 1 {
                if let Some(n) = first {
                    return Ok(n);
                }
            }
            if flags & bit != 0 { r.u8(what) } else { Ok(0) }
        };
        let cell = Cell { note: field(1, "note")?, instrument: field(2, "instrument")?, volume: field(4, "volume")?, effect: field(8, "effect")?, param: field(16, "parameter")? };
        cells.push(cell);
    }
    if cells.len() != n {
        return Err(format!("{} cells, rows × channels is {n}", cells.len()));
    }
    Ok(cells)
}

fn env(r: &mut R, what: &str) -> Result<[(u16, u16); 12], String> {
    let mut e = [(0, 0); 12];
    for p in &mut e {
        *p = (r.u16(what)?, r.u16(what)?);
    }
    Ok(e)
}

fn instrument(r: &mut R) -> Result<Instrument, String> {
    let start = r.p;
    let size = r.u32("instrument header size")? as usize;
    let name = r.text(22, "instrument name")?;
    r.u8("instrument type")?;
    let ns = r.u16("sample count")? as usize;
    let mut ins = Instrument { name, keymap: [0; 96], vol_env: Env::default(), pan_env: Env::default(), vibrato: [0; 4], fadeout: 0, samples: vec![] };
    if ns == 0 {
        if size < 29 {
            return Err(format!("header size {size}"));
        }
        r.take(size - 29, "instrument header")?;
        return Ok(ins);
    }
    if size < 241 {
        return Err(format!("header size {size} (at least 241 with samples)"));
    }
    let sh = r.u32("sample header size")?;
    if sh != 40 {
        return Err(format!("sample header size {sh} (only 40)"));
    }
    ins.keymap.copy_from_slice(r.take(96, "keymap")?);
    let (vp, pp) = (env(r, "volume envelope")?, env(r, "panning envelope")?);
    let (nv, np) = (r.u8("volume points")? as usize, r.u8("panning points")? as usize);
    if nv > 12 || np > 12 {
        return Err(format!("{nv} volume / {np} panning envelope points (at most 12)"));
    }
    let h = r.take(8, "envelope points")?;
    ins.vol_env = Env { points: vp[..nv].to_vec(), sustain: h[0], loop_start: h[1], loop_end: h[2], flags: h[6] };
    ins.pan_env = Env { points: pp[..np].to_vec(), sustain: h[3], loop_start: h[4], loop_end: h[5], flags: h[7] };
    ins.vibrato.copy_from_slice(r.take(4, "vibrato")?);
    ins.fadeout = r.u16("fadeout")?;
    r.take(start + size - r.p, "instrument header")?;
    let mut heads = vec![];
    for _ in 0..ns {
        let (bytes, ls, ll) = (r.u32("sample length")?, r.u32("loop start")?, r.u32("loop length")?);
        let h = r.take(6, "sample header")?;
        let (volume, finetune, kind, pan, relative) = (h[0], h[1] as i8, h[2], h[3], h[4] as i8);
        let name = r.text(22, "sample name")?;
        heads.push((bytes, ls, ll, volume, finetune, kind, pan, relative, name));
    }
    for (bytes, ls, ll, volume, finetune, kind, pan, relative, name) in heads {
        let bits16 = kind & 16 != 0;
        let w = if bits16 { 2 } else { 1 };
        if bits16 && (bytes % 2 != 0 || ls % 2 != 0 || ll % 2 != 0) {
            return Err(format!("sample \"{name}\": odd byte count in a 16-bit sample"));
        }
        let raw = r.take(bytes as usize, "sample data")?;
        let data = if bits16 {
            let mut v = 0i16;
            raw.chunks_exact(2).map(|c| { v = v.wrapping_add(i16::from_le_bytes([c[0], c[1]])); v }).collect()
        } else {
            let mut v = 0i8;
            raw.iter().map(|&c| { v = v.wrapping_add(c as i8); v as i16 * 256 }).collect()
        };
        ins.samples.push(Sample { name, loop_start: ls / w, loop_len: ll / w, volume, finetune, loop_kind: kind & 3, bits16, pan, relative, data });
    }
    Ok(ins)
}
