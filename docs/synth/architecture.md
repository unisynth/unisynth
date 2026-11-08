# Audio architecture

Moved from the 64k intro repo's `docs/architecture.md` (its audio section) when the synth sources moved here.
Per-crate detail: the other files in this directory.

## Audio: synth-core, synth-mix, engines (change add-wavesabre-rs)

Detailed per-engine architecture with diagrams: `docs/synth/` (`synth-core.md`, `wavesabre.md`, `tunefish4.md`).

```text
song blob (WaveSabre layout) ──► synth_core::mix::Song::parse (zero-copy, validated)
                                      │
 Env { rate, tempo, bank: Option<&[u8]> } ──► Player<K: Kinds, R: Rng>::new(song, env, pool, rng, sub=100)
                                      │   load devices in song order (Kinds::load draws load-time randoms)
                                      ▼
 per 100-frame step, tracks in dependency order:
   receives (main / sidechain × volume) ─► device chain (Device::run in place) ─► track volume ─► master
   lane events ─► Device::note_on/off(offset)   automation points ─► Device::set_param (lerp)
                                      │
                                      ▼ render_f32 / render_i16 (upstream (int)(x·32767) clamp), seek
 engines: crates/wavesabre (WsDevice kinds 0–12)   ·   crates/tfsynth (TfDevice kinds 32–39)   ·   mixed: Either<A, B>
```

- **Layers.** `synth-core` holds what both engines share: DSP primitives (Freeverb `Comb`/`AllPass`, `Ring` delay,
  DF1 `Biquad`, `OnePole`), maths, the denormal guard, the bump `Pool`, the `Rng` trait, the `Device` trait with its
  event slots (`Events`), the `VoicePool` (polyphony, unison, stealing policy, mono legato) and the mixer (`mix`).
  Engines only add devices.
- **Device contract.** Channels are main L/R + sidechain L/R. Instruments overwrite main, effects transform main and
  may read the sidechain, nothing writes the sidechain. Events carry frame offsets inside the call; a device can ask
  for an event lookahead (Tunefish: 511 frames for its 512-frame blocks). `reset()` is required (seek). No globals:
  rate, tempo, absolute frame and the song random source come in `Ctx`.
- **Memory.** No heap: the caller gives one byte slice; `Player::required_pool` sizes it exactly (voices, delay
  lines, decoded GSM samples, track buffers). Adultery reads GM.DLS waves in place from the caller's bank bytes.
- **Determinism.** One random source per song, drawn at load in device order and then in render order; seek
  restores the post-load snapshot. Output is independent of the caller's buffer size, but depends on the 100-frame
  step (voices of one synth share the random source span by span), so references are rendered at that step.
- **Features.** Each WaveSabre device is a cargo feature (`ws-<name>`); a song using a disabled kind fails at load.
- **Parity tooling.** `crates/wavesabre/tests/golden.rs` (self-golden hashes of synthetic songs covering every
  device), `tools/ws-parity` (opt-in: native upstream build vs ours per device, SNR ≥ 60 dB; 12/16 identical),
  `tools/synth-parity` (BS.1770 loudness, octave spectrum, onsets; shared with the Tunefish parity tool) for whole
  songs. Results, speed and size: `research/softsynth-research.md`; size probe `research/ws-size-probe`.
- Not wired into `intro.json` or the intro player yet (`add-audio-to-synth`); the DAW (below) plays and exports songs.

## Tunefish-compatible engine `tfsynth` (change add-tf-synth)

- **Role.** MIT reimplementation of Tunefish 4 from the behavioural spec (`research/tunefish4-functional-spec.md`):
  112-value patches, spectral IFFT wavetable generator, noise, ladder/RBJ/notch filters, block-rate ADSR/LFO,
  multiplicative mod matrix (8 of 10 slots live), 16 voices on `synth-core`'s `VoicePool` (`Steal::Oldest`), 10
  serial insert effects (`tf-fx-<kind>` features), `.tfm` decoder + `TfmPlayer`. 44.1 kHz, 512-frame blocks.
