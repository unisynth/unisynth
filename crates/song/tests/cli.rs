//! The `song` binary: export / import / render / info / check, errors with JSON paths, and `render` equal to the
//! song player's render of the exported blob, sample for sample.

use std::process::Command;
use synth_core::mix::{Env, Player, Song, DEFAULT_SUB};
use synth_core::pool::Pool;
use unisynth::rng::Xorshift;
use unisynth::UDevice;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_song"))
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("song-cli-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

const DEMO: &str = r#"{
  "format": "song/1", "rate": 44100, "tempo": 128.0, "signature": [4, 4], "ticks_per_beat": 960, "length": 7680,
  "clips": [{ "id": "a", "length": 3840, "notes": [[0, 480, 57, 100], [960, 480, 60, 90], [1920, 960, 64, 110]] }],
  "tracks": [{ "id": "lead", "name": "Lead", "volume": 0.8, "mute": false, "receives": [],
    "devices": [{ "kind": 64, "values": { "osc1.type": 0.25 } }, { "kind": 66, "values": { "mode": 0.0, "vertical": 0.3 } }],
    "placements": [{ "clip": "a", "start": 0, "length": 7680, "loop": true }],
    "lanes": [{ "device": 1, "param": "dry_wet", "points": [[0, 0.0], [7680, 1.0]] }] }],
  "master": { "id": "master", "name": "Master", "volume": 1.0, "mute": false,
    "receives": [{ "from": "lead", "channel": "main", "volume": 1.0 }], "devices": [], "placements": [], "lanes": [] }
}"#;

#[test]
fn render_equals_the_player_render_of_the_exported_blob() {
    let json = tmp("demo.song.json");
    std::fs::write(&json, DEMO).unwrap();
    let blob = tmp("demo.song.bin");
    let wav = tmp("demo.wav");
    assert!(bin().args(["export", json.to_str().unwrap(), "-o", blob.to_str().unwrap()]).status().unwrap().success());
    assert!(bin().args(["render", json.to_str().unwrap(), "-o", wav.to_str().unwrap()]).status().unwrap().success());
    let b = std::fs::read(&blob).unwrap();
    let s = Song::parse(&b).unwrap();
    let env = Env::for_song(&s, None);
    let mut mem = vec![0u8; Player::<UDevice, Xorshift>::required_pool(&s, &env, DEFAULT_SUB).unwrap()];
    let mut pool = Pool::new(&mut mem);
    let mut p = Player::<UDevice, Xorshift>::new(s, &env, &mut pool, Xorshift::new(song::render::SEED), DEFAULT_SUB).unwrap();
    let mut pcm = vec![0i16; p.length_frames() as usize * 2];
    p.render_i16(&mut pcm);
    let w = std::fs::read(&wav).unwrap();
    assert_eq!(&w[..4], b"RIFF");
    let got: Vec<i16> = w[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
    assert_eq!(got.len(), pcm.len());
    assert!(got == pcm, "CLI render differs from the player");
    assert!(pcm.iter().any(|&x| x.unsigned_abs() > 1000), "the demo is audible");
    // the blob renders the same as its song
    let wav2 = tmp("demo2.wav");
    assert!(bin().args(["render", blob.to_str().unwrap(), "-o", wav2.to_str().unwrap()]).status().unwrap().success());
    assert_eq!(std::fs::read(&wav2).unwrap(), w);
    // 32-bit float variant
    let wav3 = tmp("demo3.wav");
    assert!(bin().args(["render", json.to_str().unwrap(), "-o", wav3.to_str().unwrap(), "--f32"]).status().unwrap().success());
    assert_eq!(std::fs::read(&wav3).unwrap().len(), 44 + pcm.len() * 4);
}

#[test]
fn import_info_and_check() {
    let json = tmp("i.song.json");
    std::fs::write(&json, DEMO).unwrap();
    let blob = tmp("i.song.bin");
    assert!(bin().args(["export", json.to_str().unwrap(), "-o", blob.to_str().unwrap()]).status().unwrap().success());
    let back = tmp("back.song.json");
    assert!(bin().args(["import", blob.to_str().unwrap(), "-o", back.to_str().unwrap()]).status().unwrap().success());
    let c = bin().args(["check", back.to_str().unwrap()]).output().unwrap();
    assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
    let info = bin().args(["info", json.to_str().unwrap()]).output().unwrap();
    let out = String::from_utf8_lossy(&info.stdout);
    assert!(out.contains("128 BPM") && out.contains("Unisynth → Crusher"), "{out}");
    assert!(out.contains("us-osc-pm") && out.contains("us-fx-crusher"), "{out}");
}

#[test]
fn errors_print_paths_and_write_nothing() {
    let json = tmp("bad.song.json");
    std::fs::write(&json, DEMO.replace("\"vertical\"", "\"verticle\"")).unwrap();
    let blob = tmp("bad.song.bin");
    let _ = std::fs::remove_file(&blob);
    let o = bin().args(["export", json.to_str().unwrap(), "-o", blob.to_str().unwrap()]).output().unwrap();
    assert!(!o.status.success());
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("/tracks/0/devices/1/values/verticle: unknown parameter"), "{err}");
    assert!(!blob.exists());
    assert!(!bin().args(["check", json.to_str().unwrap()]).status().unwrap().success());
}

const META: &str = r#"{
  "title": "Demo", "author": "Jane Roe", "description": "Adapted from a tracker module and converted with synth-convert.",
  "copyright": "Copyright 2020 Jane Roe", "license": "MIT", "url": "https://example.org/demo", "adapted_from": "demo.xm by Jane Roe"
}"#;

