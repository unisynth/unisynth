use super::super::tests::data;
use super::*;
use crate::xm::replay::TickCtl;

const RATE: f64 = 44100.0;

fn jizz() -> Call {
    load(&data("patches_jizz.json")).unwrap().calls.remove(0)
}

/// `n` ticks of 730 frames at a constant playback frequency.
fn ticks(n: usize, freq: f64) -> Vec<TickCtl> {
    ticks_by(n, |_| freq)
}

fn ticks_by(n: usize, freq: impl Fn(usize) -> f64) -> Vec<TickCtl> {
    (0..n).map(|i| TickCtl { at: i as f64 * 730.0 / RATE, frame: i as u64 * 730, frames: 730, freq: freq(i), vol: 1.0, pan: 0.5 }).collect()
}

/// Jizz's stored samples: 28 000 bytes at R/2.
fn jizz_sample() -> Loop {
    Loop { len: 28000, start: 0, length: 0, kind: 0 }
}

fn close(a: &[PhraseEv], b: &[PhraseEv]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.what == y.what && (x.at - y.at).abs() < 1e-6)
}

/// The phrase events scaled to output frames for a constant ratio from frame 0.
fn scaled(call: &Call, record: usize, variant: usize, ratio: f64) -> Vec<PhraseEv> {
    let k = RATE / (R * ratio);
    phrase_events(call, record, variant, ratio).into_iter().map(|e| PhraseEv { at: e.at * k, ..e }).collect()
}

#[test]
fn a_native_trigger_plays_the_phrase_events() {
    // tied slides and note ends as `phrase_events` (spec "Tied steps slide"), one step = 3530·44100/R frames
    let j = jizz();
    for record in 0..j.records.len() {
        let p = path(&ticks(150, R / 2.0), 0, &jizz_sample(), 2, RATE);
        assert!((p.end - 56000.0 * RATE / R).abs() < 1e-6, "ends at the sample's end: {}", p.end);
        assert_eq!(p.backward, 0.0);
        let got = unroll(&j, record, 0, 1.0, &p);
        assert!(close(&got, &scaled(&j, record, 0, 1.0)), "record {record}:\n{got:?}");
    }
}

#[test]
fn an_octave_down_trigger_plays_every_step_twice_as_long() {
    let j = jizz();
    let p = path(&ticks(300, R / 4.0), 0, &jizz_sample(), 2, RATE);
    assert!(close(&unroll(&j, 0, 0, 0.5, &p), &scaled(&j, 0, 0, 0.5)));
    // and an octave lower: the same words through a ratio of ½
    assert_eq!(Tuning::semis_at(400, 0.5), Tuning::semis_at(400, 1.0) - 12.0);
}

#[test]
fn a_sample_offset_starts_with_the_step_under_it() {
    // offset into the middle of step 5 of a record whose steps 5 and 6 are untied: the first note is step 5's
    // (its retrigger and phrase position), lasting the rest of the step
    let j = jizz();
    let (record, r) = j.records.iter().enumerate().find(|(_, r)| r.tie[5] == 0 && r.tie[6] == 0).unwrap();
    let off = (5.5 * STEP as f64 / 2.0) as u32; // stored samples (decim 2)
    let p = path(&ticks(150, R / 2.0), off, &jizz_sample(), 2, RATE);
    let ev = unroll(&j, record, 0, 1.0, &p);
    let x0 = 2.0 * off as f64;
    let PEv::On { retrig, vel, note, .. } = ev[0].what else { panic!("{:?}", ev[0]) };
    assert_eq!(ev[0].at, 0.0);
    assert_eq!(retrig, note_byte(&r.retrig(0, 5).unwrap()));
    assert_eq!(vel, velocity(x0));
    assert_eq!(note, Tuning::of(&r.pitch, 1.0).note(r.pitch[5]).0);
    // step 6's note-on falls at the end of step 5
    let on6 = ev.iter().find(|e| matches!(e.what, PEv::On { .. }) && e.at > 0.0).unwrap();
    assert!((on6.at - (6.0 * STEP as f64 - x0) * RATE / R).abs() < 1e-6, "{on6:?}");
}

