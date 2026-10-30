//! The XM side of the mini-cauldron converter (change add-cauldron-xm-import, tasks 5.3–): which XM samples are
//! generated phrases, and how a trigger of one becomes notes (design D1, D9).
//!
//! A trigger's playback position moves through the stored sample at its per-tick frequency, so the phrase position
//! (internal frames = stored samples × decimation) follows the tracker's resampling exactly: `path` integrates it
//! tick by tick, with the sample offset, loops and the sample's end. `unroll` then walks the phrase's events
//! (`phrase_events`) along that path. A jump (offset, forward loop, a ping-pong turn) restarts at the step under the
//! new position; a ping-pong backward pass plays the loop region's steps in reverse order, each forwards.

use super::*;
use crate::xm::replay::TickCtl;
use crate::xm::Xm;
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

/// Which (record, variant) every XM sample is (`None`: a stored sample), by instrument and sample. Jizz names its
/// phrases `Probe<record+1><variant+1>`; Stash stores them in render order (slot by slot, each record's variants),
/// the drum slot taking `drum_count` samples, empty samples skipped.
pub fn identify(call: &Call, xm: &Xm) -> Result<Vec<Vec<Option<(usize, usize)>>>, String> {
    let mut queue = vec![];
    if call.version == Version::V2 {
        for s in &call.order {
            match *s {
                Slot::Drums => queue.extend((0..call.drum_count).map(|_| None)),
                Slot::Render { record, .. } => queue.extend((0..call.records[record].variants as usize).map(|v| Some((record, v)))),
            }
        }
    }
    let mut q = queue.into_iter();
    let mut out = vec![];
    for ins in &xm.instruments {
        let mut row = vec![];
        for s in &ins.samples {
            row.push(match call.version {
                Version::V1 => {
                    let name = s.name.trim().trim_end_matches(".smp");
                    match name.strip_prefix("Probe") {
                        Some(d) if d.len() >= 2 && d.bytes().all(|b| b.is_ascii_digit()) => {
                            let (r, v) = (d[..d.len() - 1].parse::<usize>().unwrap(), d[d.len() - 1..].parse::<usize>().unwrap());
                            let ok = r >= 1 && v >= 1 && r <= call.records.len() && v <= call.records[r - 1].variants as usize;
                            if !ok {
                                return Err(format!("sample `{}`: no record {r}, variant {v} in the patch data", s.name.trim()));
                            }
                            Some((r - 1, v - 1))
                        }
                        _ => None,
                    }
                }
                Version::V2 if s.data.is_empty() => None,
                Version::V2 => q.next().flatten(),
            });
        }
        out.push(row);
    }
    Ok(out)
}

/// The stored sample a trigger plays: its length and loop in stored samples, loop kind 0 none, 1 forward, 2 ping-pong.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loop {
    pub len: u32,
    pub start: u32,
    pub length: u32,
    pub kind: u8,
}

/// A stretch of output time `t0..t1` (frames) over which the phrase position runs `x0..x1` (phrase frames, rising)
/// at a constant rate. `cont`: it continues the previous piece without a jump.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub t0: f64,
    pub t1: f64,
    pub x0: f64,
    pub x1: f64,
    pub cont: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub pieces: Vec<Piece>,
    /// Output frame where the trigger stops sounding (its last tick's end, or the sample's end).
    pub end: f64,
    /// Output frames spent in ping-pong backward passes.
    pub backward: f64,
}

/// The phrase position of a trigger over its ticks (output frames at `rate`, from the first tick's frame), from the
/// stored-sample `offset`, through `s`'s loop.
pub fn path(ticks: &[TickCtl], offset: u32, s: &Loop, decim: u32, rate: f64) -> Path {
    let d = decim as f64;
    let looped = s.kind != 0 && s.length > 0;
    let (ls, le, len) = (s.start as f64, (s.start + s.length).min(s.len) as f64, s.len as f64);
    // raw segments (t0, t1, x0, x1) in stored samples, direction from the sign
    let mut seg: Vec<(f64, f64, f64, f64)> = vec![];
    let (mut pos, mut back) = (offset as f64, false);
    let mut end = ticks.first().map_or(0.0, |t| t.frame as f64);
    // an offset past the end plays nothing
    let ticks = if pos >= len { &ticks[..0] } else { ticks };
    'ticks: for tk in ticks {
        let v = tk.freq / rate;
        let mut t = tk.frame as f64;
        let mut left = tk.frames as f64;
        end = t + left;
        if v <= 0.0 {
            continue;
        }
        while left > 0.0 {
            let bound = if back { ls } else if looped { le } else { len };
            let dt = (bound - pos).abs() / v;
            let p0 = pos;
            if dt > left {
                pos += if back { -v * left } else { v * left };
                seg.push((t, t + left, p0, pos));
                break;
            }
            pos = bound;
            if dt > 0.0 {
                seg.push((t, t + dt, p0, pos));
            }
            t += dt;
            left -= dt;
            match (looped, s.kind, back) {
                (false, ..) => {
                    end = t;
                    break 'ticks;
                }
                (true, 2, b) => back = !b,
                (true, ..) => pos = ls,
            }
        }
    }
    // pieces: forward segments as they are; a backward run as its steps in reverse order, each forwards over the
    // time the run spent in it
    let step = STEP as f64;
    let mut pieces: Vec<(Piece, bool)> = vec![]; // (piece, from a backward run)
    let mut backward = 0.0;
    let mut i = 0;
    while i < seg.len() {
        let (t0, t1, x0, x1) = seg[i];
        if x1 >= x0 {
            let cont = pieces.last().is_some_and(|(p, b)| !b && (p.x1 - x0 * d).abs() < 1e-6 && (p.t1 - t0).abs() < 1e-6);
            pieces.push((Piece { t0, t1, x0: x0 * d, x1: x1 * d, cont }, false));
            i += 1;
            continue;
        }
        let mut j = i;
        while j < seg.len() && seg[j].3 < seg[j].2 {
            j += 1;
        }
        let run = &seg[i..j];
        backward += run[run.len() - 1].1 - run[0].0;
        // the time the run passes phrase position x (falling)
        let when = |x: f64| {
            for &(t0, t1, a, b) in run {
                let (a, b) = (a * d, b * d);
                if x <= a && x >= b {
                    return t0 + (a - x) / (a - b) * (t1 - t0);
                }
            }
            if x > run[0].2 * d { run[0].0 } else { run[run.len() - 1].1 }
        };
        let (hi, lo) = (run[0].2 * d, run[run.len() - 1].3 * d);
        let mut s = ((hi - 1e-9) / step).floor();
        while s >= (lo / step).floor() && s >= 0.0 {
            let (top, bot) = (hi.min((s + 1.0) * step), lo.max(s * step));
            if top - bot > 1e-9 {
                pieces.push((Piece { t0: when(top), t1: when(bot), x0: bot, x1: top, cont: false }, true));
            }
            s -= 1.0;
        }
        i = j;
    }
    Path { pieces: pieces.into_iter().map(|p| p.0).collect(), end, backward }
}

