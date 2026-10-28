use super::write::{write, ChanW, SongW};
use super::*;
use std::path::PathBuf;
use std::process::Command;

fn patch(over: &[(usize, u8)], mods: &[(u8, u8, u8)]) -> Patch {
    let mut params = PATCH_DEFAULTS;
    for &(i, v) in over {
        params[i] = v;
    }
    Patch { params, mods: mods.to_vec() }
}

fn phrase(patches: Vec<Patch>) -> SongW {
    let mut s = SongW::new(patches);
    s.chans[0] = ChanW { notes: vec![(0, 60, 100), (48, 60, 0), (96, 64, 90), (96, 67, 90), (192, 64, 0), (192, 67, 0)], ..Default::default() };
    s.chans[0].ctls[6] = vec![(0, 127), (100, 64)];
    // V2 reads the bend planes at the program-change width: they decode right only when both counts are equal
    s.chans[0].programs = vec![(0, 0), (5, 0)];
    s.chans[0].bends = vec![(10, 0, 64), (20, 0, 80)];
    s.chans[2] = ChanW { notes: vec![(24, 36, 127), (72, 36, 0)], programs: vec![(0, 1)], ..Default::default() };
    s.speech = vec![b"!hello".to_vec(), b"world".to_vec()];
    s
}

#[test]
fn v6_round_trip() {
    let p0 = patch(&[(2, 3), (21, 90)], &[(0, 127, 21), (10, 70, 1)]);
    let p1 = patch(&[(88, 4)], &[]);
    let b = write(&phrase(vec![p0.clone(), p1.clone()]));
    let s = parse(&b).unwrap();
    assert_eq!(s.version, 6);
    assert_eq!(s.patches, vec![p0, p1]);
    assert_eq!(s.globals, GLOBAL_DEFAULTS);
    assert_eq!(s.speech, vec![b"!hello".to_vec(), b"world".to_vec()]);
    assert_eq!(&s.bytes[..b.len()], &b[..]);
    assert_eq!(&s.bytes[b.len()..], &[0; 4]);
}

#[test]
fn old_versions_upgrade_with_defaults() {
    for v in 0..=5u8 {
        // a patch in version-v numbering: every stored param = 1 + its v6 index (mod 128), one mod on the last
        // stored param of that version
        let stored: Vec<usize> = (0..PATCH_PARAMS).filter(|&i| PATCH_PARAM_VER[i] <= v).collect();
        let mut p6 = PATCH_DEFAULTS;
        for &i in &stored {
            p6[i] = (1 + i as u8) % 128;
        }
        let last_old = (stored.len() - 1) as u8;
        let mut w = phrase(vec![Patch { params: p6, mods: vec![(1, 100, last_old), (2, 30, 0)] }; 2]);
        w.version = v;
        w.speech.clear();
        let s = parse(&write(&w)).unwrap();
        assert_eq!(s.version, v, "version {v}");
        assert_eq!(s.patches[0].params, p6, "version {v}");
        // the destination is remapped to the v6 index of the same parameter
        assert_eq!(s.patches[0].mods, vec![(1, 100, *stored.last().unwrap() as u8), (2, 30, 0)], "version {v}");
        let mut g = GLOBAL_DEFAULTS;
        g[2] = if v >= 4 { g[2] } else { 32 };
        assert_eq!(s.globals, g);
    }
}

#[test]
fn truncation_never_panics() {
    let b = write(&phrase(vec![patch(&[], &[(0, 127, 21)]), patch(&[], &[])]));
    for n in 0..b.len() {
        let _ = parse(&b[..n]);
    }
    let mut w = phrase(vec![patch(&[], &[]); 2]);
    w.version = 1;
    let b = write(&w);
    for n in 0..b.len() {
        let _ = parse(&b[..n]);
    }
}

#[test]
fn unknown_version_names_sizes() {
    let b0 = write(&phrase(vec![patch(&[], &[]); 2]));
    // globals block cut from 23 to 17 bytes: matches no version
    let l = layout(&b0).unwrap();
    let mut b = b0[..l.midi_end].to_vec();
    b.extend(17u32.to_le_bytes());
    b.extend(&b0[l.glob.0..l.glob.0 + 17]);
    b.extend(&b0[l.glob.0 + 23..]);
    let e = parse(&b).unwrap_err().to_string();
    assert!(e.contains("unknown format version") && e.contains("17"), "{e}");
    assert!(parse(&[0; 8]).is_err());
}

