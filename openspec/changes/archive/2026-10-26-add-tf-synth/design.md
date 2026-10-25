# Design

## Context

- Behaviour reference: `research/tunefish4-functional-spec.md` (verified against the upstream source). Algorithm survey and
  measurements: `research/softsynth-research.md`. See proposal.md for motivation.
- The original engine was built and run on macOS from upstream source with a small harness. It renders `no_remorse.tfm`
  (201.6 s) at 12× real time. Its `-Os` x86_64 code is about 17.7 KB uncompressed. Songs are 0.7–1.3 KB after xz.
- Workspace conventions this change follows:
  - Runtime crates are `no_std` + f32 + `libm`, with no `core::fmt` (compo build fails on fmt symbols).
  - Per-kind cargo features (`post-*`, `geo-*`, `fx-*`).
  - bun tools under `tools/`.
  - The packer harness lives in `research/bootstrap-probe/` (squishy/kkrunchy via Wine/docker).
- License rule: MIT output. The upstream source is read to understand behaviour, never copied. Upstream code builds only
  in `/tmp` through our scripts.

## Goals / Non-Goals

**Goals:**
- Sound parity with the original song player at 44.1 kHz / 512-frame blocks, for real songs (metrics in `tf-song`).
- A memory layout that fits a compo exe: no heap, state in caller memory, unused effect kinds compiled out.
- Bit-exact determinism. Every module unit-tested against analytic expectations, with no third-party assets in the repo.

**Non-Goals:**
- Sample-exact equality with the original. The original seeds from system entropy and differs between runs itself, so parity is perceptual and metric based.
- Plugin-mode behaviour (256-frame blocks, plugin loudness), sample rates other than 44.1 kHz, reverse playback.
- SIMD. Scalar f32 first; SIMD only if the speed requirement fails.

## Decisions

