//! Optional external-reference parity: ID 22 (circle oscillator through the 3D trig map + the shared 16-voice scheduler)
//! vs the original engine's output, replayed through the isolated Wine harness (local only: third-party binary + song
//! never enter git).
//!
//! `GLORP_REF_ROOT` = the `ws-intros/out` directory that holds `389811-2.bin` (read only for the ID 22 initial
//! parameters) and `glorp-win64/analysis/harness/out/id22impl/` with
//!   * `ev.csv` + `taps/dev07.tap`: the **whole song** run (78,292 blocks; song device 7 is the one ID 22 instance) with
//!     every note-on/off (with in-block offsets) and every `SetParam` the engine issued, and the device's recorded
//!     per-block buffers;
//!   * `vec/manifest.json`, `vec/NAME.{csv,tap}`: synthetic vectors (`vec/gen.py`: song-like, LFO mode, chaotic state
//!     gain, extremes, polyphony beyond 16, retrigger, same-offset ordering, offsets beyond a block, queue overflow,
//!     mid-note / lane-ramp automation, long notes, weight-guard edge, parameter sweeps) that the harness fed to a fresh
//!     original instance and recorded (`GLORP_OUT_TAP`), plus `unsafe_*` vectors: what the original does **outside**
//!     the weight guard (NaN), which `Id22` refuses.
//!
//! Tap block = 8 × 128 f32: in L, in R, aux L, aux R, then out L, out R, aux L, aux R. The synth ignores the inputs,
//! zeroes and fills out L / out R and leaves the aux pair alone (asserted). `GLORP_REF_ROOT` unset => the tests skip
//! explicitly (they do not prove parity); set but any fixture file missing => they fail loudly, naming the file.
use glorp::id22::{check, Error, Id22, PARAMS};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

const B: usize = 128;
const DEV: usize = 7;
const SONG_BLOCKS: usize = 78_292;

/// Tight per-sample bound `ABS + REL·|ref|` (a few ulp). Whole-song, vector and negative-control runs all use it when
/// the measurement allows (see `PARITY`).
const ABS: f64 = 1e-7;
const REL: f64 = 3e-7;
/// Parity bound relative to the fixture's peak: the largest absolute error of a fixture may not exceed `PEAK_REL ·
/// max|ref|`. Set from the measurement recorded in `research/glorp-devices.md` §ID 22.
const PEAK_REL: f64 = 1e-6;
/// Vectors used by the negative controls (tight bound, asserted by `control_vectors_meet_the_tight_bound`): the song's
/// settings with sustain 1 (`vec01`), the LFO mode (`vec02`) and sustain 0 (`s07lo`: the decay time P6 is audible only
/// when the sustain is below 1).
const CONTROL_VECTORS: [&str; 3] = ["vec01", "vec02", "s07lo"];
/// Vectors whose reference is legitimately silent (gain / level / radius 0).
const SILENT: [&str; 4] = ["vec05", "s00lo", "s09lo", "s14lo"];

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
    first_bad: Option<usize>,
}

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}

fn require(p: &Path) -> PathBuf {
    assert!(p.is_file(), "GLORP_REF_ROOT is set but the fixture {} is missing", p.display());
    p.to_path_buf()
}

