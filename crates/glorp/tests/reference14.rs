//! Optional external-reference parity: ID 14 (noise-excited modal resonator bank + the shared 16-voice scheduler in its
//! mono voice mode) vs the original engine's output, replayed through the isolated Wine harness (local only: third-party
//! binary + song never enter git).
//!
//! `GLORP_REF_ROOT` = the `ws-intros/out` directory that holds `389811-2.bin` (read only for the two ID 14 initial
//! parameter sets) and `glorp-win64/analysis/harness/out/id14impl/` with
//!   * `ev.csv` + `taps/dev02.tap`, `taps/dev03.tap`: the **whole song** run (78,292 blocks; song devices 2 and 3 are the
//!     two ID 14 instances, device 2 never plays a note) with every note-on / note-off and every `SetParam` the engine
//!     issued (`--events-all`), and each device's recorded per-block buffers;
//!   * `vec/manifest.json`, `vec/NAME.{csv,tap}`: synthetic vectors (`vec/gen.py`) that the harness fed to a fresh
//!     instance of song device 3 and recorded (`GLORP_OUT_TAP`).
//!
//! Tap block = 8 × 128 f32: in L, in R, aux L, aux R, then out L, out R, aux L, aux R. The synth ignores the inputs,
//! zeroes and fills out L / out R and leaves the aux pair alone (asserted). `GLORP_REF_ROOT` unset => the tests skip
//! explicitly (they do not prove parity); set but any fixture file missing => they fail loudly, naming the file.
use glorp::id14::{Error, Id14, PARAMS};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

const B: usize = 128;
const SONG_DEVICES: [usize; 2] = [2, 3];
const VECTOR_DEV: usize = 3;
const SONG_BLOCKS: usize = 78_292;

/// Tight per-sample bound `ABS + REL·|ref|` (a few ulp), used by the negative controls on vectors that meet it.
const ABS: f64 = 1e-7;
const REL: f64 = 3e-7;
/// Parity bound of a fixture (`Tol::Parity`): the largest absolute error may not exceed `PARITY_ABS + PARITY_REL ·
/// max|ref|`. The exponentials are the ordinary correctly rounded `libm` results (`expf` of the noise envelope), the
/// original's C library rounds a small share of them one ulp differently (documented for ID 25 in the research doc); the
/// 1000-Q resonator bank rings that last bit up to ~3e-5 of the peak (measured 2.9e-5 over the whole song).
const PARITY_ABS: f64 = 4e-7;
const PARITY_REL: f64 = 1e-4;

/// The vectors reach settings the song does not (the longest-ringing Q = 1000 banks, dense random harmonic sets): their
/// worst measured error is 3.3e-4 of the peak (`r08`), the bound leaves a factor 2.
const VEC_ABS: f64 = 1e-6;
const VEC_REL: f64 = 7e-4;

#[derive(Clone, Copy, PartialEq)]
enum Tol {
    Parity,
    Tight,
    /// The synthetic vectors: the same measure with the looser `VEC_ABS` / `VEC_REL`.
    Vector,
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

/// Initial parameters of the ID 14 song devices (the 27th chunk float is the byte size): exactly the two song devices.
fn chunks(blob: &[u8]) -> Result<Vec<(usize, [f32; PARAMS])>, String> {
    let u32at = |p: usize| u32::from_le_bytes(blob[p..p + 4].try_into().unwrap()) as usize;
    let n = u32at(16);
    let mut p = 20;
    let mut found = Vec::new();
    for i in 0..n {
        let (id, size) = (blob[p], u32at(p + 1));
        let c = &blob[p + 5..p + 5 + size];
        p += 5 + size;
        if id == 14 {
            if size != 4 * PARAMS + 4 {
                return Err(format!("ID 14 chunk is {size} bytes"));
            }
            found.push((i, f32s(c)[..PARAMS].try_into().unwrap()));
        }
    }
    let devs: Vec<usize> = found.iter().map(|f| f.0).collect();
    if devs != SONG_DEVICES {
        return Err(format!("expected exactly the ID 14 devices {SONG_DEVICES:?}, found {devs:?}"));
    }
    Ok(found)
}

/// Event log rows of device `dev`: `block,dev,id,kind,a,b,c` (`on`: note,vel,offset; `off`: note,offset,0;
/// `param`: idx,value,bits). A malformed row is a hard error. The velocity of a note-on is read and ignored (ID 14 has
/// none).
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
            "on" => Ev::On(f[4].parse().unwrap(), f[6].parse().unwrap()),
            "off" => Ev::Off(f[4].parse().unwrap(), f[5].parse::<f64>().unwrap() as u32),
            "param" => Ev::Param(f[4].parse().unwrap(), f32::from_bits(f[6].parse().unwrap())),
            k => panic!("{}:{}: unknown event kind {k}", path.display(), n + 1),
        };
        m.entry(blk).or_default().push(ev);
    }
    m
}