#[test]
fn a_jump_into_a_tied_step_enters_with_the_envelope_aged() {
    // offset into step 5 of a record whose step 5 is tied: the entry note retriggers like the step that started the
    // tie (h) and carries the envelope's age since then, x − h·STEP in eighth steps + 1 (o::TRACKING); the notes the
    // phrase itself retriggers restart the envelope (0)
    let j = jizz();
    let (record, r) = j.records.iter().enumerate().find(|(_, r)| r.tie[5] != 0 && r.retrig(0, 5).is_none()).unwrap();
    let h = (0..=5).rev().find(|&h| r.retrig(0, h).is_some()).unwrap();
    let off = (5.5 * STEP as f64 / 2.0) as u32; // stored samples (decim 2)
    let ev = unroll(&j, record, 0, 1.0, &path(&ticks(150, R / 2.0), off, &jizz_sample(), 2, RATE));
    let x0 = 2.0 * off as f64;
    let PEv::On { retrig, age, .. } = ev[0].what else { panic!("{:?}", ev[0]) };
    assert_eq!(retrig, note_byte(&r.retrig(0, h).unwrap()));
    assert_eq!(age, 1 + ((x0 - (h as u32 * STEP) as f64) * 8.0 / STEP as f64).round() as u8);
    assert!(ev[1..].iter().all(|e| !matches!(e.what, PEv::On { age, .. } if age != 0)), "{ev:?}");
    let ev = unroll(&j, record, 0, 1.0, &path(&ticks(150, R / 2.0), 0, &jizz_sample(), 2, RATE));
    assert!(ev.iter().all(|e| !matches!(e.what, PEv::On { age, .. } if age != 0)), "{ev:?}");
}

#[test]
fn tone_portamento_moves_the_step_clock() {
    // the playback frequency rises every tick: each step starts where the integrated position crosses its boundary
    let j = jizz();
    let f = |i: usize| R / 2.0 * (1.0 + 0.01 * i as f64);
    let p = path(&ticks_by(150, f), 0, &jizz_sample(), 2, RATE);
    let ons: Vec<f64> = unroll(&j, 0, 0, 1.0, &p).iter().filter(|e| matches!(e.what, PEv::On { .. })).map(|e| e.at).collect();
    let r = &j.records[0];
    let starts: Vec<f64> = (0..16).filter(|&s| r.tie[s] == 0).map(|s| (s as u32 * STEP) as f64).collect();
    // the time the position (phrase frames) reaches x, by integrating tick by tick
    let when = |x: f64| {
        let (mut pos, mut t) = (0.0, 0.0);
        for i in 0.. {
            let v = 2.0 * f(i) / RATE;
            if pos + v * 730.0 >= x {
                return t + (x - pos) / v;
            }
            pos += v * 730.0;
            t += 730.0;
        }
        unreachable!()
    };
    assert_eq!(ons.len(), starts.len());
    for (a, x) in ons.iter().zip(&starts) {
        assert!((a - when(*x)).abs() < 1e-6, "{a} vs {}", when(*x));
    }
}

#[test]
fn a_later_variant_is_brighter() {
    // spec "Later variant is brighter": variant 2's first note carries more compounding retriggers than variant 1's
    let j = jizz();
    let (record, _) = j.records.iter().enumerate().find(|(_, r)| r.mul > 1.0 && r.variants >= 3).unwrap();
    let first = |variant| {
        let ev = unroll(&j, record, variant, 1.0, &path(&ticks(150, R / 2.0), 0, &jizz_sample(), 2, RATE));
        let PEv::On { retrig, .. } = ev[0].what else { panic!() };
        retrig
    };
    assert!(first(2) > first(1), "{} vs {}", first(2), first(1));
}

