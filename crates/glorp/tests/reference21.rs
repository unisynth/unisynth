//! Optional external-reference parity: ID 21 (additive recursive sine bank + the shared 16-voice scheduler) vs the
//! original engine's output, replayed through the isolated Wine harness (local only: third-party binary + song never
//! enter git).
//!
//! `GLORP_REF_ROOT` = the `ws-intros/out` directory that holds `389811-2.bin` (read only for the two ID 21 initial
//! parameter sets) and `glorp-win64/analysis/harness/out/id21impl/` with
//!   * `ev.csv` + `taps/dev05.tap`, `taps/dev06.tap`: the **whole song** run (78,292 blocks; song devices 5 and 6 are the
//!     two ID 21 instances) with every note-on/off (with in-block offsets) and every `SetParam` the engine issued (the song
//!     automates P1/P5/P6/P7 of them), and each device's recorded per-block buffers;
//!   * `vec/manifest.json`, `vec/NAME.{csv,tap}`: synthetic vectors (`vec/gen.py`: song settings, the six-note chord,
//!     all-zero / near-all-one parameters, the 512-partial bound, polyphony beyond 16, retrigger, offsets beyond a block,
//!     mid-note parameter writes, note extremes with maximal spread, queue overflow, release shapes, stale release level,
//!     song-like automation, a 5200-block run, parameter sweeps) that the harness fed to a fresh original instance and
//!     recorded (`GLORP_OUT_TAP`).
//!
//! Tap block = 8 x 128 f32: in L, in R, aux L, aux R, then out L, out R, aux L, aux R. The synth ignores the inputs,
//! zeroes and fills out L / out R and leaves the aux pair alone (asserted). `GLORP_REF_ROOT` unset => the tests skip
//! explicitly (they do not prove parity); set but any fixture file missing => they fail loudly, naming the file.
use glorp::id21::{Error, Id21, MAX_PARTIALS, PARAMS};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

const B: usize = 128;
const DEVS: [usize; 2] = [5, 6];
const SONG_BLOCKS: usize = 78_292;

/// Parity bounds: the largest absolute error of a fixture may not exceed `rel * max|ref|` of that fixture. The
/// implementation uses ordinary `libm` exponentials / logarithms / powers where the original uses its own C library;
/// a voice integrates the set-up of every partial (frequency, phase) for its whole life, so a last-bit difference in
/// one partial's `2^x` (the original's `powf`) is a frequency error that grows with time and with the partial's
/// frequency. Measured with this build (release, x86_64 macOS, `libm` 0.2.16; see `research/glorp-devices.md` §ID 21):
///   * song device 6 (31 notes): max abs err 1.19e-7 = 2.8e-8 of the peak (99.9998 % of the samples bit-exact);
///   * song device 5 (167 notes): 9.39e-4 = 2.6e-4 of the peak (99.446 % exact), all of it in six consecutive notes
///     (blocks 69797..70153) played while the automated P5 (ratio spread) is 0.86..0.92: partials 100x above the
///     fundamental, i.e. far above Nyquist, where one ulp of a frequency is a large aliased phase error;
///   * 39 of the 40 synthetic vectors: <= 7.5e-8 of the peak (most bit-exact);
///   * `vec04` (every parameter at 1 except the counts, 512 partials of up to 4e5 Hz): 6.3e-2 of the peak. Perturbing
///     the single frequency of one partial by one ulp brings its error down to 1.7e-4 (diagnostic, not kept), and
///     switching any one of P5 / P7 / P8 / P9 / P10 off makes it exact again.
/// This is tolerance parity, **not** bit parity.
fn song_rel(dev: usize) -> f64 {
    match dev {
        5 => 4e-4,
        _ => 1e-6,
    }
}
const VEC_REL: f64 = 1e-6;
const VEC04_REL: f64 = 9e-2;

