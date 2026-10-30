//! Optional external-reference parity: ID 25 (percussion synth + the shared 16-voice scheduler) vs the original engine's
//! output, replayed through the isolated Wine harness (local only: third-party binary + song never enter git).
//!
//! `GLORP_REF_ROOT` = the `ws-intros/out` directory that holds `389811-2.bin` (read only for the ID 25 initial
//! parameters) and `glorp-win64/analysis/harness/out/id25impl/` with
//!   * `ev.csv` + `taps/dev08.tap`: the **whole song** run (78,292 blocks; song device 8 is the one ID 25 instance) with
//!     every note-on/off (with in-block offsets) and every `SetParam` the engine issued, and the device's recorded
//!     per-block buffers;
//!   * `vec/manifest.json`, `vec/NAME.{csv,tap}`: synthetic vectors (`vec/gen.py`: song-like and extreme parameters,
//!     polyphony beyond 16, retrigger, same-offset ordering, offsets beyond a block, queue overflow, parameter sweeps)
//!     that the harness fed to a fresh original instance and recorded (`GLORP_OUT_TAP`).
//!
//! Tap block = 8 × 128 f32: in L, in R, aux L, aux R, then out L, out R, aux L, aux R. The synth ignores the inputs,
//! zeroes and fills out L / out R and leaves the aux pair alone (asserted). `GLORP_REF_ROOT` unset => the tests skip
//! explicitly (they do not prove parity); set but any fixture file missing => they fail loudly, naming the file.
use glorp::id25::{Error, Id25, PARAMS};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

const B: usize = 128;
const DEV: usize = 8;
const SONG_BLOCKS: usize = 78_292;

/// Tight per-sample bound `ABS + REL·|ref|` (a few ulp). Only the negative controls use it, on fixtures whose baseline
/// passes it (`CONTROL_VECTORS`, asserted), so that a 1/255 parameter nudge is still decisively outside it.
const ABS: f64 = 1e-7;
const REL: f64 = 3e-7;
/// Parity bound: the largest absolute error of a fixture may not exceed `PEAK_REL · max|ref|` of that fixture. The
/// implementation uses ordinary `libm` exponentials, the original's C library (Wine's msvcrt) rounds some of them
/// differently, and the voice's 60 dB shelves magnify those last-bit differences. Measured with this build (release,
/// x86_64 macOS): whole song (20.04 M samples) max 6.857e-4 at peak 5.30 (1.3e-4 of peak), 93.13 % of samples exact;
/// 44 vectors worst `s00lo` 1.227e-2 at peak 7.40 (1.66e-3 of peak), `vec11` 9.6e-3 at peak 49.3 (2.0e-4); the bound
/// is 1.5x the worst. Most fixtures (sweeps, `vec02/05`) stay within a few ulp. This is **not** bit parity.
const PEAK_REL: f64 = 2.5e-3;
/// Vectors whose `libm` baseline still meets the tight bound (asserted): the negative controls run on these.
const CONTROL_VECTORS: [&str; 2] = ["vec02", "vec05"];

#[derive(Clone, Copy, PartialEq)]
enum Tol {
    /// Fixture-level `PEAK_REL` parity bound (checked after the replay).
    Parity,
    /// Per-sample tight bound, panics with `got … want …` at the first violation.
    Tight,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Ev {
    On(u8, u32),
    Off(u8, u32),
    Param(usize, f32),
}
type Events = HashMap<usize, Vec<Ev>>;

#[derive(Default, Debug)]
struct Stats {
    samples: usize,
    exact: usize,
    max_abs: f64,
    max_ref: f64,
    nonzero: usize,
    refused: usize,
    max_voices: usize,
}

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}

fn require(p: &Path) -> PathBuf {
    assert!(p.is_file(), "GLORP_REF_ROOT is set but the fixture {} is missing", p.display());
    p.to_path_buf()
}

/// Initial parameters of the ID 25 song device (the 16th chunk float is the byte size). Exactly one instance.
fn chunk(blob: &[u8]) -> Result<[f32; PARAMS], String> {
    let u32at = |p: usize| u32::from_le_bytes(blob[p..p + 4].try_into().unwrap()) as usize;
    let n = u32at(16);
    let mut p = 20;
    let mut found = Vec::new();
    for i in 0..n {
        let (id, size) = (blob[p], u32at(p + 1));
        let c = &blob[p + 5..p + 5 + size];
        p += 5 + size;
        if id == 25 {
            if size != 4 * PARAMS + 4 {
                return Err(format!("ID 25 chunk is {size} bytes"));
            }
            found.push((i, f32s(c)[..PARAMS].try_into().unwrap()));
        }
    }
    match found.as_slice() {
        [(d, init)] if *d == DEV => Ok(*init),
        other => Err(format!("expected exactly one ID 25 device (song device {DEV}), found {}", other.len())),
    }
}

