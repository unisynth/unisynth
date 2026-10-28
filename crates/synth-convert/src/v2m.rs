//! Farbrausch V2 `.v2m` songs (change add-v2-import): format version detection (0–6), upgrade to the version 6
//! layout, the parsed song, and the sequencer that turns it into timed MIDI events.
//!
//! The upgrade replicates V2's own `ConvertV2M` (`fr_public/v2/v2mconv.cpp`, public domain) byte for byte, including
//! its quirks: the patch count is `offsets[0] / 4`; a mod destination is remapped by a loop whose bound re-reads the
//! growing value; the output ends with 4 zero bytes. The sequencer follows `V2MPlayer::Reset/Tick/Render` and its
//! integer sample clock, including the program-change width used for pitch-bend planes. The test oracle is v2redux's
//! loader (`tools/v2-extract`).

use crate::ConvError;

pub mod write;

/// Patch parameters of format version 6 (`sounddef.h` `v2nparms`).
pub const PATCH_PARAMS: usize = 89;
/// Global parameters of format version 6 (`v2ngparms`).
pub const GLOBAL_PARAMS: usize = 23;
pub const MAX_VERSION: u8 = 6;

/// `sounddef.h` `v2parms[i].version`, in table order: a parameter is stored in a format-`v` file iff its entry ≤ v.
pub const PATCH_PARAM_VER: [u8; PATCH_PARAMS] = [
    0, 2, // voice: panning, transpose
    0, 2, 0, 0, 0, 0, // osc 1: mode, ring, transpose, detune, colour, volume
    0, 2, 0, 0, 0, 0, // osc 2
    0, 2, 0, 0, 0, 0, // osc 3
    0, 0, 0, // vcf 1: mode, cutoff, reso
    0, 0, 0, // vcf 2
    0, 3, // routing, balance
    0, 0, 0, 0, // voice dist
    0, 0, 0, 0, 0, 0, // amp EG
    0, 0, 0, 0, 0, 0, // EG 2
    0, 0, 0, 0, 0, 5, 0, // LFO 1: mode, keysync, envmode, rate, phase, polarity, amplify
    0, 0, 0, 0, 0, 5, 0, // LFO 2
    0, 0, 6, 6, 6, 6, 0, 0, 0, 1, // keysync, chanvol, aux A/B recv, aux A/B send, reverb, delay, fxroute, boost
    0, 0, 0, 0, // channel dist
    0, 0, 0, 0, 0, 0, 0, // chorus/flanger
    1, 1, 1, 1, 1, 1, 1, 1, 1, // compressor
    0, // max poly
];

/// `v2gparms[i].version`.
pub const GLOBAL_PARAM_VER: [u8; GLOBAL_PARAMS] = [0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 6];

/// `v2initsnd` (the parameter part): the values an older file gets for parameters it does not carry.
pub const PATCH_DEFAULTS: [u8; PATCH_PARAMS] = [
    64, 64, 1, 0, 64, 64, 0, 127, 0, 0, 64, 64, 32, 127, 0, 0, 64, 64, 32, 127, 1, 127, 0, 0, 64, 0, 0, 64, 0, 32, 0,
    64, 0, 64, 127, 64, 80, 0, 0, 64, 127, 64, 80, 64, 1, 1, 0, 64, 2, 0, 0, 1, 1, 0, 64, 2, 0, 127, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 32, 100, 64, 64, 64, 32, 32, 0, 0, 64, 0, 0, 1, 2, 90, 32, 20, 64, 64, 1,
];

/// `v2initglobs`.
pub const GLOBAL_DEFAULTS: [u8; GLOBAL_PARAMS] = [64, 64, 32, 127, 100, 80, 64, 64, 0, 0, 64, 0, 127, 0, 0, 1, 2, 90, 32, 20, 64, 64, 0];

pub fn patch_param_count(ver: u8) -> usize {
    PATCH_PARAM_VER.iter().filter(|&&v| v <= ver).count()
}

pub fn global_size(ver: u8) -> usize {
    GLOBAL_PARAM_VER.iter().filter(|&&v| v <= ver).count()
}

/// `sdInit`'s `v2vsizes[ver]`: parameters + mod count byte + room for 255 mods.
pub fn patch_size(ver: u8) -> usize {
    patch_param_count(ver) + 1 + 255 * 3
}

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn bad(m: impl Into<String>) -> ConvError {
    ConvError::Input(format!("v2m: {}", m.into()))
}