fn vec_rel(name: &str) -> f64 {
    if name == "vec04" { VEC04_REL } else { VEC_REL }
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

/// Initial parameters of the ID 21 song devices (the 14th chunk float is the byte size). Exactly the two instances
/// `DEVS`.
fn chunks(blob: &[u8]) -> Result<Vec<(usize, [f32; PARAMS])>, String> {
    let u32at = |p: usize| u32::from_le_bytes(blob[p..p + 4].try_into().unwrap()) as usize;
    let n = u32at(16);
    let mut p = 20;
    let mut found = Vec::new();
    for i in 0..n {
        let (id, size) = (blob[p], u32at(p + 1));
        let c = &blob[p + 5..p + 5 + size];
        p += 5 + size;
        if id == 21 {
            if size != 4 * PARAMS + 4 {
                return Err(format!("ID 21 chunk is {size} bytes"));
            }
            found.push((i, f32s(c)[..PARAMS].try_into().unwrap()));
        }
    }
    if found.iter().map(|(d, _)| *d).collect::<Vec<_>>() != DEVS {
        return Err(format!("expected exactly the ID 21 devices {DEVS:?}, found {} ({:?})", found.len(), found.iter().map(|f| f.0).collect::<Vec<_>>()));
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
            "on" => Ev::On(f[4].parse().unwrap(), f[6].parse().unwrap()),
            "off" => Ev::Off(f[4].parse().unwrap(), f[5].parse::<f64>().unwrap() as u32),
            "param" => Ev::Param(f[4].parse().unwrap(), f32::from_bits(f[6].parse().unwrap())),
            k => panic!("{}:{}: unknown event kind {k}", path.display(), n + 1),
        };
        m.entry(blk).or_default().push(ev);
    }
    m
}

fn apply(d: &mut Id21, ev: &Ev) -> Result<(), Error> {
    match *ev {
        Ev::On(n, o) => d.note_on(n, o),
        Ev::Off(n, o) => d.note_off(n, o),
        Ev::Param(i, v) => d.set_param(i, v),
    }
}

/// Replays `nblocks` blocks of the tap through a fresh `Id21` built from `init`, applying each block's events in log
/// order before the block. `refuse_ok`: tolerate `QueueFull` (counted) instead of panicking.
fn replay(what: &str, init: [f32; PARAMS], nblocks: usize, tap: &mut impl Read, events: &Events, refuse_ok: bool) -> Stats {
    let mut d = Box::new(Id21::new(&init).unwrap_or_else(|e| panic!("{what}: {e:?}")));
    let mut s = Stats::default();
    let mut raw = vec![0u8; 8 * B * 4];
    for b in 0..nblocks {
        for ev in events.get(&b).map(|v| v.as_slice()).unwrap_or(&[]) {
            match apply(&mut d, ev) {
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
            }
        }
    }
    s
}

/// The parity check proper: the replay must stay within `rel` of the fixture's peak (`got ... want` message).
fn replay_parity(what: &str, rel: f64, init: [f32; PARAMS], nblocks: usize, tap: &mut impl Read, events: &Events, refuse_ok: bool) -> Stats {
    let s = replay(what, init, nblocks, tap, events, refuse_ok);
    assert!(
        s.max_abs <= rel * s.max_ref,
        "{what}: max abs err {:e} exceeds {rel} of the peak {} (parity got/want mismatch)",
        s.max_abs,
        s.max_ref
    );
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
    let out = root.join("glorp-win64/analysis/harness/out/id21impl");
    require(&out.join("ev.csv"));
    Some(Fixtures { out, init })
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
    assert_eq!(v.len(), 14 + 26, "manifest.json: expected 14 scenario + 26 sweep vectors, got {}", v.len());
    v
}

fn vector(fx: &Fixtures, name: &str, n: usize) -> (Events, Vec<u8>) {
    let ev = events_of(&require(&fx.out.join(format!("vec/{name}.csv"))), 5);
    let tap = std::fs::read(require(&fx.out.join(format!("vec/{name}.tap")))).unwrap();
    assert_eq!(tap.len(), n * 8 * B * 4, "{name}: tap size does not match the manifest");
    (ev, tap)
}

fn run_vectors(fx: &Fixtures, tweak: &dyn Fn(&str, &mut Events)) -> Vec<(String, Stats)> {
    vector_names(fx)
        .into_iter()
        .map(|(name, n)| {
            let (mut ev, tap) = vector(fx, &name, n);
            tweak(&name, &mut ev);
            let s = replay_parity(&name, vec_rel(&name), [0.0; PARAMS], n, &mut &tap[..], &ev, name == "vec10");
            (name, s)
        })
        .collect()
}

fn song(fx: &Fixtures, dev: usize, tweak: &dyn Fn(&mut Events)) -> Stats {
    let mut ev = events_of(&require(&fx.out.join("ev.csv")), dev);
    tweak(&mut ev);
    let path = require(&fx.out.join(format!("taps/dev{dev:02}.tap")));
    let len = std::fs::metadata(&path).unwrap().len() as usize;
    assert_eq!(len, SONG_BLOCKS * 8 * B * 4, "{}: not the whole-song tap (78,292 blocks)", path.display());
    let init = fx.init.iter().find(|(d, _)| *d == dev).unwrap().1;
    let mut f = BufReader::with_capacity(1 << 20, std::fs::File::open(path).unwrap());
    replay_parity(&format!("song device {dev}"), song_rel(dev), init, SONG_BLOCKS, &mut f, &ev, false)
}