/// Event log rows of device `DEV`: `block,dev,id,kind,a,b,c` (`on`: note,vel,offset; `off`: note,offset,0;
/// `param`: idx,value,bits). A malformed row is a hard error.
fn events_of(path: &Path) -> Events {
    let mut m: Events = HashMap::new();
    for (n, line) in BufReader::new(std::fs::File::open(path).unwrap()).lines().enumerate().skip(1) {
        let line = line.unwrap();
        let f: Vec<&str> = line.split(',').collect();
        assert_eq!(f.len(), 7, "{}:{}: bad row {line:?}", path.display(), n + 1);
        if f[1].parse::<usize>().unwrap() != DEV {
            continue;
        }
        let blk: usize = f[0].parse().unwrap();
        let ev = match f[3] {
            "on" => Ev::On(f[4].parse().unwrap(), f[6].parse().unwrap()),
            "off" => Ev::Off(f[4].parse().unwrap(), f[5].parse::<f64>().unwrap() as u32),
            "param" => Ev::Param(f[4].parse().unwrap(), f32::from_bits(f[6].parse().unwrap())),
            k => panic!("{}:{}: unknown event kind {k}", path.display(), n + 1),
        };
        m.entry(blk).or_default().push(ev);
    }
    m
}

/// Replays `nblocks` blocks of the tap through a fresh `Id25` built from `init`, applying each block's events in log
/// order before the block. `refuse_ok`: tolerate `QueueFull` (counted) instead of panicking.
fn replay(
    what: &str,
    init: [f32; PARAMS],
    nblocks: usize,
    tap: &mut impl Read,
    events: &Events,
    refuse_ok: bool,
    tol: Tol,
) -> Stats {
    let mut d = Id25::new(&init).unwrap_or_else(|e| panic!("{what}: {e:?}"));
    let mut s = Stats::default();
    let mut raw = vec![0u8; 8 * B * 4];
    for b in 0..nblocks {
        for ev in events.get(&b).map(|v| v.as_slice()).unwrap_or(&[]) {
            let r = match *ev {
                Ev::On(n, o) => d.note_on(n, o),
                Ev::Off(n, o) => d.note_off(n, o),
                Ev::Param(i, v) => d.set_param(i, v),
            };
            match r {
                Ok(()) => {}
                Err(Error::QueueFull) if refuse_ok => s.refused += 1,
                Err(e) => panic!("{what} block {b}: {ev:?} rejected: {e:?}"),
            }
        }
        tap.read_exact(&mut raw).unwrap_or_else(|e| panic!("{what}: short tap at block {b}: {e}"));
        let blk = f32s(&raw);
        let (mut l, mut r) = (vec![7.0f32; B], vec![7.0f32; B]);
        d.process(&mut l, &mut r);
        s.max_voices = s.max_voices.max(d.active_voices());
        assert_eq!(&blk[2 * B..4 * B], &blk[6 * B..8 * B], "{what} block {b}: the original touched the aux pair");
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
                    tol == Tol::Parity || e <= ABS + REL * want.abs() as f64,
                    "{what} block {b} ch {c} i {i}: got {} want {want} (err {e:e})",
                    got[i]
                );
            }
        }
    }
    if tol == Tol::Parity {
        assert!(
            s.max_abs <= PEAK_REL * s.max_ref,
            "{what}: max abs err {:e} exceeds {PEAK_REL} of the peak {} (parity got/want mismatch)",
            s.max_abs,
            s.max_ref
        );
    }
    s
}

struct Fixtures {
    out: PathBuf,
    init: [f32; PARAMS],
}

fn fixtures() -> Option<Fixtures> {
    let root = PathBuf::from(std::env::var_os("GLORP_REF_ROOT")?);
    let blob = std::fs::read(require(&root.join("389811-2.bin"))).unwrap();
    let init = chunk(&blob).unwrap_or_else(|e| panic!("389811-2.bin: {e}"));
    Some(Fixtures { out: root.join("glorp-win64/analysis/harness/out/id25impl"), init })
}

