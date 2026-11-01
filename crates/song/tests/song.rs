//! song/1 scenarios (change add-synth-daw, spec song-source): round trip, linked placements, tick → sample,
//! same-sample ordering, overlapping same-pitch notes, loop flattening, validation paths, export determinism,
//! buses, mute, import round trip, legacy kinds, presets, parameter tables.

use song::{export, import, parse, preset, tables, to_json, validate, Song};
use synth_core::device::EventKind;
use synth_core::mix::Song as Blob;

/// A 120 BPM, 44.1 kHz song with one instrument track `lead` (kind 64) and a master, plus `extra` merged into the
/// root object (clips, more tracks) by the caller through `edit`.
fn base() -> serde_json::Value {
    serde_json::json!({
        "format": "song/1", "rate": 44100, "tempo": 120.0, "signature": [4, 4], "ticks_per_beat": 960,
        "length": 15360,
        "clips": [],
        "tracks": [
            { "id": "lead", "name": "Lead", "volume": 1.0, "mute": false, "receives": [],
              "devices": [ { "kind": 64, "values": { "osc1.type": 0.25 } } ], "placements": [], "lanes": [] }
        ],
        "master": { "id": "master", "name": "Master", "volume": 1.0, "mute": false, "receives": [
            { "from": "lead", "channel": "main", "volume": 1.0 } ], "devices": [], "placements": [], "lanes": [] }
    })
}

fn song_of(v: &serde_json::Value) -> Song {
    parse(&v.to_string()).unwrap_or_else(|e| panic!("parse: {e:?}"))
}

fn exported(v: &serde_json::Value) -> Vec<u8> {
    export(&song_of(v)).unwrap_or_else(|e| panic!("export: {e:?}"))
}

/// Events of blob lane `lane` as (absolute sample, kind).
fn events(blob: &[u8], lane: usize) -> Vec<(i64, EventKind)> {
    let s = Blob::parse(blob).unwrap();
    let mut at = 0i64;
    s.lane(lane).unwrap().events().map(|(d, k)| { at += d as i64; (at, k) }).collect()
}

fn on(note: u8, velocity: u8) -> EventKind {
    EventKind::NoteOn { note, velocity }
}
fn off(note: u8) -> EventKind {
    EventKind::NoteOff { note }
}

fn issues(v: &serde_json::Value) -> Vec<(String, String)> {
    match parse(&v.to_string()) {
        Err(e) => e.into_iter().map(|i| (i.path, i.message)).collect(),
        Ok(s) => validate(&s).into_iter().map(|i| (i.path, i.message)).collect(),
    }
}

fn has(v: &serde_json::Value, path: &str, msg: &str) {
    let got = issues(v);
    assert!(got.iter().any(|(p, m)| p == path && m.contains(msg)), "expected {path}: {msg}; got {got:?}");
}

#[test]
fn minimal_song_exports_two_tracks_without_events() {
    let b = exported(&base());
    let s = Blob::parse(&b).unwrap();
    assert_eq!((s.tempo, s.rate), (120, 44100));
    assert_eq!(s.track_count(), 2);
    assert_eq!(s.device_count(), 1);
    assert!(s.lanes().all(|l| l.is_empty()));
    let t: Vec<_> = s.tracks().collect();
    assert_eq!(t[1].receives().map(|r| (r.track, r.channel)).collect::<Vec<_>>(), [(0, 0)], "master receives lead");
    // song length 15360 ticks = 4 bars at 120 BPM = 8 s
    assert_eq!((s.length * s.rate as f64) as u64, 8 * 44100);
}

#[test]
fn save_round_trip_is_the_same_json_value() {
    let mut v = base();
    v["clips"] = serde_json::json!([{ "id": "a", "length": 3840, "notes": [[0, 480, 60, 100], [960, 240, 64, 90]] }]);
    v["tracks"][0]["placements"] = serde_json::json!([{ "clip": "a", "start": 0, "length": 3840, "loop": false }]);
    v["tracks"][0]["lanes"] = serde_json::json!([{ "device": 0, "param": "filt1.cutoff", "points": [[0, 0.25], [3840, 0.75]] }]);
    let s = song_of(&v);
    let again: serde_json::Value = serde_json::from_str(&to_json(&s)).unwrap();
    assert_eq!(again, v);
    assert_eq!(song_of(&again), s);
}

#[test]
fn tick_960_is_sample_22050_at_120_bpm() {
    let mut v = base();
    v["clips"] = serde_json::json!([{ "id": "a", "length": 3840, "notes": [[960, 480, 60, 100]] }]);
    v["tracks"][0]["placements"] = serde_json::json!([{ "clip": "a", "start": 0, "length": 3840, "loop": false }]);
    assert_eq!(events(&exported(&v), 0), [(22050, on(60, 100)), (33075, off(60))]);
}

#[test]
fn linked_placements_share_the_clip_notes() {
    let mut v = base();
    v["clips"] = serde_json::json!([{ "id": "bass-a", "length": 3840, "notes": [[0, 480, 36, 100]] }]);
    v["tracks"][0]["placements"] = serde_json::json!([
        { "clip": "bass-a", "start": 0, "length": 3840, "loop": false },
        { "clip": "bass-a", "start": 7680, "length": 3840, "loop": false } ]);
    let before = events(&exported(&v), 0);
    assert_eq!(before.iter().filter(|e| matches!(e.1, EventKind::NoteOn { note: 36, .. })).count(), 2);
    v["clips"][0]["notes"][0][2] = serde_json::json!(48); // move the note up an octave
    let after = events(&exported(&v), 0);
    assert_eq!(after.iter().filter(|e| matches!(e.1, EventKind::NoteOn { note: 48, .. })).count(), 2, "{after:?}");
}

