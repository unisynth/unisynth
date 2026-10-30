//! Optional external-reference parity: ID 20 (twelve negative-feedback combs, noise / click excitation, per-group and
//! master frequency shifters, velocity sensitive, on the shared 16-voice scheduler) vs the original engine's output,
//! replayed through the isolated Wine harness (local only: third-party binary + song never enter git).
//!
//! `GLORP_REF_ROOT` = the `ws-intros/out` directory that holds `389811-2.bin` (read only for the one ID 20 initial
//! parameter set) and `glorp-win64/analysis/harness/out/id20impl/` with
//!   * `ev.csv` + `taps/dev04.tap`: the **whole song** run (78,292 blocks; song device 4 is the one ID 20 instance) with
//!     every note-on/off (velocity and in-block offset) and every `SetParam` the engine issued (`--events-all`: the song
//!     automates P5, P8, P9, P15), and the device's recorded per-block buffers;
//!   * `vec/manifest.json`, `vec/NAME.{csv,tap}`: synthetic vectors (`vec/gen.py`: song settings, extreme parameters,
//!     polyphony beyond 16, retrigger, offsets beyond a block, queue overflow, note / velocity extremes, sequential notes
//!     on one voice, mid-note parameter changes, level-follower kills of short sub-blocks, a velocity sweep, parameter
//!     sweeps) that the harness fed to a fresh original instance and recorded (`GLORP_OUT_TAP`); the `unsafe_*` ones record
//!     what the original does above the crate's velocity ceiling.
//!
//! Tap block = 8 × 128 f32: in L, in R, aux L, aux R, then out L, out R, aux L, aux R. The synth ignores the inputs,
//! zeroes and fills out L / out R and leaves the aux pair alone (asserted). `GLORP_REF_ROOT` unset => the tests skip
//! explicitly (they do not prove parity); set but any fixture file missing => they fail loudly, naming the file.
use glorp::id20::{Error, Id20, MAX_VELOCITY, PARAMS};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

const B: usize = 128;
const DEV: usize = 4;
const SONG_BLOCKS: usize = 78_292;

/// Parity bounds: the largest absolute error of a fixture may not exceed `rel · max|ref|` of that fixture. Measured: the
/// 55 synthetic vectors are bit-exact (error 0, `VEC_REL` only has to separate that from any real difference); the whole
/// song is bit-exact except 1,499 of 78,292 blocks (from block 38,967 on) with a worst error of 1.1e-4 = 2.4e-5 of its
/// peak: the original's `exp` rounds some arguments one ulp differently from `libm` (the documented `expf` choice of ID 25),
/// and the comb loops carry that one ulp of the noise envelope forward. `SONG_REL` = 1e-4, four times the measurement.
const SONG_REL: f64 = 1e-4;
const VEC_REL: f64 = 1e-9;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Ev {
    /// note, velocity, offset
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
}

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}

fn require(p: &Path) -> PathBuf {
    assert!(p.is_file(), "GLORP_REF_ROOT is set but the fixture {} is missing", p.display());
    p.to_path_buf()
}

/// Initial parameters of the ID 20 song device (the 17th chunk float is the byte size). Exactly one instance, device `DEV`.
fn chunks(blob: &[u8]) -> Result<[f32; PARAMS], String> {
    let u32at = |p: usize| u32::from_le_bytes(blob[p..p + 4].try_into().unwrap()) as usize;
    let n = u32at(16);
    let mut p = 20;
    let mut found = Vec::new();
    for i in 0..n {
        let (id, size) = (blob[p], u32at(p + 1));
        let c = &blob[p + 5..p + 5 + size];
        p += 5 + size;
        if id == 20 {
            if size != 4 * PARAMS + 4 {
                return Err(format!("ID 20 chunk is {size} bytes"));
            }
            found.push((i, f32s(c)[..PARAMS].try_into().unwrap()));
        }
    }
    match found.as_slice() {
        [(d, p)] if *d == DEV => Ok(*p),
        _ => Err(format!("expected exactly the ID 20 device {DEV}, found {} ({:?})", found.len(), found.iter().map(|f| f.0).collect::<Vec<_>>())),
    }
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
            "on" => Ev::On(f[4].parse().unwrap(), f[5].parse().unwrap(), f[6].parse().unwrap()),
            "off" => Ev::Off(f[4].parse().unwrap(), f[5].parse::<f64>().unwrap() as u32),
            "param" => Ev::Param(f[4].parse().unwrap(), f32::from_bits(f[6].parse().unwrap())),
            k => panic!("{}:{}: unknown event kind {k}", path.display(), n + 1),
        };
        m.entry(blk).or_default().push(ev);
    }
    m
}

