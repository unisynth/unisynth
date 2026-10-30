//! Optional external-reference parity: ID38 vs the original engine's recorded per-device taps (reference harness output, local only: third-party
//! binary + song never enter git). Skipped unless `GLORP_REF_ROOT` points at the `ws-intros/out` directory that holds
//! `389811-2.bin` (the song blob, read only for its ID 38 initial parameters) and
//! `glorp-win64/analysis/harness/out/{ev_full.csv,tapspec.txt,taps/devNN.tap}`.
//!
//! Tap block = 8 × 128 f32: in L, in R, aux L, aux R, then out L, out R, aux L, aux R. The harness replays each
//! instance from its first non-silent block, so filter state starts at zero. Every recorded parameter write of the
//! window (and before it) is applied through `set_param`, so the test also proves the whole recorded song envelope is
//! inside the accepted subset.
use glorp::id38::{Id38, PARAMS};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

const B: usize = 128;

struct Stats {
    samples: usize,
    exact: usize,
    max_abs: f64,
    nonzero: usize,
    dist_samples: usize,
    dist_exact: usize,
}

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}

/// Initial parameters of every ID 38 song device: device index -> 15 floats.
fn chunks(blob: &[u8]) -> Vec<(usize, [f32; PARAMS])> {
    let u32at = |p: usize| u32::from_le_bytes(blob[p..p + 4].try_into().unwrap()) as usize;
    let n = u32at(16);
    let mut p = 20;
    let mut out = Vec::new();
    for i in 0..n {
        let (id, size) = (blob[p], u32at(p + 1));
        let chunk = &blob[p + 5..p + 5 + size];
        p += 5 + size;
        if id == 38 {
            assert_eq!(size, 64);
            let v = f32s(chunk);
            out.push((i, v[..PARAMS].try_into().unwrap()));
        }
    }
    out
}

/// Replays one instance against its recorded tap.
fn replay(
    dev: usize,
    init: [f32; PARAMS],
    start: usize,
    nblocks: usize,
    tap: &[u8],
    events: &HashMap<usize, Vec<(usize, f32)>>,
) -> Stats {
    let mut p = init;
    let mut d: Option<Id38> = None;
    let mut s = Stats { samples: 0, exact: 0, max_abs: 0.0, nonzero: 0, dist_samples: 0, dist_exact: 0 };
    for b in 0..start + nblocks {
        // the device keeps its construction-time chunk until the first event; mirror param writes into `p`
        for &(i, v) in events.get(&b).map(|v| v.as_slice()).unwrap_or(&[]) {
            p[i] = v;
            if let Some(d) = d.as_mut() {
                d.set_param(i, v).unwrap_or_else(|e| panic!("dev {dev} block {b}: param {i}={v} rejected: {e:?}"));
            }
        }
        if b < start {
            continue;
        }
        let dev_ref = d.get_or_insert_with(|| Id38::new(&p).unwrap_or_else(|e| panic!("dev {dev}: {e:?}")));
        let o = (b - start) * 8 * B * 4;
        let blk = f32s(&tap[o..o + 8 * B * 4]);
        let (mut l, mut r) = (blk[..B].to_vec(), blk[B..2 * B].to_vec());
        dev_ref.process(&mut l, &mut r);
        let dist = (p[6] * 5.999) as u32 > 0;
        for (c, got) in [l, r].iter().enumerate() {
            for i in 0..B {
                let want = blk[(4 + c) * B + i];
                let e = (got[i] as f64 - want as f64).abs();
                s.samples += 1;
                s.max_abs = s.max_abs.max(e);
                s.nonzero += (want != 0.0) as usize;
                s.exact += (got[i] == want) as usize;
                if dist {
                    s.dist_samples += 1;
                    s.dist_exact += (got[i] == want) as usize;
                }
                // 4 ulp-ish at unity plus relative slack; non-distorting windows are asserted bit-exact below
                assert!(
                    e <= 1e-6 + 4e-7 * want.abs() as f64,
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
fn id38_matches_recorded_original_taps() {
    let Some((dir, blob)) = fixture() else {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        return;
    };
    let specs = specs(&dir);
    let mut checked = 0;
    for (dev, init) in chunks(&blob) {
        let Some(&(start, n)) = specs.get(&dev) else { continue }; // never-active instance: no tap
        let tap = std::fs::read(dir.join(format!("taps/dev{dev:02}.tap"))).unwrap();
        let ev = events_of(&dir.join("ev_full.csv"), dev);
        let s = replay(dev, init, start, n, &tap, &ev);
        eprintln!(
            "dev {dev}: {} samples, {} exact ({} nonzero refs), max abs err {:.3e}, dist-window exact {}/{}",
            s.samples, s.exact, s.nonzero, s.max_abs, s.dist_exact, s.dist_samples
        );
        assert!(s.nonzero > 100, "dev {dev}: window too quiet to mean anything");
        // windows without distortion: the biquad path is expected bit-exact
        if s.dist_samples == 0 {
            assert_eq!(s.exact, s.samples, "dev {dev}: non-distorting window not bit-exact");
        }
        checked += 1;
    }
    assert!(checked >= 9, "expected the song's active ID 38 instances, got {checked}");
}

#[test]
#[should_panic(expected = "got")]
fn negative_control_flipped_reference_bit_is_caught() {
    let Some((dir, blob)) = fixture() else { panic!("got: skipped (no fixture)") };
    let specs = specs(&dir);
    let (dev, init) = chunks(&blob).into_iter().find(|(d, _)| specs.contains_key(d)).unwrap();
    let (start, _) = specs[&dev];
    let tap = std::fs::read(dir.join(format!("taps/dev{dev:02}.tap"))).unwrap();
    let ev = events_of(&dir.join("ev_full.csv"), dev);
    // flip one exponent bit of an expected sample: far above tolerance, must be reported
    let mut tap = tap;
    let o = (1 * 8 * B + 4 * B + 5) * 4 + 3; // block 1, out L, sample 5, exponent byte
    tap[o] ^= 0x40;
    replay(dev, init, start, 3, &tap, &ev);
}