#[test]
fn same_sample_note_offs_come_first_then_note_ons_in_pitch_order() {
    let mut v = base();
    v["clips"] = serde_json::json!([{ "id": "a", "length": 3840,
        "notes": [[0, 960, 67, 100], [0, 960, 60, 100], [960, 960, 64, 90], [960, 960, 62, 80]] }]);
    v["tracks"][0]["placements"] = serde_json::json!([{ "clip": "a", "start": 0, "length": 3840, "loop": false }]);
    let e = events(&exported(&v), 0);
    let at_22050: Vec<_> = e.iter().filter(|x| x.0 == 22050).map(|x| x.1).collect();
    assert_eq!(at_22050, [off(60), off(67), on(62, 80), on(64, 90)]);
    assert_eq!(e[0..2], [(0, on(60, 100)), (0, on(67, 100))]);
}

#[test]
fn overlapping_same_pitch_notes_never_leave_two_open() {
    let mut v = base();
    v["clips"] = serde_json::json!([{ "id": "a", "length": 1920, "notes": [[0, 1920, 60, 100]] }]);
    // second placement starts in the middle of the first one's note
    v["tracks"][0]["placements"] = serde_json::json!([
        { "clip": "a", "start": 0, "length": 1920, "loop": false },
        { "clip": "a", "start": 960, "length": 1920, "loop": false } ]);
    let e = events(&exported(&v), 0);
    assert_eq!(e, [(0, on(60, 100)), (22050, off(60)), (22050, on(60, 100)), (66150, off(60))]);
    let mut open = 0;
    for (_, k) in &e {
        open += if matches!(k, EventKind::NoteOn { .. }) { 1 } else { -1 };
        assert!((0..=1).contains(&open));
    }
}

#[test]
fn a_looping_placement_repeats_and_cuts_at_its_end() {
    // 1-bar clip (3840 ticks) placed for 4320 ticks with loop on: the second pass starts at 3840 and its first note
    // (960 ticks long) is cut at the placement end, 4320
    let mut v = base();
    v["clips"] = serde_json::json!([{ "id": "a", "length": 3840, "notes": [[0, 960, 60, 100], [1920, 1920, 64, 100]] }]);
    v["tracks"][0]["placements"] = serde_json::json!([{ "clip": "a", "start": 0, "length": 4320, "loop": true }]);
    let t = |tick: i64| (tick as f64 * 44100.0 * 60.0 / (120.0 * 960.0)).round() as i64;
    let e = events(&exported(&v), 0);
    assert_eq!(e, [(t(0), on(60, 100)), (t(960), off(60)), (t(1920), on(64, 100)), (t(3840), off(64)), (t(3840), on(60, 100)), (t(4320), off(60))]);
    // a non-looping placement of the same length plays the clip once
    v["tracks"][0]["placements"][0]["loop"] = serde_json::json!(false);
    let e = events(&exported(&v), 0);
    assert_eq!(e, [(t(0), on(60, 100)), (t(960), off(60)), (t(1920), on(64, 100)), (t(3840), off(64))]);
}

#[test]
fn export_is_deterministic() {
    let mut v = base();
    v["clips"] = serde_json::json!([{ "id": "a", "length": 3840, "notes": [[0, 480, 60, 100], [480, 480, 62, 90]] }]);
    v["tracks"][0]["placements"] = serde_json::json!([{ "clip": "a", "start": 0, "length": 3840, "loop": false }]);
    v["tracks"][0]["lanes"] = serde_json::json!([{ "device": 0, "param": "filt1.cutoff", "points": [[0, 0.1], [960.5, 0.9]] }]);
    assert_eq!(exported(&v), exported(&v));
}

#[test]
fn muted_track_exports_volume_zero_and_no_events() {
    let mut v = base();
    v["clips"] = serde_json::json!([{ "id": "a", "length": 3840, "notes": [[0, 480, 60, 100]] }]);
    v["tracks"][0]["placements"] = serde_json::json!([{ "clip": "a", "start": 0, "length": 3840, "loop": false }]);
    v["tracks"][0]["mute"] = serde_json::json!(true);
    let b = exported(&v);
    let s = Blob::parse(&b).unwrap();
    assert_eq!(s.tracks().next().unwrap().volume, 0.0);
    assert!(events(&b, 0).is_empty());
}