/// The notes of `variant` of `call`'s record `record` along `p` (output frames), at `ratio` (the track's tuning).
pub fn unroll(call: &Call, record: usize, variant: usize, ratio: f64, p: &Path) -> Vec<PhraseEv> {
    let r = &call.records[record];
    let tl = phrase_events(call, record, variant, ratio);
    let tuning = Tuning::of(&r.pitch, ratio);
    let step = STEP as f64;
    let tail = if call.version == Version::V1 { step } else { r.release as f64 };
    let mut out = vec![];
    let (mut held, mut glide) = (false, false);
    let emit = |out: &mut Vec<PhraseEv>, held: &mut bool, glide: &mut bool, at: f64, what: PEv| {
        match what {
            PEv::On { .. } | PEv::Legato { .. } => *held = true,
            PEv::Off => *held = false,
            PEv::Glide { on } => *glide = on,
            PEv::Tune { .. } => {}
        }
        out.push(PhraseEv { at, what });
    };
    for pc in &p.pieces {
        let map = |x: f64| pc.t0 + (x - pc.x0) / (pc.x1 - pc.x0) * (pc.t1 - pc.t0);
        if !pc.cont {
            // a jump: end what sounds, then enter at the step under the position
            if glide {
                emit(&mut out, &mut held, &mut glide, pc.t0, PEv::Glide { on: false });
            }
            if held {
                emit(&mut out, &mut held, &mut glide, pc.t0, PEv::Off);
            }
            let x = pc.x0;
            let s = ((x / step) as usize).min(15);
            let last_on = tl.iter().rposition(|e| e.at <= x && matches!(e.what, PEv::On { .. }));
            if let Some(k) = last_on {
                let off = tl[k..].iter().find(|e| e.what == PEv::Off).map_or(f64::MAX, |e| e.at);
                if x < off + tail {
                    let h = (0..=s).rev().find(|&h| r.retrig(variant, h).is_some()).unwrap_or(0);
                    let rt = r.retrig(variant, h).expect("step 0 retriggers");
                    let (note, coarse, fine, word) = tuning.note(r.pitch[s]);
                    let phase = if r.second_saw() { phase_byte(phase2_at(call, record, variant, s)) } else { 0 };
                    let on = PEv::On { note, coarse, fine, word, retrig: note_byte(&rt), phase, vel: velocity(x), age: age_byte(x - (h as u32 * STEP) as f64) };
                    emit(&mut out, &mut held, &mut glide, pc.t0, on);
                    if x >= off {
                        // entering a fade: the release plays from here
                        let at = map((x + ATTACK).min(pc.x1)).max(pc.t0);
                        emit(&mut out, &mut held, &mut glide, at, PEv::Off);
                    }
                }
            }
            let g = tl.iter().filter(|e| e.at <= x).fold(false, |g, e| if let PEv::Glide { on } = e.what { on } else { g });
            if g {
                emit(&mut out, &mut held, &mut glide, pc.t0, PEv::Glide { on: true });
            }
        }
        for e in &tl {
            let inside = if pc.cont { e.at >= pc.x0 } else { e.at > pc.x0 };
            if !inside || e.at >= pc.x1 {
                continue;
            }
            // a jump's entry may have released already: no off, slide or retune without a note
            if !held && matches!(e.what, PEv::Off | PEv::Legato { .. } | PEv::Tune { .. }) {
                continue;
            }
            emit(&mut out, &mut held, &mut glide, map(e.at), e.what);
        }
    }
    if glide {
        emit(&mut out, &mut held, &mut glide, p.end, PEv::Glide { on: false });
    }
    if held {
        emit(&mut out, &mut held, &mut glide, p.end, PEv::Off);
    }
    out
}

/// One track of generated phrases (design D2): a record at a playback-ratio class (semitones from native), and the
/// replay's triggers it plays (indices, in time order).
#[derive(Clone, Debug, PartialEq)]
pub struct TrackPlan {
    pub record: usize,
    pub class: i32,
    pub triggers: Vec<usize>,
}

impl TrackPlan {
    /// The ratio the track's instrument, tuning and effects are built for.
    pub fn ratio(&self) -> f64 {
        2f64.powf(self.class as f64 / 12.0)
    }
}

/// The highest class (semitones of playback ratio) a track is built for: its phrase clock is the original's rate
/// times the ratio (and the bend's share on top), so a class costs CPU in proportion to it.
pub const MAX_CLASS: i32 = 24;

/// The lowest class: a track's comb and doubler delays (frames) grow as 1/ratio, and the song's lead covers the
/// longest chain, so a slide falling far below hearing (song 1's record 1, to 5 Hz) plays from here and is silent
/// past its reach (`MIN_CLASS − MAX_SHIFT`, a ratio of 1/256; lower bought little: what the original has there is
/// its echo, baked into the sample and slowed with it, which a real-time delay cannot follow).
pub const MIN_CLASS: i32 = -48;

