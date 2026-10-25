// Writes the probe's song blob from the shared synthetic songs (crates/wavesabre/tests/songs.rs): no binary assets.
#[path = "../../crates/wavesabre/tests/songs.rs"]
#[allow(dead_code)]
mod songs;

fn main() {
    let blob = if std::env::var("CARGO_FEATURE_ALL").is_ok() {
        songs::showcase()
    } else {
        songs::all().into_iter().find(|(n, _)| *n == "slaughter_automation").unwrap().1
    };
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("song.bin");
    std::fs::write(out, blob).unwrap();
    println!("cargo:rerun-if-changed=../../crates/wavesabre/tests/songs.rs");
}
