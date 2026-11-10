//! The three converted TBL songs stay byte-identical (change add-plain-xm-import, task 1.1): the release conversion
//! (`--thin 2`, as `make_demos.py`) and the layered one (`--keep-layers --split-pitches --exact-chain`, the parity's),
//! each song and the first 30 s of its unified render, against `data/tbl_identity.json`. Opt-in: skips when
//! `research/tbl-mini-cauldron/out/listen` (made by `run_all.sh`) is absent. `TBL_IDENTITY_RECORD=1` rewrites the file.
use std::collections::BTreeMap;
use std::path::Path;
use synth_core::mix::{Env, Player, Song};
use synth_core::pool::Pool;
use unisynth::device::UDevice;
use unisynth::rng::Xorshift;

const SONGS: [(&str, &str); 3] = [("jizz", "patches_jizz.json"), ("stash_song1", "patches_stash.json"), ("stash_bluish", "patches_stash.json")];
const LAYOUTS: [(&str, &[&str]); 2] = [("release", &["--thin", "2"]), ("layered", &["--keep-layers", "--split-pitches", "--exact-chain"])];

fn fnv(bytes: &[u8]) -> String {
    let h = bytes.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    format!("{h:016x}")
}

fn render(blob: &[u8], frames: usize) -> Vec<u8> {
    let s = Song::parse(blob).expect("a valid song");
    let env = Env::for_song(&s, None);
    let need = Player::<UDevice, Xorshift>::required_pool(&s, &env, 100).expect("every kind is unified");
    let mut mem = vec![0u8; need];
    let mut pool = Pool::new(&mut mem);
    let mut p: Player<UDevice, Xorshift> = Player::new(s, &env, &mut pool, Xorshift::new(1), 100).expect("loads");
    let mut out = vec![0.0f32; 2 * frames];
    p.render_f32(&mut out);
    out.iter().flat_map(|v| v.to_le_bytes()).collect()
}

#[test]
fn the_tbl_conversions_stay_byte_identical() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../research/tbl-mini-cauldron");
    if !root.join("out/listen/jizz.xm").exists() {
        eprintln!("skip: {} absent", root.join("out/listen").display());
        return;
    }
    let dir = std::env::temp_dir().join(format!("tbl-identity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut got = BTreeMap::new();
    for (name, patches) in SONGS {
        for (layout, opts) in LAYOUTS {
            let out = dir.join(format!("{name}.{layout}.song"));
            let o = std::process::Command::new(env!("CARGO_BIN_EXE_synth-convert"))
                .arg("xm").arg(root.join(format!("out/listen/{name}.xm"))).arg(root.join("ref").join(patches)).arg(&out).args(opts)
                .output().unwrap();
            assert!(o.status.success(), "{name} {layout}: {}", String::from_utf8_lossy(&o.stderr));
            let blob = std::fs::read(&out).unwrap();
            got.insert(format!("{name}.{layout}.song"), fnv(&blob));
            got.insert(format!("{name}.{layout}.render30s"), fnv(&render(&blob, 30 * 44100)));
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/tbl_identity.json");
    if std::env::var_os("TBL_IDENTITY_RECORD").is_some() {
        std::fs::write(&file, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
        return;
    }
    let want: BTreeMap<String, String> = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(got, want, "a TBL conversion or its render changed");
}
