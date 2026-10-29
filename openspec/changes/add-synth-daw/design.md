# Design

## Context

- `crates/unisynth` (in progress, `add-unified-synth`): instrument kind 64 with 180 values (177 normalized + 3 raw sample indices), chunk v1,
  host-side `law_of`/`name` in `params/host.rs`; effects 65–73 still to come. no_std, wasm32-checked.
- `crates/synth-core::mix`: `Song::parse` (zero-copy blob), `Player` (sub-block 100, per sub-block per track: events
  with lookahead → automation `set_param` → clear → receives → device chain → volume; master = last track; one song rng
  in `Ctx`; `seek` re-renders silently from frame 0).
- `crates/wasm` + `packages/editor`: established pattern of a plain C-ABI std cdylib loaded by a Bun-built browser
  bundle; `packages/cli` serves pages on 127.0.0.1 with token + Origin checks and confined `/api/project`.
- Research and options: `research/daw-research.md`. UI direction: `mockup/index.html` in this change.

## Goals / Non-Goals

Goals: compose a full intro song in the browser with the real synth; the saved source is reviewable JSON; what you
hear while editing equals the intro (when no edit happened since play); every device UI comes from tables.

Non-goals: see proposal "Out of scope". No general plugin host, no audio clips, no tempo map.

## Decisions

### D1. Two files: `song/1` source, blob export (like `intro.json` → `64K1`)

The blob lacks names, musical time, clips and full-precision automation. Saving only the blob would lose work, so the
source is JSON and the blob is a build product. Rejected: extending the blob with optional metadata sections (the
player would have to skip them, and editing binary in git is opaque).

Values are stored **by parameter name**, not index, so reordering a table later is a migration in one place
(`crates/song`) instead of silently changing sounds. Normalized values keep full f32 precision; the physical value is
derived (laws are the single source).

### D2. One `crates/song` crate shared by CLI and browser

Validation, export, import and presets are written once in Rust. `synth-wasm` links it, so the browser saves
exactly what `song export` writes. The TS side only keeps the JSON in memory and sends whole songs or diffs to wasm.
Rejected: a TypeScript exporter (two implementations of tick→sample rounding and chunk encoding would drift).

Tick → sample: `round(tick × 60 × rate / (tempo × 960))` in f64. Placements flatten in start order; per track, same
sample: note-offs first, then note-ons in pitch order. Overlapping same-pitch notes: the earlier note is cut at the
later start (one open note per pitch).

### D3. Live host reuses the player's sub-block step through a source trait

`Player::render_sub` becomes `mix::Mixer::render_sub(&mut self, src: &mut impl Source)` where `Source` yields a
track's events before a limit and its automation values for the sub-block. `Player` = `Mixer` + `BlobSource` (current
behaviour, golden hashes unchanged). The live host = `Mixer` + `ScheduleSource` (owned, editable vectors in samples).
This is how D-spec "same sound as the player" holds by construction rather than by a parallel copy.

Risks: a `synth-core` refactor under a finished capability. Guard: existing `player_tests`, `wavesabre`/`tfsynth`
goldens and parity must stay byte-equal; doubt-driven review before merging (proposal discipline list).

Size (task 1.4): the split cost the WaveSabre probe +192 B through inlining; `Mixer::render_sub` is `#[inline(never)]`,
which recovers it there (slaughter 25 710 → 25 522 B, pre-split 25 426 + 92 B of later changes) and costs the
unified-synth probes +272 B (demo song) / +2.9 KB (all modules); chosen by the user.

Rejected: (a) rebuilding a `Player` from a fresh blob on every edit and seeking (seek is O(position), seconds late in
a song); (b) a separate live sequencer copied from `Player` (drift); (c) pool snapshots for seeking (needs the
`Player` inside the pool; kept as a later option for exact scrubbing).

### D4. Edit granularity

| Edit | Host action | Audible effect |
|---|---|---|
| notes / automation of a track | replace that track's schedule; cursor re-found by binary search at the playhead; open notes of that track get note-off | immediate, no glitch on other tracks |
| parameter value (not structural) | `set_param` at next sub-block; also stored as the device's base value | immediate |
| structural parameter, device add/remove/reorder, preset load | rebuild that track's devices from chunks in a fresh per-track pool | that track restarts voices |
| tracks, receives, track order | rebuild the whole mixer (new pool) at the playhead | short click possible; documented |