fn apply(d: &mut Id20, ev: Ev) -> Result<(), Error> {
    match ev {
        Ev::On(n, v, o) => d.note_on(n, v, o),
        Ev::Off(n, o) => d.note_off(n, o),
        Ev::Param(i, v) => d.set_param(i, v),
    }
}

/// Replays `nblocks` blocks of the tap through a fresh `Id20` built from `init`, applying each block's events in log
/// order before the block. `refuse_ok`: tolerate `QueueFull` (counted) instead of panicking.
fn replay(what: &str, init: [f32; PARAMS], nblocks: usize, tap: &mut impl Read, events: &Events, refuse_ok: bool) -> Stats {
    let mut d = Box::new(Id20::new(&init).unwrap_or_else(|e| panic!("{what}: {e:?}")));
    let mut s = Stats::default();
    let mut raw = vec![0u8; 8 * B * 4];
    for b in 0..nblocks {
        for ev in events.get(&b).map(|v| v.as_slice()).unwrap_or(&[]) {
            match apply(&mut d, *ev) {
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

struct Fixtures {
    out: PathBuf,
    init: [f32; PARAMS],
}

fn fixtures() -> Option<Fixtures> {
    let root = PathBuf::from(std::env::var_os("GLORP_REF_ROOT")?);
    let blob = std::fs::read(require(&root.join("389811-2.bin"))).unwrap();
    let init = chunks(&blob).unwrap_or_else(|e| panic!("389811-2.bin: {e}"));
    Some(Fixtures { out: root.join("glorp-win64/analysis/harness/out/id20impl"), init })
}

/// Vectors of `vec/manifest.json` as `(name, blocks)`, sorted.
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
    assert_eq!(v.len(), 23 + 32, "manifest.json: expected 23 scenario + 32 sweep vectors, got {}", v.len());
    v
}

fn vector(fx: &Fixtures, name: &str, n: usize) -> (Events, Vec<u8>) {
    let ev = events_of(&require(&fx.out.join(format!("vec/{name}.csv"))), DEV);
    let tap = std::fs::read(require(&fx.out.join(format!("vec/{name}.tap")))).unwrap();
    assert_eq!(tap.len(), n * 8 * B * 4, "{name}: tap size does not match the manifest");
    (ev, tap)
}

fn blocks_of(fx: &Fixtures, name: &str) -> usize {
    vector_names(fx).into_iter().find(|(k, _)| k == name).unwrap_or_else(|| panic!("{name} is not in the manifest")).1
}

fn is_unsafe(name: &str) -> bool {
    name.starts_with("unsafe_")
}

fn run_vectors(fx: &Fixtures, tweak: &dyn Fn(&str, &mut Events)) -> Vec<(String, Stats)> {
    vector_names(fx)
        .into_iter()
        .filter(|(name, _)| !is_unsafe(name))
        .map(|(name, n)| {
            let (mut ev, tap) = vector(fx, &name, n);
            tweak(&name, &mut ev);
            let s = replay_parity(&name, VEC_REL, fx.init, n, &mut &tap[..], &ev, name == "vec08");
            (name, s)
        })
        .collect()
}

fn song(fx: &Fixtures, tweak: &dyn Fn(&mut Events)) -> Stats {
    let mut ev = events_of(&require(&fx.out.join("ev.csv")), DEV);
    tweak(&mut ev);
    let path = require(&fx.out.join("taps/dev04.tap"));
    let len = std::fs::metadata(&path).unwrap().len() as usize;
    assert_eq!(len, SONG_BLOCKS * 8 * B * 4, "{}: not the whole-song tap (78,292 blocks)", path.display());
    let mut f = BufReader::with_capacity(1 << 20, std::fs::File::open(path).unwrap());
    replay_parity("song device 4", SONG_REL, fx.init, SONG_BLOCKS, &mut f, &ev, false)
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
    let ev = events_of(&require(&fx.out.join("ev.csv")), DEV);
    let notes = ev.values().flatten().filter(|e| matches!(e, Ev::On(..))).count();
    assert!(notes > 0, "no note-on recorded for device {DEV}");
    let s = song(&fx, &|_| {});
    report("song device 4 (whole song)", &s);
    assert_eq!(s.samples, SONG_BLOCKS * B * 2);
    assert!(s.nonzero > 1_000_000, "the whole-song tap is too quiet to mean anything: {} nonzero", s.nonzero);
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
        if name == "vec08" {
            assert!(s.refused > 0, "vec08: 300 note-ons in one block, 256 slots");
            assert_eq!(s.max_voices, 16);
        }
        if name == "vec05" {
            assert_eq!(s.max_voices, 16, "vec05: 17 simultaneous notes");
        }
    }
    // kill_b1: a one-sample first sub-block frees the voice before it is heard
    for q in &quiet {
        assert_eq!(q, "kill_b1", "{q}: unexpectedly silent reference");
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

/// Vectors the parameter negative controls run on.
const CONTROL_VECTORS: [&str; 3] = ["vec01", "vec02", "vec13"];

#[test]
fn negative_control_flipped_reference_bit_is_caught() {
    let Some(fx) = fixtures() else { return };
    let n = blocks_of(&fx, "vec02");
    let (ev, mut tap) = vector(&fx, "vec02", n);
    let blk = (0..n).find(|b| tap[(b * 8 + 4) * B * 4..(b * 8 + 5) * B * 4].iter().any(|&x| x != 0)).unwrap();
    let i = (0..B).find(|&i| tap[((blk * 8 + 4) * B + i) * 4..][..4] != [0; 4]).unwrap();
    tap[((blk * 8 + 4) * B + i) * 4 + 3] ^= 0x40;
    must_mismatch("flipped bit", || {
        replay_parity("flip", VEC_REL, fx.init, blk + 2, &mut &tap[..], &ev, false);
    });
}

#[test]
fn negative_control_parameter_nudges_are_caught_and_p4_is_not() {
    let Some(fx) = fixtures() else { return };
    let fixtures: Vec<(&str, usize, Events, Vec<u8>)> = CONTROL_VECTORS
        .iter()
        .map(|&name| {
            let n = blocks_of(&fx, name);
            let (ev, tap) = vector(&fx, name, n);
            (name, n, ev, tap)
        })
        .collect();
    for i in (0..PARAMS).filter(|&i| i != 4) {
        let smallest = [1.0 / 255.0, 0.02, 0.05, 0.1].into_iter().find(|&by| {
            fixtures.iter().any(|(name, n, ev0, tap)| {
                let mut ev = ev0.clone();
                nudge(i, by)(name, &mut ev);
                std::panic::catch_unwind(|| replay_parity("nudge", VEC_REL, fx.init, *n, &mut &tap[..], &ev, false)).is_err()
            })
        });
        eprintln!("P{i}: smallest detected nudge {smallest:?}");
        assert!(smallest.is_some(), "a 0.1 nudge of P{i} was not detected on any control vector");
    }
    for (name, n, ev0, tap) in &fixtures {
        let mut ev = ev0.clone();
        nudge(4, 0.1)(name, &mut ev);
        replay_parity("inert", VEC_REL, fx.init, *n, &mut &tap[..], &ev, false); // group 3 is never rendered: must still match
    }
}

#[test]
fn negative_control_scheduler_and_velocity_mutations_are_caught() {
    let Some(fx) = fixtures() else { return };
    let n = blocks_of(&fx, "vec05");
    let (ev, tap) = vector(&fx, "vec05", n);
    let map = |f: &dyn Fn(Ev) -> Ev| -> Events { ev.iter().map(|(b, v)| (*b, v.iter().map(|e| f(*e)).collect())).collect() };
    // velocity ignored (every note at 127)
    let no_vel = map(&|e| if let Ev::On(n, _, o) = e { Ev::On(n, 127, o) } else { e });
    must_mismatch("velocity ignored", || {
        replay_parity("no-velocity", VEC_REL, fx.init, n, &mut &tap[..], &no_vel, false);
    });
    // offsets dropped (every event fires at sample 0 of its block), on vec07 whose notes sit at mid-block offsets
    let n7 = blocks_of(&fx, "vec07");
    let (ev7, tap7) = vector(&fx, "vec07", n7);
    let no_offsets: Events = ev7
        .iter()
        .map(|(b, v)| {
            let v = v
                .iter()
                .map(|e| match *e {
                    Ev::On(n, v, _) => Ev::On(n, v, 0),
                    Ev::Off(n, _) => Ev::Off(n, 0),
                    p => p,
                })
                .collect();
            (*b, v)
        })
        .collect();
    must_mismatch("offsets dropped", || {
        replay_parity("no-offsets", VEC_REL, fx.init, n7, &mut &tap7[..], &no_offsets, false);
    });
    // note-offs are inaudible in the original too: dropping them still matches... except that they split sub-blocks, which
    // the level follower sees, so only the notes' own sub-blocks decide; vec05's note-offs fall inside long sub-blocks
    let no_off: Events = ev.iter().map(|(b, v)| (*b, v.iter().copied().filter(|e| !matches!(e, Ev::Off(..))).collect())).collect();
    replay_parity("no-offs", VEC_REL, fx.init, n, &mut &tap[..], &no_off, false);
    // the 17th simultaneous note was dropped by the original: removing it changes nothing, removing the 16th does
    let drop_note = |note: u8| -> Events {
        ev.iter().map(|(b, v)| (*b, v.iter().copied().filter(|e| !(*b == 0 && matches!(e, Ev::On(n, _, _) if *n == note))).collect())).collect()
    };
    replay_parity("17th note removed", VEC_REL, fx.init, n, &mut &tap[..], &drop_note(30 + 16), false);
    must_mismatch("16th note removed", || {
        replay_parity("16th note removed", VEC_REL, fx.init, n, &mut &tap[..], &drop_note(30 + 15), false);
    });
}

#[test]
fn negative_control_a_changed_note_or_an_injected_automation_is_caught() {
    let Some(fx) = fixtures() else { return };
    let n = blocks_of(&fx, "vec11");
    let (ev, tap) = vector(&fx, "vec11", n);
    let shifted: Events = ev.iter().map(|(b, v)| (*b, v.iter().map(|e| if let Ev::On(k, v, o) = *e { Ev::On((k + 1).min(127), v, o) } else { *e }).collect())).collect();
    must_mismatch("notes shifted by one", || {
        replay_parity("shifted", VEC_REL, fx.init, n, &mut &tap[..], &shifted, false);
    });
    // song automation dropped / an automation write that was never there
    let ev = events_of(&require(&fx.out.join("ev.csv")), DEV);
    let mut with_auto = ev.clone();
    with_auto.entry(20_000).or_default().push(Ev::Param(0, 0.5));
    must_mismatch("injected automation", || {
        let path = require(&fx.out.join("taps/dev04.tap"));
        let mut f = BufReader::with_capacity(1 << 20, std::fs::File::open(path).unwrap());
        replay_parity("injected", SONG_REL, fx.init, SONG_BLOCKS, &mut f, &with_auto, false);
    });
}

// ------------------------------------------------------------------ the original above the velocity ceiling

/// The original's stereo output (per block L then R) of a tap.
fn tap_out(tap: &[u8]) -> Vec<f32> {
    (0..tap.len() / (8 * B * 4)).flat_map(|b| f32s(&tap[(b * 8 + 4) * B * 4..(b * 8 + 6) * B * 4])).collect()
}

#[test]
fn above_the_velocity_ceiling_the_original_keeps_growing_and_the_crate_refuses() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    let rms = |o: &[f32]| (o.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>() / o.len() as f64).sqrt();
    // blocks 2900.. of 3000 vs blocks 100..200: a decaying voice is far quieter at the end; these two are not
    let win = |o: &[f32], a: usize, b: usize| -> f64 { rms(&o[a * 2 * B..b * 2 * B]) };
    for (name, grows_by) in [("unsafe_v214", 1.0), ("unsafe_v255", 1e3)] {
        let n = blocks_of(&fx, name);
        let (ev, tap) = vector(&fx, name, n);
        let out = tap_out(&tap);
        let (early, late) = (win(&out, 100, 200), win(&out, 2900, 3000));
        eprintln!("{name}: rms blocks 100..200 {early:.4e}, blocks 2900..3000 {late:.4e}");
        assert!(out.iter().all(|x| x.is_finite()), "{name}: the original's output is finite over this window");
        assert!(late >= grows_by * early, "{name}: the original's output must not decay (early {early:e}, late {late:e})");
        // the crate refuses the very note that does this
        let mut d = Box::new(Id20::new(&fx.init).unwrap());
        let on = ev.values().flatten().find_map(|e| if let Ev::On(n, v, o) = e { Some((*n, *v, *o)) } else { None }).unwrap();
        assert!(on.1 > MAX_VELOCITY, "{name}: the vector's note is above the ceiling");
        assert_eq!(d.note_on(on.0, on.1, on.2), Err(Error::Velocity));
    }
    // the allowed neighbour decays (its own tap: velocity 213 at P13 = 1, the slowest envelope the crate accepts)
    let n = blocks_of(&fx, "slow213");
    let (_, tap) = vector(&fx, "slow213", n);
    let out = tap_out(&tap);
    assert!(out.iter().all(|x| x.is_finite() && x.abs() < 1e4));
    assert!(win(&out, 2900, 3000) < win(&out, 100, 200), "velocity 213 must decay");
}

// ------------------------------------------------------------------ the level-follower kills are the original's

#[test]
fn the_original_frees_a_young_voice_at_the_end_of_a_short_sub_block() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this does not prove parity)");
        return;
    };
    // kill_a: twelve notes 10 blocks apart, the first at offset 127 (a 1-sample first sub-block): silent in the original
    let n = blocks_of(&fx, "kill_a");
    let (_, tap) = vector(&fx, "kill_a", n);
    let out = tap_out(&tap);
    let block_peak = |b: usize| out[b * 2 * B..(b + 1) * 2 * B].iter().fold(0.0f32, |m, x| m.max(x.abs()));
    assert_eq!(block_peak(0), 0.0, "the offset-127 note is freed before it is heard");
    assert!(block_peak(10) > 0.0 && block_peak(11) > 0.0, "the offset-126 note (2 samples) is heard");
    // the implementation agrees voice for voice (kill_a / kill_b* / kill_c are part of the vector run) and a mutated offset
    // changes the outcome, so the kill is not an accident of the parity bound
    let (ev, tap) = vector(&fx, "kill_a", n);
    let shifted: Events = ev.iter().map(|(b, v)| (*b, v.iter().map(|e| if let Ev::On(k, v, o) = *e { Ev::On(k, v, o.saturating_sub(1)) } else { *e }).collect())).collect();
    replay_parity("kill_a", VEC_REL, fx.init, n, &mut &tap[..], &ev, false);
    must_mismatch("kill_a with the first note one sample earlier", || {
        replay_parity("kill_a shifted", VEC_REL, fx.init, n, &mut &tap[..], &shifted, false);
    });
}

