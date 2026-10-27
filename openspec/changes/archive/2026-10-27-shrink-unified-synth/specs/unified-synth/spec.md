# Spec Delta

## ADDED Requirements

### Requirement: Module code is removed with its feature
A build without a module's cargo feature SHALL NOT contain that module's code (in addition to rejecting patches that use
it at load): the compiled player for a song needs no more than the modules `synth-convert features` lists for it.

#### Scenario: Module code is absent
- **WHEN** a song that uses only the noise source and the state-variable filter is built with exactly its listed features
- **THEN** the build holds no phase-mod, saw/pulse, wavetable, sampler, ladder, RBJ or notch code (checked by symbol listing or size against the all-modules build)

#### Scenario: Explained map
- **WHEN** `synth-convert features --explain` runs on a song
- **THEN** each listed feature is followed by what requires it (device, parameter or automation lane)

### Requirement: Optional voice paths are features derived from the song
The voice paths a song may not use (LFOs, glide/slide, mono-legato, unison above one voice, one-shot, the mod matrix, automation lanes) SHALL each be a
cargo feature. `synth-convert features` SHALL list a path's feature exactly when a device or an automation lane of the
song can use it, and a build without it SHALL reject a patch that uses it at load with `UnsupportedModule`.

#### Scenario: A path the song does not use is left out
- **WHEN** a song whose instruments read no LFO, have no glide, mono, unison or one-shot is built with exactly its listed features
- **THEN** the build holds no LFO, pitch-move, note-log, unison or one-shot code and plays the song bit for bit as the full build

#### Scenario: A path reachable by automation is listed
- **WHEN** an automation lane can raise the glide time, switch mono mode on, or route a matrix slot to an LFO
- **THEN** `synth-convert features` lists the matching `us-voice-*` feature (and `us-matrix` for a routing lane) and `--explain` names the lane

### Requirement: A song can be compiled into the build as Rust
`synth-convert rust` SHALL write a unified song as Rust source that a compo build embeds instead of the blob. It SHALL
load the song with the player on the host first and refuse a song the player would refuse, SHALL keep the song's section
bytes unchanged, and the source SHALL fail to compile in a build that lacks a module the song needs. A build of a
generated song MAY leave out load-time validation (`us-trusted`); it SHALL render bit for bit what the blob renders.

#### Scenario: Same output as the blob
- **WHEN** a reference song is rendered from its blob with every module and from its generated Rust with exactly its features and `us-trusted`
- **THEN** the f32 output from frame 0 and the i16 output after seeks are identical, for sub-block sizes 100 and 77

#### Scenario: Missing module does not compile
- **WHEN** the generated Rust of a song that needs `us-voice-lfo` is built without that feature
- **THEN** compilation fails on the generated module assert

