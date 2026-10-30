//! Optional external-reference parity: ID 28 (unison oscillator bank through a 4-pole ladder + the shared 16-voice
//! scheduler) vs the original engine's output, replayed through the isolated Wine harness (local only: third-party
//! binary + song never enter git).
//!
//! `GLORP_REF_ROOT` = the `ws-intros/out` directory that holds `389811-2.bin` (read only for the four ID 28 initial
//! parameter sets) and `glorp-win64/analysis/harness/out/id28impl/` with
//!   * `ev.csv` + `taps/dev11..14.tap`: the **whole song** run (78,292 blocks; song devices 11, 12, 13, 14 are the four
//!     ID 28 instances) with every note-on (velocity, in-block offset) / note-off and every `SetParam` the engine issued
//!     (`--events-all`), and each device's recorded per-block buffers;
//!   * `vec/manifest.json`, `vec/NAME.{csv,tap}`: synthetic vectors (`vec/gen.py`: the four song settings, all-0 / all-1 /
//!     all-0.5, the nine spread modes and their bucket edges, voice counts around the 1/31 steps, spread 0 / 0.25 / 1,
//!     polyphony beyond 16, retrigger, same-offset order, offsets beyond a block, velocities 0..255, queue overflow,
//!     voice reuse, mid-note changes of every parameter, per-block lane ramps, voices expiring inside a block, filter
//!     extremes, every parameter at 0 and at 1) that the harness fed to a fresh instance of song device 11 and recorded
//!     (`GLORP_OUT_TAP`).
//!
//! Tap block = 8 × 128 f32: in L, in R, aux L, aux R, then out L, out R, aux L, aux R. The synth ignores the inputs,
//! zeroes and fills out L / out R and leaves the aux pair alone (asserted). `GLORP_REF_ROOT` unset => the tests skip
//! explicitly (they do not prove parity); set but any fixture file missing => they fail loudly, naming the file.
use glorp::id28::{Error, Id28, PARAMS};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

const B: usize = 128;
const SONG_DEVICES: [usize; 4] = [11, 12, 13, 14];
const VECTOR_DEV: usize = 11;
const SONG_BLOCKS: usize = 78_292;

/// Tight per-sample bound `ABS + REL·|ref|` (a few ulp), used by the negative controls on vectors that meet it.
const ABS: f64 = 1e-7;
const REL: f64 = 3e-7;
/// Parity bound of a fixture (`Tol::Parity`): the largest absolute error may not exceed `PARITY_ABS + PARITY_REL ·
/// max|ref|`. Set from the measurement in `research/glorp-devices.md` §ID 28 (the 125 synthetic vectors measure at most
/// 1.7e-7 absolute, 3.3e-6 of the peak on the one vector whose peak is 1.3e-3).
const PARITY_ABS: f64 = 4e-7;
const PARITY_REL: f64 = 2e-6;