fn vector_names(fx: &Fixtures) -> Vec<(String, usize)> {
    let txt = std::fs::read_to_string(require(&fx.out.join("vec/manifest.json"))).unwrap();
    let mut v: Vec<(String, usize)> = txt
        .lines()
        .filter_map(|l| {
            let (k, n) = l.trim().trim_end_matches(',').split_once(':')?;
            Some((k.trim().trim_matches('"').to_string(), n.trim().parse().ok()?))
        })
        .collect();
    v.sort();
    assert_eq!(v.len(), 14 + 30, "manifest.json: expected 14 scenario + 30 sweep vectors, got {}", v.len());
    v
}

fn vector(fx: &Fixtures, name: &str, n: usize) -> (Events, Vec<u8>) {
    let ev = events_of(&require(&fx.out.join(format!("vec/{name}.csv"))));
    let tap = std::fs::read(require(&fx.out.join(format!("vec/{name}.tap")))).unwrap();
    assert_eq!(tap.len(), n * 8 * B * 4, "{name}: tap size does not match the manifest");
    (ev, tap)
}

/// Replays every synthetic vector; returns per-vector stats.
fn run_vectors(fx: &Fixtures, tweak: &dyn Fn(&str, &mut Events)) -> Vec<(String, Stats)> {
    vector_names(fx)
        .into_iter()
        .map(|(name, n)| {
            let (mut ev, tap) = vector(fx, &name, n);
            tweak(&name, &mut ev);
            let s = replay(&name, fx.init, n, &mut &tap[..], &ev, name == "vec11", Tol::Parity);
            (name, s)
        })
        .collect()
}

fn song(fx: &Fixtures, tweak: &dyn Fn(&mut Events)) -> Stats {
    let mut ev = events_of(&require(&fx.out.join("ev.csv")));
    tweak(&mut ev);
    let path = require(&fx.out.join("taps/dev08.tap"));
    let len = std::fs::metadata(&path).unwrap().len() as usize;
    assert_eq!(len, SONG_BLOCKS * 8 * B * 4, "{}: not the whole-song tap (78,292 blocks)", path.display());
    let mut f = BufReader::with_capacity(1 << 20, std::fs::File::open(path).unwrap());
    replay("song device 8", fx.init, SONG_BLOCKS, &mut f, &ev, false, Tol::Parity)
}

fn report(what: &str, s: &Stats) {
    eprintln!(
        "{what}: {} samples, {} exact ({:.4} %), {} nonzero refs, max |ref| {:.4}, max abs err {:.3e}, max voices {}",
        s.samples,
        s.exact,
        100.0 * s.exact as f64 / s.samples as f64,
        s.nonzero,
        s.max_ref,
        s.max_abs,
        s.max_voices
    );
}

#[test]
fn whole_song_matches_the_original() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let s = song(&fx, &|_| {});
    report("song device 8 (whole song)", &s);
    assert_eq!(s.samples, SONG_BLOCKS * B * 2);
    assert!(s.nonzero > 100_000, "the whole-song tap is too quiet to mean anything: {} nonzero", s.nonzero);
}

#[test]
fn synthetic_vectors_match_the_original() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let mut quiet = Vec::new();
    for (name, s) in run_vectors(&fx, &|_, _| {}) {
        report(&name, &s);
        if s.nonzero == 0 {
            quiet.push(name.clone());
        }
        if name == "vec11" {
            assert_eq!(s.refused, 300 - 256, "vec11: 300 note-ons in one block, 256 slots");
            assert_eq!(s.max_voices, 16);
        }
        if name == "vec06" {
            assert_eq!(s.max_voices, 16, "vec06: 17 simultaneous notes");
        }
    }
    // only the all-zero-parameter vector (gain 0) and the sweeps that zero the gain/body may be silent
    for q in &quiet {
        assert!(q == "vec04" || q == "s09lo", "{q}: unexpectedly silent reference");
    }
}

// ------------------------------------------------------------------ negative controls

/// The mutated replay must fail with the `got … want …` mismatch (not with some other panic).
fn must_mismatch<R>(what: &str, f: impl FnOnce() -> R + std::panic::UnwindSafe) {
    let err = std::panic::catch_unwind(f).err().unwrap_or_else(|| panic!("the mutation '{what}' was not detected"));
    let msg = err.downcast_ref::<String>().cloned().or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
    assert!(msg.contains("got") && msg.contains("want"), "failed for another reason: {msg}");
}