/// The class of a trigger: the middle of the playback ratios (frequency over the phrase's native one) of its ticks
/// before `end` (output frame) that sound (volume and frequency above 0: a held ping-pong ends on silent ticks at
/// 0 Hz), in semitones, raised so the top stays within `MAX_SHIFT` (a slide past reach leaves its deep end, which
/// plays far below hearing, rather than the top), at most `MAX_CLASS`; a trigger without a slide is its start's. The
/// track's transpose and bend (`MAX_SHIFT` each way) carry the rest of a slide, and the cauldron slot's phrase clock
/// follows them (an XM slide resamples the whole sample), so a riser plays in tune from any class within reach.
/// A tick is heard: volume and frequency above 0.
fn sounds(k: &TickCtl) -> bool {
    k.vol > 0.0 && k.freq > 1.0
}

fn class_of(t: &crate::xm::replay::Trigger, decim: u32, end: f64) -> i32 {
    let semis = |f: f64| 12.0 * (f * decim as f64 / R).log2();
    let (lo, hi) = t.ticks.iter().take_while(|k| (k.frame as f64) < end).filter(|k| sounds(k)).fold((f64::MAX, f64::MIN), |(a, b), k| (a.min(semis(k.freq)), b.max(semis(k.freq))));
    let class = if lo <= hi { ((lo + hi) / 2.0).round().max((hi - MAX_SHIFT).ceil()) } else { t.ticks.first().map_or(0.0, |k| semis(k.freq)).round() };
    (class as i32).clamp(MIN_CLASS, MAX_CLASS)
}

/// The widest slide (semitones) one segment of a trigger may span: a track reaches `±MAX_SHIFT` from its class, less a
/// semitone each side for the class's rounding.
const SEGMENT_SPAN: f64 = 2.0 * MAX_SHIFT - 2.0;

/// Where a phrase trigger's slide leaves what one track reaches (its sounding ticks before `end` span more than
/// `SEGMENT_SPAN` semitones, song 1's record 1 riser some 130): the tick indices where a new segment starts, each
/// segment then grouped and classed as a trigger of its own (`convert`), entering the phrase where the last left off
/// (`clip`). Greedy: a segment runs until its next sounding tick would widen it past the span. Empty when the
/// trigger fits.
pub fn handovers(t: &crate::xm::replay::Trigger, decim: u32, end: f64) -> Vec<usize> {
    let semis = |f: f64| 12.0 * (f * decim as f64 / R).log2();
    let mut cuts = vec![];
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for (i, k) in t.ticks.iter().enumerate().take_while(|(_, k)| (k.frame as f64) < end) {
        // what lies past the lowest class's reach stays silent: no handover for it
        if !sounds(k) || semis(k.freq) < MIN_CLASS as f64 - MAX_SHIFT {
            continue;
        }
        let s = semis(k.freq);
        if lo <= hi && (hi.max(s) - lo.min(s)) > SEGMENT_SPAN {
            cuts.push(i);
            (lo, hi) = (s, s);
        } else {
            (lo, hi) = (lo.min(s), hi.max(s));
        }
    }
    cuts
}

/// The part of path `p` in output frames `a..b`: its pieces cut to that time (positions unchanged), the first an
/// entry (`cont` false: a note at the step under it, the envelope aged), the end at `b` or the path's. A segment of a
/// handed-over slide (`handovers`); the ping-pong seconds stay with the segment that starts the path.
pub fn clip(p: &Path, a: f64, b: f64) -> Path {
    let mut pieces: Vec<Piece> = vec![];
    for pc in &p.pieces {
        let (t0, t1) = (pc.t0.max(a), pc.t1.min(b));
        if t1 <= t0 {
            continue;
        }
        let x = |t: f64| if pc.t1 > pc.t0 { pc.x0 + (t - pc.t0) / (pc.t1 - pc.t0) * (pc.x1 - pc.x0) } else { pc.x0 };
        let cont = pc.cont && !pieces.is_empty();
        pieces.push(Piece { t0, t1, x0: x(t0), x1: x(t1), cont });
    }
    let first = p.pieces.first().is_some_and(|q| q.t0 >= a);
    Path { pieces, end: p.end.min(b), backward: if first { p.backward } else { 0.0 } }
}

/// Groups the generated-phrase triggers into tracks by (record, XM channel, class: `class_of`), then
/// merges the groups of one (record, class) whose triggers never overlap in time (spec "Track grouping"). `ids` is
/// `identify`'s map; `decim` gives a record's decimation; `span` a trigger's sounding output frames (its `path`: a
/// trigger runs on in the replay until the channel's next one, though its sample may have ended long before).
pub fn group(r: &crate::xm::replay::Replay, ids: &[Vec<Option<(usize, usize)>>], decim: impl Fn(usize) -> u32, span: impl Fn(usize) -> (f64, f64)) -> Vec<TrackPlan> {
    let mut by: BTreeMap<((usize, i32), usize), Vec<usize>> = BTreeMap::new();
    for (i, t) in r.triggers.iter().enumerate() {
        let Some((record, _)) = ids.get(t.instrument).and_then(|s| s.get(t.sample)).copied().flatten() else { continue };
        let class = class_of(t, decim(record), span(i).1);
        by.entry(((record, class), t.channel)).or_default().push(i);
    }
    merge(r, by, span).into_iter().map(|((record, class), triggers)| TrackPlan { record, class, triggers }).collect()
}

/// Merges per-(key, channel) trigger groups into tracks: a group joins the first track of its key whose triggers
/// never overlap its own (by `span`), else starts one. Tracks keep their triggers in time order.
fn merge<K: Ord + Copy>(r: &crate::xm::replay::Replay, by: BTreeMap<(K, usize), Vec<usize>>, span: impl Fn(usize) -> (f64, f64)) -> Vec<(K, Vec<usize>)> {
    let mut out: Vec<(K, Vec<usize>)> = vec![];
    let overlaps = |a: &[usize], b: &[usize]| {
        a.iter().any(|&i| {
            let (s0, e0) = span(i);
            b.iter().any(|&j| {
                let (s1, e1) = span(j);
                s0 < e1 && s1 < e0
            })
        })
    };
    for ((key, _), trig) in by {
        match out.iter_mut().find(|(k, t)| *k == key && !overlaps(t, &trig)) {
            Some((_, t)) => {
                t.extend(trig);
                t.sort_by_key(|&i| r.triggers[i].frame);
            }
            None => out.push((key, trig)),
        }
    }
    out
}

