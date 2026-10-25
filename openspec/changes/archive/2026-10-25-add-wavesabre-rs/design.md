# Design

## Context

- Source comparison and measurements: `research/softsynth-research.md` ("WaveSabre and 64klang: integration check",
  "WaveSabre: native macOS build", "Tunefish 4 + WaveSabre").
- Upstream WaveSabre (MIT) builds and renders natively on the macOS dev box with no source edits. The lab lives at
  `~/Downloads/64k-tools/synth-lab/wsmac/` (pthread `Windows.h` shim, libgsm `GsmSample`, `$GM_DLS` loader); the
  procedure is the project skill `wavesabre-native-macos-render`. Elysian renders at 7.7× real time single-threaded.
  Upstream `-Os` x86_64 code is ≈36.8 KB for every device plus the song renderer.
- Workspace conventions: runtime crates are `no_std` + f32 + `libm` with no `core::fmt`; per-kind cargo features; bun tools
  under `tools/`; the packer harness is in `research/bootstrap-probe/`.
- Do not run long Docker/Wine renders on the dev Mac (two kernel panics). All reference renders are native.
- `add-tf-synth` will be rebased on this change. Its Tunefish instrument and effects implement `Device` and register kind
  ids ≥ 32.

## Goals / Non-Goals

**Goals:**
- One shared core that the WaveSabre port and the Tunefish engine both use, with no duplicated primitives.
- One song/mixer model that can hold any engine's devices, loading WaveSabre songs unchanged.
- WaveSabre sound parity, near sample-exact per device and metric-based for whole songs.
- No heap, deterministic output, only used devices linked.

**Non-Goals:**
- Multithreaded rendering, audio output, timeline sync, editor/VST work, 64klang, the merged instrument.
- Reproducing upstream's x87 `exp2` and `sin` paths (they are MSVC x86-only; we use `libm` everywhere).
- Sample rates other than 44.1 kHz in parity tests. The code takes the song's rate, but only 44.1 kHz is verified.

## Decisions

### D1. Three layers, two new crates
- `crates/synth-core`: modules `dsp/{comb,allpass,delay,biquad,onepole}`, `math`, `denormal`, `pool`, `rng`, `device`,
  `voice`, and `mix/{song,registry,player}`.
- The registry is a trait, not a table: `synth-core` cannot depend on the engines, so each engine (or a later facade
  crate for mixed songs) defines an enum of its enabled kinds implementing `Kinds` (`need`, `load`) + `Device`, and
  `mix` builds and dispatches through it (`load_devices`, `devices_need`). A disabled feature has no variant, so its
  kind id fails with `UnsupportedKind` at load.
- `crates/wavesabre`: `helpers` (FastSin table, parameter mappings, random law), `gsm`, one module per device.
- The mixer lives in `synth-core` (as module `mix`), not in its own crate. Every engine needs it, it is small, and a
  separate crate would only add a dependency edge.
- *Alternative:* the mixer inside `wavesabre`. Rejected: Tunefish devices would then depend on the WaveSabre crate.
- *Alternative:* one crate per primitive. Rejected: churn with no benefit; features already strip unused code.

### D2. What goes into synth-core (rule)
- A primitive moves into `synth-core` only when both engines use the *same algorithm*: Freeverb comb/allpass, delay ring,
  DF1 biquad core, one-pole, math, denormal guard.
- Engine-specific numerics stay in the engine: WaveSabre's FastSin table and random law; Tunefish's IFFT, ladder and
  block-rate envelopes. RBJ designers are free functions, so Tunefish's log₁₀2 bandwidth variant sits next to the
  standard one without flags in the hot path.
- Envelopes, oscillators and the SVF are not shared (different curves and algorithms; see the research table).

### D3. Device trait shape
- Methods: `note_on(note, vel, offset)`, `note_off(note, offset)`, `set_param(i, v)`, `param_count()`, `reset()`,
  `event_lookahead()`, `run(&mut Io, n, &mut Ctx)`, `all_notes_off()`.