#[test]
fn effect_only_bus_with_two_receives() {
    let mut v = base();
    let lead2 = { let mut t = v["tracks"][0].clone(); t["id"] = "lead2".into(); t["name"] = "Lead 2".into(); t };
    v["tracks"].as_array_mut().unwrap().push(lead2);
    v["tracks"].as_array_mut().unwrap().push(serde_json::json!({
        "id": "verb", "name": "Verb", "volume": 1.0, "mute": false,
        "receives": [ { "from": "lead", "channel": "main", "volume": 0.5 }, { "from": "lead2", "channel": "main", "volume": 0.25 } ],
        "devices": [ { "kind": 71, "values": { "mode": 0.0, "room": 0.7 } } ], "placements": [], "lanes": [] }));
    v["master"]["receives"] = serde_json::json!([{ "from": "verb", "channel": "main", "volume": 1.0 }]);
    let b = exported(&v);
    let s = Blob::parse(&b).unwrap();
    let verb = s.tracks().nth(2).unwrap();
    assert_eq!(verb.device_count(), 1);
    assert_eq!(verb.receives().map(|r| (r.track, r.volume)).collect::<Vec<_>>(), [(0, 0.5), (1, 0.25)]);
    let d = s.devices().nth(verb.devices().next().unwrap()).unwrap();
    assert_eq!(d.kind, 71);
}

#[test]
fn sidechain_receive_exports_on_channel_2() {
    let mut v = base();
    v["tracks"].as_array_mut().unwrap().push(serde_json::json!({
        "id": "pump", "name": "Pump", "volume": 1.0, "mute": false,
        "receives": [ { "from": "lead", "channel": "side", "volume": 1.0 } ],
        "devices": [ { "kind": 72, "values": { "mode": 0.0, "sidechain": 1.0 } } ], "placements": [], "lanes": [] }));
    v["master"]["receives"] = serde_json::json!([{ "from": "pump", "channel": "main", "volume": 1.0 }]);
    let s = Blob::parse(&exported(&v)).unwrap().tracks().nth(1).unwrap().receives().collect::<Vec<_>>();
    assert_eq!((s[0].track, s[0].channel), (0, 2));
}

#[test]
fn automation_converts_ticks_and_bytes() {
    let mut v = base();
    v["tracks"][0]["lanes"] = serde_json::json!([{ "device": 0, "param": "filt1.cutoff", "points": [[0, 0.0], [960, 0.5], [1920, 1.0]] }]);
    let b = exported(&v);
    let s = Blob::parse(&b).unwrap();
    let t = s.tracks().next().unwrap();
    let a = t.automations().next().unwrap();
    assert_eq!((a.device, a.param), (0, unisynth::params::FILT + unisynth::params::f::CUTOFF));
    let pts: Vec<_> = a.points().collect();
    assert_eq!(pts, [(0, 0.0), (22050, 128.0 / 255.0), (44100, 1.0)]);
}

// ----- validation -----

#[test]
fn unknown_parameter_has_its_path() {
    let mut v = base();
    v["tracks"][0]["devices"][0]["values"]["osc1.cutof"] = serde_json::json!(0.5);
    has(&v, "/tracks/0/devices/0/values/osc1.cutof", "unknown parameter");
}

#[test]
fn a_bad_value_type_points_at_the_dotted_parameter_name() {
    let mut v = base();
    v["tracks"][0]["devices"][0]["values"]["filt1.cutoff"] = serde_json::json!("high");
    has(&v, "/tracks/0/devices/0/values/filt1.cutoff", "invalid type");
}

#[test]
fn duplicate_automation_lane_fails_at_the_second_lane() {
    let mut v = base();
    v["tracks"][0]["lanes"] = serde_json::json!([
        { "device": 0, "param": "filt1.cutoff", "points": [[0, 0.5]] },
        { "device": 0, "param": "filt1.cutoff", "points": [[0, 0.2]] } ]);
    has(&v, "/tracks/0/lanes/1", "parameter already automated on this track");
}

#[test]
fn receive_from_master_is_rejected() {
    let mut v = base();
    v["tracks"][0]["receives"] = serde_json::json!([{ "from": "master", "channel": "main", "volume": 1.0 }]);
    has(&v, "/tracks/0/receives/0/from", "the master cannot send; route through a bus before the master");
}

#[test]
fn validation_reports_every_problem_with_a_path() {
    let mut v = base();
    v["tracks"][0]["devices"].as_array_mut().unwrap().push(serde_json::json!({ "kind": 9, "values": {} }));
    v["tracks"][0]["devices"][0]["values"]["filt1.cutoff"] = serde_json::json!(1.5);
    v["tracks"][0]["placements"] = serde_json::json!([{ "clip": "nope", "start": 0, "length": 960, "loop": false }]);
    v["master"]["receives"] = serde_json::json!([{ "from": "ghost", "channel": "main", "volume": 1.0 }]);
    v["clips"] = serde_json::json!([{ "id": "a", "length": 3840, "notes": [[0, 480, 130, 100], [0, 0, 60, 100], [3840, 10, 60, 100]] }]);
    v["tracks"][0]["lanes"] = serde_json::json!([{ "device": 0, "param": "filt1.cutoff", "points": [[20000, 0.5]] },
        { "device": 0, "param": "capacity", "points": [[0, 0.5]] }]);
    has(&v, "/tracks/0/devices/1/kind", "kind 9 is a legacy device");
    has(&v, "/tracks/0/devices/0/values/filt1.cutoff", "out of range");
    has(&v, "/tracks/0/placements/0/clip", "unknown clip");
    has(&v, "/master/receives/0/from", "unknown track");
    has(&v, "/clips/0/notes/0", "pitch");
    has(&v, "/clips/0/notes/1", "length");
    has(&v, "/clips/0/notes/2", "outside the clip");
    has(&v, "/tracks/0/lanes/0/points/0", "outside the song");
    has(&v, "/tracks/0/lanes/1/param", "structural");
}

