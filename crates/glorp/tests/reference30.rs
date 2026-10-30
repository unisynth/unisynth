//! Optional external-reference parity: ID 30 (granular delay + comb/all-pass reverb + filters + frequency shifter +
//! all-pass cascade) vs the original engine's output, captured through the isolated Wine harness (local only: third-party
//! binary + song never enter git; nothing here is a recorded tap or song data).
//!
//! `GLORP_REF_ROOT` = the `ws-intros/out` directory. **Unset ⇒ every test here skips explicitly and proves nothing.
//! Set ⇒ the fixture must be complete: a missing or malformed file panics** (a misconfigured root must never look like
//! a pass). Required, below `$GLORP_REF_ROOT/`:
//!   * `389811-2.bin`: song blob (read only for the ID 30 initial chunks of song devices 15..21);
//!   * `glorp-win64/analysis/harness/out/id30impl/ev.csv`: every recorded `SetParam` of the whole song;
//!   * `.../id30impl/taps/dev{15..21}.tap`: **full-song** taps (78,292 blocks each, 8 × 128 f32 per block: in L, in R, aux
//!     L, aux R, then the original's out L, out R, aux L, aux R) captured with
//!     `analysis/id30-impl/run30.sh 'G song x.wav --tapspec tapspec.txt taps --events ev.csv --events-all'`;
//!   * `.../id30impl/vec/manifest.json` + `NAME.{csv,tap}`: synthetic vectors (`vec/gen.py` writes `NAME.csv` = the
//!     event log of song device 15 and `NAME.in.tap`; `vec/capture.sh` records `NAME.tap` with `GLORP_OUT_TAP`).
//!
//! The replay starts at block 0 with the recorded inputs, so the delay lines, the grain slots, the hash chain, the
//! filters and the shifter carry exactly the original's history; every recorded `SetParam` is applied through
//! `set_param` before its block (which also proves the recorded automation is accepted). Note events are ignored (the
//! original's note slots do nothing). The output is compared for **every** sample of the song.
use glorp::id30::{Id30, PARAMS, SLAB_F32};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use synth_core::pool::Pool;

const B: usize = 128;
const SONG_BLOCKS: usize = 78_292;
/// The seven ID 30 song instances.
const DEVICES: [usize; 7] = [15, 16, 17, 18, 19, 20, 21];
/// Per-device tolerance bound as a fraction of the device's peak output. Measured over the whole song (20.04 M samples
/// per device): devices 15, 16, 17, 18, 21 are 100 % bit-exact; device 19 max 2.5e-13 (1.5e-13 of peak, 75 % exact; the second deviation besides device 20, cause not investigated);
/// device 20 max 1.41e-4 (9.6e-5 of peak, 62.9 % exact). The difference of device 20 starts at block 34,576 as a smooth
/// ramp from the block boundary (not a jump), where its automated P6 (high-pass cutoff ≈ 48 Hz) moves again: consistent
/// with a 1-ulp difference of a filter coefficient computed through this crate's `libm` instead of the original's
/// (Wine) trigonometry, which a 48 Hz high-pass in f32 direct form 1 turns into ~1e-4. Everything else on that device
/// stays within it. No tap / vector carries non-finite input (not reference-verified). The bound leaves ×3 over the measured worst case.
const SONG_TOL_REL: f64 = 3e-4;
/// Song devices whose whole-song output equals their input sample for sample (the song never gives the effect a
/// wet gain or a signal): bit-exact but proves nothing about the wet path, which the synthetic vectors cover.
const INACTIVE_IN_SONG: [usize; 1] = [18];

type Events = HashMap<usize, Vec<(usize, f32)>>;

struct Fixture {
    out: PathBuf,
    blob: Vec<u8>,
}

fn fixture() -> Option<Fixture> {
    Some(fixture_at(PathBuf::from(std::env::var_os("GLORP_REF_ROOT")?)))
}

