//! Optional external-reference parity: ID 27 (comb-resonator percussion synth + the shared 16-voice scheduler) vs the
//! original engine's output, replayed through the isolated Wine harness (local only: third-party binary + song never
//! enter git).
//!
//! `GLORP_REF_ROOT` = the `ws-intros/out` directory that holds `389811-2.bin` (read only for the two ID 27 initial
//! parameter sets) and `glorp-win64/analysis/harness/out/id27impl/` with
//!   * `ev.csv` + `taps/dev09.tap`, `taps/dev10.tap`: the **whole song** run (78,292 blocks; song devices 9 and 10 are the
//!     two ID 27 instances) with every note-on/off (with in-block offsets) and every `SetParam` the engine issued, and
//!     each device's recorded per-block buffers;
//!   * `vec/manifest.json`, `vec/NAME.{csv,tap}`: synthetic vectors (`vec/gen.py`: song settings, extreme parameters,
//!     polyphony beyond 16, retrigger, offsets beyond a block, queue overflow, note extremes, sequential notes on one
//!     voice, mid-note parameter changes, parameter sweeps) that the harness fed to a fresh original instance and
//!     recorded (`GLORP_OUT_TAP`).
//!
//!   * `expiry/NAME.{csv,tap}` + `expiry/manifest.json` (`expiry/gen.py`, `expiry/capture.sh`): a voice expiring inside a block
//!     with / without a sub-block cut (an inaudible note-off for a note that never sounded) next to the expiry sample, then
//!     two more notes on the same slot (the delay write position persists between notes), captured like the others.
//!
//! Tap block = 8 × 128 f32: in L, in R, aux L, aux R, then out L, out R, aux L, aux R. The synth ignores the inputs,
//! zeroes and fills out L / out R and leaves the aux pair alone (asserted). `GLORP_REF_ROOT` unset => the tests skip
//! explicitly (they do not prove parity); set but any fixture file missing => they fail loudly, naming the file.
use glorp::id27::{Error, Id27, PARAMS};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

const B: usize = 128;
const DEVS: [usize; 2] = [9, 10];
const SONG_BLOCKS: usize = 78_292;

/// Parity bounds: the largest absolute error of a fixture may not exceed `rel · max|ref|` of that fixture. The
/// implementation uses ordinary `libm` exponentials / sines and true division where the original uses its own C library
/// and a reciprocal estimate, and re-designs four biquads per sample, so the last-bit differences accumulate. Measured
/// with this build (release, x86_64 macOS, `libm` 0.2.16): whole song device 9 6.71e-4 of peak, device 10 5.63e-5
/// (SONG_REL = 1.5x the worst); 41 vectors worst `s10lo` 4.23e-3 (an HPF at 20 Hz amplifies coefficient ulps), then
/// `vec08` 6.3e-4, `vec13` 6.0e-4 (VEC_REL = 1.4x the worst). This is **not** bit parity.
const SONG_REL: f64 = 1e-3;
const VEC_REL: f64 = 6e-3;

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

/// Initial parameters of the ID 27 song devices (the 14th chunk float is the byte size). Exactly the two instances
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
        if id == 27 {
            if size != 4 * PARAMS + 4 {
                return Err(format!("ID 27 chunk is {size} bytes"));
            }
            found.push((i, f32s(c)[..PARAMS].try_into().unwrap()));
        }
    }
    if found.iter().map(|(d, _)| *d).collect::<Vec<_>>() != DEVS {
        return Err(format!("expected exactly the ID 27 devices {DEVS:?}, found {} ({:?})", found.len(), found.iter().map(|f| f.0).collect::<Vec<_>>()));
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

/// Replays `nblocks` blocks of the tap through a fresh `Id27` built from `init`, applying each block's events in log
/// order before the block. `refuse_ok`: tolerate `QueueFull` (counted) instead of panicking.
fn replay(what: &str, init: [f32; PARAMS], nblocks: usize, tap: &mut impl Read, events: &Events, refuse_ok: bool) -> Stats {
    let mut d = Box::new(Id27::new(&init).unwrap_or_else(|e| panic!("{what}: {e:?}")));
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
            }
        }
    }
    s
}

/// The parity check proper: the replay must stay within `rel` of the fixture's peak (`got … want` message).
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

