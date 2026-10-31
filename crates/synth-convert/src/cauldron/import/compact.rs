//! The compact layout of a converted mini-cauldron XM (change compact-cauldron-xm-import): the XMs fake stereo and
//! reverb with extra channels (a note hard left and hard right, one side detuned, then quieter echo taps panned in
//! turn), and the split layout gives every playback-ratio class its own tracks. `layers` drops the repeats (dry,
//! centred, the absorbed copies' level kept); `pack` puts a record's phrase triggers on as few tracks as their overlap
//! and one track's pitch reach allow, and `pack_by` does the same for stored samples. What the layers did comes back
//! as effects on the track (`ambience`): the echo taps as a stereo delay (their times and sides), a small reverb on
//! a track whose triggers mostly had layers.

use super::{class_of, sounds, TrackPlan, MAX_CLASS, MAX_SHIFT, MIN_CLASS, R, SEGMENT_SPAN};
use crate::xm::replay::Replay;
use std::collections::BTreeMap;

/// How long (s) after a kept trigger a quieter repeat on another channel still counts as its echo tap.
pub const ECHO_WINDOW_S: f64 = 2.5;

/// What `layers` kept and dropped.
#[derive(Clone, Debug, PartialEq)]
pub struct Layers {
    /// Kept trigger indices, ascending.
    pub keep: Vec<usize>,
    /// Dropped copies starting with a kept trigger, and dropped echo taps.
    pub same_tick: usize,
    pub echoes: usize,
    /// Per kept trigger (as `keep`): what it absorbed.
    pub absorbed: Vec<Absorbed>,
}

/// The layers a kept trigger absorbed: same-frame copies (`pair`) and echo taps.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Absorbed {
    pub pair: bool,
    pub taps: Vec<Tap>,
}

/// A dropped echo tap: seconds after its kept trigger, its first-tick pan, and its first-tick volume over the kept
/// trigger's (before absorption).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tap {
    pub delay: f64,
    pub pan: f64,
    pub gain: f64,
}

/// Drops the layered triggers of `r` (spec "Layered triggers dropped"): taken in time order, a trigger repeating a
/// kept one (same instrument, sample, note and offset, another channel) is dropped when it starts in the same frame,
/// or later, before the kept one's sounding `end` (output frame) and within `ECHO_WINDOW_S`, at a lower first-tick
/// volume. A kept trigger absorbs its same-frame copies' level (spec "Absorbed layers keep their level"): per side,
/// the first-tick levels of the copies at its own first-tick frequency add as amplitudes (identical signals), those of
/// detuned copies as powers; the sum sets its pan (equal-power) and scales its volume, on every tick.
pub fn layers(r: &mut Replay, end: impl Fn(usize) -> f64, rate: f64) -> Layers {
    let firsts: Vec<(f64, f64, f64)> = r.triggers.iter().map(|t| t.ticks.first().map_or((0.0, 0.5, 0.0), |k| (k.vol, k.pan, k.freq))).collect();
    let first = |i: usize| (firsts[i].0, firsts[i].1);
    let lr = |(g, p): (f64, f64)| (g * (1.0 - p).sqrt(), g * p.sqrt());
    let mut order: Vec<usize> = (0..r.triggers.len()).collect();
    order.sort_by_key(|&i| (r.triggers[i].frame, r.triggers[i].channel));
    let key = |i: usize| {
        let t = &r.triggers[i];
        (t.instrument, t.sample, t.note, t.offset)
    };
    // kept triggers by key (latest last), and each kept one's absorbed left/right level: amplitudes of the copies at its
    // frequency (itself included), powers of the detuned ones
    let mut kept: BTreeMap<(usize, usize, u8, u32), Vec<usize>> = BTreeMap::new();
    let mut level: BTreeMap<usize, [f64; 4]> = BTreeMap::new();
    let mut absorbed: BTreeMap<usize, Absorbed> = BTreeMap::new();
    let (mut same_tick, mut echoes) = (0, 0);
    let mut keep = vec![];
    for &j in &order {
        let tj = &r.triggers[j];
        let hit = kept.get(&key(j)).and_then(|ks| {
            ks.iter().rev().copied().find(|&i| {
                let ti = &r.triggers[i];
                ti.channel != tj.channel
                    && (ti.frame == tj.frame
                        || ((tj.frame as f64) < end(i) && (tj.frame - ti.frame) as f64 <= ECHO_WINDOW_S * rate && first(j).0 < first(i).0))
            })
        });
        match hit {
            Some(i) if r.triggers[i].frame == tj.frame => {
                let (l, rr) = lr(first(j));
                let e = level.get_mut(&i).unwrap();
                if firsts[j].2 == firsts[i].2 {
                    (e[0], e[1]) = (e[0] + l, e[1] + rr);
                } else {
                    (e[2], e[3]) = (e[2] + l * l, e[3] + rr * rr);
                }
                absorbed.entry(i).or_default().pair = true;
                same_tick += 1;
            }
            Some(i) => {
                let tap = Tap { delay: (tj.frame - r.triggers[i].frame) as f64 / rate, pan: first(j).1, gain: first(j).0 / first(i).0 };
                absorbed.entry(i).or_default().taps.push(tap);
                echoes += 1;
            }
            None => {
                kept.entry(key(j)).or_default().push(j);
                let (l, rr) = lr(first(j));
                level.insert(j, [l, rr, 0.0, 0.0]);
                keep.push(j);
            }
        }
    }
    for (&i, &[lc, rc, lp, rp]) in &level {
        let g0 = first(i).0;
        if !absorbed.get(&i).is_some_and(|a| a.pair) || g0 <= 0.0 {
            continue;
        }
        let (l2, r2) = (lc * lc + lp, rc * rc + rp);
        let g = (l2 + r2).sqrt();
        let (scale, pan) = (g / g0, r2 / (l2 + r2));
        for k in &mut r.triggers[i].ticks {
            k.vol *= scale;
            k.pan = pan;
        }
    }
    keep.sort_unstable();
    let absorbed = keep.iter().map(|i| absorbed.remove(i).unwrap_or_default()).collect();
    Layers { keep, same_tick, echoes, absorbed }
}

