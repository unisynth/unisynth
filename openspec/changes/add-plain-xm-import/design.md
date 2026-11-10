# Design

## Context

- Local material, never committed, lives in `~/Downloads/zenekk`:
  - the three XMs
  - `re/`: unpacked images, the emulation tools, a Python port of Fudge's generators and a C port of Theta's poespack
    (bit-exact to the emulation), `FINDINGS.md`
  - `compare/`: the A/B page
- What each intro stores and generates:

| Intro | Stored PCM | Generated |
|---|---|---|
| Fudge | 7, 14, 15 (8-bit, in the song blob) | 22 samples from two generators, one call each, nine float/int parameters per call |
| Theta | 1, 2, 3, 18 (4-bit packed in the image) | phrases of a 16-note sequencer (tracks 0–5), 3 additive pads, 1 chirp |
| Pure | 33, 34, 35 (4-bit packed in the image; Theta's 18, 2, 3) | 1–20, 22–32: voice phrases and the inline loop of 32 |

- How the engines work (behavioural summary; the full reading goes to `research/xm-softsynth-intros.md`):
  - **Fudge tone generator** (11 samples). A sine whose phase is either a slow sine (`depth·sin(2πi·rate/period)`) or
    a frequency-modulated carrier. Then a two-stage shaper: a 4× drive blend below ±8160, a fold-back above ±16320, and
    a blend toward a shaper value that carries over from sample to sample. Truncating arithmetic throughout.
  - **Fudge drum generator** (11 samples). A sine with a frequency that falls by an accelerating sweep; the same shaper
    family; an envelope that is linear, `1−sin` or `cos`. Lengths 256–1556 frames.
  - **poespack phrase voice:**
    - three 1024-entry wavetable oscillators (saw or square copies) at per-oscillator ratios and gains, a fourth table
      and LCG noise
    - an optional DC blocker or leaky integrator, and a gate after a percentage of the step
    - a 6-mode resonant filter (low and high variants with `2^(c/25)` or `2^c` laws), whose cutoff = base + an LFO
      table + a per-sample rise
    - a note-to-note slide at a fixed rate, and an 8-bit clip
    - notes come from a per-track table of 16 notes, gate percentages and slide flags
  - **poespack additive pads.** Summed saw or square partials with table-lookup vibrato, then the filter (Theta 4, 6,
    8). Plus a decaying sine chirp (17).
- The converter today:
  - `cauldron::import::convert_with` replays the XM (`xm/replay.rs`, FT2 subset) and integrates each trigger's phrase
    position per tick (`path`)
  - it unrolls the phrase's events (`unroll` via `phrase_events`) into notes of a clocked instrument and packs the
    tracks
  - stored samples become GSM sampler instruments
- The unified synth today:
  - the clocked slot (`us-osc-clocked`) runs the mini-cauldron's waves and filter at a phrase clock (playback ratio ×
    native rate)
  - resampling is Catmull-Rom with a reconstruction low-pass
  - phrase position comes from the note velocity (eighths of a 3530-frame step); latched bytes carry phase, age and
    retrigger
  - every clocked rule lives in the `unified-synth` spec, plus "Functional names"

## Goals / Non-Goals

**Goals:**
- A base that any precalc-sample intro can plug an engine into: map format, trigger→note path, the note byte contract,
  start position, parity.
- The XM replay features the three songs use.
- The TBL songs byte-identical.

**Non-Goals:**
- The engines (changes `add-fudge-softsynth`, `add-poespack-softsynth`) and publishing the songs.
- Bit-exact song output: the unified synth runs in f32, the originals in truncating x87. Per-sample parity is judged
  by the shared metrics.
- Committing original code, constant tables beyond the song's own patch numbers, images, ports, XMs or renders.

## Decisions

### D1. Generator map `xm-gen/1`