#[test]
fn timeline_of_a_phrase() {
    // 96 ticks per quarter at 500 000 us -> 1 tick = 1/192 s = 229.6875 frames at 44.1 kHz
    let s = parse(&write(&phrase(vec![patch(&[], &[]); 2]))).unwrap();
    let t = s.timeline(44_100);
    let notes: Vec<_> = t.events.iter().filter(|e| matches!(e.msg, Msg::Note { .. }) && e.chan == 0).map(|e| (e.frame, e.msg)).collect();
    assert_eq!(notes[0], (0, Msg::Note { key: 60, vel: 100 }));
    // V2 truncates each segment (its remainder only carries on u32 overflow), so frames land at or a few below the
    // exact positions; the oracle test checks the exact values against V2's own sequencer
    let near = |f: u64, ticks: f64| (ticks * 229.6875 - f as f64).abs() < 8.0;
    assert!(near(notes[1].0, 48.0), "{}", notes[1].0);
    assert!(near(notes[2].0, 96.0), "{}", notes[2].0);
    assert_eq!(t.events.iter().find(|e| e.chan == 2).unwrap().msg, Msg::Program(1));
    let bends: Vec<_> = t.events.iter().filter(|e| matches!(e.msg, Msg::Bend { .. })).map(|e| e.msg).collect();
    assert_eq!(bends, vec![Msg::Bend { lo: 0, hi: 64 }, Msg::Bend { lo: 0, hi: 80 }]);
    assert!(near(t.end_frame, 192.0), "{}", t.end_frame);
}

// ---- oracle tests against v2redux (skipped when the corpus or the oracles are absent) ----

fn songs_dir() -> PathBuf {
    std::env::var_os("V2_SONGS").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Downloads/64k-tools/synth-lab/v2songs")
    })
}

fn corpus(sub: &[&str]) -> Vec<PathBuf> {
    let mut v = vec![];
    for d in sub {
        if let Ok(rd) = std::fs::read_dir(songs_dir().join(d)) {
            v.extend(rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "v2m")));
        }
    }
    v.sort();
    v
}

fn oracle(name: &str) -> Option<PathBuf> {
    let p = songs_dir().join(".bin").join(name);
    if p.exists() {
        Some(p)
    } else {
        eprintln!("skip: {} absent (run `bun tools/v2-extract/v2.ts oracles`)", p.display());
        None
    }
}

#[test]
fn upgrade_matches_v2redux_on_the_corpus() {
    let Some(dump) = oracle("v2dumpjson") else { return };
    let songs = corpus(&["acceptance", "fr_public", "modland"]);
    let tmp = std::env::temp_dir().join(format!("v2canon-{}", std::process::id()));
    let (mut checked, mut both_reject) = (0, 0);
    for p in &songs {
        let out = Command::new(&dump).arg(p).arg("--canon").arg(&tmp).output().unwrap();
        let js = String::from_utf8_lossy(&out.stdout);
        let ours = canonicalize(&std::fs::read(p).unwrap());
        if js.contains("\"error\"") {
            assert!(ours.is_err(), "{}: v2redux rejects, we accept", p.display());
            both_reject += 1;
            continue;
        }
        let (ver, bytes) = ours.unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert!(js.contains(&format!("\"version\":{ver},")), "{}: version {ver} vs {js:.40}", p.display());
        assert!(std::fs::read(&tmp).unwrap() == bytes, "{}: canonical bytes differ", p.display());
        parse(&std::fs::read(p).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        checked += 1;
    }
    let _ = std::fs::remove_file(&tmp);
    eprintln!("upgrade oracle: {checked} songs byte-identical, {both_reject} rejected by both");
}

#[test]
fn timeline_matches_the_v2_sequencer() {
    let (Some(dump), Some(seq)) = (oracle("v2dumpjson"), oracle("v2events")) else { return };
    let tmp = std::env::temp_dir().join(format!("v2seq-{}", std::process::id()));
    for p in corpus(&["acceptance"]) {
        Command::new(&dump).arg(&p).arg("--canon").arg(&tmp).output().unwrap();
        let out = String::from_utf8(Command::new(&seq).arg(&tmp).output().unwrap().stdout).unwrap();
        let s = parse(&std::fs::read(&p).unwrap()).unwrap();
        let t = s.timeline(44_100);
        let mut ours = vec![];
        for e in &t.events {
            let (st, d): (u8, Vec<u8>) = match e.msg {
                Msg::Note { key, vel } => (0x90, vec![key, vel]),
                Msg::Program(p) => (0xc0, vec![p]),
                Msg::Ctl { ctl, val } => (0xb0, vec![ctl, val]),
                Msg::Bend { lo, hi } => (0xe0, vec![lo, hi]),
            };
            ours.push(format!("{} {}{}", e.frame, st | e.chan, d.iter().map(|x| format!(" {x}")).collect::<String>()));
        }
        ours.push(format!("end {}", t.end_frame));
        let theirs: Vec<&str> = out.lines().collect();
        assert_eq!(ours.len(), theirs.len(), "{}: event count", p.display());
        for (i, (a, b)) in ours.iter().zip(&theirs).enumerate() {
            assert_eq!(a, b, "{}: event {i}", p.display());
        }
    }
    let _ = std::fs::remove_file(&tmp);
}

