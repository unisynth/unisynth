// Copies the unified song named by `US_SONG` into the probe (no binary assets in the repository), and writes it as Rust
// (`synth-convert rust`) for the `baked` feature.
fn main() {
    let path = std::env::var("US_SONG").expect("US_SONG=<unified.song> names the song the probe renders");
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).to_path_buf();
    let blob = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    std::fs::write(out.join("song.bin"), &blob).unwrap();
    if std::env::var_os("CARGO_FEATURE_BAKED").is_some() {
        let src = synth_convert::rust::generate(&blob, None, &path).unwrap_or_else(|e| panic!("{path}: {e}"));
        std::fs::write(out.join("song.rs"), src).unwrap();
    }
    println!("cargo:rerun-if-env-changed=US_SONG");
    println!("cargo:rerun-if-changed={path}");
}