/// Byte ranges of one channel's event planes.
#[derive(Clone, Copy, Default, Debug)]
pub struct ChanPos {
    pub notes: (usize, usize),
    pub pcs: (usize, usize),
    pub pbs: (usize, usize),
    pub ccs: [(usize, usize); 7],
}

/// The container layout: positions and counts, all bounds-checked.
#[derive(Clone, Debug)]
struct Layout {
    gdnum: usize,
    chans: [ChanPos; 16],
    midi_end: usize,
    glob: (usize, usize),
    patch: (usize, usize),
    speech: Option<(usize, usize)>,
}

struct Cur<'a> {
    b: &'a [u8],
    pos: usize,
}

impl Cur<'_> {
    fn u32(&mut self, what: &str) -> Result<usize, ConvError> {
        if self.b.len() - self.pos < 4 {
            return Err(bad(format!("truncated at {what} (byte {})", self.pos)));
        }
        let v = u32le(self.b, self.pos) as usize;
        self.pos += 4;
        Ok(v)
    }
    fn skip(&mut self, n: usize, what: &str) -> Result<usize, ConvError> {
        let at = self.pos;
        if n > self.b.len() - self.pos {
            return Err(bad(format!("truncated in {what} (needs {n} bytes at byte {at})")));
        }
        self.pos += n;
        Ok(at)
    }
}

fn layout(b: &[u8]) -> Result<Layout, ConvError> {
    let mut c = Cur { b, pos: 0 };
    c.u32("timediv")?;
    c.u32("maxtime")?;
    let gdnum = c.u32("global event count")?;
    c.skip(gdnum.checked_mul(10).ok_or_else(|| bad("global event count"))?, "global events")?;
    let mut chans = [ChanPos::default(); 16];
    for (ch, cp) in chans.iter_mut().enumerate() {
        let n = c.u32("note count")?;
        if n == 0 {
            continue;
        }
        let what = format!("channel {} events", ch + 1);
        let plane = |c: &mut Cur, w: usize| -> Result<(usize, usize), ConvError> {
            let at = c.skip(n.checked_mul(w).ok_or_else(|| bad(what.clone()))?, &what)?;
            Ok((at, n))
        };
        cp.notes = plane(&mut c, 5)?;
        let pc = c.u32("program change count")?;
        cp.pcs = (c.skip(pc.checked_mul(4).ok_or_else(|| bad("pc count"))?, "program changes")?, pc);
        let pb = c.u32("pitch bend count")?;
        cp.pbs = (c.skip(pb.checked_mul(5).ok_or_else(|| bad("pb count"))?, "pitch bends")?, pb);
        for cc in cp.ccs.iter_mut() {
            let k = c.u32("controller count")?;
            *cc = (c.skip(k.checked_mul(4).ok_or_else(|| bad("cc count"))?, "controllers")?, k);
        }
    }
    let midi_end = c.pos;
    let gs = c.u32("globals size")?;
    if gs > 131_072 {
        return Err(bad(format!("globals size {gs} is implausible")));
    }
    let glob = (c.skip(gs, "globals")?, gs);
    let ps = c.u32("patch block size")?;
    if ps == 0 || ps > 1_048_576 {
        return Err(bad(format!("patch block size {ps} is implausible")));
    }
    let patch = (c.skip(ps, "patches")?, ps);
    // optional speech block; V2 zeroes an implausible size instead of failing
    let mut speech = None;
    if c.pos < b.len() {
        let at = c.pos;
        match c.u32("speech size") {
            Ok(s) if s <= 8192 && s <= b.len() - c.pos => speech = Some((c.pos, s)),
            _ => speech = Some((at + 4.min(b.len() - at), 0)),
        }
    }
    Ok(Layout { gdnum, chans, midi_end, glob, patch, speech })
}

