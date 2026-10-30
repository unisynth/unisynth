//! FastTracker II tick replay of an [`Xm`] (change add-cauldron-xm-import, task 5.2): the order list at its speed and
//! BPM, rows tick by tick, and per channel the triggers with their per-tick control (frequency, volume, pan). The
//! effect set is the one the mini-cauldron songs use (spec "XM control replay"); anything else is an error naming
//! the pattern, row and channel. Linear frequency table only (`Xm::linear`); no auto-vibrato.
//!
//! Semantics (FT2): a tick lasts `trunc(rate·2.5/bpm)` frames at the render rate (as openmpt renders: BPM 151 is 730
//! frames at 44.1 kHz, not 730.13); `Fxx` sets the speed (< 0x20) or BPM on tick 0 and the row uses it.
//! Period `7680 − 64·(note − 1 + relative) − finetune/2`, frequency `8363·2^((4608 − period)/768)` Hz. Volume
//! `vol/64 · env/64 · fade/32768 · global/64`; panning `pan + (env − 32)·(128 − |pan − 128|)/32`. Envelopes run
//! one point-tick per tick, hold at the sustain point while the key is down, and jump from the loop end to the loop
//! start; after a key-off the fadeout subtracts `fadeout` from 32768 per tick (a key-off without a volume envelope
//! cuts the note). Slides act on ticks after the first; `E1x`/`E2x`, set volume/panning on tick 0.

use super::{Cell, Env, Xm};

#[cfg(test)]
mod tests;

/// One tick of a trigger: its start time (s) and frame, length in frames, playback frequency (Hz), final volume (0–1)
/// and pan (`pan/256`: 0 left, ½ centre, as openmpt).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TickCtl {
    pub at: f64,
    pub frame: u64,
    pub frames: u32,
    pub freq: f64,
    pub vol: f64,
    pub pan: f64,
}