/// Frames the post-chain gain (EQ 3band) delays a phrase track by.
pub const POST_DELAY: i64 = 3;

/// The post-chain gain device's values: EQ 3band, all bands at 0 (the song's automation sets them).
fn post_gain() -> Vec<f32> {
    vec![unisynth::fx::mode_value(unisynth::kind::EQ, 1), 0.0, 0.0, 0.0]
}

/// The largest pitch bend (semitones) a track may use: the instrument's bend range.
pub const MAX_BEND: f64 = 24.0;

/// The largest pitch shift (semitones from a track's class) transpose and bend carry together.
pub const MAX_SHIFT: f64 = 2.0 * MAX_BEND;

/// A track's pitch shift (semitones from its class, within `±MAX_SHIFT`) as the voice transpose (`±24`) and bend
/// (`±MAX_BEND`): `(transpose, bend)`; both are voice-wide, so the cauldron slot's phrase clock follows them.
/// `lane`: the track automates the transpose, whose byte cannot hold 0 (128 plays +0.094 semitones), so its zero is
/// that and the bend makes up the rest.
pub fn split_shift(shift: f64, lane: bool) -> (f64, f64) {
    use unisynth::law::{host::inv_transpose, transpose};
    let rest = shift;
    let edge = MAX_BEND - 0.1; // leaves the bend room for a lane's zero
    let t = if rest.abs() <= edge { 0.0 } else { rest - rest.signum() * edge };
    // the transpose as its automation byte plays it back, rounded away from zero so the bend's share stays in range
    let t = match (t == 0.0, lane) {
        (true, false) => 0.0,
        (true, true) => transpose(128.0 / 255.0) as f64,
        _ => transpose(if t > 0.0 { (inv_transpose(t as f32) * 255.0).ceil() } else { (inv_transpose(t as f32) * 255.0).floor() } / 255.0) as f64,
    };
    (t, (rest - t).clamp(-MAX_BEND, MAX_BEND))
}

/// A track's XM control as points `(output frame, value)` where the value changes (design D10): gain (the replay's
/// final volume; the track volume holds `XM_GAIN`), pan (the XM's, `pan/256`: openmpt pans an FT2 file with the
/// equal-power law unisynth uses) and pitch shift (semitones from the track's class, played as transpose and bend
/// by `split_shift`). Where a slide leaves `±MAX_SHIFT` (a riser far past hearing) the track is silent there and a
/// warning names the trigger; `muted` counts those seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Controls {
    pub gain: Vec<(u64, f64)>,
    pub pan: Vec<(u64, f64)>,
    pub bend: Vec<(u64, f64)>,
    pub range: f64,
    pub muted: f64,
    /// For a track with an effect chain (phrases): the XM volume and pan go after the chain, as the tracker scales
    /// and pans the finished sample (its echo, comb and doubler in it). `master` is the voice level, centred at √2
    /// (each channel at the original's level: the chain's clamps must see it); `post` the gain after the chain
    /// (`vol`, track volume `XM_GAIN`) and `post_pan` the pan after it (EQ 3band's equal-power pan), in output
    /// frames: a trigger's first point `LANE_LEAD` early, the gain 0 at its end (the original's echo stops with its
    /// sample) unless the next trigger starts by then.
    pub master: Vec<(u64, f64)>,
    pub post: Vec<(i64, f64)>,
    pub post_pan: Vec<(i64, f64)>,
    pub warnings: Vec<String>,
}

///
/// Only the ticks before a trigger's `end` (output frame: its path's end) count: the replay runs a trigger on until
/// its channel's next note, long after its sample ended, and another trigger merged onto the track sounds there.
pub fn controls(r: &crate::xm::replay::Replay, t: &TrackPlan, decim: impl Fn(usize) -> u32, end: impl Fn(usize) -> f64) -> Controls {
    let hz = R / decim(t.record) as f64 * t.ratio();
    controls_of(r, &t.triggers, &format!("record {}", t.record), |_| hz, end)
}

/// `controls` for any triggers, the shift measured from `reference(trigger)` Hz; `label` names the track in warnings.
fn controls_of(r: &crate::xm::replay::Replay, triggers: &[usize], label: &str, reference: impl Fn(usize) -> f64, end: impl Fn(usize) -> f64) -> Controls {
    let (lo, hi) = (-MAX_SHIFT, MAX_SHIFT);
    let mut c = Controls { gain: vec![], pan: vec![], bend: vec![], range: 0.0, muted: 0.0, master: vec![], post: vec![], post_pan: vec![], warnings: vec![] };
    let push = |v: &mut Vec<(u64, f64)>, f: u64, x: f64| {
        if v.last().is_none_or(|&(_, y)| y != x) {
            v.push((f, x));
        }
    };
    for (n, &i) in triggers.iter().enumerate() {
        let tr = &r.triggers[i];
        let (f0, e) = (reference(i), end(i));
        let mut first = true;
        let (mut worst, mut muted): (f64, u64) = (0.0, 0);
        for k in tr.ticks.iter().take_while(|k| (k.frame as f64) < e) {
            // a silent tick (volume 0, or the 0 Hz a held ping-pong ends on) keeps the last shift
            let b = if sounds(k) { 12.0 * (k.freq / f0).log2() } else { c.bend.last().map_or(0.0, |x| x.1) };
            worst = worst.max(b.abs());
            let out = !(lo..=hi).contains(&b);
            if out {
                muted += k.frames as u64;
            }
            // openmpt pans an FT2 file equal-power, (√(1 − p), √p), as unisynth does: the pan passes through (the
            // voice's for a stored sample, the post device's after a phrase's chain)
            let p = k.pan;
            let vol = if out { 0.0 } else { k.vol };
            push(&mut c.gain, k.frame, vol);
            push(&mut c.master, k.frame, std::f64::consts::SQRT_2);
            let at = k.frame as i64 - if first { crate::cauldron::LANE_LEAD } else { 0 };
            if first || c.post.last().is_none_or(|&(_, y)| y != vol) {
                c.post.push((at, vol));
            }
            if first || c.post_pan.last().is_none_or(|&(_, y)| y != p) {
                c.post_pan.push((at, p));
            }
            first = false;
            push(&mut c.pan, k.frame, p);
            push(&mut c.bend, k.frame, b.clamp(lo, hi));
        }
        let next = triggers.get(n + 1).map_or(f64::MAX, |&j| r.triggers[j].frame as f64 - crate::cauldron::LANE_LEAD as f64);
        if e.is_finite() && e < next && !first {
            c.post.push((e.round() as i64, 0.0));
        }
        if muted > 0 {
            // seconds per frame, from the ticks' own times (a report only)
            let spf = tr.ticks.iter().find(|k| k.frame > 0).map_or(1.0 / 44100.0, |k| k.at / k.frame as f64);
            let s = muted as f64 * spf;
            c.muted += s;
            c.warnings.push(format!("{label} on channel {} at {:.2} s slides {worst:.0} semitones, past {lo:+.0}..{hi:+.0}: silent there for {s:.2} s", tr.channel, tr.at));
        }
        c.range = c.range.max(worst.min(hi.max(-lo)));
    }
    c
}