#[test]
fn a_pad_past_its_end_plays_the_loop_in_reversed_step_order() {
    // ping-pong loop over steps 8–11: after the forward pass the steps come 11, 10, 9, 8 (each forwards), then
    // 8, 9, 10, 11 again (spec "Pad past its end")
    let j = jizz();
    let s = Loop { len: 28000, start: 8 * STEP / 2, length: 4 * STEP / 2, kind: 2 };
    let p = path(&ticks(400, R / 2.0), 0, &s, 2, RATE);
    assert!(p.backward > 0.0);
    let ev = unroll(&j, 0, 0, 1.0, &p);
    let steps: Vec<u32> = ev.iter().filter_map(|e| if let PEv::On { vel, .. } = e.what { Some((vel as u32 - 1) / 8) } else { None }).collect();
    let r = &j.records[0];
    let fwd: Vec<u32> = (0..12).filter(|&s| r.tie[s as usize] == 0).collect();
    assert_eq!(steps[..fwd.len()], fwd[..]);
    assert_eq!(steps[fwd.len()..fwd.len() + 4], [11, 10, 9, 8], "{steps:?}");
    // every on follows an off (mono, one note at a time)
    let mut held = false;
    for e in &ev {
        match e.what {
            PEv::On { .. } => {
                assert!(!held, "{ev:?}");
                held = true;
            }
            PEv::Off => held = false,
            _ => {}
        }
    }
    assert!(!held);
    // a forward loop jumps back to step 8 instead
    let p = path(&ticks(400, R / 2.0), 0, &Loop { kind: 1, ..s }, 2, RATE);
    assert_eq!(p.backward, 0.0);
    let steps: Vec<u32> = unroll(&j, 0, 0, 1.0, &p).iter().filter_map(|e| if let PEv::On { vel, .. } = e.what { Some((vel as u32 - 1) / 8) } else { None }).collect();
    assert_eq!(steps[fwd.len()], 8, "{steps:?}");
}

#[test]
fn the_trigger_end_releases_the_held_note() {
    // a trigger cut after 20 ticks: nothing after the cut, nothing left held, and a note held there ends at the cut
    let j = jizz();
    let p = path(&ticks(20, R / 2.0), 0, &jizz_sample(), 2, RATE);
    assert_eq!(p.end, 20.0 * 730.0);
    let mut cut = 0;
    for record in 0..j.records.len() {
        let ev = unroll(&j, record, 0, 1.0, &p);
        let last = ev.last().unwrap();
        assert!(last.what == PEv::Off && last.at <= p.end, "record {record}: {last:?}");
        cut += (last.at == p.end) as usize;
    }
    assert!(cut > 0);
}

#[test]
fn generated_samples_map_to_records_and_variants() {
    // Jizz by name `Probe<record+1><variant+1>`; Stash by sample order, the drum slot taking `drum_count` samples
    use crate::xm::{Instrument, Sample, Xm};
    let smp = |name: &str| Sample { name: name.into(), loop_start: 0, loop_len: 0, volume: 64, finetune: 0, loop_kind: 0, bits16: false, pan: 128, relative: 0, data: vec![0; 4] };
    let ins = |names: &[&str]| Instrument { name: String::new(), keymap: [0; 96], vol_env: Default::default(), pan_env: Default::default(), vibrato: [0; 4], fadeout: 0, samples: names.iter().map(|n| smp(n)).collect() };
    let xm = |instruments| Xm { name: String::new(), channels: 1, restart: 0, linear: true, speed: 6, bpm: 125, order: vec![], patterns: vec![], instruments };
    let j = jizz();
    let m = identify(&j, &xm(vec![ins(&["Probe111.smp"]), ins(&["Ahh.smp", "Probe14.smp"])])).unwrap();
    assert_eq!(m, [vec![Some((10, 0))], vec![None, Some((0, 3))]]);
    assert!(identify(&j, &xm(vec![ins(&["Probe19.smp"])])).unwrap_err().contains("Probe19"));
    let s = load(&data("patches_stash.json")).unwrap();
    let b = s.call("bluish").unwrap();
    let names: Vec<String> = (0..40).map(|i| format!("J{i}")).collect();
    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let m = identify(b, &xm(vec![ins(&refs[..20]), ins(&refs[20..])])).unwrap();
    let flat: Vec<Option<(usize, usize)>> = m.concat();
    assert_eq!(flat[..4], [Some((0, 0)), Some((0, 1)), Some((0, 2)), Some((0, 3))]);
    assert_eq!(flat[4..10], [None; 6]); // the stored drums
    assert_eq!(flat[10], Some((1, 0)));
}