/// Highest used program + 1, from the program-change and note streams (`CheckV2MVersion`'s first half).
fn used_patches(b: &[u8], l: &Layout) -> usize {
    let mut maxp = 0usize;
    for cp in &l.chans {
        let (np_at, nn) = cp.notes;
        if nn == 0 {
            continue;
        }
        let (pc_at, pcn) = cp.pcs;
        let (mut p, mut pct, mut np, mut nt) = (0u8, 0i64, 0usize, 0i64);
        let d3 = |at: usize, w: usize, i: usize| b[at + i] as i64 + 0x100 * b[at + w + i] as i64 + 0x10000 * b[at + 2 * w + i] as i64;
        for pcp in 0..pcn {
            pct += d3(pc_at, pcn, pcp);
            if pct > nt && p as usize >= maxp {
                maxp = p as usize + 1;
            }
            p = p.wrapping_add(b[pc_at + 3 * pcn + pcp]);
            while np < nn && nt <= pct {
                nt += d3(np_at, nn, np);
                np += 1;
            }
        }
        if np < nn && p as usize >= maxp {
            maxp = p as usize + 1;
        }
    }
    maxp
}

/// Format version from the global size and the patch sizes (`CheckV2MVersion`'s second half).
fn detect(b: &[u8], l: &Layout, maxp: usize) -> Option<u8> {
    let (pm, psize) = l.patch;
    let mut best = None;
    for v in 0..=MAX_VERSION {
        if l.glob.1 != global_size(v) {
            continue;
        }
        let pc = patch_param_count(v);
        let mut ok = true;
        for p in 0..maxp.saturating_sub(1) {
            if (p + 2) * 4 > psize {
                ok = false;
                break;
            }
            let (o0, o1) = (u32le(b, pm + 4 * p) as usize, u32le(b, pm + 4 * (p + 1)) as usize);
            if o1 <= o0 || o1 > psize || o0 + pc >= psize {
                ok = false;
                break;
            }
            let d = (o1 - o0) as isize - (pc as isize + 1);
            if d < 0 || d % 3 != 0 || (d / 3) as u8 as isize != d / 3 || b[pm + o0 + pc] as isize != d / 3 {
                ok = false;
                break;
            }
        }
        if ok {
            best = Some(v);
        }
    }
    best
}

/// Detects the format version and returns `(version, version-6 bytes)`, byte-identical to V2's `ConvertV2M`.
pub fn canonicalize(b: &[u8]) -> Result<(u8, Vec<u8>), ConvError> {
    if b.len() < 16 {
        return Err(bad(format!("{} bytes is too short", b.len())));
    }
    let l = layout(b)?;
    let maxp = used_patches(b, &l);
    if maxp == 0 {
        return Err(bad("no channel plays a patch (no notes or program changes)"));
    }
    let Some(ver) = detect(b, &l, maxp) else {
        return Err(bad(format!(
            "unknown format version: globals size {} and patch sizes match no version 0-6 (globals per version: {:?})",
            l.glob.1,
            (0..=MAX_VERSION).map(global_size).collect::<Vec<_>>()
        )));
    };
    if ver == MAX_VERSION {
        let mut out = b.to_vec();
        out.extend_from_slice(&[0; 4]);
        return Ok((ver, out));
    }
    let (pm, psize) = l.patch;
    let maxp = u32le(b, pm) as usize / 4;
    if maxp == 0 || maxp * 4 > psize {
        return Err(bad("patch offset table is malformed"));
    }
    let gdiff = global_size(MAX_VERSION) - global_size(ver);
    let pdiff = patch_size(MAX_VERSION) - patch_size(ver);
    let newsize = b.len() + gdiff + maxp * pdiff;
    let mut out = Vec::with_capacity(newsize + 4);
    out.extend_from_slice(&b[..l.midi_end]);
    out.extend_from_slice(&(GLOBAL_PARAMS as u32).to_le_bytes());
    let mut g = GLOBAL_DEFAULTS;
    let mut old = b[l.glob.0..l.glob.0 + l.glob.1].iter();
    for (i, gv) in g.iter_mut().enumerate() {
        if GLOBAL_PARAM_VER[i] <= ver {
            *gv = *old.next().ok_or_else(|| bad("globals shorter than their version"))?;
        }
    }
    out.extend_from_slice(&g);
    out.extend_from_slice(&((psize + maxp * pdiff) as u32).to_le_bytes());
    for p in 0..maxp {
        out.extend_from_slice(&((u32le(b, pm + 4 * p) as usize + p * pdiff) as u32).to_le_bytes());
    }
    for p in 0..maxp {
        let off = u32le(b, pm + 4 * p) as usize;
        if off >= psize {
            return Err(bad(format!("patch {p} offset {off} is outside the patch block")));
        }
        let mut src = pm + off;
        let get = |src: &mut usize| -> Result<u8, ConvError> {
            let v = *b.get(*src).ok_or_else(|| bad(format!("patch {p} runs past the end of the file")))?;
            *src += 1;
            Ok(v)
        };
        let mut params = PATCH_DEFAULTS;
        for (i, pv) in params.iter_mut().enumerate() {
            if PATCH_PARAM_VER[i] <= ver {
                *pv = get(&mut src)?;
            }
        }
        out.extend_from_slice(&params);
        let modnum = get(&mut src)?;
        out.push(modnum);
        for _ in 0..modnum {
            let (s, a, mut d) = (get(&mut src)?, get(&mut src)?, get(&mut src)?);
            // ConvertV2M: `for (k = 0; k <= dest; k++) if (version[k] > vdelta) dest++;` (bound grows with dest)
            let mut k = 0usize;
            while k <= d as usize {
                if k < PATCH_PARAMS && PATCH_PARAM_VER[k] > ver {
                    d = d.wrapping_add(1);
                    if d == 0 {
                        break;
                    }
                }
                k += 1;
            }
            out.extend_from_slice(&[s, a, d]);
        }
    }
    let (sp_at, sp_len) = l.speech.unwrap_or((0, 0));
    out.extend_from_slice(&(sp_len as u32).to_le_bytes());
    out.extend_from_slice(&b[sp_at..sp_at + sp_len]);
    if out.len() > newsize + 4 {
        return Err(bad("upgraded song is larger than V2's converter allows"));
    }
    out.resize(newsize + 4, 0);
    Ok((ver, out))
}