- **On shared tracks.** `TfDevice` registers kind 32 (instrument, lookahead 511, fires events at the start of the
  absolute 512-frame block that contains them — bit-identical to the `.tfm` player) and kinds 33–39 (each effect
  standalone, caller-block-size independent). Mixed-engine songs load through `synth_core::mix::Either<A, B>`
  (e.g. `Either<WsDevice, TfDevice>`).
- **Memory.** Voices from the caller pool; each instrument carves an effect pool of `Chain::need(patch)` bytes and
  allocates a slot's state the first time its kind is used (delay 2 × 44 100 floats, reverb ≈ 100 KB, chorus/flanger
  small). A kind that no longer fits after a mid-song change acts as none. `TfmPlayer::required_pool` sizes a song.
  Shared lookup tables (64K noise, sine, exp, note table) are a once-initialised static built from a fixed seed.
- **Quirk policy.** Sound-shaping reference quirks are reproduced (phase-stage table range, 8 live mod slots,
  block-rate control, decay can rise, log₁₀2 HP/BP bandwidth, linear-Hz bend, delay zero-latency copy, chorus flat
  spot, flanger counter flip, effects auto-sleep after 2 s); undefined behaviour is fixed without audible change
  (empty-instrument song length, right-phase overrun, unwrapped chorus/flanger phases, unbounded flanger counter,
  uninitialised state). Randomness: engine-owned fixed-seed xorshift (the reference seeds from entropy).
  Flush-to-zero is on for every instrument render, as in the reference.
- **Verification.** Analytic unit tests per module, self-goldens (`crates/tfsynth/tests/golden.txt`), `.tfm` vs
  shared-track bit-identity, and opt-in reference parity `tools/tf-parity` (upstream built outside the repo by
  `build-ref.sh` + our MIT `render-ref.cpp`, metrics from `tools/synth-parity`). Results, speed (12.9× real time)
  and size: `research/softsynth-research.md`; size probe `research/tf-size-probe`.

## Unified synth `unisynth` and the converter (change add-unified-synth)

Details, formats, parity: `docs/synth/unisynth.md`.

- **One instrument, one effect set.** `crates/unisynth` (`no_std`) implements device kinds 64 (instrument: three
  oscillator slots, noise, four filters, four envelopes, two LFOs, a 12-slot matrix) and 65–73 (distortion, crusher, EQ,
  delay, chorus, flanger, reverb, compressor, formant; a mode per legacy effect). Every module is a cargo feature, so a
  song's build links only what `synth-convert features` lists for it. It loads through the same `Kinds` registry and
  plays the same `synth-mix` blob, so the DAW and the intro player take it unchanged.
- **Converter.** `crates/synth-convert` (host-only) turns legacy WaveSabre songs, `.tfm` songs and Tunefish patches into
  unified songs (`ws`, `tfm`, `patch`, `features`). The reference songs it converts are the demo songs in `project/music`.
- **Parity gate.** `tools/unisynth-parity` renders every legacy device test song and the six reference songs with the
  legacy crate and, converted, with `unisynth`, compares them (loudness, octave bands, onset timing) and writes the
  committed `report.md` with speed, size and the retirement verdict. The legacy crates are removed only when it says
  READY and the A/B listening is signed off.
- **V2 songs** (change add-v2-import). `synth-convert v2m` imports Farbrausch V2 `.v2m` songs, format versions 0–6.
  The mono-sum voice, channel strip, speech and the 7-bit global effects are `unisynth` modules (functional names,
  change unify-v2-algorithms); behaviour that changed between V2 builds is written by the converter as settings or
  quirk flags (the player has no format version since the v3 layout). The channel strip's sends travel on extra
  device output pairs, which a receive selects through its channel field (`synth-core` feature `pairs`; old blobs
  are unchanged). There is no legacy V2 crate. The reference is the external v2redux player, and
  `tools/unisynth-parity/v2-report.md` holds the parity of the 15 acceptance songs. Songs stay outside the
  repository (`tools/v2-extract`, `research/v2-intros.md`).

