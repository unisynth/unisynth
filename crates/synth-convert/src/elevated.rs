//! Songs of Mentor's 4k synthesizer (Elevated, Atrium) as unified `synth-mix` songs (change add-elevated-import).
//!
//! The input is the generated `music.asm` (NASM data: `_tempo_data`, `_pattern_data`, `_sequence_data`,
//! `_machine_tree`) and the `music.h` beside it (`NUM_ROWS`, the sequence stride the synth uses). The machine tree is
//! a stack program over the whole song, read here symbolically (design D7):
//!
//! - a synth pushes a track with its unified instrument and its notes;
//! - an effect machine appends its device to the top track's chain;
//! - a mixer `(g_top, g_below)` pops two tracks and pushes a track that receives them at those gains;
//! - the one track left is the master.
//!
//! The machine table is a property of the synth's build: 8 machines (Atrium: synth, delay, filter, compressor, mixer,
//! sine, atan, allpass) or 7 without atan (Elevated, where 6 is the allpass). Each tree walks to exactly its last byte
//! only under its own table, which picks it unless `--table` says. Behaviour and laws: `research/elevated-synth.md`,
//! `docs/synth/elevated.md`.

use crate::model::{Dev, Ev, SongM, Trk};
use crate::{ConvError, Converted};
use unisynth::law::host as inv;
use unisynth::params::{self as p, e, el, o, shape as sh, v};

const RATE: f64 = 44_100.0;
/// A synth's record bytes; and the other machines'.
const SYNTH_LEN: usize = 72;

/// The data of a `music.asm` and its `music.h`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Music {
    /// Frames per step (`_tempo_data`'s first value, the header's `MAX_NOTE_SAMPLES`).
    pub step: u32,
    /// Rows per synth in the sequence (`NUM_ROWS`).
    pub rows: usize,
    pub patterns: Vec<u8>,
    pub sequence: Vec<u8>,
    pub tree: Vec<u8>,
}

fn input(m: impl Into<String>) -> ConvError {
    ConvError::Input(m.into())
}

/// One `DB`/`DW`/`DD` value: decimal, `0x` hex, negative, or (`DD`) a float literal; `width` bytes, little-endian.
fn value(tok: &str, width: usize, line: usize) -> Result<Vec<u8>, ConvError> {
    let t = tok.trim();
    let bad = || input(format!("music.asm line {line}: cannot read value '{t}'"));
    let int = if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        i64::from_str_radix(h, 16).ok()
    } else if let Some(h) = t.strip_prefix("-0x") {
        i64::from_str_radix(h, 16).ok().map(|x| -x)
    } else {
        t.parse::<i64>().ok()
    };
    match (int, width) {
        (Some(x), w) => Ok(x.to_le_bytes()[..w].to_vec()),
        (None, 4) => t.parse::<f32>().map(|f| f.to_le_bytes().to_vec()).map_err(|_| bad()),
        _ => Err(bad()),
    }
}

