# crates/synth-convert

Host-only (std) converter from legacy songs to unified songs (change add-unified-synth). Never linked into the player,
not in the workspace `default-members`. Library `synth_convert` + bin `synth-convert`. Today: `ws` (effects, the WaveSabre instruments and
the Tunefish instrument kind 32 with its insert effects), `tfm`, `patch` and `features`.

```text
synth-convert ws    <in.song> <out.song>                 legacy synth-mix song (kinds 0–12, 32–39) → unified song (64–73)
synth-convert tfm   <in.tfm> <out.song> [tail-s]         .tfm song → unified song (tracks, inserts, master)
synth-convert patch <patch.txt> <phrase.txt> <out.song>  one Tunefish patch (112 numbers) + `block note velocity` lines
synth-convert features <unified.song>                    the unisynth cargo features it needs, comma separated
```

Any problem (truncated or malformed song, a sample that is not GSM, unknown or not-yet-mapped device kind, an already
unified device) prints a message naming it to stderr, exits non-zero and writes no output. Warnings (voice cap, unison
over 16) go to stderr and do not stop the conversion.

Rules (details and mapping tables in `docs/synth/unisynth.md`, section Converter):

- **Structure.** Devices convert in place (same index); tracks, lanes, receives, volumes and device order are copied.
- **Effects.** Unified chunk = mode value + the legacy chunk unchanged; effect automation moves to parameter + 1.
- **Instruments.** Physical values are mapped through the unified laws. Automation lanes are re-mapped per affected unified
  parameter: continuous laws keep the points and add points where interpolation would be > 1 byte step off at a 100-frame
  sub-block start; discrete decodes become steps (two points at one frame). Structural parameters are never automated.
- **Tunefish.** Insert effects become devices after the instrument; automated routing (effect-slot kind, mod source/target)
  is an error; `.tfm` events land at 512-frame block starts (the legacy `f32` clock).
- **Output.** Written only after every device converted; errors name the problem and exit non-zero.
- **Walkthrough.** The commands in the docs run as written (`FEATURES_DUMP=<dir>` makes the legacy test songs).

