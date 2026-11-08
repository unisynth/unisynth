# crates/unisynth (sidecar: full intro, features, checks)

Pull-only (not auto-injected). Moved from the directory AGENTS.md.

- MIT no_std f32 unified synth (change add-unified-synth, archived): one slot-based instrument (`synth-mix` kind 64) and one merged effect set (kinds 65–73) built from the WaveSabre and Tunefish algorithms (plus V2 and mini-cauldron modes); checked against the original engines in `synth-convert`'s tests (this crate does not depend on them).
- WaveSabre-derived code keeps the upstream notice (`LICENSE-WaveSabre` here and in `crates/wavesabre`).
- Depends on `synth-core`, `libm`, optional `gsm` (`us-sample-gsm`) and `dls` (`us-sample-dls`).
- Every module type is a cargo feature `us-*` (default `us-all`); a patch that needs a disabled module fails at load with `LoadError::UnsupportedModule(module id)`.
- Feature `std` = host-side only (chunk encoder, names, law inverses, `needs`, `migrate`; never in the player).
- Toolchain: `PATH="$HOME/.cargo/bin:$PATH"`.
- Checks: `cargo check -p unisynth --target i686-pc-windows-msvc -Zbuild-std=core`, `--target wasm32-unknown-unknown`.
- Docs: `docs/synth/unisynth.md`; design: `openspec/changes/archive/2026-10-26-add-unified-synth/design.md`.
- Features: `tools/check-unisynth-features.sh [--no-test]` builds every single-module set (none, each `us-*` module, groups, all) for i686 core-only and wasm32, runs the host tests of each set, fails on any warning. Tests that load default patches need `us-env-seconds`; the instrument tests need both envelope modes.
