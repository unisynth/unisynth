# DAW: song source, live host and browser editor

Change `add-synth-daw`. Compose music for the intro with the unified synth in the browser: the editor plays the
**same code the intro plays**, and the song lives in the repo as editable JSON next to `intro.json`. Specs:
`openspec/changes/archive/2026-10-30-add-synth-daw/specs/` (song-source, synth-live-host, synth-daw); decisions: its `design.md`.

## Pieces

```text
project/music/*.song.json  (song/2, schema/song.schema.json)       project/music/patches/*.patch.json (us-patch/2)
        │  crates/song: parse → validate (JSON-pointer issues) → export / import / presets / tables / render
        ▼
synth-mix blob (*.song.bin) ──► synth_core::mix::Player (the intro's player)  ─► song render, DAW bounce
        │
        └──► crates/synth-wasm Host = synth_core::mix::Mixer + an editable schedule  ─► DAW live playback
                    (same Mixer step as Player: bit-identical from frame 0 without edits)

packages/daw (served at /daw by packages/cli)
  main thread: engine module instance → tables, validate, export, import, presets; UI (SolidJS components in src/ui, session logic in src/ui/app, started by src/ui/App.tsx) on the editor model
  AudioWorklet: engine module instance → Host.render(128) per quantum; commands in, status (position, peaks, load) out
  Worker:       engine module instance → bounce (the exported blob through Player) → WAV
  /api/music:   list / read / validated write under project/music;  /api/library, /api/bank (GM.DLS)
```

| Piece | Where | Notes |
|---|---|---|
| `song/2` source | `crates/song` | Musical time (960 ticks per beat), clips placed on tracks (linked, looping), devices by parameter name, receives, automation lanes, a master. Deterministic export. |
| `us-patch/2` preset | `crates/song::preset` | Non-default values of one device, sorted by parameter index. |
| `song` CLI | `crates/song/src/main.rs` | `export`, `import`, `render` (16-bit or `--f32`, `--bank`/`GM_DLS`), `info` (tracks, devices, features), `check`. |
| Shared mixer step | `synth_core::mix::Mixer` | `Player` = `Mixer` + `BlobSource`; the live host = `Mixer` + its schedule. |
| Live host | `crates/synth-wasm` | Edits between calls (D4), transport with loop and note chase, audition, meters, no allocation in `render`. C ABI for wasm32. |
| Browser DAW | `packages/daw` | Arrangement, piano roll, generated device panels, mixer, library, files, bounce. |
| Server routes | `packages/cli/src/music.ts` | Loopback only; foreign `Origin` refused; paths confined to `project/music`; writes validated by the engine module. |

## Formats

`song/2` (abridged; full schema `schema/song.schema.json`):

```json
{
  "format": "song/2", "rate": 44100, "tempo": 128.0, "signature": [4, 4], "ticks_per_beat": 960, "length": 30720,
  "clips": [{ "id": "bass-a", "length": 3840, "notes": [[0, 220, 36, 100], [240, 220, 36, 80]] }],
  "tracks": [{ "id": "bass", "name": "Bass", "color": "#61afef", "volume": 0.6, "mute": false,
    "receives": [{ "from": "kick", "channel": "side", "volume": 0.5 }],
    "devices": [{ "kind": 64, "preset": "Acid Bass", "values": { "osc1.type": 0.5, "filt1.type": 0.625, "filt1.cutoff": 0.32 } },
                { "kind": 66, "values": { "mode": 0.0, "vertical": 0.35 } }],
    "placements": [{ "clip": "bass-a", "start": 0, "length": 30720, "loop": true }],
    "lanes": [{ "device": 0, "param": "filt1.cutoff", "points": [[0, 0.25], [15360, 0.6]] }] }],
  "master": { "id": "master", "name": "Master", "volume": 1.0, "mute": false,
    "receives": [{ "from": "bass", "channel": "main", "volume": 1.0 }], "devices": [], "placements": [], "lanes": [] }
}
```