/// Puts the phrase triggers of `r` on tracks (spec "Track grouping", default): in time order, a trigger joins the
/// first track of its record it overlaps nowhere (`span`) whose sounding pitch range, with its own, spans at most
/// `SEGMENT_SPAN`; the track's class is its triggers' most frequent `class_of`, moved into the range all reach.
pub fn pack(r: &Replay, ids: &[Vec<Option<(usize, usize)>>], decim: impl Fn(usize) -> u32, span: impl Fn(usize) -> (f64, f64)) -> Vec<TrackPlan> {
    struct Open {
        record: usize,
        triggers: Vec<usize>,
        classes: Vec<i32>,
        lo: f64,
        hi: f64,
        end: f64,
    }
    let mut order: Vec<usize> = (0..r.triggers.len()).collect();
    order.sort_by_key(|&i| (r.triggers[i].frame, r.triggers[i].channel));
    let mut open: Vec<Open> = vec![];
    for i in order {
        let t = &r.triggers[i];
        let Some((record, _)) = ids.get(t.instrument).and_then(|s| s.get(t.sample)).copied().flatten() else { continue };
        let (start, end) = span(i);
        let class = class_of(t, decim(record), end);
        // the sounding range (semitones from native) a track must reach; a trigger without one reaches its class
        let semis = |f: f64| 12.0 * (f * decim(record) as f64 / R).log2();
        let (mut lo, mut hi) = t
            .ticks
            .iter()
            .take_while(|k| (k.frame as f64) < end)
            .filter(|k| sounds(k) && semis(k.freq) >= MIN_CLASS as f64 - MAX_SHIFT)
            .fold((f64::MAX, f64::MIN), |(a, b), k| (a.min(semis(k.freq)), b.max(semis(k.freq))));
        if lo > hi {
            (lo, hi) = (class as f64, class as f64);
        }
        match open.iter_mut().find(|o| o.record == record && o.end <= start && o.hi.max(hi) - o.lo.min(lo) <= SEGMENT_SPAN) {
            Some(o) => {
                o.triggers.push(i);
                o.classes.push(class);
                (o.lo, o.hi, o.end) = (o.lo.min(lo), o.hi.max(hi), o.end.max(end));
            }
            None => open.push(Open { record, triggers: vec![i], classes: vec![class], lo, hi, end }),
        }
    }
    open.into_iter()
        .map(|o| {
            let mut count: BTreeMap<i32, usize> = BTreeMap::new();
            for &c in &o.classes {
                *count.entry(c).or_default() += 1;
            }
            // most frequent; a tie goes to the class nearest native
            let mode = count.iter().max_by_key(|(c, n)| (**n, std::cmp::Reverse(c.abs()))).map_or(0, |(c, _)| *c);
            let (lower, upper) = ((o.hi - MAX_SHIFT).ceil() as i32, (o.lo + MAX_SHIFT).floor() as i32);
            let class = mode.clamp(lower, upper.max(lower)).clamp(MIN_CLASS, MAX_CLASS);
            TrackPlan { record: o.record, class, triggers: o.triggers }
        })
        .collect()
}

