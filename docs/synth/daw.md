# DAW: song source, live host and browser editor

Change `add-synth-daw`. Compose music for the intro with the unified synth in the browser: the editor plays the
**same code the intro plays**, and the song lives in the repo as editable JSON next to `intro.json`. Specs:
`openspec/changes/add-synth-daw/specs/` (song-source, synth-live-host, synth-daw); decisions: its `design.md`.

## Pieces

```text
project/music/*.song.json  (song/1, schema/song.schema.json)       project/music/patches/*.patch.json (us-patch/1)
        │  crates/song: parse → validate (JSON-pointer issues) → export / import / presets / tables / render
        ▼
synth-mix blob (*.song.bin) ──► synth_core::mix::Player (the intro's player)  ─► song render, DAW bounce
        │
        └──► crates/synth-wasm Host = synth_core::mix::Mixer + an editable schedule  ─► DAW live playback
                    (same Mixer step as Player: bit-identical from frame 0 without edits)

packages/daw (served at /daw by packages/cli)
  main thread: engine module instance → tables, validate, export, import, presets; UI (app.js) on the editor model
  AudioWorklet: engine module instance → Host.render(128) per quantum; commands in, status (position, peaks, load) out
  Worker:       engine module instance → bounce (the exported blob through Player) → WAV
  /api/music:   list / read / validated write under project/music;  /api/library, /api/bank (GM.DLS)
```

| Piece | Where | Notes |
|---|---|---|
| `song/1` source | `crates/song` | Musical time (960 ticks per beat), clips placed on tracks (linked, looping), devices by parameter name, receives, automation lanes, a master. Deterministic export. |
| `us-patch/1` preset | `crates/song::preset` | Non-default values of one device, sorted by parameter index. |
| `song` CLI | `crates/song/src/main.rs` | `export`, `import`, `render` (16-bit or `--f32`, `--bank`/`GM_DLS`), `info` (tracks, devices, features), `check`. |
| Shared mixer step | `synth_core::mix::Mixer` | `Player` = `Mixer` + `BlobSource`; the live host = `Mixer` + its schedule. |
| Live host | `crates/synth-wasm` | Edits between calls (D4), transport with loop and note chase, audition, meters, no allocation in `render`. C ABI for wasm32. |
| Browser DAW | `packages/daw` | Arrangement, piano roll, generated device panels, mixer, library, files, bounce. |
| Server routes | `packages/cli/src/music.ts` | Loopback only; foreign `Origin` refused; paths confined to `project/music`; writes validated by the engine module. |

## Formats

`song/1` (abridged; full schema `schema/song.schema.json`):

```json
{
  "format": "song/1", "rate": 44100, "tempo": 128.0, "signature": [4, 4], "ticks_per_beat": 960, "length": 30720,
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
  (`osc2.gain`, `filt1.cutoff`, `env1.attack`, `mod3.target`, effect `mode`, `dry_wet`, …) and are frozen with unisynth
  chunk v1.
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


### GSM samples

A GSM sampler slot plays a sample stored inside its instrument (base64 in the device's `samples`; GSM 06.10, about
9 KB per second of 44.1 kHz mono). Get one from the library's **GSM samples** (every sample stored in a project song;
▶ plays it, Load copies it in) or with **Import sample…** on the instrument / **Import WAV…** on a GSM slot: the
browser decodes any audio file it can read, resamples to 44.1 kHz and mixes to mono; the engine encodes it
(`crates/gsm` encoder, bit-exact with libgsm 1.0.24). Imports are capped at 30 s. **Remove unused** drops samples no
slot plays, since each costs bytes in the intro.

## Using the DAW

```bash
bun packages/daw/build.ts                         # engine module + page into packages/daw/dist (also pnpm -r build)
bun packages/cli/src/cli.ts serve --no-headless   # then open http://127.0.0.1:6464/daw/
```

`?song=project/music/x.song.json` opens a song; `?tab=roll|dev|mix` a panel; `?nobank` shows the missing-bank state.

The song list also shows `project/music/released/`: 25 WaveSabre songs from released intros and executable music,
built locally by `python3 tools/ws-extract/make-demos.py` (git-ignored, no licence; see `project/music/NOTICE.md`).
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
