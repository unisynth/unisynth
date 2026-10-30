//! Optional external-reference parity: ID 7 (stereo look-ahead compressor / limiter) vs the original engine's output,
//! replayed through the isolated Wine harness (local only: third-party binary + song never enter git).
//!
//! `GLORP_REF_ROOT` = the `ws-intros/out` directory that holds `389811-2.bin` (read only for the ID 7 initial
//! parameters) and `glorp-win64/analysis/harness/out/` with
//!   * `ev_full.csv`, `tapspec.txt`, `taps/dev01.tap`: the song run's recorded tap (song device 1, blocks 0..600) and
//!     parameter writes (none for device 1: the song never automates it);
//!   * `id7vec/manifest.json`, `id7vec/vecNN.{csv,tap}` (NN = 01..11): synthetic vectors the harness fed to a fresh
//!     original instance and recorded (`GLORP_OUT_TAP`, research doc §ID 7); `vecNN.csv` = event log (all 7 parameters
//!     at block 0, later per-block changes), `vecNN.tap` = inputs + the original's output. The song may be silent or
//!     barely active, so the vectors are the evidence for everything the song does not exercise.
//!
//! Tap block = 8 × 128 f32: in L, in R, aux L, aux R, then out L, out R, aux L, aux R. The original runs in place
//! (same buffers for input and output); the aux pair is the side-chain input when P0 ≥ 0.5, and is never written.
//! Every recorded `SetParam` is applied through `set_param` before the block it is logged in, so the test also proves
//! the recorded automation is accepted. `GLORP_REF_ROOT` unset => the tests below skip explicitly (they do not
//! prove parity). `GLORP_REF_ROOT` set but a fixture file missing => they fail loudly: a misconfigured root must
//! never look like a pass (`missing_fixtures_are_a_hard_error` proves that path without needing the env var).
use glorp::id7::{Id7, PARAMS};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const B: usize = 128;
const VECTORS: usize = 11;

/// Per-sample bound `ABS + REL·|ref|`, one for every fixture. Measured on the whole song window (153,600 samples) and all
/// ten synthetic vectors (~430,000 samples): **bit-exact** (max error 0), with `libm` 0.2.x `pow` here. The bound only
/// absorbs the last-ulp (6e-8 relative) difference a different C library's `powf` makes in the three gain factors;
/// a 1/255 nudge of any parameter is far outside it (negative controls below).
const ABS: f64 = 1e-7;
const REL: f64 = 3e-7;

struct Stats {
    samples: usize,
    exact: usize,
    max_abs: f64,
    max_ref: f64,
    nonzero: usize,
    /// Samples whose output differs from the uncompressed path (87-sample delayed input × input gain × make-up gain,
    /// from the current P1/P6) by more than 1e-3 relative: gain reduction visible.
    shaped: usize,
    /// Reference samples that are NaN / infinite (the replay must match them as such).
    nonfinite: usize,
}

type Events = HashMap<usize, Vec<(usize, f32)>>;

struct Fixtures {
    dir: PathBuf,
    init: [f32; PARAMS],
}

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}

/// Initial parameters of the ID 7 song device and its index: `(device, 7 floats)` (the 8th chunk float is the byte size).
fn chunk(blob: &[u8]) -> Result<(usize, [f32; PARAMS]), String> {
    let u32at = |p: usize| u32::from_le_bytes(blob[p..p + 4].try_into().unwrap()) as usize;
    let n = u32at(16);
    let mut p = 20;
    let mut found = Vec::new();
    for i in 0..n {
        let (id, size) = (blob[p], u32at(p + 1));
        let c = &blob[p + 5..p + 5 + size];
        p += 5 + size;
        if id == 7 {
            if size != 32 {
                return Err(format!("ID 7 chunk has {size} bytes, expected 32"));
            }
            found.push((i, f32s(c)[..PARAMS].try_into().unwrap()));
        }
    }
    if found.len() != 1 {
        return Err(format!("expected exactly one ID 7 instance in the song, found {}", found.len()));
    }
    Ok(found.remove(0))
}

