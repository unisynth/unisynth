// Copies the unified song named by `US_SONG` into the probe (no binary assets in the repository).
fn main() {
    let path = std::env::var("US_SONG").expect("US_SONG=<unified.song> names the song the probe renders");
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("song.bin");
    std::fs::copy(&path, out).unwrap_or_else(|e| panic!("{path}: {e}"));
    println!("cargo:rerun-if-env-changed=US_SONG");
    println!("cargo:rerun-if-changed={path}");
}