// ------------------------------------------------------------------ always-on hardening of the harness itself

#[test]
fn missing_fixtures_are_a_hard_error() {
    let p = std::env::temp_dir().join("glorp-id20-definitely-missing.csv");
    let err = std::panic::catch_unwind(|| require(&p)).err().expect("require must panic");
    let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("fixture") && msg.contains("glorp-id20-definitely-missing.csv"), "{msg}");
}

#[test]
fn chunk_parser_refuses_a_song_without_exactly_the_one_id20_device() {
    let build = |ids: &[u8]| {
        let mut blob = vec![0u8; 20];
        blob[16..20].copy_from_slice(&(ids.len() as u32).to_le_bytes());
        for &id in ids {
            blob.push(id);
            blob.extend_from_slice(&68u32.to_le_bytes());
            blob.extend_from_slice(&[0u8; 68]);
        }
        blob
    };
    assert!(chunks(&build(&[7, 7])).unwrap_err().contains("found 0"));
    assert!(chunks(&build(&[1, 1, 1, 1, 20, 20])).unwrap_err().contains("found 2"));
    assert!(chunks(&build(&[1, 1, 1, 20])).unwrap_err().contains("(["), "ID 20 at the wrong device index");
    assert!(chunks(&build(&[1, 1, 1, 1, 20])).is_ok());
    let mut wrong = build(&[1, 1, 1, 1, 20]);
    let at = wrong.len() - 68 - 4;
    wrong[at..at + 4].copy_from_slice(&64u32.to_le_bytes());
    assert!(chunks(&wrong).unwrap_err().contains("64 bytes"));
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
    let p = std::env::temp_dir().join("glorp-id20-bad-events.csv");
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,4,20,boom,1,2,3\n").unwrap();
    let err = std::panic::catch_unwind(|| events_of(&p, 4)).err().unwrap();
    assert!(err.downcast_ref::<String>().cloned().unwrap_or_default().contains("unknown event kind"));
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,4,20,on,1,2\n").unwrap();
    assert!(std::panic::catch_unwind(|| events_of(&p, 4)).is_err());
    std::fs::write(&p, "block,dev,id,kind,a,b,c\n0,4,20,on,60,99,12\n3,5,20,on,1,1,1\n1,4,20,off,60,7,0\n2,4,20,param,3,0.5,1056964608\n").unwrap();
    let m = events_of(&p, 4);
    assert_eq!(m[&0], vec![Ev::On(60, 99, 12)]);
    assert_eq!(m[&1], vec![Ev::Off(60, 7)]);
    assert_eq!(m[&2], vec![Ev::Param(3, 0.5)]);
    assert!(!m.contains_key(&3), "other devices' rows are ignored");
}