/// Stereo output of a fresh `Id27` over `nblocks` blocks (per block: L then R), events applied before their block.
fn render_out(init: [f32; PARAMS], nblocks: usize, events: &Events) -> Vec<f32> {
    let mut d = Box::new(Id27::new(&init).unwrap());
    let mut out = Vec::with_capacity(nblocks * 2 * B);
    for b in 0..nblocks {
        for ev in events.get(&b).map(|v| v.as_slice()).unwrap_or(&[]) {
            match *ev {
                Ev::On(n, o) => d.note_on(n, o),
                Ev::Off(n, o) => d.note_off(n, o),
                Ev::Param(i, v) => d.set_param(i, v),
            }
            .unwrap();
        }
        let (mut l, mut r) = (vec![7.0f32; B], vec![7.0f32; B]);
        d.process(&mut l, &mut r);
        out.extend_from_slice(&l);
        out.extend_from_slice(&r);
    }
    out
}

struct Fixtures {
    out: PathBuf,
    init: Vec<(usize, [f32; PARAMS])>,
}

fn fixtures() -> Option<Fixtures> {
    let root = PathBuf::from(std::env::var_os("GLORP_REF_ROOT")?);
    let blob = std::fs::read(require(&root.join("389811-2.bin"))).unwrap();
    let init = chunks(&blob).unwrap_or_else(|e| panic!("389811-2.bin: {e}"));
    Some(Fixtures { out: root.join("glorp-win64/analysis/harness/out/id27impl"), init })
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
    assert_eq!(v.len(), 15 + 26, "manifest.json: expected 15 scenario + 26 sweep vectors, got {}", v.len());
    v
}

fn vector(fx: &Fixtures, name: &str, n: usize) -> (Events, Vec<u8>) {
    let ev = events_of(&require(&fx.out.join(format!("vec/{name}.csv"))), 9);
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
            let s = replay_parity(&name, VEC_REL, fx.init[0].1, n, &mut &tap[..], &ev, name == "vec10");
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
    replay_parity(&format!("song device {dev}"), SONG_REL, init, SONG_BLOCKS, &mut f, &ev, false)
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
        // the song never writes an ID 27 parameter: the chunk is the whole parameter history of both instances
        let ev = events_of(&require(&fx.out.join("ev.csv")), dev);
        assert!(ev.values().flatten().all(|e| !matches!(e, Ev::Param(..))), "device {dev}: the song automates an ID 27 parameter");
        let notes = ev.values().flatten().filter(|e| matches!(e, Ev::On(..))).count();
        assert_eq!(notes, if dev == 9 { 41 } else { 300 }, "device {dev}: note-on count");
        let s = song(&fx, dev, &|_| {});
        report(&format!("song device {dev} (whole song)"), &s);
        assert_eq!(s.samples, SONG_BLOCKS * B * 2);
        assert!(s.nonzero > 1_000_000, "device {dev}: the whole-song tap is too quiet to mean anything: {} nonzero", s.nonzero);
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
    }
    // only the all-zero-parameter vector (gain 0) and the gain-0 sweep may be silent
    for q in &quiet {
        assert!(q == "vec04" || q == "s08lo", "{q}: unexpectedly silent reference");
    }
}

// ------------------------------------------------------------------ negative controls

/// The mutated replay must fail with the `got … want …` / parity mismatch (not with some other panic).
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

/// Vectors the parameter negative controls run on: a song-settings vector, a mid-range one and a sweep-free extreme.
const CONTROL_VECTORS: [&str; 3] = ["vec01", "vec02", "vec14"];

#[test]
fn negative_control_flipped_reference_bit_is_caught() {
    let Some(fx) = fixtures() else { return };
    let (ev, mut tap) = vector(&fx, "vec02", 900);
    let blk = (0..900).find(|b| tap[(b * 8 + 4) * B * 4..(b * 8 + 5) * B * 4].iter().any(|&x| x != 0)).unwrap();
    // exponent MSB of one nonzero sample
    let i = (0..B).find(|&i| tap[((blk * 8 + 4) * B + i) * 4..][..4] != [0; 4]).unwrap();
    tap[((blk * 8 + 4) * B + i) * 4 + 3] ^= 0x40;
    must_mismatch("flipped bit", || {
        replay_parity("flip", VEC_REL, fx.init[1].1, blk + 2, &mut &tap[..], &ev, false);
    });
}