/// Initial parameters of the ID 22 song device (the 16th chunk float is the byte size). Exactly one instance.
fn chunk(blob: &[u8]) -> Result<[f32; PARAMS], String> {
    let u32at = |p: usize| u32::from_le_bytes(blob[p..p + 4].try_into().unwrap()) as usize;
    let n = u32at(16);
    let mut p = 20;
    let mut found = Vec::new();
    for i in 0..n {
        let (id, size) = (blob[p], u32at(p + 1));
        let c = &blob[p + 5..p + 5 + size];
        p += 5 + size;
        if id == 22 {
            if size != 4 * PARAMS + 4 {
                return Err(format!("ID 22 chunk is {size} bytes"));
            }
            found.push((i, f32s(c)[..PARAMS].try_into().unwrap()));
        }
    }
    match found.as_slice() {
        [(d, init)] if *d == DEV => Ok(*init),
        other => Err(format!("expected exactly one ID 22 device (song device {DEV}), found {}", other.len())),
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

/// Replays `nblocks` blocks of the tap through a fresh `Id22` built from `init`, applying each block's events in log
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
    let mut d = Id22::new(&init).unwrap_or_else(|e| panic!("{what}: {e:?}"));
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
                assert!(want.is_finite() && got[i].is_finite(), "{what} block {b} ch {c} i {i}: non-finite (got {} want {want})", got[i]);
                let e = (got[i] as f64 - want as f64).abs();
                s.samples += 1;
                s.max_abs = s.max_abs.max(e);
                s.max_ref = s.max_ref.max(want.abs() as f64);
                s.nonzero += (want != 0.0) as usize;
                s.exact += (got[i] == want) as usize;
                if e > ABS + REL * want.abs() as f64 && s.first_bad.is_none() {
                    s.first_bad = Some(b);
                }
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
    Some(Fixtures { out: root.join("glorp-win64/analysis/harness/out/id22impl"), init })
}

fn manifest(fx: &Fixtures) -> Vec<(String, usize)> {
    let txt = std::fs::read_to_string(require(&fx.out.join("vec/manifest.json"))).unwrap();
    let mut v: Vec<(String, usize)> = txt
        .lines()
        .filter_map(|l| {
            let (k, n) = l.trim().trim_end_matches(',').split_once(':')?;
            Some((k.trim().trim_matches('"').to_string(), n.trim().parse().ok()?))
        })
        .collect();
    v.sort();
    assert_eq!(v.len(), 26 + 30 + 3, "manifest.json: expected 26 scenario + 30 sweep + 3 unsafe vectors, got {}", v.len());
    v
}

/// The vectors the original answers finitely (everything but `unsafe_*`).
fn parity_vectors(fx: &Fixtures) -> Vec<(String, usize)> {
    manifest(fx).into_iter().filter(|(k, _)| !k.starts_with("unsafe_")).collect()
}

fn vector(fx: &Fixtures, name: &str, n: usize) -> (Events, Vec<u8>) {
    let ev = events_of(&require(&fx.out.join(format!("vec/{name}.csv"))));
    let tap = std::fs::read(require(&fx.out.join(format!("vec/{name}.tap")))).unwrap();
    assert_eq!(tap.len(), n * 8 * B * 4, "{name}: tap size does not match the manifest");
    (ev, tap)
}

fn blocks_of(fx: &Fixtures, name: &str) -> usize {
    manifest(fx).into_iter().find(|(k, _)| k == name).unwrap_or_else(|| panic!("{name} is not in the manifest")).1
}

/// Replays every parity vector; returns per-vector stats.
fn run_vectors(fx: &Fixtures, tweak: &dyn Fn(&str, &mut Events)) -> Vec<(String, Stats)> {
    parity_vectors(fx)
        .into_iter()
        .map(|(name, n)| {
            let (mut ev, tap) = vector(fx, &name, n);
            tweak(&name, &mut ev);
            let s = replay(&name, fx.init, n, &mut &tap[..], &ev, name == "vec13", Tol::Parity);
            (name, s)
        })
        .collect()
}

fn song(fx: &Fixtures, tweak: &dyn Fn(&mut Events)) -> Stats {
    let mut ev = events_of(&require(&fx.out.join("ev.csv")));
    tweak(&mut ev);
    let path = require(&fx.out.join("taps/dev07.tap"));
    let len = std::fs::metadata(&path).unwrap().len() as usize;
    assert_eq!(len, SONG_BLOCKS * 8 * B * 4, "{}: not the whole-song tap (78,292 blocks)", path.display());
    let mut f = BufReader::with_capacity(1 << 20, std::fs::File::open(path).unwrap());
    replay("song device 7", fx.init, SONG_BLOCKS, &mut f, &ev, false, Tol::Parity)
}

fn report(what: &str, s: &Stats) {
    eprintln!(
        "{what}: {} samples, {} exact ({:.4} %), {} nonzero refs, max |ref| {:.4}, max abs err {:.3e}, max voices {}, first block off the tight bound {:?}",
        s.samples,
        s.exact,
        100.0 * s.exact as f64 / s.samples as f64,
        s.nonzero,
        s.max_ref,
        s.max_abs,
        s.max_voices,
        s.first_bad
    );
}

#[test]
fn whole_song_matches_the_original() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let s = song(&fx, &|_| {});
    report("song device 7 (whole song)", &s);
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
        if name == "vec13" {
            assert_eq!(s.refused, 300 - 256, "vec13: 300 note-ons in one block, 256 slots");
            assert_eq!(s.max_voices, 16);
        }
        if name == "vec07" {
            assert_eq!(s.max_voices, 16, "vec07: 17 simultaneous notes");
        }
    }
    for q in &quiet {
        assert!(SILENT.contains(&q.as_str()), "{q}: unexpectedly silent reference");
    }
    assert_eq!(quiet.len(), SILENT.len(), "silent vectors: {quiet:?}");
}