#[test]
fn receive_cycle_is_rejected() {
    let mut v = base();
    let mut b = v["tracks"][0].clone();
    b["id"] = "b".into();
    b["receives"] = serde_json::json!([{ "from": "lead", "channel": "main", "volume": 1.0 }]);
    v["tracks"][0]["receives"] = serde_json::json!([{ "from": "b", "channel": "main", "volume": 1.0 }]);
    v["tracks"].as_array_mut().unwrap().push(b);
    has(&v, "/tracks/0/receives/0", "cycle");
}

#[test]
fn unknown_fields_and_bad_format_are_rejected() {
    let mut v = base();
    v["tracks"][0]["colour"] = serde_json::json!("#fff");
    has(&v, "/tracks/0", "unknown field");
    let mut v = base();
    v["format"] = serde_json::json!("song/2");
    has(&v, "/format", "song/1");
}

#[test]
fn export_refuses_an_invalid_song() {
    let mut v = base();
    v["tracks"][0]["devices"][0]["values"]["nope"] = serde_json::json!(0.5);
    let e = export(&song_of(&v)).unwrap_err();
    assert_eq!(e[0].path, "/tracks/0/devices/0/values/nope");
}

// ----- import -----

#[test]
fn blob_round_trip_keeps_chunks_routing_volumes_and_automation() {
    let mut v = base();
    v["clips"] = serde_json::json!([{ "id": "a", "length": 3840, "notes": [[0, 480, 60, 100], [481, 479, 62, 90], [1000, 7, 64, 1]] }]);
    v["tracks"][0]["placements"] = serde_json::json!([{ "clip": "a", "start": 0, "length": 3840, "loop": false }]);
    v["tracks"][0]["devices"][0]["values"] = serde_json::json!({ "osc1.type": 0.25, "filt1.type": 0.2, "env1.attack": 0.123456 });
    v["tracks"][0]["devices"].as_array_mut().unwrap().push(serde_json::json!({ "kind": 68, "values": { "mode": 0.0, "feedback": 0.3 } }));
    v["tracks"][0]["lanes"] = serde_json::json!([{ "device": 1, "param": "feedback", "points": [[0, 0.1], [1234.5, 0.7]] }]);
    v["tracks"][0]["volume"] = serde_json::json!(0.7);
    v["master"]["receives"][0]["volume"] = serde_json::json!(0.9);
    let b1 = exported(&v);
    let s2 = import(&b1).unwrap();
    assert!(validate(&s2).is_empty(), "{:?}", validate(&s2));
    let b2 = export(&s2).unwrap();
    let (x, y) = (Blob::parse(&b1).unwrap(), Blob::parse(&b2).unwrap());
    assert_eq!(x.devices().map(|d| (d.kind, d.chunk.to_vec())).collect::<Vec<_>>(), y.devices().map(|d| (d.kind, d.chunk.to_vec())).collect::<Vec<_>>());
    for (a, b) in x.tracks().zip(y.tracks()) {
        assert_eq!(a.volume, b.volume);
        assert_eq!(a.receives().collect::<Vec<_>>(), b.receives().collect::<Vec<_>>());
        assert_eq!(a.devices().collect::<Vec<_>>(), b.devices().collect::<Vec<_>>());
        let pa: Vec<_> = a.automations().map(|q| (q.device, q.param, q.points().collect::<Vec<_>>())).collect();
        let pb: Vec<_> = b.automations().map(|q| (q.device, q.param, q.points().collect::<Vec<_>>())).collect();
        assert_eq!(pa, pb);
    }
    let half_tick = 44100.0 * 60.0 / (120.0 * 960.0) / 2.0;
    let (e1, e2) = (events(&b1, 0), events(&b2, 0));
    assert_eq!(e1.len(), e2.len());
    for (a, b) in e1.iter().zip(&e2) {
        assert_eq!(a.1, b.1);
        assert!(((a.0 - b.0) as f64).abs() <= half_tick, "{a:?} vs {b:?}");
    }
}

#[test]
fn automation_after_the_song_end_is_cut_at_the_end_not_stretching_the_song() {
    let mut v = base();
    v["length"] = serde_json::json!(7680);
    v["tracks"][0]["devices"].as_array_mut().unwrap().push(serde_json::json!({ "kind": 68, "values": { "mode": 0.0 } }));
    v["tracks"][0]["lanes"] = serde_json::json!([{ "device": 1, "param": "feedback", "points": [[0, 0.0], [7680, 1.0]] }]);
    let mut b = exported(&v);
    b[8..16].copy_from_slice(&2.0f64.to_le_bytes()); // the blob now ends at 2 s = 3840 ticks, halfway along the lane
    let s = import(&b).unwrap();
    assert!(validate(&s).is_empty(), "{:?}", validate(&s));
    assert_eq!(s.length, 3840);
    let pts = &s.tracks[0].lanes[0].points;
    assert_eq!(pts.len(), 2);
    assert_eq!(pts[1].tick, 3840.0);
    assert!((pts[1].value - 0.5).abs() < 0.01, "{}", pts[1].value);
}

