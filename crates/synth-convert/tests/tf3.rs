//! Tunefish 3 projects (change add-tf3-import): synthetic projects convert, load and sound; the CLI; opt-in on the
//! Enigma-Studio-3 checkout (`$TF3_REF`, default `/tmp/tf3-ref`): Transplant and the presets convert.

mod common;

use std::process::Command;
use synth_convert::e3prj::{self, Cell, Instance, Instrument, Pattern, Project, Song, COUNT};
use synth_convert::tf3::Mode;

fn params(pairs: &[(&str, f32)]) -> Vec<f32> {
    let mut p = vec![0.0f32; COUNT];
    for &(n, x) in pairs {
        p[e3prj::index(n).unwrap()] = x;
    }
    p
}

/// A spline point oscillator with sub-oscillator and unison, a PAD table, ADSR 1 on volume, LFO 1 on the low-pass, a
/// reverb: most of what Transplant uses, on one instrument and a four-note phrase.
fn project() -> Project {
    let p = params(&[
        ("OscPoints", 1.0 / 3.0), ("OscP1Off", 0.25), ("OscP1Val", 1.0), ("OscP1Int", 1.0), ("OscP2Off", 0.66), ("OscP2Int", 1.0),
        ("OscFiInt", 1.0), ("OscVol", 0.5), ("OscPan", 0.5), ("OscPoly", 0.5), ("OscUnisn", 2.0 / 9.0), ("OscSpread", 0.3),
        ("OscSubOsc", 1.0), ("OscOctave", 0.5), ("AddVolume", 0.3), ("AddProfile", 1.0), ("AddOctave", 0.5), ("AddBandw", 0.4),
        ("AddDamp", 0.5), ("AddHarmncs", 0.2), ("NoiseBw", 1.0), ("FltLPOn", 1.0), ("FltLPCut", 0.6), ("FltLPRes", 0.3),
        ("Env1Dec", 0.5), ("Env1Sus", 0.6), ("Env1Rel", 0.3), ("Env1Slp", 0.5), ("Lfo1Rate", 0.3), ("Lfo1Dep", 0.5),
        ("Mm1Source", 3.0 / 9.0), ("Mm1Mod", 0.55), ("Mm1Target", 9.0 / 25.0), ("Mm2Source", 1.0 / 9.0), ("Mm2Mod", 0.55),
        ("Mm2Target", 16.0 / 25.0), ("Effect1", 0.5), ("RevRoom", 0.5), ("RevWet", 0.2), ("GainAmount", 0.5),
    ]);
    let cells = vec![
        Cell { row: 0, note: 58, instrument: 0, velocity: -1, effect: 0 },
        Cell { row: 4, note: 62, instrument: 0, velocity: 100, effect: 0 },
        Cell { row: 8, note: 65, instrument: 0, velocity: -1, effect: 0 },
        Cell { row: 12, note: 128, instrument: -1, velocity: -1, effect: 0 },
    ];
    Project {
        version: "3.11b".into(),
        songs: vec![Song { id: 1, name: "t".into(), bpm: 140, patterns: vec![Pattern { id: 0, rows: 16, tracks: vec![cells] }], instances: vec![Instance { row_offset: 0, track: 0, pattern: 0 }] }],
        instruments: vec![Instrument { id: 0, name: "lead".into(), params: p }],
    }
}

#[test]
fn a_synthetic_project_converts_loads_and_sounds() {
    for mode in [Mode::Player, Mode::Editor] {
        let c = synth_convert::tf3song::convert(&project(), None, mode, 1.0).unwrap();
        let out = common::loads_and_renders(&c.blob);
        let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 1e-3, "{mode:?}: silent");
    }
}