/// Every fixture file this suite reads; any missing one is an error naming it.
fn load(root: &Path) -> Result<(Fixtures, usize), String> {
    if !root.is_dir() {
        return Err(format!("GLORP_REF_ROOT={} is not a directory", root.display()));
    }
    let dir = root.join("glorp-win64/analysis/harness/out");
    let mut need = vec![
        root.join("389811-2.bin"),
        dir.join("ev_full.csv"),
        dir.join("tapspec.txt"),
        dir.join("taps/dev01.tap"),
        dir.join("id7vec/manifest.json"),
    ];
    for n in 1..=VECTORS {
        need.push(dir.join(format!("id7vec/vec{n:02}.csv")));
        need.push(dir.join(format!("id7vec/vec{n:02}.tap")));
    }
    for f in &need {
        if !f.is_file() {
            return Err(format!("GLORP_REF_ROOT is set but fixture {} is missing", f.display()));
        }
    }
    let blob = std::fs::read(root.join("389811-2.bin")).map_err(|e| e.to_string())?;
    let (dev, init) = chunk(&blob)?;
    Ok((Fixtures { dir, init }, dev))
}

/// `None` = `GLORP_REF_ROOT` unset (explicit skip). Set but unusable = panic, so a typo cannot pass silently.
fn fixtures() -> Option<(Fixtures, usize)> {
    let root = PathBuf::from(std::env::var_os("GLORP_REF_ROOT")?);
    Some(load(&root).unwrap_or_else(|e| panic!("{e}")))
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

/// Replays every block of `tap` from block 0 and checks every sample against the bound above. Calls `process_sidechain`
/// exactly when the device is in side-chain mode (`force_internal` = negative control: ignore P0 writes and the aux pair).
fn replay(label: &str, init: [f32; PARAMS], events: &Events, tap: &[u8], force_internal: bool) -> Stats {
    let mut d = Id7::new(&init).unwrap_or_else(|e| panic!("{label}: {e:?}"));
    let mut s = Stats { samples: 0, exact: 0, max_abs: 0.0, max_ref: 0.0, nonzero: 0, shaped: 0, nonfinite: 0 };
    let mut cur = init;
    let mut hist: [Vec<f32>; 2] = [Vec::new(), Vec::new()];
    let nblocks = tap.len() / (8 * B * 4);
    assert_eq!(tap.len(), nblocks * 8 * B * 4, "{label}: tap is not a whole number of blocks");
    for b in 0..nblocks {
        for &(i, v) in events.get(&b).map(|v| v.as_slice()).unwrap_or(&[]) {
            let v = if force_internal && i == 0 { 0.0 } else { v };
            cur[i] = v;
            d.set_param(i, v).unwrap_or_else(|e| panic!("{label} block {b}: param {i}={v} rejected: {e:?}"));
        }
        let o = b * 8 * B * 4;
        let blk = f32s(&tap[o..o + 8 * B * 4]);
        // bitwise: the aux pair may hold NaN (vec11), and NaN != NaN
        let bits = |r: std::ops::Range<usize>| blk[r].iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(2 * B..4 * B), bits(6 * B..8 * B), "{label} block {b}: the original touched the aux pair");
        let (mut l, mut r) = (blk[..B].to_vec(), blk[B..2 * B].to_vec());
        let res = if d.sidechain() && !force_internal {
            d.process_sidechain(&mut l, &mut r, &blk[2 * B..3 * B], &blk[3 * B..4 * B])
        } else {
            d.process(&mut l, &mut r)
        };
        res.unwrap_or_else(|e| panic!("{label} block {b}: process refused: {e:?}"));
        let gain = |db: f32| 2f64.powf((2.0 * db as f64 - 1.0) * 2.0);
        let plain = gain(cur[1]) * gain(cur[6]);
        for (c, got) in [l, r].iter().enumerate() {
            hist[c].extend_from_slice(&blk[c * B..(c + 1) * B]);
            for i in 0..B {
                let want = blk[(4 + c) * B + i];
                let (g, w) = (got[i] as f64, want as f64);
                s.samples += 1;
                // NaN must match NaN exactly (bit pattern aside) and an infinity must match its sign
                let same_nonfinite = !w.is_finite() && !g.is_finite() && (w.is_nan() == g.is_nan()) && (w.is_nan() || w == g);
                let e = if same_nonfinite { 0.0 } else { (g - w).abs() };
                if e.is_finite() {
                    s.max_abs = s.max_abs.max(e);
                }
                if w.is_finite() {
                    s.max_ref = s.max_ref.max(w.abs());
                }
                s.nonzero += (want != 0.0) as usize;
                s.exact += (got[i].to_bits() == want.to_bits() || same_nonfinite) as usize;
                let n = hist[c].len() - B + i;
                let unshaped = n.checked_sub(87).map_or(0.0, |k| hist[c][k] as f64 * plain);
                s.shaped += ((g - unshaped).abs() > 1e-3 * unshaped.abs().max(1e-3)) as usize;
                s.nonfinite += !w.is_finite() as usize;
                let bound = if w.is_finite() { ABS + REL * w.abs() } else { 0.0 };
                assert!(e <= bound, "{label} block {b} ch {c} i {i}: got {} want {want}", got[i]);
            }
        }
    }
    s
}

