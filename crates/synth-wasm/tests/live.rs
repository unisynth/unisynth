//! synth-live-host scenarios (change add-synth-daw): same sound as the player, call sizes, edits while playing,
//! transport (stop, loop + chase, seek), audition, meters, and an edit fuzz.

mod common;
use common::*;
use synth_wasm::Host;

fn loaded(b: &[u8]) -> Host {
    let mut h = Host::new();
    h.load(b).unwrap();
    h
}

#[test]
fn live_equals_the_player_in_128_frame_calls() {
    let b = blob(&song_json(64));
    let want = player(&b, 44100 * 3);
    for call in [128, 1, 77, 100, 4096] {
        let mut h = loaded(&b);
        h.play(0);
        let got = live(&mut h, 44100 * 3, call);
        assert!(got == want, "call size {call}: first difference at {:?}", got.iter().zip(&want).position(|(a, b)| a != b));
    }
    assert!(want.iter().any(|x| x.abs() > 0.05), "the test song is audible");
}

#[test]
fn stopped_host_is_silent_and_play_starts_at_the_frame() {
    let b = blob(&song_json(64));
    let mut h = loaded(&b);
    assert!(h.render(1024).iter().all(|&x| x == 0.0));
    assert_eq!(h.position(), 0, "stopped transport does not move");
    h.play(0);
    let _ = live(&mut h, 1000, 128);
    assert_eq!(h.position(), 1000);
}

#[test]
fn knob_while_playing_changes_only_that_track_from_the_next_sub_block() {
    let b = blob(&song_json(64));
    let (mut a, mut c) = (loaded(&b), loaded(&b));
    a.play(0);
    c.play(0);
    let x = live(&mut a, 22050, 128);
    let y = live(&mut c, 22050, 128);
    assert_eq!(x, y);
    // pad (track 1), instrument (device 0), master gain
    c.set_param(1, 0, unisynth::params::v::MASTER, 0.05);
    let x = live(&mut a, 4410, 128);
    let y = live(&mut c, 4410, 128);
    assert_ne!(x, y, "the change is audible");
    // 22 050 frames = 220.5 sub-blocks: the rest of the current sub-block (50 frames) was rendered before the edit
    assert_eq!(x[..2 * 50], y[..2 * 50], "applies at the next sub-block start, not mid-block");
    // the lead track's own output is unaffected (its peaks match)
    let _ = (live(&mut a, 128, 128), live(&mut c, 128, 128));
    assert_eq!(a.peaks()[0], c.peaks()[0]);
    assert_ne!(a.peaks()[1], c.peaks()[1]);
}

#[test]
fn note_edit_ahead_of_the_playhead_plays_at_the_new_pitch() {
    // edit at 0.25 s; the edited lead note starts at tick 1920 = 1 s (and every bar after)
    let (b1, b2) = (blob(&song_json(64)), blob(&song_json(76)));
    let mut h = loaded(&b1);
    h.play(0);
    let mut got = live(&mut h, 11025, 128);
    h.update(&b2).unwrap();
    got.extend(live(&mut h, 44100 * 2, 128));
    // a host that had the edited song from the start, rendered the same way
    let mut r = loaded(&b2);
    r.play(0);
    let want = live(&mut r, 11025 + 44100 * 2, 128);
    assert!(got == want, "after a schedule swap the track plays the edited notes");
    assert_ne!(want, { let mut o = loaded(&b1); o.play(0); live(&mut o, 11025 + 44100 * 2, 128) });
}

#[test]
fn value_only_edit_is_a_parameter_set_not_a_rebuild() {
    let b1 = blob(&song_json(64));
    let mut v = song_json(64);
    v["tracks"][1]["devices"][0]["values"]["master"] = serde_json::json!(0.1);
    let b2 = blob(&v);
    let mut h = loaded(&b1);
    h.play(0);
    let _ = live(&mut h, 22050, 128);
    let e = h.update(&b2).unwrap();
    assert_eq!((e.params, e.rebuilt, e.schedules, e.mixer), (1, 0, 0, false), "{e:?}");
    v["tracks"][1]["devices"][0]["values"]["osc1.type"] = serde_json::json!(0.25); // structural: oscillator type
    let e = h.update(&blob(&v)).unwrap();
    assert_eq!(e.rebuilt, 1, "{e:?}");
}