// ---------- tracks and their controls (task 5.4) ----------

use crate::xm::replay::{Replay, Trigger};

/// A trigger of sample `sample` (instrument 0) on `channel` at `frame`, `n` ticks at `ratio` × the native frequency
/// (Jizz, decimation 2), with volume and pan per tick.
fn trig(channel: usize, sample: usize, frame: u64, n: usize, ratio: impl Fn(usize) -> f64, vol: impl Fn(usize) -> f64) -> Trigger {
    let ticks = (0..n)
        .map(|i| {
            let f = frame + i as u64 * 730;
            TickCtl { at: f as f64 / RATE, frame: f, frames: 730, freq: ratio(i) * R / 2.0, vol: vol(i), pan: 0.5 }
        })
        .collect();
    Trigger { channel, tick: 0, frame, at: frame as f64 / RATE, instrument: 0, sample, note: 62, offset: 0, ticks }
}

/// Samples 0–1: record 0 variants 0–1; sample 2: record 1 variant 0; sample 3: stored.
fn ids() -> Vec<Vec<Option<(usize, usize)>>> {
    vec![vec![Some((0, 0)), Some((0, 1)), Some((1, 0)), None]]
}

fn decim(_: usize) -> u32 {
    2
}

/// A trigger's span: its ticks (the test samples never end early).
fn tspan(r: &Replay, i: usize) -> (f64, f64) {
    let t = &r.triggers[i];
    (t.frame as f64, (t.frame + 730 * t.ticks.len() as u64) as f64)
}

#[test]
fn a_slide_past_reach_hands_over_to_another_track() {
    // a riser over 130 semitones (a semitone a tick, sounding throughout) cannot play from one class (±MAX_SHIFT): it
    // splits at tick boundaries into segments that each fit, so nothing of it is muted
    let t = trig(0, 0, 0, 131, |i| 2f64.powf((i as f64 - 65.0) / 12.0), |_| 1.0);
    let cuts = handovers(&t, 2, f64::MAX);
    assert!(!cuts.is_empty());
    let mut b = vec![0];
    b.extend(&cuts);
    b.push(t.ticks.len());
    for w in b.windows(2) {
        let seg = Trigger { ticks: t.ticks[w[0]..w[1]].to_vec(), ..t.clone() };
        let semis: Vec<f64> = seg.ticks.iter().map(|k| 12.0 * (k.freq * 2.0 / R).log2()).collect();
        let (lo, hi) = semis.iter().fold((f64::MAX, f64::MIN), |(a, b), &x| (a.min(x), b.max(x)));
        assert!(hi - lo <= 2.0 * MAX_SHIFT - 2.0, "segment {w:?} spans {}", hi - lo);
        let c = controls(&Replay { triggers: vec![seg.clone()], length: 1.0 }, &TrackPlan { record: 0, class: class_of(&seg, 2, f64::MAX), triggers: vec![0] }, decim, |_| f64::MAX);
        assert_eq!(c.muted, 0.0, "segment {w:?}: {:?}", c.warnings);
    }
    // a slide within reach stays one trigger
    assert!(handovers(&trig(0, 0, 0, 61, |i| 2f64.powf(i as f64 / 12.0), |_| 1.0), 2, f64::MAX).is_empty());
    // a fall far below hearing: no class under MIN_CLASS (a low class's comb and doubler delays, and so the song's
    // lead, grow as 1/ratio), and no handover for what lies past its reach (left silent)
    let deep = trig(0, 0, 0, 200, |i| 2f64.powf(-(i as f64) / 12.0), |_| 1.0);
    let cuts = handovers(&deep, 2, f64::MAX);
    let last = Trigger { ticks: deep.ticks[*cuts.last().unwrap_or(&0)..].to_vec(), ..deep.clone() };
    assert!(class_of(&last, 2, f64::MAX) >= MIN_CLASS);
    assert!(cuts.iter().all(|&i| 12.0 * (deep.ticks[i].freq * 2.0 / R).log2() >= (MIN_CLASS as f64) - MAX_SHIFT), "{cuts:?}");
}

