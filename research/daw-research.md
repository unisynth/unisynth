# DAW research: how DAWs and trackers work, and a DAW for `unisynth` (2026-10-26)

Goal (user): a DAW integrated with the soft synth (`crates/unisynth`) that loads and saves unified-synth song files,
using wasm where it makes sense. This page surveys how DAWs and trackers are built, what the demoscene synths did, how
a web audio engine with wasm works, and maps all of it onto what this repo already has. It ends with options and open
questions. Nothing here is decided yet.

Sources: Ableton Live 12 manual (Session View, Arrangement View chapters), Wikipedia "MOD (file format)",
github.com/vsariola/sointu README, github.com/logicomacorp/WaveSabre README, github.com/andremichelle/openDAW README,
Chrome "Audio worklet design pattern" (Using Audio Worklet with WebAssembly; Audio Worklet + SharedArrayBuffer).
Repo facts: `docs/synth/unisynth.md`, `openspec/specs/synth-mix/spec.md`, `crates/synth-core/AGENTS.md`,
`docs/architecture.md`.

## 1. The parts every DAW has

| Part | What it is | Notes for us |
|---|---|---|
| **Transport and clock** | Play/stop/loop/seek. Musical time in ticks (PPQ, e.g. 960 per beat) mapped to samples through a tempo map. | Our player knows one tempo and seconds/samples only. Musical time (bars, beats, ticks) must live in the source format. |
| **Tracks** | One per instrument or bus: input (MIDI clips or receives), device chain, volume/pan, mute/solo, sends. | `synth-mix` already has this: track = volume, receives (main or sidechain), device chain, one MIDI lane, automations; last track is master. |
| **Clips / regions** | Blocks of notes (or audio) placed on a track at a time. Can be moved, copied, looped, linked (several instances of one clip). | The blob has no clips: one flat event lane per track. Clips are an editor concept that flattens on export. |
| **Note editor** | Piano roll (pitch × time grid, mouse) or tracker pattern (rows × channels, keyboard). Same data, two views. | Both are views over the same clip data. |
| **Devices** | Instrument (notes → audio) then effects (audio → audio), in series. Each has parameters with a range, unit and curve. | Kind 64 instrument + kinds 65–73 effects. `params/host.rs` already has names and laws (normalized ↔ physical) for all 177 instrument values, so a parameter UI can be generated. |
| **Mixer** | Faders, sends/returns, sidechain routing, master. | Track volume + receives (channel 0 main, 2 sidechain). No pan per track in the blob (pan lives in devices). |
| **Automation** | Parameter curves over time: per track in the arrangement (Ableton "automation") or per clip (Ableton "clip envelopes", Renoise per-pattern automation). | Blob: points per (device, param), time in samples, value as a byte (/255), applied once per 100-frame sub-block with linear interpolation. |
| **Audio engine** | A real-time thread rendering small blocks (64–512 frames). It never allocates, locks or does I/O; the UI talks to it through lock-free queues. | Our engine already has this shape: no heap, a fixed pool, sub-blocks, sample-exact event offsets. |
| **Undo / project file** | Every edit is a command (do/undo). The project file holds musical structure, names, colours, UI state. Bounce/export renders the final output. | Same pattern as `intro.json` → `64K1` blob: a rich source file, a compact exported blob. |

## 2. Two arrangement models

### Ableton Live

- **Arrangement View**: one fixed timeline, clips laid out left to right, automation lanes under each track. This is
  the traditional sequencer model, and what a finished song (or a demo soundtrack) is.
- **Session View**: a grid of clip slots, tracks as columns and **scenes** as rows. Any clip can be launched at any
  time, in any order; launches snap to a global quantization (e.g. next bar). Scenes launch a whole row. Good for
  sketching and jamming; the result is usually recorded into the arrangement.
