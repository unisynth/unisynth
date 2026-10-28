//! `.v2m` writer for synthetic test songs (task 1.2): one-patch phrases for module parity, and files of older format
//! versions for the upgrade tests. Writes V2's container: planar delta-coded event streams, one tempo event,
//! globals, patches and an optional speech block.

use super::{global_size, Patch, GLOBAL_PARAMS, GLOBAL_PARAM_VER, PATCH_PARAMS, PATCH_PARAM_VER};

/// Events of one MIDI channel, at absolute ticks (sorted per stream).
#[derive(Clone, Debug, Default)]
pub struct ChanW {
    /// `(tick, key, velocity)`; velocity 0 = note-off.
    pub notes: Vec<(u32, u8, u8)>,
    /// `(tick, program)`.
    pub programs: Vec<(u32, u8)>,
    /// Controllers 1–7: `(tick, value)`.
    pub ctls: [Vec<(u32, u8)>; 7],
    /// `(tick, lo, hi)`.
    pub bends: Vec<(u32, u8, u8)>,
}

#[derive(Clone, Debug)]
pub struct SongW {
    /// Format version to write (0–6). Older versions leave out the parameters added later; mod destinations are
    /// written as given (in that version's parameter numbering).
    pub version: u8,
    /// Ticks per quarter note.
    pub timediv: u32,
    /// Microseconds per quarter note.
    pub usecs: u32,
    pub globals: [u8; GLOBAL_PARAMS],
    pub patches: Vec<Patch>,
    pub chans: [ChanW; 16],
    /// Speech texts (`speechptrs`), or none.
    pub speech: Vec<Vec<u8>>,
}

impl SongW {
    pub fn new(patches: Vec<Patch>) -> Self {
        SongW {
            version: 6,
            timediv: 96,
            usecs: 500_000,
            globals: super::GLOBAL_DEFAULTS,
            patches,
            chans: Default::default(),
            speech: vec![],
        }
    }
}

/// Planar stream: 3 delta-time planes, then one plane per value (each value delta-coded from the previous one).
fn planes(out: &mut Vec<u8>, ev: &[(u32, Vec<u8>)]) {
    let mut t = 0u32;
    let dts: Vec<u32> = ev.iter().map(|(tt, _)| {
        let d = tt - t;
        t = *tt;
        d
    }).collect();
    for k in 0..3 {
        out.extend(dts.iter().map(|d| (d >> (8 * k)) as u8));
    }
    let w = ev.first().map_or(0, |e| e.1.len());
    for k in 0..w {
        let mut last = 0u8;
        out.extend(ev.iter().map(|e| {
            let d = e.1[k].wrapping_sub(last);
            last = e.1[k];
            d
        }));
    }
}

pub fn write(s: &SongW) -> Vec<u8> {
    let mut o = vec![];
    let maxtime = s.chans.iter().flat_map(|c| c.notes.iter().map(|n| n.0)).max().unwrap_or(0);
    o.extend(s.timediv.to_le_bytes());
    o.extend(maxtime.to_le_bytes());
    // one global event at tick 0: time (3 planes), usecs u32, num, den, tpq
    o.extend(1u32.to_le_bytes());
    o.extend([0, 0, 0]);
    o.extend(s.usecs.to_le_bytes());
    o.extend([4, 4, 8]);
    for c in &s.chans {
        o.extend((c.notes.len() as u32).to_le_bytes());
        if c.notes.is_empty() {
            continue;
        }
        planes(&mut o, &c.notes.iter().map(|&(t, k, v)| (t, vec![k, v])).collect::<Vec<_>>());
        o.extend((c.programs.len() as u32).to_le_bytes());
        planes(&mut o, &c.programs.iter().map(|&(t, p)| (t, vec![p])).collect::<Vec<_>>());
        o.extend((c.bends.len() as u32).to_le_bytes());
        planes(&mut o, &c.bends.iter().map(|&(t, l, h)| (t, vec![l, h])).collect::<Vec<_>>());
        for cc in &c.ctls {
            o.extend((cc.len() as u32).to_le_bytes());
            planes(&mut o, &cc.iter().map(|&(t, v)| (t, vec![v])).collect::<Vec<_>>());
        }
    }
    let v = s.version;
    o.extend((global_size(v) as u32).to_le_bytes());
    o.extend((0..GLOBAL_PARAMS).filter(|&i| GLOBAL_PARAM_VER[i] <= v).map(|i| s.globals[i]));
    let bodies: Vec<Vec<u8>> = s
        .patches
        .iter()
        .map(|p| {
            let mut b: Vec<u8> = (0..PATCH_PARAMS).filter(|&i| PATCH_PARAM_VER[i] <= v).map(|i| p.params[i]).collect();
            b.push(p.mods.len() as u8);
            for &(a, c, d) in &p.mods {
                b.extend([a, c, d]);
            }
            b
        })
        .collect();
    let table = 4 * bodies.len();
    let size = table + bodies.iter().map(Vec::len).sum::<usize>();
    o.extend((size as u32).to_le_bytes());
    let mut off = table;
    for b in &bodies {
        o.extend((off as u32).to_le_bytes());
        off += b.len();
    }
    for b in &bodies {
        o.extend(b);
    }
    // speech block: count, offset table, zero-terminated strings
    let mut sp = vec![];
    if !s.speech.is_empty() {
        sp.extend((s.speech.len() as u32).to_le_bytes());
    }
    let mut off = 4 + 4 * s.speech.len();
    for t in &s.speech {
        sp.extend((off as u32).to_le_bytes());
        off += t.len() + 1;
    }
    for t in &s.speech {
        sp.extend(t);
        sp.push(0);
    }
    o.extend((sp.len() as u32).to_le_bytes());
    o.extend(sp);
    o
}