fn fixture_at(root: PathBuf) -> Fixture {
    let blob_path = root.join("389811-2.bin");
    let blob = std::fs::read(&blob_path)
        .unwrap_or_else(|e| panic!("GLORP_REF_ROOT is set but {} is unreadable: {e}", blob_path.display()));
    let out = root.join("glorp-win64/analysis/harness/out/id30impl");
    let mut files: Vec<String> = vec!["ev.csv".into()];
    files.extend(DEVICES.iter().map(|d| format!("taps/dev{d}.tap")));
    for f in files {
        let p = out.join(&f);
        assert!(p.is_file(), "GLORP_REF_ROOT is set but fixture file {} is missing", p.display());
    }
    Fixture { out, blob }
}

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}

/// Initial parameters of every ID 30 song device: device index -> 24 floats (the 25th chunk float is the byte size).
fn chunks(blob: &[u8]) -> HashMap<usize, [f32; PARAMS]> {
    let u32at = |p: usize| u32::from_le_bytes(blob[p..p + 4].try_into().unwrap()) as usize;
    let n = u32at(16);
    let mut p = 20;
    let mut out = HashMap::new();
    for i in 0..n {
        let (id, size) = (blob[p], u32at(p + 1));
        let chunk = &blob[p + 5..p + 5 + size];
        p += 5 + size;
        if id == 30 {
            assert_eq!(size, 100, "ID 30 chunk is 4 + 4·24 bytes");
            out.insert(i, f32s(chunk)[..PARAMS].try_into().unwrap());
        }
    }
    out
}

fn events_of(path: &Path, dev: usize) -> Events {
    let mut m: Events = HashMap::new();
    let file = std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    for (n, line) in BufReader::new(file).lines().enumerate().skip(1) {
        let line = line.unwrap();
        let f: Vec<&str> = line.split(',').collect();
        assert_eq!(f.len(), 7, "{}:{}: malformed event line {line:?}", path.display(), n + 1);
        if f[3] == "param" && f[1].parse() == Ok(dev) {
            let bits: u32 = f[6].parse().unwrap_or_else(|_| panic!("{}:{}: bad bits", path.display(), n + 1));
            m.entry(f[0].parse().unwrap()).or_default().push((f[4].parse().unwrap(), f32::from_bits(bits)));
        }
    }
    m
}

#[derive(Default, Debug, Clone, Copy)]
struct BlockStat {
    max_abs: f64,
    max_ref: f64,
    exact: usize,
    /// Samples where the original's output differs from its own input.
    active: usize,
}

#[derive(Default, Debug)]
struct Report {
    blocks: Vec<BlockStat>,
}

impl Report {
    fn samples(&self) -> usize {
        self.blocks.len() * 2 * B
    }
    fn exact(&self) -> usize {
        self.blocks.iter().map(|b| b.exact).sum()
    }
    fn active(&self) -> usize {
        self.blocks.iter().map(|b| b.active).sum()
    }
    fn max_abs(&self) -> f64 {
        self.blocks.iter().fold(0.0, |m, b| m.max(b.max_abs))
    }
    fn max_ref(&self) -> f64 {
        self.blocks.iter().fold(0.0, |m, b| m.max(b.max_ref))
    }
}

struct Tap {
    r: BufReader<std::fs::File>,
    blocks: usize,
}