/// Replays `nblocks` blocks of the tap through a fresh `Id14` built from `init`, applying each block's events in log
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
    let mut d = Box::new(Id14::new(&init).unwrap_or_else(|e| panic!("{what}: {e:?}")));
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
                    tol != Tol::Tight || e <= ABS + REL * want.abs() as f64,
                    "{what} block {b} ch {c} i {i}: got {} want {want} (err {e:e})",
                    got[i]
                );
            }
        }
    }
    if tol != Tol::Tight {
        let (abs, rel) = if tol == Tol::Parity { (PARITY_ABS, PARITY_REL) } else { (VEC_ABS, VEC_REL) };
        assert!(
            s.max_abs <= abs + rel * s.max_ref,
            "{what}: max abs err {:e} exceeds {abs:e} + {rel:e} of the peak {} (parity got/want mismatch)",
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
    Some(Fixtures { out: root.join("glorp-win64/analysis/harness/out/id14impl"), init })
}

fn init_of(fx: &Fixtures, dev: usize) -> [f32; PARAMS] {
    fx.init.iter().find(|f| f.0 == dev).unwrap().1
}

fn song(fx: &Fixtures, dev: usize, tweak: &dyn Fn(&mut Events)) -> Stats {
    let mut ev = events_of(&require(&fx.out.join("ev.csv")), dev);
    tweak(&mut ev);
    let path = require(&fx.out.join(format!("taps/dev{dev:02}.tap")));
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
    assert_eq!(v.len(), 97, "manifest.json: expected 97 vectors, got {}", v.len());
    v
}

fn vector(fx: &Fixtures, name: &str, n: usize) -> (Events, Vec<u8>) {
    let ev = events_of(&require(&fx.out.join(format!("vec/{name}.csv"))), VECTOR_DEV);
    let tap = std::fs::read(require(&fx.out.join(format!("vec/{name}.tap")))).unwrap();
    assert_eq!(tap.len(), n * 8 * B * 4, "{name}: tap size does not match the manifest");
    (ev, tap)
}

fn run_vectors(fx: &Fixtures, tol: Tol) -> Vec<(String, Stats)> {
    manifest(fx)
        .into_iter()
        .map(|(name, n)| {
            let (ev, tap) = vector(fx, &name, n);
            let s = replay(&name, init_of(fx, VECTOR_DEV), n, &mut &tap[..], &ev, name == "vec12", tol);
            (name, s)
        })
        .collect()
}

#[test]
fn synthetic_vectors_match_the_original() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let mut quiet = Vec::new();
    for (name, s) in run_vectors(&fx, Tol::Vector) {
        report(&name, &s);
        if s.nonzero == 0 {
            quiet.push(name.clone());
        }
        match name.as_str() {
            "vec12" => assert_eq!(s.refused, 300 - 256, "vec12: 300 note-ons in one block, 256 slots"),
            "vec08" => assert_eq!(s.max_voices, 16, "vec08: 17 simultaneous notes"),
            _ => {}
        }
    }
    assert_eq!(quiet, ["p22lo", "vec05"], "silent references: output gain 0 and all parameters 0");
}