- **Clip View** (notes + clip envelopes) and **Device View** (the track's chain) sit at the bottom.

### Trackers (Ultimate Soundtracker → NoiseTracker/ProTracker → FastTracker/Impulse Tracker → Renoise, Sointu)

- A **pattern** is a table: one column per channel, a fixed number of rows (MOD: 4 channels × 64 rows). A cell holds
  note, instrument, volume and an **effect command** (MOD: `0xy` arpeggio, `1xx`/`2xx` portamento, `Cxx` volume,
  `Fxx` speed/tempo, `Bxx`/`Dxx` jump/break).
- An **order list** (song sequence) says which pattern plays at each position, so repeated parts cost one pattern.
- Time is rows: MOD uses *speed* (ticks per row, default 6) and *tempo* (BPM, 125 → 50 Hz ticks). Renoise uses lines
  per beat (LPB).
- Editing is keyboard-first: the cursor moves down a row after each note, the computer keyboard is a piano.
- Renoise adds a pattern matrix (per-track pattern slots, muted blocks), per-pattern automation and native/plugin
  instruments.

Why trackers matter for a 64k: the data is compact and repetitive by design. Pattern + order list is a compression
scheme the musician sees. 4klang/Sointu store songs as short note patterns per track plus an order list; Tunefish
`.tfm` uses a row grid (4 rows per beat).

## 3. What demoscene synths did for the "DAW" part

| Synth | Authoring | Song data in the intro |
|---|---|---|
| **WaveSabre** (MIT) | VST plugins in a stock DAW (Ableton Live, Renoise, Reaper). A converter reads the DAW project and writes the song blob. | Flat per-track MIDI lanes + automation (the layout our `synth-mix` reads). |
| **Tunefish 4** | VST in Renoise. | `.tfm`: row grid, 4 rows per beat. |
| **64klang** | VST with a node-graph editor, Windows only. | Split delta streams. |
| **Sointu** (MIT, 4klang fork) | Its own tracker app (`sointu-track`, Go + gioui GUI, oto audio) **and** a VST. Songs are YAML. Targets 386, amd64 and **WebAssembly**. | Patterns per track + order list. |

The common route is "plugin in a pro DAW + converter". Only Sointu built its own app, and it is a tracker with the
synth editor built in, so the music and the exact player synth are one program.

## 4. A web audio engine with wasm

- **AudioWorklet**: the browser calls `AudioWorkletProcessor.process()` on the audio thread for every **128-frame
  render quantum**. At 44.1 kHz that is a 2.9 ms budget. The AudioContext can be created at 44 100 Hz, which matches
  our songs.
- **wasm in the worklet**: instantiate the module inside `AudioWorkletGlobalScope` (compile on the main thread, pass
  the module in the node options, or `addModule` a bundle). wasm works only on its own heap, so each quantum copies the
  output from the wasm heap into the worklet's `Float32Array`s (cheap: 2 × 128 floats).
- **Talking to the engine**: `port.postMessage` for edits (simple, a few ms latency, allocates on the audio thread) or
  a `SharedArrayBuffer` ring buffer + `Atomics` (no allocation, needs cross-origin isolation: our local server would
  send `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`).
- **Heavy work off the audio thread**: Chrome's pattern runs the DSP in a dedicated Worker that fills a SAB ring
  buffer; the worklet only copies. This tolerates slow renders (more latency, no glitches). Useful if a song is too
  heavy for 2.9 ms quanta.
- **Prior art**: openDAW (TypeScript web DAW, AGPL-3.0 / commercial licence, reference only, do not copy code).

## 5. What we already have

```text
crates/unisynth     no_std f32 instrument kind 64 + effects 65–73 (in progress), chunk v1, 177 params, laws, names (std)
crates/synth-core   Device trait, VoicePool, Pool (no heap), mix::Song::parse, mix::Player (render_f32, seek)
synth-mix blob      tempo, rate, length, devices {kind, chunk}, lanes (delta, note, vel), tracks, receives, automations
crates/wasm         std cdylib, plain C ABI (alloc/dealloc/out_ptr/out_len), already built by packages/editor/build.ts
packages/editor     three.js intro editor, loads wasm engine; packages/cli serves it on 127.0.0.1 with /api/project
```

`unisynth` and `synth-core` already pass `cargo check --target wasm32-unknown-unknown`. So the editor can run **the
same code the intro plays**: the preview is the final sound, not an approximation. That alone justifies wasm; a JS
re-implementation of the synth would drift.

### Gaps between "a DAW" and the blob

1. **No source format.** The blob is a player format: no names, no musical time (bars/beats), no clips or patterns,
   automation quantized to bytes, flat lanes. Saving a DAW project as only the blob loses the work. We need a source
   file (e.g. `song.json`, like `intro.json`) that **exports** to the blob. "Load" of an existing blob is an import:
   devices, chunks, tracks, routing and automation come back exactly; each lane becomes one long clip.
2. **Live editing vs `Player::seek`.** `seek(f)` resets and re-renders silently from frame 0. At ~10× real time,
   seeking to 3:00 costs ~18 s. A DAW needs edits to sound immediately. Options:
   - a **live sequencer** in the wasm host: it drives devices directly (`note_on/off`, `set_param`) from the editable
     clip data with the same rules as `Player` (100-frame sub-blocks, block-start automation lerp). Note edits ahead
     of the playhead just play. Patch knob moves call `set_param` on the running device. Exactness is checked by an
     offline bounce of the exported blob through the real `Player`.
   - **state snapshots**: all engine state lives in one caller pool (no heap), so a snapshot every N seconds could be
     a byte copy, if the `Player` itself is placed in that pool. Fast scrubbing, but it needs work in `synth-core`.
   - Structural edits (oscillator type, voice capacity, adding a device) rebuild that device from its chunk.
3. **Instrument editor.** A UI for 177 values (groups: voice, osc ×3, noise, filter ×4, env ×4, LFO ×2, mod ×12) can
   be generated from the layout and the laws (`params/host.rs`), showing physical units (Hz, ms, dB, semitones).
   The wasm module exports that table once.
4. **Audition.** Play notes from the computer/MIDI keyboard (Web MIDI) into a selected instrument while stopped.
5. **Sync with the intro.** Song time is intro time. The keyframe timeline (`crates/timeline`) could snap to bars and
   beats, and later read note/envelope events as sync sources (the Rocket idea).
6. **Size.** The blob stores flat lanes; repetition is left to the packer. Patterns + order list (Sointu, 64klang
   split deltas) could be a later blob change, measured with the size probes. Not needed for a first DAW.

## 6. Options

| | A. Plugin in a pro DAW | B. Own web DAW in the editor (wasm) | C. Own native app (Rust, egui + cpal) |
|---|---|---|---|
| How | `unisynth` as CLAP/VST3 (e.g. nih-plug), compose in Ableton/Renoise/Reaper, converter from the project file (WaveSabre route) | New editor view: tracks, clips, piano roll / pattern editor, mixer, generated instrument UI; engine = `unisynth` in an AudioWorklet | Same features as B, as a desktop app with native audio |
| Pros | Mature editing, MIDI, recording at once | Same engine bit-for-bit, one project with the visuals, JSON source that agents and git can edit, reuses `packages/editor` + `packages/cli` + `crates/wasm` | No browser audio limits, no wasm boundary |
| Cons | Converter per DAW format (`.als` is gzip XML, Renoise `.xrns` zip XML); musician needs a licence; no intro sync; plugin GUI work anyway | Most code to write; browser audio budget per 128 frames; live-sequencer work | Second UI stack next to the three.js editor; no shared project UI |
| wasm? | No | Yes, central | No |

Hybrid: B first; A later as a thin plugin over the same crate if a musician prefers their own DAW.

Recommendation: **B**, scoped as a first slice: `song.json` source + export/import of the `synth-mix` blob, a wasm
synth host in an AudioWorklet (live sequencer, `set_param`, audition, offline bounce through `Player`), an arrangement
view with clips and a piano roll, a generated instrument editor, a simple mixer. Session View, tracker view, audio
clips, recording, plugin hosting and pattern-compressed blobs come later or never.

## 7. Open questions for the user

1. "Unified synth format files": is that the `synth-mix` song blob with kinds 64–73 (the player format), a separate
   patch file (one instrument chunk, e.g. a preset library), or a new editable song source? Proposal: source
   `song.json` + export/import of the blob + chunk-based patch presets.
2. Note editing style: piano roll, tracker pattern editor, or both over the same data?
3. Session View (clip launching) needed, or arrangement only?
4. Inside the existing intro editor (one app, shared timeline) or a separate page/package?
5. Timing: wait for `add-unified-synth` to finish effects (kinds 65–73) and the converter, or start the DAW now
   against the instrument alone?