/// A sample start on a channel and its control until it ends (the channel's next trigger, a cut, a finished fadeout
/// or the song's end).
#[derive(Clone, Debug, PartialEq)]
pub struct Trigger {
    pub channel: usize,
    /// Global tick index, frame at the render rate and time (s) of the start.
    pub tick: usize,
    pub frame: u64,
    pub at: f64,
    /// 0-based instrument and sample within it.
    pub instrument: usize,
    pub sample: usize,
    pub note: u8,
    /// Start position in samples (`9xx`).
    pub offset: u32,
    pub ticks: Vec<TickCtl>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Replay {
    pub triggers: Vec<Trigger>,
    /// Song length (s): to the end of the order list.
    pub length: f64,
}

#[derive(Default)]
struct Ch {
    /// 1-based instrument (0 none) and its sample.
    ins: usize,
    smp: usize,
    note: u8,
    period: f64,
    target: f64,
    vol: i32,
    pan: i32,
    key_on: bool,
    env_pos: [u16; 2],
    fade: i32,
    /// The active trigger (index into the output).
    cur: Option<usize>,
    // effect memories
    porta_up: u8,
    porta_down: u8,
    fine_up: u8,
    fine_down: u8,
    tone: u8,
    offset: u8,
    slide: u8,
    vib: (u8, u8),
    vib_pos: u8,
}

/// FT2's limits for a slid period.
const PERIOD_MIN: f64 = 1.0;
const PERIOD_MAX: f64 = 31999.0;

fn period(note: u8, relative: i8, finetune: i8) -> f64 {
    7680.0 - 64.0 * (note as f64 - 1.0 + relative as f64) - finetune as f64 / 2.0
}

fn freq(period: f64) -> f64 {
    8363.0 * 2f64.powf((4608.0 - period) / 768.0)
}

/// FT2's vibrato table (half a sine, 0–255).
const VIB: [u8; 32] = [0, 24, 49, 74, 97, 120, 141, 161, 180, 197, 212, 224, 235, 244, 250, 253, 255, 253, 250, 244, 235, 224, 212, 197, 180, 161, 141, 120, 97, 74, 49, 24];

fn env_value(e: &Env, pos: u16) -> f64 {
    let p = &e.points;
    if p.is_empty() {
        return 64.0;
    }
    for w in p.windows(2) {
        let ((t0, v0), (t1, v1)) = (w[0], w[1]);
        if pos >= t0 && pos < t1 {
            return v0 as f64 + (v1 as f64 - v0 as f64) * (pos - t0) as f64 / (t1 - t0) as f64;
        }
    }
    if pos < p[0].0 { p[0].1 as f64 } else { p[p.len() - 1].1 as f64 }
}

fn env_advance(e: &Env, pos: &mut u16, key_on: bool) {
    let p = &e.points;
    if p.is_empty() {
        return;
    }
    let at = |i: u8| p.get(i as usize).map(|x| x.0);
    if e.flags & 2 != 0 && key_on && at(e.sustain) == Some(*pos) {
        return;
    }
    *pos = pos.saturating_add(1);
    if e.flags & 4 != 0 {
        if let (Some(end), Some(start)) = (at(e.loop_end), at(e.loop_start)) {
            if *pos >= end {
                *pos = start;
            }
        }
    }
}

/// Supported: effects 0 (arpeggio), 1–4, 8, 9, A, C, D, F, G and E1, E2, E9, EC, ED, EE; volume column set volume
/// (0x10–0x50) and set panning (0xC0–0xCF).
fn supported(c: &Cell) -> Result<(), String> {
    let ok = match c.effect {
        0..=4 | 8 | 9 | 0xA | 0xC | 0xD | 0xF | 0x10 => true,
        0xE => matches!(c.param >> 4, 1 | 2 | 9 | 0xC | 0xD | 0xE),
        _ => false,
    };
    if !ok {
        return Err(if c.effect == 0xE { format!("effect E{:X}", c.param >> 4) } else { format!("effect {:X}", c.effect) });
    }
    if !(c.volume == 0 || (0x10..=0x50).contains(&c.volume) || (0xC0..=0xCF).contains(&c.volume)) {
        return Err(format!("volume column {:#04x}", c.volume));
    }
    Ok(())
}

pub fn replay(x: &Xm, rate: u32) -> Result<Replay, String> {
    if !x.linear {
        return Err("Amiga frequency table".into());
    }
    for (i, ins) in x.instruments.iter().enumerate() {
        if ins.vibrato[2] != 0 {
            return Err(format!("instrument {}: auto-vibrato", i + 1));
        }
    }
    let n = x.channels as usize;
    let mut chs: Vec<Ch> = (0..n).map(|_| Ch { fade: 32768, ..Ch::default() }).collect();
    let mut out: Vec<Trigger> = vec![];
    let (mut speed, mut bpm, mut global) = (x.speed as u32, x.bpm as u32, 64i32);
    let (mut frame, mut tick) = (0u64, 0usize);
    let (mut o, mut row) = (0usize, 0usize);
    while o < x.order.len() {
        let pi = x.order[o] as usize;
        let pat = x.patterns.get(pi).ok_or_else(|| format!("order {o}: pattern {pi} does not exist"))?;
        if row >= pat.rows as usize {
            row = 0;
        }
        let cells = &pat.cells[row * n..(row + 1) * n];
        let mut brk = None;
        let mut delay = 0;
        for (c, cell) in cells.iter().enumerate() {
            supported(cell).map_err(|e| format!("{e} at pattern {pi}, row {row}, channel {c}"))?;
            match (cell.effect, cell.param >> 4) {
                (0xF, _) if cell.param == 0 => return Err(format!("F00 (stop) at pattern {pi}, row {row}, channel {c}")),
                (0xF, _) if cell.param < 0x20 => speed = cell.param as u32,
                (0xF, _) => bpm = cell.param as u32,
                (0xD, _) => brk = Some((cell.param >> 4) as usize * 10 + (cell.param & 15) as usize),
                (0xE, 0xE) => delay = (cell.param & 15) as u32,
                _ => {}
            }
        }
        for rep in 0..=delay {
            for k in 0..speed {
                let first = rep == 0 && k == 0;
                let t = frame as f64 / rate as f64;
                let frames = (rate * 5 / 2) / bpm;
                for (c, cell) in cells.iter().enumerate() {
                    let ch = &mut chs[c];
                    let (e, p, sub) = (cell.effect, cell.param, cell.param >> 4);
                    let at_tick = |x: u32| rep == 0 && k == x;
                    // the note (tick 0, or the note-delay tick)
                    let note_tick = if e == 0xE && sub == 0xD { (p & 15) as u32 } else { 0 };
                    if at_tick(note_tick) && (cell.note != 0 || cell.instrument != 0) {
                        note_on(x, ch, cell, c, (tick, frame, t), &mut out)?;
                    }
                    if first {
                        match (e, sub) {
                            (0xC, _) => ch.vol = (p as i32).min(64),
                            (8, _) => ch.pan = p as i32,
                            (0xE, 1) => {
                                if p & 15 != 0 {
                                    ch.fine_up = p & 15;
                                }
                                ch.period = (ch.period - 4.0 * ch.fine_up as f64).max(PERIOD_MIN);
                            }
                            (0xE, 2) => {
                                if p & 15 != 0 {
                                    ch.fine_down = p & 15;
                                }
                                ch.period = (ch.period + 4.0 * ch.fine_down as f64).min(PERIOD_MAX);
                            }
                            (0x10, _) => global = (p as i32).min(64),
                            _ => {}
                        }
                        match cell.volume {
                            v @ 0x10..=0x50 => ch.vol = v as i32 - 0x10,
                            v @ 0xC0..=0xCF => ch.pan = (v as i32 & 15) * 16,
                            _ => {}
                        }
                    } else {
                        match (e, sub) {
                            (1, _) => {
                                if k == 1 && p != 0 {
                                    ch.porta_up = p;
                                }
                                ch.period = (ch.period - 4.0 * ch.porta_up as f64).max(PERIOD_MIN);
                            }
                            (2, _) => {
                                if k == 1 && p != 0 {
                                    ch.porta_down = p;
                                }
                                ch.period = (ch.period + 4.0 * ch.porta_down as f64).min(PERIOD_MAX);
                            }
                            (3, _) => {
                                if p != 0 {
                                    ch.tone = p;
                                }
                                let s = 4.0 * ch.tone as f64;
                                ch.period = if ch.period < ch.target { (ch.period + s).min(ch.target) } else { (ch.period - s).max(ch.target) };
                            }
                            (4, _) => {
                                if p >> 4 != 0 {
                                    ch.vib.0 = p >> 4;
                                }
                                if p & 15 != 0 {
                                    ch.vib.1 = p & 15;
                                }
                            }
                            (0xA, _) => {
                                if p != 0 {
                                    ch.slide = p;
                                }
                                let s = ch.slide;
                                ch.vol = if s >> 4 != 0 { (ch.vol + (s >> 4) as i32).min(64) } else { (ch.vol - (s & 15) as i32).max(0) };
                            }
                            _ => {}
                        }
                    }
                    if e == 3 && first {
                        if p != 0 {
                            ch.tone = p;
                        }
                    }
                    // retrigger (E9x: every x ticks after the first) and cut (ECx at tick x)
                    if e == 0xE && sub == 9 && p & 15 != 0 && !first && (rep * speed + k) % (p & 15) as u32 == 0 {
                        if let Some(i) = ch.cur {
                            let tr = Trigger { tick, frame, at: t, offset: 0, ticks: vec![], ..out[i].clone() };
                            ch.cur = Some(out.len());
                            out.push(tr);
                            ch.env_pos = [0, 0];
                            ch.fade = 32768;
                            ch.key_on = true;
                        }
                    }
                    if e == 0xE && sub == 0xC && at_tick((p & 15) as u32) {
                        ch.vol = 0;
                    }
                    // the tick's control
                    if let Some(i) = ch.cur {
                        let ins = &x.instruments[ch.ins - 1];
                        let mut per = ch.period;
                        if e == 0 && p != 0 {
                            per -= 64.0 * [0, p >> 4, p & 15][(k % 3) as usize] as f64;
                        }
                        if e == 4 && !first {
                            let v = VIB[(ch.vib_pos >> 2) as usize & 31] as f64 * ch.vib.1 as f64 / 32.0;
                            per += if ch.vib_pos & 0x80 == 0 { -v } else { v } * 4.0;
                            ch.vib_pos = ch.vib_pos.wrapping_add(ch.vib.0 * 4);
                        }
                        let env = |e: &Env, pos: u16, off: f64| if e.flags & 1 != 0 { env_value(e, pos) } else { off };
                        let ev = env(&ins.vol_env, ch.env_pos[0], 64.0);
                        let ep = env(&ins.pan_env, ch.env_pos[1], 32.0);
                        let vol = ch.vol as f64 / 64.0 * ev / 64.0 * ch.fade as f64 / 32768.0 * global as f64 / 64.0;
                        let pan = ch.pan as f64 + (ep - 32.0) * (128.0 - (ch.pan as f64 - 128.0).abs()) / 32.0;
                        out[i].ticks.push(TickCtl { at: t, frame, frames, freq: freq(per), vol, pan: (pan / 256.0).clamp(0.0, 1.0) });
                        let done = (vol == 0.0 && !ch.key_on && ch.fade == 0) || (e == 0xE && sub == 0xC && ch.vol == 0);
                        env_advance(&ins.vol_env, &mut ch.env_pos[0], ch.key_on);
                        env_advance(&ins.pan_env, &mut ch.env_pos[1], ch.key_on);
                        if !ch.key_on {
                            ch.fade = (ch.fade - ins.fadeout as i32).max(0);
                        }
                        if done {
                            ch.cur = None;
                        }
                    }
                }
                frame += frames as u64;
                tick += 1;
            }
        }
        match brk {
            Some(r) => {
                o += 1;
                row = r;
            }
            None => {
                row += 1;
                if row >= pat.rows as usize {
                    o += 1;
                    row = 0;
                }
            }
        }
    }
    Ok(Replay { triggers: out, length: frame as f64 / rate as f64 })
}

/// Tick 0 (or the delay tick) of a cell with a note or instrument: key-off, a new trigger, or a tone-portamento
/// target; an instrument number resets the volume, panning and envelopes.
fn note_on(x: &Xm, ch: &mut Ch, cell: &Cell, c: usize, (tick, frame, t): (usize, u64, f64), out: &mut Vec<Trigger>) -> Result<(), String> {
    if cell.instrument != 0 {
        if cell.instrument as usize > x.instruments.len() {
            return Err(format!("instrument {} does not exist (channel {c})", cell.instrument));
        }
        // FT2: a tone portamento keeps the sounding note's instrument; its number only resets the volume and restarts
        // that instrument's envelopes (below)
        if !(cell.effect == 3 && ch.cur.is_some()) {
            ch.ins = cell.instrument as usize;
        }
    }
    if cell.note == 97 {
        ch.key_on = false;
        if ch.ins != 0 && x.instruments[ch.ins - 1].vol_env.flags & 1 == 0 {
            ch.cur = None; // FT2: a key-off without a volume envelope cuts the note
        }
        return Ok(());
    }
    if ch.ins == 0 {
        return Ok(());
    }
    let ins = &x.instruments[ch.ins - 1];
    if cell.note != 0 {
        let s = ins.keymap[cell.note as usize - 1] as usize;
        if s >= ins.samples.len() {
            ch.cur = None; // a note mapped to no sample is silent
            return Ok(());
        }
        let smp = &ins.samples[s];
        let p = period(cell.note, smp.relative, smp.finetune);
        if cell.effect == 3 && ch.cur.is_some() {
            ch.target = p;
        } else {
            ch.smp = s;
            ch.note = cell.note;
            ch.period = p;
            ch.target = p;
            if cell.effect == 9 && cell.param != 0 {
                ch.offset = cell.param;
            }
            let offset = if cell.effect == 9 { ch.offset as u32 * 256 } else { 0 };
            ch.cur = Some(out.len());
            out.push(Trigger { channel: c, tick, frame, at: t, instrument: ch.ins - 1, sample: s, note: cell.note, offset, ticks: vec![] });
            ch.vib_pos = 0;
            ch.env_pos = [0, 0];
            ch.fade = 32768;
            ch.key_on = true;
        }
    }
    if cell.instrument != 0 {
        let smp = &ins.samples.get(ch.smp).or(ins.samples.first());
        if let Some(smp) = smp {
            ch.vol = smp.volume as i32;
            ch.pan = smp.pan as i32;
        }
        ch.env_pos = [0, 0];
        ch.fade = 32768;
        ch.key_on = true;
    }
    Ok(())
}
