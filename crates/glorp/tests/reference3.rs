//! Optional external-reference parity: ID 3 (waveshaper / saturator) vs the original engine's output, replayed through the
//! isolated Wine harness (local only: third-party binary + song never enter git).
//!
//! `GLORP_REF_ROOT` = the `ws-intros/out` directory that holds `389811-2.bin` (read only for the ID 3 initial parameters)
//! and `glorp-win64/analysis/harness/out/` with
//!   * `ev_full.csv`, `tapspec.txt`, `taps/dev00.tap`: the song run's first 600 blocks of song device 0 (the one ID 3
//!     instance) and the parameter writes of the whole song (none for device 0: the song never automates it);
//!   * `tapspec3.txt`, `taps3/dev00.tap`: the **whole song** (78,292 blocks, 320,684,032 bytes) of device 0, recorded by
//!     the harness (`--tapspec`, research doc §ID 3);
//!   * `id3ref/manifest.json`, `id3ref/vecNN.{csv,tap}` (NN = 01..13): synthetic vectors the harness fed to a fresh original
//!     instance (`GLORP_OUT_TAP`); `vecNN.csv` = event log (all 11 parameters at block 0, later per-block changes), `vecNN.tap`
//!     = inputs + the original's output.
//!
//! Tap block = 8 × 128 f32: in L, in R, aux L, aux R, then out L, out R, aux L, aux R. The original runs in place; the
//! aux pair is never written (asserted bitwise). Every recorded `SetParam` is applied through `set_param` before the block it is logged in.
//! `GLORP_REF_ROOT` unset => the tests below skip explicitly (they do not prove parity). Set but a fixture missing => they
//! fail loudly (`missing_fixtures_are_a_hard_error` proves that path without the env var).
use glorp::id3::{Id3, PARAMS};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

const B: usize = 128;
const BLOCK_BYTES: usize = 8 * B * 4;
const VECTORS: usize = 13;
const SONG_BLOCKS: usize = 78_292;
/// Per-sample bound `ABS + REL·|ref|` for every fixture. Measured: see the research doc (§ID 3); the bound allows a few
/// ulp, a 1/255 nudge of an acting parameter is far outside it (negative controls).
const ABS: f64 = 1e-7;
const REL: f64 = 3e-7;