/// Vectors replayed with a mutation must leave the vector bound `VEC_ABS + VEC_REL · max|ref|`.
fn mutated_vector(fx: &Fixtures, name: &str, init: Option<(usize, f32)>, f: &dyn Fn(Ev) -> Option<Ev>) -> Stats {
    let n = manifest(fx).into_iter().find(|(k, _)| k == name).unwrap().1;
    let (ev, tap) = vector(fx, name, n);
    let ev: Events = ev.into_iter().map(|(b, v)| (b, v.into_iter().filter_map(|e| f(e)).collect())).collect();
    let mut p = init_of(fx, VECTOR_DEV);
    if let Some((i, v)) = init {
        p[i] = v;
    }
    replay(name, p, n, &mut &tap[..], &ev, false, Tol::Vector)
}

#[test]
fn negative_control_vector_mutations_are_caught() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    // the unmutated vectors pass the same bound
    for name in ["mono01", "mono02", "mono03", "mono08", "vec08", "vec10"] {
        mutated_vector(&fx, name, None, &|e| Some(e));
    }
    // the mono rules: with block 0 writing wire 23 = 1.0, mutating that write to poly changes the legato scenarios
    for name in ["mono01", "mono02", "mono03", "mono08"] {
        must_mismatch(name, || {
            mutated_vector(&fx, name, None, &|e| Some(match e { Ev::Param(23, _) => Ev::Param(23, 0.0), e => e }));
        });
    }
    // polyphony: the 16th note and the offsets of vec08, the order of vec10
    must_mismatch("offsets dropped", || {
        mutated_vector(&fx, "vec10", None, &|e| Some(match e { Ev::On(n, _) => Ev::On(n, 0), Ev::Off(n, _) => Ev::Off(n, 0), e => e }));
    });
    must_mismatch("note-offs dropped", || {
        mutated_vector(&fx, "vec13", None, &|e| (!matches!(e, Ev::Off(..))).then_some(e));
    });
    // every wire of the chord vectors is observable except the inert ones: each lo / hi vector replayed with the wire forced
    // to the other extreme (P0, the hold flag, only shows at the sustain stage that these 100-block notes never reach: the
    // whole-song nudge control pins it)
    let mut missed = Vec::new();
    for i in 0..PARAMS {
        let detected = [("lo", 1.0f32), ("hi", 0.0)].iter().any(|&(tag, other)| {
            let name = format!("p{i:02}{tag}");
            std::panic::catch_unwind(|| mutated_vector(&fx, &name, None, &|e| Some(match e { Ev::Param(k, _) if k == i => Ev::Param(k, other), e => e }))).is_err()
        });
        if !detected {
            missed.push(i);
        }
    }
    eprintln!("wires the lo / hi vectors do not tell apart: {missed:?}");
    assert_eq!(missed, vec![0, 5, 6, 12, 24], "the inert wires and the hold flag");
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
        if dev == 3 {
            assert!(s.nonzero > 100_000, "device {dev}: the whole-song tap is too quiet to mean anything: {} nonzero", s.nonzero);
        } else {
            assert_eq!(s.nonzero, 0, "song device 2 never plays: its recorded output is silent");
        }
    }
}

// ------------------------------------------------------------------ negative controls

/// The mutated replay must fail with the `got … want …` mismatch (not with some other panic).
fn must_mismatch<R>(what: &str, f: impl FnOnce() -> R + std::panic::UnwindSafe) {
    let err = std::panic::catch_unwind(f).err().unwrap_or_else(|| panic!("the mutation '{what}' was not detected"));
    let msg = err.downcast_ref::<String>().cloned().or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
    assert!(msg.contains("got") && msg.contains("want"), "'{what}' failed for another reason: {msg}");
}