#[test]
fn loop_wraps_at_the_loop_end_sample_and_chases_held_notes() {
    let b = blob(&song_json(64));
    let (start, end) = (22050u64, 44100 + 333);
    let mut h = loaded(&b);
    h.set_loop(Some((start, end)));
    h.play(end - 1000);
    let got = live(&mut h, 1000 + 4000, 128);
    assert_eq!(h.position(), start + 4000);
    // after the wrap the host sounds like a host started at the loop start (devices reset, notes chased)
    let mut r = loaded(&b);
    r.set_loop(Some((start, end)));
    r.play(start);
    let want = live(&mut r, 4000, 128);
    assert!(got[2 * 1000..] == want[..], "wrap = seek to the loop start with chase");
    // the held pad chord is re-triggered at the loop start
    assert!(want[..2 * 2000].iter().any(|x| x.abs() > 0.01));
}

#[test]
fn audition_sounds_while_stopped() {
    let b = blob(&song_json(64));
    let mut h = loaded(&b);
    h.note_on(0, 60, 100);
    let out = live(&mut h, 2048, 128);
    assert!(out.iter().any(|x| x.abs() > 0.01));
    h.note_off(0, 60);
    let _ = live(&mut h, 44100, 128);
    // out-of-range tracks and notes are ignored
    h.note_on(99, 60, 100);
    h.note_on(0, 200, 100);
    h.note_off(99, 1);
}

#[test]
fn stop_lets_tails_ring_and_stop_twice_silences() {
    let b = blob(&song_json(64));
    let mut h = loaded(&b);
    h.play(0);
    let _ = live(&mut h, 30000, 128);
    h.stop();
    let pos = h.position();
    let tail = live(&mut h, 4096, 128);
    assert!(tail.iter().any(|x| x.abs() > 1e-4), "effect tails continue");
    assert_eq!(h.position(), pos, "position holds while stopped");
    h.stop();
    assert!(live(&mut h, 1024, 128).iter().all(|&x| x == 0.0), "second stop silences");
}

#[test]
fn meters_report_per_track_peaks_and_load() {
    let mut v = song_json(64);
    v["tracks"][0]["placements"] = serde_json::json!([]);
    v["tracks"][0]["lanes"] = serde_json::json!([]);
    let mut h = loaded(&blob(&v));
    h.play(0);
    let _ = live(&mut h, 22050, 128);
    let p = h.peaks();
    assert_eq!(p.len(), 4, "3 tracks + master");
    assert_eq!(p[0], [0.0, 0.0], "a track without notes has peak 0");
    assert!(p[1][0] > 0.0 && p[3][0] > 0.0);
    assert!(h.load_ratio() >= 0.0);
}

#[test]
fn seek_restarts_at_the_target_without_preroll() {
    let b = blob(&song_json(64));
    let mut h = loaded(&b);
    h.play(0);
    let _ = live(&mut h, 1000, 128);
    h.seek(30000);
    assert_eq!(h.position(), 30000);
    let out = live(&mut h, 2000, 128);
    assert_eq!(h.position(), 32000);
    assert!(out.iter().any(|x| x.abs() > 0.01), "held pad chord chased at the seek target");
}

