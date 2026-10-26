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