#[test]
fn import_with_meta_writes_it_and_info_shows_the_attribution() {
    let json = tmp("m.song.json");
    std::fs::write(&json, DEMO).unwrap();
    let blob = tmp("m.song.bin");
    assert!(bin().args(["export", json.to_str().unwrap(), "-o", blob.to_str().unwrap()]).status().unwrap().success());
    let meta = tmp("m.meta.json");
    std::fs::write(&meta, META).unwrap();
    let out = tmp("m.out.song.json");
    let o = bin().args(["import", blob.to_str().unwrap(), "-o", out.to_str().unwrap(), "--meta", meta.to_str().unwrap()]).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let s = song::load(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(s.meta.unwrap().author, "Jane Roe");
    let info = bin().args(["info", out.to_str().unwrap()]).output().unwrap();
    let text = String::from_utf8_lossy(&info.stdout);
    assert!(text.contains("Jane Roe") && text.contains("adapted from demo.xm by Jane Roe") && text.contains("https://example.org/demo"), "{text}");
    // a song without meta prints no attribution line
    let plain = bin().args(["info", json.to_str().unwrap()]).output().unwrap();
    assert!(!String::from_utf8_lossy(&plain.stdout).contains("by "));
}

#[test]
fn a_bad_meta_file_is_reported_with_its_path_and_nothing_is_written() {
    let json = tmp("b.song.json");
    std::fs::write(&json, DEMO).unwrap();
    let blob = tmp("b.song.bin");
    assert!(bin().args(["export", json.to_str().unwrap(), "-o", blob.to_str().unwrap()]).status().unwrap().success());
    let meta = tmp("b.meta.json");
    std::fs::write(&meta, META.replace("Adapted from a tracker module and", "A tune,")).unwrap(); // adapted_from but no "adapted"
    let out = tmp("b.out.song.json");
    let _ = std::fs::remove_file(&out);
    let o = bin().args(["import", blob.to_str().unwrap(), "-o", out.to_str().unwrap(), "--meta", meta.to_str().unwrap()]).output().unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("/meta/description"), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(!out.exists());
    let o = bin().args(["import", blob.to_str().unwrap(), "-o", out.to_str().unwrap(), "--meta", "/nonexistent.json"]).output().unwrap();
    assert!(!o.status.success() && !out.exists());
}

#[test]
fn import_patterns_by_default_one_clip_on_request_and_names_from_the_meta_file() {
    let json = tmp("p.song.json");
    std::fs::write(&json, DEMO).unwrap();
    let blob = tmp("p.song.bin");
    assert!(bin().args(["export", json.to_str().unwrap(), "-o", blob.to_str().unwrap()]).status().unwrap().success());
    let meta = tmp("p.meta.json");
    let mut cfg: serde_json::Value = serde_json::from_str(META).unwrap();
    cfg["tracks"] = serde_json::json!([{ "name": "Bassline", "about": { "type": "bass", "description": "Low line." } }]);
    std::fs::write(&meta, cfg.to_string()).unwrap();
    let out = tmp("p.out.song.json");
    let o = bin().args(["import", blob.to_str().unwrap(), "-o", out.to_str().unwrap(), "--meta", meta.to_str().unwrap()]).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let s = song::load(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(s.tracks[0].name, "Bassline");
    // the one-bar clip looped twice comes back as one pattern
    assert_eq!(s.clips.len(), 1);
    assert_eq!(s.clips[0].id, "t1/A");
    let one = tmp("p.one.song.json");
    assert!(bin().args(["import", blob.to_str().unwrap(), "-o", one.to_str().unwrap(), "--one-clip"]).status().unwrap().success());
    let s1 = song::load(&std::fs::read_to_string(&one).unwrap()).unwrap();
    assert_eq!(s1.clips[0].id, "lane1");
    assert_eq!(song::export(&s).unwrap(), song::export(&s1).unwrap());
    // a track count that does not match: reported, nothing written
    cfg["tracks"] = serde_json::json!([]);
    std::fs::write(&meta, cfg.to_string()).unwrap();
    let bad = tmp("p.bad.song.json");
    let _ = std::fs::remove_file(&bad);
    let o = bin().args(["import", blob.to_str().unwrap(), "-o", bad.to_str().unwrap(), "--meta", meta.to_str().unwrap()]).output().unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("0 entries for 1 tracks"), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(!bad.exists());
}

/// `--threads n` (change add-synth-threads): same WAV as the single-thread render; bad counts fail, nothing written.
#[cfg(feature = "threads")]
#[test]
fn render_threads_writes_the_same_wav_and_rejects_bad_counts() {
    let demo = concat!(env!("CARGO_MANIFEST_DIR"), "/../../project/music/demo.song.json");
    let one = tmp("t1.wav");
    assert!(bin().args(["render", demo, "-o", one.to_str().unwrap()]).status().unwrap().success());
    for n in ["1", "4"] {
        let w = tmp(&format!("t{n}x.wav"));
        assert!(bin().args(["render", demo, "-o", w.to_str().unwrap(), "--threads", n]).status().unwrap().success());
        assert_eq!(std::fs::read(&w).unwrap(), std::fs::read(&one).unwrap(), "--threads {n}");
    }
    for bad in [&["--threads", "0"][..], &["--threads", "x"], &["--threads"]] {
        let w = tmp("bad-threads.wav");
        let _ = std::fs::remove_file(&w);
        let o = bin().args(["render", demo, "-o", w.to_str().unwrap()]).args(bad).output().unwrap();
        assert!(!o.status.success(), "{bad:?}");
        assert!(String::from_utf8_lossy(&o.stderr).contains("--threads"), "{bad:?}");
        assert!(!w.exists(), "{bad:?}");
    }
}
