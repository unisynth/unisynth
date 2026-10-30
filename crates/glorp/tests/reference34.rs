//! Optional external-reference parity: ID34 (Hilbert frequency shifter) vs the original engine's recorded per-device
//! taps (reference harness output, local only: third-party binary + song never enter git). Skipped unless
//! `GLORP_REF_ROOT` points at the `ws-intros/out` directory that holds `389811-2.bin` (read only for the ID 34
//! initial parameters) and `glorp-win64/analysis/harness/out/{ev_full.csv,tapspec.txt,taps/devNN.tap}`.
//!
//! Tap block = 8 × 128 f32: in L, in R, aux L, aux R, then out L, out R, aux L, aux R. The harness replays a fresh
//! instance from block 0 (silent input before the window), as the original runs every device every block, and the
//! shifter phase integrates through that silence. Every recorded parameter write is applied through `set_param`
//! before the block it is logged in, so the test also proves the recorded song automation is accepted. `GLORP_REF_ROOT` unset => the tests below return early (they do not prove parity).
use glorp::id34::{Id34, PARAMS};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

const B: usize = 128;

struct Stats {
    samples: usize,
    exact: usize,
    max_abs: f64,
    max_ref: f64,
    nonzero: usize,
}

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}

/// Initial parameters of every ID 34 song device: device index -> 4 floats (the 5th chunk float is the byte size).
fn chunks(blob: &[u8]) -> Vec<(usize, [f32; PARAMS])> {
    let u32at = |p: usize| u32::from_le_bytes(blob[p..p + 4].try_into().unwrap()) as usize;
    let n = u32at(16);
    let mut p = 20;
    let mut out = Vec::new();
    for i in 0..n {
        let (id, size) = (blob[p], u32at(p + 1));
        let chunk = &blob[p + 5..p + 5 + size];
        p += 5 + size;
        if id == 34 {
            assert_eq!(size, 20);
            out.push((i, f32s(chunk)[..PARAMS].try_into().unwrap()));
        }
    }
    out
}

fn replay(
    dev: usize,
    init: [f32; PARAMS],
    start: usize,
    nblocks: usize,
    tap: &[u8],
    events: &HashMap<usize, Vec<(usize, f32)>>,
) -> Stats {
    // The original runs every device on every block from block 0 (silent input before the tap window) and the
    // shifter's phase keeps accumulating through the silence, so the replay does too.
    let mut d = Id34::new(&init).unwrap_or_else(|e| panic!("dev {dev}: {e:?}"));
    let mut s = Stats { samples: 0, exact: 0, max_abs: 0.0, max_ref: 0.0, nonzero: 0 };
    let silence = (vec![0.0f32; B], vec![0.0f32; B]);
    for b in 0..start + nblocks {
        for &(i, v) in events.get(&b).map(|v| v.as_slice()).unwrap_or(&[]) {
            d.set_param(i, v).unwrap_or_else(|e| panic!("dev {dev} block {b}: param {i}={v} rejected: {e:?}"));
        }
        if b < start {
            let (mut l, mut r) = silence.clone();
            d.process(&mut l, &mut r);
            continue;
        }
        let dev_ref = &mut d;
        let o = (b - start) * 8 * B * 4;
        let blk = f32s(&tap[o..o + 8 * B * 4]);
        let (mut l, mut r) = (blk[..B].to_vec(), blk[B..2 * B].to_vec());
        dev_ref.process(&mut l, &mut r);
        for (c, got) in [l, r].iter().enumerate() {
            for i in 0..B {
                let want = blk[(4 + c) * B + i];
                let e = (got[i] as f64 - want as f64).abs();
                s.samples += 1;
                s.max_abs = s.max_abs.max(e);
                s.max_ref = s.max_ref.max(want.abs() as f64);
                s.nonzero += (want != 0.0) as usize;
                s.exact += (got[i] == want) as usize;
                // ≤ ~2 ulp: the only known difference is the sine-table node 305, which libm rounds one ulp apart
                // from the original's sinf (patching that one node makes both instances bit-exact)
                assert!(
                    e <= 3e-7 + 3e-7 * want.abs() as f64,
                    "dev {dev} block {b} ch {c} i {i}: got {} want {want}",
                    got[i]
                );
            }
        }
    }
    s
}

fn fixture() -> Option<(PathBuf, Vec<u8>)> {
    let root = PathBuf::from(std::env::var_os("GLORP_REF_ROOT")?);
    let blob = std::fs::read(root.join("389811-2.bin")).ok()?;
    Some((root.join("glorp-win64/analysis/harness/out"), blob))
}

fn events_of(path: &std::path::Path, dev: usize) -> HashMap<usize, Vec<(usize, f32)>> {
    let mut m: HashMap<usize, Vec<(usize, f32)>> = HashMap::new();
    for line in BufReader::new(std::fs::File::open(path).unwrap()).lines() {
        let line = line.unwrap();
        let f: Vec<&str> = line.split(',').collect();
        if f.len() == 7 && f[3] == "param" && f[1].parse() == Ok(dev) {
            let bits: u32 = f[6].parse().unwrap();
            m.entry(f[0].parse().unwrap()).or_default().push((f[4].parse().unwrap(), f32::from_bits(bits)));
        }
    }
    m
}

fn specs(dir: &std::path::Path) -> HashMap<usize, (usize, usize)> {
    std::fs::read_to_string(dir.join("tapspec.txt"))
        .unwrap()
        .lines()
        .filter_map(|l| {
            let a: Vec<usize> = l.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            (a.len() == 3).then(|| a[2].checked_sub(a[1]).map(|n| (a[0], (a[1], n)))).flatten()
        })
        .collect()
}

#[test]
fn id34_matches_recorded_original_taps() {
    let Some((dir, blob)) = fixture() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    let specs = specs(&dir);
    let mut checked = 0;
    for (dev, init) in chunks(&blob) {
        let Some(&(start, n)) = specs.get(&dev) else { continue };
        let tap = std::fs::read(dir.join(format!("taps/dev{dev:02}.tap"))).unwrap();
        let ev = events_of(&dir.join("ev_full.csv"), dev);
        let s = replay(dev, init, start, n, &tap, &ev);
        eprintln!(
            "dev {dev}: {} samples, {} exact ({} nonzero refs), max |ref| {:.4}, max abs err {:.3e}",
            s.samples, s.exact, s.nonzero, s.max_ref, s.max_abs
        );
        assert!(s.nonzero > 10_000, "dev {dev}: window too quiet to mean anything");
        checked += 1;
    }
    assert_eq!(checked, 2, "the song has two active ID 34 instances (25, 26)");
}

#[test]
#[should_panic(expected = "got")]
fn negative_control_flipped_reference_bit_is_caught() {
    let Some((dir, blob)) = fixture() else { panic!("got: skipped (no fixture)") };
    let specs = specs(&dir);
    let (dev, init) = chunks(&blob).into_iter().find(|(d, _)| specs.contains_key(d)).unwrap();
    let (start, _) = specs[&dev];
    let mut tap = std::fs::read(dir.join(format!("taps/dev{dev:02}.tap"))).unwrap();
    let ev = events_of(&dir.join("ev_full.csv"), dev);
    // first block that carries signal: flip an exponent bit of an expected output sample
    let blk = (0..600).find(|b| tap[b * 8 * B * 4..b * 8 * B * 4 + B * 4].iter().any(|&x| x != 0)).unwrap();
    let o = (blk * 8 * B + 4 * B + 5) * 4 + 3;
    tap[o] ^= 0x40;
    replay(dev, init, start, blk + 2, &tap, &ev);
}
