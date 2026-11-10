use super::*;
use crate::xm::replay::replay;
use crate::xm::{Cell, Env, Instrument, Pattern, Sample};

/// A test engine (task 3.3): a clocked saw at the given clock, seekable; its notes are the default tone events, so
/// the frames it would render are the positions the events carry.
pub struct CountFrames;

impl Engine for CountFrames {
    fn id(&self) -> &'static str {
        "count-frames"
    }
    fn seekable(&self, _: &Value) -> bool {
        true
    }
    fn instrument(&self, _: &Value, clock: f64) -> [f32; unisynth::params::old::COUNT] {
        use unisynth::params::old::{o, osc_type, shape as sp};
        let mut p = crate::patch::Patch::new();
        p.osc(0, o::TYPE, osc_type::encode(osc_type::CLOCKED));
        p.shape(0, sp::CLK_CLOCK, (clock / 1_048_576.0) as f32);
        p.finish()
    }
}

const C5: u8 = 61;

fn sample(len: usize, loop_start: u32, loop_kind: u8) -> Sample {
    let data = (0..len).map(|i| ((i % 32) as i16 - 16) * 1000).collect();
    let loop_len = if loop_kind == 0 { 0 } else { len as u32 - loop_start };
    Sample { name: format!("s{len}"), loop_start, loop_len, volume: 64, finetune: 0, loop_kind, bits16: true, pan: 128, relative: 0, data }
}

fn ins(s: Sample) -> Instrument {
    Instrument { name: "i".into(), keymap: [0; 96], vol_env: Env::default(), pan_env: Env::default(), vibrato: [0; 4], fadeout: 0, samples: vec![s] }
}

/// One channel, speed 6, BPM 125, `rows` rows.
fn xm(rows: Vec<Cell>, instruments: Vec<Instrument>) -> Xm {
    let n = rows.len() as u16;
    Xm { name: "t".into(), channels: 1, restart: 0, linear: true, speed: 6, bpm: 125, order: vec![0], patterns: vec![Pattern { rows: n, cells: rows }], instruments, mix: Default::default() }
}

fn generated(instrument: usize, length: u32) -> Entry {
    Entry { instrument, sample: 0, generated: Some(Generated { engine: "count-frames".into(), length, params: serde_json::json!({}) }) }
}

fn stored(instrument: usize) -> Entry {
    Entry { instrument, sample: 0, generated: None }
}

