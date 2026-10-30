//! Optional external-reference parity: ID 32 (chorus / all-pass-feedback ensemble) vs the original engine's output,
//! captured through the isolated Wine harness (local only: third-party binary + song never enter git).
//!
//! `GLORP_REF_ROOT` = the `ws-intros/out` directory. **Unset ⇒ every test here skips explicitly and proves nothing.
//! Set ⇒ the fixture must be complete: a missing or malformed file panics** (a misconfigured root must never look like
//! a pass). Required, below `$GLORP_REF_ROOT/`:
//!   * `389811-2.bin`: song blob (read only for the ID 32 initial chunks of song devices 22, 23, 24);
//!   * `glorp-win64/analysis/harness/out/ev_full.csv`: recorded parameter writes of the whole song;
//!   * `.../out/taps32/dev{22,23,24}.tap`: **full-song** taps (78,292 blocks each) of those devices, captured with
//!     `G song x.wav --tapspec tapspec32.txt taps32` (spec lines `22 0 78292` …, see the research doc);
//!   * `.../out/id32vec/vecNN.{csv,tap}` (NN = 01..10) + `manifest.json`: synthetic vectors (`gen.py`; `vecNN.csv` =
//!     event log of song device 22, `vecNN.tap` = inputs + the output a fresh original instance produced, recorded with
//!     `GLORP_OUT_TAP`; `vecNN.in.tap`, the harness input, is not read here).
//!
//! Tap block = 8 × 128 f32: in L, in R, aux L, aux R, then out L, out R, aux L, aux R. The original runs in place (the
//! harness passes the same buffers as input and output). The replay starts at block 0 with the recorded inputs, so
//! delay lines, LFO phases, the base-delay glide and mode B's feedback pair carry exactly the original's history.
//! Every recorded `SetParam` is applied through `set_param` before its block (also proves the recorded automation is
//! accepted). Note events are ignored (the original's note slots do nothing). The output is compared for **every**
//! sample of the song, then reported per active window (maximal runs of blocks where the original changed its input).
use glorp::id32::{Id32, PARAMS};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

const B: usize = 128;
const SONG_BLOCKS: usize = 78_292;
const VECTORS: usize = 10;
/// Song instances: song device index → legacy 600-block tap window (`tapspec.txt`) reported separately.
const DEVICES: [(usize, usize); 3] = [(22, 0), (23, 0), (24, 62_664)];
const WINDOW_BLOCKS: usize = 600;
/// Absolute per-sample tolerance on the song instances (peak |output| 7.5). Measured over the whole song: dev 22 max
/// 3.040e-5, dev 23 max 7.033e-6, dev 24 max 4.835e-5 (99.99 % / 99.88 % / 99.06 % of the samples bit-exact). The whole
/// difference is one table node: the shared sine table (`id34`) is built with `libm::sinf`, whose node 305 is one ulp
/// from the original's `sinf`; with that single node nudged in a scratch build every song instance and every vector
/// below is bit-exact (the nudge is not kept, same stance as ID 34). A feedback delay with a modulated read position
/// magnifies the 1-ulp phase error to ≤ 5e-5.
const SONG_TOL: f64 = 1e-4;
/// Negative-control tolerance on dev 23 only: its faithful replay is 7.0e-6, a 1/255 parameter nudge first exceeds ~1.0e-4,
/// so 5e-5 keeps a wide margin on both sides (SONG_TOL would sit 0.02 % under the nudged error).
const CONTROL_TOL: f64 = 5e-5;
/// Synthetic vectors: bit-exact where no modulated read meets node 305 (`vec03`, `vec08`, `vec09`, `vec10`); `vec01`
/// differs only by a 1e-18 flush-to-zero tail; the rest measured ≤ 3.4e-5 (`vec02` 3.375e-5, `vec06` 3.326e-5).
fn vector_tol(name: &str) -> f64 {
    match name {
        "vec03" | "vec08" | "vec09" | "vec10" => 0.0,
        "vec01" => 1e-12,
        _ => 1e-4,
    }
}

