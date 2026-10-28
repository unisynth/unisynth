//! `ws` command, effect devices (task 6.1): structure, mode mapping, automation, malformed input.

mod common;
use common::*;
use synth_convert::{convert_ws, ConvError};
use tfsynth::device::kind as t;
use unisynth::kind as u;
use wavesabre::kind as w;

/// Every legacy effect kind with its unified `(kind, mode)`.
const MAP: [(u8, u8, usize); 16] = [
    (w::SCISSOR, u::DISTORTION, 0),
    (w::SCREWDRIVER, u::DISTORTION, 1),
    (t::DISTORTION, u::DISTORTION, 2),
    (w::CRUSHER, u::CRUSHER, 0),
    (w::LEVELLER, u::EQ, 0),
    (t::EQ, u::EQ, 1),
    (w::ECHO, u::DELAY, 0),
    (t::DELAY, u::DELAY, 1),
    (t::CHORUS, u::CHORUS, 0),
    (w::TWISTER, u::FLANGER, 0),
    (t::FLANGER, u::FLANGER, 1),
    (w::CATHEDRAL, u::REVERB, 0),
    (t::REVERB, u::REVERB, 1),
    (w::CHAMBER, u::REVERB, 2),
    (w::SMASHER, u::COMPRESSOR, 0),
    (t::FORMANT, u::FORMANT, 0),
];

fn params(n: usize, seed: f32) -> Vec<f32> {
    (0..n).map(|i| ((i as f32 * 0.137 + seed) % 1.0).abs()).collect()
}

fn convert(s: &SongM) -> SongM {
    SongM::read(&convert_ws(&s.write()).expect("converts")).expect("reads back")
}

/// One effect of every kind, one track each, plus a master that receives them all.
fn all_effects() -> SongM {
    let devices: Vec<_> = MAP.iter().enumerate().map(|(i, &(k, _, _))| dev(k, &params(1 + i % 9, i as f32 * 0.1))).collect();
    let mut tracks: Vec<_> = (0..MAP.len()).map(|i| track(&[i], i % 2)).collect();
    for (i, tr) in tracks.iter_mut().enumerate() {
        tr.volume = 0.25 + i as f32 * 0.05;
    }
    let mut master = track(&[], 0);
    master.receives = (0..MAP.len()).map(|i| (i, 0, 0.5 + i as f32 / 40.0)).collect();
    tracks.push(master);
    let lanes = vec![vec![(0, 60, Some(100)), (22050, 60, None)], vec![(100, 64, Some(1)), (5, 67, Some(127)), (1000, 64, None), (1, 67, None)]];
    song(devices, lanes, tracks)
}

#[test]
fn every_legacy_effect_becomes_its_kind_and_mode_with_its_parameters() {
    let src = all_effects();
    let out = convert(&src);
    assert_eq!(out.devices.len(), src.devices.len());
    for (i, &(_, k, mode)) in MAP.iter().enumerate() {
        let (d, s) = (&out.devices[i], &src.devices[i]);
        assert_eq!(d.kind, k, "device {i}");
        assert_eq!(unisynth::fx::mode_of(k, &d.chunk), mode, "device {i}");
        // the legacy chunk follows the mode unchanged: parameters and the trailer
        assert_eq!(&d.chunk[4..], &s.chunk[..], "device {i}");
    }
}

#[test]
fn structure_is_preserved_field_by_field() {
    let src = all_effects();
    let out = convert(&src);
    assert_eq!((out.tempo, out.rate, out.length), (src.tempo, src.rate, src.length));
    assert_eq!(out.lanes, src.lanes);
    assert_eq!(out.tracks.len(), src.tracks.len());
    for (a, b) in out.tracks.iter().zip(&src.tracks) {
        assert_eq!(a, b);
    }
}

#[test]
fn the_converted_song_loads_and_renders_in_the_unified_player() {
    loads_and_renders(&convert_ws(&all_effects().write()).unwrap());
}

#[test]
fn a_sidechain_receive_keeps_its_track_channel_and_volume() {
    // track 0: a source; track 1: Smasher keyed by track 0's sidechain send; track 2: master
    let mut t1 = track(&[0], 0);
    t1.receives = vec![(0, 2, 0.8), (0, 0, 0.3)];
    let mut master = track(&[], 0);
    master.receives = vec![(1, 0, 1.0)];
    let src = song(vec![dev(w::SMASHER, &[1.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5])], vec![vec![]], vec![track(&[], 0), t1, master]);
    let out = convert(&src);
    assert_eq!(out.devices[0].kind, u::COMPRESSOR);
    assert_eq!(out.tracks[1].receives, vec![(0, 2, 0.8), (0, 0, 0.3)]);
    loads_and_renders(&out.write());
}

