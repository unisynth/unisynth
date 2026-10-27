# synth-threads Specification

## Purpose
Opt-in multi-threaded rendering of synth songs: independent tracks render on several cores with output bit-identical to
the single-thread render, and builds without the option carry no thread code.

## Requirements

### Requirement: Opt-in build option
Multi-threaded rendering SHALL be available only when the synth is built with the `threads` option. The option SHALL be
off by default, and no build of the intro player or the size probes SHALL enable it implicitly.

#### Scenario: Default build
- **WHEN** a synth crate is built without naming the `threads` option
- **THEN** only the single-thread render path exists

### Requirement: No thread code when the option is off
A build without the `threads` option SHALL contain no thread-related code, data, symbols or imports. Binaries built
without the option SHALL be byte-identical to binaries built from the same sources before the option existed.

#### Scenario: Size probe unchanged
- **WHEN** the i686 `no_std` size probes are built without the option before and after this change
- **THEN** the executables are byte-identical

#### Scenario: No thread imports
- **WHEN** a probe built without the option is inspected
- **THEN** it imports no thread, synchronisation or atomic-wait function

### Requirement: Bit-identical parallel output
With the option on, rendering a song in parallel with any worker count SHALL produce output bit-identical to the
single-thread render of the same song, for any sequence of render call sizes and after seeks.

#### Scenario: Worker counts agree
- **WHEN** a parallel-safe song renders with 1, 2, 3 and 8 workers
- **THEN** all four outputs equal the single-thread output bit for bit

#### Scenario: Repeatable parallel render
- **WHEN** the same song renders twice in parallel
- **THEN** both outputs are bit-identical

### Requirement: Dependency order kept
In parallel, a track SHALL start a sub-block only after every track it receives from has finished that sub-block, the
master SHALL render last, and each track's devices SHALL run in chain order.

#### Scenario: Sidechain across workers
- **WHEN** track A sends into the sidechain of track B and both are rendered in parallel
- **THEN** B's sidechain input for each sub-block equals A's finished main output for that sub-block

### Requirement: Single-thread fallback
A song SHALL render on the single-thread path, with the same output, when it is not parallel-safe: a device is listed on
more than one track, an automation targets a device not on its own track, or an engine in the song draws from the shared
song random source while rendering.

#### Scenario: WaveSabre song
- **WHEN** a song with WaveSabre devices is rendered with several workers
- **THEN** it renders on the single-thread path and the output equals the single-thread render

#### Scenario: Unified synth song
- **WHEN** a song using only unified-synth devices is rendered with several workers
- **THEN** it renders on the parallel path

### Requirement: Float mode on every worker
Each worker SHALL render with the same denormal flush mode as a single-thread render call, and SHALL restore its own
previous mode when its work for the call ends.

#### Scenario: Denormal-heavy tail
- **WHEN** a song whose reverb tails decay into the subnormal range renders in parallel
- **THEN** the output equals the single-thread render bit for bit

### Requirement: Worker count on the command line
`song render` SHALL accept `--threads <n>` with `n ≥ 1`. `n` = 1 or an absent flag SHALL use the single-thread path; an
invalid `n` SHALL be an error with nothing written.

#### Scenario: Same WAV
- **WHEN** a song renders with `--threads 4` and without the flag
- **THEN** both WAV files are byte-identical

#### Scenario: Bad count
- **WHEN** `song render` gets `--threads 0`
- **THEN** it exits with an error and writes no file