#[test]
fn the_cli_converts_a_project_and_a_preset() {
    let dir = std::env::temp_dir().join(format!("tf3-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (prj, song) = (dir.join("t.e3prj"), dir.join("t.song"));
    std::fs::write(&prj, e3prj::write(&project())).unwrap();
    let run = |a: &[&str]| Command::new(env!("CARGO_BIN_EXE_synth-convert")).args(a).output().unwrap();
    let r = run(&["e3prj", prj.to_str().unwrap(), song.to_str().unwrap(), "--song", "1", "2"]);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    common::loads_and_renders(&std::fs::read(&song).unwrap());
    // a missing pattern: an error naming it, no output
    let bad = dir.join("bad.e3prj");
    std::fs::write(&bad, e3prj::write(&project()).replace("patternid=\"0\" track=\"0\"", "patternid=\"7\" track=\"0\"")).unwrap();
    let out = dir.join("bad.song");
    let r = run(&["e3prj", bad.to_str().unwrap(), out.to_str().unwrap()]);
    assert!(!r.status.success() && String::from_utf8_lossy(&r.stderr).contains("missing pattern 7"));
    assert!(!out.exists());
    // a preset as a chunk
    let preset = dir.join("p.txt");
    let names = e3prj::names();
    let text: String = std::iter::once("String, Short".to_string())
        .chain(project().instruments[0].params.iter().enumerate().map(|(k, v)| format!("{};{v}", names[k])))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&preset, text).unwrap();
    let chunk = dir.join("p.chunk");
    assert!(run(&["tf3", preset.to_str().unwrap(), chunk.to_str().unwrap()]).status.success());
    unisynth::chunk::Chunk::parse(&std::fs::read(&chunk).unwrap()).expect("the preset chunk parses");
}

#[test]
fn the_cli_converts_with_the_2011_engine() {
    // add-tf3-pandora: --engine 2011 plays the older laws (here 3 sub passes at OscSubOsc 0.33, no PAD) and names the
    // known differences; an unknown engine is an error
    let dir = std::env::temp_dir().join(format!("tf3-early-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut prj = project();
    let p = &mut prj.instruments[0].params;
    p[e3prj::index("OscSubOsc").unwrap()] = 0.33;
    p[e3prj::index("AddVolume").unwrap()] = 0.0;
    p[e3prj::index("Effect1").unwrap()] = 0.0;
    let (file, song) = (dir.join("e.e3prj"), dir.join("e.song"));
    std::fs::write(&file, e3prj::write(&prj)).unwrap();
    let run = |a: &[&str]| Command::new(env!("CARGO_BIN_EXE_synth-convert")).args(a).output().unwrap();
    let r = run(&["e3prj", file.to_str().unwrap(), song.to_str().unwrap(), "--engine", "2011", "1"]);
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(r.status.success(), "{err}");
    assert!(err.contains("2011 engine: no decimation stage"), "{err}");
    let out = common::loads_and_renders(&std::fs::read(&song).unwrap());
    assert!(out.iter().any(|v| v.abs() > 1e-3), "silent");
    let c = synth_convert::tf3song::convert(&prj, None, Mode::Early, 1.0).unwrap();
    let c12 = synth_convert::tf3song::convert(&prj, None, Mode::Player, 1.0).unwrap();
    assert_ne!(c.blob, c12.blob, "the 2011 laws change the conversion");
    let r = run(&["e3prj", file.to_str().unwrap(), song.to_str().unwrap(), "--engine", "1999"]);
    assert!(!r.status.success() && String::from_utf8_lossy(&r.stderr).contains("--engine takes 2011 or 2012"));
}

fn checkout() -> Option<std::path::PathBuf> {
    let dir = std::path::PathBuf::from(std::env::var("TF3_REF").unwrap_or_else(|_| "/tmp/tf3-ref".into()));
    dir.join("demos/transplant/transplant.e3prj").exists().then_some(dir)
}

#[test]
fn transplant_converts_and_sounds() {
    let Some(dir) = checkout() else { return };
    let text = String::from_utf8_lossy(&std::fs::read(dir.join("demos/transplant/transplant.e3prj")).unwrap()).into_owned();
    let c = synth_convert::tf3song::convert(&e3prj::read(&text).unwrap(), None, Mode::Player, 4.0).unwrap();
    let s = synth_convert::model::SongM::read(&c.blob).unwrap();
    assert_eq!(s.tracks.len(), 11 + 1, "the 11 instruments that play and the master");
    assert!(s.length > 218.0 && s.length < 230.0, "{}", s.length);
    common::loads_and_renders(&c.blob);
}

#[test]
fn every_preset_converts_to_a_chunk_that_loads() {
    let Some(dir) = checkout() else { return };
    let mut n = 0;
    for entry in std::fs::read_dir(dir.join("binary/tf3programs")).unwrap() {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        let (chunk, _) = synth_convert::tf3song::preset_chunk(&e3prj::read_preset(&text).unwrap(), Mode::Player);
        unisynth::UInstrument::need(&chunk).expect("the chunk loads");
        n += 1;
    }
    assert_eq!(n, 128);
}