fn report(what: &str, s: &Stats) {
    eprintln!(
        "{what}: {} samples, {} exact ({:.4} %), {} nonzero refs, max |ref| {:.4}, max abs err {:.3e} ({:.2e} of peak), max voices {}",
        s.samples,
        s.exact,
        100.0 * s.exact as f64 / s.samples as f64,
        s.nonzero,
        s.max_ref,
        s.max_abs,
        s.max_abs / s.max_ref.max(1e-30),
        s.max_voices
    );
}

#[test]
fn whole_song_matches_the_original() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    for dev in DEVS {
        let ev = events_of(&require(&fx.out.join("ev.csv")), dev);
        let notes = ev.values().flatten().filter(|e| matches!(e, Ev::On(..))).count();
        let params = ev.values().flatten().filter(|e| matches!(e, Ev::Param(..))).count();
        eprintln!("device {dev}: {notes} note-ons, {params} recorded parameter writes");
        assert_eq!(notes, if dev == 5 { 167 } else { 31 }, "device {dev}: note-on count");
        assert!(params > 0, "device {dev}: the song automates this device (the log must hold param rows)");
        let s = song(&fx, dev, &|_| {});
        report(&format!("song device {dev} (whole song)"), &s);
        assert_eq!(s.samples, SONG_BLOCKS * B * 2);
        assert!(s.nonzero > 500_000, "device {dev}: the whole-song tap is too quiet to mean anything: {} nonzero", s.nonzero);
    }
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
        if name == "vec10" {
            assert_eq!(s.refused, 300 - 256, "vec10: 300 note-ons in one block, 256 slots");
            assert_eq!(s.max_voices, 16);
        }
        if name == "vec06" {
            assert_eq!(s.max_voices, 16, "vec06: 17 simultaneous notes");
        }
        if name == "vec04" || name == "vec05" {
            assert!(s.max_voices >= 1);
        }
    }
    // only the all-zero-parameter vector (gain 0) and the gain-0 sweep may be silent
    for q in &quiet {
        assert!(q == "vec03" || q == "s11lo", "{q}: unexpectedly silent reference");
    }
}

// ------------------------------------------------------------------ negative controls

/// The mutated replay must fail with the `got ... want ...` / parity mismatch (not with some other panic).
fn must_mismatch<R>(what: &str, f: impl FnOnce() -> R + std::panic::UnwindSafe) {
    let err = std::panic::catch_unwind(f).err().unwrap_or_else(|| panic!("the mutation '{what}' was not detected"));
    let msg = err.downcast_ref::<String>().cloned().or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
    assert!(msg.contains("parity got/want mismatch"), "failed for another reason: {msg}");
}

fn nudge(i: usize, by: f32) -> impl Fn(&str, &mut Events) {
    move |_, ev| {
        for e in ev.get_mut(&0).unwrap() {
            if let Ev::Param(k, v) = e {
                if *k == i {
                    *v = if *v + by <= 1.0 { *v + by } else { *v - by };
                }
            }
        }
    }
}

/// Vectors the parameter negative controls run on: song settings, the chord, and a long run.
const CONTROL_VECTORS: [&str; 3] = ["vec01", "vec02", "vec14"];

#[test]
fn negative_control_flipped_reference_bit_is_caught() {
    let Some(fx) = fixtures() else { return };
    let (ev, mut tap) = vector(&fx, "vec02", 1100);
    let blk = (0..1100).find(|b| tap[(b * 8 + 4) * B * 4..(b * 8 + 5) * B * 4].iter().any(|&x| x != 0)).unwrap();
    let i = (0..B).find(|&i| tap[((blk * 8 + 4) * B + i) * 4..][..4] != [0; 4]).unwrap();
    tap[((blk * 8 + 4) * B + i) * 4 + 3] ^= 0x40;
    must_mismatch("flipped bit", || {
        replay_parity("flip", VEC_REL, [0.0; PARAMS], blk + 2, &mut &tap[..], &ev, false);
    });
}