#[test]
fn negative_control_parameter_nudges_are_caught_and_p6_is_not() {
    let Some(fx) = fixtures() else { return };
    let fixtures: Vec<(&str, usize, Events, Vec<u8>)> = CONTROL_VECTORS
        .iter()
        .map(|&name| {
            let n = vector_names(&fx).into_iter().find(|(k, _)| k == name).unwrap().1;
            let (ev, tap) = vector(&fx, name, n);
            (name, n, ev, tap)
        })
        .collect();
    // a nudge must be caught by the parity bound on at least one control vector; report the smallest one that is
    for i in (0..PARAMS).filter(|&i| i != 6) {
        let smallest = [1.0 / 255.0, 0.02, 0.05, 0.1].into_iter().find(|&by| {
            fixtures.iter().any(|(name, n, ev0, tap)| {
                let mut ev = ev0.clone();
                nudge(i, by)(name, &mut ev);
                std::panic::catch_unwind(|| replay_parity("nudge", VEC_REL, fx.init[0].1, *n, &mut &tap[..], &ev, false)).is_err()
            })
        });
        eprintln!("P{i}: smallest detected nudge {smallest:?}");
        assert!(smallest.is_some(), "a 0.1 nudge of P{i} was not detected on any control vector");
    }
    for (name, n, ev0, tap) in &fixtures {
        let mut ev = ev0.clone();
        nudge(6, 0.1)(name, &mut ev);
        replay_parity("inert", VEC_REL, fx.init[0].1, *n, &mut &tap[..], &ev, false); // never read: must still match
    }
}

#[test]
fn negative_control_scheduler_mutations_are_caught() {
    let Some(fx) = fixtures() else { return };
    // offsets dropped (every event fires at sample 0 of its block)
    let (ev, tap) = vector(&fx, "vec06", 1900);
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
        replay_parity("no-offsets", VEC_REL, fx.init[0].1, 1900, &mut &tap[..], &no_offsets, false);
    });
    // note-offs are inaudible in the original too: dropping them still matches
    let no_off: Events = ev.iter().map(|(b, v)| (*b, v.iter().copied().filter(|e| !matches!(e, Ev::Off(..))).collect())).collect();
    replay_parity("no-offs", VEC_REL, fx.init[0].1, 1900, &mut &tap[..], &no_off, false);
    // the 17th simultaneous note was dropped by the original: removing it changes nothing, removing the 16th does
    let drop_note = |note: u8| -> Events {
        ev.iter().map(|(b, v)| (*b, v.iter().copied().filter(|e| !(*b == 0 && matches!(e, Ev::On(n, _) if *n == note))).collect())).collect()
    };
    replay_parity("17th note removed", VEC_REL, fx.init[0].1, 1900, &mut &tap[..], &drop_note(30 + 16), false);
    must_mismatch("16th note removed", || {
        replay_parity("16th note removed", VEC_REL, fx.init[0].1, 1900, &mut &tap[..], &drop_note(30 + 15), false);
    });
}

#[test]
fn negative_control_a_changed_note_or_an_injected_automation_is_caught() {
    let Some(fx) = fixtures() else { return };
    let (ev, tap) = vector(&fx, "vec11", 1000);
    let shifted: Events = ev.iter().map(|(b, v)| (*b, v.iter().map(|e| if let Ev::On(n, o) = *e { Ev::On((n + 1).min(127), o) } else { *e }).collect())).collect();
    must_mismatch("notes shifted by one", || {
        replay_parity("shifted", VEC_REL, fx.init[1].1, 1000, &mut &tap[..], &shifted, false);
    });
    // the song writes no ID 27 parameter; an automation write that was never there must be visible
    let ev = events_of(&require(&fx.out.join("ev.csv")), 10);
    let mut with_auto = ev.clone();
    with_auto.entry(20_000).or_default().push(Ev::Param(0, 0.5));
    must_mismatch("injected automation", || {
        let path = require(&fx.out.join("taps/dev10.tap"));
        let mut f = BufReader::with_capacity(1 << 20, std::fs::File::open(path).unwrap());
        replay_parity("injected", SONG_REL, fx.init[1].1, SONG_BLOCKS, &mut f, &with_auto, false);
    });
}

// ------------------------------------------------------------------ voice expiry inside a block

/// Block counts of the expiry vectors (`expiry/manifest.json`): third note at block 1740, 360 blocks of it.
const EXPIRY_BLOCKS: usize = 2100;
const EXPIRY_NAMES: [&str; 10] =
    ["n20_mid", "n20_cut12", "n20_cut13", "n20_cut14", "n104_mid", "n104_cut13", "o77_mid", "o77_cut90", "restart13", "restart90"];