- Buffer roles: instruments overwrite main L/R (upstream `SynthDevice` clears its output, so receives into main
  are replaced); effects transform main L/R and may read sidechain L/R; nobody writes the sidechain.
- `reset()` is required, not defaulted, so `seek` (reset all, replay from 0) cannot silently keep stale state.
- Pitch bend and mod wheel are not in the trait: neither song format carries them. Engines expose them directly.
- `Io` is 4 channel slices (main L/R + sidechain L/R) processed in place, the same as upstream's track buffers.
- `Ctx` carries tempo, sample rate, the song frame of the call's first frame (`frame`, the absolute grid for
  block-based devices) and `&mut dyn Rng`. One `dyn` call per random draw is accepted (it is the song RNG, not a
  per-device vtable). Upstream's globals (`CurrentSampleRate`, `CurrentTempo`,
  `RandomSeed`) become explicit.
- Dispatch is through an enum of enabled device kinds (`match`), not `dyn Device`. That gives no vtables in the compo
  exe and lets disabled features vanish. The trait still exists, so engines and tests use one interface.
- Instruments buffer events with offsets, the same as upstream's `SynthDevice` event array (256 events), and split
  rendering at event offsets.
- `event_lookahead()` L (default 0) lets a block-based engine receive events early: in a call over [t, t+n) the mixer
  delivers every event with frame < t + n + L. A B-frame block engine needs L = B − 1 (Tunefish: 511) for any mixer
  sub-block size, so it renders whole blocks with no added latency. Doubt review (2.1) found that "L frames before
  due" was one call short when a block starts mid-call. Event arrays are fixed slot arrays (`Events<N>`): WaveSabre
  uses 256 like upstream; lookahead devices size N for their window. WaveSabre devices return 0. The mixer keeps one lane cursor per
  (track, lookahead), so each event is delivered exactly once.

### D4. Mixer semantics = upstream SongRenderer, single-threaded
- Tracks render in the dependency order upstream's thread pool would converge to. Upstream with one thread renders the
  first ready track in index order; we use that same order, so device call order and random draws match.
- Automation runs at sub-block start and events fire with offsets inside the sub-block, as upstream does.
  Automation point times are signed (real converted songs hold points before frame 0: SabreWulf 12 has 28 negative
  deltas); note deltas must be ≥ 0.
- The internal sub-block is 100 frames (the upstream players' `stepSize`). Caller buffers are decoupled through a small
  ring of rendered frames.
- `seek` replays from 0 silently. Songs are prerendered in the intro, so seek cost is only an editor concern.

### D5. Memory
- Delay lines, reverb buffers, decoded GSM samples, voices and track buffers come from the caller `Pool`.
- `required_pool(song)` walks the decoded song and sums each device's static need. Echo and Chamber get their worst
  case from parameter ranges, because upstream resizes on parameter change (Echo coarse ≤ 16 × 1/8 note at the song
  tempo plus 200 ms fine; Chamber 31 ms × 10; Smasher lookahead).
- Upstream's resize-on-change behaviour is kept by resizing inside the preallocated maximum.
- Voices: 256 per instrument, exactly like upstream. Upstream's voice constructors draw from the song random source
  (Slaughter: 3 phases × 256 voices at load), so fewer voices would shift every later random draw; and with
  stealing policy `None` a smaller pool would drop notes upstream plays. ≈50 KB pool per Slaughter.
- Random order: `Kinds::load` receives the song random source (load-time draws, device order); the player snapshots
  it after loading, and seek restores that snapshot. `Voice<C>` gets a device context `C` (settings, rate, song rng)
  for note-on/off/slide, because upstream voices read their device at those moments (Falcon draws its phase at note-on).

### D6. Parity strategy
1. **Unit tests:** analytic checks for each primitive, plus each device's documented formulas (Crusher step and hold,
   Scissor transfer curves, Smasher gain law, Echo tap times at a tempo, Cathedral comb lengths in the impulse response).
