//! Optional external-reference check that separates ID35's *structure* from its *coefficients*: the all-pass cascade
//! (direct form II order, state kept per stage, mix) is run here with the stage coefficients the **original** computed
//! (dumped by the Wine harness after every block, `GLORP_DUMP_COEF`, see the research doc) instead of our own, and
//! must reproduce the original's recorded output bit for bit. The tolerance in `reference35.rs` is therefore purely the
//! cost of re-deriving the coefficients with a different C library (`sin`/`cos`/`pow` last-ulp differences).
//! Needs `GLORP_REF_ROOT` plus `id35vec/coefsong.bin` (song window) and `id35vec/coef05.bin` (vector 5) next to the
//! taps. `GLORP_REF_ROOT` unset => explicit skip; set but a dump missing => fail loudly. The all-pass loop below is a deliberate re-statement of
//! `id35::Id35::process` over externally supplied coefficients.
use std::collections::HashMap;
use std::path::PathBuf;

const B: usize = 128;
const REC: usize = 4 + 2 * 1024;

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}

/// Runs the stage loop with the dumped coefficients and returns (samples, exact, max abs err).
fn structure(dir: &std::path::Path, coef: &str, tap: &str, csv: &str, start: usize, nb: usize, flip: bool) -> (usize, usize, f64) {
    let cb = std::fs::read(dir.join(coef)).unwrap();
    let tap = std::fs::read(dir.join(tap)).unwrap();
    // only P2 (mix) is read from the event log: the coefficients and the stage count come from the dump
    let mut mix = 0.0f32;
    let mut ev: HashMap<usize, Vec<f32>> = HashMap::new();
    for l in std::fs::read_to_string(dir.join(csv)).unwrap().lines().skip(1) {
        let f: Vec<&str> = l.split(',').collect();
        if f[1] == "27" && f[3] == "param" && f[4] == "2" {
            ev.entry(f[0].parse().unwrap()).or_default().push(f32::from_bits(f[6].parse().unwrap()));
        }
    }
    let mut st = vec![[0.0f32; 4]; 256];
    let (mut tot, mut exact, mut maxe) = (0, 0, 0.0f64);
    for b in 0..start + nb {
        if let Some(v) = ev.get(&b) {
            mix = *v.last().unwrap();
        }
        if b < start {
            continue;
        }
        let o = b * REC;
        let n = u32::from_le_bytes(cb[o..o + 4].try_into().unwrap()) as usize;
        let mut c = f32s(&cb[o + 4..o + REC]);
        if flip && n > 0 {
            c[0] = f32::from_bits(c[0].to_bits() ^ 1); // one ulp in one coefficient
        }
        let bo = (b - start) * 8 * B * 4;
        let blk = f32s(&tap[bo..bo + 8 * B * 4]);
        for i in 0..B {
            let dry = [blk[i], blk[B + i]];
            let mut v = dry;
            for k in 0..n {
                let (c1, c2) = (c[k], c[256 + k]);
                let s = &mut st[k];
                let (u, vv) = ([s[0], s[1]], [s[2], s[3]]);
                let mut w = [0.0; 2];
                for ch in 0..2 {
                    let t = c1 * u[ch];
                    w[ch] = v[ch] - ((c2 * vv[ch]) + t);
                    v[ch] = (t + vv[ch]) + c2 * w[ch];
                }
                *s = [w[0], w[1], u[0], u[1]];
            }
            let out = [(v[0] - dry[0]) * mix + dry[0], (v[1] - dry[1]) * mix + dry[1]];
            for ch in 0..2 {
                let want = blk[(4 + ch) * B + i];
                tot += 1;
                exact += (out[ch] == want) as usize;
                maxe = maxe.max((out[ch] as f64 - want as f64).abs());
            }
        }
    }
    (tot, exact, maxe)
}

fn dir() -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os("GLORP_REF_ROOT").or_else(|| {
        eprintln!("GLORP_REF_ROOT not set: skipped");
        None
    })?);
    let d = root.join("glorp-win64/analysis/harness/out");
    for f in ["id35vec/coefsong.bin", "id35vec/coef05.bin"] {
        assert!(d.join(f).is_file(), "GLORP_REF_ROOT is set but {} is missing", d.join(f).display());
    }
    Some(d)
}

#[test]
fn id35_structure_is_bit_exact_with_the_originals_coefficients() {
    let Some(d) = dir() else { return };
    let song = structure(&d, "id35vec/coefsong.bin", "taps/dev27.tap", "ev_full.csv", 7539, 600, false);
    let v5 = structure(&d, "id35vec/coef05.bin", "id35vec/vec05.tap", "id35vec/vec05.csv", 0, 150, false);
    eprintln!("song window: {} samples, {} exact, max abs err {:e}", song.0, song.1, song.2);
    eprintln!("vector 5:    {} samples, {} exact, max abs err {:e}", v5.0, v5.1, v5.2);
    assert_eq!((song.0, song.1), (153_600, 153_600));
    assert_eq!((v5.0, v5.1), (38_400, 38_400));
}

#[test]
fn negative_control_one_ulp_in_a_coefficient_breaks_exactness() {
    let Some(d) = dir() else { return };
    let (tot, exact, maxe) = structure(&d, "id35vec/coef05.bin", "id35vec/vec05.tap", "id35vec/vec05.csv", 0, 150, true);
    eprintln!("flipped coefficient: {exact}/{tot} exact, max abs err {maxe:e}");
    assert!(exact < tot && maxe > 0.0);
}