#[derive(Default)]
struct Stats {
    samples: usize,
    exact: usize,
    max_abs: f64,
    max_ref: f64,
    nonzero: usize,
    /// Output differs from the input by more than 1e-6 (the shaper acted).
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

/// Initial parameters of the ID 3 song device and its index: `(device, 11 floats)` (the 12th chunk float is the byte size).
fn chunk(blob: &[u8]) -> Result<(usize, [f32; PARAMS]), String> {
    let u32at = |p: usize| u32::from_le_bytes(blob[p..p + 4].try_into().unwrap()) as usize;
    let n = u32at(16);
    let mut p = 20;
    let mut found = Vec::new();
    for i in 0..n {
        let (id, size) = (blob[p], u32at(p + 1));
        let c = &blob[p + 5..p + 5 + size];
        p += 5 + size;
        if id == 3 {
            if size != 48 {
                return Err(format!("ID 3 chunk has {size} bytes, expected 48"));
            }
            found.push((i, f32s(c)[..PARAMS].try_into().unwrap()));
        }
    }
    if found.len() != 1 {
        return Err(format!("expected exactly one ID 3 instance in the song, found {}", found.len()));
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
        dir.join("taps/dev00.tap"),
        dir.join("tapspec3.txt"),
        dir.join("taps3/dev00.tap"),
        dir.join("id3ref/manifest.json"),
    ];
    for n in 1..=VECTORS {
        need.push(dir.join(format!("id3ref/vec{n:02}.csv")));
        need.push(dir.join(format!("id3ref/vec{n:02}.tap")));
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
    param_events(path, Some(dev))
}

/// Parsed `param` rows keyed by block; `dev` = `None` keeps every device (parser-liveness check).
fn param_events(path: &Path, dev: Option<usize>) -> Events {
    let mut m: Events = HashMap::new();
    for line in BufReader::new(std::fs::File::open(path).unwrap()).lines() {
        let line = line.unwrap();
        let f: Vec<&str> = line.split(',').collect();
        if f.len() == 7 && f[3] == "param" && dev.map_or(true, |d| f[1].parse() == Ok(d)) {
            let bits: u32 = f[6].parse().unwrap();
            m.entry(f[0].parse().unwrap()).or_default().push((f[4].parse().unwrap(), f32::from_bits(bits)));
        }
    }
    m
}

/// Replays blocks from `src` (a whole-block reader) from block 0 and checks every sample against the bound.
/// `tweak` is the negative-control hook: it may rewrite an event before it is applied.
fn replay(label: &str, init: [f32; PARAMS], events: &Events, src: &mut dyn Read, nblocks: usize, tweak: &dyn Fn(usize, f32) -> Option<f32>) -> Stats {
    let mut d = Id3::new(&init).unwrap_or_else(|e| panic!("{label}: {e:?}"));
    let mut s = Stats::default();
    let mut raw = vec![0u8; BLOCK_BYTES];
    for b in 0..nblocks {
        for &(i, v) in events.get(&b).map(|v| v.as_slice()).unwrap_or(&[]) {
            if let Some(v) = tweak(i, v) {
                d.set_param(i, v).unwrap_or_else(|e| panic!("{label} block {b}: param {i}={v} rejected: {e:?}"));
            }
        }
        src.read_exact(&mut raw).unwrap_or_else(|e| panic!("{label}: tap ends before block {b}: {e}"));
        let blk = f32s(&raw);
        // bitwise: the aux pair may hold NaN, and NaN != NaN
        let bits = |r: std::ops::Range<usize>| blk[r].iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(2 * B..4 * B), bits(6 * B..8 * B), "{label} block {b}: the original touched the aux pair");
        let (mut l, mut r) = (blk[..B].to_vec(), blk[B..2 * B].to_vec());
        d.process(&mut l, &mut r);
        for (c, got) in [l, r].iter().enumerate() {
            for i in 0..B {
                let (x, want) = (blk[c * B + i], blk[(4 + c) * B + i]);
                let (g, w) = (got[i] as f64, want as f64);
                s.samples += 1;
                // NaN must match NaN (bit pattern aside) and an infinity must match its sign
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
                s.shaped += ((want as f64 - x as f64).abs() > 1e-6 || !want.is_finite()) as usize;
                s.nonfinite += !w.is_finite() as usize;
                let bound = if w.is_finite() { ABS + REL * w.abs() } else { 0.0 };
                assert!(e <= bound, "{label} block {b} ch {c} i {i}: got {} want {want}", got[i]);
            }
        }
    }
    s
}

fn replay_file(label: &str, init: [f32; PARAMS], events: &Events, path: &Path, tweak: &dyn Fn(usize, f32) -> Option<f32>) -> Stats {
    let len = std::fs::metadata(path).unwrap().len() as usize;
    assert_eq!(len % BLOCK_BYTES, 0, "{label}: tap is not a whole number of blocks");
    let mut f = BufReader::with_capacity(1 << 20, std::fs::File::open(path).unwrap());
    replay(label, init, events, &mut f, len / BLOCK_BYTES, tweak)
}

fn report(label: &str, s: &Stats) {
    eprintln!(
        "{label}: {} samples, {} exact ({} nonzero refs, {} non-finite), max |ref| {:.4}, max abs err {:.3e}, {} shaped",
        s.samples, s.exact, s.nonzero, s.nonfinite, s.max_ref, s.max_abs, s.shaped
    );
}

fn keep(_: usize, v: f32) -> Option<f32> {
    Some(v)
}

fn vector_path(fx: &Fixtures, n: usize) -> (String, PathBuf, Events) {
    let name = format!("vec{n:02}");
    let manifest = std::fs::read_to_string(fx.dir.join("id3ref/manifest.json")).unwrap();
    assert!(manifest.contains(&format!("\"{name}\"")), "{name} missing from manifest.json");
    let ev = events_of(&fx.dir.join(format!("id3ref/{name}.csv")), 0);
    (name.clone(), fx.dir.join(format!("id3ref/{name}.tap")), ev)
}

/// The song tap's spec row must say what the test assumes: a partial or shifted tap would pass for the wrong reason.
fn check_tapspec(path: &Path, dev: usize, b0: usize, b1: usize) {
    let specs = std::fs::read_to_string(path).unwrap();
    let ok = specs.lines().any(|l| {
        let a: Vec<usize> = l.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        a == [dev, b0, b1]
    });
    assert!(ok, "{} has no `{dev} {b0} {b1}` row", path.display());
}

/// The fixture loader must refuse an incomplete root, naming the missing file: this runs without the env var.
#[test]
fn missing_fixtures_are_a_hard_error() {
    let base = std::env::temp_dir().join(format!("glorp3-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let e = load(&base.join("nope")).err().expect("a non-directory root must fail");
    assert!(e.contains("not a directory"), "{e}");
    std::fs::create_dir_all(&base).unwrap();
    let e = load(&base).err().expect("an empty root must fail");
    assert!(e.contains("389811-2.bin") && e.contains("missing"), "{e}");
    std::fs::write(base.join("389811-2.bin"), b"x").unwrap();
    let e = load(&base).err().expect("a root without the harness out dir must fail");
    assert!(e.contains("ev_full.csv") && e.contains("missing"), "{e}");
    // everything present but the whole-song tap: that exact file is named
    let out = base.join("glorp-win64/analysis/harness/out");
    for d in ["taps", "taps3", "id3ref"] {
        std::fs::create_dir_all(out.join(d)).unwrap();
    }
    for f in ["ev_full.csv", "tapspec.txt", "taps/dev00.tap", "tapspec3.txt", "id3ref/manifest.json"] {
        std::fs::write(out.join(f), b"").unwrap();
    }
    for n in 1..=VECTORS {
        std::fs::write(out.join(format!("id3ref/vec{n:02}.csv")), b"").unwrap();
        std::fs::write(out.join(format!("id3ref/vec{n:02}.tap")), b"").unwrap();
    }
    let e = load(&base).err().expect("a missing whole-song tap must fail");
    assert!(e.contains("taps3/dev00.tap") && e.contains("missing"), "{e}");
    std::fs::write(out.join("taps3/dev00.tap"), b"").unwrap();
    std::fs::remove_file(out.join("id3ref/vec07.tap")).unwrap();
    let e = load(&base).err().expect("a missing vector must fail");
    assert!(e.contains("vec07.tap") && e.contains("missing"), "{e}");
    let _ = std::fs::remove_dir_all(&base);
}

/// A song blob without exactly one well-formed ID 3 chunk is refused rather than guessed at.
#[test]
fn chunk_parser_refuses_a_song_without_exactly_one_id3() {
    let mut blob = vec![0u8; 20];
    blob[16..20].copy_from_slice(&1u32.to_le_bytes());
    blob.push(38);
    blob.extend(4u32.to_le_bytes());
    blob.extend([0u8; 4]);
    assert!(chunk(&blob).err().unwrap().contains("found 0"));
    let mut two = vec![0u8; 20];
    two[16..20].copy_from_slice(&2u32.to_le_bytes());
    for _ in 0..2 {
        two.push(3);
        two.extend(48u32.to_le_bytes());
        two.extend([0u8; 48]);
    }
    assert!(chunk(&two).err().unwrap().contains("found 2"));
    let mut bad = vec![0u8; 20];
    bad[16..20].copy_from_slice(&1u32.to_le_bytes());
    bad.push(3);
    bad.extend(8u32.to_le_bytes());
    bad.extend([0u8; 8]);
    assert!(chunk(&bad).err().unwrap().contains("expected 48"));
}

/// A tap shorter than the declared length must be a loud failure, not a shorter comparison.
#[test]
fn truncated_tap_is_a_hard_error() {
    let init = [0.5; PARAMS];
    let r = std::panic::catch_unwind(|| {
        let tap = vec![0u8; BLOCK_BYTES * 2 - 4];
        replay("trunc", init, &HashMap::new(), &mut &tap[..], 2, &keep);
    });
    let err = r.err().expect("a truncated tap must panic");
    let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("tap ends before block 1"), "{msg}");
}

#[test]
fn id3_matches_the_song_window_tap() {
    let Some((fx, dev)) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    assert_eq!(dev, 0, "the song's ID 3 instance is device 0");
    // parser liveness: a broken CSV would parse zero rows and make the emptiness check below vacuous
    let all: usize = param_events(&fx.dir.join("ev_full.csv"), None).values().map(|v| v.len()).sum();
    assert!(all > 0, "ev_full.csv parsed no param events at all: the no-automation check would be vacuous");
    assert!(events_of(&fx.dir.join("ev_full.csv"), dev).is_empty(), "the song log has parameter writes for device {dev}: this test assumes it never automates ID 3");
    check_tapspec(&fx.dir.join("tapspec.txt"), dev, 0, 600);
    let tap = fx.dir.join("taps/dev00.tap");
    assert_eq!(std::fs::metadata(&tap).unwrap().len() as usize, 600 * BLOCK_BYTES, "dev00.tap has an unexpected size");
    let s = replay_file("song window", fx.init, &HashMap::new(), &tap, &keep);
    report("song window (600 blocks)", &s);
    assert!(s.nonzero > 100_000 && s.shaped > 10_000, "window too quiet or never shaped");
}

/// The whole song: 78,292 blocks (~227 s), device 0 from block 0, every block compared.
#[test]
fn id3_matches_the_whole_song_tap() {
    let Some((fx, dev)) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    check_tapspec(&fx.dir.join("tapspec3.txt"), dev, 0, SONG_BLOCKS);
    let tap = fx.dir.join("taps3/dev00.tap");
    assert_eq!(std::fs::metadata(&tap).unwrap().len() as usize, SONG_BLOCKS * BLOCK_BYTES, "taps3/dev00.tap is incomplete");
    let s = replay_file("whole song", fx.init, &HashMap::new(), &tap, &keep);
    report("whole song", &s);
    assert!(s.nonzero > 1_000_000 && s.shaped > 100_000, "song too quiet or the shaper never acted");
}

#[test]
fn id3_matches_the_original_on_synthetic_vectors() {
    let Some((fx, _)) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    for n in 1..=VECTORS {
        let (name, path, ev) = vector_path(&fx, n);
        let s = replay_file(&name, fx.init, &ev, &path, &keep);
        report(&name, &s);
        assert!(s.samples > 0 && s.nonzero > 10, "{name}: reference output is silent");
    }
}

/// The vectors must exercise what they claim.
#[test]
fn vectors_exercise_every_mode_nonfinite_input_and_automation() {
    let Some((fx, _)) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    for n in [1, 2, 3, 4, 5, 6, 8, 9, 10, 11, 13] {
        let (name, path, ev) = vector_path(&fx, n);
        let s = replay_file(&name, fx.init, &ev, &path, &keep);
        assert!(s.shaped > 1_000, "{name} never shaped the signal ({} samples)", s.shaped);
    }
    let (name, path, ev) = vector_path(&fx, 12);
    assert!(replay_file(&name, fx.init, &ev, &path, &keep).nonfinite >= 10, "vec12 reference has too few non-finite outputs");
    let (_, _, ev10) = vector_path(&fx, 10);
    assert!(ev10.len() >= 169 && ev10.values().map(|v| v.len()).sum::<usize>() >= 1800, "vec10 automation log looks empty");
    // vec11 steps through the mode boundaries (P4) and every oversampling value (P5)
    let (_, _, ev11) = vector_path(&fx, 11);
    let all: Vec<(usize, f32)> = ev11.values().flatten().copied().collect();
    for want in [0.2501f32, 0.5004, 0.7502] {
        assert!(all.contains(&(4, want)), "vec11 lacks P4 = {want}");
    }
    for want in [0.4999f32, 0.5, 0.9999, 1.0] {
        assert!(all.contains(&(5, want)), "vec11 lacks P5 = {want}");
    }
}

/// Runs `f`, which must panic with `replay`'s `"<label> block .. got .. want .."` mismatch message (not an unrelated
/// failure, so a broken control cannot pass for the wrong reason).
fn must_mismatch(label: &str, what: &str, f: impl FnOnce() + std::panic::UnwindSafe) {
    let err = std::panic::catch_unwind(f).err().unwrap_or_else(|| panic!("{what}: stayed inside the tolerance"));
    let msg = err.downcast_ref::<String>().map(String::as_str).or_else(|| err.downcast_ref::<&str>().copied()).unwrap_or("");
    assert!(msg.starts_with(&format!("{label} block")) && msg.contains(" got ") && msg.contains(" want "), "{what}: panicked for another reason: {msg}");
}

/// Negative control: one flipped bit in the reference must leave the tolerance.
#[test]
fn negative_control_flipped_reference_bit_is_caught() {
    let Some((fx, _)) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    let mut tap = std::fs::read(fx.dir.join("taps/dev00.tap")).unwrap();
    let blk = (0..600).find(|b| tap[(b * 8 * B + 4 * B + 5) * 4..][..4] != [0, 0, 0, 0]).unwrap();
    tap[(blk * 8 * B + 4 * B + 5) * 4 + 2] ^= 0x40;
    must_mismatch("flip", "a flipped reference bit", || {
        replay("flip", fx.init, &HashMap::new(), &mut &tap[..], 600, &keep);
    });
}

/// Negative control: the song window with one parameter of the chunk nudged by 1/255 must leave the tolerance. P2 (fold)
/// is unused in mode 3 and P4's 1/255 step keeps mode 3 (`0.996·3.999 = 3.98`): both are *expected* to stay inside, which is
/// asserted too, so the control cannot pass by accident for them.
#[test]
fn negative_control_wrong_parameters_are_caught() {
    let Some((fx, _)) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    let tap = fx.dir.join("taps/dev00.tap");
    for param in 0..PARAMS {
        let mut init = fx.init;
        let d = if init[param] + 1.0 / 255.0 > 1.0 { -1.0 / 255.0 } else { 1.0 / 255.0 };
        init[param] += d;
        let run = std::panic::catch_unwind(|| {
            replay("wrong", init, &HashMap::new(), &mut std::fs::File::open(&tap).unwrap(), 600, &keep);
        });
        if param == 2 || param == 4 {
            assert!(run.is_ok(), "P{param} is inert in this configuration, but its nudge left the tolerance");
        } else {
            must_mismatch("wrong", &format!("a nudge of P{param} by {d}"), std::panic::AssertUnwindSafe(|| {
                run.map(|_| ()).unwrap_or_else(|e| std::panic::resume_unwind(e));
            }));
        }
    }
}

/// Negative control: dropped automation (vec10), an ignored mode switch (vec11: P4 forced to the song's value) and
/// ignored oversampling (vec04 with P5 forced to 0) must all be caught.
#[test]
fn negative_control_dropped_automation_mode_and_oversampling_are_caught() {
    let Some((fx, _)) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    let (_, p10, ev10) = vector_path(&fx, 10);
    let first: Events = ev10.iter().filter(|(b, _)| **b == 0).map(|(b, v)| (*b, v.clone())).collect();
    must_mismatch("dropped", "dropped automation", || {
        replay_file("dropped", fx.init, &first, &p10, &keep);
    });
    let (_, p11, ev11) = vector_path(&fx, 11);
    must_mismatch("mode", "an ignored mode switch", || {
        replay_file("mode", fx.init, &ev11, &p11, &|i, v| if i == 4 { None } else { Some(v) });
    });
    let (_, p4, ev4) = vector_path(&fx, 4);
    must_mismatch("over", "ignored oversampling", || {
        replay_file("over", fx.init, &ev4, &p4, &|i, v| Some(if i == 5 { 0.0 } else { v }));
    });
}