fn report(label: &str, s: &Stats) {
    eprintln!(
        "{label}: {} samples, {} exact ({} nonzero refs, {} non-finite), max |ref| {:.4}, max abs err {:.3e}, {} shaped",
        s.samples, s.exact, s.nonzero, s.nonfinite, s.max_ref, s.max_abs, s.shaped
    );
}

fn song_tap(fx: &Fixtures, dev: usize) -> Vec<u8> {
    let specs = std::fs::read_to_string(fx.dir.join("tapspec.txt")).unwrap();
    let ok = specs.lines().any(|l| {
        let a: Vec<usize> = l.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        a == [dev, 0, 600]
    });
    assert!(ok, "tapspec.txt has no `{dev} 0 600` row: the ID 7 tap is not the first 600 blocks");
    let tap = std::fs::read(fx.dir.join(format!("taps/dev{dev:02}.tap"))).unwrap();
    assert_eq!(tap.len(), 600 * 8 * B * 4, "dev{dev:02}.tap has an unexpected size");
    tap
}

fn vector(fx: &Fixtures, n: usize) -> (String, Events, Vec<u8>) {
    let name = format!("vec{n:02}");
    let tap = std::fs::read(fx.dir.join(format!("id7vec/{name}.tap"))).unwrap();
    let manifest = std::fs::read_to_string(fx.dir.join("id7vec/manifest.json")).unwrap();
    assert!(manifest.contains(&format!("\"{name}\"")), "{name} missing from manifest.json");
    (name.clone(), events_of(&fx.dir.join(format!("id7vec/{name}.csv")), 1), tap)
}