/// Parity bound of the expiry vectors (fraction of the peak). Measured worst 1.77e-3 (`n104_mid`), the others ≤ 1.1e-3;
/// tighter than `VEC_REL` so that the effect under test (up to 5.7e-3 of the peak) cannot hide inside it.
const EXPIRY_REL: f64 = 2.5e-3;

fn expiry_tap(fx: &Fixtures, name: &str) -> Vec<u8> {
    let tap = std::fs::read(require(&fx.out.join(format!("expiry/{name}.tap")))).unwrap();
    assert_eq!(tap.len(), EXPIRY_BLOCKS * 8 * B * 4, "expiry/{name}: tap size");
    tap
}

fn expiry_events(fx: &Fixtures, name: &str) -> Events {
    events_of(&require(&fx.out.join(format!("expiry/{name}.csv"))), 9)
}

/// The original's stereo output (per block L then R) of a tap.
fn tap_out(tap: &[u8]) -> Vec<f32> {
    (0..tap.len() / (8 * B * 4)).flat_map(|b| f32s(&tap[(b * 8 + 4) * B * 4..(b * 8 + 6) * B * 4])).collect()
}

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

fn max_abs(a: &[f32], b: &[f32]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).fold(0.0, |m, (x, y)| m.max((*x as f64 - *y as f64).abs()))
}

fn diff(a: &[f32], b: &[f32]) -> Vec<f32> {
    a.iter().zip(b).map(|(x, y)| x - y).collect()
}

/// Every expiry vector (a voice expiring inside a block, with or without a sub-block cut next to the expiry sample,
/// then two more notes on the same slot) matches the original's output.
#[test]
fn voice_expiry_vectors_match_the_original() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let manifest = std::fs::read_to_string(require(&fx.out.join("expiry/manifest.json"))).unwrap();
    assert_eq!(manifest.matches(&format!(": {EXPIRY_BLOCKS}")).count(), EXPIRY_NAMES.len(), "expiry/manifest.json: vector count / block count");
    for name in EXPIRY_NAMES {
        let tap = expiry_tap(&fx, name);
        let s = replay_parity(name, EXPIRY_REL, fx.init[0].1, EXPIRY_BLOCKS, &mut &tap[..], &expiry_events(&fx, name), false);
        report(name, &s);
        assert!(s.nonzero > 100_000, "{name}: reference too quiet");
    }
}

/// The original renders the rest of the sub-block a voice expires in: its tail after the expiry sample exists, a cut
/// right after that sample removes it, and the extra samples move the persistent delay write position, which changes the
/// next notes on the slot by up to 5.7 % of the peak (`n20`). The implementation reproduces that difference, not just
/// each output: a cut right after the expiry sample (what one-sample calls would do) differs from the unsplit block.
#[test]
fn rendering_past_the_expiry_sample_is_what_the_original_does() {
    let Some(fx) = fixtures() else { return };
    let load = |name: &str| {
        let want = tap_out(&expiry_tap(&fx, name));
        let got = render_out(fx.init[0].1, EXPIRY_BLOCKS, &expiry_events(&fx, name));
        (got, want)
    };
    let tail = |o: &[f32]| -> f32 {
        // block 862: the first note's last sample is offset 12 (offset 0 start) of L (first B floats) and R
        let base = 862 * 2 * B;
        (13..B).flat_map(|i| [o[base + i], o[base + B + i]]).fold(0.0f32, |m, x| m.max(x.abs()))
    };
    let (gm, wm) = load("n20_mid");
    let (gc, wc) = load("n20_cut13");
    assert!(tail(&wm) > 0.0 && tail(&gm) > 0.0, "the original / the implementation render past the expiry sample (tail {:e} / {:e})", tail(&wm), tail(&gm));
    assert_eq!((tail(&wc), tail(&gc)), (0.0, 0.0), "a cut right after the expiry sample leaves nothing to render");
    // a cut BEFORE the expiry sample changes nothing, bit for bit, in the original and in the implementation
    let (g12, w12) = load("n20_cut12");
    assert_eq!(bits(&w12), bits(&wm), "the original: cut at offset 12 vs unsplit");
    assert_eq!(bits(&g12), bits(&gm), "the implementation: cut at offset 12 vs unsplit");
    for (mid, cut, strict) in [("n20_mid", "n20_cut13", true), ("n20_mid", "n20_cut14", true), ("n104_mid", "n104_cut13", false), ("o77_mid", "o77_cut90", false)] {
        let (gm, wm) = load(mid);
        let (gc, wc) = load(cut);
        let (dref, dgot) = (diff(&wc, &wm), diff(&gc, &gm));
        let zero = vec![0.0f32; dref.len()];
        let (r, g, dd) = (max_abs(&dref, &zero), max_abs(&dgot, &zero), max_abs(&dref, &dgot));
        eprintln!("{cut} vs {mid}: |ref diff| {r:.3e}, |impl diff| {g:.3e}, |diff of diffs| {dd:.3e}");
        assert!(r > 2e-3, "{cut}: the original must tell the two apart (diff {r:e})");
        assert!((0.85 * r..=1.15 * r).contains(&g), "{cut}: the implementation's difference {g:e} is not the original's {r:e}");
        assert!(dd <= if strict { 0.15 } else { 0.6 } * r, "{cut}: difference of differences {dd:e} of {r:e}");
    }
}