/// Reads the four labels of a `music.asm` and `NUM_ROWS` from its `music.h`.
pub fn parse(asm: &str, header: &str) -> Result<Music, ConvError> {
    let mut sections: Vec<(String, Vec<u8>)> = vec![];
    for (n, raw) in asm.lines().enumerate() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(label) = line.strip_suffix(':').filter(|l| l.starts_with('_') && !l.contains(char::is_whitespace)) {
            sections.push((label.to_string(), vec![]));
            continue;
        }
        let mut it = line.splitn(2, char::is_whitespace);
        let width = match it.next().map(|w| w.to_ascii_lowercase()) {
            Some(w) if w == "db" => 1,
            Some(w) if w == "dw" => 2,
            Some(w) if w == "dd" => 4,
            _ => continue, // section, global, extern: not data
        };
        let Some((_, data)) = sections.last_mut() else {
            return Err(input(format!("music.asm line {}: data before any label", n + 1)));
        };
        for tok in it.next().unwrap_or("").split(',').filter(|t| !t.trim().is_empty()) {
            data.extend(value(tok, width, n + 1)?);
        }
    }
    let get = |name: &str| {
        sections.iter().find(|s| s.0 == name).map(|s| s.1.clone()).ok_or_else(|| input(format!("music.asm: no {name} section")))
    };
    let tempo = get("_tempo_data")?;
    if tempo.len() < 4 {
        return Err(input("music.asm: _tempo_data is shorter than its step"));
    }
    let step = u32::from_le_bytes([tempo[0], tempo[1], tempo[2], tempo[3]]);
    if step == 0 {
        return Err(input("music.asm: the step is 0 frames"));
    }
    let rows = header
        .lines()
        .find_map(|l| {
            let l = l.split("//").next()?.trim();
            let rest = l.strip_prefix("static const int NUM_ROWS")?;
            rest.trim().strip_prefix("equ")?.trim().trim_end_matches(';').trim().parse::<usize>().ok()
        })
        .ok_or_else(|| input("music.h: no NUM_ROWS"))?;
    Ok(Music { step, rows, patterns: get("_pattern_data")?, sequence: get("_sequence_data")?, tree: get("_machine_tree")? })
}