/// One V2 patch (version 6 layout).
#[derive(Clone, Debug, PartialEq)]
pub struct Patch {
    pub params: [u8; PATCH_PARAMS],
    /// Modulation entries `(source, amount, destination)`: amount 64 = 0, destination = parameter index.
    pub mods: Vec<(u8, u8, u8)>,
}

/// A parsed `.v2m` (any version, upgraded to version 6).
#[derive(Clone, Debug)]
pub struct V2m {
    /// The source format version (0–6).
    pub version: u8,
    /// The version 6 bytes (`canonicalize`).
    pub bytes: Vec<u8>,
    pub timediv: u32,
    pub maxtime: u32,
    pub globals: [u8; GLOBAL_PARAMS],
    pub patches: Vec<Patch>,
    /// Ronan phoneme text: `speech[i]` is the text of index `i` (V2 `speechptrs`); empty when the song has none.
    pub speech: Vec<Vec<u8>>,
    layout: Layout,
}

pub fn parse(input: &[u8]) -> Result<V2m, ConvError> {
    let (version, bytes) = canonicalize(input)?;
    let l = layout(&bytes)?;
    let b = &bytes;
    if l.glob.1 < GLOBAL_PARAMS {
        return Err(bad("version 6 globals are short"));
    }
    let mut globals = [0; GLOBAL_PARAMS];
    globals.copy_from_slice(&b[l.glob.0..l.glob.0 + GLOBAL_PARAMS]);
    let (pm, psize) = l.patch;
    let n = u32le(b, pm) as usize / 4;
    let mut patches = Vec::with_capacity(n);
    for p in 0..n {
        let off = u32le(b, pm + 4 * p) as usize;
        if off + PATCH_PARAMS + 1 > psize {
            return Err(bad(format!("patch {p} is outside the patch block")));
        }
        let at = pm + off;
        let mut params = [0; PATCH_PARAMS];
        params.copy_from_slice(&b[at..at + PATCH_PARAMS]);
        let mn = b[at + PATCH_PARAMS] as usize;
        let m0 = at + PATCH_PARAMS + 1;
        if m0 + 3 * mn > pm + psize {
            return Err(bad(format!("patch {p}: modulation list is outside the patch block")));
        }
        let mods = (0..mn).map(|i| (b[m0 + 3 * i], b[m0 + 3 * i + 1], b[m0 + 3 * i + 2])).collect();
        patches.push(Patch { params, mods });
    }
    let speech = match l.speech {
        Some((at, len)) if len > 0 => speech_texts(&b[at..at + len]),
        _ => vec![],
    };
    Ok(V2m { version, timediv: u32le(b, 0), maxtime: u32le(b, 4), globals, patches, speech, bytes: bytes.clone(), layout: l })
}