/// Packs triggers keyed by `key` (`None`: not this kind) into tracks per key: in time order, a trigger joins the first
/// track of its key it overlaps nowhere (`span`), whatever its channel. Tracks keep their triggers in time order.
pub fn pack_by<K: Ord + Copy>(r: &Replay, key: impl Fn(usize) -> Option<K>, span: impl Fn(usize) -> (f64, f64)) -> Vec<(K, Vec<usize>)> {
    let mut order: Vec<usize> = (0..r.triggers.len()).collect();
    order.sort_by_key(|&i| (r.triggers[i].frame, r.triggers[i].channel));
    let mut out: Vec<(K, Vec<usize>, f64)> = vec![];
    for i in order {
        let Some(k) = key(i) else { continue };
        let (start, end) = span(i);
        match out.iter_mut().find(|(kk, _, e)| *kk == k && *e <= start) {
            Some((_, t, e)) => {
                t.push(i);
                *e = e.max(end);
            }
            None => out.push((k, vec![i], end)),
        }
    }
    out.sort_by_key(|x| x.0);
    out.into_iter().map(|(k, t, _)| (k, t)).collect()
}

/// The effects that stand in for a track's dropped layers (change compact-cauldron-xm-import): the echo taps' stereo
/// delay and a small reverb.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ambience {
    pub echo: Option<Echo>,
    pub reverb: bool,
}

/// A two-tap stereo echo: the left and right tap times (s) and their level over the dry signal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Echo {
    pub left: f64,
    pub right: f64,
    pub gain: f64,
}

/// The small reverb on a layered track (freeverb mode after the mode value): no freeze, a small room, half damping,
/// full width, wet cut to 150 Hz–7 kHz, 12 % wet, 10 ms pre-delay.
pub const REVERB: [f32; 8] = [0.0, 0.3, 0.5, 1.0, 0.0807, 0.591, 0.12, 0.02];

/// One trigger's echo from its taps: the first left tap (pan below ½) and the first right one; a missing side takes
/// the other's time; the level is the two taps' RMS. `None` without taps.
fn echo_of(taps: &[Tap]) -> Option<Echo> {
    let mut by: Vec<&Tap> = taps.iter().collect();
    by.sort_by(|a, b| a.delay.total_cmp(&b.delay));
    let l = by.iter().find(|t| t.pan < 0.5).copied();
    let r = by.iter().find(|t| t.pan >= 0.5).copied();
    let (l, r) = match (l, r) {
        (Some(l), Some(r)) => (l, r),
        (Some(x), None) | (None, Some(x)) => (x, x),
        (None, None) => return None,
    };
    Some(Echo { left: l.delay, right: r.delay, gain: ((l.gain * l.gain + r.gain * r.gain) / 2.0).sqrt() })
}