fn nudge(i: usize) -> impl Fn(&str, &mut Events) {
    move |_, ev| {
        for e in ev.get_mut(&0).unwrap() {
            if let Ev::Param(k, v) = e {
                if *k == i {
                    *v = if *v + 1.0 / 255.0 <= 1.0 { *v + 1.0 / 255.0 } else { *v - 1.0 / 255.0 };
                }
            }
        }
    }
}

#[test]
fn control_vectors_meet_the_tight_bound() {
    let Some(fx) = fixtures() else { return };
    for name in CONTROL_VECTORS {
        let n = vector_names(&fx).into_iter().find(|(k, _)| k == name).unwrap().1;
        let (ev, tap) = vector(&fx, name, n);
        let s = replay(name, fx.init, n, &mut &tap[..], &ev, false, Tol::Tight);
        assert!(s.nonzero > 10_000, "{name}: control vector too quiet ({} nonzero)", s.nonzero);
        report(name, &s);
    }
}

#[test]
fn negative_control_flipped_reference_bit_is_caught() {
    let Some(fx) = fixtures() else { return };
    let (ev, mut tap) = vector(&fx, "vec02", 200);
    let blk = (0..200).find(|b| tap[(b * 8 + 4) * B * 4..(b * 8 + 5) * B * 4].iter().any(|&x| x != 0)).unwrap();
    tap[((blk * 8 + 4) * B + 5) * 4 + 3] ^= 0x40;
    must_mismatch("flipped bit", || {
        replay("flip", fx.init, blk + 2, &mut &tap[..], &ev, false, Tol::Tight);
    });
}

#[test]
fn negative_control_parameter_nudges_are_caught_and_unread_ones_are_not() {
    let Some(fx) = fixtures() else { return };
    let fixtures: Vec<(&str, usize, Events, Vec<u8>)> = CONTROL_VECTORS
        .iter()
        .map(|&name| {
            let n = vector_names(&fx).into_iter().find(|(k, _)| k == name).unwrap().1;
            let (ev, tap) = vector(&fx, name, n);
            (name, n, ev, tap)
        })
        .collect();
    // a nudge must be caught by the tight bound on at least one control vector (a vector that never reads the
    // parameter, or sets it at block 0 only where it is inaudible, may not show it)
    for i in [0usize, 2, 3, 5, 6, 7, 9, 10, 11, 12, 13, 14] {
        let caught = fixtures.iter().any(|(name, n, ev0, tap)| {
            let mut ev = ev0.clone();
            if !ev.get(&0).is_some_and(|v| v.iter().any(|e| matches!(e, Ev::Param(k, _) if *k == i))) {
                return false;
            }
            nudge(i)(name, &mut ev);
            std::panic::catch_unwind(|| replay("nudge", fx.init, *n, &mut &tap[..], &ev, false, Tol::Tight).max_abs).is_err()
        });
        assert!(caught, "a 1/255 nudge of P{i} was not detected on any control vector");
    }
    for i in [1usize, 4, 8] {
        for (name, n, ev0, tap) in &fixtures {
            let mut ev = ev0.clone();
            nudge(i)(name, &mut ev);
            replay("inert", fx.init, *n, &mut &tap[..], &ev, false, Tol::Tight); // never read: must still match
        }
    }
}

#[test]
fn negative_control_scheduler_mutations_are_caught() {
    let Some(fx) = fixtures() else { return };
    // offsets dropped (all events fire at sample 0 of their block)
    let (ev, tap) = vector(&fx, "vec06", 400);
    let no_offsets: Events = ev
        .iter()
        .map(|(b, v)| {
            let v = v
                .iter()
                .map(|e| match *e {
                    Ev::On(n, _) => Ev::On(n, 0),
                    Ev::Off(n, _) => Ev::Off(n, 0),
                    p => p,
                })
                .collect();
            (*b, v)
        })
        .collect();
    must_mismatch("offsets dropped", || {
        replay("no-offsets", fx.init, 400, &mut &tap[..], &no_offsets, false, Tol::Parity);
    });
    // note-offs dropped: voices never free, the 16-voice limit then eats the 23rd.. notes (vec14)
    let (ev, tap) = vector(&fx, "vec14", 340);
    let no_off: Events = ev.iter().map(|(b, v)| (*b, v.iter().copied().filter(|e| !matches!(e, Ev::Off(..))).collect())).collect();
    must_mismatch("note-offs dropped", || {
        replay("no-offs", fx.init, 340, &mut &tap[..], &no_off, false, Tol::Parity);
    });
    // same-offset order reversed (vec08 depends on queue order)
    let (ev, tap) = vector(&fx, "vec08", 300);
    let rev: Events = ev.iter().map(|(b, v)| (*b, if *b == 0 { v.iter().rev().copied().collect() } else { v.clone() })).collect();
    must_mismatch("same-offset order reversed", || {
        replay("reversed", fx.init, 300, &mut &tap[..], &rev, false, Tol::Parity);
    });
}