#[test]
fn negative_control_parameter_nudges_are_caught() {
    let Some(fx) = fixtures() else { return };
    let fixtures: Vec<(&str, usize, Events, Vec<u8>)> = CONTROL_VECTORS
        .iter()
        .map(|&name| {
            let n = vector_names(&fx).into_iter().find(|(k, _)| k == name).unwrap().1;
            let (ev, tap) = vector(&fx, name, n);
            (name, n, ev, tap)
        })
        .collect();
    // every parameter of ID 21 is read: a nudge must be caught by the parity bound on at least one control vector
    for i in 0..PARAMS {
        let smallest = [1.0 / 255.0, 0.02, 0.05, 0.1].into_iter().find(|&by| {
            fixtures.iter().any(|(name, n, ev0, tap)| {
                let mut ev = ev0.clone();
                nudge(i, by)(name, &mut ev);
                std::panic::catch_unwind(|| replay_parity("nudge", VEC_REL, [0.0; PARAMS], *n, &mut &tap[..], &ev, false)).is_err()
            })
        });
        eprintln!("P{i}: smallest detected nudge {smallest:?}");
        assert!(smallest.is_some(), "a 0.1 nudge of P{i} was not detected on any control vector");
    }
}

#[test]
fn negative_control_scheduler_and_event_mutations_are_caught() {
    let Some(fx) = fixtures() else { return };
    let (ev, tap) = vector(&fx, "vec06", 700);
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
        replay_parity("no-offsets", VEC_REL, [0.0; PARAMS], 700, &mut &tap[..], &no_offsets, false);
    });
    // note-offs are audible here (they start the release): dropping them is caught
    let no_off: Events = ev.iter().map(|(b, v)| (*b, v.iter().copied().filter(|e| !matches!(e, Ev::Off(..))).collect())).collect();
    must_mismatch("note-offs dropped", || {
        replay_parity("no-offs", VEC_REL, [0.0; PARAMS], 700, &mut &tap[..], &no_off, false);
    });
    // the 17th simultaneous note was dropped by the original: removing it changes nothing, removing the 16th does
    let drop_note = |note: u8| -> Events {
        ev.iter().map(|(b, v)| (*b, v.iter().copied().filter(|e| !(*b == 0 && matches!(e, Ev::On(n, _) if *n == note))).collect())).collect()
    };
    replay_parity("17th note removed", VEC_REL, [0.0; PARAMS], 700, &mut &tap[..], &drop_note(40 + 32), false);
    must_mismatch("16th note removed", || {
        replay_parity("16th note removed", VEC_REL, [0.0; PARAMS], 700, &mut &tap[..], &drop_note(40 + 30), false);
    });
    // a note shifted by one semitone
    let (ev, tap) = vector(&fx, "vec11", 3000);
    let shifted: Events = ev.iter().map(|(b, v)| (*b, v.iter().map(|e| if let Ev::On(n, o) = *e { Ev::On(n + 1, o) } else { *e }).collect())).collect();
    must_mismatch("notes shifted by one", || {
        replay_parity("shifted", VEC_REL, [0.0; PARAMS], 3000, &mut &tap[..], &shifted, false);
    });
}

#[test]
fn negative_control_seed_ulp_and_dropped_song_automation_are_caught() {
    let Some(fx) = fixtures() else { return };
    // P7 is a seed down to the last bit: one ulp of it on a song vector changes the partials (vec01 uses song device 5's P7)
    let (ev, tap) = vector(&fx, "vec01", 700);
    let mut ulp = ev.clone();
    for e in ulp.get_mut(&0).unwrap() {
        if let Ev::Param(7, v) = e {
            *v = f32::from_bits(v.to_bits() + 1);
        }
    }
    must_mismatch("P7 one ulp", || {
        replay_parity("seed ulp", VEC_REL, [0.0; PARAMS], 700, &mut &tap[..], &ulp, false);
    });
    // the song's own automation of device 5 dropped / injected
    must_mismatch("song automation dropped", || {
        song(&fx, 5, &|ev| {
            for v in ev.values_mut() {
                v.retain(|e| !matches!(e, Ev::Param(..)));
            }
        });
    });
    // device 6's P7 lane (the seed) dropped: the chunk's value is not the one the song plays with. (An injected write is
    // not a test: the log rewrites every automated parameter at every block, so it is overwritten before it is heard.)
    must_mismatch("device 6 P7 writes dropped", || {
        song(&fx, 6, &|ev| {
            for v in ev.values_mut() {
                v.retain(|e| !matches!(e, Ev::Param(7, _)));
            }
        });
    });
}

// ------------------------------------------------------------------ always-on hardening of the harness itself