type Events = HashMap<usize, Vec<(usize, f32)>>;

struct Fixture {
    out: PathBuf,
    blob: Vec<u8>,
}

fn fixture() -> Option<Fixture> {
    let root = PathBuf::from(std::env::var_os("GLORP_REF_ROOT")?);
    let blob_path = root.join("389811-2.bin");
    let blob = std::fs::read(&blob_path)
        .unwrap_or_else(|e| panic!("GLORP_REF_ROOT is set but {} is unreadable: {e}", blob_path.display()));
    let out = root.join("glorp-win64/analysis/harness/out");
    let mut files: Vec<String> =
        ["ev_full.csv", "taps32/dev22.tap", "taps32/dev23.tap", "taps32/dev24.tap", "id32vec/manifest.json"].map(String::from).into();
    for n in 1..=VECTORS {
        files.extend([format!("id32vec/vec{n:02}.csv"), format!("id32vec/vec{n:02}.tap")]);
    }
    for f in files {
        let p = out.join(&f);
        assert!(p.is_file(), "GLORP_REF_ROOT is set but fixture file {} is missing", p.display());
    }
    Some(Fixture { out, blob })
}

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}

/// Initial parameters of every ID 32 song device: device index -> 11 floats (the 12th chunk float is the byte size).
fn chunks(blob: &[u8]) -> HashMap<usize, [f32; PARAMS]> {
    let u32at = |p: usize| u32::from_le_bytes(blob[p..p + 4].try_into().unwrap()) as usize;
    let n = u32at(16);
    let mut p = 20;
    let mut out = HashMap::new();
    for i in 0..n {
        let (id, size) = (blob[p], u32at(p + 1));
        let chunk = &blob[p + 5..p + 5 + size];
        p += 5 + size;
        if id == 32 {
            assert_eq!(size, 48, "ID 32 chunk is 4 + 4·11 bytes");
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

#[derive(Default, Debug, Clone)]
struct Window {
    first: usize,
    blocks: usize,
    exact: usize,
    max_abs: f64,
    max_ref: f64,
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
    fn window(&self, first: usize, blocks: usize) -> Window {
        let s = &self.blocks[first..first + blocks];
        Window {
            first,
            blocks,
            exact: s.iter().map(|b| b.exact).sum(),
            max_abs: s.iter().fold(0.0, |m, b| m.max(b.max_abs)),
            max_ref: s.iter().fold(0.0, |m, b| m.max(b.max_ref)),
        }
    }
    /// Maximal runs of blocks that contain an active sample.
    fn active_windows(&self) -> Vec<Window> {
        let mut out = Vec::new();
        let mut b = 0;
        while b < self.blocks.len() {
            if self.blocks[b].active == 0 {
                b += 1;
                continue;
            }
            let first = b;
            while b < self.blocks.len() && self.blocks[b].active > 0 {
                b += 1;
            }
            out.push(self.window(first, b - first));
        }
        out
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

/// Replays `tap` through `Id32::new(init)`; `events` are applied through `set_param` before their block. Any sample
/// further than `tol` (absolute; NaN counts) from the original's output panics with a `mismatch` message. `flip`
/// corrupts one expected output sample of that block (negative control).
fn replay(name: &str, init: [f32; PARAMS], events: &Events, tap: &mut Tap, tol: f64, flip: Option<usize>) -> Report {
    let mut d = Id32::new(&init).unwrap_or_else(|e| panic!("{name}: initial chunk rejected: {e:?}"));
    let mut rep = Report::default();
    for b in 0..tap.blocks {
        for &(i, v) in events.get(&b).map(|v| v.as_slice()).unwrap_or(&[]) {
            d.set_param(i, v).unwrap_or_else(|e| panic!("{name} block {b}: param {i}={v} rejected: {e:?}"));
        }
        let mut blk = tap.next();
        assert_eq!(blk[2 * B..4 * B], blk[6 * B..8 * B], "{name} block {b}: the original touched its aux pair");
        if flip == Some(b) {
            blk[4 * B + 5] = f32::from_bits(blk[4 * B + 5].to_bits() ^ 0x4000_0000);
        }
        let (mut l, mut r) = (blk[..B].to_vec(), blk[B..2 * B].to_vec());
        d.process(&mut l, &mut r).unwrap_or_else(|e| panic!("{name} block {b}: process refused: {e:?}"));
        let mut s = BlockStat::default();
        for (c, got) in [&l, &r].into_iter().enumerate() {
            for i in 0..B {
                let want = blk[(4 + c) * B + i];
                let e = (got[i] as f64 - want as f64).abs();
                assert!(e <= tol, "mismatch: {name} block {b} ch {c} i {i}: got {} want {want} (err {e:e}, tol {tol:e})", got[i]);
                s.max_abs = s.max_abs.max(e);
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
    let init = *chunks(&fx.blob).get(&dev).unwrap_or_else(|| panic!("the song blob has no ID 32 device {dev}"));
    let ev = events_of(&fx.out.join("ev_full.csv"), dev);
    let tap = Tap::open(&fx.out.join(format!("taps32/dev{dev}.tap")), Some(SONG_BLOCKS));
    (init, ev, tap)
}

fn run_song_device(dev: usize) {
    let Some(fx) = fixture() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    let (init, ev, mut tap) = song_inputs(&fx, dev);
    let rep = replay(&format!("dev {dev}"), init, &ev, &mut tap, SONG_TOL, None);
    let windows = rep.active_windows();
    let covered: usize = windows.iter().map(|w| w.blocks).sum();
    eprintln!(
        "dev {dev}: {} samples ({SONG_BLOCKS} blocks), {} exact ({:.2}%), max |ref| {:.4}, max abs err {:.3e}; active (out != in): \
         {} samples in {} windows covering {covered} blocks ({:.1}% of the song)",
        rep.samples(),
        rep.exact(),
        100.0 * rep.exact() as f64 / rep.samples() as f64,
        rep.max_ref(),
        rep.max_abs(),
        rep.active(),
        windows.len(),
        100.0 * covered as f64 / SONG_BLOCKS as f64,
    );
    for w in windows.iter().take(40) {
        eprintln!("  active window blocks {}..{}: {} exact of {}, max |ref| {:.4}, max abs err {:.3e}", w.first, w.first + w.blocks, w.exact, w.blocks * 2 * B, w.max_ref, w.max_abs);
    }
    // the legacy recorded 600-block tap window of this instance (`tapspec.txt`), reported on its own
    let start = DEVICES.iter().find(|d| d.0 == dev).unwrap().1;
    let w = rep.window(start, WINDOW_BLOCKS);
    let act: usize = rep.blocks[start..start + WINDOW_BLOCKS].iter().map(|b| b.active).sum();
    eprintln!("  legacy tap window {}..{}: {act} active samples, {} exact of {}, max |ref| {:.4}, max abs err {:.3e}", start, start + WINDOW_BLOCKS, w.exact, WINDOW_BLOCKS * 2 * B, w.max_ref, w.max_abs);
    assert!(rep.active() > 100_000, "dev {dev}: only {} active samples: too little activity to mean anything", rep.active());
}

#[test]
fn id32_song_device_22_matches_the_original() {
    run_song_device(22);
}
#[test]
fn id32_song_device_23_matches_the_original() {
    run_song_device(23);
}
#[test]
fn id32_song_device_24_matches_the_original() {
    run_song_device(24);
}

#[test]
fn id32_synthetic_vectors_match_the_original() {
    let Some(fx) = fixture() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    for n in 1..=VECTORS {
        let name = format!("vec{n:02}");
        let dir = fx.out.join("id32vec");
        let (init, ev) = vector_inputs(&dir, &name);
        let mut tap = Tap::open(&dir.join(format!("{name}.tap")), None);
        let rep = replay(&name, init, &ev, &mut tap, vector_tol(&name), None);
        eprintln!(
            "{name}: {} samples, {} exact, max |ref| {:.4}, max abs err {:.3e}, active {}",
            rep.samples(), rep.exact(), rep.max_ref(), rep.max_abs(), rep.active()
        );
        assert!(rep.active() > 0, "{name}: the original never changed its input");
    }
}

/// Synthetic vector: the block-0 events must set all 11 parameters (they replace the song chunk); later blocks are events.
fn vector_inputs(dir: &Path, name: &str) -> ([f32; PARAMS], Events) {
    let mut ev = events_of(&dir.join(format!("{name}.csv")), 22);
    let first = ev.remove(&0).unwrap_or_else(|| panic!("{name}: no block-0 events"));
    let mut init = [f32::NAN; PARAMS];
    for (i, v) in first {
        init[i] = v;
    }
    assert!(init.iter().all(|v| v.is_finite()), "{name}: block 0 must set all {PARAMS} parameters");
    let manifest = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
    assert!(manifest.contains(&format!("\"{name}\"")), "{name} missing from manifest.json");
    (init, ev)
}

#[test]
fn negative_controls_are_caught() {
    let Some(fx) = fixture() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    let dir = fx.out.join("id32vec");
    let caught = |f: &mut dyn FnMut()| catch_unwind(AssertUnwindSafe(f)).is_err();
    // 1. a flipped reference bit is reported as a mismatch
    let (init, ev) = vector_inputs(&dir, "vec01");
    let mut hit = caught(&mut || {
        let mut tap = Tap::open(&dir.join("vec01.tap"), None);
        replay("vec01+flip", init, &ev, &mut tap, 5e-6, Some(20));
    });
    assert!(hit, "a flipped reference sample went unnoticed");
    // 2. wrong mode (P7 forced to the other side) and wrong parameters are caught on mode A / mode B vectors
    for (name, idx, v) in [("vec01", 7usize, 0.75f32), ("vec05", 7, 0.0), ("vec01", 2, 0.30), ("vec05", 3, 0.7), ("vec02", 5, 0.31)] {
        let (mut init, ev) = vector_inputs(&dir, name);
        init[idx] = v;
        hit = caught(&mut || {
            let mut tap = Tap::open(&dir.join(format!("{name}.tap")), None);
            replay(name, init, &ev, &mut tap, vector_tol(name), None);
        });
        assert!(hit, "{name}: forcing P{idx} = {v} was not detected");
    }
    // 3. one-byte (1/255) parameter nudges on the static song instance 23 are caught over the song
    let (init23, ev23, _) = song_inputs(&fx, 23);
    // (P0 is inert there: its voice count saturates at 16 lines for any P0 ≥ 7/12; P7/P8/P10 do not reach mode A)
    for idx in [1usize, 2, 3, 4, 5, 6, 9] {
        let delta = if init23[idx] > 0.5 { -1.0 / 255.0 } else { 1.0 / 255.0 };
        let mut init = init23;
        init[idx] += delta; // never clamped: no parameter of this instance sits within 1/255 of both ends
        let mut tap = Tap::open(&fx.out.join("taps32/dev23.tap"), Some(SONG_BLOCKS));
        let r = catch_unwind(AssertUnwindSafe(|| replay("dev 23 nudged", init, &ev23, &mut tap, CONTROL_TOL, None)));
        assert!(r.is_err(), "dev 23: nudging P{idx} by {delta} went unnoticed");
    }
    // 4. dropping the recorded automation of a song instance must be detected (events matter)
    let (init, _, mut tap) = song_inputs(&fx, 24);
    let none: Events = HashMap::new();
    let r = catch_unwind(AssertUnwindSafe(|| replay("dev 24 no events", init, &none, &mut tap, SONG_TOL, None)));
    assert!(r.is_err(), "ignoring the song's parameter automation went unnoticed");
}
