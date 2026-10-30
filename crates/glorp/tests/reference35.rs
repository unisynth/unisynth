//! Optional external-reference parity: ID35 (second-order all-pass cascade) vs the original engine's output, replayed
//! through the isolated Wine harness (local only: third-party binary + song never enter git). Skipped unless
//! `GLORP_REF_ROOT` points at the `ws-intros/out` directory that holds `389811-2.bin` (read only for the ID 35
//! initial parameters) and `glorp-win64/analysis/harness/out/` with
//!   * `ev_full.csv`, `tapspec.txt`, `taps/devNN.tap`: the song run's recorded taps and parameter writes (device 27);
//!   * `id35vec/vecNN.{csv,tap}`: synthetic vectors the harness fed to a fresh original instance and recorded
//!     (`GLORP_OUT_TAP`, see the research doc); `vecNN.csv` = event log, `vecNN.tap` = inputs + the original's output.
//!
//! Tap block = 8 × 128 f32: in L, in R, aux L, aux R, then out L, out R, aux L, aux R. The original runs in place
//! (the harness passes the same buffers as input and output). Every recorded `SetParam` is applied through
//! `set_param` before the block it is logged in, so the test also proves the recorded automation is accepted.
//! `GLORP_REF_ROOT` unset => the tests below skip explicitly (they do not prove parity). `GLORP_REF_ROOT` set but a
//! fixture file missing => they fail loudly: a misconfigured root must never look like a pass.
use glorp::id35::{Id35, PARAMS};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const B: usize = 128;
/// Song window tolerance (absolute, per sample; peak |output| 3.4). Measured max 3.170e-3: a 255-stage cascade of
/// narrow sections turns last-ulp differences of the stage coefficients (the C library's `sin`/`cos`/`pow`, see the
/// research doc and `reference35_structure.rs`, which is bit-exact with the original's coefficients) into ~1e-3.
/// The measurement depends on the `libm` crate's `pow`/`sin`/`cos` results, i.e. on the version in `Cargo.lock`
/// (0.2.16 when measured), not on an exact `=` pin: a lockfile bump that moves a last ulp means re-measuring the
/// figures here (and the vector tolerances below), not a parity regression by itself.
const SONG_TOL: f64 = 4e-3;

/// Synthetic vectors: bit-exact except the two with continuously automated parameters (`vec05` synthetic sweeps,
/// measured max 1.454e-5; `vec08` the song's own recorded automation of blocks 10200..10499 on noise, measured max
/// 2.007e-4): there the per-block coefficient re-derivation meets the C library's last-ulp differences.
fn vector_tol(name: &str) -> f64 {
    match name {
        "vec05" => 3e-5,
        "vec08" => 4e-4,
        _ => 0.0,
    }
}

struct Stats {
    samples: usize,
    exact: usize,
    max_abs: f64,
    max_ref: f64,
    nonzero: usize,
}

type Events = HashMap<usize, Vec<(usize, f32)>>;

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}

/// Initial parameters of every ID 35 song device: device index -> 6 floats (the 7th chunk float is the byte size).
fn chunks(blob: &[u8]) -> Vec<(usize, [f32; PARAMS])> {
    let u32at = |p: usize| u32::from_le_bytes(blob[p..p + 4].try_into().unwrap()) as usize;
    let n = u32at(16);
    let mut p = 20;
    let mut out = Vec::new();
    for i in 0..n {
        let (id, size) = (blob[p], u32at(p + 1));
        let chunk = &blob[p + 5..p + 5 + size];
        p += 5 + size;
        if id == 35 {
            assert_eq!(size, 28);
            out.push((i, f32s(chunk)[..PARAMS].try_into().unwrap()));
        }
    }
    out
}