### D1. One crate `tfsynth` on top of `synth-core`
Modules: `params` (indices, names, mapping functions), `rng` (Tunefish law, implements `synth-core`'s `Rng`), `tables`
(sine/exp/noise tables computed at init, never embedded), `ifft`, `wavetable`, `noise`, `filter`, `env`, `lfo`, `modmx`, `voice`,
`instr`, `fx/{dist,delay,chorus,flanger,reverb,formant,eq}`, `device` (the `Device` adapters), `tfm` (decoder), `player`.
- From `synth-core` it uses: Freeverb comb/allpass (reverb), the delay ring (delay, chorus taps, flanger 4096 ring), the DF1
  biquad core with its own log₁₀2 bandwidth designer (HP/BP), the one-pole (EQ), `math`, `denormal`, `pool`, and the voice
  pool with policy `Oldest`.
- Tunefish-only algorithms stay here: IFFT generator, ladder, notch, block-rate ADSR/LFO, mod matrix, chorus/flanger
  modulation, formant, power-law distortion, 3-band EQ.
*Alternative (previous version of this design):* keep private `pub(crate)` primitives and extract later. Superseded:
`add-wavesabre-rs` now provides `synth-core` with a second consumer.

### D1b. Devices on shared tracks
- `TfInstrument` implements `Device` (kind 32, 112 params) and declares an event lookahead of 511 frames (the
  `synth-core` `event_lookahead()` hook: in a call over [t, t+n) it gets every event with frame < t + n + 511).
  The instrument renders 512-frame blocks on the absolute grid given by `ctx.frame`, fires each event at the start of
  the block that contains it (Tunefish's own timing), and `run(n)` drains the rendered frames.
- `.tfm` velocity 0 is converted to a note-off when events enter the device. Pitch bend and mod wheel are
  `TfInstrument` methods, not `Device` events (songs never carry them). There is no added latency,
  and the `.tfm` player and `synth-mix` give the same timing.
- Standalone effects (kinds 33–39) wrap one effect slot each with its own copy of the relevant effect parameters
  (parameter order = the effect's subset of the 112-param table). Inside `TfInstrument`, the 10 slots keep sharing the
  instrument's single parameter set (parity).
- Standalone effects run at the caller's `n` where the algorithm allows. Chorus and flanger keep block-rate (512) control
  updates, counted in frames across calls, so their sound does not depend on the caller's block size.
- Auto-sleep applies only inside `TfInstrument`. Standalone effects always run (a track's silence is the mixer's concern).

### D2. Memory: fixed voice state, effect state from a caller pool
- Voice and instrument state are fixed-size structs (16 voices per instrument). Each voice holds three 1024-float tables,
  about 12 KB, so one instrument is about 200 KB.
- Effect buffers can be large: a delay needs seconds of audio, and there are 10 slots per instrument. They are
  carved on demand from a caller-provided `&mut [u8]` bump pool (`Pool`), only for slots actually in use.
- `Player::new(song, pool)` builds instruments for the I used instruments, also from the pool. Running out returns
  `Err(OutOfMemory)`. In the compo, the pool is a zeroed `static mut` in BSS (no file bytes). Its size is printed by
  a `required_pool(song)` helper so the exporter can size it later.
- *Alternative:* const-generic `Synth<I>` arrays. Rejected: the effect buffers would still be worst-case for all 10 slots.
- *Alternative:* `format::Arena`. Rejected: it pulls the scene/timeline dependencies in.
- `Pool` is `synth-core`'s bump pool, shared with every engine in a `synth-mix` song.

### D3. IFFT
An iterative radix-2 complex inverse FFT of size 512, in place, with twiddles from the init-time sine table. Each voice
rebuilds its table only when the generator inputs change (the reference refreshes every 4 blocks), as the spec requires.
*Alternative:* direct additive synthesis. Rejected: it cannot reproduce the spectral bandwidth smearing.

### D4. Math
Use `libm` f32 (`sinf`, `expf`, `powf`, `exp2f`) everywhere at first. Tables cover the hot paths (sine, exp profile)
the way the reference behaves. Swap in cheaper approximations only if the size report or the speed requirement demands it,
each one checked against the parity metrics.

### D5. Determinism and quirk fixes
- One xorshift32 RNG per engine with a fixed seed, advanced in a defined order (note-on → layers → noise).
- The fixes keep the sound and only remove undefined behaviour:
  - The right-channel start phase wraps into [0, 1).
  - The chorus and flanger phase accumulators wrap modulo 2π, and their counters use the wrapped phase.
  - All state starts zeroed.
  - Song length skips empty instruments.
- The list of reproduced quirks is in the `tf-synth` "Quirk policy" requirement. Each quirk gets a named test.

### D6. Effects as features
- Features `tf-fx-distortion`, `tf-fx-delay`, `tf-fx-chorus`, `tf-fx-flanger`, `tf-fx-reverb`, `tf-fx-formant`, `tf-fx-eq`,
  plus `tf-fx-all` (default).
- A disabled kind becomes an empty match arm, so the slot behaves as none.
- Formant coefficients come from the public musicdsp.org "Formant filter" set; we cite it there, not to Tunefish.

### D7. `.tfm` decoding
`Song<'a>` borrows the file bytes. It validates the tags and lengths once (fallible). It then exposes per-instrument
patches as `[u8; 112]` slices and the event arrays as slices, read in a structure-of-arrays layout. No copy is made.
Event times are computed from cumulative row deltas during playback, with one cursor per instrument, instead of being
stored as f32 times. This gives the same block-window semantics without per-event storage.

### D8. Verification layers
1. **Unit tests in the crate.** These are analytic and own-asset only:
   - Spectrum peak positions for the GenScale/harmonics laws.
   - Filter magnitude at known frequencies.
   - ADSR timing in blocks, LFO shapes.
   - Mod-matrix multiplication and the inert slots 9–10.
   - Voice stealing.
   - Each effect's impulse or step response (e.g. Freeverb lengths visible in the impulse response).
   - `.tfm` decode of songs built inside the tests.
   - Determinism (render twice, compare bits).
2. **Self-golden.** FNV hashes of our own renders of in-repo synthetic songs, kept in `crates/tfsynth/tests/golden.txt`. They
   catch regressions and are updated deliberately (`UPDATE_GOLDEN=1`).
3. **Reference parity** (`tools/tf-parity/`, opt-in):
   - `build-ref.sh` clones upstream into `/tmp/tf-ref` and compiles our MIT harness (`render-ref.cpp`, written fresh)
     against upstream's player. The harness computes song length itself, so the upstream crash path is never called.
   - `parity.ts` (bun) renders with `cargo run -p tfsynth --example render` and compares:
     integrated loudness (1 dB), 1/1-octave long-term spectrum 63 Hz–16 kHz (3 dB), and onset frames (≤ 1 block).
   - Songs come from `TF_SONGS`, by default the upstream examples in the `/tmp` clone.
   - The tool skips cleanly when the reference is absent.
4. **Size report.** `research/tf-size-probe/` is a standalone workspace like `bootstrap-probe`. It is a `no_std` i686 exe
   that renders an in-repo synthetic song into a static buffer, built in two feature sets and packed with the existing
   `pack.sh`.

### D9. Player output scaling
The i16 path reproduces the reference mix: instrument sum ×11, then ×(2000·volume) with volume 0.1, then clamp. This
keeps reference-relative loudness for parity. The f32 path returns the pre-scaling bus for the later audio-output change.

### D10. Decisions made during implementation
- **Effect memory per instrument:** each instrument carves an effect pool of `Chain::need(patch)` bytes and creates a
  slot's state on its first use of a kind (bump pool: a replaced instance's memory is not reused; a kind that no
  longer fits acts as none). Exact for static `.tfm` patches; automation that changes slot kinds needs headroom.
- **Mixed-engine songs:** `synth_core::mix::Either<A, B>` (additive to the `synth-mix` capability) combines engine
  registries; `Either<WsDevice, TfDevice>` holds WaveSabre and Tunefish devices on one song.
- **Shared tables** (noise, sine, exp, ρ, LFO steps, note Hz) are a once-initialised static from a fixed seed (as
  `wavesabre::helpers`), not per-instrument memory.
- **Flush-to-zero** in `Instrument::render` (the reference's FTZ): without it decaying ladder/EQ state went subnormal
  and the render ran at 7.1× instead of 12.9× real time.
- **Flanger with LFO = 0** sweeps both channels together (480 s period); the functional spec's "right static" was a
  misreading and is corrected. The fix for the unbounded counter folds the running term every 2²² samples.
- **Formant coefficients** from musicdsp.org #110 with the A-vowel gain adjustment published on the same page.
- Module `gen` is named `wavetable` (`gen` is reserved in edition 2024).

## Risks / Trade-offs

- [Perceptual differences hide in the reference's run-to-run randomness] → Compare metrics averaged over whole songs, plus
  per-module analytic tests. A/B listen on `no_remorse` before closing the change.
- [Memory: about 200 KB per instrument plus effect pools may stress the 32-bit compo address space at 32 instruments] →
  Fine for real songs (typically 8–16 instruments, ≈ 2–4 MB BSS). `required_pool` reports the exact need.
- [`libm` transcendental code inflates the packed size] → The size report quantifies it. D4 allows targeted replacements.
- [The flanger and auto-sleep quirks are odd but audible] → Reproduce them by default, each behind a named test so a later
  "fix" is a deliberate spec change.
- [The upstream reference may stop building] → It is only a dev tool. Parity results are recorded in the change for archive,
  and the unit and self-golden tests keep guarding behaviour.

## Migration Plan

Additive crate. Not linked into the player until the audio-output change, so there is nothing to roll back.

## Open Questions

- Final compo pool size per song. Answered by `required_pool` once the exporter embeds songs (next change).