#[test]
fn a_note_on_before_a_note_off_at_one_sample_keeps_its_order() {
    // a blob may start the next note before it ends the last at the same sample (a mono instrument's legato, the
    // clocked converter's tied phrases); song/1 puts note-offs first at one sample, so the import holds the ended
    // note one tick longer: the export plays the new note first again. A note ended at its own start (on, off at one
    // sample) keeps its shortest length.
    let mut m = song::export::export_model(&song_of(&base())).unwrap();
    let l = m.tracks[0].lane;
    m.lanes[l] = vec![(1000, on(64, 100)), (5000, on(67, 90)), (5000, off(64)), (9000, on(69, 80)), (9000, off(69)), (9000, off(67))];
    let back = export(&import(&m.write()).unwrap()).unwrap();
    let kinds: Vec<EventKind> = events(&back, l).into_iter().map(|e| e.1).collect();
    assert_eq!(kinds, [on(64, 100), on(67, 90), off(64), on(69, 80), off(67), off(69)]);
}

#[test]
fn imported_released_songs_quirks_become_valid_songs() {
    // velocity 128 in the blob (a byte) imports as 127
    let mut v = base();
    v["clips"] = serde_json::json!([{ "id": "a", "length": 3840, "notes": [[0, 480, 60, 99]] }]);
    v["tracks"][0]["placements"] = serde_json::json!([{ "clip": "a", "start": 0, "length": 3840, "loop": false }]);
    v["tracks"][0]["devices"].as_array_mut().unwrap().push(serde_json::json!({ "kind": 68, "values": { "mode": 0.0 } }));
    v["tracks"][0]["lanes"] = serde_json::json!([{ "device": 1, "param": "feedback", "points": [[0, 0.0], [960, 0.5], [1920, 1.0]] }]);
    let mut b = exported(&v);
    let k = b.windows(2).position(|w| w == [60, 99]).expect("note-on 60 velocity 99");
    b[k + 1] = 128;
    let s = import(&b).unwrap();
    assert_eq!(s.clips.iter().flat_map(|c| c.notes.iter()).map(|n| n.vel).collect::<Vec<_>>(), vec![127]);
    assert!(validate(&s).is_empty(), "{:?}", validate(&s));
}

#[test]
fn legacy_kinds_point_to_synth_convert() {
    let mut v = base();
    v["tracks"][0]["devices"] = serde_json::json!([]);
    let mut b = exported(&v);
    // patch the blob: one device of kind 1 (Slaughter) on track 0
    let s = Blob::parse(&b).unwrap();
    assert_eq!(s.device_count(), 0);
    let mut w = song::blob::BlobM::read(&b).unwrap();
    w.devices.push((1, vec![0, 0, 0, 0]));
    w.tracks[0].devices.push(0);
    b = w.write();
    let e = import(&b).unwrap_err();
    assert!(e.contains("kind 1 is a legacy device; convert the song with synth-convert first"), "{e}");
}

// ----- presets -----

#[test]
fn preset_round_trip_gives_the_same_chunk() {
    let mut v = base();
    v["tracks"][0]["devices"][0]["values"] = serde_json::json!({ "env1.attack": 0.3, "osc1.type": 0.25, "master": 0.5 });
    let s = song_of(&v);
    let p = preset::save(&s.tracks[0].devices[0], "Lead 1");
    let text = preset::to_json(&p);
    assert!(text.find("\"master\"").unwrap() < text.find("\"osc1.type\"").unwrap(), "sorted by index: {text}");
    assert!(text.find("\"osc1.type\"").unwrap() < text.find("\"env1.attack\"").unwrap(), "sorted by index: {text}");
    let p2 = preset::parse(&text).unwrap();
    let mut fresh = s.clone();
    fresh.tracks[0].devices[0] = preset::load(&p2);
    let (a, b) = (export(&s).unwrap(), export(&fresh).unwrap());
    assert_eq!(Blob::parse(&a).unwrap().devices().next().unwrap().chunk, Blob::parse(&b).unwrap().devices().next().unwrap().chunk);
}

#[test]
fn preset_drops_default_values() {
    let mut v = base();
    v["tracks"][0]["devices"][0]["values"] = serde_json::json!({ "osc1.type": 0.25, "filt1.cutoff": unisynth::params::DEFAULTS[unisynth::params::FILT + 1] });
    let p = preset::save(&song_of(&v).tracks[0].devices[0], "x");
    assert_eq!(p.values.len(), 1);
}

// ----- tables -----

#[test]
fn cutoff_at_half_reads_5015_hz() {
    let t = tables::table(64, 0).unwrap();
    let p = t.param("filt1.cutoff").unwrap();
    assert_eq!(p.physical(0.5).round(), 5015.0);
    assert_eq!(p.unit, "Hz");
    assert!(t.param("capacity").unwrap().structural);
    let tg = t.param("mod1.target").unwrap();
    assert_eq!((tg.options[0].as_str(), tg.options[5].as_str(), tg.options.len()), ("none", "osc1.gain", unisynth::modmx::target::COUNT as usize));
}