#[test]
fn the_map_loads_and_is_told_from_patch_data() {
    let text = r#"{"format": "xm-gen/1", "intro": "t", "samples": [
        {"instrument": 1, "sample": 0, "kind": "generated", "engine": "count-frames", "length": 4096, "params": {"flag": 1, "scale": 0.0125}},
        {"instrument": 2, "sample": 0, "kind": "stored"}]}"#;
    assert!(is_map(text) && !is_map(r#"{"records": [], "order": []}"#));
    // any `format` goes to the map loader, which names what it does not read
    assert!(is_map(r#"{"format": "xm-gen/2"}"#) && load(r#"{"format": "xm-gen/2"}"#).unwrap_err().contains("xm-gen/1"));
    let m = load(text).unwrap();
    assert_eq!(m.samples[0], Entry { instrument: 1, sample: 0, generated: Some(Generated { engine: "count-frames".into(), length: 4096, params: serde_json::json!({"flag": 1, "scale": 0.0125}) }) });
    assert_eq!(m.samples[1], stored(2));
    // an integer flag stays an integer
    assert!(m.samples[0].generated.as_ref().unwrap().params["flag"].is_u64());
    assert!(load(r#"{"format": "xm-gen/1", "samples": [{"instrument": 1, "sample": 0, "kind": "made"}]}"#).unwrap_err().contains("kind"));
}

#[test]
fn map_covers_the_xm() {
    let x = xm(vec![Cell::default()], vec![ins(sample(1024, 0, 1)), ins(sample(256, 0, 0))]);
    let m = |s: Vec<Entry>| Map { intro: "t".into(), samples: s };
    assert_eq!(check(&m(vec![generated(1, 1024), stored(2)]), &x), Ok(()));
    let e = check(&m(vec![generated(1, 1024)]), &x).unwrap_err();
    assert!(e.contains("instrument 2 sample 0") && e.contains("not in the map"), "{e}");
    let e = check(&m(vec![generated(1, 1024), stored(2), stored(3)]), &x).unwrap_err();
    assert!(e.contains("instrument 3 sample 0") && e.contains("not in the XM"), "{e}");
    let e = check(&m(vec![generated(1, 1024), stored(2), stored(2)]), &x).unwrap_err();
    assert!(e.contains("twice"), "{e}");
}

#[test]
fn engine_missing() {
    let x = xm(vec![Cell::default()], vec![ins(sample(1024, 0, 1))]);
    let mut e = generated(1, 1024);
    e.generated.as_mut().unwrap().engine = "nope".into();
    let err = check(&Map { intro: "t".into(), samples: vec![e] }, &x).unwrap_err();
    assert!(err.contains("engine `nope`") && err.contains("instrument 1 sample 0"), "{err}");
}

#[test]
fn position_bytes_follow_the_contract() {
    // spec: 9xx = 0x13 is frame 4864 = 1·4096 + 48·16
    assert_eq!(position_bytes(4864.0), Ok((1, 48)));
    assert_eq!(position_bytes(4095.0), Ok((1, 0)), "rounded to 16 frames, carried");
    assert_eq!(position_bytes(255.0 * 4096.0 + 255.0 * 16.0), Ok((255, 255)));
    assert!(position_bytes(1_100_000.0).is_err());
}

/// The trigger's tone events.
fn events_of(x: &Xm) -> (crate::xm::replay::Replay, Vec<PhraseEv>, Path) {
    let r = replay(x, 44100).unwrap();
    let t = &r.triggers[0];
    let s = &x.instruments[0].samples[0];
    let lp = crate::cauldron::import::Loop { len: s.data.len() as u32, start: s.loop_start, length: s.loop_len, kind: s.loop_kind };
    let p = crate::cauldron::import::path(&t.ticks, t.offset, &lp, 1, 44100.0);
    let ev = tone_events(&p, true);
    (r, ev, p)
}

#[test]
fn offset_and_loop() {
    // 9x02 starts at frame 512; the forward loop 256..1024 re-enters at 256 each time the playback passes 1024
    let x = xm(vec![Cell { note: C5, instrument: 1, volume: 0, effect: 9, param: 2 }, Cell::default(), Cell::default()], vec![ins(sample(1024, 256, 1))]);
    let (_, ev, p) = events_of(&x);
    let ons: Vec<(f64, (u8, u8, u8))> = ev.iter().filter_map(|e| if let PEv::Gen { hi, lo, back } = e.what { Some((e.at, (hi, lo, back))) } else { None }).collect();
    assert_eq!(ons[0], (0.0, (0, 32, 0)), "frame 512");
    assert!(ons.len() >= 3 && ons[1..].iter().all(|o| o.1 == (0, 16, 0)), "{ons:?}");
    // C-5 plays 2·8363 frames a second: 512 frames to the loop end, then 768 per pass
    let v = 2.0 * 8363.0 / 44100.0;
    assert!((ons[1].0 - 512.0 / v).abs() < 1.0 && (ons[2].0 - ons[1].0 - 768.0 / v).abs() < 1.0, "{ons:?}");
    // an off before every re-entry, and one at the end
    assert_eq!(ev.iter().filter(|e| e.what == PEv::Off).count(), ons.len());
    assert_eq!(ev.last().unwrap().at, p.end);
}

#[test]
fn a_non_looped_sample_ends_its_note() {
    let x = xm(vec![Cell { note: C5, instrument: 1, ..Cell::default() }, Cell::default(), Cell::default()], vec![ins(sample(256, 0, 0))]);
    let (_, ev, p) = events_of(&x);
    let v = 2.0 * 8363.0 / 44100.0;
    assert_eq!(ev.len(), 2);
    assert!((p.end - 256.0 / v).abs() < 1.0 && ev[1] == PhraseEv { at: p.end, what: PEv::Off }, "{ev:?}");
}

#[test]
fn engine_neutral_path() {
    // a slide up (1xx) through a ping-pong loop: the position the events imply at every tick equals the replay's
    // resampled sample position, within a frame
    let rows = vec![Cell { note: C5, instrument: 1, volume: 0, effect: 1, param: 8 }, Cell { effect: 1, param: 8, ..Cell::default() }, Cell { effect: 1, param: 8, ..Cell::default() }];
    let x = xm(rows, vec![ins(sample(2048, 512, 2))]);
    let (r, ev, _) = events_of(&x);
    // the replay's position, integrated per tick through the ping-pong loop
    let (mut pos, mut back) = (0.0f64, false);
    let mut want = vec![];
    for k in &r.triggers[0].ticks {
        want.push((k.frame as f64, pos));
        let mut left = k.freq / 44100.0 * k.frames as f64;
        while left > 0.0 {
            let bound = if back { 512.0 } else { 2048.0 };
            let d = (bound - pos).abs().min(left);
            pos += if back { -d } else { d };
            left -= d;
            if (pos - bound).abs() < 1e-9 && left > 0.0 {
                back = !back;
            }
        }
    }
    // the events' position at frame t: the last note-on's start, then its rate (from the path's segments)
    let p = {
        let t = &r.triggers[0];
        let lp = crate::cauldron::import::Loop { len: 2048, start: 512, length: 1536, kind: 2 };
        crate::cauldron::import::path(&t.ticks, 0, &lp, 1, 44100.0)
    };
    assert!(ev.iter().any(|e| matches!(e.what, PEv::Gen { back: 255, .. })), "a backward pass");
    for (t, x) in want {
        let g = p.segs.iter().find(|g| g.t0 <= t && t < g.t1).unwrap();
        let on = ev.iter().rev().find(|e| e.at <= t && matches!(e.what, PEv::Gen { .. })).unwrap();
        let PEv::Gen { hi, lo, back } = on.what else { unreachable!() };
        let start = hi as f64 * 4096.0 + lo as f64 * 16.0;
        // the engine renders from the start at the segment's rate (frames per output frame) in its direction
        let mut implied = start;
        for s in p.segs.iter().filter(|s| s.t0 >= on.at && s.t1 <= t) {
            implied += s.x1 - s.x0;
        }
        implied += (t - g.t0.max(on.at)) / (g.t1 - g.t0) * (g.x1 - g.x0);
        assert!((implied - x).abs() <= 16.0 / 2.0 + 1.0, "tick at {t}: events imply {implied}, replay {x} (back {back})");
    }
}

#[test]
fn stored_and_generated_in_one_song() {
    let x = xm(
        vec![Cell { note: C5, instrument: 1, ..Cell::default() }, Cell::default(), Cell { note: C5, instrument: 2, ..Cell::default() }, Cell::default()],
        vec![ins(sample(1024, 0, 1)), ins(sample(256, 0, 0))],
    );
    let map = Map { intro: "t".into(), samples: vec![generated(1, 1024), stored(2)] };
    let c = crate::xm::convert::convert(&GenSource::new(&map, &x).unwrap(), &x, 44100, crate::cauldron::import::Options::default()).unwrap();
    assert_eq!(c.tracks, (1, 1));
    // the song plays, and its only sample data is instrument 2's
    let blob = c.song.write().unwrap();
    assert!(synth_core::mix::Song::parse(&blob).is_ok());
    let song = crate::model::SongM::read(&blob).unwrap();
    let samples: usize = song.devices.iter().filter(|d| d.kind == unisynth::kind::INSTRUMENT).map(|d| unisynth::chunk::Chunk::parse(&d.chunk).unwrap().sample_count()).sum();
    assert_eq!(samples, 1);
}

/// Spec "Engine tracks in the v3 layout" (add-fudge-softsynth, design D5): a Fudge drum's track is a v3 instrument
/// whose slot 1 is the Sweep drum, its byte lanes address slot 1's v3 fields, and it plays; a forward loop's
/// re-entries and an offset write the start bytes.
#[test]
fn an_engine_track_takes_its_type_after_the_migration() {
    use unisynth::params::{o, osc, osc_type};
    let record = serde_json::json!({"frames": 256, "args": [1123942400u32, 1036831949u32, 966609234u32, 3076135571u32, 0, 1056964608u32, 1065353216u32, 1065353216u32, 0], "carry": 1185420165u32});
    let x = xm(
        vec![Cell { note: C5, instrument: 1, ..Cell::default() }, Cell::default(), Cell { note: C5, instrument: 1, effect: 9, param: 1, ..Cell::default() }, Cell::default()],
        vec![ins(sample(512, 0, 1))],
    );
    let map = Map { intro: "t".into(), samples: vec![Entry { instrument: 1, sample: 0, generated: Some(Generated { engine: "sweep-drum".into(), length: 512, params: record }) }] };
    let c = crate::xm::convert::convert(&GenSource::new(&map, &x).unwrap(), &x, 44100, crate::cauldron::import::Options::default()).unwrap();
    let blob = c.song.write().unwrap();
    assert!(synth_core::mix::Song::parse(&blob).is_ok());
    let song = crate::model::SongM::read(&blob).unwrap();
    let t = &song.tracks[0];
    let d = &song.devices[t.devices[0]];
    let v = unisynth::chunk::Chunk::parse(&d.chunk).unwrap().values();
    assert_eq!(osc_type::decode(v[osc(0, o::TYPE)]), osc_type::SWEEP_DRUM);
    assert_eq!(v[osc(0, o::SHAPE + unisynth::params::shape::DRUM_SHAPE)], 0.5, "the stand-in's shape values come through");
    let params: Vec<usize> = t.autos.iter().filter(|a| a.device == 0).map(|a| a.param).collect();
    assert!(params.contains(&osc(0, o::PM_OUT)), "the offset's start byte: {params:?}");
    // it loads and plays in a unisynth build with the engine types
    let out = song::render::render(&blob, None).unwrap();
    assert!(out.iter().any(|s| s.abs() > 0.01), "the drum sounds");
}

#[test]
fn the_byte_contract_as_lanes() {
    use unisynth::params::old::{self as pm, o};
    // spec "Position bytes": a note at frame 4864, then a backward pass from 0
    let evs = vec![(1000, PEv::Gen { hi: 1, lo: 48, back: 0 }), (5000, PEv::Off), (6000, PEv::Gen { hi: 0, lo: 0, back: 255 }), (9000, PEv::Off)];
    let mut values = unisynth::params::old::DEFAULTS;
    let (lane, autos) = note_lanes(evs.into_iter(), &mut values).unwrap();
    assert_eq!(lane, vec![(1000, NOTE, Some(127)), (4000, NOTE, None), (1000, NOTE, Some(127)), (3000, NOTE, None)]);
    // the value a lane holds at song frame f
    let at = |param: usize, f: i64| -> u8 {
        let a = autos.iter().find(|a| a.param == pm::osc(0, param)).unwrap_or_else(|| panic!("no lane for {param}"));
        let (mut t, mut v) = (0i64, a.points[0].1);
        for &(d, b) in &a.points {
            if t + d as i64 > f {
                break;
            }
            t += d as i64;
            v = b;
        }
        v
    };
    for (f, want) in [(1000, (1, 48, 0)), (6000, (0, 0, 255))] {
        // held from a lane lead before the note-on through it
        for g in [f - LANE_LEAD, f] {
            assert_eq!((at(o::TRACKING, g), at(o::PM_OUT, g), at(o::FEEDBACK, g)), want, "at {g}");
        }
    }
    assert_eq!(values[pm::osc(0, o::TRACKING)], 1.0 / 255.0, "the first note's bytes are the instrument's values");
}

#[test]
fn track_names_for_unnamed_samples() {
    use crate::xm::convert::Source;
    let mut x = xm(vec![Cell::default()], vec![ins(sample(1024, 0, 1)), ins(sample(256, 0, 0)), ins(sample(256, 0, 0))]);
    x.instruments[0].samples[0].name = "Bass".into();
    x.instruments[1].samples[0].name = " ".into();
    x.instruments[1].name = "Lead".into();
    x.instruments[2].samples[0].name.clear();
    x.instruments[2].name.clear();
    let map = Map { intro: "t".into(), samples: vec![generated(1, 1024), generated(2, 256), stored(3)] };
    let s = GenSource::new(&map, &x).unwrap();
    let n = |key, class, i: usize| s.track_name(key, class, &x.instruments[i], i, 0);
    assert_eq!([n(Some(0), 0, 0), n(Some(1), -12, 1), n(None, 0, 2)], ["Bass".to_string(), "Lead (-12)".into(), "Instrument 3 (sample)".into()]);
    assert_eq!(crate::cauldron::import::number_names(vec!["Lead".into(), "Bass".into(), "Lead".into()]), ["Lead 1", "Bass", "Lead 2"]);
}

#[test]
fn the_map_is_strict() {
    // a generated record without params, an instrument 0
    let e = load(r#"{"format": "xm-gen/1", "samples": [{"instrument": 1, "sample": 0, "kind": "generated", "engine": "count-frames", "length": 1}]}"#).unwrap_err();
    assert!(e.contains("entry 0") && e.contains("params"), "{e}");
    assert!(load(r#"{"format": "xm-gen/1", "samples": [{"instrument": 0, "sample": 0, "kind": "stored"}]}"#).unwrap_err().contains("from 1"));
    // a length that is not the XM's; an empty sample listed
    let mut x = xm(vec![Cell::default()], vec![ins(sample(1024, 0, 1)), ins(sample(256, 0, 0))]);
    let m = |s: Vec<Entry>| Map { intro: "t".into(), samples: s };
    let e = check(&m(vec![generated(1, 1000), stored(2)]), &x).unwrap_err();
    assert!(e.contains("instrument 1 sample 0") && e.contains("length 1000") && e.contains("1024"), "{e}");
    x.instruments[1].samples[0].data.clear();
    assert!(check(&m(vec![generated(1, 1024), stored(2)]), &x).unwrap_err().contains("empty"));
}

#[test]
fn identical_records_share_an_instrument() {
    use crate::xm::convert::Source;
    let x = xm(vec![Cell::default()], vec![ins(sample(1024, 0, 1)), ins(sample(1024, 0, 1)), ins(sample(1024, 0, 1))]);
    let mut other = generated(3, 1024);
    other.generated.as_mut().unwrap().params = serde_json::json!({"k": 1});
    let map = Map { intro: "t".into(), samples: vec![generated(1, 1024), generated(2, 1024), other] };
    let ids = GenSource::new(&map, &x).unwrap().ids(&x).unwrap();
    assert_eq!(ids, vec![vec![Some((0, 0))], vec![Some((0, 0))], vec![Some((1, 0))]]);
}

#[test]
fn notes_too_close_for_their_bytes() {
    let mut values = unisynth::params::old::DEFAULTS;
    // different bytes LANE_LEAD apart: the second note's step would land on the first note-on, so the first is dropped
    let evs = vec![(1000, PEv::Gen { hi: 0, lo: 1, back: 0 }), (1100, PEv::Off), (1000 + LANE_LEAD, PEv::Gen { hi: 0, lo: 2, back: 0 }), (2000, PEv::Off)];
    let (lane, _) = note_lanes(evs.into_iter(), &mut values).unwrap();
    assert_eq!(lane, vec![(1128, NOTE, Some(127)), (872, NOTE, None)]);
    // a re-entry (off and on at one frame) 59 frames before a cut: the running note plays on to the cut
    let evs = vec![
        (1000, PEv::Gen { hi: 0, lo: 0, back: 0 }),
        (1255, PEv::Off),
        (1255, PEv::Gen { hi: 0, lo: 16, back: 255 }),
        (1314, PEv::Off),
        (1314, PEv::Gen { hi: 1, lo: 0, back: 0 }),
        (2000, PEv::Off),
    ];
    let (lane, autos) = note_lanes(evs.into_iter(), &mut values).unwrap();
    assert_eq!(lane, vec![(1000, NOTE, Some(127)), (314, NOTE, None), (0, NOTE, Some(127)), (686, NOTE, None)]);
    let hi = autos.iter().find(|a| a.param == unisynth::params::old::osc(0, unisynth::params::old::o::TRACKING)).expect("a start-byte lane");
    assert_eq!(hi.points.last().unwrap().1, 1);
    // the same bytes (a forward loop's re-entries) may follow as closely as they come
    let evs = vec![(1000, PEv::Gen { hi: 0, lo: 1, back: 0 }), (1050, PEv::Off), (1050, PEv::Gen { hi: 0, lo: 1, back: 0 }), (2000, PEv::Off)];
    let (lane, _) = note_lanes(evs.into_iter(), &mut values).unwrap();
    assert_eq!(lane.len(), 4);
}

/// The committed maps (`tools/xm-gen/maps`, task 5.1): what each intro stored; against its XM when `$ZENEKK_DIR`
/// holds them (every check passes but the engines, which arrive with their changes).
#[test]
fn the_committed_maps() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/xm-gen/maps");
    let zenekk = std::env::var("ZENEKK_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Downloads/zenekk"));
    for (name, xm, stored, total) in [
        ("fudge-asc", "Fudge - Alien Sex Clone.xm", vec![7, 14, 15], 25),
        ("quad-theta", "Quad - Theta.xm", vec![1, 2, 3, 18], 27),
        ("quad-pure", "Quad - Pure.xm", vec![33, 34, 35], 34),
    ] {
        let m = load(&std::fs::read_to_string(dir.join(format!("{name}.json"))).unwrap()).unwrap();
        assert_eq!(m.intro, name);
        assert_eq!(m.samples.len(), total, "{name}");
        assert_eq!(m.samples.iter().filter(|e| e.generated.is_none()).map(|e| e.instrument).collect::<Vec<_>>(), stored, "{name}");
        let Ok(bytes) = std::fs::read(zenekk.join(xm)) else {
            eprintln!("skip: {xm} absent");
            continue;
        };
        // Fudge's engines are built (change add-fudge-softsynth); poespack's are not yet
        match (name, check(&m, &crate::xm::parse(&bytes).unwrap())) {
            ("fudge-asc", r) => r.unwrap(),
            (_, r) => assert!(r.unwrap_err().starts_with("engine `"), "{name}"),
        }
    }
}

#[test]
fn a_non_seekable_backward_run_starts_at_its_lowest_frame() {
    // a ping-pong loop 512..2048 held for rows: each backward pass spans several ticks (one segment each)
    let x = xm(vec![Cell { note: C5, instrument: 1, ..Cell::default() }, Cell::default(), Cell::default(), Cell::default(), Cell::default()], vec![ins(sample(2048, 512, 2))]);
    let (_, _, p) = events_of(&x);
    let ev = tone_events(&p, false);
    let back_runs: Vec<&Seg> = p.segs.iter().filter(|g| g.x1 < g.x0).collect();
    assert!(back_runs.len() > 2, "a backward run over several ticks");
    // every note entering a backward run starts at the loop start (the run's lowest frame), forwards
    let entries: Vec<(u8, u8, u8)> = ev
        .iter()
        .filter_map(|e| if let PEv::Gen { hi, lo, back } = e.what { Some((e.at, (hi, lo, back))) } else { None })
        .filter(|(at, _)| p.segs.iter().any(|g| (g.t0 - at).abs() < 1e-6 && g.x1 < g.x0))
        .map(|x| x.1)
        .collect();
    assert!(!entries.is_empty() && entries.iter().all(|&b| b == (0, 32, 0)), "{entries:?}");
}

