// Writes the probe's song blob from the shared synthetic songs (crates/wavesabre/tests/songs.rs): no binary assets.
#[path = "../../crates/wavesabre/tests/songs.rs"]
#[allow(dead_code)]
mod songs;

fn main() {
    // WS_SONG=<file>: embed a real song (research/size-compare) instead of a synthetic one.
    println!("cargo:rerun-if-env-changed=WS_SONG");
    let blob = if let Ok(f) = std::env::var("WS_SONG") {
        std::fs::read(f).unwrap()
    } else if std::env::var("CARGO_FEATURE_ALL").is_ok() {
        songs::showcase()
    } else {
        songs::all().into_iter().find(|(n, _)| *n == "slaughter_automation").unwrap().1
    };
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("song.bin");
    std::fs::write(out, blob).unwrap();
    println!("cargo:rerun-if-changed=../../crates/wavesabre/tests/songs.rs");
}