/// One oscillator pair of a synth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pair {
    /// 1 sine, 2 square, other saw.
    pub wave: u8,
    /// 1 off, 2 add, 3 subtract, 4 multiply.
    pub op: u8,
    pub shift: f32,
    pub detune: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Synth {
    /// Attack, decay, sustain, release in frames.
    pub seg: [i32; 4],
    pub noise: f32,
    pub freq_exp: f32,
    /// Cycles per sample.
    pub base: f32,
    pub volume: f32,
    pub stereo: f32,
    pub pairs: [Pair; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Machine {
    Synth(Synth),
    Delay { len: i32, feedback: f32 },
    Filter { cutoff: f32, q: f32, rate1: f32, depth1: f32, rate2: f32, depth2: f32, dry: f32, output: i32 },
    Compressor { threshold: f32, ratio: f32, offset: f32 },
    Mixer { top: f32, below: f32 },
    Sine { a: f32, b: f32 },
    Atan { a: f32, b: f32 },
    Allpass { len: i32, feedback: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Synth,
    Delay,
    Filter,
    Compressor,
    Mixer,
    Sine,
    Atan,
    Allpass,
}

/// The two known machine tables.
const FULL: [Op; 8] = [Op::Synth, Op::Delay, Op::Filter, Op::Compressor, Op::Mixer, Op::Sine, Op::Atan, Op::Allpass];
const NO_ATAN: [Op; 7] = [Op::Synth, Op::Delay, Op::Filter, Op::Compressor, Op::Mixer, Op::Sine, Op::Allpass];

/// The machine table of `n` entries (7 or 8).
fn table(n: u8) -> Option<&'static [Op]> {
    match n {
        8 => Some(&FULL),
        7 => Some(&NO_ATAN),
        _ => None,
    }
}

fn size(op: Op) -> usize {
    match op {
        Op::Synth => SYNTH_LEN,
        Op::Delay | Op::Compressor | Op::Allpass => 12,
        Op::Filter => 32,
        Op::Mixer | Op::Sine | Op::Atan => 8,
    }
}

/// Walks `tree` with machine table `t`: the machines with their byte offsets, or why it does not walk (an unknown or
/// truncated machine, a stack that underflows, more than one entry left, bytes after the terminator).
fn walk(tree: &[u8], t: &[Op]) -> Result<Vec<(usize, Machine)>, String> {
    let f = |at: usize| f32::from_le_bytes([tree[at], tree[at + 1], tree[at + 2], tree[at + 3]]);
    let i = |at: usize| i32::from_le_bytes([tree[at], tree[at + 1], tree[at + 2], tree[at + 3]]);
    let (mut at, mut op, mut depth) = (0usize, Op::Synth, 0usize);
    let mut out = vec![];
    loop {
        if at + size(op) > tree.len() {
            return Err(format!("the {op:?} machine at byte {at} is truncated"));
        }
        let m = match op {
            Op::Synth => {
                let pair = |j: usize| {
                    let b = at + 36 + 12 * j;
                    Pair { wave: tree[b], op: tree[b + 1], shift: f(b + 4), detune: f(b + 8) }
                };
                Machine::Synth(Synth {
                    seg: [i(at), i(at + 4), i(at + 8), i(at + 12)],
                    noise: f(at + 16),
                    freq_exp: f(at + 20),
                    base: f(at + 24),
                    volume: f(at + 28),
                    stereo: f(at + 32),
                    pairs: [pair(0), pair(1), pair(2)],
                })
            }
            Op::Delay => Machine::Delay { len: i(at + 4), feedback: f(at + 8) },
            Op::Allpass => Machine::Allpass { len: i(at + 4), feedback: f(at + 8) },
            Op::Filter => Machine::Filter {
                cutoff: f(at),
                q: f(at + 4),
                rate1: f(at + 8),
                depth1: f(at + 12),
                rate2: f(at + 16),
                depth2: f(at + 20),
                dry: f(at + 24),
                output: i(at + 28),
            },
            Op::Compressor => Machine::Compressor { threshold: f(at), ratio: f(at + 4), offset: f(at + 8) },
            Op::Mixer => Machine::Mixer { top: f(at), below: f(at + 4) },
            Op::Sine => Machine::Sine { a: f(at), b: f(at + 4) },
            Op::Atan => Machine::Atan { a: f(at), b: f(at + 4) },
        };
        depth = match op {
            Op::Synth => depth + 1,
            Op::Mixer if depth < 2 => return Err(format!("the mixer at byte {at} has fewer than two tracks to mix")),
            Op::Mixer => depth - 1,
            _ if depth == 0 => return Err(format!("the {op:?} at byte {at} has no track")),
            _ => depth,
        };
        out.push((at, m));
        at += size(op);
        let Some(&b) = tree.get(at) else {
            return Err(format!("the tree ends at byte {at} without its terminator"));
        };
        if b >= 128 {
            if at + 1 != tree.len() {
                return Err(format!("{} bytes follow the terminator at byte {at}", tree.len() - at - 1));
            }
            if depth != 1 {
                return Err(format!("the tree ends with {depth} tracks on the stack, not one"));
            }
            return Ok(out);
        }
        op = *t.get(b as usize).ok_or_else(|| format!("unknown machine opcode {b} at byte {at}"))?;
        at += 1;
    }
}

/// The machines of `tree`: under table `force` (7 or 8), or the one table it walks under.
pub fn machines(tree: &[u8], force: Option<u8>) -> Result<(u8, Vec<(usize, Machine)>), ConvError> {
    if let Some(n) = force {
        let t = table(n).ok_or_else(|| input(format!("--table {n}: the tables are 7 and 8")))?;
        return walk(tree, t).map(|m| (n, m)).map_err(|e| input(format!("machine tree (table {n}): {e}")));
    }
    match (walk(tree, &FULL), walk(tree, &NO_ATAN)) {
        (Ok(m), Err(_)) => Ok((8, m)),
        (Err(_), Ok(m)) => Ok((7, m)),
        // opcodes 0–5 mean the same in both tables: only a tree that reads differently is ambiguous
        (Ok(a), Ok(b)) if a == b => Ok((8, a)),
        (Ok(_), Ok(_)) => Err(input("machine tree: it walks under both the 7- and the 8-machine table, differently; pass --table")),
        (Err(a), Err(b)) => Err(input(format!("machine tree: {a} (8-machine table); {b} (7-machine table)"))),
    }
}

/// Options of [`convert`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// The machine table (7 or 8); `None` detects it.
    pub table: Option<u8>,
    /// Keep only synth machine `k` (tree order): every other synth's volume is 0, as the reference harness does.
    pub solo: Option<usize>,
    /// Seconds after the song's end.
    pub tail: f64,
}

/// The note events of synth `k`: `(frame, note, Some(velocity) | None for a note-off)`.
pub fn notes(m: &Music, k: usize) -> Result<Vec<(u64, u8, Option<u8>)>, ConvError> {
    let mut out = vec![];
    let mut sounding: Option<u8> = None;
    for r in 0..m.rows {
        let pat = *m
            .sequence
            .get(k * m.rows + r)
            .ok_or_else(|| input(format!("_sequence_data has no row {r} for synth {k} ({} bytes)", m.sequence.len())))?
            as usize;
        for s in 0..16 {
            let n = *m
                .patterns
                .get(pat * 16 + s)
                .ok_or_else(|| input(format!("synth {k} row {r}: pattern {pat} is outside _pattern_data")))?;
            let frame = (16 * r + s) as u64 * m.step as u64;
            match n {
                0 => {}
                0x7f => {
                    // the original renders the note with zero envelope steps: silence, a cut
                    if let Some(old) = sounding.take() {
                        out.push((frame, old, None));
                    }
                }
                n if n >= 0x80 => return Err(input(format!("synth {k} row {r} step {s}: note byte {n} is above 127"))),
                n => {
                    out.push((frame, n, Some(100)));
                    sounding = Some(n);
                }
            }
        }
    }
    Ok(out)
}

/// The unified instrument chunk of a synth machine.
pub fn instrument(s: &Synth, k: usize, w: &mut Vec<String>) -> Result<Vec<u8>, ConvError> {
    if let Some(i) = s.seg.iter().position(|&x| x <= 0) {
        return Err(input(format!("synth {k}: its {} segment is {} frames (the original divides by it)", ["attack", "decay", "sustain", "release"][i], s.seg[i])));
    }
    let mut val = p::DEFAULTS;
    // one voice, each new note cuts the last (the original overwrites); full gain at any velocity
    val[v::CAPACITY] = inv::inv_count(1, 1, 64);
    val[v::VELOCITY] = 0.0;
    // L = volume·x, R = volume·stereo·x: the voice pan sets the ratio (equal-power), the master the level
    let st = if s.stereo < 0.0 {
        w.push(format!("synth {k}: stereo factor {} is negative; its magnitude is used", s.stereo));
        -s.stereo
    } else {
        s.stereo
    };
    let pan = st * st / (1.0 + st * st);
    let gain = s.volume.abs() * (1.0 + st * st).sqrt();
    if s.volume < 0.0 {
        w.push(format!("synth {k}: volume {} is negative; its magnitude is used", s.volume));
    }
    if gain > 16.0 {
        w.push(format!("synth {k}: gain {gain} is above the instrument's 16; clipped"));
    }
    val[v::PAN] = pan;
    val[v::MASTER] = inv::inv_gain(gain.min(16.0));
    // the three pairs
    for (j, pr) in s.pairs.iter().enumerate() {
        if !matches!(pr.op, 2..=4) {
            if pr.op != 1 {
                w.push(format!("synth {k} pair {}: op {} is not one the original knows; the pair is off", j + 1, pr.op));
            }
            continue;
        }
        let f = |fld: usize| p::osc(j, fld);
        val[f(o::TYPE)] = p::osc_type::encode(p::osc_type::PAIR);
        let wave = match pr.wave {
            1 => 0,
            2 => 1,
            _ => 2,
        };
        val[f(o::SHAPE + sh::PAIR_WAVE)] = inv::inv_choice(wave, 3);
        val[f(o::SHAPE + sh::PAIR_SHIFT)] = pr.shift - pr.shift.floor();
        if !(0.0..=2.0).contains(&pr.detune) {
            w.push(format!("synth {k} pair {}: detune {} is outside 0..2; clamped", j + 1, pr.detune));
        }
        val[f(o::SHAPE + sh::PAIR_DETUNE)] = (pr.detune / 2.0).clamp(0.0, 1.0);
        val[f(o::SHAPE + sh::PAIR_POLARITY)] = inv::inv_choice((pr.op == 3) as usize, 2);
        if pr.op == 4 {
            val[f(o::COMBINE)] = inv::inv_choice(p::combine::MUL, p::combine::COUNT);
        }
    }
    // noise into the sum, the same on both channels
    if s.noise != 0.0 {
        val[v::NOISE_LEVEL] = inv::inv_gain(s.noise.abs());
        val[p::NOISE + p::n::WIDTH] = 0.0;
        if s.noise < 0.0 {
            w.push(format!("synth {k}: noise mix {} is negative; its magnitude is used (white noise either way)", s.noise));
        }
    }
    // the linear timed envelope: attack, decay to ½, hold for the sustain frames, release
    let ms = |frames: i32| inv::inv_lin_ms(frames as f32 * 1000.0 / RATE as f32);
    val[p::el_env(0, el::LINEAR)] = 1.0;
    val[p::env(0, e::ATTACK)] = ms(s.seg[0]);
    val[p::env(0, e::DECAY)] = ms(s.seg[1]);
    val[p::el_env(0, el::HOLD)] = ms(s.seg[2]);
    val[p::env(0, e::RELEASE)] = ms(s.seg[3]);
    val[p::env(0, e::SUSTAIN)] = 0.5;
    if s.seg.iter().any(|&x| x as f64 > 5.0 * RATE) {
        w.push(format!("synth {k}: a segment is longer than the envelope's 5 s; clamped"));
    }
    // the pitch drop towards the base frequency
    if s.freq_exp != 1.0 {
        if !(0.0..1.0).contains(&s.freq_exp) {
            w.push(format!("synth {k}: pitch factor {} is not below 1; no drop", s.freq_exp));
        } else {
            val[el::DROP_BASE] = inv::inv_drop_hz((s.base as f64 * RATE) as f32);
            val[el::DROP_RATE] = inv::inv_drop_k(s.freq_exp as f64);
        }
    }
    Ok(unisynth::chunk::encode(&val, &[]))
}

/// The unified effect device of an effect machine (`None` for the synth and the mixer).
pub fn effect(m: &Machine, w: &mut Vec<String>, at: usize) -> Option<Dev> {
    use unisynth::fx::{encode, mode_value};
    use unisynth::kind::{DELAY, DISTORTION, EQ};
    let factor = |a: f32| (a.abs().log10() + 3.0) / 6.0;
    let unit = |x: f32, lo: f32, hi: f32, what: &str, w: &mut Vec<String>| {
        if !(lo..=hi).contains(&x) {
            w.push(format!("machine at byte {at}: {what} {x} is outside {lo}..{hi}; clamped"));
        }
        ((x - lo) / (hi - lo)).clamp(0.0, 1.0)
    };
    let dev = |k: u8, mode: usize, ps: &[f32]| {
        let mut vals = vec![mode_value(k, mode)];
        vals.extend_from_slice(ps);
        Dev { kind: k, chunk: encode(&vals) }
    };
    Some(match *m {
        Machine::Synth(_) | Machine::Mixer { .. } => return None,
        Machine::Delay { len, feedback } | Machine::Allpass { len, feedback } => {
            let mode = if matches!(m, Machine::Delay { .. }) { 3 } else { 4 };
            let l = unit(len as f32, 0.0, 65_536.0, "length", w); // len = round(65 536·p)
            dev(DELAY, mode, &[l, unit(feedback, -1.0, 1.0, "feedback", w)])
        }
        Machine::Filter { cutoff, q, rate1, depth1, rate2, depth2, dry, output } => {
            if !(0..=2).contains(&output) {
                w.push(format!("filter at byte {at}: output {output} is not low, high or band; low"));
            }
            dev(
                EQ,
                3,
                &[
                    unit(cutoff, 0.0, 65_536.0, "cutoff", w),
                    unit(q, 0.0, 2.0, "resonance", w),
                    unit(rate1, 0.0, 0.01, "LFO 1 rate", w),
                    unit(depth1, 0.0, 65_536.0, "LFO 1 depth", w),
                    unit(rate2, 0.0, 0.01, "LFO 2 rate", w),
                    unit(depth2, 0.0, 65_536.0, "LFO 2 depth", w),
                    unit(dry, -1.0, 1.0, "dry", w),
                    inv::inv_choice(output.clamp(0, 2) as usize, 3),
                ],
            )
        }
        Machine::Compressor { threshold, ratio, offset } => dev(
            DISTORTION,
            6,
            &[unit(threshold, 0.0, 4.0, "threshold", w), unit(ratio, 0.0, 4.0, "ratio", w), unit(offset, -2.0, 2.0, "offset", w)],
        ),
        Machine::Sine { a, b } | Machine::Atan { a, b } => {
            let mode = if matches!(m, Machine::Sine { .. }) { 5 } else { 4 };
            if a == 0.0 || !(1e-3..=1e3).contains(&a.abs()) {
                w.push(format!("machine at byte {at}: factor {a} is outside 0.001..1000; clamped"));
            }
            if a < 0.0 {
                w.push(format!("machine at byte {at}: factor {a} is negative; its magnitude is used"));
            }
            dev(DISTORTION, mode, &[factor(a.max(1e-30)).clamp(0.0, 1.0), unit(b, 0.0, 16.0, "gain", w)])
        }
    })
}

/// Converts a song (`asm` = `music.asm`, `header` = `music.h`).
pub fn convert(asm: &str, header: &str, opt: Options) -> Result<Converted, ConvError> {
    let music = parse(asm, header)?;
    let (_, ms) = machines(&music.tree, opt.table)?;
    let mut w = vec![];
    let mut devices: Vec<Dev> = vec![];
    let mut lanes: Vec<Vec<Ev>> = vec![];
    let mut tracks: Vec<Trk> = vec![];
    let mut stack: Vec<usize> = vec![];
    let mut synth = 0usize;
    for (at, m) in &ms {
        match m {
            Machine::Synth(s) => {
                let mut s = *s;
                if opt.solo.is_some_and(|k| k != synth) {
                    s.volume = 0.0;
                }
                devices.push(Dev { kind: unisynth::kind::INSTRUMENT, chunk: instrument(&s, synth, &mut w)? });
                let mut prev = 0u64;
                let lane: Vec<Ev> = notes(&music, synth)?
                    .into_iter()
                    .map(|(f, n, vel)| {
                        let d = (f - prev) as i32;
                        prev = f;
                        (d, n, vel)
                    })
                    .collect();
                lanes.push(lane);
                tracks.push(Trk { volume: 1.0, receives: vec![], devices: vec![devices.len() - 1], lane: lanes.len() - 1, autos: vec![] });
                stack.push(tracks.len() - 1);
                synth += 1;
            }
            Machine::Mixer { top, below } => {
                let (t, b) = (stack.pop().expect("walk checked the depth"), stack.pop().expect("walk checked the depth"));
                lanes.push(vec![]);
                tracks.push(Trk { volume: 1.0, receives: vec![(t, 0, *top), (b, 0, *below)], devices: vec![], lane: lanes.len() - 1, autos: vec![] });
                stack.push(tracks.len() - 1);
            }
            m => {
                let d = effect(m, &mut w, *at).expect("an effect machine");
                devices.push(d);
                let t = *stack.last().expect("walk checked the depth");
                tracks[t].devices.push(devices.len() - 1);
            }
        }
    }
    if let Some(k) = opt.solo.filter(|&k| k >= synth) {
        return Err(input(format!("--solo {k}: the song has {synth} synths")));
    }
    // the master is the last track: the one left on the stack
    let master = stack[0];
    if master != tracks.len() - 1 {
        lanes.push(vec![]);
        tracks.push(Trk { volume: 1.0, receives: vec![(master, 0, 1.0)], devices: vec![], lane: lanes.len() - 1, autos: vec![] });
    }
    let frames = music.rows as f64 * 16.0 * music.step as f64;
    let tempo = (RATE * 60.0 / (4.0 * music.step as f64)).round() as i32;
    w.dedup();
    let s = SongM { tempo, rate: RATE as i32, length: frames / RATE + opt.tail, devices, lanes, tracks };
    Ok(Converted { blob: s.write()?, warnings: w })
}

#[cfg(test)]
mod tests;