#[test]
fn outside_the_weight_guard_the_original_returns_nan_and_the_crate_refuses() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    for (name, n) in manifest(&fx).into_iter().filter(|(k, _)| k.starts_with("unsafe_")) {
        let (ev, tap) = vector(&fx, &name, n);
        // the parameters the vector wrote at block 0
        let mut p = fx.init;
        for e in ev.get(&0).unwrap() {
            if let Ev::Param(i, v) = e {
                p[*i] = *v;
            }
        }
        assert_eq!(check(&p), Err(Error::Weights), "{name}: the crate must refuse what the original overflows on");
        assert_eq!(Id22::new(&p).err(), Some(Error::Weights), "{name}");
        let out = f32s(&tap);
        let nan = (0..n).flat_map(|b| (4..6).flat_map(move |c| (0..B).map(move |i| b * 8 * B + c * B + i))).filter(|&k| out[k].is_nan()).count();
        assert!(nan > 0, "{name}: the original did not produce a NaN, the guard would be unfounded");
        eprintln!("{name}: the original returned {nan} NaN samples; Id22 refuses ({:?})", check(&p));
    }
}

// ------------------------------------------------------------------ negative controls

/// The mutated replay must fail with the `got … want …` mismatch (not with some other panic).
fn must_mismatch<R>(what: &str, f: impl FnOnce() -> R + std::panic::UnwindSafe) {
    let err = std::panic::catch_unwind(f).err().unwrap_or_else(|| panic!("the mutation '{what}' was not detected"));
    let msg = err.downcast_ref::<String>().cloned().or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
    assert!(msg.contains("got") && msg.contains("want"), "failed for another reason: {msg}");
}

fn nudge(i: usize, d: f32) -> impl Fn(&str, &mut Events) {
    move |_, ev| {
        for e in ev.get_mut(&0).unwrap() {
            if let Ev::Param(k, v) = e {
                if *k == i {
                    *v = if i == 11 {
                        // P11 is a switch at 0.5 (inclusive): only a move across it is audible
                        if *v >= 0.5 { 0.499_999_94 } else { 0.5 }
                    } else if *v + d <= 1.0 {
                        *v + d
                    } else {
                        *v - d
                    };
                }
            }
        }
    }
}

#[test]
fn control_vectors_meet_the_tight_bound() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    for name in CONTROL_VECTORS {
        let n = blocks_of(&fx, name);
        let (ev, tap) = vector(&fx, name, n);
        let s = replay(name, fx.init, n, &mut &tap[..], &ev, false, Tol::Tight);
        assert!(s.nonzero > 10_000, "{name}: control vector too quiet ({} nonzero)", s.nonzero);
        report(name, &s);
    }
}