Exactness after edits is not promised; the bounce (D7) is the truth. Device rebuilds draw their seed from a host rng,
so live sound after a rebuild can differ from the player in random phases only.

### D5. Seek and loop: chase, no pre-roll

Seek resets devices and starts at the target; notes held across the target are re-triggered at it (DAW "note chase").
Effect tails before the target are lost. Exact preview of a position = bounce. Rejected: silent pre-roll from 0
(too slow late in the song, see D3 (a)).

### D6. Audio path: AudioWorklet + wasm, messages over the port

- `AudioContext({ sampleRate: song.rate })`, one `AudioWorkletNode` (stereo out). The wasm module is compiled on the
  main thread and passed in `processorOptions`; the processor instantiates it synchronously.
- `process()` calls `render(128)` and copies 256 floats from the wasm heap to the two output channels.
- UI → engine: `port.postMessage` with small command objects (schedule arrays as transferable `ArrayBuffer`s).
  Engine → UI: meters, playhead and load every ~30 ms. No `SharedArrayBuffer`, so no COOP/COEP headers are needed.
- If a song exceeds the 2.9 ms budget, a fallback runs the host in a Worker with a SAB ring buffer (Chrome's worklet
  + worker pattern). Built only if measurement shows a need (performance discipline: measure first).
- Measured (task 3.7, `packages/daw/bench/dense.ts`, Chrome, main-thread timing of the worklet's exports): the demo
  song needs 0.1 ms per 128 frames on average and 0.7 ms at worst (24 % of 2.9 ms). Stress songs with hundreds of
  voices exceed the budget on sustained CPU (natively too: 1.25× real time for 8 retriggered 6-note chord tracks),
  which the Worker + SAB fallback would not absorb; it is not built. Engine speed belongs to `add-unified-synth` 7.4.
- The AudioWorklet scope has no `TextDecoder` and no `performance.now`: replies go back as bytes, and the worklet's
  load figure has `Date.now` (1 ms) resolution, so the transport shows the worst load of each ~30 ms window.

### D7. Offline bounce in a Worker through the real `Player`

The bounce exports the blob (D2) and renders it with `Player` inside a Worker running the same wasm module; WAV is
written in TS. It equals `song render` byte for byte (same code, same blob). This is the "what the intro plays"
reference while editing.

### D8. UI generated from parameter tables

`synth-wasm` exports JSON tables per kind: `{index, name, group, default, law, unit, min, max, options?, structural}`.
Group names come from the parameter names (`voice`, `osc1..osc3`, `noise`, `filt1..filt4`, `env1..env4`, `lfo1..lfo2`, `mod1..mod12`, `fx`; unisynth `params::name`). Each group renders
as a panel of knobs/selects; known groups may get a small visual (envelope curve, filter response) drawn from the same
laws. New kinds (65–73) get editors without UI code. Knobs: vertical drag, Shift fine, wheel, arrows, double-click
default, `role=slider` with `aria-valuetext` = physical value.

### D8a. Slot racks over fixed slots, not a pluggable voice graph

The instrument has fixed slots (3 oscillators, 4 serial filters, 4 envelopes, 2 LFOs, 12 mod slots); a free node graph
was rejected in `add-unified-synth`. The editor makes them feel pluggable: a rack shows only used slots (type ≠ Off,
or a non-default envelope/LFO/mod slot), "+" fills the next free slot, remove compacts, move swaps. Off slots cost
nothing: the chunk omits default values and a filter type that no song uses is not linked (`us-flt-*` features).
Because slots are addressed by index (`f2.cutoff`), every compact/swap is one command that also rewrites mod-matrix
targets and automation lanes (the rack layer owns this mapping; the song file always stores plain slot indices).

More filters per track than 4: add filter effects after the instrument (per-track, not per-voice), for example EQ
(kind 67). More per-voice slots would be a chunk v2 change in `unisynth`, outside this change.

### D8b. Library catalog and the GM bank

The server builds the library catalog from the project presets, the GM bank (`dls` crate: names, bank, program,
regions), Tunefish program files, WaveSabre song blobs and the repo test patches, and serves it read-only with the bank
bytes. Lab sources are optional paths (`SYNTH_LAB`, `GM_DLS`); missing sources show as empty, not as errors. Loading a
legacy entry runs `synth-convert` once and stores the result as a project preset, so a song never depends on the lab.
GPL-3 Tunefish data is treated as a reference to convert and re-save, as in `research/softsynth-research.md`.
The mockup's `gen-catalog.py` is the prototype of this catalog builder.

### D8c. Velocity, mod wheel and pitch bend

Velocity is per note (the blob's note-on byte). The blob has no controller events, so mod wheel and pitch bend become
instrument parameters `ctl.wheel` and `ctl.bend` that the existing automation path carries (block-start linear
interpolation at the 100-frame sub-block, about 2.3 ms, smooth enough for wheel and bend). They are clip envelopes in
`song/1` so linked clips repeat them, and export flattens them into track automation. Rejected: new CC events in the
lane format (a `synth-mix` blob change for every engine). Audition wheel/bend go to the live host as parameter sets
and are never recorded.

### D8d. Editing model

Both editors share one snap setting, one undo stack of commands (each gesture or command = one entry) and two typed
clipboards (notes, clips). The insert cursor is set by a click on empty space; paste targets it, duplicate targets the
end of the selection. Clips never overlap: placing a clip trims, shortens or splits what it covers (Ableton's rule),
so export never sees overlaps. Paste makes unique clips by default and linked placements with Shift, matching
Ctrl+D / Ctrl+Shift+D. The app keeps the mockup's whole-song snapshot undo (one snapshot per gesture or command):
songs are small (tens of kB), every command is undoable without per-command inverse code, and each snapshot restore
is sent to the live host like any edit. Command objects stay an option if snapshots become too large.

### D8e. Transport UI and the mockup's preview voice

The UI clock follows the worklet's reported frame position; it never keeps time itself, so loop wraps and the
display match the audio. Song length is derived from content (grows on edit, never shrinks on its own) to match the
exported header length. The mockup has no engine, so it runs its own clock and a few WebAudio oscillators per
instrument type, only so edits can be heard. That is not the product's sound path.

### D9. Front-end stack: plain TypeScript + DOM/canvas, no framework

Matches `packages/editor` and `packages/shader-viewer` (Bun.build, no React). The app is the reviewed mockup ported
onto the real model: the piano roll grid and velocity lane are `<canvas>`; the arrangement clips, ruler and
automation lanes stay DOM/SVG as in the mockup, which is fast enough at intro scale (tens of clips, a few lanes) and
keeps every clip focusable. Canvas for the arrangement remains an option for much larger songs. Headers, transport,
device panels and mixer are DOM (focus, accessibility). The editor keeps the mockup's working model (bars, 1/16
steps, dB, sends on the sender) and `packages/daw/src/model.ts` converts it to and from `song/1` exactly. State: one `Song` object + command stack (`do/undo`) for undo; each command reports which
host messages it needs (D4). Tokens from `shader-viewer/src/client/style.css` (`--bg --panel --line --fg --dim --ok
--warn --err`, focus `#6aa7ff`).

### D10. Server

`packages/cli/src/server.ts` adds `/daw` (static `packages/daw/dist`, token meta injected like `/`) and `/api/music`
(`GET` list, `GET`/`PUT` file), `/api/library` and `/api/bank` (`packages/cli/src/music.ts`). Paths are confined to
`project/music` (no `..`, absolute paths or symlinks out); extensions whitelisted (`.song.json`, `.patch.json`,
`.song.bin`); 16 MB limit; `PUT` of `*.song.json` / `*.patch.json` validates through the engine module (`synth-wasm`,
the same `crates/song` code as `song check`, loaded in Bun) before writing; atomic write (temp + rename).

## Risks / Trade-offs

- Live vs bounce difference after edits → visible "edited since play" badge on the transport; bounce is the reference.
- Worklet budget: unisynth wavetable rebuilds every 2048 frames per voice are bursty → measure on a dense song;
  Worker+SAB fallback (D6).
- `synth-core` refactor (D3) touches a shipped spec → golden hashes as the gate.
- Names as keys (D1): renaming a parameter breaks songs → names are frozen with chunk v1; a rename needs an alias
  table in `crates/song`.
- Big songs with GSM samples in base64 make JSON large → acceptable (samples are rare; ≤ 255 per chunk).

## Migration Plan

New files only; no existing song files. `add-audio-to-synth`'s "assemble" stage may target `song/1` later.

## Open Questions

- Tracker pattern view as a second editor over the same clips (later change)?
- Should the intro editor's timeline read `song/1` bars/beats for snapping (later change)?
