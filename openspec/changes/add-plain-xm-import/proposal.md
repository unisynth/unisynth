# Proposal

## Why

`~/Downloads/zenekk` holds the XM soundtracks of three 1998 64K intros. Each intro synthesized most of its instruments
at start-up with its own softsynth, then played them with the Cubic tiny GUS XM player:

| Intro | Release | Music | Synth |
|---|---|---|---|
| Fudge – Alien Sex Clone | The Party 1998, 1st PC 64K (pouet 444, Demozoo 21494) | Simon (Fudge, also Acme) | Fudge's own: an FM-tone and a drum generator |
| Quad – Pure | The Party 1998, 4th PC 64K (pouet 4666, Demozoo 21496) | Darkxceed (Quad) | Darkxceed's "poespack" |
| Quad – Theta | Wired 1998, 1st PC 64K (pouet 4665, Demozoo 41468) | G-Day (Quad; pouet credit) | poespack |

The engines were reverse-engineered locally (unpacked, emulated, ported). Re-run in one uninterrupted emulation, they
reproduce the XMs' samples: Pure 34/34 bit-identical, Fudge 23/25 (2 within ±1 LSB), Theta 23/27 (the XM is from a
later build than the 1998 compo executable).

The user asked for the three songs in the DAW's song list, with Demozoo and pouet information. The rule
(user, 2026-11-10): **a song may store only the samples the original stored.** Every sample the intro synthesized must
be played by live unified-synth instruments, as TBL's mini-cauldron phrases are (user choice: live instruments, not
generation at load). Stored in the originals:
- Theta: instruments 1, 2, 3 and 18 (4-bit packed PCM in the executable)
- Fudge: instruments 7, 14 and 15 (8-bit PCM in the song data)
- Pure: instruments 33, 34 and 35 (4-bit packed PCM in the executable; the same three as Theta's 18, 2 and 3)

This needs one shared base and two engines. The work is split so that each change lands green and no song ships
before its engine does:

1. **This change, `add-plain-xm-import`: the converter base.**
   - the XM replay features these songs use
   - a per-song *generator map* that says which XM samples are generated, by which engine, with which parameters, and
     which are stored
   - the converter path that turns a generated sample's triggers into notes, with the clock and position handling
     shared by all engines, and the byte contract those notes use
   - parity at sample level (engine vs reference renders) and at song level
2. **`add-fudge-softsynth`:** the engine oscillator types in unisynth (start position, backward pass, seek-by-replay)
   and Fudge's two generators as the first of them. Alien Sex Clone is published.
3. **`add-poespack-softsynth`:** Quad's poespack as a clocked engine (wavetable oscillators, noise, the 6-mode resonant
   filter, the LFO-table cutoff, slides, gate). Its additive and chirp instruments follow. Theta and Pure are
   published.

`synth-convert xm` cannot take these XMs today. It requires mini-cauldron patch data, and its FT2 replay
(spec "XM control replay") covers only what the TBL songs use. On Pure it stops at "volume column 0x91".

| Missing XM feature | Fudge | Pure | Theta |
|---|---|---|---|
| Volume column fine volume slide down/up (`0x80–0x9F`) | 846 | 24 | 274 |
| `Rxy` multi retrig with volume change | 169 | – | – |
| `Hxy` global volume slide | – | 16 | – |
| `Pxy` panning slide | – | – | 62 |
| `X1x`/`X2x` extra-fine portamento | – | – | 6 |
| Instrument auto-vibrato | – | – | 1 instrument |
| `F00` (openmpt ignores it; the replay rejects it) | – | – | 1 (last pattern) |

## What Changes

- **Replay extended, FT2 semantics:**
  - volume column fine volume slide down/up
  - `Rxy` multi retrig with its volume change
  - `Hxy` global volume slide
  - `Pxy` panning slide
  - `X1x`/`X2x` extra-fine portamento
  - instrument auto-vibrato
  - `F00` ignored, as openmpt does

  Anything outside the set stays an error that names it.