#[test]
fn effect_tables_match_the_engine_parameter_counts() {
    use synth_core::device::Device;
    use synth_core::mix::Env;
    use synth_core::pool::Pool;
    for &(kind, mode) in tables::EFFECT_MODES {
        let t = tables::table(kind, mode).unwrap_or_else(|| panic!("no table for {kind}/{mode}"));
        let mut vals = vec![unisynth::fx::mode_value(kind, mode)];
        vals.extend(t.params.iter().skip(1).map(|p| p.default));
        let chunk = unisynth::fx::encode(&vals);
        let env = Env { rate: 44100.0, tempo: 120, bank: None };
        let mut mem = vec![0u8; unisynth::fx::UFx::need(kind, &chunk, &env).unwrap() + 64];
        let mut pool = Pool::new(&mut mem);
        let fx = unisynth::fx::UFx::new(kind, &chunk, &env, &mut pool).unwrap();
        assert_eq!(fx.param_count(), t.params.len(), "{kind}/{mode}");
        assert_eq!(unisynth::fx::mode_of(kind, &chunk), mode);
    }
}

#[test]
fn every_oscillator_kind_has_an_option_name() {
    // the expanded osc type choice names every kind the engine plays (the clocked oscillator too)
    let t = tables::table(64, 0).unwrap();
    let o = &t.param("osc1.type").unwrap().options;
    for name in ["Off", "Phase mod", "Saw/pulse", "Wavetable", "Sampler", "Naive", "Clocked"] {
        assert!(o.iter().any(|x| x == name), "{name} missing from {o:?}");
    }
}

#[test]
fn every_engine_effect_mode_has_a_table() {
    // a mode the engine plays (the clocked's wrap and comb too) must load into a song
    for kind in unisynth::kind::DISTORTION..=unisynth::kind::FORMANT {
        for mode in 0..unisynth::fx::mode_total(kind) {
            assert!(tables::EFFECT_MODES.contains(&(kind, mode)), "{kind}/{mode} missing from EFFECT_MODES");
            assert!(tables::table(kind, mode).is_some(), "no table for {kind}/{mode}");
            assert!(tables::mode_names(kind).len() > mode, "no name for {kind}/{mode}");
        }
    }
}

// ---- song metadata (author, description, copyright, url; adapted works say so) ----

fn with_meta(meta: serde_json::Value) -> serde_json::Value {
    let mut v = base();
    v["meta"] = meta;
    v
}

fn adapted() -> serde_json::Value {
    serde_json::json!({
        "title": "Elysian",
        "author": "logicoma",
        "description": "Adapted from the WaveSabre project Elysian: converted to the unified synth with synth-convert.",
        "copyright": "Copyright 2012-2021 WaveSabre Team",
        "license": "MIT",
        "url": "https://github.com/logicomacorp/WaveSabre",
        "adapted_from": "WaveSabre test project Elysian.als (logicoma, 2016)"
    })
}

#[test]
fn metadata_survives_a_save_round_trip_and_is_written_before_the_music() {
    let v = with_meta(adapted());
    let s = song_of(&v);
    assert_eq!(s.meta.as_ref().unwrap().author, "logicoma");
    let json = to_json(&s);
    assert_eq!(serde_json::from_str::<serde_json::Value>(&json).unwrap()["meta"], v["meta"]);
    assert!(json.find("\"meta\"").unwrap() < json.find("\"clips\"").unwrap(), "meta comes first");
    assert_eq!(to_json(&parse(&json).unwrap()), json);
}

#[test]
fn a_song_without_metadata_saves_without_a_meta_key() {
    let json = to_json(&song_of(&base()));
    assert!(!json.contains("\"meta\""));
}

#[test]
fn metadata_is_not_part_of_the_blob() {
    assert_eq!(exported(&with_meta(adapted())), exported(&base()));
    assert!(import(&exported(&with_meta(adapted()))).unwrap().meta.is_none());
}

#[test]
fn metadata_fields_are_validated_with_paths() {
    // unknown field
    let mut m = adapted();
    m["mood"] = "calm".into();
    has(&with_meta(m), "/meta", "unknown field");
    // author, description, copyright and url may not be empty or blank
    for f in ["author", "description", "copyright", "url"] {
        let mut m = adapted();
        m[f] = "  ".into();
        has(&with_meta(m), &format!("/meta/{f}"), "empty");
    }
    // a url that is not http(s)
    let mut m = adapted();
    m["url"] = "javascript:alert(1)".into();
    has(&with_meta(m), "/meta/url", "http");
    // a missing field is a parse error naming it
    let mut m = adapted();
    m.as_object_mut().unwrap().remove("copyright");
    has(&with_meta(m), "/meta", "copyright");
}

#[test]
fn an_adapted_work_must_say_so_in_its_description() {
    let mut m = adapted();
    m["description"] = "A lovely tune.".into();
    has(&with_meta(m), "/meta/description", "adapted");
    // the original is named
    let mut m = adapted();
    m["adapted_from"] = "".into();
    has(&with_meta(m), "/meta/adapted_from", "empty");
    // an original work needs neither
    let mut m = adapted();
    m.as_object_mut().unwrap().remove("adapted_from");
    m["description"] = "A lovely tune.".into();
    assert!(issues(&with_meta(m)).is_empty());
    // "Adapted" in any case counts
    let mut m = adapted();
    m["description"] = "ADAPTED from x.".into();
    assert!(issues(&with_meta(m)).is_empty());
}


// ---- raw sample index versus the normalized shape[8] of a wavetable slot ----

fn with_osc(values: serde_json::Value) -> serde_json::Value {
    let mut v = base();
    v["tracks"][0]["devices"][0]["values"] = values;
    v
}