- **Notes** `[start tick, length ticks, pitch, velocity]`; **points** `[tick, value 0..1]` (ticks may be fractional:
  import keeps exact sample times). Values are normalized by parameter name; names come from the engine's tables
  (`osc2.gain`, `filt1.cutoff`, `env1.attack`, `mod3.target`, effect `mode`, `dry_wet`, …) and follow the unisynth
  v3 layout. A `song/1` file (the layout before change unify-v2-algorithms) is migrated once when it is read (`song
  migrate` saves it; the DAW opens it through the engine's `song_migrate`); a `song/2` file with an old name is an
  error.
- **Export** (`song export`, the DAW's Export and live playback): tick → sample `round(tick·60·rate/(tempo·960))`;
  placements flattened into one lane per track (loops repeat every clip length, cut at the placement end); one open
  note per pitch (an earlier note is cut at the next start); at one sample note-offs come first, then note-ons in
  pitch order; automation bytes `round(value·255)`; muted tracks export volume 0 and no events; bypassed devices and
  their lanes are left out; the master is the last blob track.
- **Import** (`song import`, the DAW's Import blob…): devices, routing, volumes and automation come back exactly;
  legacy kinds (0–39) fail with a pointer to `synth-convert`. Each lane is cut into **patterns** (`crates/song/src/patterns.rs`,
  change add-import-patterns): per lane the 2-, 4- or 8-bar grid with the least note data (notes in distinct clips + 8
  per placement; ties → longer); a grid line inside a note is never a cut (the segment grows); identical segments share
  one clip, back-to-back repeats become one looping placement, empty segments get none, the last placement ends at the
  song end. A segment sharing ≥ ½ of the larger note set (same pitch, start within 1/64 bar) with an earlier pattern of
  the same length is its **variant**: clip ids `<track id>/A`, `/B`, `/A.1`, `/A.2` (the DAW shows `A`, `A.1`). Tracks
  sharing a lane get the same placements. The export is byte-identical to the one-clip import (`--one-clip`: one clip
  per lane at tick 0), checked on the demo songs and all 52 WaveSabre lab songs.
- **Import config** (`song import --meta <file>`): the `meta` fields plus optional `about` (the song) and `tracks`
  (`[{name, about?}]`, one per blob track before the master; a count mismatch fails, nothing written).
- **Descriptions** (`about` on the song and on tracks, never exported): `type` (song: genre; track: role such as kick,
  bass, lead, pad, arp, bus), `style`, `tempo` (song: BPM and feel; track: rhythm), `engine` (original engine and how
  it makes the sound), `similar_to` (real-world synths / drum machines), `description`. The DAW shows a track's as the
  tooltip of its name; `song info` prints the song's and each track's type. The demo songs' names and descriptions
  live in `tools/demo-songs/meta/<name>.json` (WaveSabre names from the original project's `.ref`, Tunefish names
  given from the analysis).
- **Validation** reports every problem with its JSON pointer: unknown kind or parameter
  (`/tracks/1/devices/0/values/osc1.cutof: unknown parameter`), ranges, references, receive cycles, a receive from
  the master ("the master cannot send; route through a bus before the master"), duplicate lanes ("parameter already
  automated on this track"), notes and points outside the clip or song.

## Live playback vs bounce

| | Live (AudioWorklet) | Bounce (Worker), `song render` |
|---|---|---|
| Code | `Host`: `Mixer` + editable schedule | `Player` on the exported blob |
| Equal to the intro | Bit-identical from frame 0 when nothing was edited since play (tested at call sizes 1–4096, and wasm vs native on the demo song) | Always (the WAV equals `song render` byte for byte) |
| After edits | Applied per track at the next 100-frame sub-block: values via `set_param`, structural changes rebuild that track's devices, note edits swap that track's schedule, routing rebuilds the mixer; the transport shows "edited since play" | — |
| Seek / loop | Devices reset at the target, held notes re-triggered ("chase"); no silent pre-roll | Renders from frame 0 |
| Threads | One (AudioWorklet) | `song render --threads <n>` renders independent tracks on `n` threads, same WAV byte for byte (change add-synth-threads). The browser bounce uses `synth-mt.wasm` on up to 8 Web Workers when the page is cross-origin isolated (the local server sends COOP/COEP), else one thread; same samples either way (change add-wasm-threads) |

Load (task 3.7, Chrome, main-thread timing of the same exports the worklet calls, `packages/daw/bench/dense.ts`):
the demo song needs 0.1 ms per 128 frames on average, 0.7 ms worst case (24 % of the 2.9 ms budget). Stress songs
with hundreds of voices exceed the budget on sustained CPU (natively as well), which a Worker + SharedArrayBuffer
fallback would not absorb, so it is not built; engine speed is `add-unified-synth` task 7.4.


### Engine load readout and CPU

The **Engine** figure is the audio engine's mean render time as a share of real time, averaged over about half
a second (the worklet clock has 1 ms resolution, so single calls cannot be timed). It drops to 0% a few seconds
after Stop, when the effect tails have run out and the engine stops rendering. Measured in Chrome while playing:
`demo` ~5%, `tf4testsong` ~12%, `elysian` and `engage` ~30% (32-42 tracks, 93-153 devices). Voices are freed
when their first envelope ends (`cargo run --release -p synth-wasm --example voices -- <song.json>` prints
voices against held notes); on the converted songs most of the time goes to the effect chains, which process
every sample whether or not their track is sounding, as in the WaveSabre player.

Clicks or dropouts while the Engine figure is low come from the machine, not the engine: a 128-frame block of
`engage` never needs more than 1.6 ms (budget 2.9 ms; `--example spikes` times every block over three passes), but
when the computer is busy the audio thread can be paused for 5-40 ms. The **Buffer** setting next to the Engine
figure trades that tolerance against keyboard-audition delay: Low ~5 ms, Normal ~10 ms, **Safe ~23 ms (default)**,
Max ~50 ms. It is remembered, and changing it while playing keeps the position.

**Run open filters** (checkbox after Buffer, remembered) is a listening switch for comparing with WaveSabre renders.
To save CPU, the unified synth skips a low-pass that is fully open with no resonance (`flt::transparent`). The
original engines still run it, and WaveSabre's state-variable low-pass takes about 4 dB off at 16 kHz even fully open.
With the switch on, live playback runs those filters too. On Dope On Wax, the 16 kHz octave goes from +2.4 dB to 0.0 dB
against WaveSabre's own C++ render, every band ends within 0.3 dB, and render time rises about 3 %. Only live
playback changes: the song file, Bounce, Export and the intro are not affected.

**N drops** after the Engine figure counts audio dropouts since the last Play: the browser's own count of output
underruns (`AudioContext.playbackStats`, Chrome; hidden where the browser has none), i.e. moments the sound card got
no samples in time and played silence. It turns red; its tooltip gives the milliseconds of silence and the song
positions where new drops appeared (the counter updates about once a second, so ±1 s). The first 1.5 s after Play
are not counted (a just-started output often underruns once before any sound). Drops are a playback problem, never
in a bounce: raise Buffer or free the machine.

A load that climbs while playing and only falls on Stop → Play is state building up. One case is fixed: WebAssembly
cannot switch the CPU into flush-to-zero mode (the native player does, `denormal::Guard`), so effect feedback decaying
into subnormal floats made every later sample many times slower on x86. `engage` went from ~45 % at bar 37 to over
100 % by bar 41. Feedback state, filter state and each device's output now pass `denormal::flush` in wasm builds
(nothing natively; wasm bounces still equal the native render). `bun packages/daw/bench/perbar.ts` (`SONG=…`,
`RESTART=<bar>`, `MUTE`/`KEEP` to bisect) prints the load per bar.


### GSM samples

A GSM sampler slot plays a sample stored inside its instrument (base64 in the device's `samples`; GSM 06.10, about
9 KB per second of 44.1 kHz mono). Get one from the library's **GSM samples** (every sample stored in a project song;
▶ plays it, Load copies it in) or with **Import sample…** on the instrument / **Import WAV…** on a GSM slot: the
browser decodes any audio file it can read, resamples to 44.1 kHz and mixes to mono; the engine encodes it
(`crates/gsm` encoder, bit-exact with libgsm 1.0.24). Imports are capped at 30 s. **Remove unused** drops samples no
slot plays, since each costs bytes in the intro.

### Device panel and live view

The instrument panel has a fixed layout (change add-device-live-view): Voice + Noise, Oscillators 1–3, Filters 1–4
(serial), Envelopes 1–4, LFOs 1–2 and a 12-row mod matrix. Every slot keeps its place; a slot that is not set (type
Off, or all defaults and not routed) starts collapsed to one header line with its type control, and setting the type
expands it. ✕ resets one slot in place: mod rows that targeted it switch off and its automation lanes go, later slots do
not move. Only filters reorder (⇅ between two neighbours; mod targets and lanes follow), because their order changes
the sound. ▸/▾ collapses a slot (for the session) or a block (remembered in the browser, not in the song). Each block
has its own colour; envelope and LFO headers show what the mod matrix routes them to.

Active slots show a screen drawn from their parameters (oscillator waveform, filter response with the cutoff, the
envelope as the engine plays it, one LFO cycle). While the selected track's instrument sounds, the screens show it
live, one colour per voice (chips with the note names in the header, at most 16 voices, loudest first, then "+N"):
sampler playheads, a dot per voice on each envelope (and a level bar), a dot per voice on each LFO, and on each filter
the input spectrum in grey, the output in the filter colour and a tick at each voice's cutoff after modulation. Dots
at the same spot stack. **Live** in the instrument header turns the live view off (remembered in the browser); then no
track is watched and the engine does no probe work.

The data comes from the engine: `unisynth`'s host-only feature `probe` (never in the player) reads each voice
(note-on id, note, amplitude, release, age, envelope levels and stages, LFO phases, sampler positions, cutoffs) and,
for the watched instrument only, sums each filter slot's input and output over the voices into a 2048-frame ring.
The live host watches one track (`probe_watch`); the worklet adds the flat probe (`probe_len`/`probe_ptr`, layout in
`crates/synth-wasm/src/lib.rs`) to its ~30 ms status. Watching never changes a sample (the instrument goldens and the
live-vs-player tests run with taps on). Cost (`bench/dense.ts`, `WATCH`/`AB`): taps on the watched instrument +0.5–3 %
render time; the page's spectra and overlays 0.5 ms per status and 0.5 ms per frame at 16 voices and 4 filters. The
tap branch in the voice loop makes the DAW engine about 5 % slower even when nothing is watched; this is accepted
(the player is unaffected: same i686 size, no probe code).

### Mod wheel, pitch bend and converted library entries

A clip's mod-wheel and pitch-bend envelopes (controller lane) export as automation of the instrument's `ctl.wheel` /
`ctl.bend`, per placement and loop pass, back to rest (wheel 0, bend centre) at the placement end; a track lane on the
same parameter is a validation error. The bend lane shows semitones from the instrument's `ctl.bend_range`. The MW/PB
sliders and Web MIDI CC1 / pitch bend set the armed track's controllers live (not recorded; PB springs back).

Tunefish 4 programs and the instruments of the WaveSabre reference songs load from the library once converted:
`cargo run --release -p synth-convert --example library` writes `project/music/.library.json` (git-ignored, from the
local lab); the server attaches each entry's devices to `/api/library`. Loading replaces the track's instrument and puts
a Tunefish program's insert effects right after it.

## UI structure

TypeScript (strict) + SolidJS (change daw-solid-ts). `src/main.ts` boots the engine and tables, then
`src/ui/App.tsx` `start(daw)` mounts the panels and wires the session modules. Per-file detail: the `AGENTS.md` in
each `src/ui/` directory.

| Layer | Where | Rule |
|---|---|---|
| Model helpers | `src/ui/model/` | Pure: parameter tables, value text (`fmtVal`), track and device helpers, base64/GM. |
| Song store | `src/ui/state/song.ts` | One Solid store of the editor model. Components read through it; hot paths read `raw()`. Restores reconcile per top-level key (`restoreSong`) or per track (`restoreTracks`), never at the root. |
| Display state | `src/ui/state/` | Signals and small stores for view state: selection, tabs, scroll/zoom and row windows (`arrange.ts`), playhead, meters and automation values at the playhead (`live.ts`, `device.ts`), audition, and the layout (splitter sizes, persisted via `persist.ts`). Never part of the song, never undone. |
| Session logic | `src/ui/app/` | The only code that writes the song (`test/write-ratchet.test.ts` counts the writes and only lets the count fall). Edit core `core.ts`: `snap()` before a gesture, `commit(before, msg)` after it (one undo step), `undo`/`redo`. |
| Components | `src/ui/<panel>/` | Transport bar, files, library, arrangement (`arrange/`), piano roll, device panel, mixer, footer, shared knob/splitter/menu/dialog (`common/`). Same ids, classes and ARIA as before the port. |

**Undo snapshots** (design D2). A `Snap` keeps each track's body and its lane headers as JSON, with one cached
string per lane's points (keyed by the points array). Lane points are copy-on-write: an edit assigns a new array and
never changes one in place. Verify mode, on in browser tests, re-serializes cached lanes and throws on a mismatch.
Undo restores only the tracks that differ and refills only their automation rows (`restored(ids)`). The history has
an epoch, bumped on a song load, so a gesture or async action that began before the load commits nothing.

**Mounting and windowing.** The Device panel and the piano roll exist only while their tab shows (design D5). The
arrangement mounts the rows in view (`windowRows`) and the clips of the bars around it (`barWindow`). An automation
lane draws its line thinned per pixel column and builds point handles only around the view, at most the lowest and
highest point per 4 px. A press still takes the nearest point; the keyboard reaches every point after zooming in.
Selects with more than 16 options (mod source/target) hold only the current option until first used, so tests
`focus()` them before `selectOption`. Lane SVGs, the roll canvas and the device screens are drawn imperatively; the
rest is components.

**Budgets.** `test/budgets.test.ts` checks the spec budgets on a generated 50-track song and on a dense-automation
song (≈ 6 MB of lane points). Every edit (select, fader, knob, mute, solo, send level, select with the Device tab
shown, undo) must finish within one 60 Hz frame (16.7 ms), and 95 % of frames during playback must stay under 20 ms.
`bench/ui.ts` measures released songs, and `bench/ui-profile.ts` (`SELECT`, `TAB`, `SCROLL`, `PLAY`, knob + undo)
gives a CPU profile per caller. The production build pins Solid's production runtime (`vite.config.ts`).

## Using the DAW

```bash
bun packages/daw/build.ts                         # engine module + page into packages/daw/dist (also pnpm -r build)
bun packages/cli/src/cli.ts serve --no-headless   # then open http://127.0.0.1:6464/daw/
```

`?song=project/music/x.song.json` opens a song; `?tab=roll|dev|mix` a panel; `?nobank` shows the missing-bank state.

**Dev loop (hot reload).** The page is SolidJS built by Vite (change daw-solid-ts). For UI work, run the CLI server for `/api` and the engine files, and Vite in front of it:

```bash
bun packages/daw/build.ts                                         # once: engine modules + worklet/workers in dist
bun packages/cli/src/cli.ts serve --port 6474 --no-headless       # /api, synth*.wasm, worklet.js, bounce-*.js
DAW_API=http://127.0.0.1:6474 pnpm --filter @64k/daw dev          # then open http://127.0.0.1:6465/daw/
```

Vite serves `src/` with hot reload (a component edit swaps in place; the song keeps playing), sends the same COOP/COEP
headers (threaded bounce works) and proxies the rest to `DAW_API`. Keep Solid stores in their own `*-store.ts` / `state/`
modules: a hot-reloaded module re-runs, and a store defined next to components would start empty. Type-check with
`npx tsc -p packages/daw/tsconfig.json`; UI cost with `ROOT=<repo> SONG=<song> bun packages/daw/bench/ui-mixer.ts`.

The song list also shows `project/music/released/`: 25 WaveSabre songs from released intros and executable music
(and, in `released/v2/`, 347 Farbrausch V2 songs from `tools/v2-extract/make-demos.py`),
built locally by `python3 tools/ws-extract/make-demos.py` (no licence; committed while the authors are asked for permission, see `project/music/NOTICE.md`).
Songs with Adultery (GM.DLS) instruments sound as released only with the Windows bank: start the server with
`GM_DLS=<path to gm.dls>`.

**Static preview.** `GM_DLS=<gm.dls> bun packages/daw/build-static.ts` writes a server-less copy (page + snapshot of
songs, library and bank) to `packages/daw/build/dsp`; it runs read-only (Save downloads the song).
`packages/daw/publish-preview.sh` uploads it to <https://preview.judo.technology/dsp/>, which is unlisted but not
private: it contains the GM.DLS bank and the released songs, so it is for the owner's testing only.

| Keys | Action |
|---|---|
| Space · Home · L · Ctrl/⌘+L · F | Play/stop · to the loop or song start · loop on/off · loop the selection · follow the playhead |
| Ctrl/⌘ + X C V D, Shift+V/D linked, Delete | Clip or note clipboard, duplicate, delete (the editor with focus) |
| Arrows, Shift = coarser, Alt = length · Q | Move/transpose notes, move clips (↑↓: to another track) · quantize / align |
| Ctrl/⌘ + Z / Shift+Z | Undo / redo (every gesture is one step) |
| Ctrl/⌘ + wheel or pinch · − / + · Shift+Z | Zoom at the pointer · zoom · fit |
| Z S X D C V G B H N J M · Q 2 W 3 E R 5 T 6 Y 7 U I 9 O 0 P | Notes on the selected track, tracker layout by physical key (Z = C, Q = C above) |
| [ ] · ; ' · ` | Octave · velocity · computer keyboard on/off |
| ● or Shift+R | Automation record: knobs turned or held while playing write their lanes (touch) |
| ⇥ Follow / F | Page along with the playhead (on; a hand scroll pauses it until the playhead reaches the edge) |
| F2 · Alt+↑↓ · Ctrl+Delete · Shift+F10 | Rename · move · delete a track · menus |
| Ctrl/⌘ + S | Save (validated; errors list their JSON paths) |

Not yet: mod-wheel/pitch-bend clip envelopes reach the engine once unisynth has its controller block
(`ctl.wheel`/`ctl.bend`, `add-unified-synth` task 6a); loading Tunefish/WaveSabre library entries needs
`synth-convert` instrument conversion (tasks 6.x there); the GM sampler plays one wave over all keys until the
DLS-instrument source (6a.2) lands.