/// The fixture loader must refuse an incomplete root, naming the missing file: this runs without the env var.
#[test]
fn missing_fixtures_are_a_hard_error() {
    let base = std::env::temp_dir().join(format!("glorp7-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    // not a directory
    let e = load(&base.join("nope")).err().expect("a non-directory root must fail");
    assert!(e.contains("not a directory"), "{e}");
    // empty root: the song blob is the first missing file
    std::fs::create_dir_all(&base).unwrap();
    let e = load(&base).err().expect("an empty root must fail");
    assert!(e.contains("389811-2.bin") && e.contains("missing"), "{e}");
    // song blob present, harness outputs absent
    std::fs::write(base.join("389811-2.bin"), b"x").unwrap();
    let e = load(&base).err().expect("a root without the harness out dir must fail");
    assert!(e.contains("ev_full.csv") && e.contains("missing"), "{e}");
    // all but one synthetic vector present: that exact file is named
    let out = base.join("glorp-win64/analysis/harness/out");
    std::fs::create_dir_all(out.join("taps")).unwrap();
    std::fs::create_dir_all(out.join("id7vec")).unwrap();
    for f in ["ev_full.csv", "tapspec.txt", "taps/dev01.tap", "id7vec/manifest.json"] {
        std::fs::write(out.join(f), b"").unwrap();
    }
    for n in 1..=VECTORS {
        std::fs::write(out.join(format!("id7vec/vec{n:02}.csv")), b"").unwrap();
        if n != 7 {
            std::fs::write(out.join(format!("id7vec/vec{n:02}.tap")), b"").unwrap();
        }
    }
    let e = load(&base).err().expect("a missing vector must fail");
    assert!(e.contains("vec07.tap") && e.contains("missing"), "{e}");
    let _ = std::fs::remove_dir_all(&base);
}

/// A song blob without exactly one ID 7 chunk is refused rather than guessed at.
#[test]
fn chunk_parser_refuses_a_song_without_exactly_one_id7() {
    let mut blob = vec![0u8; 20];
    blob[16..20].copy_from_slice(&1u32.to_le_bytes());
    blob.push(38);
    blob.extend(4u32.to_le_bytes());
    blob.extend([0u8; 4]);
    assert!(chunk(&blob).err().unwrap().contains("found 0"));
    let mut two = vec![0u8; 20];
    two[16..20].copy_from_slice(&2u32.to_le_bytes());
    for _ in 0..2 {
        two.push(7);
        two.extend(32u32.to_le_bytes());
        two.extend([0u8; 32]);
    }
    assert!(chunk(&two).err().unwrap().contains("found 2"));
    let mut bad = vec![0u8; 20];
    bad[16..20].copy_from_slice(&1u32.to_le_bytes());
    bad.push(7);
    bad.extend(8u32.to_le_bytes());
    bad.extend([0u8; 8]);
    assert!(chunk(&bad).err().unwrap().contains("expected 32"));
}

#[test]
fn id7_matches_the_recorded_song_tap() {
    let Some((fx, dev)) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    assert_eq!(dev, 1, "the song's ID 7 instance is device 1");
    let ev = events_of(&fx.dir.join("ev_full.csv"), dev);
    assert!(ev.is_empty(), "the song log has parameter writes for device {dev}: this test assumes it never automates ID 7");
    let tap = song_tap(&fx, dev);
    let s = replay("song dev 1", fx.init, &ev, &tap, false);
    report("song dev 1", &s);
    assert!(s.nonzero > 100_000, "window too quiet to mean anything");
    assert!(s.shaped > 10_000, "the compressor never engaged in the song window ({} shaped samples)", s.shaped);
}

#[test]
fn id7_matches_the_original_on_synthetic_vectors() {
    let Some((fx, _)) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    for n in 1..=VECTORS {
        let (name, ev, tap) = vector(&fx, n);
        let s = replay(&name, fx.init, &ev, &tap, false);
        report(&name, &s);
        assert!(s.nonzero > 10 && s.samples > 0, "{name}: reference output is silent");
    }
}

/// The vectors must exercise what they claim: a vector set that never reaches the compressor proves nothing.
#[test]
fn vectors_exercise_compression_sidechain_and_automation() {
    let Some((fx, _)) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    let shaped = |n: usize| {
        let (name, ev, tap) = vector(&fx, n);
        replay(&name, fx.init, &ev, &tap, false).shaped
    };
    for n in [1, 3, 4, 5, 6, 7] {
        assert!(shaped(n) > 5_000, "vec{n:02} never compresses");
    }
    assert_eq!(shaped(2), 0, "vec02 is a pure delay");
    let (_, ev7, _) = vector(&fx, 7);
    assert!(ev7.len() >= 299 && ev7.values().map(|v| v.len()).sum::<usize>() >= 1800, "vec07 automation log looks empty");
    let (_, ev11, _) = vector(&fx, 11);
    assert!(ev11.get(&60).is_some_and(|v| v.contains(&(0, 1.0))), "vec11 side-chain switch missing");
    let (_, ev6, _) = vector(&fx, 6);
    assert!(ev6.get(&120).is_some_and(|v| v.contains(&(0, 0.4999))) && ev6.get(&180).is_some_and(|v| v.contains(&(0, 0.5))), "vec06 P0 toggles missing");
}

/// Runs `f`, which must panic with `replay`'s `"<label> block .. got .. want .."` mismatch message (not with an unrelated
/// unwrap / index / not-implemented failure, so a broken control cannot pass for the wrong reason).
fn must_mismatch(label: &str, what: &str, f: impl FnOnce() + std::panic::UnwindSafe) {
    let err = std::panic::catch_unwind(f).err().unwrap_or_else(|| panic!("{what}: stayed inside the tolerance"));
    let msg = err.downcast_ref::<String>().map(String::as_str).or_else(|| err.downcast_ref::<&str>().copied()).unwrap_or("");
    assert!(msg.starts_with(&format!("{label} block")) && msg.contains(" got ") && msg.contains(" want "), "{what}: panicked for another reason: {msg}");
}

/// Negative control: one flipped bit in the reference must leave the tolerance.
#[test]
fn negative_control_flipped_reference_bit_is_caught() {
    let Some((fx, dev)) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    let mut tap = song_tap(&fx, dev);
    // a nonzero output sample, one mantissa bit near the top (relative change ~1e-3..6e-1)
    let blk = (0..600).find(|b| tap[(b * 8 * B + 4 * B + 5) * 4..][..4] != [0, 0, 0, 0]).unwrap();
    tap[(blk * 8 * B + 4 * B + 5) * 4 + 2] ^= 0x40;
    must_mismatch("flip", "a flipped reference bit", || {
        replay("flip", fx.init, &HashMap::new(), &tap, false);
    });
}

/// Negative control: the song window with the chunk value of one parameter nudged by 1/255 must leave the tolerance. P1..P6
/// all act in the window (compressing, threshold 1.0, hot input), P0 is the detector source (internal in the song) and
/// is covered by the side-chain control below.
#[test]
fn negative_control_wrong_parameters_are_caught() {
    let Some((fx, dev)) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    let tap = song_tap(&fx, dev);
    for param in 1..PARAMS {
        let mut init = fx.init;
        // P2 is 1.0 in the song (the top of its range): nudge it downwards
        let d = if init[param] + 1.0 / 255.0 > 1.0 { -1.0 / 255.0 } else { 1.0 / 255.0 };
        init[param] += d;
        must_mismatch("wrong", &format!("a nudge of P{param} by {d}"), || {
            replay("wrong", init, &HashMap::new(), &tap, false);
        });
    }
}

/// Negative control: dropping the recorded automation (vec07), treating the side-chain vector as internal (vec06),
/// or a reference shifted by one sample (vec02: an 86- / 88-sample delay) must all be caught.
#[test]
fn negative_control_dropped_automation_wrong_detector_and_delay_are_caught() {
    let Some((fx, _)) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    let (_, ev7, tap7) = vector(&fx, 7);
    let first: Events = ev7.iter().filter(|(b, _)| **b == 0).map(|(b, v)| (*b, v.clone())).collect();
    must_mismatch("dropped", "dropped automation", || {
        replay("dropped", fx.init, &first, &tap7, false);
    });
    let (_, ev6, tap6) = vector(&fx, 6);
    must_mismatch("internal", "ignoring the side-chain", || {
        replay("internal", fx.init, &ev6, &tap6, true);
    });
    let (_, ev2, mut tap2) = vector(&fx, 2);
    for b in 0..tap2.len() / (8 * B * 4) {
        let o = (b * 8 * B + 4 * B) * 4;
        tap2.copy_within(o + 4..o + B * 4, o);
    }
    must_mismatch("delay", "a reference one sample early", || {
        replay("delay", fx.init, &ev2, &tap2, false);
    });
}