/// A track's ambience from what its triggers absorbed: none unless at least half of them had layers; then the
/// reverb, and the echo most of the tapped triggers share (times to the millisecond, level to 1/64) when at least
/// half of the track's triggers had taps.
pub fn ambience(abs: &[&Absorbed]) -> Ambience {
    let n = abs.len();
    if 2 * abs.iter().filter(|a| a.pair || !a.taps.is_empty()).count() < n || n == 0 {
        return Ambience::default();
    }
    let echoes: Vec<Echo> = abs.iter().filter_map(|a| echo_of(&a.taps)).collect();
    let echo = if 2 * echoes.len() < n {
        None
    } else {
        let key = |e: &Echo| ((e.left * 1000.0).round() as i64, (e.right * 1000.0).round() as i64, (e.gain * 64.0).round() as i64);
        let mut count: BTreeMap<(i64, i64, i64), (usize, Echo)> = BTreeMap::new();
        for e in &echoes {
            count.entry(key(e)).or_insert((0, *e)).0 += 1;
        }
        count.values().max_by_key(|(c, _)| *c).map(|(_, e)| *e)
    };
    Ambience { echo, reverb: true }
}

impl Ambience {
    /// The devices after the track's post gain (`(kind, [mode value, p…])`, tempo delay then freeverb) at `tempo`
    /// BPM, and the track volume factor that keeps the dry signal at its level (each mixes dry at `1 − wet`). The
    /// delay has no feedback: each side plays its tap once, at `gain` (wet `gain/(1 + gain)`).
    pub fn devices(&self, tempo: i32) -> (Vec<(u8, Vec<f32>)>, f64) {
        use unisynth::fx::mode_value;
        use unisynth::kind::{DELAY, REVERB as KIND};
        let (mut out, mut volume) = (vec![], 1.0);
        if let Some(e) = self.echo {
            let (cl, fl, _) = crate::cauldron::delay_time(e.left * 1000.0, tempo);
            let (cr, fr, _) = crate::cauldron::delay_time(e.right * 1000.0, tempo);
            let wet = e.gain / (1.0 + e.gain);
            out.push((DELAY, vec![mode_value(DELAY, 0), cl, fl, cr, fr, 0.0, 1.0, 0.0, 0.0, wet as f32]));
            volume /= 1.0 - wet;
        }
        if self.reverb {
            let mut v = vec![mode_value(KIND, 0)];
            v.extend(REVERB);
            out.push((KIND, v));
            volume /= 1.0 - REVERB[6] as f64;
        }
        (out, volume)
    }