```json
{ "format": "xm-gen/1", "intro": "fudge-asc",
  "samples": [
    { "instrument": 1, "sample": 0, "kind": "generated", "engine": "fm-shaper",
      "length": 8192, "params": { "args": [1124073472, 1, 0, "..."] } },
    { "instrument": 7, "sample": 0, "kind": "stored" } ] }
```

- `params` hold the engine's record as the original computed it (a required object), with float32 values as their
  exact decimal and integer flags as integers. That keeps the `push 1` lesson: a flag is not a float.
- `length` must equal the XM sample's (so every position is inside the sample and within the byte contract); a
  listed sample must be non-empty. Identical generated records share one instrument (Fudge's 18, 19 and 24 reuse the
  data of 11, 13 and 11); an explicit alias key could be added later without breaking committed maps.
- **Phrase engines** add their events source: the per-track note table, a gate list and slide flags, plus the
  instrument block.
- **No clock field:** the playback rate comes from the XM (notes, slides, relative note, finetune); a track's
  phrase clock is `cauldron::R` × its class ratio, bend and transpose carry the rest. The engines work in sample
  frames (Fudge's generators have no rate); one needing a time base keeps it in `params`.
- Alternatives:
  - Extending the cauldron patch format: its schema is cauldron-specific, and two versions are already branched on.
  - Raw image offsets: these would commit addresses of a third-party program and are fragile.

### D2. Converter structure: one flow, two sample sources

Reading `cauldron::import::convert_with` showed that the conversion flow itself is what both importers share: the
replay and control rate, trigger paths, layer dropping and handovers, track packing (a pitch class per track, the
per-tick playback speed as bend + transpose lanes, which the clocked slot reads as its playback rate), the volume,
pan and bend lanes, stored-sample tracks, lead, length, master and track names. What is specific to the mini-cauldron
is narrow. So the flow is made generic over a **sample source** (trait `Source` in `xm/convert.rs`):

| `Source` item | Mini-cauldron (`cauldron::import`) | Generator map (`xmgen`) |
|---|---|---|
| `ids(&Xm)` → per XM sample `Option<(key, variant)>` (None = stored) | `identify` (records, variants) | map entries (`generated` → key = sample index, variant 0) |
| `decimation(key)` | the record's | 1 (paths in generated-sample frames) |
| `instrument(key, ratio)` → values | `instrument(version, record, ratio)` | the engine's instrument at phrase clock `R × ratio` |
| `chain(key, ratio, rate, tempo)` | `effects` + slide/slim low-pass, doubler restart, record 7 halving | none (empty chain) |
| `events(key, variant, ratio, &Path)` → `Vec<PhraseEv>` | `unroll` (phrase events) | `Engine::events`: one note per path piece (start position latched, re-entry at loop jumps, off at the end) or a phrase engine's events |
| `name(key)` | `record r` | the sample or instrument name |

The flow moves to `xm/convert.rs` unchanged in behaviour (the TBL identity guard, task 1.1, holds byte for byte). The
cauldron importer becomes `convert_with(patches, …) = xm::convert::convert(&CauldronSource, …)`. `xmgen` holds the map
parser, an `Engine` trait (`id`, `seekable`, `instrument`, `events`) and a registry; this change registers only a test
engine (`#[cfg(test)]`, it counts frames), so the trigger→note path is tested end to end before a real engine exists.
Generated notes write the D3 bytes as `latched_lane`s; the cauldron's `PEv::On` keeps its own bytes, so its lanes are
unchanged.

Rejected: moving only the pure helpers and giving the generator map its own copy of the flow (about 250 duplicated
lines, two places to fix every lane or timing rule).

### D3. Engines as oscillator types; the note byte contract

Decided during apply (doubt-driven review, user 2026-11-10), replacing an `engine` field in the clocked type with three
appended parameters:

- **Each engine is its own oscillator type** at a free odd position of the type choice (7, 9, … 23; Elevated's pair
  took 5), behind its own feature, named by function (a tone with a two-stage shaper, a sweep drum, a wavetable phrase
  voice, an additive pad, a chirp). It runs through the clocked machinery: the phrase clock (`ClockedT.clock`, the
  native clock × playback ratio), Catmull-Rom resampling and the reconstruction low-pass; only the frame function
  differs. Its nine shape values are the engine's own parameters. No parameter is appended, nothing migrates, and the
  clocked type (6) and its songs are untouched by construction.
- **The byte contract** (fixed here, written by the converter): at a non-legato note-on the slot reads
  `o::TRACKING` byte × 4096 + `o::PM_OUT` byte × 16 as its start frame and `o::FEEDBACK` byte ≥ 128 as a backward
  pass. The clocked type reads these bytes as age, phase and filter factor; an engine type reads them as above. The
  converter writes them as `latched_lane`s, as it writes the cauldron's age and phase bytes today. 16 frames is below
  the smallest loop-start step in the three XMs (the extraction tool warns otherwise); the largest position is
  1 048 560 + 4080 frames.
- **Scope and spacing** (doubt-driven review, task 3.7): the contract is for engines with one note per run of the
  sample (`tone_events`). Phrase engines bring their own events and lanes (`Engine::events`, `Engine::lanes`; `PEv` is
  internal, not stored format) in their change. A note whose bytes differ from the previous note's must start more
  than `LANE_LEAD` (128) frames after it, or the conversion fails: its bytes would step before the previous note-on
  reads its own. The three XMs' tightest re-entry is 255 frames (Fudge 19: a 256-frame loop at note 78), and a
  forward loop's re-entries repeat the same bytes.
- **Built later:** the engine types, their note-on handling and the seek of D4 arrive with the first engine
  (`add-fudge-softsynth`), where a real engine drives them. This change tests the contract at lane level.
- Rejected: an appended `engine`/`pos_hi`/`pos_lo`/`backward` block (a layout change frozen around a mechanism with no
  engine yet); carrying the position in the velocity (the cauldron uses it, and 7 bits are too few).

### D4. Seeking for stateful engines

- Fudge's generators carry a shaper value from sample to sample. Their lengths are short (≤ 65 536 frames), so they
  are **seekable by replay**: a start position or a backward pass is rendered by running the engine from frame 0 at
  note-on into the voice's scratch state.
  - This costs at most 65 536 engine steps once per note-on. The `performance-optimization` budget checks it, and a
    cache of the last seek per voice covers loops.
- poespack's filtered phrases are not seekable. A ping-pong backward pass is reversed event order with a warning, as
  for cauldron phrases.
- The engine declares which applies.

### D5. Replay extensions (FT2 semantics, openmpt as the arbiter)

| Feature | Rule |
|---|---|
| Volume column `0x8x`/`0x9x` | tick 0: `vol ∓= x`, clamp 0..64 |
| `Hxy` | ticks after the first: `global += x` or `−= y`, 0..64, memory per channel |
| `Pxy` | ticks after the first: pan `+= x` (right) or `−= y` (left), 0..255, memory |
| `X1x` / `X2x` | tick 0: period `−= x` / `+= x`, memories per direction |
| `Rxy` | FT2 counter and volume table (0/8 none, 1–5 −1/−2/−4/−8/−16, 6 ×2/3, 7 ×½, 9–D +1/+2/+4/+8/+16, E ×3/2, F ×2); new triggers through the `E9x` path. Pinned by the openmpt fixture: the counter runs on every tick across rows, except a note's tick 0, which resets it; a retrigger applies the change, then the row's set-volume column again |
| Auto-vibrato | type (sine, square, ramp down, ramp up), sweep, depth, rate per instrument; per tick from the trigger (tick 0 included): amplitude += `256·depth/sweep` up to the depth while the key is down, position += rate, period += `wave·amplitude / 2^14` truncated toward 0 (the fixture rules out a floor); added to the tick's period independently of `4xy` |
| `F00` | nothing (openmpt plays the row and the next at the same speed) |

Each feature has unit tests and a synthetic two-row XM rendered by `openmpt123`. The fixtures commit onset frames and
pitch tracks, not audio.

### D6. Map extraction from the running original

`tools/xm-gen/` (Python, unicorn + capstone, local venv like `research/tbl-mini-cauldron/tools`):
1. unpack the UPX 0.20 LE image
2. run the intro's sample calculation once, uninterrupted
3. hook each generator call to record the parameter state at entry: Fudge's pushed arguments; poespack's global
   parameter block and track table at each voice call, plus each inline instrument's loop parameters
4. mark samples stored or generated by which code wrote them: the raw-sample unpack or `memcpy` versus the generators

The maps come out of the original's own execution rather than from parsing instruction immediates, so a quirk such as
an integer `push 1` lands as the integer it is.

Pitfalls recorded from the probe:
- Never time-slice unicorn: resuming corrupts x87 state.
- Seed the LCG as `main` does.
- Prefill the allocator's handle table.

The maps are committed under `tools/xm-gen/maps/`, like `patches_*.json`.

### D7. Parity

- **Sample level:** an engine render at native clock, compared with the local reference render. The reference is the
  emulated original, or the bit-exact ports where available.
- **Song level:** `openmpt123` renders of the XM, with every generated sample replaced by the reference render of the
  build the map was extracted from. Theta's map comes from the 1998 compo executable, whose instruments 4, 6, 7 and 8
  differ from the later-build XM the user has, so its reference is the XM with the executable's samples.
- Shared metrics. Reports carry metrics only. `$ZENEKK_DIR` holds the material; when it is absent, the suite skips.
- In this change no engine renders: songs are pending, and the received XM is compared with the reference for
  reading (Theta: −0.06 dB, one band 2.06 dB off). Each engine change adds its per-sample parity and turns its songs'
  rows into judged ones.

### D8. Credits and publication (recorded for the engine changes)

| Song | Title | Author | Sources |
|---|---|---|---|
| fudge-alien-sex-clone | Alien Sex Clone | Simon (Fudge) | Demozoo production 21494 credits Simon for music. Demozoo music 202313 "Alien Seckse Clone (final version)" credits Echo and Simn; its Modland XM shares the order list and 86 % of note cells with the intro's. Both are stated. |
| quad-pure | Pure | Darkxceed (Quad) | Demozoo 21496 and the PURE.NFO, which also names poespack as his softsynth |
| quad-theta | Theta | G-Day (Quad) | pouet 4665 ("gday [Music]"); Demozoo 41468 has no credits; QUAD.NFO lists g-day among Quad's musicians |

- Titles are the productions': none of the XMs carries one, and the internal modules are named `phase4.mxm` and
  `spirit4.mxm`.
- Checked and ruled out by comparing the XMs:
  - Theta is not G-day's "Spiritual Feeling" (Wired 1998 music compo)
  - Pure is not Darkxceed's "Knock Down Your Brains"
- The songs follow the released-music rule and get a listening-page group, "GUS 64K intros, 1998 (softsynth XMs)".

## Risks / Trade-offs

- **Generalising the cauldron path** could change the TBL songs. Mitigation: byte-identity tests on songs and renders
  come before the move (1.1), and it is a pure function move.
- **The byte contract and `xm-gen/1` are frozen** once songs use them. Mitigation: `doubt-driven-review` before
  closing (task 4.4); no parameter layout change is involved.
- **Seek-by-replay cost** at note-on, up to 65 536 steps. Mitigation: a measured budget, plus a per-voice last-seek
  cache.
- **`Rxy`/auto-vibrato quirks.** Mitigation: openmpt fixtures and song parity on Fudge (169 `R`) and Theta.
- **f32 vs truncating x87:** the originals' truncation produces ±1-LSB-level differences and, on feedback-heavy
  instruments, slow drift. Judged by the shared metrics, not bit-exactness; drift that breaks a band is debugged per
  engine.
- **Scope:** three changes. This one delivers no audible song by itself. The value it locks in is that nothing
  generated gets stored.