#[test]
fn negative_control_flipped_reference_bit_is_caught() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let (ev, mut tap) = vector(&fx, "vec02", 200);
    let blk = (0..200).find(|b| tap[(b * 8 + 4) * B * 4..(b * 8 + 5) * B * 4].iter().any(|&x| x != 0)).unwrap();
    tap[((blk * 8 + 4) * B + 5) * 4 + 3] ^= 0x40;
    must_mismatch("flipped bit", || {
        replay("flip", fx.init, blk + 2, &mut &tap[..], &ev, false, Tol::Tight);
    });
}

#[test]
fn negative_control_parameter_nudges_are_caught() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let fixtures: Vec<(&str, usize, Events, Vec<u8>)> = CONTROL_VECTORS
        .iter()
        .map(|&name| {
            let n = blocks_of(&fx, name);
            let (ev, tap) = vector(&fx, name, n);
            (name, n, ev, tap)
        })
        .collect();
    // every parameter, nudged by 1/255 (P4 by a full iteration step, P11 across its switch), must leave the tight bound on at least one
    // control vector (P10 only acts behind P11 >= 0.5: vec02 is in LFO mode; P6 only with a sustain below 1: s07lo)
    for i in 0..PARAMS {
        let step = if i == 4 { 1.0 / 15.0 } else { 1.0 / 255.0 };
        let caught = fixtures.iter().any(|(name, n, ev0, tap)| {
            let mut ev = ev0.clone();
            nudge(i, step)(name, &mut ev);
            std::panic::catch_unwind(|| replay("nudge", fx.init, *n, &mut &tap[..], &ev, false, Tol::Tight).max_abs).is_err()
        });
        assert!(caught, "a nudge of P{i} was not detected on any control vector");
    }
}

#[test]
fn negative_control_scheduler_mutations_are_caught() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let (ev, tap) = vector(&fx, "vec07", 300);
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
        replay("no-offsets", fx.init, 300, &mut &tap[..], &no_offsets, false, Tol::Parity);
    });
    // note-offs dropped: voices never free, the 16-voice limit then eats later notes (vec16)
    let (ev, tap) = vector(&fx, "vec16", 300);
    let no_off: Events = ev.iter().map(|(b, v)| (*b, v.iter().copied().filter(|e| !matches!(e, Ev::Off(..))).collect())).collect();
    must_mismatch("note-offs dropped", || {
        replay("no-offs", fx.init, 300, &mut &tap[..], &no_off, false, Tol::Parity);
    });
    // same-offset order reversed (vec09 depends on queue order)
    let (ev, tap) = vector(&fx, "vec09", 200);
    let rev: Events = ev.iter().map(|(b, v)| (*b, if *b == 0 { v.iter().rev().copied().collect() } else { v.clone() })).collect();
    must_mismatch("same-offset order reversed", || {
        replay("reversed", fx.init, 200, &mut &tap[..], &rev, false, Tol::Parity);
    });
    // lane writes dropped (vec12: b, a, n, radius ramp every block)
    let (ev, tap) = vector(&fx, "vec12", 520);
    let flat: Events = ev.iter().map(|(b, v)| (*b, v.iter().copied().filter(|e| *b == 0 || !matches!(e, Ev::Param(..))).collect())).collect();
    must_mismatch("lane ramps dropped", || {
        replay("flat", fx.init, 520, &mut &tap[..], &flat, false, Tol::Parity);
    });
}