/// A stored XM sample as a unified sampler instrument (design D8): its PCM as the chunk's GSM sample 1, played one
/// sample per output frame at unisynth note 60 (the sampler's law), so an XM note `n` is unisynth note `note(n)`
/// with the sample's relative note, finetune and `8363/rate` folded into the slot's octave and fine. Its loop as
/// manual bounds; gain and pan are the song's automation (velocity sensitivity 0, voice master √2 for the centred
/// pan), gate envelope (instant sustain, a short release: an XM note sounds until cut).
#[derive(Clone, Debug)]
pub struct Stored {
    pub values: [f32; unisynth::params::COUNT],
    pub gsm: Vec<u8>,
    /// Unisynth note = XM note + `shift`.
    pub shift: i32,
    /// The GSM sample holds the PCM upsampled by this.
    pub up: usize,
}

impl Stored {
    pub fn note(&self, xm_note: u8) -> u8 {
        (xm_note as i32 + self.shift).clamp(0, 127) as u8
    }
}

pub fn sampler(s: &crate::xm::Sample, rate: u32) -> Stored {
    use crate::patch::Patch;
    use unisynth::law::host as inv;
    use unisynth::params::{o, osc_type, shape as sp, v};
    let mut p = Patch::new();
    p.osc_type(0, osc_type::SAMPLER);
    p.sampler_source(0, false, 1.0);
    // XM note n plays 8363·2^((n − 49 + rel + ft/128)/12) Hz: semitones n + c over unisynth's note 60 at one sample
    // per frame, c = 11 + rel + ft/128 + 12·log2(8363/rate); the octave keeps notes 1–96 inside 0–127
    // GSM keeps a low-rate sample's waveform poorly (correlation 0.4 on Jizz's vocals at their own rate): encode it
    // upsampled by the smallest factor up to 4 that keeps it (≥ 0.95), at the level the codec loses made up
    let (up, chunk, level) = gsm_upsampled(&s.data);
    let c = 11.0 + s.relative as f64 + s.finetune as f64 / 128.0 + 12.0 * (8363.0 * up as f64 / rate as f64).log2();
    let ci = c.round();
    let k = ((ci + 1.0) / 12.0).floor().clamp(-4.0, 4.0);
    p.osc(0, o::OCTAVE, inv::inv_octave(k as f32));
    p.osc(0, o::FINE, inv::inv_fine((c - ci) as f32));
    let len = (s.data.len() * up).max(1) as f64;
    let frac = |x: u32| ((x as f64 * up as f64 + 0.5) / len) as f32;
    let mode = match (s.loop_kind, s.loop_len) {
        (_, 0) => 0,
        (1, _) => 1,
        (2, _) => 2,
        _ => 0,
    };
    p.shape(0, sp::SMP_LOOP_MODE, inv::inv_choice(mode, 3));
    p.shape(0, sp::SMP_BOUNDARY, inv::inv_choice(1, 2));
    p.shape(0, sp::SMP_LOOP_START, frac(s.loop_start));
    p.shape(0, sp::SMP_LOOP_LENGTH, frac(s.loop_len));
    p.shape(0, sp::SMP_INTERP, 1.0);
    p.set(v::VELOCITY, 0.0);
    p.set(v::MASTER, inv::inv_gain(std::f32::consts::SQRT_2));
    p.set(v::CAPACITY, inv::inv_count(1, 1, 64));
    p.env(0, 0.0, 0.0, 1.0, inv::inv_env_ms(1.0));
    p.osc(0, o::GAIN, inv::inv_gain(level as f32));
    Stored { values: p.finish(), gsm: chunk, shift: (ci - 12.0 * k) as i32, up }
}

/// `pcm` encoded as a GSM chunk at the smallest upsampling (1–4, windowed sinc) whose decode correlates ≥ 0.95
/// with it: (factor, chunk, the gain that restores its level).
fn gsm_upsampled(pcm: &[i16]) -> (usize, Vec<u8>, f64) {
    let x: Vec<f64> = pcm.iter().map(|&v| v as f64 / 32768.0).collect();
    let mut best = None;
    for up in 1..=4usize {
        let n = x.len() * up;
        let u: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f64 / up as f64;
                let k0 = t.floor() as i64;
                let mut acc = 0.0;
                for k in (k0 - 16).max(0)..=(k0 + 16).min(x.len() as i64 - 1) {
                    let d = t - k as f64;
                    let sinc = if d == 0.0 { 1.0 } else { (std::f64::consts::PI * d).sin() / (std::f64::consts::PI * d) };
                    acc += x[k as usize] * sinc * (0.5 + 0.5 * (std::f64::consts::PI * d / 17.0).cos());
                }
                acc as f32
            })
            .collect();
        let mut blocks = vec![0u8; gsm::blocks_for(n) * gsm::BLOCK_BYTES];
        let m = gsm::encode(&u, &mut blocks);
        blocks.truncate(m);
        let mut dec = vec![0.0f32; n];
        gsm::decode(&blocks, &mut dec).unwrap();
        let (mut xy, mut xx, mut yy) = (0.0, 0.0, 0.0);
        for (a, b) in dec.iter().zip(&u) {
            let (a, b) = (*a as f64, *b as f64);
            xy += a * b;
            xx += a * a;
            yy += b * b;
        }
        let corr = xy / (xx * yy).sqrt().max(1e-12);
        let mut head = [0u8; gsm::CHUNK_HEADER];
        gsm::write_chunk_header(m, n, &mut head);
        let mut chunk = head.to_vec();
        chunk.extend_from_slice(&blocks);
        let level = if xx > 0.0 { (yy / xx).sqrt() } else { 1.0 };
        best = Some((up, chunk, level));
        if corr >= 0.95 {
            break;
        }
    }
    best.unwrap()
}