/// The mutation 'stop at the expiry sample' (= the cut vector's events) must be rejected by the unsplit vector's tap and
/// the other way round: the parity bound separates the two behaviours.
#[test]
fn negative_control_stopping_at_the_expiry_sample_is_caught() {
    let Some(fx) = fixtures() else { return };
    for (events, tap) in [("n20_cut13", "n20_mid"), ("n20_mid", "n20_cut13"), ("n20_cut14", "n20_mid")] {
        let ev = expiry_events(&fx, events);
        let tap = expiry_tap(&fx, tap);
        must_mismatch(events, || {
            replay_parity("expiry mutation", EXPIRY_REL, fx.init[0].1, EXPIRY_BLOCKS, &mut &tap[..], &ev, false);
        });
    }
}

// ------------------------------------------------------------------ always-on hardening of the harness itself

#[test]
fn missing_fixtures_are_a_hard_error() {
    let p = std::env::temp_dir().join("glorp-id27-definitely-missing.csv");
    let err = std::panic::catch_unwind(|| require(&p)).err().expect("require must panic");
    let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("fixture") && msg.contains("glorp-id27-definitely-missing.csv"), "{msg}");
}

#[test]
fn chunk_parser_refuses_a_song_without_exactly_the_two_id27_devices() {
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
    assert!(chunks(&build(&[1, 1, 1, 1, 1, 1, 1, 1, 1, 27])).unwrap_err().contains("found 1"));
    let mut ids = [1u8; 12];
    ids[9] = 27;
    ids[10] = 27;
    ids[11] = 27;
    assert!(chunks(&build(&ids)).unwrap_err().contains("found 3"));
    ids[11] = 1;
    assert_eq!(chunks(&build(&ids)).unwrap().iter().map(|c| c.0).collect::<Vec<_>>(), DEVS);
    let mut wrong = build(&[27]);
    wrong[21..25].copy_from_slice(&60u32.to_le_bytes());
    wrong.extend_from_slice(&[0u8; 4]);
    assert!(chunks(&wrong).unwrap_err().contains("60 bytes"));
}

#[test]
fn short_tap_is_a_hard_error() {
    let tap = vec![0u8; 8 * B * 4 * 2 + 17];
    let err = std::panic::catch_unwind(|| replay("short", [0.5; PARAMS], 3, &mut &tap[..], &Events::new(), false)).err().unwrap();
    let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("short tap"), "{msg}");
}

#[test]
fn event_parser_rejects_malformed_rows() {
    let p = std::env::temp_dir().join("glorp-id27-bad-events.csv");
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,9,27,boom,1,2,3\n").unwrap();
    let err = std::panic::catch_unwind(|| events_of(&p, 9)).err().unwrap();
    assert!(err.downcast_ref::<String>().cloned().unwrap_or_default().contains("unknown event kind"));
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,9,27,on,1,2\n").unwrap();
    assert!(std::panic::catch_unwind(|| events_of(&p, 9)).is_err());
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,9,27,on,60,127,12\n3,10,27,on,1,1,1\n1,9,27,off,60,7,0\n2,9,27,param,3,0.5,1056964608\n").unwrap();
    let m = events_of(&p, 9);
    assert_eq!(m[&0], vec![Ev::On(60, 12)]);
    assert_eq!(m[&1], vec![Ev::Off(60, 7)]);
    assert_eq!(m[&2], vec![Ev::Param(3, 0.5)]);
    assert!(!m.contains_key(&3), "other devices' rows are ignored");
}