/// Replays `nblocks` tap blocks from block `start`. Blocks before `start` only apply their parameter events: the
/// original's input there is silence and an all-pass cascade with zero state maps zero to zero, so processing them
/// would change nothing (unit-tested: `silence_in_silence_out_and_the_ringing_decays`).
fn replay(label: &str, init: [f32; PARAMS], start: usize, nblocks: usize, tap: &[u8], events: &Events, tol: f64) -> Stats {
    let mut d = Id35::new(&init).unwrap_or_else(|e| panic!("{label}: {e:?}"));
    let mut s = Stats { samples: 0, exact: 0, max_abs: 0.0, max_ref: 0.0, nonzero: 0 };
    for b in 0..start + nblocks {
        for &(i, v) in events.get(&b).map(|v| v.as_slice()).unwrap_or(&[]) {
            d.set_param(i, v).unwrap_or_else(|e| panic!("{label} block {b}: param {i}={v} rejected: {e:?}"));
        }
        if b < start {
            continue;
        }
        let o = (b - start) * 8 * B * 4;
        let blk = f32s(&tap[o..o + 8 * B * 4]);
        assert_eq!(blk[2 * B..4 * B], blk[6 * B..8 * B], "{label} block {b}: the original touched the aux pair");
        let (mut l, mut r) = (blk[..B].to_vec(), blk[B..2 * B].to_vec());
        d.process(&mut l, &mut r);
        for (c, got) in [l, r].iter().enumerate() {
            for i in 0..B {
                let want = blk[(4 + c) * B + i];
                let e = (got[i] as f64 - want as f64).abs();
                s.samples += 1;
                s.max_abs = s.max_abs.max(e);
                s.max_ref = s.max_ref.max(want.abs() as f64);
                s.nonzero += (want != 0.0) as usize;
                s.exact += (got[i] == want) as usize;
                assert!(
                    e <= tol,
                    "{label} block {b} ch {c} i {i}: got {} want {want}",
                    got[i]
                );
            }
        }
    }
    s
}

/// `None` = `GLORP_REF_ROOT` unset (explicit skip). Set but unusable = panic, so a typo cannot pass silently.
fn root() -> Option<PathBuf> {
    let r = PathBuf::from(std::env::var_os("GLORP_REF_ROOT")?);
    assert!(r.is_dir(), "GLORP_REF_ROOT={} is not a directory", r.display());
    Some(r)
}

fn song_fixture() -> Option<(PathBuf, Vec<u8>)> {
    let root = root()?;
    let bin = root.join("389811-2.bin");
    let blob = std::fs::read(&bin).unwrap_or_else(|e| panic!("GLORP_REF_ROOT is set but {} is unreadable: {e}", bin.display()));
    let dir = root.join("glorp-win64/analysis/harness/out");
    for f in ["ev_full.csv", "tapspec.txt"] {
        assert!(dir.join(f).is_file(), "GLORP_REF_ROOT is set but {} is missing", dir.join(f).display());
    }
    Some((dir, blob))
}

fn events_of(path: &Path, dev: usize) -> Events {
    let mut m: Events = HashMap::new();
    for line in BufReader::new(std::fs::File::open(path).unwrap()).lines() {
        let line = line.unwrap();
        let f: Vec<&str> = line.split(',').collect();
        if f.len() == 7 && f[3] == "param" && f[1].parse() == Ok(dev) {
            let bits: u32 = f[6].parse().unwrap();
            m.entry(f[0].parse().unwrap()).or_default().push((f[4].parse().unwrap(), f32::from_bits(bits)));
        }
    }
    m
}

fn specs(dir: &Path) -> HashMap<usize, (usize, usize)> {
    std::fs::read_to_string(dir.join("tapspec.txt"))
        .unwrap()
        .lines()
        .filter_map(|l| {
            let a: Vec<usize> = l.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            (a.len() == 3).then(|| a[2].checked_sub(a[1]).map(|n| (a[0], (a[1], n)))).flatten()
        })
        .collect()
}

fn report(label: &str, s: &Stats) {
    eprintln!(
        "{label}: {} samples, {} exact ({} nonzero refs), max |ref| {:.4}, max abs err {:.3e}",
        s.samples, s.exact, s.nonzero, s.max_ref, s.max_abs
    );
}

#[test]
fn id35_matches_the_recorded_song_tap() {
    let Some((dir, blob)) = song_fixture() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    let specs = specs(&dir);
    let mut checked = 0;
    for (dev, init) in chunks(&blob) {
        let Some(&(start, n)) = specs.get(&dev) else { continue };
        let tap = std::fs::read(dir.join(format!("taps/dev{dev:02}.tap"))).unwrap();
        let ev = events_of(&dir.join("ev_full.csv"), dev);
        let s = replay(&format!("song dev {dev}"), init, start, n, &tap, &ev, SONG_TOL);
        report(&format!("song dev {dev}"), &s);
        assert!(s.nonzero > 100_000, "dev {dev}: window too quiet to mean anything");
        checked += 1;
    }
    assert_eq!(checked, 1, "the song has one ID 35 instance (27)");
}

fn vectors(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir.join("id35vec"))
        .map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "tap") && !p.to_string_lossy().ends_with(".in.tap")).collect())
        .unwrap_or_default();
    v.sort();
    v
}