impl Tap {
    fn open(p: &Path, expect_blocks: Option<usize>) -> Tap {
        let f = std::fs::File::open(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        let len = f.metadata().unwrap().len() as usize;
        assert_eq!(len % (8 * B * 4), 0, "{}: not a whole number of taps blocks", p.display());
        let blocks = len / (8 * B * 4);
        if let Some(n) = expect_blocks {
            assert_eq!(blocks, n, "{}: expected {n} blocks", p.display());
        }
        Tap { r: BufReader::with_capacity(1 << 20, f), blocks }
    }
    fn next(&mut self) -> Vec<f32> {
        let mut raw = vec![0u8; 8 * B * 4];
        self.r.read_exact(&mut raw).expect("short tap");
        f32s(&raw)
    }
}

fn arena() -> Vec<u8> {
    vec![0u8; SLAB_F32 * 4 + 64]
}

/// Replays `tap` through a fresh `Id30` built from `init`; `events` are applied through `set_param` before their block.
/// `limit` blocks at most. Returns the report and, when `tol` is `Some`, panics with a `mismatch` message at the first
/// sample further than `tol` (absolute; NaN counts) from the original's output. `flip` corrupts one expected sample.
fn replay(name: &str, init: [f32; PARAMS], events: &Events, tap: &mut Tap, limit: usize, tol: Option<f64>, flip: Option<usize>) -> Report {
    let mut mem = arena();
    let mut pool = Pool::new(&mut mem);
    let mut d = Id30::new(&init, &mut pool).unwrap_or_else(|e| panic!("{name}: initial chunk rejected: {e:?}"));
    let mut rep = Report::default();
    for b in 0..tap.blocks.min(limit) {
        for &(i, v) in events.get(&b).map(|v| v.as_slice()).unwrap_or(&[]) {
            d.set_param(i, v).unwrap_or_else(|e| panic!("{name} block {b}: param {i}={v} rejected: {e:?}"));
        }
        let mut blk = tap.next();
        assert_eq!(blk[2 * B..4 * B], blk[6 * B..8 * B], "{name} block {b}: the original touched its aux pair");
        if flip == Some(b) {
            blk[4 * B + 5] = f32::from_bits(blk[4 * B + 5].to_bits() ^ 0x4000_0000);
        }
        let (mut l, mut r) = (blk[..B].to_vec(), blk[B..2 * B].to_vec());
        d.process(&mut l, &mut r);
        let mut s = BlockStat::default();
        for (c, got) in [&l, &r].into_iter().enumerate() {
            for i in 0..B {
                let want = blk[(4 + c) * B + i];
                let e = (got[i] as f64 - want as f64).abs();
                if let Some(tol) = tol {
                    assert!(e <= tol || (got[i] == want), "mismatch: {name} block {b} ch {c} i {i}: got {} want {want} (err {e:e}, tol {tol:e})", got[i]);
                }
                s.max_abs = s.max_abs.max(if e.is_nan() { f64::INFINITY } else { e });
                s.max_ref = s.max_ref.max(want.abs() as f64);
                s.exact += (got[i] == want) as usize;
                s.active += (want != blk[c * B + i]) as usize;
            }
        }
        rep.blocks.push(s);
    }
    rep
}

fn song_inputs(fx: &Fixture, dev: usize) -> ([f32; PARAMS], Events, Tap) {
    let init = *chunks(&fx.blob).get(&dev).unwrap_or_else(|| panic!("the song blob has no ID 30 device {dev}"));
    let ev = events_of(&fx.out.join("ev.csv"), dev);
    let tap = Tap::open(&fx.out.join(format!("taps/dev{dev}.tap")), explore_blocks().is_none().then_some(SONG_BLOCKS));
    (init, ev, tap)
}

/// Exploration aid: `GLORP_ID30_BLOCKS=n` limits the replay (no tolerance assert) and prints the per-device figures.
fn explore_blocks() -> Option<usize> {
    std::env::var("GLORP_ID30_BLOCKS").ok().map(|v| v.parse().expect("GLORP_ID30_BLOCKS is a block count"))
}

fn run_song_device(dev: usize) {
    let Some(fx) = fixture() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    let (init, ev, mut tap) = song_inputs(&fx, dev);
    let limit = explore_blocks().unwrap_or(usize::MAX);
    let rep = replay(&format!("dev {dev}"), init, &ev, &mut tap, limit, None, None);
    let peak = rep.max_ref();
    eprintln!(
        "dev {dev}: {} samples, {} exact ({:.3}%), max |ref| {:.4}, max abs err {:.3e} ({:.2e} of peak), active {} samples",
        rep.samples(),
        rep.exact(),
        100.0 * rep.exact() as f64 / rep.samples() as f64,
        peak,
        rep.max_abs(),
        rep.max_abs() / peak.max(1e-30),
        rep.active(),
    );
    if let Some(b) = rep.blocks.iter().position(|b| b.exact < 2 * B) {
        eprintln!("dev {dev}: first inexact block {b} (max abs err there {:.3e})", rep.blocks[b].max_abs);
    }
    if limit != usize::MAX {
        return;
    }
    if INACTIVE_IN_SONG.contains(&dev) {
        assert_eq!(rep.active(), 0, "dev {dev}: expected the song to leave this instance inactive");
    } else {
        assert!(rep.active() > 100_000, "dev {dev}: only {} active samples: too little activity to mean anything", rep.active());
    }
    assert!(rep.max_abs() <= SONG_TOL_REL * peak, "dev {dev}: max abs err {:.3e} exceeds {SONG_TOL_REL:e} of the peak {peak}", rep.max_abs());
}

macro_rules! song_device {
    ($name:ident, $dev:expr) => {
        #[test]
        fn $name() {
            run_song_device($dev);
        }
    };
}
song_device!(id30_song_device_15_matches_the_original, 15);
song_device!(id30_song_device_16_matches_the_original, 16);
song_device!(id30_song_device_17_matches_the_original, 17);
song_device!(id30_song_device_18_matches_the_original, 18);
song_device!(id30_song_device_19_matches_the_original, 19);
song_device!(id30_song_device_20_matches_the_original, 20);
song_device!(id30_song_device_21_matches_the_original, 21);

/// Synthetic vector `name`: the block-0 events of song device 15 must set all 24 parameters (they replace the song
/// chunk); later blocks are events. Returns `(init, events, block count from manifest.json)`.
fn vector_inputs(dir: &Path, name: &str) -> ([f32; PARAMS], Events, usize) {
    let mut ev = events_of(&dir.join(format!("{name}.csv")), 15);
    let first = ev.remove(&0).unwrap_or_else(|| panic!("{name}: no block-0 events"));
    let mut init = [f32::NAN; PARAMS];
    for (i, v) in first {
        init[i] = v;
    }
    assert!(init.iter().all(|v| v.is_finite()), "{name}: block 0 must set all {PARAMS} parameters");
    let manifest = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
    let key = format!("\"{name}\": ");
    let at = manifest.find(&key).unwrap_or_else(|| panic!("{name} missing from manifest.json")) + key.len();
    let n: usize = manifest[at..].split(|c: char| !c.is_ascii_digit()).next().unwrap().parse().unwrap();
    (init, ev, n)
}

fn vector_names(dir: &Path) -> Vec<String> {
    let manifest = std::fs::read_to_string(dir.join("manifest.json")).unwrap_or_else(|e| panic!("manifest.json: {e}"));
    let mut v: Vec<String> = manifest.split('"').skip(1).step_by(2).map(String::from).collect();
    v.sort();
    v
}

#[test]
fn id30_synthetic_vectors_match_the_original() {
    let Some(fx) = fixture() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    let dir = fx.out.join("vec");
    let names = vector_names(&dir);
    assert!(names.len() >= 80, "manifest lists only {} vectors", names.len());
    let mut refused = vec![];
    let mut inactive = vec![];
    for name in &names {
        let (init, ev, n) = vector_inputs(&dir, name);
        if let Err(e) = glorp::id30::check(&init) {
            eprintln!("{name}: refused by design ({e:?})");
            refused.push(name.clone());
            continue;
        }
        let mut tap = Tap::open(&dir.join(format!("{name}.tap")), Some(n));
        let rep = replay(name, init, &ev, &mut tap, usize::MAX, None, None);
        let rel = rep.max_abs() / rep.max_ref().max(1e-30);
        eprintln!("{name}: {} samples, {} exact, max |ref| {:.4}, max abs err {:.3e} ({:.2e} of peak), active {}", rep.samples(), rep.exact(), rep.max_ref(), rep.max_abs(), rel, rep.active());
        assert!(rel <= vector_tol_rel(name), "{name}: relative error {rel:e} exceeds {:e}", vector_tol_rel(name));
        if rep.active() == 0 {
            inactive.push(name.clone());
        }
    }
    // the cascade topologies 1 / 2 are refused, never approximated (the exact set, so nothing is silently skipped)
    assert_eq!(refused, ["e_halves", "u_t1", "u_t2"]);
    // identity vectors (wet gain 0 / bypass / silent input): bit-exact, listed so a new one cannot hide
    assert_eq!(inactive, ["e_ones", "e_sil", "e_zeros", "f_dry", "h_off_at_start", "s21"]);
}

/// Bit-exact on every synthetic vector but `a_many` (many grain streams: summation / libm order, 1.5e-6 abs, 4.8e-6 of peak).
fn vector_tol_rel(name: &str) -> f64 {
    if name == "a_many" { 2e-5 } else { 0.0 }
}

#[test]
fn a_set_but_unusable_root_is_a_hard_error() {
    // a root without the song blob must panic, never skip
    let r = catch_unwind(|| fixture_at(PathBuf::from("/nonexistent-glorp-ref-root")).out);
    assert!(r.is_err(), "a set but unusable GLORP_REF_ROOT must fail loudly");
}

#[test]
fn negative_controls_are_caught() {
    let Some(fx) = fixture() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    let caught = |f: &mut dyn FnMut()| catch_unwind(AssertUnwindSafe(f)).is_err();
    // 1. a flipped reference bit is reported (bit-exact vector, tolerance 0)
    let dir = fx.out.join("vec");
    let (init, ev, n) = vector_inputs(&dir, "c_mid");
    assert!(
        caught(&mut || {
            let mut tap = Tap::open(&dir.join("c_mid.tap"), Some(n));
            replay("c_mid+flip", init, &ev, &mut tap, usize::MAX, Some(0.0), Some(20));
        }),
        "a flipped reference sample went unnoticed"
    );
    // 2. forcing one parameter elsewhere is caught on the vector that exercises it
    for (name, idx, v) in [
        ("c_mid", 17usize, 0.9f32), // cascade centre
        ("r_all", 12, 0.2),         // reverb feedback
        ("f_lp0", 5, 0.6),          // low-pass
        ("g_p9hi", 9, 0.0),         // grain scatter
        ("b_pp1", 21, 0.0),         // ping-pong
        ("t0", 0, 0.5),             // delay time
        ("x3", 4, 0.9),             // shift
    ] {
        let (mut init, ev, n) = vector_inputs(&dir, name);
        init[idx] = v;
        assert!(
            caught(&mut || {
                let mut tap = Tap::open(&dir.join(format!("{name}.tap")), Some(n));
                replay(name, init, &ev, &mut tap, usize::MAX, Some(1e-5), None);
            }),
            "{name}: forcing P{idx} = {v} was not detected"
        );
    }
    // 3. one-byte (1/255) parameter nudges on the static-ish song instance 21 are caught over its first blocks
    let (init21, ev21, _) = song_inputs(&fx, 21);
    for idx in [0usize, 1, 5, 9, 13] {
        let mut init = init21;
        init[idx] += if init[idx] > 0.5 { -1.0 / 255.0 } else { 1.0 / 255.0 };
        let mut tap = Tap::open(&fx.out.join("taps/dev21.tap"), None);
        let r = catch_unwind(AssertUnwindSafe(|| replay("dev 21 nudged", init, &ev21, &mut tap, 6000, Some(1e-5), None)));
        assert!(r.is_err(), "dev 21: nudging P{idx} went unnoticed");
    }
    // 4. dropping the recorded automation of a song instance is detected
    let (init, _, mut tap) = song_inputs(&fx, 20);
    let none: Events = HashMap::new();
    assert!(
        caught(&mut || {
            replay("dev 20 no events", init, &none, &mut tap, 40_000, Some(1e-5), None);
        }),
        "ignoring the song's parameter automation went unnoticed"
    );
}