#[test]
fn a_wavetable_layer_spread_is_a_normal_value_and_the_sample_index_a_whole_number() {
    // osc1.s8 is the wavetable's layer spread
    assert!(issues(&with_osc(serde_json::json!({ "osc1.type": 0.75, "osc1.s8": 0.29 }))).is_empty());
    has(&with_osc(serde_json::json!({ "osc1.s8": 1.5 })), "/tracks/0/devices/0/values/osc1.s8", "out of range");
    // the sampler's sample index has its own raw parameter
    assert!(issues(&with_osc(serde_json::json!({ "osc1.type": 1.0, "osc1.sample": 3 }))).is_empty());
    has(&with_osc(serde_json::json!({ "osc1.sample": 0.5 })), "/tracks/0/devices/0/values/osc1.sample", "whole number");
    has(&with_osc(serde_json::json!({ "osc1.sample": 70000 })), "/tracks/0/devices/0/values/osc1.sample", "out of range");
}

#[test]
fn fractional_ticks_survive_load_and_save_exactly() {
    // imported automation keeps sample-exact fractional ticks; an inexact float parse would move them by an ulp on every save
    let mut v = base();
    v["tracks"][0]["lanes"] = serde_json::json!([{ "device": 0, "param": "master", "points": [[195597.72879818594, 0.5], [184319.9782312925, 0.25]] }]);
    let text = v.to_string();
    let s = parse(&text).unwrap();
    assert_eq!(s.tracks[0].lanes[0].points[0].tick, 195597.72879818594);
    let again = parse(&to_json(&s)).unwrap();
    assert_eq!(again, s);
    assert_eq!(to_json(&again), to_json(&s));
}

#[test]
fn the_controllers_are_named_parameters_with_a_semitone_range() {
    assert!(issues(&with_osc(serde_json::json!({ "ctl.wheel": 0.5, "ctl.bend": 0.75, "ctl.bend_range": 0.5 }))).is_empty());
    has(&with_osc(serde_json::json!({ "ctl.bend": 1.5 })), "/tracks/0/devices/0/values/ctl.bend", "out of range");
    let t = tables::table(64, 0).unwrap();
    let p = t.param("ctl.bend_range").expect("ctl.bend_range is a parameter");
    assert_eq!((p.unit, p.min, p.max), ("st", 0.0, 24.0));
    // automation of the wheel exports (the blob has no controller events: it is an ordinary lane)
    let mut v = with_osc(serde_json::json!({}));
    v["tracks"][0]["lanes"] = serde_json::json!([{ "device": 0, "param": "ctl.wheel", "points": [[0, 0.0], [960, 1.0]] }]);
    assert!(!exported(&v).is_empty());
}

// ---- descriptions (`about` on the song and its tracks; change add-import-patterns) ----

fn with_about() -> serde_json::Value {
    let mut v = base();
    v["about"] = serde_json::json!({ "type": "electronic", "style": "trance", "tempo": "120 BPM, four on the floor", "description": "A test song." });
    v["tracks"][0]["about"] = serde_json::json!({
        "type": "lead", "engine": "2-operator FM", "similar_to": ["Yamaha DX7"], "description": "A bright FM lead."
    });
    v
}

#[test]
fn descriptions_survive_a_save_round_trip_and_are_not_exported() {
    let v = with_about();
    let s = song_of(&v);
    assert!(validate(&s).is_empty(), "{:?}", validate(&s));
    assert_eq!(s.tracks[0].about.as_ref().unwrap().similar_to, ["Yamaha DX7"]);
    let json = to_json(&s);
    let back: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!((&back["about"], &back["tracks"][0]["about"]), (&v["about"], &v["tracks"][0]["about"]));
    assert_eq!(exported(&v), exported(&base()));
}

#[test]
fn descriptions_are_validated_with_paths() {
    let mut v = with_about();
    v["tracks"][0]["about"]["type"] = "".into();
    v["about"]["description"] = " ".into();
    v["tracks"][0]["about"]["similar_to"] = serde_json::json!(["", "Juno-106"]);
    has(&v, "/tracks/0/about/type", "empty");
    has(&v, "/about/description", "empty");
    has(&v, "/tracks/0/about/similar_to/0", "empty");
    let mut v = with_about();
    v["tracks"][0]["about"]["mood"] = "calm".into();
    has(&v, "/tracks/0/about", "unknown field");
}

#[test]
fn an_import_config_names_the_tracks_and_attaches_descriptions() {
    let mut s = import(&exported(&base())).unwrap();
    let mut cfg = adapted();
    cfg["about"] = serde_json::json!({ "type": "electronic", "description": "A test song." });
    cfg["tracks"] = serde_json::json!([{ "name": "Big Lead", "about": { "type": "lead", "description": "FM lead." } }]);
    song::apply_meta(&mut s, &cfg.to_string()).unwrap();
    assert_eq!(s.tracks[0].name, "Big Lead");
    assert_eq!(s.tracks[0].about.as_ref().unwrap().kind, "lead");
    assert_eq!(s.about.as_ref().unwrap().kind, "electronic");
    assert_eq!(s.meta.as_ref().unwrap().author, "logicoma");
    assert!(validate(&s).is_empty());
    // a plain meta file (no about, no tracks) still works
    let mut s = import(&exported(&base())).unwrap();
    song::apply_meta(&mut s, &adapted().to_string()).unwrap();
    assert_eq!(s.tracks[0].name, "Track 1");
    // the track count must match; nothing changes on error
    cfg["tracks"] = serde_json::json!([{ "name": "A" }, { "name": "B" }]);
    let e = song::apply_meta(&mut s, &cfg.to_string()).unwrap_err();
    assert!(e.contains("2 entries for 1 tracks"), "{e}");
    assert_eq!(s.tracks[0].name, "Track 1");
    cfg["tracks"] = serde_json::json!([{ "name": "A", "colour": "red" }]);
    assert!(song::apply_meta(&mut s, &cfg.to_string()).unwrap_err().contains("unknown field"));
}