#[test]
fn a_clipped_path_enters_its_first_piece() {
    // a segment of a path: its pieces cut to the segment's time, positions unchanged, the first an entry (a note at
    // the step under it), the end at the cut
    let p = path(&ticks(150, R / 2.0), 0, &jizz_sample(), 2, RATE);
    let (a, b) = (40.0 * 730.0, 90.0 * 730.0);
    let c = clip(&p, a, b);
    assert!(!c.pieces[0].cont && (c.pieces[0].t0 - a).abs() < 1e-9);
    assert!((c.end - b).abs() < 1e-9);
    // positions: as the whole path's at the same time
    let at = |p: &Path, t: f64| p.pieces.iter().find(|q| q.t0 <= t && t < q.t1).map(|q| q.x0 + (t - q.t0) / (q.t1 - q.t0) * (q.x1 - q.x0));
    for t in [a, a + 1000.0, b - 1.0] {
        assert!((at(&c, t).unwrap() - at(&p, t).unwrap()).abs() < 1e-6);
    }
    // its notes start with an entry at a's step, then the phrase's own from there
    let j = jizz();
    let ev = unroll(&j, 0, 0, 1.0, &c);
    assert!(matches!(ev[0].what, PEv::On { .. }) && (ev[0].at - a).abs() < 1e-6, "{:?}", ev[0]);
    assert!(matches!(ev.last().unwrap().what, PEv::Off) && ev.last().unwrap().at <= b + 1e-6);
}

#[test]
fn one_record_on_two_channels_at_two_ratios_is_two_tracks() {
    // spec "Two channels, two ratios": native on channel 0, an octave up on channel 1, at the same time
    let r = Replay { triggers: vec![trig(0, 0, 0, 50, |_| 1.0, |_| 1.0), trig(1, 1, 0, 50, |_| 2.0, |_| 1.0)], length: 2.0 };
    let t = group(&r, &ids(), decim, |i| tspan(&r, i));
    assert_eq!(t.len(), 2);
    let mut by: Vec<(i32, f64, Vec<usize>)> = t.iter().map(|t| (t.class, t.ratio(), t.triggers.clone())).collect();
    by.sort_by_key(|x| x.0);
    assert_eq!(by, [(0, 1.0, vec![0]), (12, 2.0, vec![1])]);
    // their effects follow the ratio (the echo is half as long an octave up: `times_follow_the_ratio`)
    let j = jizz();
    let chain = |ratio| effects(&j, 0, ratio, RATE, 125).devices;
    assert_ne!(chain(1.0), chain(2.0));
}