#[test]
fn missing_fixtures_are_a_hard_error() {
    let p = std::env::temp_dir().join("glorp-id21-definitely-missing.csv");
    let err = std::panic::catch_unwind(|| require(&p)).err().expect("require must panic");
    let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("fixture") && msg.contains("glorp-id21-definitely-missing.csv"), "{msg}");
    // a set root that lacks the song blob fails loudly through `fixtures()` (the variable is process-global: use a child
    // scope that restores it)
    let dir = std::env::temp_dir().join("glorp-id21-empty-root");
    std::fs::create_dir_all(&dir).unwrap();
    let err = std::panic::catch_unwind(|| require(&dir.join("389811-2.bin"))).err().unwrap();
    assert!(err.downcast_ref::<String>().cloned().unwrap_or_default().contains("389811-2.bin"));
}

#[test]
fn chunk_parser_refuses_a_song_without_exactly_the_two_id21_devices() {
    let build = |ids: &[u8]| {
        let mut blob = vec![0u8; 20];
        blob[16..20].copy_from_slice(&(ids.len() as u32).to_le_bytes());
        for &id in ids {
            blob.push(id);
            blob.extend_from_slice(&56u32.to_le_bytes());
            blob.extend_from_slice(&[0u8; 56]);
        }
        blob
    };
    assert!(chunks(&build(&[7, 7])).unwrap_err().contains("found 0"));
    assert!(chunks(&build(&[1, 1, 1, 1, 1, 21])).unwrap_err().contains("found 1"));
    let mut ids = [1u8; 8];
    ids[5] = 21;
    ids[6] = 21;
    ids[7] = 21;
    assert!(chunks(&build(&ids)).unwrap_err().contains("found 3"));
    ids[7] = 1;
    assert_eq!(chunks(&build(&ids)).unwrap().iter().map(|c| c.0).collect::<Vec<_>>(), DEVS);
    let mut wrong = build(&[21]);
    wrong[21..25].copy_from_slice(&60u32.to_le_bytes());
    wrong.extend_from_slice(&[0u8; 4]);
    assert!(chunks(&wrong).unwrap_err().contains("60 bytes"));
}

#[test]
fn short_tap_is_a_hard_error() {
    let tap = vec![0u8; 8 * B * 4 * 2 + 17];
    let err = std::panic::catch_unwind(|| replay("short", [0.0; PARAMS], 3, &mut &tap[..], &Events::new(), false)).err().unwrap();
    let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("short tap"), "{msg}");
}

#[test]
fn event_parser_rejects_malformed_rows() {
    let p = std::env::temp_dir().join("glorp-id21-bad-events.csv");
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,5,21,boom,1,2,3\n").unwrap();
    let err = std::panic::catch_unwind(|| events_of(&p, 5)).err().unwrap();
    assert!(err.downcast_ref::<String>().cloned().unwrap_or_default().contains("unknown event kind"));
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,5,21,on,1,2\n").unwrap();
    assert!(std::panic::catch_unwind(|| events_of(&p, 5)).is_err());
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,5,21,on,60,127,12\n3,6,21,on,1,1,1\n1,5,21,off,60,7,0\n2,5,21,param,3,0.5,1056964608\n").unwrap();
    let m = events_of(&p, 5);
    assert_eq!(m[&0], vec![Ev::On(60, 12)]);
    assert_eq!(m[&1], vec![Ev::Off(60, 7)]);
    assert_eq!(m[&2], vec![Ev::Param(3, 0.5)]);
    assert!(!m.contains_key(&3), "other devices' rows are ignored");
}

/// The bound that keeps the memory fixed is a hard refusal, not a clamp: a recorded write that would exceed it fails the
/// replay loudly instead of rendering something else.
#[test]
fn a_write_beyond_the_partial_bound_is_refused_loudly() {
    let mut d = Box::new(Id21::new(&[0.0; PARAMS]).unwrap());
    assert_eq!(MAX_PARTIALS, 512);
    d.set_param(4, 0.3105).unwrap();
    assert_eq!(d.set_param(6, 0.2), Err(Error::Partials));
    let err = std::panic::catch_unwind(|| {
        let mut ev = Events::new();
        ev.insert(0, vec![Ev::Param(4, 1.0), Ev::Param(6, 1.0)]);
        replay("bound", [0.0; PARAMS], 1, &mut &vec![0u8; 8 * B * 4][..], &ev, false)
    })
    .err()
    .unwrap();
    assert!(err.downcast_ref::<String>().cloned().unwrap_or_default().contains("rejected"));
}