/// V2MPlayer: the speech block is `u32 n`, then `n` u32 offsets (from the block start) of zero-terminated strings.
fn speech_texts(sp: &[u8]) -> Vec<Vec<u8>> {
    if sp.len() < 4 {
        return vec![];
    }
    let n = (u32le(sp, 0) as usize).min(256).min((sp.len() - 4) / 4);
    (0..n)
        .map(|i| {
            let o = u32le(sp, 4 + 4 * i) as usize;
            if o >= sp.len() {
                return vec![];
            }
            let end = sp[o..].iter().position(|&c| c == 0).map_or(sp.len(), |e| o + e);
            sp[o..end].to_vec()
        })
        .collect()
}

/// A MIDI event the V2 sequencer sends to the synth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Msg {
    /// Velocity 0 is a note-off (V2 sends note-offs as note-ons with velocity 0).
    Note { key: u8, vel: u8 },
    Program(u8),
    /// Controller 1–7 (V2 stores only these).
    Ctl { ctl: u8, val: u8 },
    /// 14-bit bend as V2 sends it: low and high 7-bit bytes (wrapping deltas, not masked).
    Bend { lo: u8, hi: u8 },
}

/// One event at an absolute output frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub frame: u64,
    pub chan: u8,
    pub msg: Msg,
}

/// Song length in frames: where V2's player stops (its last tick), at `rate`.
#[derive(Clone, Debug)]
pub struct Timeline {
    pub events: Vec<Event>,
    pub end_frame: u64,
}