#[test]
fn tracks_split_by_record_and_class_and_merge_where_they_never_overlap() {
    // record 0 on channels 0 and 1 one after the other: one track; overlapping: two; record 1 its own; a stored
    // sample none; a quarter tone off native is still class 0
    let after = Replay { triggers: vec![trig(0, 0, 0, 20, |_| 1.0, |_| 1.0), trig(1, 1, 20 * 730, 20, |_| 1.02, |_| 1.0), trig(2, 3, 0, 20, |_| 1.0, |_| 1.0)], length: 1.0 };
    let t = group(&after, &ids(), decim, |i| tspan(&after, i));
    assert_eq!(t.len(), 1);
    assert_eq!((t[0].record, t[0].class, t[0].triggers.clone()), (0, 0, vec![0, 1]));
    let both = Replay { triggers: vec![trig(0, 0, 0, 20, |_| 1.0, |_| 1.0), trig(1, 1, 10 * 730, 20, |_| 1.0, |_| 1.0), trig(2, 2, 0, 20, |_| 1.0, |_| 1.0)], length: 1.0 };
    let t = group(&both, &ids(), decim, |i| tspan(&both, i));
    let mut k: Vec<(usize, Vec<usize>)> = t.iter().map(|t| (t.record, t.triggers.clone())).collect();
    k.sort();
    assert_eq!(k, [(0, vec![0]), (0, vec![1]), (1, vec![2])]);
}

#[test]
fn levels_and_slides_become_control_points() {
    // spec "Tone portamento on a phrase": sliding 5 semitones up over 20 ticks, the bend (semitones from the track's
    // class, the middle of the slide: 2.5 → 3) rises along it; the volume falls; points only where a value changes
    let up = |i: usize| 2f64.powf((i.min(20) as f64 / 4.0) / 12.0);
    let r = Replay { triggers: vec![trig(0, 0, 1000, 40, up, |i| if i < 10 { 1.0 } else { 0.5 })], length: 1.0 };
    let t = group(&r, &ids(), decim, |i| tspan(&r, i));
    let c = controls(&r, &t[0], decim, |_| f64::MAX);
    assert_eq!(c.gain, [(1000, 1.0), (1000 + 10 * 730, 0.5)]);
    assert_eq!(c.pan, [(1000, 0.5)]);
    assert_eq!(c.bend.len(), 21);
    for (i, &(f, b)) in c.bend.iter().enumerate() {
        assert_eq!(f, 1000 + i as u64 * 730);
        assert!((b - (i as f64 / 4.0 - 3.0)).abs() < 1e-9, "{i}: {b}");
    }
    assert!((c.range - 3.0).abs() < 1e-9);
    assert!(c.warnings.is_empty());
    // a trigger counts only while it sounds: cut at tick 10, the volume drop after it is not the track's
    let c = controls(&r, &t[0], decim, |_| 1000.0 + 10.0 * 730.0);
    assert_eq!(c.gain, [(1000, 1.0)]);
}

#[test]
fn the_volume_and_pan_go_after_the_chain_and_stop_with_the_sample() {
    // two triggers on one channel, the first centred at volume 0.5, the second hard right; the first ends (its path)
    // at tick 10, long before the second: the post gain drops to 0 there. The voice plays centred at √2 (the level
    // the phrases were matched at: the chain's clamps see it), and the XM volume and pan act after the chain, as the
    // tracker scales and pans the finished sample: a pan before the comb, echo and doubler smeared one tick's pan into
    // the next (song 1's record 1, its tremor panning hard left and right every tick, played on both sides +4 dB)
    let mut a = trig(0, 0, 0, 20, |_| 1.0, |_| 0.5);
    let mut b = trig(0, 0, 40 * 730, 20, |_| 1.0, |_| 1.0);
    b.ticks.iter_mut().for_each(|k| k.pan = 1.0);
    a.ticks.iter_mut().for_each(|k| k.pan = 0.5);
    let r = Replay { triggers: vec![a, b], length: 1.0 };
    let t = group(&r, &ids(), decim, |i| tspan(&r, i));
    assert_eq!(t.len(), 1);
    let c = controls(&r, &t[0], decim, |i| if i == 0 { 10.0 * 730.0 } else { f64::MAX });
    let near = |a: &[(i64, f64)], b: &[(i64, f64)]| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.0 == y.0 && (x.1 - y.1).abs() < 1e-12);
    let master: Vec<(i64, f64)> = c.master.iter().map(|&(f, x)| (f as i64, x)).collect();
    assert!(near(&master, &[(0, std::f64::consts::SQRT_2)]), "{master:?}");
    let lead = crate::cauldron::LANE_LEAD;
    assert!(near(&c.post, &[(-lead, 0.5), (10 * 730, 0.0), (40 * 730 - lead, 1.0)]), "{:?}", c.post);
    assert!(near(&c.post_pan, &[(-lead, 0.5), (40 * 730 - lead, 1.0)]), "{:?}", c.post_pan);
    // the voice pan (a stored sample's: no chain) still passes through
    assert_eq!(c.pan, [(0, 0.5), (40 * 730, 1.0)]);
}