/// openmpt123's level for a hard-panned full-volume XM channel of these songs (task 6.2, measured on a one-note XM
/// made from song 1: a centred sample plays at 0.267 of its RMS on each side): openmpt plays a file it detects as
/// "FastTracker 2 or compatible" (all three songs) with the equal-power pan law, `(√(1 − p), √p)` times this, whatever
/// the channel count. (A file it cannot place, or takes for early ModPlug, it pans linearly at 0.375 or 0.25 hard: a
/// synthetic test XM must be detected as FT2; one channel or an odd count is not, a pattern swapped into a real XM is.)
pub const XM_GAIN: f64 = 0.375;

/// The longest echo (ms) the song's tempo is chosen for; a track's longer echo (a phrase triggered far below its
/// native note, as the risers' starts) is clamped with a warning.
pub const ECHO_MAX_MS: f64 = 4000.0;

/// The song tempo for echoes up to `ms`: only the tempo delay depends on it (its unit, an eighth, and its line,
/// sized for 16 eighths + 200 ms on every echo device), so the fastest tempo that still reaches `ms` keeps the
/// lines short.
pub fn tempo_for(ms: f64) -> i32 {
    ((240_000.0 / (ms - 200.0).max(1.0)).floor() as i32).clamp(56, 999)
}

/// An echo's time (ms) at playback `ratio`: 3 steps of 3529 phrase frames.
fn echo_ms(ratio: f64) -> f64 {
    3.0 * 3529.0 / (R * ratio) * 1000.0
}

/// A converted XM: the song, the frames of lead-in before the XM's time 0 (each track's notes start its chain latency
/// early), warnings, and the seconds of phrases played in ping-pong backward passes.
pub struct Converted {
    pub song: crate::model::SongM,
    pub lead: usize,
    pub warnings: Vec<String>,
    pub pingpong: f64,
    /// Seconds of slides past `reach`, silent (summed over triggers).
    pub muted: f64,
    /// Phrase tracks, stored-sample tracks.
    pub tracks: (usize, usize),
    /// Per song track (the master excluded): its record, `None` for a stored sample.
    pub records: Vec<Option<usize>>,
}

/// The call of `p` an XM was made with: Jizz's only call, or the Stash call whose generated and drum samples count the
/// XM's non-empty ones.
pub fn pick_call<'a>(p: &'a Patches, xm: &Xm) -> Result<&'a Call, String> {
    if p.calls.len() == 1 {
        return Ok(&p.calls[0]);
    }
    let n = xm.instruments.iter().flat_map(|i| &i.samples).filter(|s| !s.data.is_empty()).count();
    let want = |c: &Call| {
        c.order.iter().map(|s| match *s {
            Slot::Drums => c.drum_count,
            Slot::Render { record, .. } => c.records[record].variants as usize,
        }).sum::<usize>()
    };
    let fit: Vec<&Call> = p.calls.iter().filter(|c| want(c) == n).collect();
    match fit[..] {
        [c] => Ok(c),
        _ => Err(format!("the XM's {n} samples match no single call of the patch data ({})", p.calls.iter().map(|c| format!("{} {}", c.name, want(c))).collect::<Vec<_>>().join(", "))),
    }
}