## DAW: song source, live host, browser editor (change add-synth-daw)

Details, formats and shortcuts: `docs/synth/daw.md`.

```text
project/music/*.song.json (song/2) ─► crates/song (validate · export · import · presets · tables · render)
        export ─► synth-mix blob ─► Player (= Mixer + BlobSource)  ─► `song render`, DAW bounce (Worker)
                                    (the intro will play the same blob with the same Player: add-audio-to-synth)
        live   ─► crates/synth-wasm Host (= Mixer + editable schedule) ─► AudioWorklet in packages/daw (/daw)
```

- **One mixer step.** `synth_core::mix::Mixer` runs the per-sub-block track step for both `Player` and the live host
  (`Source`/`TrackSource` traits), so live playback from frame 0 without edits is bit-identical to the player (the code
  the intro will use). The split kept all 56 lab reference renders byte-identical; it costs +192 B in the i686 size probe
  (size pass deferred to add-synth-daw task 1.4).
- **Source of truth.** The song is editable JSON (`song/2`, values by parameter name); the blob is its export.
  Validation reports JSON pointers; the server validates before writing (`/api/music`, confined to `project/music`).
- **Edits while playing** apply per track at the next 100-frame sub-block (parameter set, device rebuild, schedule
  swap, mixer rebuild); seek and loop chase held notes; the bounce is the exact reference.

## Multi-threaded rendering (change add-synth-threads)

- **Build option, zero cost when off.** Cargo feature `synth-core/threads` (forwarded by `unisynth/threads`,
  `tfsynth/threads`; on by default only in `crates/song`). Without it nothing thread-related is compiled: the i686
  size probes are byte-identical to the pre-change build (`research/size-threads/hashes.sh` vs `baseline.txt`).
  `synth-wasm` and the player do not enable it.
- **Unit = track, by dependency level.** At load the mixer groups rendered tracks into levels (no receives = level 1,
  a track one above its highest sender, master alone last). Per sub-block each level's tracks run as jobs on a
  caller-supplied `Par` (fork-join); `synth-core` stays `no_std` and spawns nothing. `crates/song/src/workers.rs` is the
  std pool (persistent scoped helpers, barrier per run, spin → yield → park).
- **Bit-identical.** Each job is the serial track step (same op order, receives summed in the same order), with its own
  denormal guard (FTZ/DAZ is per thread). Songs fall back to the serial path when a device kind draws from the shared
  song random source while rendering (`Kinds::RENDER_RNG`: WaveSabre yes, `unisynth`/`tfsynth` no) or a device sits on
  two rendered tracks.
- **Use.** `song render <song> -o out.wav --threads 8`. Measured speed-up on the dense project songs: 1.7–4.5× at 8
  workers on 8 cores (`research/size-threads/README.md`).
- **Browser bounce (change add-wasm-threads).** A second engine module `synth-mt.wasm` (`synth-wasm` feature
  `threads`, shared imported memory, atomics, `-Z build-std`, own target dir) serves the DAW bounce when the page is
  cross-origin isolated. Per bounce attempt the page creates a fresh shared `WebAssembly.Memory`; the bounce Worker
  opens the bounce with a `song::workers::Pool` and allocates a stack + TLS block per helper; helper Workers instantiate
  the same module on that memory, set `__stack_pointer` / `__wasm_init_tls` (no wasm-bindgen) and block in
  `pool_helper`. Any failure or cancel terminates all Workers and the bounce falls back to the single-thread
  `synth.wasm` path. 3.3–3.7× on the dense songs at 8 workers; live playback stays single-threaded (measured p99
  overruns on `elysian`: a render-ahead threaded live path is a possible follow-up).