#[test]
fn id35_matches_the_original_on_synthetic_vectors() {
    let Some((dir, blob)) = song_fixture() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    let (dev, init) = chunks(&blob).remove(0);
    let vecs = vectors(&dir);
    assert!(vecs.len() >= 8, "expected the generated id35vec/vec*.tap set, found {}", vecs.len());
    for tap_path in vecs {
        let tap = std::fs::read(&tap_path).unwrap();
        let ev = events_of(&tap_path.with_extension("csv"), dev);
        let n = tap.len() / (8 * B * 4);
        let name = tap_path.file_stem().unwrap().to_string_lossy().into_owned();
        let s = replay(&name, init, 0, n, &tap, &ev, vector_tol(&name));
        report(&name, &s);
        assert!(s.nonzero > 1_000, "{name}: reference output is silent");
    }
}

/// Negative control: one flipped bit in the reference must leave the tolerance, i.e. `replay` must panic with its
/// `got .. want ..` mismatch message (not with an unrelated unwrap/index failure).
#[test]
fn negative_control_flipped_reference_bit_is_caught() {
    let Some((dir, blob)) = song_fixture() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    let specs = specs(&dir);
    let (dev, init) = chunks(&blob).into_iter().find(|(d, _)| specs.contains_key(d)).unwrap();
    let (start, _) = specs[&dev];
    let mut tap = std::fs::read(dir.join(format!("taps/dev{dev:02}.tap"))).unwrap();
    let ev = events_of(&dir.join("ev_full.csv"), dev);
    let blk = (0..600).find(|b| tap[b * 8 * B * 4..b * 8 * B * 4 + B * 4].iter().any(|&x| x != 0)).unwrap();
    let o = (blk * 8 * B + 4 * B + 5) * 4 + 3;
    tap[o] ^= 0x40;
    let err = std::panic::catch_unwind(|| replay("flip", init, start, blk + 2, &tap, &ev, SONG_TOL))
        .err()
        .expect("a flipped reference bit stayed inside the song tolerance");
    let msg = err.downcast_ref::<String>().map(String::as_str).or_else(|| err.downcast_ref::<&str>().copied()).unwrap_or("");
    assert!(msg.contains("flip block") && msg.contains("got"), "panicked for another reason: {msg}");
}

/// Negative control: the same song window replayed with every write of one parameter nudged must leave the tolerance.
#[test]
fn negative_control_wrong_parameters_are_caught() {
    let Some((dir, blob)) = song_fixture() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    let specs = specs(&dir);
    let (dev, init0) = chunks(&blob).into_iter().find(|(d, _)| specs.contains_key(d)).unwrap();
    let (start, n) = specs[&dev];
    let tap = std::fs::read(dir.join(format!("taps/dev{dev:02}.tap"))).unwrap();
    // (parameter, delta): P0 +1/255 (centre frequency), P4 +1/255 (scatter), P1 +1/255 (Q), P2 -1/255 (mix), P3 -1/256 (one stage)
    for (param, delta) in [(0, 1.0 / 255.0), (4, 1.0 / 255.0), (1, 1.0 / 255.0), (2, -1.0 / 255.0), (3, -1.0 / 256.0)] {
        let mut init = init0;
        init[param] = (init[param] + delta).clamp(0.0, 1.0);
        let mut ev = events_of(&dir.join("ev_full.csv"), dev);
        for e in ev.values_mut().flatten().filter(|e| e.0 == param) {
            e.1 = (e.1 + delta).clamp(0.0, 1.0);
        }
        let caught = std::panic::catch_unwind(|| replay("wrong", init, start, n, &tap, &ev, SONG_TOL)).is_err();
        assert!(caught, "a nudge of P{param} by {delta} stayed inside the song tolerance");
    }
}

#[test]
fn event_automation_replay_is_what_the_song_does() {
    // the recorded log re-sends every automated parameter each block: check that this is really exercised
    let Some((dir, _)) = song_fixture() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    let ev = events_of(&dir.join("ev_full.csv"), 27);
    let writes: usize = ev.values().map(|v| v.len()).sum();
    let blocks_with: usize = ev.len();
    let mut params = [false; PARAMS];
    for v in ev.values().flatten() {
        params[v.0] = true;
    }
    eprintln!("dev 27: {writes} param writes over {blocks_with} blocks, params touched {params:?}");
    assert!(writes > 1000 && blocks_with > 1000, "automation log looks empty");
    assert!(params[..5].iter().all(|&p| p) && !params[5], "song automates P0..P4 and never P5");
}