#[test]
fn negative_control_dropped_song_automation_is_caught() {
    let Some(fx) = fixtures() else { return };
    let ev = events_of(&require(&fx.out.join("ev.csv")));
    let later: Vec<(f32, usize)> = {
        let mut seen: Vec<(f32, usize)> = Vec::new();
        for e in ev.iter().filter(|(b, _)| **b > 0).flat_map(|(_, v)| v.iter()) {
            if let Ev::Param(i, v) = e {
                if !seen.iter().any(|(x, k)| x == v && k == i) {
                    seen.push((*v, *i));
                }
            }
        }
        seen
    };
    assert!(later.len() > 100, "the song automates the instrument: {} distinct (value, index) rows parsed", later.len());
    assert!(later.iter().all(|(_, i)| *i == 0), "only P0 is automated in this song");
    let stripped: Events = ev.iter().map(|(b, v)| (*b, v.iter().copied().filter(|e| *b == 0 || !matches!(e, Ev::Param(..))).collect())).collect();
    must_mismatch("song automation dropped", || {
        song(&fx, &|e| *e = stripped.clone());
    });
}

// ------------------------------------------------------------------ always-on hardening of the harness itself

#[test]
fn missing_fixtures_are_a_hard_error() {
    let p = std::env::temp_dir().join("glorp-id25-definitely-missing.csv");
    let err = std::panic::catch_unwind(|| require(&p)).err().expect("require must panic");
    let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("fixture") && msg.contains("glorp-id25-definitely-missing.csv"), "{msg}");
}

#[test]
fn chunk_parser_refuses_a_song_without_exactly_one_id25() {
    let mut blob = vec![0u8; 20];
    blob[16..20].copy_from_slice(&1u32.to_le_bytes());
    blob.push(7);
    blob.extend_from_slice(&32u32.to_le_bytes());
    blob.extend_from_slice(&[0u8; 32]);
    assert!(chunk(&blob).unwrap_err().contains("found 0"));
    let mut two = vec![0u8; 20];
    two[16..20].copy_from_slice(&2u32.to_le_bytes());
    for _ in 0..2 {
        two.push(25);
        two.extend_from_slice(&64u32.to_le_bytes());
        two.extend_from_slice(&[0u8; 64]);
    }
    assert!(chunk(&two).unwrap_err().contains("found 2"));
}

#[test]
fn short_tap_is_a_hard_error() {
    let tap = vec![0u8; 8 * B * 4 * 2 + 17];
    let err = std::panic::catch_unwind(|| replay("short", [0.5; PARAMS], 3, &mut &tap[..], &Events::new(), false, Tol::Tight)).err().unwrap();
    let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("short tap"), "{msg}");
}

#[test]
fn event_parser_rejects_malformed_rows() {
    let p = std::env::temp_dir().join("glorp-id25-bad-events.csv");
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,8,25,boom,1,2,3\n").unwrap();
    let err = std::panic::catch_unwind(|| events_of(&p)).err().unwrap();
    assert!(err.downcast_ref::<String>().cloned().unwrap_or_default().contains("unknown event kind"));
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,8,25,on,1,2\n").unwrap();
    assert!(std::panic::catch_unwind(|| events_of(&p)).is_err());
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,8,25,on,60,127,12\n3,9,25,on,1,1,1\n1,8,25,off,60,7,0\n2,8,25,param,3,0.5,1056964608\n").unwrap();
    let m = events_of(&p);
    assert_eq!(m[&0], vec![Ev::On(60, 12)]);
    assert_eq!(m[&1], vec![Ev::Off(60, 7)]);
    assert_eq!(m[&2], vec![Ev::Param(3, 0.5)]);
    assert!(!m.contains_key(&3), "other devices' rows are ignored");
}