| File | Purpose |
|---|---|
| `Cargo.toml` | Deps `synth-core`, `gsm`, `wavesabre` (`ws-all`), `tfsynth` (`tf-fx-all`; legacy decoders, kind ids, `.tfm` parser), `unisynth` with `std` (chunk encoder, law inverses, `needs`). Bin `synth-convert`. |
| `examples/patch.txt`, `examples/phrase.txt` | Walkthrough inputs for `synth-convert patch` (a 3-layer wavetable pad with a chorus; two notes). Kept valid by `tests/tunefish.rs`. |
| `examples/legacy.rs` | Renders a legacy synth-mix song (kinds 0–12 or 32–39) or a `.tfm` with its legacy crate to a 16-bit WAV and prints `engine audio-s render-s`; `tools/unisynth-parity`'s reference render. |
| `examples/legacy_features.rs` | Prints the `ws-*` cargo features a legacy WaveSabre song's devices need; `research/size-compare`. |
| `src/auto.rs` | Automation re-mapping: `affected(probe)` (unified params a legacy param moves, over 256 bytes, structural/raw excluded), `convert(points, params, choices, eval)` → `Lane`s: continuous = legacy points at unified values + points where interpolation is off by > 1 byte at a 100-frame block start; discrete (law choice/count/index/coarse/ratio, or `choices` for sampler shapes) = steps at block starts where the decode changes. `Sim` ports the player's `AutoState::step`; `read_lane` reads a lane like the player. |
| `src/capacity.rs` | Voice capacity 1–64: `analyse(lanes, life, unison, legato)` = max overlapping note spans (note-on to note-off + tail; Thunder fixed sample length; unreleased notes to the end) over all lanes the device hears × unison; returns capped and uncapped counts. |
| `src/features.rs` | `features(blob)` → `us-*` names the unified song needs: every device's modules (`unisynth::needs`), instrument automation spans per device (via the track's device list), effects by mode. Legacy kinds → `ConvError::Device` (convert first). `explain(blob, cost)` → `Vec<Need{feature, why, cost}>` (causes from `needs::instrument_why`, effects by device/kind/mode), `read_cost(marginal.csv)` (`all` row minus feature row), `render_text` (3 causes per feature + `… and N more`, total), `render_json`; CLI `features <song> --explain|--json [--cost csv]`. See change: shrink-unified-synth. |
| `src/fx.rs` | Effect mapping: `target(kind)` → (unified kind, mode) for the 15 legacy effect kinds (WaveSabre 3–10 except instruments, Tunefish 33–39); `chunk(kind, mode, legacy)` puts `inv_choice(mode)` in front of the legacy chunk (params + trailer unchanged; a chunk under 4 bytes becomes the mode and an empty parameter list). |
| `src/instr.rs` | WaveSabre instrument mappings: `build(&Legacy, &Cx)` → `Patch` (Falcon, Slaughter, Specimen/Adultery via `SmpMap`, Thunder). `Cx { automated, capacity }`: neutral routes of never-moving params are left out, silent never-rising oscs are off. `discrete_shapes` lists the sampler's choice shapes for `auto`. Slaughter sets `ext::NOISE_WIDTH` 0 (one noise on both channels). |
| `src/legacy.rs` | Legacy instrument chunks (WaveSabre 0–12 instruments and Tunefish 32): kinds, `param_count`, constructor `defaults`, `roles` (unison/mode/release indices), `decode` (params after the GSM sample for Specimen, none for Thunder; non-finite → 0; sample entry = the chunk's sample section). |
| `src/lib.rs` | `convert(blob)` → `Converted { blob, warnings }` (`convert_ws` = blob only): reads the song, maps every device in place (same index), effect automation parameter + 1, instrument automation to lanes via `auto`, rejects automated Tunefish routing, appends a Tunefish instrument's insert effects after it in each track (automation positions and effect-parameter automation follow), writes. `ConvError::{Input, Device}`. |
| `src/main.rs` | CLI: `ws`, `tfm`, `patch`, `features` (text files of numbers, `#` comments); converts fully before writing the output file; prints warnings to stderr. |
| `src/model.rs` | `SongM` (owned song: devices, lanes, tracks with receives, device lists, automations as point bytes): `read` via `synth_core::mix::Song` (all validation there), `write` in the same layout. |
| `src/patch.rs` | `Patch` under construction: 184 values (180 normalized, 3 raw sample indices, 3 controllers, noise width) from `DEFAULTS`, setters in physical terms (`osc`, `shape`, `env`, `lfo_sine`, `filter`, `sampler_source`), mod slot allocator (`scale`, `offset`, `route`), `finish` (clamp to 0..1, raw sampler index to 0..65535, non-finite → default). |
| `src/tf.rs` | Tunefish instrument mapping: `build(p, cx)` → `Patch` (wavetable slot 1, noise + band, 4 filters in Tunefish order, LFOs, Tunefish-mode envelopes, mod slots 1–8 as Scale at the same index, velocity routes in slots 9–10, pan compensation `pan()`); organ gate `gate_ms`/`release_ms` (simulates `tfsynth::env::Adsr`, whole blocks + `FADE_CUT` ⅓); `volume_adsr`; `insert_chain(p)` (effect slots → unified effect devices), `effect_chunk`, `instrument_device`, `capacity` (`GenPoly`), `warnings` (`GenSlop` > 1 Hz, release > 5 s, dropped slots 9–10), `discrete_shapes`. |
| `src/tfm.rs` | `convert_tfm(blob, tail)`: `.tfm` → song (track per non-empty instrument: instrument + inserts; master last with receives ×11 and volume 200/32768); `Clock` replays the legacy player's `f32` block clock to place events at block starts; note 0 dropped, velocity 0 = off, note & 0x7f. `convert_patch(patch, phrase, tempo, length)`: one instrument + `(block, note, velocity)` phrase. |
| `tests/common/mod.rs` | Test song builders and `loads_and_renders` (unified player only; a legacy kind fails the load). |
| `tests/effects.rs` | Every unified effect mode against the original effect of WaveSabre or Tunefish: random parameter sets through noise, bit-exact or within `TOL_FM`/`TOL_QUANT`; moved from `unisynth` so that crate has no dependency on them. |
| `tests/engines.rs` | Unified wavetable tables, ladder/RBJ/notch filters and the noise band path against the Tunefish code, sample for sample. |
| `tests/features.rs` | Minimal-feature scenario (Slaughter), every instrument kind and all 15 effect modes, automation reach (filter types, noise, envelopes, routing lanes, per-device spans), errors; opt-in `FEATURES_DUMP=<dir>` writes converted songs for `tools/check-song-features.sh`. |
| `tests/instruments.rs` | Scenarios of the spec (WaveSabre song, Slaughter cutoff route, cutoff automation, filter type sweep, Thunder level), physical decode of every Slaughter/Falcon/sampler parameter, discrete sampler sweeps, shared-device capacity, out-of-range decodes, structural lanes never written, hostile chunks load. |
| `tests/parity.rs` | In-process device parity: every `wavesabre/tests/songs.rs` song and ~30 single-parameter variants rendered by `wavesabre` and, converted, by `unisynth`; loudness ≤ 1 dB, octave bands ≤ 3 dB (GM bank songs when the macOS bank exists). |
| `tests/tunefish.rs` | Tunefish: every `tfsynth` synthetic patch vs its legacy render (loudness ≤ 1 dB, bands ≤ 3 dB; unison stacks averaged over repeats), `.tfm` structure/timing/master loudness, organ gate (loudness and end-of-note), parameter decode, pan, mod slots/targets, kind 32 in songs (inserts, effect automation, rejected routing, capacity), CLI. Opt-in `TF_SONGS`. `PARITY_DUMP=<dir>` writes the legacy song of every synthetic patch for the parity tool. `no_remorse_instruments_ring_as_long_as_their_legacy_render` (single note per no_remorse instrument, ring time vs tfsynth; skips without the synth-lab file); opt-in `NR_SOLO=1` `no_remorse_solo_tails` (each instrument solo through TfmPlayer vs converted song: loud/quiet level Δ, tail end). Opt-in `NR_DUMP=<i>` (10 ms windows), `NR_WAV=<dir>` (writes solo `lane<i+1>-tunefish-reference.wav` / `-daw-unisynth.wav`), `NR_NOTE=<i>` (two short notes with/without effects), `NR_DET=1` (unison and slop off). `an_enveloped_wavetable_is_bright_from_the_first_block` (ADSR→harmonics + ADSR→volume: first 50 ms bands 500 Hz–4 kHz within 4 dB of legacy). Opt-in `NR_SOLO` also prints per-instrument octave-band Δ; `NR_NOTE` takes `NR_KEEP=<slots>`, `NR_NOMOD`, `NR_NOLP`, `NR_BANDS` (50 ms band table), `NR_QUIET`. |
| `tests/ws.rs` | Every legacy effect → its (kind, mode) and unchanged parameters/trailer; structure preserved field by field; sidechain receive; automation index + 1 with points kept; short chunks; second conversion rejected; every truncation of a song rejected; CLI success and no output on error. |