#[test]
fn effect_automation_moves_one_parameter_up_and_keeps_its_points() {
    let pts = [(0, 0), (100, 255), (-50, 128), (1000, 7)];
    let mut tr = track(&[0, 1], 0);
    tr.autos = vec![auto(0, 0, &pts), auto(1, 3, &pts[..2])];
    let src = song(vec![dev(w::ECHO, &params(9, 0.2)), dev(t::REVERB, &params(4, 0.7))], vec![vec![]], vec![tr, track(&[], 0)]);
    let out = convert(&src);
    assert_eq!((out.tracks[0].autos[0].device, out.tracks[0].autos[0].param), (0, 1));
    assert_eq!(out.tracks[0].autos[0].points, pts);
    assert_eq!((out.tracks[0].autos[1].device, out.tracks[0].autos[1].param), (1, 4));
    assert_eq!(out.tracks[0].autos[1].points, pts[..2]);
    loads_and_renders(&out.write());
}

#[test]
fn short_legacy_chunks_keep_their_defaults() {
    for len in [0usize, 1, 3, 4, 5, 8] {
        let mut d = dev(w::CHAMBER, &[]);
        d.chunk = vec![7; len];
        let src = song(vec![d], vec![vec![]], vec![track(&[0], 0), track(&[], 0)]);
        let out = convert(&src);
        assert_eq!(unisynth::fx::mode_of(u::REVERB, &out.devices[0].chunk), 2, "{len}");
        loads_and_renders(&out.write());
    }
}

#[test]
fn converting_twice_is_an_error_not_a_silent_copy() {
    let once = convert_ws(&all_effects().write()).unwrap();
    assert!(matches!(convert_ws(&once), Err(ConvError::Device(k, _)) if k == u::DISTORTION));
}

#[test]
fn malformed_input_names_the_problem() {
    let good = all_effects().write();
    for n in 0..good.len() {
        assert!(matches!(convert_ws(&good[..n]), Err(ConvError::Input(_))), "truncated at {n}");
    }
    let mut trailing = good.clone();
    trailing.push(0);
    assert!(matches!(convert_ws(&trailing), Err(ConvError::Input(_))));
    let unknown = song(vec![dev(20, &[0.5])], vec![vec![]], vec![track(&[0], 0), track(&[], 0)]);
    let e = convert_ws(&unknown.write()).unwrap_err();
    assert!(matches!(e, ConvError::Device(20, _)) && e.to_string().contains("unknown"), "{e}");
}

mod cli {
    use std::process::Command;

    fn run(args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_synth-convert")).args(args).output().unwrap()
    }

    #[test]
    fn ws_converts_a_file_and_a_bad_input_writes_nothing() {
        let dir = std::env::temp_dir().join(format!("synth-convert-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (good, bad, out) = (dir.join("in.song"), dir.join("bad.song"), dir.join("out.song"));
        std::fs::write(&good, super::all_effects().write()).unwrap();
        let o = run(&["ws", good.to_str().unwrap(), out.to_str().unwrap()]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        assert!(super::loads_and_renders(&std::fs::read(&out).unwrap()).len() > 0);
        use super::common::dev;
        use super::common::track;
        use wavesabre::kind as w;

        std::fs::remove_file(&out).unwrap();
        let g = std::fs::read(&good).unwrap();
        std::fs::write(&bad, &g[..g.len() / 2]).unwrap();
        let o = run(&["ws", bad.to_str().unwrap(), out.to_str().unwrap()]);
        assert!(!o.status.success());
        assert!(String::from_utf8_lossy(&o.stderr).contains("not a valid"), "{}", String::from_utf8_lossy(&o.stderr));
        assert!(!out.exists(), "no output on error");

        assert!(!run(&["ws", dir.join("missing").to_str().unwrap(), out.to_str().unwrap()]).status.success());
        // warnings reach stderr and do not stop the conversion: 12 notes × 15 unison voices
        let mut sl = vec![0.0f32; 42];
        sl[2] = 1.0;
        sl[29] = 14.0 / 15.0;
        let big: Vec<_> = (0..12).map(|i| (0, 40 + i as u8, Some(100))).collect();
        let loud = super::common::song(vec![dev(w::SLAUGHTER, &sl)], vec![big], vec![track(&[0], 0), track(&[], 0)]);
        std::fs::write(&good, loud.write()).unwrap();
        let o = run(&["ws", good.to_str().unwrap(), out.to_str().unwrap()]);
        assert!(o.status.success());
        assert!(String::from_utf8_lossy(&o.stderr).contains("warning") && out.exists());
        assert!(!run(&[]).status.success());
        std::fs::remove_dir_all(&dir).ok();
    }
}
