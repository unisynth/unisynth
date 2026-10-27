// The song named by `US_SONG`, as a blob and (feature `baked`) as Rust from `synth-convert rust`.
fn main() {
    let path = std::env::var("US_SONG").expect("US_SONG=<unified.song>");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let blob = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    std::fs::write(out.join("song.bin"), &blob).unwrap();
    let src = if std::env::var_os("CARGO_FEATURE_BAKED").is_some() {
        synth_convert::rust::generate(&blob, None, &path).unwrap_or_else(|e| panic!("{path}: {e}"))
    } else {
        String::new()
    };
    std::fs::write(out.join("song.rs"), src).unwrap();
    println!("cargo:rerun-if-env-changed=US_SONG");
    println!("cargo:rerun-if-changed={path}");
}