#[test]
fn random_edits_between_calls_never_panic() {
    let base = song_json(64);
    let mut h = loaded(&blob(&base));
    let mut x = 0x1234_5678u32;
    let mut rnd = move || {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x
    };
    h.play(0);
    for step in 0..400 {
        match rnd() % 9 {
            0 => h.set_param(rnd() as usize % 6, rnd() as usize % 3, rnd() as usize % 200, (rnd() % 1000) as f32 / 999.0),
            1 => h.note_on(rnd() as usize % 6, (rnd() % 140) as u8, (rnd() % 128) as u8),
            2 => h.note_off(rnd() as usize % 6, (rnd() % 140) as u8),
            3 => h.seek(rnd() as u64 % 300_000),
            4 => h.set_loop(if rnd() % 2 == 0 { None } else { let a = rnd() as u64 % 100_000; Some((a, a + 1 + rnd() as u64 % 50_000)) }),
            5 => { if rnd() % 3 == 0 { h.stop() } else { h.play(rnd() as u64 % 200_000) } }
            6 => {
                let mut v = base.clone();
                v["clips"][0]["notes"][0][2] = serde_json::json!(rnd() % 128);
                v["tracks"][rnd() as usize % 3]["volume"] = serde_json::json!((rnd() % 100) as f64 / 50.0);
                if rnd() % 2 == 0 { v["tracks"][0]["devices"][0]["values"]["osc1.type"] = serde_json::json!((rnd() % 5) as f64 / 12.0); }
                if rnd() % 3 == 0 { v["tracks"][2]["receives"] = serde_json::json!([{ "from": "lead", "channel": "main", "volume": 1.0 }]); }
                if rnd() % 4 == 0 { v["tracks"].as_array_mut().unwrap().pop(); v["master"]["receives"] = serde_json::json!([{ "from": "lead", "channel": "main", "volume": 1.0 }]); }
                h.update(&blob(&v)).unwrap();
            }
            7 => { let _ = h.update(&[1, 2, 3]); }
            _ => {}
        }
        let out = h.render(1 + rnd() as usize % 600);
        assert!(out.iter().all(|v| v.is_finite()), "step {step}");
    }
}

#[test]
fn watching_a_track_reports_its_voices_and_taps_and_never_changes_the_sound() {
    let b = blob(&song_json(64));
    let mut a = loaded(&b);
    a.play(0);
    let want = live(&mut a, 44100 * 2, 128);
    assert!(a.probe().is_empty(), "nothing watched: no probe");
    let mut h = loaded(&b);
    h.watch(Some(1)); // the pad: a held two-note chord
    h.play(0);
    let mut got = live(&mut h, 128 * 300, 128);
    let p = h.probe().to_vec();
    assert_eq!((p[0], p[1], p[2], p[3]), (1.0, 2.0, 4.0, 2048.0), "two pad voices, 4 filter slots");
    let notes: Vec<f32> = (0..2).map(|v| p[4 + v * 22 + 1]).collect();
    assert!(notes.contains(&48.0) && notes.contains(&55.0), "{notes:?}");
    // the pad has no filter on: every slot reports no taps
    assert_eq!(&p[4 + 2 * 22..], &[0.0, 0.0, 0.0, 0.0]);
    got.extend(live(&mut h, 44100 * 2 - 128 * 300, 128));
    assert!(got == want, "watching changed the output");
    h.watch(Some(0)); // the lead has filter 1 on: its taps come with the probe
    let _ = live(&mut h, 44100, 128);
    let p = h.probe().to_vec();
    let nv = p[1] as usize;
    assert_eq!(p[4 + nv * 22], 1.0, "filter 1 tapped");
    // the layout the page parses (packages/daw/src/app.js `parseProbe`, VREC = 22): header, voices, slot 1 + taps, 3 flags
    assert_eq!(p.len(), 4 + nv * 22 + 1 + 2 * 2048 + 3);
}

#[test]
fn running_open_filters_is_a_listening_switch_off_by_default() {
    // the lead with its low-pass fully open and no resonance: skipped by default, run with the switch
    let mut v = song_json(64);
    let lead = &mut v["tracks"][0];
    lead["devices"][0]["values"]["filt1.resonance"] = 0.0.into();
    lead["devices"][0]["values"]["filt1.cutoff"] = 1.0.into();
    lead["lanes"] = serde_json::json!([]);
    let b = blob(&v);
    let want = player(&b, 44100);
    let mut h = loaded(&b);
    h.play(0);
    assert!(live(&mut h, 44100, 128) == want, "off: the song as exported");
    let mut r = loaded(&b);
    r.set_run_open_filters(true);
    r.play(0);
    let got = live(&mut r, 44100, 128);
    assert!(got != want, "on: the open filter runs and changes the sound");
    // high-frequency energy (first difference) drops when the filter runs
    let hf = |x: &[f32]| x.windows(3).map(|w| ((w[2] - w[0]) as f64).powi(2)).sum::<f64>();
    assert!(hf(&got) < hf(&want), "{} vs {}", hf(&got), hf(&want));
    r.set_run_open_filters(false);
}
