# Proposal

## Why

Brain Control's 2010–2012 intros (Pandora, Transplant, Train Control) were made with Enigma Studio 3, whose synth
is Tunefish 3. `tools/tf-extract` lists these intros as "another engine; not supported". Tunefish 3 is the
predecessor of Tunefish 4, which `unisynth` already imports. The two share most modules: noise, LP/HP/BP/notch
filters, two ADSRs with slope, two LFOs, a 10-slot multiplicative mod matrix, 10 insert slots with the same effects
(distortion, delay, chorus, flanger, Freeverb, formant, 3-band EQ). Many parameter names are identical as well.

Tunefish 3 differs in three places that `unisynth` cannot express today:

- **Point oscillator.** A 256-sample cycle drawn through up to 4 control points with step, linear or spline
  interpolation. The mod matrix can move the points' offsets and values. The oscillator also has unison, a
  sub-oscillator, L/R detune and drive, and it runs with up to 8× oversampling and a downsampling low-pass. Tunefish 4
  dropped this oscillator.
- **PADsynth table.** One static table of 2^16 samples per instrument, built from bandwidth, harmonics, damp, scale
  and one of 4 profiles. It is rebuilt only when those values change; it sounds an octave below the note and ignores velocity. Tunefish 4 turned it into the per-block,
  per-voice spectral wavetable (`us-osc-wt`), which sounds related but different.
- **Tracker song format.** Patterns of rows × tracks (note, instrument, velocity, a 16-bit effect word that sets one
  of four per-voice mod inputs) are placed on 16 sequence tracks. The editor stores projects as XML (`.e3prj`).

Material is available: the Enigma-Studio-3 repository (`enigmastudio/Enigma-Studio-3`) holds the synth source, the
complete Transplant project (`demos/transplant/transplant.e3prj`: songs, 58 patterns, 351 pattern instances,
12 instruments), a default project with 20 instruments and 128 preset files (`binary/tf3programs`). The repository
states no licence. As with Tunefish 4, the source is read for behaviour only; our code, tables and identifiers are our
own.

## What Changes

- **Behavioural spec** `research/tunefish3-functional-spec.md`: signal flow, laws, the player build's compiled-out
  modules (`ePLAYER` drops chorus, flanger, EQ, band-pass, notch and the effect-word mod inputs), song timing and
  quirks. It also records the differences from Tunefish 4 against `research/tunefish4-functional-spec.md`, plus a
  feature survey of Transplant and the presets.
- **Reference renderer (opt-in, local)**: `tools/tf3-parity` builds the Tunefish 3 synth and player from an external
  Enigma-Studio-3 checkout with an MIT shim (platform types, scalar SIMD, no editor) in the player configuration. It
  renders a `.e3prj` song to WAV. It skips with a message when the checkout is absent; nothing third-party is committed.
- **Unified instrument (kind 64), two new oscillator types**, each behind its own feature:
  - `points` (`us-osc-points`): a cycle built from up to 4 control points (offset and value can be modulated) plus
    fixed end points, with step, linear or spline interpolation. It is rebuilt per control segment when its inputs
    change and played with the wavetable slot's layers, L/R detune, spread, drive and hard clip, through TF3's
    oversampling and fixed 12.83 kHz decimation stage (shared with `pad`).
  - `pad` (`us-osc-pad`): a static PADsynth table built at load from structural shape values (bandwidth, harmonics,
    damp, scale, profile). It is shared by all voices of the instrument, non-interpolated, with L/R detune and drive.
  - Both are stored at free positions of the oscillator-type choice, so existing chunks keep their meaning. The point
    oscillator's extra fields (point count, interpolation modes, layer settings) are appended parameters (two-byte
    indices, `us-chunk-wide`).
- **Converter**: `synth-convert e3prj <in.e3prj> <out.song> [--song <id>] [tail]` reads the XML project. It maps each
  used instrument to one kind-64 device plus its insert effects (reusing the Tunefish 4 effect and module mapping
  where the laws agree) and expands patterns and pattern instances into note lanes on the player's 384-frame block
  grid. Effect-word mod inputs become controller events at the note. `synth-convert tf3 <program.txt>` converts one
  preset file to a kind-64 patch for the DAW library.
- **Capture check (second reference)**: `tools/tf3-parity/captures.sh` fetches the audio of public YouTube captures of
  Transplant, Pandora and Train Control (pinned video ids, `yt-dlp` via `uvx`, 44.1 kHz WAV) into a local directory
  outside the repo. A capture comparison aligns the converted song to the capture with the shared onset
  cross-correlation, normalises loudness, and checks octave bands (63 Hz–12.5 kHz, since the codec is lossy) and onsets.
  It shows what the released intro really played (player vs editor modules). It is also the fallback reference if the
  Enigma Studio 3 source cannot be built.
- **Parity**: `tools/unisynth-parity` gains Tunefish 3 items: synthetic one-instrument songs per module, and
  Transplant. They are compared with the shared metrics (1 dB loudness, 3 dB per octave band, onsets within one
  384-frame block).

Out of scope: extracting Pandora and Train Control from their executables (the binary song stream has no magic
bytes, so a follow-up change extends `tools/tf-extract`); bit-exact output; Enigma Studio 3's visuals; publishing
Brain Control's music (local only, pending permission as in `project/music/NOTICE.md`).

## Discipline Skills

`doubt-driven-review` (the new oscillator-type positions and the appended parameter layout are frozen once songs
exist), `systematic-debugging` (parity gaps), `performance-optimization` (point-oscillator oversampling and the PAD
table's load time and memory).

## Capabilities

### New Capabilities

None. Tunefish 3 support extends existing capabilities.

### Modified Capabilities

- `unified-synth`: point and PAD oscillator types, their storage in the type choice, appended point-oscillator
  parameters, memory accounting for the shared PAD table.
- `synth-convert`: `.e3prj` import, Tunefish 3 instrument and song mapping, preset conversion, Tunefish 3 parity, and
  where the material lives (and the released intro songs, extracted from the executables: task group 6).
- `synth-daw`: Points and PAD table oscillator blocks; PAD table inputs rebuild the device (task 6.3).

## Impact

- Code: `crates/unisynth` (`osc.rs`, new point and PAD builders, `params.rs`, `module.rs`/`needs.rs`, memory),
  `crates/synth-convert` (new `e3prj.rs`, `tf3.rs`, CLI), `tools/unisynth-parity`, new `tools/tf3-parity`.
- Docs: `docs/synth/unisynth.md` (oscillator slots, converter section), new `docs/synth/tunefish3.md`, the
  directory `AGENTS.md` files, `tools/tf-extract/README.md` (Tunefish 3 status).
- Dependencies: none at build time. The Enigma-Studio-3 checkout is used only by the opt-in reference tool.
- Size and memory: a song without the new types builds and renders unchanged (self-goldens). A PAD slot costs 256 KB
  of pool per instrument and one 2^16-point inverse FFT at load.