/// A prefix of the whole-song replay of device 3 (`blocks` blocks), with the events mapped by `f`.
fn prefix(fx: &Fixtures, blocks: usize, init: [f32; PARAMS], f: &dyn Fn(usize, Ev) -> Option<Ev>) -> Stats {
    let ev: Events = events_of(&require(&fx.out.join("ev.csv")), 3)
        .into_iter()
        .map(|(b, v)| (b, v.into_iter().filter_map(|e| f(b, e)).collect()))
        .collect();
    let mut tap = BufReader::new(std::fs::File::open(require(&fx.out.join("taps/dev03.tap"))).unwrap());
    replay("prefix", init, blocks, &mut tap, &ev, false, Tol::Parity)
}

/// The unmutated prefixes the controls compare against must pass (otherwise a "detected" mutation proves nothing).
const PREFIX: usize = 9000;

/// A parameter nudge: flags across their threshold, the rest by 0.03 (down at the top).
fn nudged(i: usize, v: f32) -> f32 {
    match i {
        // the hold flag (P0 < 0.5 releases at the sustain stage), the octave bucket, the tone filter kind and the
        // dropout need a whole step; P11 is a flag above 0.5
        0 => if v < 0.5 { 1.0 } else { 0.0 },
        7 => if v < 0.5 { 0.9 } else { 0.1 },
        11 => if v > 0.5 { 0.5 } else { 1.0 },
        15 => if v < 0.5 { 0.9 } else { 0.1 },
        25 => if v < 0.5 { 1.0 } else { 0.0 },
        _ => if v + 0.03 <= 1.0 { v + 0.03 } else { v - 0.03 },
    }
}

#[test]
fn negative_control_unmutated_prefix_passes_and_flipped_reference_bit_is_caught() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let init = init_of(&fx, 3);
    let s = prefix(&fx, PREFIX, init, &|_, e| Some(e));
    assert!(s.nonzero > 100_000, "the control prefix is too quiet: {} nonzero", s.nonzero);
    let ev = events_of(&require(&fx.out.join("ev.csv")), 3);
    let mut tap = std::fs::read(require(&fx.out.join("taps/dev03.tap"))).unwrap();
    tap.truncate(2000 * 8 * B * 4);
    // a mantissa bit well above the f32 noise: flip bit 12 of an audible sample's left output
    let blk = (0..2000).find(|b| tap[(b * 8 + 4) * B * 4..(b * 8 + 5) * B * 4].chunks(4).any(|c| f32::from_le_bytes(c.try_into().unwrap()).abs() > 1e-2)).unwrap();
    tap[((blk * 8 + 4) * B + 5) * 4 + 1] ^= 0x10;
    must_mismatch("flipped bit", || {
        replay("flip", init, 2000, &mut &tap[..], &ev, false, Tol::Tight);
    });
}

#[test]
fn negative_control_parameter_nudges_are_caught() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let init = init_of(&fx, 3);
    // every wire nudged in the initial vector and in every later write of the song: only the wires the device ignores
    // (P5 / P6 / P12 / P24: nothing audible in the song settings, see the synthetic suite) may go unnoticed
    let mut missed = Vec::new();
    for i in (0..PARAMS).filter(|&i| i != 23) {
        let mut p = init;
        p[i] = nudged(i, p[i]);
        let r = std::panic::catch_unwind(|| prefix(&fx, PREFIX, p, &|_, e| Some(match e { Ev::Param(k, v) if k == i => Ev::Param(k, nudged(i, v)), e => e })));
        if r.is_ok() {
            missed.push(i);
        }
    }
    eprintln!("nudges not detected: {missed:?}");
    assert_eq!(missed, vec![5, 6, 12, 24], "exactly the inert wires go unnoticed");
}

