//! Clip controller envelopes (change add-synth-daw task 2.5a, spec "Clip controller envelopes"): a clip's `wheel` and
//! `bend` become automation of the instrument's `ctl.wheel` / `ctl.bend`, per placement and loop pass, back to rest at
//! the placement end.

use song::export::controller;
use song::model::Point;
use song::{export, export_model, parse, Song};

/// One phase-mod sine voice, 4 bars at 120 BPM; `clips` and `placements` filled in by each test.
fn song(clips: serde_json::Value, placements: serde_json::Value) -> Song {
    let v = serde_json::json!({
        "format": "song/1", "rate": 44100, "tempo": 120.0, "signature": [4, 4], "ticks_per_beat": 960,
        "length": 15360,
        "clips": clips,
        "tracks": [
            { "id": "lead", "name": "Lead", "volume": 1.0, "mute": false, "receives": [],
              "devices": [ { "kind": 64, "values": { "osc1.type": 0.25 } } ], "placements": placements, "lanes": [] }
        ],
        "master": { "id": "master", "name": "Master", "volume": 1.0, "mute": false, "receives": [
            { "from": "lead", "channel": "main", "volume": 1.0 } ], "devices": [], "placements": [], "lanes": [] }
    });
    parse(&v.to_string()).unwrap_or_else(|e| panic!("{e:?}"))
}

fn pts(p: &[Point]) -> Vec<(f64, f32)> {
    p.iter().map(|p| (p.tick, p.value)).collect()
}

#[test]
fn a_bend_envelope_is_automation_per_placement_and_loop_pass_reset_at_the_end() {
    let s = song(
        serde_json::json!([
            { "id": "up", "length": 1920, "notes": [[0, 1920, 60, 100]], "bend": [[0, 0.5], [960, 1.0]] },
            { "id": "plain", "length": 1920, "notes": [[0, 1920, 60, 100]] }
        ]),
        serde_json::json!([
            { "clip": "up", "start": 0, "length": 3840, "loop": true },
            { "clip": "plain", "start": 3840, "loop": false },
            { "clip": "up", "start": 7680, "loop": false }
        ]),
    );
    let t = &s.tracks[0];
    let bend = controller(&s, t, |c| &c.bend, 0.5).unwrap();
    assert_eq!(
        pts(&bend),
        vec![
            (0.0, 0.5),
            // first placement: two loop passes, the envelope restarting at each
            (0.0, 0.5), (960.0, 1.0), (1920.0, 1.0),
            (1920.0, 0.5), (2880.0, 1.0), (3840.0, 1.0),
            (3840.0, 0.5), // rest at its end: the plain clip plays unbent
            (7680.0, 0.5), (8640.0, 1.0), (9600.0, 1.0),
            (9600.0, 0.5),
        ]
    );
    assert!(controller(&s, t, |c| &c.wheel, 0.0).is_none(), "no wheel envelope, no wheel lane");
    // in the blob: one automation of ctl.bend (index 181) in samples, bytes
    let b = export_model(&s).unwrap();
    let a = &b.tracks[0].autos;
    assert_eq!(a.len(), 1);
    assert_eq!((a[0].device, a[0].param), (0, unisynth::params::ctl::BEND));
    assert_eq!(&a[0].points[..4], &[(0, 128), (0, 128), (22050, 255), (44100, 255)]);
}

/// Frequency of the strongest stretch of a mono signal by counting rising zero crossings over `[a, b)` seconds.
fn freq(x: &[f32], a: f64, b: f64) -> f64 {
    let (i0, i1) = ((a * 44100.0) as usize, (b * 44100.0) as usize);
    let mono: Vec<f32> = (i0..i1).map(|i| x[2 * i] + x[2 * i + 1]).collect();
    let ups: Vec<usize> = (1..mono.len()).filter(|&i| mono[i - 1] < 0.0 && mono[i] >= 0.0).collect();
    (ups.len() - 1) as f64 * 44100.0 / (ups[ups.len() - 1] - ups[0]) as f64
}

#[test]
fn a_bend_is_heard_and_ends_with_its_placement() {
    // bar 1: bent up by the full range (2 semitones by default); bar 3: the same note, unbent
    let s = song(
        serde_json::json!([
            { "id": "up", "length": 3840, "notes": [[0, 3840, 69, 100]], "bend": [[0, 1.0]] },
            { "id": "plain", "length": 3840, "notes": [[0, 3840, 69, 100]] }
        ]),
        serde_json::json!([{ "clip": "up", "start": 0, "loop": false }, { "clip": "plain", "start": 7680, "loop": false }]),
    );
    let x = song::render::render(&export(&s).unwrap(), None).unwrap();
    let (bent, plain) = (freq(&x, 0.5, 1.5), freq(&x, 4.5, 5.5));
    assert!((plain - 440.0).abs() < 2.0, "unbent A4 {plain}");
    assert!((bent / plain - 2f64.powf(2.0 / 12.0)).abs() < 0.005, "bent {bent} vs {plain}");
}

#[test]
fn a_bend_at_rest_renders_exactly_like_no_envelope() {
    // the byte nearest 0.5 is 128 = 0.50196; the engine treats it as the centre
    let notes = serde_json::json!([[0, 3840, 64, 100]]);
    let with = song(
        serde_json::json!([{ "id": "c", "length": 3840, "notes": notes, "bend": [[0, 0.5], [3840, 0.5]] }]),
        serde_json::json!([{ "clip": "c", "start": 0, "loop": false }]),
    );
    let without = song(serde_json::json!([{ "id": "c", "length": 3840, "notes": notes }]), serde_json::json!([{ "clip": "c", "start": 0, "loop": false }]));
    let (a, b) = (song::render::render(&export(&with).unwrap(), None).unwrap(), song::render::render(&export(&without).unwrap(), None).unwrap());
    assert_eq!(a, b);
}