2. **Device parity** (`tools/ws-parity/`, opt-in): our MIT harness `render-device.cpp` links the native upstream build
   and renders one device from a chunk plus a fixed note list. Our Rust `examples/render_device.rs` renders the same.
   SNR ≥ 60 dB over ≤ 2 s. Short renders keep libm drift below the threshold: Global Groove already matched
   cross-platform at 111 dB.
3. **Song parity:** loudness, octave spectrum and onset metrics, because long double-precision phase accumulators drift
   between libm implementations. The same metric code as `tf-parity`, shared in `tools/synth-parity/`.
4. **Self-golden:** FNV hashes of renders of in-repo synthetic songs, built in tests from our own chunks and with no
   third-party assets.
5. **Size probe:** a standalone no_std i686 workspace like `bootstrap-probe`, in two feature sets, packed with `pack.sh`.

### D6b. Reference renders use 100-frame steps
Output depends on the render step, not just on the song: each synth renders its voices span by span, and all voices
draw from one song random source, so when two voices sound at once (a release overlapping a retrigger) the span length
decides which voice gets which random number. Upstream's players step 100 frames; the lab reference harness therefore
renders with `WS_STEP=100` (the old 4096-frame harness made Global Groove look 0.9 dB SNR; at 100 frames our port is
bit-identical). First full-song comparison (Rust `examples/render` vs native upstream, 1 thread, 100 frames): Global
Groove identical; Engage 80.3 dB; Eladamri 72.7 dB (steady); Elysian 53.1 dB (77–94 dB with a few 39–54 dB windows) —
per-device parity (5.2) narrows these down.

### D7. GSM decoder
- Port libgsm's decoder path only (no encoder), fixed-point as in the original, so the output is exact.
- Output is f32 into the pool. Only WAV49 framing is supported, because that is what the ACM codec upstream produces.

### D8. Adultery and DLS
Upstream reads Windows `gm.dls` through a fixed offset (`0x44602`) and indexes 495 waves. We index the same wave
number through `crates/dls`, which works with any bank. On Windows `gm.dls` the pool-table order equals upstream's
file-order walk (env-gated test over all 495 waves). The wave is read in place from the bank (`Source::Wave`, 8- or
16-bit; 16-bit gives upstream's exact `s / 32768`), so Adultery needs no sample memory and a `SampleIndex` change is
free; voices keep the wave they started with. Specimen and Adultery share one sample voice (`sampler.rs`); they differ
only in parameter map, defaults (Specimen: no loop, manual bounds; Adultery: repeat, bounds from the wave's `wsmp`)
and sample source. The bank bytes are passed to `Player::new` as an optional
`&[u8]`. Loading the bank from the OS (Windows `drivers\gm.dls`, macOS `gs_instruments.dls`) is the player's job and
belongs to the later audio-output change.

## Risks / Trade-offs

- [The `Device` contract is wrong for Tunefish (512-frame internal blocks, shared effect params)] → Tunefish buffers
  internally behind `run(n)`. `add-tf-synth` is updated in the same review round to prove the trait fits before this
  change is implemented (doubt-driven-review on D3).
- [Per-voice pool sizing is coarse] → Fixed 32 voices plus `None` stealing matches upstream when voices run out.
  `required_pool` makes the cost visible.
- [Song-level random order breaks if a later change adds threads] → Spelled out in `ws-synth`. Threads need a spec change.
- [libm vs upstream's macOS libm drift fails per-device SNR on some devices] → Lower the per-device window, or compare
  in the reference with libm (`openlibm`) linked. Each decision is recorded in the change.
- [Third-party songs for parity] → Converted outside the repo in the lab, never committed. Unit and self-golden tests
  carry the in-repo guard.

## Migration Plan

Additive crates, not linked into the player until the audio-output change. `add-tf-synth` gets these updates:
- its DSP primitives come from `synth-core`;
- its instrument and its 7 effects implement `Device` with kind ids ≥ 32, so they work on any `synth-mix` track;
- its `.tfm` player stays as is.

## Open Questions

- Whether to keep Thunder (deprecated upstream) under `ws-thunder` if no target song uses it. Default: keep it, since it
  shares the GSM path with Specimen and costs about 0.9 KB.