#[test]
fn negative_control_dropped_song_automation_is_caught() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let ev = events_of(&require(&fx.out.join("ev.csv")));
    let mut automated = [false; PARAMS];
    let mut distinct: Vec<(u32, usize)> = Vec::new();
    for e in ev.iter().filter(|(b, _)| **b > 0).flat_map(|(_, v)| v.iter()) {
        if let Ev::Param(i, v) = e {
            automated[*i] = true;
            if !distinct.contains(&(v.to_bits(), *i)) {
                distinct.push((v.to_bits(), *i));
            }
        }
    }
    assert!(distinct.len() > 100, "the song automates the instrument: {} distinct (value, index) rows parsed", distinct.len());
    eprintln!("song device 7 automated parameters: {:?}", (0..PARAMS).filter(|&i| automated[i]).collect::<Vec<_>>());
    let stripped: Events = ev.iter().map(|(b, v)| (*b, v.iter().copied().filter(|e| *b == 0 || !matches!(e, Ev::Param(..))).collect())).collect();
    must_mismatch("song automation dropped", || {
        song(&fx, &|e| *e = stripped.clone());
    });
}

// ------------------------------------------------------------------ always-on hardening of the harness itself

#[test]
fn missing_fixtures_are_a_hard_error() {
    let p = std::env::temp_dir().join("glorp-id22-definitely-missing.csv");
    let err = std::panic::catch_unwind(|| require(&p)).err().expect("require must panic");
    let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("fixture") && msg.contains("glorp-id22-definitely-missing.csv"), "{msg}");
}

#[test]
fn chunk_parser_refuses_a_song_without_exactly_one_id22() {
    let mut blob = vec![0u8; 20];
    blob[16..20].copy_from_slice(&1u32.to_le_bytes());
    blob.push(7);
    blob.extend_from_slice(&32u32.to_le_bytes());
    blob.extend_from_slice(&[0u8; 32]);
    assert!(chunk(&blob).unwrap_err().contains("found 0"));
    let mut two = vec![0u8; 20];
    two[16..20].copy_from_slice(&2u32.to_le_bytes());
    for _ in 0..2 {
        two.push(22);
        two.extend_from_slice(&64u32.to_le_bytes());
        two.extend_from_slice(&[0u8; 64]);
    }
    assert!(chunk(&two).unwrap_err().contains("found 2"));
    // one ID 22, but not as song device 7
    let mut one = vec![0u8; 20];
    one[16..20].copy_from_slice(&2u32.to_le_bytes());
    one.push(7);
    one.extend_from_slice(&32u32.to_le_bytes());
    one.extend_from_slice(&[0u8; 32]);
    one.push(22);
    one.extend_from_slice(&64u32.to_le_bytes());
    one.extend_from_slice(&[0u8; 64]);
    assert!(chunk(&one).unwrap_err().contains(&format!("song device {DEV}")));
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
    let p = std::env::temp_dir().join("glorp-id22-bad-events.csv");
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,7,22,boom,1,2,3\n").unwrap();
    let err = std::panic::catch_unwind(|| events_of(&p)).err().unwrap();
    assert!(err.downcast_ref::<String>().cloned().unwrap_or_default().contains("unknown event kind"));
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,7,22,on,1,2\n").unwrap();
    assert!(std::panic::catch_unwind(|| events_of(&p)).is_err());
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,7,22,on,60,127,12\n3,9,22,on,1,1,1\n1,7,22,off,60,7,0\n2,7,22,param,3,0.5,1056964608\n").unwrap();
    let m = events_of(&p);
    assert_eq!(m[&0], vec![Ev::On(60, 12)]);
    assert_eq!(m[&1], vec![Ev::Off(60, 7)]);
    assert_eq!(m[&2], vec![Ev::Param(3, 0.5)]);
    assert!(!m.contains_key(&3), "other devices' rows are ignored");
}

#[test]
fn a_set_but_missing_root_is_a_hard_error() {
    // GLORP_REF_ROOT present but pointing nowhere must fail loudly, not skip: `fixtures()` reads the song blob first
    let p = std::env::temp_dir().join("glorp-id22-no-such-root").join("389811-2.bin");
    let err = std::panic::catch_unwind(|| require(&p)).err().expect("a missing song blob must panic");
    assert!(err.downcast_ref::<String>().cloned().unwrap_or_default().contains("GLORP_REF_ROOT is set"));
}