#[test]
fn a_riser_plays_through_transpose_and_bend() {
    let riser = |i: usize| 2f64.powf(i as f64 / 12.0); // a semitone per tick
    let r = Replay { triggers: vec![trig(3, 0, 0, 61, riser, |_| 1.0)], length: 1.0 };
    let t = group(&r, &ids(), decim, |i| tspan(&r, i));
    // the track runs at the riser's middle (30), at most MAX_CLASS, and shifts −24..+36 from it
    assert_eq!(t[0].class, MAX_CLASS);
    let c = controls(&r, &t[0], decim, |_| f64::MAX);
    assert!(c.warnings.is_empty(), "{:?}", c.warnings);
    assert!((c.bend[0].1 + 24.0).abs() < 1e-9 && (c.bend.last().unwrap().1 - 36.0).abs() < 1e-9);
    assert!(c.gain.iter().all(|g| g.1 > 0.0));
    // a slide wider than reach keeps its top: 0 down to −120 is class −48 (the deep end falls silent), not −60
    let deep = Replay { triggers: vec![trig(3, 0, 0, 121, |i| 2f64.powf(-(i as f64) / 12.0), |_| 1.0)], length: 1.0 };
    assert_eq!(group(&deep, &ids(), decim, |i| tspan(&deep, i))[0].class, -48);
    // a slide down: the middle too (−14.5 → −15)
    let fall = Replay { triggers: vec![trig(3, 0, 0, 30, |i| 2f64.powf(-(i as f64) / 12.0), |_| 1.0)], length: 1.0 };
    assert_eq!(group(&fall, &ids(), decim, |i| tspan(&fall, i))[0].class, -15);
}

#[test]
fn silent_ticks_do_not_move_the_class_or_mute() {
    // a held trigger whose last ticks are silent at 0 Hz (a ping-pong past its end): class 0, nothing muted
    let mut t = trig(3, 0, 0, 30, |_| 1.0, |i| if i < 25 { 1.0 } else { 0.0 });
    t.ticks[25..].iter_mut().for_each(|k| k.freq = 0.0);
    let r = Replay { triggers: vec![t], length: 1.0 };
    let g = group(&r, &ids(), decim, |i| tspan(&r, i));
    assert_eq!(g[0].class, 0);
    let c = controls(&r, &g[0], decim, |_| f64::MAX);
    assert!(c.warnings.is_empty() && c.muted == 0.0, "{:?}", c.warnings);
    assert!(c.bend.iter().all(|b| b.1 == 0.0));
}

#[test]
fn a_riser_past_reach_is_silent_with_a_warning() {
    let riser = |i: usize| 2f64.powf(i as f64 / 12.0);
    let r = Replay { triggers: vec![trig(3, 0, 0, 120, riser, |_| 1.0)], length: 1.0 };
    let t = group(&r, &ids(), decim, |i| tspan(&r, i));
    let c = controls(&r, &t[0], decim, |_| f64::MAX);
    assert_eq!(c.warnings.len(), 1);
    assert!(c.warnings[0].contains("95 semitones") && c.warnings[0].contains("channel 3"), "{:?}", c.warnings);
    // class 24 (capped): ticks 0..=72 sound (−24..+48), 73.. are silent; the shift holds at the edge
    assert_eq!(c.gain.last().unwrap().1, 0.0);
    assert_eq!(c.bend.last().unwrap().1, MAX_SHIFT);
    assert!(c.muted > 0.0);
}