#[derive(Clone, Copy, PartialEq)]
enum Tol {
    Parity,
    Tight,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Ev {
    On(u8, u8, u32),
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

/// Initial parameters of the ID 28 song devices (the 27th chunk float is the byte size): exactly the four song devices.
fn chunks(blob: &[u8]) -> Result<Vec<(usize, [f32; PARAMS])>, String> {
    let u32at = |p: usize| u32::from_le_bytes(blob[p..p + 4].try_into().unwrap()) as usize;
    let n = u32at(16);
    let mut p = 20;
    let mut found = Vec::new();
    for i in 0..n {
        let (id, size) = (blob[p], u32at(p + 1));
        let c = &blob[p + 5..p + 5 + size];
        p += 5 + size;
        if id == 28 {
            if size != 4 * PARAMS + 4 {
                return Err(format!("ID 28 chunk is {size} bytes"));
            }
            found.push((i, f32s(c)[..PARAMS].try_into().unwrap()));
        }
    }
    let devs: Vec<usize> = found.iter().map(|f| f.0).collect();
    if devs != SONG_DEVICES {
        return Err(format!("expected exactly the ID 28 devices {SONG_DEVICES:?}, found {devs:?}"));
    }
    Ok(found)
}

/// Event log rows of device `dev`: `block,dev,id,kind,a,b,c` (`on`: note,vel,offset; `off`: note,offset,0;
/// `param`: idx,value,bits). A malformed row is a hard error.
fn events_of(path: &Path, dev: usize) -> Events {
    let mut m: Events = HashMap::new();
    for (n, line) in BufReader::new(std::fs::File::open(path).unwrap()).lines().enumerate().skip(1) {
        let line = line.unwrap();
        let f: Vec<&str> = line.split(',').collect();
        assert_eq!(f.len(), 7, "{}:{}: bad row {line:?}", path.display(), n + 1);
        if f[1].parse::<usize>().unwrap() != dev {
            continue;
        }
        let blk: usize = f[0].parse().unwrap();
        let ev = match f[3] {
            "on" => Ev::On(f[4].parse().unwrap(), f[5].parse::<f64>().unwrap() as u8, f[6].parse().unwrap()),
            "off" => Ev::Off(f[4].parse().unwrap(), f[5].parse::<f64>().unwrap() as u32),
            "param" => Ev::Param(f[4].parse().unwrap(), f32::from_bits(f[6].parse().unwrap())),
            k => panic!("{}:{}: unknown event kind {k}", path.display(), n + 1),
        };
        m.entry(blk).or_default().push(ev);
    }
    m
}

/// Replays `nblocks` blocks of the tap through a fresh `Id28` built from `init`, applying each block's events in log
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
    let mut d = Id28::new(&init).unwrap_or_else(|e| panic!("{what}: {e:?}"));
    let mut s = Stats::default();
    let mut raw = vec![0u8; 8 * B * 4];
    for b in 0..nblocks {
        for ev in events.get(&b).map(|v| v.as_slice()).unwrap_or(&[]) {
            let r = match *ev {
                Ev::On(n, v, o) => d.note_on(n, v, o),
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
            s.max_abs <= PARITY_ABS + PARITY_REL * s.max_ref,
            "{what}: max abs err {:e} exceeds {PARITY_ABS:e} + {PARITY_REL:e} of the peak {} (parity got/want mismatch)",
            s.max_abs,
            s.max_ref
        );
    }
    s
}

struct Fixtures {
    out: PathBuf,
    init: Vec<(usize, [f32; PARAMS])>,
}

fn fixtures() -> Option<Fixtures> {
    let root = PathBuf::from(std::env::var_os("GLORP_REF_ROOT")?);
    let blob = std::fs::read(require(&root.join("389811-2.bin"))).unwrap();
    let init = chunks(&blob).unwrap_or_else(|e| panic!("389811-2.bin: {e}"));
    Some(Fixtures { out: root.join("glorp-win64/analysis/harness/out/id28impl"), init })
}

fn init_of(fx: &Fixtures, dev: usize) -> [f32; PARAMS] {
    fx.init.iter().find(|f| f.0 == dev).unwrap().1
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
    assert_eq!(v.len(), 125, "manifest.json: expected 125 vectors, got {}", v.len());
    v
}

fn blocks_of(fx: &Fixtures, name: &str) -> usize {
    manifest(fx).into_iter().find(|(k, _)| k == name).unwrap_or_else(|| panic!("{name} is not in the manifest")).1
}

fn vector(fx: &Fixtures, name: &str, n: usize) -> (Events, Vec<u8>) {
    let ev = events_of(&require(&fx.out.join(format!("vec/{name}.csv"))), VECTOR_DEV);
    let tap = std::fs::read(require(&fx.out.join(format!("vec/{name}.tap")))).unwrap();
    assert_eq!(tap.len(), n * 8 * B * 4, "{name}: tap size does not match the manifest");
    (ev, tap)
}

fn run_vectors(fx: &Fixtures) -> Vec<(String, Stats)> {
    manifest(fx)
        .into_iter()
        .map(|(name, n)| {
            let (ev, tap) = vector(fx, &name, n);
            let s = replay(&name, init_of(fx, VECTOR_DEV), n, &mut &tap[..], &ev, name == "vec15", Tol::Parity);
            (name, s)
        })
        .collect()
}

fn song(fx: &Fixtures, dev: usize, tweak: &dyn Fn(&mut Events)) -> Stats {
    let mut ev = events_of(&require(&fx.out.join("ev.csv")), dev);
    tweak(&mut ev);
    let path = require(&fx.out.join(format!("taps/dev{dev}.tap")));
    let len = std::fs::metadata(&path).unwrap().len() as usize;
    assert_eq!(len, SONG_BLOCKS * 8 * B * 4, "{}: not the whole-song tap (78,292 blocks)", path.display());
    let mut f = BufReader::with_capacity(1 << 20, std::fs::File::open(path).unwrap());
    replay(&format!("song device {dev}"), init_of(fx, dev), SONG_BLOCKS, &mut f, &ev, false, Tol::Parity)
}

fn report(what: &str, s: &Stats) {
    eprintln!(
        "{what}: {} samples, {} exact ({:.4} %), {} nonzero refs, max |ref| {:.4}, max abs err {:.3e} ({:.2e} of peak), max voices {}, first block off the tight bound {:?}",
        s.samples,
        s.exact,
        100.0 * s.exact as f64 / s.samples as f64,
        s.nonzero,
        s.max_ref,
        s.max_abs,
        s.max_abs / s.max_ref.max(1e-30),
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
    for dev in SONG_DEVICES {
        let s = song(&fx, dev, &|_| {});
        report(&format!("song device {dev} (whole song)"), &s);
        assert_eq!(s.samples, SONG_BLOCKS * B * 2);
        assert!(s.nonzero > 100_000, "device {dev}: the whole-song tap is too quiet to mean anything: {} nonzero", s.nonzero);
    }
}

/// Vectors whose reference is legitimately silent: all parameters 0, and level P13 = 0.
const SILENT: [&str; 2] = ["p13lo", "vec05"];
/// Vectors used by the negative controls: the per-sample tight bound holds on them (asserted by
/// `control_vectors_meet_the_tight_bound`): the song settings of device 13 (sine-fold, `vec03`) and 14 (a chord, `vec04`),
/// the song's voice-count / spread / detune with nine unison voices (`m03`), mid-note parameter changes (`vec17` is not
/// tight, `vec11` is), velocities and a long sustained chord with a different filter (`f02`, `p10hi`).
const CONTROL_VECTORS: [&str; 6] = ["vec03", "vec04", "m03", "vec11", "p09lo", "p10hi"];

#[test]
fn synthetic_vectors_match_the_original() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let mut quiet = Vec::new();
    for (name, s) in run_vectors(&fx) {
        report(&name, &s);
        if s.nonzero == 0 {
            quiet.push(name.clone());
        }
        match name.as_str() {
            "vec15" => {
                assert_eq!(s.refused, 300 - 256, "vec15: 300 note-ons in one block, 256 slots");
                assert_eq!(s.max_voices, 16);
            }
            "vec10" => assert_eq!(s.max_voices, 16, "vec10: 17 simultaneous notes"),
            "vec16" => assert_eq!(s.max_voices, 16, "vec16: released notes keep their slot"),
            _ => {}
        }
    }
    for q in &quiet {
        assert!(SILENT.contains(&q.as_str()), "{q}: unexpectedly silent reference");
    }
    assert_eq!(quiet.len(), SILENT.len(), "silent vectors: {quiet:?}");
}

/// A voice that expires inside a block still renders the rest of that sub-block (the filter rings out), and nothing
/// after it: block 2 holds the note-off (offset 20, release 0), the reference is non-zero after the expiry in that
/// block and exactly zero in the next one.
#[test]
fn a_voice_expiring_inside_a_block_renders_the_rest_of_the_sub_block() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let (_, tap) = vector(&fx, "x00", blocks_of(&fx, "x00"));
    let out = f32s(&tap);
    let blk = |b: usize, c: usize| &out[(b * 8 + 4 + c) * B..(b * 8 + 5 + c) * B];
    assert!(blk(2, 0)[21..].iter().any(|&x| x != 0.0), "no ring-out after the expiry in the reference");
    assert!(blk(3, 0).iter().chain(blk(3, 1)).all(|&x| x == 0.0), "the reference renders after the sub-block");
}

// ------------------------------------------------------------------ negative controls

/// The mutated replay must fail with the `got … want …` mismatch (not with some other panic).
fn must_mismatch<R>(what: &str, f: impl FnOnce() -> R + std::panic::UnwindSafe) {
    let err = std::panic::catch_unwind(f).err().unwrap_or_else(|| panic!("the mutation '{what}' was not detected"));
    let msg = err.downcast_ref::<String>().cloned().or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
    assert!(msg.contains("got") && msg.contains("want"), "'{what}' failed for another reason: {msg}");
}

fn tweak_block0(ev: &Events, f: impl Fn(&mut Ev)) -> Events {
    let mut ev = ev.clone();
    for e in ev.get_mut(&0).unwrap() {
        f(e);
    }
    ev
}

/// The nudge of parameter `i`: `d` up (down at the top), the threshold parameters across their switch (P0, P4 and P8 at
/// 0.5, P6 across 0.001, P17 by a whole bucket).
fn nudged(i: usize, v: f32, d: f32) -> f32 {
    match i {
        // P0 selects the waveform at 0.5 (`trunc(2·P0)`), P4 / P8 are flags above 0.5
        0 => if v >= 0.5 { 0.4999 } else { 0.5 },
        4 | 8 => if v > 0.5 { 0.5 } else { 0.5001 },
        // P17 selects among nine buckets of width 0.111: a full bucket
        17 => if v + 0.12 <= 1.0 { v + 0.12 } else { v - 0.12 },
        6 => if v > 0.001 { 0.0 } else { 0.01 },
        _ => if v + d <= 1.0 { v + d } else { v - d },
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
        let s = replay(name, init_of(&fx, VECTOR_DEV), n, &mut &tap[..], &ev, false, Tol::Tight);
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
    let (ev, mut tap) = vector(&fx, "vec03", blocks_of(&fx, "vec03"));
    let blk = (0..200).find(|b| tap[(b * 8 + 4) * B * 4..(b * 8 + 5) * B * 4].iter().any(|&x| x != 0)).unwrap();
    tap[((blk * 8 + 4) * B + 5) * 4 + 3] ^= 0x40;
    must_mismatch("flipped bit", || {
        replay("flip", init_of(&fx, VECTOR_DEV), blk + 2, &mut &tap[..], &ev, false, Tol::Tight);
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
    // every parameter nudged by 1/255 (thresholds across their switch) must leave the tight bound on at least one control vector
    for i in 0..PARAMS {
        let caught = fixtures.iter().any(|(name, n, ev0, tap)| {
            let ev = tweak_block0(ev0, |e| {
                if let Ev::Param(k, v) = e {
                    if *k == i {
                        *v = nudged(i, *v, 1.0 / 255.0);
                    }
                }
            });
            std::panic::catch_unwind(|| replay(name, init_of(&fx, VECTOR_DEV), *n, &mut &tap[..], &ev, false, Tol::Tight).max_abs).is_err()
        });
        assert!(caught, "a nudge of P{i} was not detected on any control vector");
    }
}

#[test]
fn negative_control_scheduler_and_event_mutations_are_caught() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let init = init_of(&fx, VECTOR_DEV);
    let map = |ev: &Events, f: &dyn Fn(usize, Ev) -> Option<Ev>| -> Events {
        ev.iter().map(|(b, v)| (*b, v.iter().filter_map(|e| f(*b, *e)).collect())).collect()
    };
    // offsets dropped (vec10 spreads them)
    let (ev, tap) = vector(&fx, "vec10", blocks_of(&fx, "vec10"));
    let m = map(&ev, &|_, e| Some(match e { Ev::On(n, v, _) => Ev::On(n, v, 0), Ev::Off(n, _) => Ev::Off(n, 0), p => p }));
    must_mismatch("offsets dropped", || {
        replay("no-offsets", init, 300, &mut &tap[..], &m, false, Tol::Parity);
    });
    // velocity ignored (vec14 plays 0 / 1 / 64 / 127 / 128 / 255)
    let (ev, tap) = vector(&fx, "vec14", blocks_of(&fx, "vec14"));
    let m = map(&ev, &|_, e| Some(match e { Ev::On(n, _, o) => Ev::On(n, 127, o), p => p }));
    must_mismatch("velocity ignored", || {
        replay("no-velocity", init, 260, &mut &tap[..], &m, false, Tol::Parity);
    });
    // note-offs dropped (vec16: voices never free, the 16-voice limit eats later notes)
    let (ev, tap) = vector(&fx, "vec16", blocks_of(&fx, "vec16"));
    let m = map(&ev, &|_, e| (!matches!(e, Ev::Off(..))).then_some(e));
    must_mismatch("note-offs dropped", || {
        replay("no-offs", init, 520, &mut &tap[..], &m, false, Tol::Parity);
    });
    // same-offset order reversed (vec12 depends on queue order)
    let (ev, tap) = vector(&fx, "vec12", blocks_of(&fx, "vec12"));
    let rev: Events = ev.iter().map(|(b, v)| (*b, if *b == 0 { v.iter().rev().copied().collect() } else { v.clone() })).collect();
    must_mismatch("same-offset order reversed", || {
        replay("reversed", init, 200, &mut &tap[..], &rev, false, Tol::Parity);
    });
    // lane writes dropped (vec18: detune, spread mode, cutoff, envelope amount ramp every block)
    let (ev, tap) = vector(&fx, "vec18", blocks_of(&fx, "vec18"));
    let flat = map(&ev, &|b, e| (b == 0 || !matches!(e, Ev::Param(..))).then_some(e));
    must_mismatch("lane ramps dropped", || {
        replay("flat", init, 420, &mut &tap[..], &flat, false, Tol::Parity);
    });
    // mid-note parameter writes dropped (vec17)
    let (ev, tap) = vector(&fx, "vec17", blocks_of(&fx, "vec17"));
    let flat = map(&ev, &|b, e| (b == 0 || !matches!(e, Ev::Param(..))).then_some(e));
    must_mismatch("mid-note writes dropped", || {
        replay("flat", init, 260, &mut &tap[..], &flat, false, Tol::Parity);
    });
    // 16th simultaneous note removed (vec10: caught); the 17th is dropped by the original (removing it changes nothing)
    let (ev, tap) = vector(&fx, "vec10", blocks_of(&fx, "vec10"));
    let nth = |k: usize| {
        let seen = std::cell::Cell::new(0);
        map(&ev, &|b, e| {
            if b == 0 && matches!(e, Ev::On(..)) {
                seen.set(seen.get() + 1);
                (seen.get() != k).then_some(e)
            } else {
                Some(e)
            }
        })
    };
    let (no16, no17) = (nth(16), nth(17));
    must_mismatch("16th note removed", || {
        replay("no-16th", init, 300, &mut &tap[..], &no16, false, Tol::Parity);
    });
    replay("no-17th", init, 300, &mut &tap[..], &no17, false, Tol::Parity);
}

#[test]
fn negative_control_dropped_song_automation_is_caught() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let mut seen = Vec::new();
    for dev in SONG_DEVICES {
        let ev = events_of(&require(&fx.out.join("ev.csv")), dev);
        let mut automated: Vec<usize> = ev.iter().filter(|(b, _)| **b > 0).flat_map(|(_, v)| v.iter()).filter_map(|e| if let Ev::Param(i, _) = e { Some(*i) } else { None }).collect();
        automated.sort();
        automated.dedup();
        eprintln!("song device {dev} automated parameters: {automated:?}");
        assert!(!automated.is_empty(), "device {dev}: the song automates every ID 28 instance");
        seen.push((dev, automated));
    }
    // device 13: the shortest automated instance (blocks 34,215 .. 55,095), stripped of its lanes
    must_mismatch("song automation dropped", || {
        song(&fx, 13, &|ev| {
            for (b, v) in ev.iter_mut() {
                if *b > 0 {
                    v.retain(|e| !matches!(e, Ev::Param(..)));
                }
            }
        });
    });
}

// ------------------------------------------------------------------ always-on hardening of the harness itself

#[test]
fn missing_fixtures_are_a_hard_error() {
    let p = std::env::temp_dir().join("glorp-id28-definitely-missing.csv");
    let err = std::panic::catch_unwind(|| require(&p)).err().expect("require must panic");
    let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("fixture") && msg.contains("glorp-id28-definitely-missing.csv"), "{msg}");
}

#[test]
fn a_set_but_missing_root_is_a_hard_error() {
    let p = std::env::temp_dir().join("glorp-id28-no-such-root").join("389811-2.bin");
    let err = std::panic::catch_unwind(|| require(&p)).err().expect("a missing song blob must panic");
    assert!(err.downcast_ref::<String>().cloned().unwrap_or_default().contains("GLORP_REF_ROOT is set"));
}

#[test]
fn chunk_parser_refuses_a_song_without_exactly_the_four_id28_devices() {
    let song = |ids: &[u8]| {
        let mut blob = vec![0u8; 20];
        blob[16..20].copy_from_slice(&(ids.len() as u32).to_le_bytes());
        for &id in ids {
            let n = if id == 28 { 4 * PARAMS + 4 } else { 32 };
            blob.push(id);
            blob.extend_from_slice(&(n as u32).to_le_bytes());
            blob.extend_from_slice(&vec![0u8; n]);
        }
        blob
    };
    assert!(chunks(&song(&[7, 38])).unwrap_err().contains("found []"));
    assert!(chunks(&song(&[28])).unwrap_err().contains("found [0]"));
    // four ID 28 devices, but not as song devices 11..14
    assert!(chunks(&song(&[28, 28, 28, 28])).unwrap_err().contains("found [0, 1, 2, 3]"));
    let mut ids = vec![7u8; 11];
    ids.extend([28, 28, 28, 28]);
    assert_eq!(chunks(&song(&ids)).unwrap().iter().map(|c| c.0).collect::<Vec<_>>(), SONG_DEVICES);
    ids.push(28);
    assert!(chunks(&song(&ids)).unwrap_err().contains("found [11, 12, 13, 14, 15]"));
    // a chunk of another size is refused by name
    let mut blob = vec![0u8; 20];
    blob[16..20].copy_from_slice(&1u32.to_le_bytes());
    blob.push(28);
    blob.extend_from_slice(&(4 * PARAMS as u32 + 8).to_le_bytes());
    blob.extend_from_slice(&vec![0u8; 4 * PARAMS + 8]);
    assert!(chunks(&blob).unwrap_err().contains("chunk is"));
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
    let p = std::env::temp_dir().join("glorp-id28-bad-events.csv");
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,11,28,boom,1,2,3\n").unwrap();
    let err = std::panic::catch_unwind(|| events_of(&p, 11)).err().unwrap();
    assert!(err.downcast_ref::<String>().cloned().unwrap_or_default().contains("unknown event kind"));
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,11,28,on,1,2\n").unwrap();
    assert!(std::panic::catch_unwind(|| events_of(&p, 11)).is_err());
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,11,28,on,60,127,12\n3,9,28,on,1,1,1\n1,11,28,off,60,7,0\n2,11,28,param,3,0.5,1056964608\n").unwrap();
    let m = events_of(&p, 11);
    assert_eq!(m[&0], vec![Ev::On(60, 127, 12)]);
    assert_eq!(m[&1], vec![Ev::Off(60, 7)]);
    assert_eq!(m[&2], vec![Ev::Param(3, 0.5)]);
    assert!(!m.contains_key(&3), "other devices' rows are ignored");
}
