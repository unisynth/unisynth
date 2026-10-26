// Writes the probe's .tfm from the shared synthetic patches (crates/tfsynth/tests/patches.rs): no binary assets.
#[path = "../../crates/tfsynth/tests/patches.rs"]
#[allow(dead_code)]
mod patches;

use tfsynth::params::COUNT;

fn main() {
    // TF_SONG=<file>: embed a real .tfm (research/size-compare) instead of the synthetic one.
    println!("cargo:rerun-if-env-changed=TF_SONG");
    if let Ok(f) = std::env::var("TF_SONG") {
        let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("song.tfm");
        std::fs::copy(f, out).unwrap();
        return;
    }
    let mut ins: Vec<[f32; COUNT]> = patches::instrument_patches().into_iter().take(6).map(|(_, p)| p).collect();
    ins.extend(patches::effect_patches().into_iter().map(|(_, _, p)| p));
    let mut b = vec![];
    b.extend((ins.len() as u16).to_le_bytes());
    b.extend(120u16.to_le_bytes());
    let ev: Vec<(u16, u8, u8)> = (0..8u8).flat_map(|i| [(if i == 0 { 0 } else { 2 }, 48 + 3 * i, 100), (2, 48 + 3 * i, 0)]).collect();
    for _ in &ins {
        b.extend((ev.len() as u16).to_le_bytes());
    }
    b.extend(b"INST");
    for p in &ins {
        b.extend(p.iter().map(|v| (v * 100.0) as u8));
    }
    b.extend(b"SONG");
    for _ in &ins {
        ev.iter().for_each(|e| b.extend(e.0.to_le_bytes()));
        ev.iter().for_each(|e| b.push(e.1));
        ev.iter().for_each(|e| b.push(e.2));
    }
    b.extend(b"ENDS");
    std::fs::write(std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("song.tfm"), b).unwrap();
    println!("cargo:rerun-if-changed=../../crates/tfsynth/tests/patches.rs");
}