#[test]
fn a_shift_splits_into_transpose_and_bend() {
    use unisynth::law::{host::inv_transpose, transpose};
    let plays = |(t, b): (f64, f64)| t + b;
    // within ±24: bend alone (tracks that never leave it are unchanged)
    assert_eq!(split_shift(17.5, false), (0.0, 17.5));
    assert_eq!(split_shift(-23.9, false), (0.0, -23.9));
    for lane in [false, true] {
        let mut s = -MAX_SHIFT;
        while s <= MAX_SHIFT {
            let (t, b) = split_shift(s, lane);
            assert!(t.abs() <= 24.0 && b.abs() <= MAX_BEND, "{s}: {t} {b}");
            // the transpose is one its automation byte holds (without a lane, 0 is the default's exact 0.5)
            assert!(!lane && t == 0.0 || transpose((inv_transpose(t as f32) * 255.0).round() / 255.0) as f64 == t);
            assert!((plays((t, b)) - s).abs() < 1e-9, "{s}");
            s += 0.37;
        }
    }
}

// ---------- stored samples (task 5.5) ----------

fn stored(loop_kind: u8, relative: i8, finetune: i8) -> crate::xm::Sample {
    let data: Vec<i16> = (0..5000).map(|i| ((i as f64 * 0.05).sin() * 12000.0) as i16).collect();
    crate::xm::Sample { name: "kick".into(), loop_start: 1234, loop_len: 2345, volume: 64, finetune, loop_kind, bits16: true, pan: 128, relative, data }
}

#[test]
fn a_stored_sample_keeps_its_loop() {
    use unisynth::law;
    use unisynth::params::{self as pm, o, shape as sp};
    for (kind, mode) in [(0, 0), (1, 1), (2, 2)] {
        let s = stored(kind, 0, 0);
        let st = sampler(&s, 44100);
        let sh = |k: usize| st.values[pm::osc(0, o::SHAPE + k)];
        assert_eq!(law::choice(sh(sp::SMP_LOOP_MODE), 3), mode);
        assert_eq!(law::choice(sh(sp::SMP_BOUNDARY), 2), 1, "manual bounds");
        // the bounds the sampler computes (`smp.rs`): trunc(len·p)
        let (len, up) = ((s.data.len() * st.up) as f32, st.up as i32);
        assert_eq!(((len * sh(sp::SMP_LOOP_START)) as i32, (len * sh(sp::SMP_LOOP_LENGTH)) as i32), (1234 * up, 2345 * up));
        assert_eq!(st.values[pm::smp_index(0)], 1.0, "GSM sample 1 of the chunk");
        // the GSM sample decodes to the sample's length
        assert_eq!(gsm::GsmChunk::parse(&st.gsm).unwrap().unwrap().samples, s.data.len() * st.up);
    }
}

#[test]
fn a_stored_sample_plays_at_the_xm_frequency() {
    // the sampler steps 2^((semis − 60)/12) samples per output frame: an XM note must step freq/rate
    use unisynth::law;
    use unisynth::params::{self as pm, o};
    for (rel, ft) in [(0, 0), (12, -64), (-7, 100), (30, 127)] {
        let s = stored(0, rel, ft);
        let st = sampler(&s, 44100);
        let v = |k: usize| st.values[pm::osc(0, k)];
        for n in [1u8, 25, 49, 61, 96] {
            let semis = st.note(n) as f64 + 12.0 * law::octave(v(o::OCTAVE)) as f64 + law::fine(v(o::FINE)) as f64 + law::coarse(v(o::COARSE)) as f64;
            let freq = 8363.0 * 2f64.powf((4608.0 - (7680.0 - 64.0 * (n as f64 - 1.0 + rel as f64) - ft as f64 / 2.0)) / 768.0);
            let want = 60.0 + 12.0 * (freq * st.up as f64 / 44100.0).log2();
            assert!((semis - want).abs() < 1e-3, "rel {rel} ft {ft} note {n}: {semis} vs {want}");
        }
    }
}