#[test]
fn negative_control_scheduler_and_event_mutations_are_caught() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let init = init_of(&fx, 3);
    // note offsets dropped (the song's notes start 27, 55, 82, … samples into a block)
    must_mismatch("offsets dropped", || {
        prefix(&fx, PREFIX, init, &|_, e| Some(match e { Ev::On(n, _) => Ev::On(n, 0), Ev::Off(n, _) => Ev::Off(n, 0), p => p }));
    });
    // note-offs dropped: the mono stack never empties and the releases never happen
    must_mismatch("note-offs dropped", || {
        prefix(&fx, PREFIX, init, &|_, e| (!matches!(e, Ev::Off(..))).then_some(e));
    });
    // automation dropped after block 0 (P2 / P3 / P17 ramp every block)
    must_mismatch("automation dropped", || {
        prefix(&fx, PREFIX, init, &|b, e| (b == 0 || !matches!(e, Ev::Param(..))).then_some(e));
    });
    // transposed by one semitone
    must_mismatch("transposed", || {
        prefix(&fx, PREFIX, init, &|_, e| Some(match e { Ev::On(n, o) => Ev::On(n + 1, o), Ev::Off(n, o) => Ev::Off(n + 1, o), p => p }));
    });
    // The mono voice mode (wire 23) cannot be controlled here: the song never overlaps two notes (every note-off precedes
    // its successor's note-on), so poly and mono render the whole song identically, measured. The legato rules are pinned
    // by the synthetic suite and `tests_mono` only.
}

// ------------------------------------------------------------------ always-on hardening of the harness itself

#[test]
fn missing_fixtures_are_a_hard_error() {
    let p = std::env::temp_dir().join("glorp-id14-definitely-missing.csv");
    let err = std::panic::catch_unwind(|| require(&p)).err().expect("require must panic");
    let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("fixture") && msg.contains("glorp-id14-definitely-missing.csv"), "{msg}");
}

#[test]
fn chunk_parser_refuses_a_song_without_exactly_the_two_id14_devices() {
    let song = |ids: &[u8]| {
        let mut blob = vec![0u8; 20];
        blob[16..20].copy_from_slice(&(ids.len() as u32).to_le_bytes());
        for &id in ids {
            let n = if id == 14 { 4 * PARAMS + 4 } else { 32 };
            blob.push(id);
            blob.extend_from_slice(&(n as u32).to_le_bytes());
            blob.extend_from_slice(&vec![0u8; n]);
        }
        blob
    };
    assert!(chunks(&song(&[7, 38])).unwrap_err().contains("found []"));
    assert!(chunks(&song(&[14])).unwrap_err().contains("found [0]"));
    assert!(chunks(&song(&[14, 14])).unwrap_err().contains("found [0, 1]"));
    let mut ids = vec![7u8; 2];
    ids.extend([14, 14]);
    assert_eq!(chunks(&song(&ids)).unwrap().iter().map(|c| c.0).collect::<Vec<_>>(), SONG_DEVICES);
    ids.push(14);
    assert!(chunks(&song(&ids)).unwrap_err().contains("found [2, 3, 4]"));
    let mut blob = vec![0u8; 20];
    blob[16..20].copy_from_slice(&1u32.to_le_bytes());
    blob.push(14);
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
    let p = std::env::temp_dir().join("glorp-id14-bad-events.csv");
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,3,14,boom,1,2,3\n").unwrap();
    let err = std::panic::catch_unwind(|| events_of(&p, 3)).err().unwrap();
    assert!(err.downcast_ref::<String>().cloned().unwrap_or_default().contains("unknown event kind"));
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,3,14,on,1,2\n").unwrap();
    assert!(std::panic::catch_unwind(|| events_of(&p, 3)).is_err());
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,3,14,on,60,127,12\n3,2,14,on,1,1,1\n1,3,14,off,60,7,0\n2,3,14,param,3,0.5,1056964608\n").unwrap();
    let m = events_of(&p, 3);
    assert_eq!(m[&0], vec![Ev::On(60, 12)]);
    assert_eq!(m[&1], vec![Ev::Off(60, 7)]);
    assert_eq!(m[&2], vec![Ev::Param(3, 0.5)]);
    assert!(!m.contains_key(&3), "other devices' rows are ignored");
}