    /// The longest echo tap (ms): the song tempo must reach it.
    pub fn longest_ms(&self) -> f64 {
        self.echo.map_or(0.0, |e| e.left.max(e.right) * 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xm::replay::{TickCtl, Trigger};

    const RATE: f64 = 44100.0;
    const TICK: u64 = 730;

    /// A trigger of instrument 0 `sample` on `channel` at tick `at`, `n` ticks at `ratio` × native (decimation 2),
    /// first-tick volume `vol` (then 1) and pan `pan`.
    fn trig(channel: usize, sample: usize, at: u64, n: usize, ratio: f64, vol: f64, pan: f64) -> Trigger {
        let frame = at * TICK;
        let ticks = (0..n)
            .map(|i| {
                let f = frame + i as u64 * TICK;
                TickCtl { row: true, at: f as f64 / RATE, frame: f, frames: TICK as u32, freq: ratio * R / 2.0, vol, pan }
            })
            .collect();
        Trigger { channel, tick: at as usize, frame, at: frame as f64 / RATE, instrument: 0, sample, note: 62, offset: 0, ticks }
    }

    fn ends(r: &Replay) -> Vec<f64> {
        r.triggers.iter().map(|t| (t.frame + TICK * t.ticks.len() as u64) as f64).collect()
    }

    fn ids() -> Vec<Vec<Option<(usize, usize)>>> {
        vec![vec![Some((0, 0)), Some((0, 1)), Some((1, 0)), None]]
    }

    #[test]
    fn a_stereo_pair_becomes_one_centred_trigger_at_the_pairs_level() {
        // spec "Stereo pair", "Centred pair": hard left and hard right at full volume, same frame
        let mut r = Replay { triggers: vec![trig(0, 0, 0, 20, 1.0, 1.0, 0.0), trig(1, 0, 0, 20, 1.0, 1.0, 1.0)], length: 1.0 };
        let e = ends(&r);
        let l = layers(&mut r, |i| e[i], RATE);
        assert_eq!((l.keep.clone(), l.same_tick, l.echoes), (vec![0], 1, 0));
        let k = &r.triggers[0].ticks;
        assert!(k.iter().all(|k| (k.vol - 2f64.sqrt()).abs() < 1e-12 && (k.pan - 0.5).abs() < 1e-12), "{:?}", k[0]);
        // two centred copies: one at twice the volume (identical signals add)
        let mut r = Replay { triggers: vec![trig(0, 0, 0, 20, 1.0, 0.5, 0.5), trig(1, 0, 0, 20, 1.0, 0.5, 0.5)], length: 1.0 };
        let e = ends(&r);
        layers(&mut r, |i| e[i], RATE);
        assert!((r.triggers[0].ticks[0].vol - 1.0).abs() < 1e-12 && (r.triggers[0].ticks[0].pan - 0.5).abs() < 1e-12);
        // a detuned copy (Jizz's `E11` side, panned 0xF0) adds power, not amplitude: hard left + 15/16 right at full
        // volume plays as √2 (each voice's power kept), its pan the right side's share of the power
        let mut r = Replay { triggers: vec![trig(0, 0, 0, 20, 1.0, 1.0, 0.0), trig(1, 0, 0, 20, 1.004, 1.0, 0.9375)], length: 1.0 };
        let e = ends(&r);
        layers(&mut r, |i| e[i], RATE);
        let k = r.triggers[0].ticks[0];
        assert!((k.vol - 2f64.sqrt()).abs() < 1e-12 && (k.pan - 0.9375 / 2.0).abs() < 1e-12, "{k:?}");
    }

    #[test]
    fn a_quieter_repeat_while_the_first_sounds_is_an_echo_tap() {
        // spec "Echo tap": volume 64 then 32 three rows (18 ticks) later on another channel; the first is unchanged
        let mut r = Replay { triggers: vec![trig(0, 0, 0, 40, 1.0, 1.0, 0.0), trig(2, 0, 18, 40, 1.0, 0.5, 1.0), trig(3, 0, 36, 40, 1.0, 0.25, 0.0)], length: 1.0 };
        let before = r.triggers[0].clone();
        let e = ends(&r);
        let l = layers(&mut r, |i| e[i], RATE);
        assert_eq!((l.keep, l.same_tick, l.echoes), (vec![0], 0, 2));
        assert_eq!(r.triggers[0], before);
    }

    #[test]
    fn a_louder_later_or_other_note_stays() {
        // spec "A louder or later note stays"
        let cases = [
            trig(1, 0, 10, 20, 1.0, 1.0, 0.5), // same volume, later
            trig(1, 0, 30, 20, 1.0, 0.5, 0.5), // after the first ended (20 ticks)
            trig(1, 1, 10, 20, 1.0, 0.5, 0.5), // another sample
            trig(0, 0, 10, 20, 1.0, 0.5, 0.5), // the same channel
        ];
        for c in cases {
            let mut r = Replay { triggers: vec![trig(0, 0, 0, 20, 1.0, 1.0, 0.5), c.clone()], length: 1.0 };
            let e = ends(&r);
            assert_eq!(layers(&mut r, |i| e[i], RATE).keep, vec![0, 1], "{:?}", (c.channel, c.sample, c.tick));
        }
        // past the echo window: stays even while the first still sounds
        let late = (ECHO_WINDOW_S * RATE / TICK as f64) as u64 + 2;
        let mut r = Replay { triggers: vec![trig(0, 0, 0, 400, 1.0, 1.0, 0.5), trig(1, 0, late, 20, 1.0, 0.5, 0.5)], length: 1.0 };
        let e = ends(&r);
        assert_eq!(layers(&mut r, |i| e[i], RATE).keep, vec![0, 1]);
    }

    fn spans(r: &Replay) -> impl Fn(usize) -> (f64, f64) + '_ {
        move |i| {
            let t = &r.triggers[i];
            (t.frame as f64, (t.frame + TICK * t.ticks.len() as u64) as f64)
        }
    }

    #[test]
    fn one_record_at_two_ratios_one_after_the_other_is_one_track() {
        // spec "Two channels, two ratios": native on channel 0, then an octave up on channel 1
        let r = Replay { triggers: vec![trig(0, 0, 0, 20, 1.0, 1.0, 0.5), trig(1, 1, 20, 20, 2.0, 1.0, 0.5)], length: 1.0 };
        let t = pack(&r, &ids(), |_| 2, spans(&r));
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].triggers, vec![0, 1]);
        // a tie between classes 0 and 12 goes to native
        assert_eq!(t[0].class, 0);
        // the split layout keeps two
        assert_eq!(super::super::group(&r, &ids(), |_| 2, spans(&r)).len(), 2);
    }

    #[test]
    fn overlap_and_reach_split_tracks() {
        // spec "Out of reach": 100 semitones apart, one after the other: two tracks; overlapping: two; another record
        // its own; a stored sample none
        let far = 2f64.powf(100.0 / 12.0);
        let r = Replay { triggers: vec![trig(0, 0, 0, 20, 1.0 / 64.0, 1.0, 0.5), trig(1, 1, 20, 20, far / 64.0, 1.0, 0.5)], length: 1.0 };
        assert_eq!(pack(&r, &ids(), |_| 2, spans(&r)).len(), 2);
        let r = Replay {
            triggers: vec![trig(0, 0, 0, 20, 1.0, 1.0, 0.5), trig(1, 1, 10, 20, 1.0, 1.0, 0.5), trig(2, 2, 0, 20, 1.0, 1.0, 0.5), trig(3, 3, 0, 20, 1.0, 1.0, 0.5)],
            length: 1.0,
        };
        let mut k: Vec<(usize, Vec<usize>)> = pack(&r, &ids(), |_| 2, spans(&r)).into_iter().map(|t| (t.record, t.triggers)).collect();
        k.sort();
        assert_eq!(k, [(0, vec![0]), (0, vec![1]), (1, vec![2])]);
    }

    #[test]
    fn the_class_is_the_most_played_moved_into_reach() {
        // three triggers at +12 and one at −30 (42 apart, within reach): class 12 reaches −30 (−42 ≥ −MAX_SHIFT)
        let at = |c: f64| 2f64.powf(c / 12.0);
        let tr = |n: u64, c: f64| trig(0, 0, n * 20, 20, at(c), 1.0, 0.5);
        let r = Replay { triggers: vec![tr(0, 12.0), tr(1, 12.0), tr(2, 12.0), tr(3, -30.0)], length: 1.0 };
        let t = pack(&r, &ids(), |_| 2, spans(&r));
        assert_eq!((t.len(), t[0].class), (1, 12));
        // at +40 and −50 (90 apart): the most played (+40) cannot reach −50, the class moves to −50 + MAX_SHIFT
        let r = Replay { triggers: vec![tr(0, 40.0), tr(1, 40.0), tr(2, -50.0)], length: 1.0 };
        let t = pack(&r, &ids(), |_| 2, spans(&r));
        assert_eq!((t.len(), t[0].class), (1, -50 + MAX_SHIFT as i32));
        // every trigger within ±MAX_SHIFT of the class
        let c = super::super::controls(&r, &t[0], |_| 2, |i| spans(&r)(i).1);
        assert_eq!(c.muted, 0.0, "{:?}", c.warnings);
    }

    #[test]
    fn stored_samples_pack_across_channels() {
        let r = Replay { triggers: vec![trig(0, 3, 0, 20, 1.0, 1.0, 0.5), trig(5, 3, 20, 20, 1.0, 1.0, 0.5), trig(6, 3, 30, 20, 1.0, 1.0, 0.5)], length: 1.0 };
        let p = pack_by(&r, |i| Some(r.triggers[i].sample), spans(&r));
        assert_eq!(p, [(3, vec![0, 1]), (3, vec![2])]);
    }

    #[test]
    fn the_absorbed_layers_are_recorded_per_kept_trigger() {
        // a hard pair plus two taps (3 and 6 rows of 6 ticks: 18 and 36 ticks) at half volume, right then left
        let mut r = Replay {
            triggers: vec![trig(0, 0, 0, 60, 1.0, 1.0, 0.0), trig(1, 0, 0, 60, 1.0, 1.0, 1.0), trig(2, 0, 18, 60, 1.0, 0.5, 0.94), trig(3, 0, 36, 60, 1.0, 0.5, 0.0), trig(4, 1, 0, 10, 1.0, 1.0, 0.5)],
            length: 1.0,
        };
        let e = ends(&r);
        let l = layers(&mut r, |i| e[i], RATE);
        assert_eq!(l.keep, vec![0, 4]);
        let a = &l.absorbed[0];
        assert!(a.pair && a.taps.len() == 2);
        assert!((a.taps[0].delay - 18.0 * TICK as f64 / RATE).abs() < 1e-12 && (a.taps[0].gain - 0.5).abs() < 1e-12 && (a.taps[0].pan - 0.94).abs() < 1e-12);
        assert_eq!(l.absorbed[1], Absorbed::default());
    }

    fn tap(delay: f64, pan: f64, gain: f64) -> Tap {
        Tap { delay, pan, gain }
    }

    #[test]
    fn a_layered_track_gets_its_taps_as_a_stereo_delay_and_a_reverb() {
        let pad = Absorbed { pair: true, taps: vec![tap(0.6, 0.0, 0.5), tap(0.3, 0.94, 0.25)] };
        let plain = Absorbed::default();
        // the left tap at 0.6 s, the right at 0.3 s, level the RMS of ½ and ¼
        let a = ambience(&[&pad, &pad, &plain]);
        assert!(a.reverb);
        let e = a.echo.unwrap();
        assert_eq!((e.left, e.right), (0.6, 0.3));
        assert!((e.gain - (0.3125f64 / 2.0).sqrt()).abs() < 1e-12);
        assert_eq!(a.longest_ms(), 600.0);
        // mostly plain triggers: nothing; pairs without taps: the reverb alone; one side only: both sides at its time
        assert_eq!(ambience(&[&pad, &plain, &plain]), Ambience::default());
        let pair = Absorbed { pair: true, taps: vec![] };
        assert_eq!(ambience(&[&pair, &pair]), Ambience { echo: None, reverb: true });
        let left = Absorbed { pair: false, taps: vec![tap(0.4, 0.0, 0.5)] };
        assert_eq!(ambience(&[&left]).echo, Some(Echo { left: 0.4, right: 0.4, gain: 0.5 }));
        // the most common echo wins
        let other = Absorbed { pair: true, taps: vec![tap(0.2, 0.0, 0.5), tap(0.4, 1.0, 0.5)] };
        assert_eq!(ambience(&[&other, &pad, &other]).echo.unwrap().left, 0.2);
    }

    #[test]
    fn the_delay_plays_each_tap_once_and_the_volume_keeps_the_dry_level() {
        let a = Ambience { echo: Some(Echo { left: 0.6, right: 0.3, gain: 0.5 }), reverb: true };
        let (d, vol) = a.devices(125);
        assert_eq!(d.len(), 2);
        let (k, p) = &d[0];
        assert_eq!(*k, unisynth::kind::DELAY);
        // 125 BPM: an eighth (the delay's unit) is 120 ms; 600 ms = 5 units, 300 ms = 2 units + 60 ms
        let c = |x: f32| (x * 16.0) as i32;
        let f = |x: f32| (x * 200.0) as i32;
        assert_eq!((c(p[1]), f(p[2]), c(p[3]), f(p[4])), (5, 0, 2, 60));
        assert_eq!((p[7], p[8]), (0.0, 0.0)); // no feedback, no cross
        assert!((p[9] as f64 - 1.0 / 3.0).abs() < 1e-6); // wet ½/(1 + ½): each tap at ½ of the dry
        assert_eq!(d[1].0, unisynth::kind::REVERB);
        // dry (1 − ⅓)·(1 − 0.12) made up by the track volume
        assert!((vol - 1.0 / ((2.0 / 3.0) * 0.88)).abs() < 1e-6);
        assert_eq!(Ambience::default().devices(125), (vec![], 1.0));
    }
}