/// change unify-v2-algorithms, "Functional names": no table, parameter, option, effect mode, module or feature names a
/// source synth (words, so `env2` — envelope 2 — is fine).
#[test]
fn no_source_names_in_the_parameter_table() {
    fn source(s: &str) -> bool {
        let l = s.to_ascii_lowercase();
        ["tunefish", "wavesabre", "ronan", "moog", "slaughter", "falcon", "cauldron"].iter().any(|x| l.contains(x))
            || l.split(|c: char| !c.is_ascii_alphanumeric()).any(|t| t == "tf" || t == "ws" || t.starts_with("v2"))
    }
    let mut bad = vec![];
    for t in song::tables::tables() {
        if source(&t.label) {
            bad.push(t.label.clone());
        }
        for p in &t.params {
            if source(&p.name) || source(p.unit) {
                bad.push(format!("{}: {}", t.label, p.name));
            }
            bad.extend(p.options.iter().filter(|o| source(o)).map(|o| format!("{}: {} option {o}", t.label, p.name)));
        }
        bad.extend(song::tables::mode_names(t.kind).iter().filter(|m| source(m)).map(|m| format!("mode {m}")));
    }
    for &id in unisynth::module::ALL {
        for s in [unisynth::module::feature(id), unisynth::module::name(id)] {
            if source(s) {
                bad.push(format!("module {id}: {s}"));
            }
        }
    }
    let cargo = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../unisynth/Cargo.toml")).unwrap();
    bad.extend(cargo.lines().filter_map(|l| l.split_once(" = ").map(|(k, _)| k.trim())).filter(|k| k.starts_with("us-") && source(k)).map(String::from));
    assert!(bad.is_empty(), "{bad:#?}");
}

/// change unify-v2-algorithms 1.3: a song file with the old parameter names (values and lanes) loads under the new names.
#[test]
fn old_parameter_names_load_under_the_new_ones() {
    let d: song::model::Values = serde_json::from_str(r#"{"env1.mode": 0.5, "vdist.mode": 0.25, "ch.volume": 0.5, "lfo1.eg": 1, "master": 0.2}"#).unwrap();
    let names: Vec<&str> = d.0.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["env1.law", "drive.mode", "strip.volume", "lfo1.one_shot", "master"]);
    let l: song::model::Lane = serde_json::from_str(r#"{"device": 0, "param": "env2.slope", "points": []}"#).unwrap();
    assert_eq!(l.param, "env2.curve_slope");
}

/// Receives on the extra pairs (V2 reverb/delay sends, aux inputs): `send` names the sending track's output pair (1–4),
/// `channel` `in1`/`in2` the receiving track's extra input pair. They reach the blob as `recv::encode(send, to)` and
/// come back from `import` unchanged.
#[test]
fn receives_on_extra_pairs_round_trip() {
    let mut v = base();
    v["tracks"].as_array_mut().unwrap().push(serde_json::json!({
        "id": "verb", "name": "Verb", "volume": 1.0, "mute": false,
        "receives": [ { "from": "lead", "channel": "main", "send": 1, "volume": 1.0 } ],
        "devices": [ { "kind": 70, "values": {} } ], "placements": [], "lanes": [] }));
    v["tracks"].as_array_mut().unwrap().push(serde_json::json!({
        "id": "aux", "name": "Aux", "volume": 1.0, "mute": false,
        "receives": [ { "from": "lead", "channel": "in1", "send": 3, "volume": 0.5 } ],
        "devices": [ { "kind": 64, "values": {} } ], "placements": [], "lanes": [] }));
    let blob = exported(&v);
    let m = song::blob::BlobM::read(&blob).unwrap();
    assert_eq!(m.tracks[1].receives, vec![(0, synth_core::device::recv::encode(1, 0), 1.0)]);
    assert_eq!(m.tracks[2].receives, vec![(0, synth_core::device::recv::encode(3, 4), 0.5)]);
    let back: serde_json::Value = serde_json::from_str(&to_json(&import(&blob).unwrap())).unwrap();
    let recv = |t: usize| back["tracks"][t]["receives"][0].clone();
    assert_eq!((recv(1)["channel"].clone(), recv(1)["send"].clone()), (serde_json::json!("main"), serde_json::json!(1)));
    assert_eq!((recv(2)["channel"].clone(), recv(2)["send"].clone()), (serde_json::json!("in1"), serde_json::json!(3)));
    assert!(back["master"]["receives"][0].get("send").is_none(), "a main-output receive writes no send");
}

#[test]
fn a_send_past_the_extra_pairs_is_an_error() {
    let mut v = base();
    v["master"]["receives"][0]["send"] = serde_json::json!(5);
    let e = validate(&song_of(&v));
    assert!(e.iter().any(|i| i.path == "/master/receives/0/send"), "{e:?}");
}