impl V2m {
    /// Replays V2's sequencer at `rate` Hz: the same event order per tick (per channel: program change, controllers
    /// 1–7, pitch bend, notes) and the same integer sample clock (`UpdateSampleDelta`, 32.32 remainder carry). The
    /// first tick happens at frame 0, whatever its song time.
    pub fn timeline(&self, rate: u32) -> Timeline {
        let b = &self.bytes;
        let l = &self.layout;
        let delta = |at: usize, w: usize, i: usize| -> u32 { b[at + i] as u32 + ((b[at + w + i] as u32) << 8) + ((b[at + 2 * w + i] as u32) << 16) };
        let timediv2 = 10_000u32.wrapping_mul(self.timediv);
        #[derive(Default, Clone, Copy)]
        struct Str {
            at: usize,
            n: usize,
            i: usize,
            next: u32,
            last: u8,
            last2: u8,
        }
        let mut time: u32 = 0;
        let mut nexttime: u32 = u32::MAX;
        // global events
        let gat = 12usize;
        let gd = l.gdnum;
        let mut g = Str { at: gat, n: gd, ..Default::default() };
        // UPDATENT
        let upd = |s: &mut Str, w: usize, time: u32, nexttime: &mut u32| {
            if s.i < s.n {
                s.next = time.wrapping_add(delta(s.at + s.i, w, 0));
                if s.next < *nexttime {
                    *nexttime = s.next;
                }
            }
        };
        upd(&mut g, gd, time, &mut nexttime);
        let mut ch_s = [[Str::default(); 10]; 16]; // 0 notes, 1 pc, 2 pb, 3.. cc
        for (ch, cp) in l.chans.iter().enumerate() {
            if cp.notes.1 == 0 {
                continue;
            }
            let s = &mut ch_s[ch];
            s[0] = Str { at: cp.notes.0, n: cp.notes.1, ..Default::default() };
            upd(&mut s[0], cp.notes.1, time, &mut nexttime);
            s[1] = Str { at: cp.pcs.0, n: cp.pcs.1, ..Default::default() };
            upd(&mut s[1], cp.pcs.1, time, &mut nexttime);
            s[2] = Str { at: cp.pbs.0, n: cp.pbs.1, ..Default::default() };
            // V2MPlayer::Reset uses the program-change count as the plane width here
            if s[2].i < s[2].n {
                let w = cp.pcs.1;
                s[2].next = time.wrapping_add(b.get(s[2].at).map_or(0, |_| {
                    let r = |k: usize| *b.get(s[2].at + k).unwrap_or(&0) as u32;
                    r(0) + (r(w) << 8) + (r(2 * w) << 16)
                }));
                if s[2].next < nexttime {
                    nexttime = s[2].next;
                }
            }
            for k in 0..7 {
                s[3 + k] = Str { at: cp.ccs[k].0, n: cp.ccs[k].1, ..Default::default() };
                upd(&mut s[3 + k], cp.ccs[k].1, time, &mut nexttime);
            }
        }
        let mut usecs: u32 = 5000u32.wrapping_mul(rate);
        let (mut smplrem, mut frame) = (0u32, 0u64);
        let mut events = vec![];
        // UPDATENT2 then UPDATENT3 after consuming one event of a stream
        let adv = |s: &mut Str, w: usize, time: u32| {
            if s.i < s.n {
                let d = delta(s.at + s.i, w, 0);
                if d != 0 {
                    s.next = time.wrapping_add(d);
                }
            }
        };
        let chk = |s: &Str, nexttime: &mut u32| {
            if s.i < s.n && s.next < *nexttime {
                *nexttime = s.next;
            }
        };
        loop {
            // Tick
            time = nexttime;
            nexttime = u32::MAX;
            if g.i < gd && time == g.next {
                let gi = g.i;
                usecs = u32le(b, gat + 3 * gd + 4 * gi).wrapping_mul(rate / 100);
                g.i += 1;
                adv(&mut g, gd, time);
            }
            chk(&g, &mut nexttime);
            for (ch, cp) in l.chans.iter().enumerate() {
                if cp.notes.1 == 0 {
                    continue;
                }
                let chan = ch as u8;
                let s = &mut ch_s[ch];
                // 1. program change
                let pcn = cp.pcs.1;
                if s[1].i < pcn && time == s[1].next {
                    s[1].last = s[1].last.wrapping_add(b[s[1].at + 3 * pcn + s[1].i]);
                    events.push(Event { frame, chan, msg: Msg::Program(s[1].last) });
                    s[1].i += 1;
                    adv(&mut s[1], pcn, time);
                }
                chk(&s[1], &mut nexttime);
                // 2. controllers
                for k in 0..7 {
                    let n = cp.ccs[k].1;
                    let c = &mut s[3 + k];
                    if c.i < n && time == c.next {
                        c.last = c.last.wrapping_add(b[c.at + 3 * n + c.i]);
                        events.push(Event { frame, chan, msg: Msg::Ctl { ctl: k as u8 + 1, val: c.last } });
                        c.i += 1;
                        adv(c, n, time);
                    }
                    chk(c, &mut nexttime);
                }
                // 3. pitch bend (value planes at the program-change width, as V2 reads them)
                let pbn = cp.pbs.1;
                if s[2].i < pbn && time == s[2].next {
                    let base = s[2].at + s[2].i;
                    let r = |k: usize| *b.get(base + k).unwrap_or(&0);
                    s[2].last = s[2].last.wrapping_add(r(3 * pcn));
                    s[2].last2 = s[2].last2.wrapping_add(r(4 * pcn));
                    events.push(Event { frame, chan, msg: Msg::Bend { lo: s[2].last, hi: s[2].last2 } });
                    s[2].i += 1;
                    adv(&mut s[2], pbn, time);
                }
                chk(&s[2], &mut nexttime);
                // 4. notes
                let nn = cp.notes.1;
                while s[0].i < nn && time == s[0].next {
                    s[0].last = s[0].last.wrapping_add(b[s[0].at + 3 * nn + s[0].i]);
                    s[0].last2 = s[0].last2.wrapping_add(b[s[0].at + 4 * nn + s[0].i]);
                    events.push(Event { frame, chan, msg: Msg::Note { key: s[0].last, vel: s[0].last2 } });
                    s[0].i += 1;
                    adv(&mut s[0], nn, time); // a zero delta keeps `next`: the next note is in this tick too
                }
                chk(&s[0], &mut nexttime);
            }
            if nexttime == u32::MAX {
                break;
            }
            // UpdateSampleDelta
            let prod = nexttime.wrapping_sub(time) as u64 * usecs as u64;
            let (quot, rem) = ((prod / timediv2 as u64) as u32, (prod % timediv2 as u64) as u32);
            let newrem = smplrem.wrapping_add(rem);
            let carry = (newrem < smplrem) as u32;
            smplrem = newrem;
            frame += quot.wrapping_add(carry) as u64;
        }
        Timeline { events, end_frame: frame }
    }
}

#[cfg(test)]
mod tests;