- **Generator map:** `synth-convert xm <in.xm> <map.json> <out.song>`, where the map is a new JSON format
  (`xm-gen/1`). Per XM sample it records:
  - `stored`, or
  - `generated` with the engine id, the engine's parameter record, the sample's native generation clock and its
    length

  The mini-cauldron patch files keep working unchanged (the converter tells the formats apart by their keys).
  - Stored samples become sampler instruments on their GSM-encoded PCM, as Stash's drum kit does today.
  - A generated sample whose engine is not in the build is an error that names the engine and the sample. Nothing is
    ever stored in its place.
- **Generated triggers become notes**, engine-neutral, building on the clocked slot (`us-osc-clocked`):
  - the slot's clock follows the trigger's playback ratio per tick, as the cauldron phrases do
  - a trigger's start, sample offset, loops and end become note-on, latched start position, loop re-entries and
    note-off
  - per-engine *events* (a phrase engine's internal notes) unroll along the trigger's path through a callback, as
    `phrase_events` does for the cauldron
  - generalising today's cauldron-only clock and position machinery is behaviour-preserving: the TBL conversions stay
    byte-identical
- **Generated-note byte contract** (design D3): each engine will be its own oscillator type at a free position of the
  type choice, run through the clocked machinery (phrase clock, Catmull-Rom resampling, reconstruction low-pass). Its
  notes carry the start position in the slot's per-note bytes (`o::TRACKING` ×4096 frames, `o::PM_OUT` ×16 frames)
  and a ping-pong backward pass in `o::FEEDBACK`, latched at note-on. This change fixes the contract, and the
  converter writes it; the oscillator types and their note-on handling arrive with the first engine
  (`add-fudge-softsynth`). No parameter is added and the clocked type (6) is unchanged.
- **Map extraction:** `tools/xm-gen/` holds the scripts that read each intro's unpacked image and write its map. That is
  Fudge's 22 call-site parameter sets and poespack's track tables and instrument blocks. Only the maps (numbers that
  are the song's patch data, like `patches_*.json`) are committed; the images, ports and XMs stay local.
- **Parity:** each converted song against an `openmpt123` render of its reference XM (the XM with the map build's
  generated samples, written locally by `tools/xm-gen`), with the shared metrics; a song whose engine is not built yet
  is pending. Material from `$ZENEKK_DIR`; absent → skip. Per-sample engine parity comes with each engine's change.
- **Docs:** `research/xm-softsynth-intros.md` (provenance, method, a behavioural reading of both engines in our own
  words, match results; no copied code or tables) and a `docs/synth/unisynth.md` section on generator maps.

Out of scope here:
- the engines themselves (changes 2 and 3)
- publishing the songs (each engine change does that)
- Haujobb's Discloned: it is sample-based, and its final release ships `DISCLONE.IT`
- XM features no song uses: Amiga frequency table, the rest of the volume column, `Txy`, `Kxx`, `Lxx`, `Yxy`
- committing XMs, executables, the local ports, renders or MP3s

## Discipline Skills

- `doubt-driven-review`: the `xm-gen/1` format and the note byte contract are frozen once songs use them.
- `systematic-debugging`: parity gaps against the references.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `synth-convert`: the extended XM control replay, the generator map, generated triggers as notes and their byte
  contract, stored samples without patch data, map extraction, parity.

## Impact

- **Code:**
  - `crates/synth-convert`: `xm/replay.rs`; a new `xmgen` module (the map plus the trigger→note path generalised from
    `cauldron/import.rs`); `main.rs`
  - `tools/xm-gen/` and `tools/unisynth-parity` (plain-XM suites)
- **Existing songs:** none change; the TBL conversions and their renders stay byte-identical.
- **Theta's samples:** with live engines, Theta plays the 1998 compo executable's own parameters. Its instruments
  4, 6, 7 and 8 therefore follow the executable, not the later-build XM the user has. The song-level parity judges
  those instruments against the executable's rendering instead of the XM's (recorded in the report).