/// Converts a mini-cauldron XM with its patch data to a unified song at `rate` (spec "Mini-cauldron XM import").
pub fn convert(p: &Patches, xm: &Xm, rate: i32) -> Result<Converted, String> {
    use crate::model::{Auto, Dev, SongM, Trk};
    use crate::xm::replay::replay;
    use unisynth::law::host as inv;
    use unisynth::params::{ctl, osc, shape as sp, v};
    let call = pick_call(p, xm)?;
    let ids = identify(call, xm)?;
    let r = replay(xm, rate as u32)?;
    let dec = |rec: usize| call.records[rec].decimation(call.version);
    let sample = |t: &crate::xm::replay::Trigger| &xm.instruments[t.instrument].samples[t.sample];
    let loop_of = |s: &crate::xm::Sample| Loop { len: s.data.len() as u32, start: s.loop_start, length: s.loop_len, kind: s.loop_kind };
    // every trigger's path (phrases: in phrase frames; stored samples: only its end matters)
    let paths: Vec<Path> = r
        .triggers
        .iter()
        .map(|t| {
            let d = ids[t.instrument][t.sample].map_or(1, |(rec, _)| dec(rec));
            path(&t.ticks, t.offset, &loop_of(sample(t)), d, rate as f64)
        })
        .collect();
    // a slide past one track's reach hands over: its segments become triggers of their own (handovers, clip)
    let (r, paths) = {
        let (mut trig, mut ps) = (vec![], vec![]);
        for (t, p) in r.triggers.iter().zip(paths) {
            let cuts = ids[t.instrument][t.sample].map_or(vec![], |(rec, _)| handovers(t, dec(rec), p.end));
            if cuts.is_empty() {
                trig.push(t.clone());
                ps.push(p);
                continue;
            }
            let mut b = vec![0];
            b.extend(cuts);
            b.push(t.ticks.len());
            for w in b.windows(2) {
                let ticks = t.ticks[w[0]..w[1]].to_vec();
                let (a, z) = (ticks[0].frame as f64, t.ticks.get(w[1]).map_or(f64::MAX, |k| k.frame as f64));
                trig.push(crate::xm::replay::Trigger { tick: t.tick + w[0], frame: ticks[0].frame, at: ticks[0].at, ticks, ..t.clone() });
                ps.push(clip(&p, a, z));
            }
        }
        (crate::xm::replay::Replay { triggers: trig, length: r.length }, ps)
    };
    let span = |i: usize| (r.triggers[i].frame as f64, paths[i].end);
    let plans = group(&r, &ids, dec, span);
    let mut stored: BTreeMap<((usize, usize), usize), Vec<usize>> = BTreeMap::new();
    for (i, t) in r.triggers.iter().enumerate() {
        if ids[t.instrument][t.sample].is_none() && !sample(t).data.is_empty() {
            stored.entry(((t.instrument, t.sample), t.channel)).or_default().push(i);
        }
    }
    let stored = merge(&r, stored, span);
    let longest = plans.iter().filter(|t| call.records[t.record].echo > 0.0).map(|t| echo_ms(t.ratio())).filter(|&ms| ms <= ECHO_MAX_MS).fold(0.0, f64::max);
    let tempo = tempo_for(longest);
    // a track whose triggers slide more than a semitone gets the band-limiting low-pass (slide_lowpass)
    let slides = |t: &TrackPlan| t.triggers.iter().any(|&i| {
        let tr = &r.triggers[i];
        let f: Vec<f64> = tr.ticks.iter().take_while(|k| (k.frame as f64) < paths[i].end).filter(|k| sounds(k)).map(|k| k.freq).collect();
        f.iter().zip(f.iter().skip(1)).any(|_| true) && {
            let (lo, hi) = f.iter().fold((f64::MAX, 0.0f64), |(a, b), &x| (a.min(x), b.max(x)));
            12.0 * (hi / lo).log2() > 1.0
        }
    });
    let chains: Vec<Chain> = plans
        .iter()
        .map(|t| {
            let mut c = effects(call, t.record, t.ratio(), rate as f64, tempo);
            if slides(t) {
                slide_lowpass(&mut c, call, t.record, t.ratio());
            }
            c
        })
        .collect();
    let lead = chains.iter().map(|c| c.latency as i64 + POST_DELAY).max().unwrap_or(0) + 2 * LANE_LEAD;
    let mut warnings = vec![];
    let mut song = SongM { tempo, rate, length: 0.0, devices: vec![], lanes: vec![], tracks: vec![] };
    // gain, pan and bend as the instrument's automation, a lane lead early: the voice ramps a parameter over the
    // sub-block after the player's step, so a gain stepping up at a note-on must land a sub-block before it (centred
    // on the step, a drum hit's first 100 frames rose from the previous hit's tail: −3 dB)
    // `post`: the track's post-chain gain device (phrases): the voice takes `master`, the device `post`
    let levels = |c: &Controls, latency: i64, values: &mut [f32; unisynth::params::COUNT], autos: &mut Vec<Auto>, post: Option<usize>| {
        // the shift as transpose + bend (split_shift); a track within ±MAX_BEND keeps bend alone
        let lane_t = c.bend.iter().any(|&(_, b)| split_shift(b, false).0 != 0.0);
        let parts: Vec<(u64, (f64, f64))> = c.bend.iter().map(|&(f, b)| (f, split_shift(b, lane_t))).collect();
        let (trans, bends): (Vec<_>, Vec<_>) = parts.iter().map(|&(f, (t, b))| ((f, t), (f, b))).unzip();
        let range = bends.iter().fold(0.0f64, |m, x| m.max(x.1.abs()));
        // the stored range is read back through its law: bend to it
        let back = if range > 0.0 {
            values[ctl::BEND_RANGE] = inv::inv_bend_range(range.max(1e-3) as f32);
            unisynth::law::bend_range(values[ctl::BEND_RANGE]) as f64
        } else {
            0.0
        };
        let at = |f: u64| lead - latency + f as i64 - LANE_LEAD;
        let byte = |p: f32| (p.clamp(0.0, 1.0) * 255.0).round() as u8;
        let mut lane = |param: usize, pts: &[(u64, f64)], to_p: &dyn Fn(f64) -> f32| {
            let ch: Vec<(i64, u8)> = pts.iter().map(|&(f, x)| (at(f), byte(to_p(x)))).collect();
            let b0 = ch.first().map_or(byte(values[param]), |x| x.1);
            values[param] = b0 as f32 / 255.0;
            if ch.iter().any(|x| x.1 != b0) {
                autos.push(Auto { device: 0, param, points: byte_lane(b0, &ch) });
            }
        };
        lane(v::MASTER, if post.is_some() { &c.master } else { &c.gain }, &|g| inv::inv_gain(g as f32));
        // a phrase voice plays centred: its pan is the post device's
        let centre = [(0u64, 0.5f64)];
        lane(v::PAN, if post.is_some() { &centre } else { &c.pan }, &|p| p as f32);
        if back > 0.0 {
            lane(ctl::BEND, &bends, &|b| (0.5 + b / (2.0 * back)) as f32);
        }
        if lane_t {
            lane(v::TRANSPOSE, &trans, &|x| inv::inv_transpose(x as f32));
        }
        if let Some(d) = post {
            // EQ 3band, all three bands at the gain (linear below 0.5): flat, 3 frames late; it sees output frame
            // `lead + f` at `lead + f − 3`
            let ch: Vec<(i64, u8)> = c.post.iter().map(|&(f, g)| (lead + f - POST_DELAY, byte((g / 2.0) as f32))).collect();
            let b0 = ch.first().map_or(0, |x| x.1);
            for param in 1..=3 {
                autos.push(Auto { device: d, param, points: byte_lane(b0, &ch) });
            }
            // its pan: a byte 1..255, hard left to hard right
            let ch: Vec<(i64, u8)> = c.post_pan.iter().map(|&(f, p)| (lead + f - POST_DELAY, 1 + (p.clamp(0.0, 1.0) * 254.0).round() as u8)).collect();
            let b0 = ch.first().map_or(128, |x| x.1);
            autos.push(Auto { device: d, param: 4, points: byte_lane(b0, &ch) });
        }
    };
    let (mut pingpong, mut muted) = (0.0, 0.0);
    let mut backward_samples: Vec<(usize, usize)> = vec![];
    for (plan, chain) in plans.iter().zip(&chains) {
        let rec = &call.records[plan.record];
        if let Some(ms) = chain.echo_clamped {
            warnings.push(format!("record {} at {:+} semitones: echo of {ms:.0} ms clamped to the delay's longest", plan.record, plan.class));
        }
        let mut values = instrument(call.version, rec, plan.ratio());
        let latency = chain.latency as i64 + POST_DELAY;
        let mut evs: Vec<(i64, PEv)> = vec![];
        let mut restarts = vec![];
        for &i in &plan.triggers {
            let t = &r.triggers[i];
            let (_, variant) = ids[t.instrument][t.sample].unwrap();
            let pth = &paths[i];
            if pth.backward > 0.0 {
                pingpong += pth.backward / rate as f64;
                if !backward_samples.contains(&(t.instrument, t.sample)) {
                    backward_samples.push((t.instrument, t.sample));
                    warnings.push(format!("sample `{}` (instrument {}) plays past its end in a ping-pong loop: its steps play in reverse order", sample(t).name.trim(), t.instrument + 1));
                }
            }
            evs.extend(unroll(call, plan.record, variant, plan.ratio(), pth).into_iter().map(|e| (lead - latency + e.at.round() as i64, e.what)));
            restarts.push((lead + t.frame as i64 - POST_DELAY, if restarts.len() % 2 == 0 { 255 } else { 0 }));
        }
        let (lane, mut autos) = note_lanes(evs.into_iter(), &mut values);
        let c = controls(&r, plan, dec, |i| paths[i].end);
        muted += c.muted;
        warnings.extend(c.warnings.iter().cloned());
        levels(&c, latency, &mut values, &mut autos, Some(1 + chain.devices.len()));
        // the decimation low-pass follows the slide: the stored Nyquist times the playing ratio (it sits at the
        // chain's end, before the post gain)
        if let Some((k, nyq)) = chain.lowpass {
            let open = |hz: f64| if hz < 0.45 * rate as f64 { lowpass_p(hz) } else { 1.0 };
            let byte = |p: f32| (p.clamp(0.0, 1.0) * 255.0).round() as u8;
            let ch: Vec<(i64, u8)> = c.bend.iter().map(|&(f, b)| (lead + f as i64 - LANE_LEAD - POST_DELAY, byte(open(nyq * plan.ratio() * 2f64.powf(b / 12.0))))).collect();
            if ch.iter().any(|x| x.1 != ch[0].1) {
                for s in 0..LP_STAGES {
                    autos.push(Auto { device: 1 + k + s, param: 6, points: byte_lane(ch[0].1, &ch) });
                }
            }
        }
        let base = song.devices.len();
        if let Some(k) = chain.restart {
            autos.push(Auto { device: 1 + k, param: 4, points: byte_lane(0, &restarts) });
        }
        song.devices.push(Dev { kind: unisynth::kind::INSTRUMENT, chunk: unisynth::chunk::encode(&values, &[]) });
        song.devices.extend(chain.devices.iter().map(|(k, v)| Dev { kind: *k, chunk: unisynth::fx::encode(v) }));
        song.devices.push(Dev { kind: unisynth::kind::EQ, chunk: unisynth::fx::encode(&post_gain()) });
        let halve = if call.version == Version::V1 && plan.record == 7 { 0.5 } else { 1.0 };
        let devices = (base..song.devices.len()).collect();
        song.tracks.push(Trk { volume: (XM_GAIN * halve / chain.level) as f32, receives: vec![], devices, lane: song.lanes.len(), autos });
        song.lanes.push(lane);
    }
    for ((ins, smp), trig) in &stored {
        let s = &xm.instruments[*ins].samples[*smp];
        let st = sampler(s, rate as u32);
        let mut values = st.values;
        let (mut lane, mut starts) = (vec![], vec![]);
        let mut t0 = 0i64;
        for &i in trig {
            let t = &r.triggers[i];
            let (on, off) = (lead + t.frame as i64, lead + paths[i].end.round() as i64);
            let note = st.note(t.note);
            lane.push(((on - t0) as i32, note, Some(127)));
            lane.push(((off - on) as i32, note, None));
            t0 = off;
            starts.push((on - LANE_LEAD, ((t.offset as f64 / s.data.len().max(1) as f64) * 255.0).round().min(255.0) as u8));
        }
        let freq = |i: usize| {
            let t = &r.triggers[i];
            8363.0 * 2f64.powf((t.note as f64 - 49.0 + s.relative as f64 + s.finetune as f64 / 128.0) / 12.0)
        };
        let c = controls_of(&r, trig, &format!("sample `{}`", s.name.trim()), freq, |i| paths[i].end);
        muted += c.muted;
        warnings.extend(c.warnings.iter().cloned());
        let mut autos = vec![];
        levels(&c, 0, &mut values, &mut autos, None);
        if starts.iter().any(|x| x.1 != 0) {
            let id = osc(0, unisynth::params::o::SHAPE + sp::SMP_START);
            values[id] = starts[0].1 as f32 / 255.0;
            autos.push(Auto { device: 0, param: id, points: byte_lane(starts[0].1, &starts) });
        }
        let base = song.devices.len();
        song.devices.push(Dev { kind: unisynth::kind::INSTRUMENT, chunk: unisynth::chunk::encode(&values, &[&st.gsm]) });
        song.tracks.push(Trk { volume: XM_GAIN as f32, receives: vec![], devices: vec![base], lane: song.lanes.len(), autos });
        song.lanes.push(lane);
    }
    // the master: every track summed, no devices
    let n = song.tracks.len();
    song.tracks.push(Trk { volume: 1.0, receives: (0..n).map(|t| (t, 0, 1.0)).collect(), devices: vec![], lane: song.lanes.len(), autos: vec![] });
    song.lanes.push(vec![]);
    // openmpt123 renders 0.1 s past the end
    song.length = (lead as f64 + (r.length + 0.1) * rate as f64) / rate as f64;
    Ok(Converted { song, lead: lead as usize, warnings, pingpong, muted, tracks: (plans.len(), stored.len()),
        records: plans.iter().map(|t| Some(t.record)).chain(stored.iter().map(|_| None)).collect() })
}
