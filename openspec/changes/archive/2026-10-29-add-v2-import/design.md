# Design

## Context

See proposal.md (Why) and `research/v2-synth.md` (format versions, corpus survey, gap matrix).

The current state that shapes the approach:

- `unisynth` kind 64 has 184 parameters. The chunk v1 stores `u8 n` and then `n × (u8 index, f32 value)` pairs, so as
  written it holds at most 256 parameters and 255 pairs. No chunk written so far uses an index byte above 183 or a
  count of 255. `ext::NOISE_WIDTH` (183) is the precedent for appending a parameter.
- The mod matrix has 12 slots with Scale/Offset and target ids 0–77, frozen with chunk v1. Scale and Offset act around
  the parameter **laws** (`law.rs`), so a matrix amount means different things for different laws.
- V2 evaluates its matrix once per 128-sample frame (at 44.1 kHz). The rule is
  `param = clamp(raw + (amt − 64)/64 · source, 0, 128)` in **raw 0..128 space, before** any law, for voice parameters
  and, from the newest voice, for channel parameters.
- `synth-mix` receives read the sending track's main pair into the main pair (0) or the sidechain pair (2).
  Devices have no further outputs.
- Reference: v2redux `v2dump` renders every version at its own era's behaviour. It is external, CC0, and built at
  `~/Downloads/64k-tools/synth-lab/v2redux`. The original sources: fr_public `v2/` (`synth_core.cpp`, `ronan.cpp`,
  `sounddef.h`) are public domain.

## Goals / Non-Goals

**Goals:**
- The acceptance songs pass the existing parity metrics against v2redux.
- Every V2 module is its own `us-*` feature. A non-V2 song's build and output are unchanged; the self-goldens prove it.
- One mod matrix carries WaveSabre, Tunefish and V2 routings.

**Non-Goals:**
- Bit-exactness with V2, and V2's x87 transcendental quirks. We use our `fm.rs` functions.
- A V2-native player or a V2 device kind next to kind 64.
- Per-version behaviour beyond what parity on the acceptance songs requires (see D9).

## Decisions

### D1. Port from the public-domain sources, verify against v2redux
Port the DSP from `synth_core.cpp` (ryg's readable C++ port of `synth.asm`), reading v2redux for era differences.
Both are public domain or CC0, so the code may be translated directly. Add a "Port of V2 by kb, public domain" note
per file. v2redux stays outside the repo and serves only as the reference renderer and loader oracle.
*Alternative*: a clean-room behavioural spec, as for Tunefish (GPL). This is not needed here, because the license
allows direct porting.

### D2. V2 functions are slot types and modes, read with V2's laws
V2 modules become new types and modes of the existing slots: oscillator type `v2`, filter types `v2-svf-*` and
`v2-moog-*`, envelope mode `v2`, LFO shapes and flags. A slot of a V2 type reads its existing fields (coarse/fine/gain,
cutoff/resonance, envelope times) as **V2 raw / 128** through V2's law. This uses the per-type field meaning that
already exists for `shape`. As a result the Add operation (D3) is exactly V2's rule in V2 slots.
*Alternative*: one V2 voice block that replaces the slots. It would be simpler to port, but it would break the
"unified" model: V2 filters would be unusable on a wavetable, and a second voice path would cost size.

### D3. Matrix: Add operation, controller sources, appended targets, 16 slots
- Operation **Add** (choice id 1, see D4a). Per control evaluation, each Add slot in slot order sets
  `v = clamp(v + amount·source, 0, 1)` on the normalized value of its parameter target, then the Scale product, then the law, then the Offset sum. With no Add slot, nothing changes, so old patches are
  bit-identical. Add amount is −1..1 (V2 `(amt − 64)/64`). Sources are 0..1 (V2 values / 128).
- Sources appended: breath (CC2), CC3–CC6, volume (CC7), next to the wheel (CC1). These are controller parameters
  like `ctl::WHEEL`, set by automation from the song's CC lanes. V2's "Note" maps to `key`; amp EG and EG2 map to
  envelopes 1 and 2; LFO1/2 to LFOs 1/2 with V2's polarity/amplify output (the LFOs requirement).
- Targets appended after 77: one id per appended V2 parameter that V2 marks `isdest`, plus the existing parameters
  V2 can reach that have no id yet (for example envelope attack, sustain, release, LFO rate).
- Slots 13–16 are appended parameters (4 × 4). The survey's largest patch has 14 entries, so the converter's
  drop-smallest rule never fires on the known corpus.
- Channel-stage targets read the sources of the instrument's newest sounding voice and are evaluated once per control
  block at instrument level (V2's `storeChanValues` uses `voicemap[chan]`).
- The control rate stays 32 frames, finer than V2's 128. Parity is the judge of this.
*Alternative*: a second, V2-only matrix. The user asked for one matrix.

### D4. Escaped parameter indices and counts (chunk v1, compatible)
The chunk keeps version 1 and reads indices and the pair count with a prefix escape, like UTF-8 lead bytes:

```text
pair count:  c < 0xFF        → n = c                                  (1 byte)
             c = 0xFF        → n = next u16 LE                        (3 bytes)
index:       b < 0xF0        → index = b                              (0..239, 1 byte)
             b = 0xF0 + k    → index = 240 + 256·k + next byte        (k = 0..15 → 240..4335, 2 bytes)
then f32 value
```

- **Compatible**: every chunk written before this change has `n < 255` and indices ≤ 183, so it decodes to the same
  values. The self-goldens, the effect goldens and `us-bake-check` prove it.
- **Canonical**: pairs are strictly increasing by *decoded* index. Each index and each count has exactly one valid
  form: a 2-byte index always decodes to 240 or more, and an escaped count below 255 is `BadChunk`. The encoder always writes the
  shortest form. The same patch therefore still gives the same bytes.
- **Bounds**: `COUNT` may grow to 4336. An escape byte at the end of the chunk, or an index ≥ `COUNT`, is `BadChunk`.
  Truncation at every byte is tested.
- **Player cost**: with escapes, `pairs()` and the `us-trusted` fast path walk them. Only a chunk with an index ≥ 240
  or ≥ 255 pairs uses one, so the walk is module `CHUNK_WIDE` (feature `us-chunk-wide`, id 53, implied by `us-chan`,
  found by `needs`). Without it, both step by 5 bytes as before, and an escape is `UnsupportedModule(53)`.
- **Packer friendliness**: the most-used new parameters take the remaining one-byte indices (184–239). Rarely set ones
  go to bank 0 (240+). A typical V2 patch stays almost entirely 5-byte pairs.
- **Measured** (task 2.1, i686 probe, compo build with exactly the song's features): a song that uses no V2 module
  grows by 512 B raw and about 270 B xz (WaveSabre showcase 72,704 → 73,216 raw, 34,652 → 34,932 xz;
  slaughter_automation 34,816 → 35,840 raw, 18,580 → 18,844 xz). The cost comes from the escape walk in the chunk
  reader, the expanded-choice decoders, and the 76 extra defaults. The new matrix paths sit behind `us-matrix-add`.
- **Review** (`doubt-driven-review`, 2026-10-28): canonicality, old-chunk byte identity, i686 arithmetic and the Add
  rule against `storeV2Values` were confirmed. `target::PARAMS` equals `sounddef.h`'s 64 `isdest` parameters in order, and
  its voice/channel split equals V2's. As before, `us-trusted` reads only host-baked bytes: it does not bounds-check
  (it indexes out of range on malformed input, as documented in `Cargo.toml`).
*Alternatives*: delta-coded indices (smaller, but the format changes for every chunk, so it needs a version bump), and
a plain `u16` index in chunk version 2 (+1 byte for every pair of every patch). Both were rejected for this one: old
chunks keep their bytes, and only high indices pay.

### D4a. Appended parameter layout (frozen by task 2.2)
**Rule for every V2 field: value = V2 raw value / 128.** The V2 code then runs on `raw = 128·value` (plus the Add
routes), so a V2 modulation entry is exactly an Add slot. This holds for the appended V2 parameters and for the existing
fields that a V2 type or mode reads: oscillator coarse/fine/gain/shape, filter cutoff/resonance, envelope
attack/decay/sustain/slope/release, LFO shape/rate/depth/sync, and voice pan/transpose when the V2 voice is on.

| Index | Name | Meaning (V2 raw) |
|---|---|---|
| 184 | `v2::VOICE` | choice off/on. On: voice transpose/pan read with V2 laws; after the filters and the voice distortion comes V2's DC filter, then V2's equal-power pan of the mono voice (V2 oscillator slots add the same signal to both channels and ignore the slot pan); the amplitude is envelope 1's V2 output only (no master, velocity or gain law) |
| 185 | `v2::KEYSYNC` | raw 0 none / 1 oscillators / 2 full (oscillator, filter, distortion and envelope state reset at note-on) |
| 186 | `v2::ERA` | source format version, `count(0..6)`, default 6 |
| 187, 188 | `v2::ROUTING`, `v2::BALANCE` | filter slots 1–2: raw 0/1 serial, 2 parallel; balance raw 0..127 (64 = both full) |
| 189–192 | `vdist::{MODE,GAIN,P1,P2}` | voice distortion after the filters, V2 modes 0–10 |
| 193–196 | `env_amp(i)` | V2 envelope amplify, envelopes 1–4 (V2-mode envelopes only) |
| 197–204 | `lfo_ext(i, {MODE,PHASE,POLARITY,EG})` | LFO mode choice unisynth/V2 (V2: shape, rate, depth = amplify and sync read raw), start phase, polarity raw 0/1/2, envelope (one-shot) mode |
| 205–220 | `modslot(12..16, …)` | matrix slots 13–16 |
| 221–226 | `ctl::CC2..CC7` | controllers 2–7 (breath, 3–6, volume), raw/128; CC1 is `ctl::WHEEL`; default CC7 = 127/128 |
| 227 | `ch::ON` | channel stage choice off/on (off = not run) |
| 228–230 | `ch::{VOL,BOOST,FXROUTE}` | channel volume, bass boost, distortion/chorus order |
| 231–234 | `ch::SEND_{RVB,DLY,AUXA,AUXB}` | send levels to the four extra output pairs |
| 235, 236 | `ch::RECV_{AUXA,AUXB}` | aux input levels into the channel |
| 237 | `ch::SPEECH` | speech stage choice off/on |
| 238, 239 | — | free (one-byte) |
| 240–243 | `ch::DIST_{MODE,GAIN,P1,P2}` | channel distortion |
| 244–250 | `ch::CHORUS_*` | amount, feedback, delay L/R, mod rate/depth/phase |
| 251–259 | `ch::COMP_*` | mode, couple, auto gain, look-ahead (ms), threshold, ratio, attack, release, out gain |

`COUNT` = 260. Defaults: V2's `v2initsnd` values / 128, with the stage switches off and `ch::VOL` = 1. A patch
without these indices is unchanged.

**New choices keep old values.** A choice stores `k/(n−1)`. To add kinds without moving the old ones, a choice
with `n` kinds grows to `2n − 1`: old kind `k` becomes id `2k`, which has the same stored value, and new kinds take the
odd ids. Oscillator type 5 → 9 (V2 = 1), filter type 9 → 17 (V2 SVF LP/BP/HP/notch/all-pass and Moog LP/HP at 1, 3,
… 13), envelope mode 2 → 3 (V2 = 1), mod source 10 → 19 (CC2–CC7 and V2 note at odd ids), mod operation 2 → 3 (Add = 1).
Unused odd ids act as off. A value that only automation or a slider can make (one no encoder writes) may decode
to a new kind. For example, an operation near 0.5 is Add, and on a legacy target that slot then does not route.

**Add targets are parameter targets.** Target ids 78–141 each name one parameter: the 64 V2 parameters that
`sounddef.h` marks as modulation destinations, at their unified index. An Add slot on a legacy target (1–77) is
inactive. V2 clamps after each entry (`clamp(v + a·s, 0, 128)` in entry order). Add slots therefore apply in slot
order, each clamping to 0..1, and the specs say the same.

### D5. The channel stage lives inside the instrument
V2's channel strip is modulated by the matrix from the newest voice. Effects on other devices cannot see voice sources,
so the strip runs inside kind 64 after the voice sum (and after the speech stage). The order is
`boost → [distortion ↔ chorus by FX order] → compressor → volume`, with sends taken after volume, as in V2's `V2Chan`.
Its distortion, chorus and compressor are the same code as the voice distortion and the `unified-fx` V2 modes (shared
module, one feature each), so a song pays once.
*Alternative*: V2 strip modules as separate effect devices, with the matrix modulation dropped. This was rejected:
channel targets are used in about 90 songs (chanvol, sends, distortion, chorus, compressor gain).

### D6. Extra output and input pairs, encoded in the receive channel field
The receive's `receiving channel` i32 is split into bits. Byte 0 is the receiving pair: 0 main, 2 sidechain,
4 input 1, 6 input 2. Byte 1 is the sending pair: 0 main, 1–4 extra outputs of the sending track's devices, first
device first. Old blobs only hold 0 or 2, so they decode unchanged. The mixer sizes the extra buffers from the
devices' declared pair counts (`Device` gets `outputs()`/`inputs()`, default 0). The pool grows only for tracks that
use pairs. The threads split (`par.rs`) treats a pair receive like any receive for dependency levels.
Review (`doubt-driven-review`, 2026-10-28): old blobs are bit-identical, and the threads levels follow `r.track`.
Pairs are checked only for range. A pair no device writes reads as silence, and the devices of a track share its
pairs. The spec says both.
*Alternative*: fixed send amounts as receive volumes. This loses per-voice send modulation (about 90 song–target
pairs) and aux A/B oscillator input.

### D7. Song mapping
- One track per (channel, program) that plays notes. Notes go to the program in force at note-on. Pitch bend → bend
  parameter automation; CC1–7 → controller parameter automation (step points). Initial controller values: V2's
  `chans[].ctl` start at 0, except volume (CC7) at 127. Check this against v2redux and record it.
- Event times are converted to absolute samples through V2's tempo map (`usecs` per beat, `timediv`) with V2's tick
  rounding. The song tempo field is the first tempo, used only by tempo-synced modes, which V2 never uses.
- Bus tracks: reverb (V2 reverb mode), delay (V2 delay mode), aux A and aux B (receive pairs from every instrument).
  The master holds EQ `cut` and compressor `v2` (the sum compressor), in V2's order.
- Voice capacity: from `MaxPoly`, capped by the global 64-voice pool via `capacity.rs`'s overlap analysis.
- Known gap: V2 changes a sounding voice's parameters at a program change, and keeps one channel strip across
  programs. Per-program tracks keep each program's voices and strip separate. Accepted; the parity report shows any
  song where it matters.

### D8. Speech stage and chunk text section
Port `ronan.cpp` (public domain) as the speech stage. The text needs bytes in the chunk. **Chunk version 2** = version
1 (with the D4 escapes) + one trailing section `u16 length, bytes` of phoneme text. This is the only reason for a new
version. The encoder writes version 1 when there is no text, which keeps old chunks canonical. The version 1 parser
stays strict. A version 2 chunk without the speech feature
fails with `UnsupportedModule(SPEECH)`.

### D9. Format version behaviour
The source format version is an appended parameter (D4a). Modes consult it only where V2's behaviour changed between
versions and parity shows it is audible: reverb low cut below v4 (spec scenario), the FM scheme, the noise generator,
and DC offset (v2redux `v2eras.h`). Each gate is added with a test only when an acceptance song fails without it.

### D10. Parity and tests
- Unit level: a `.v2m` writer in `synth-convert` tests builds one-patch songs. v2redux renders them. Module parity is
  checked through the same metrics (`tools/unisynth-parity`'s device mode), one synthetic song per V2 module and mode.
- Loader: the upgrade output is compared byte-for-byte with v2redux's `v2loadCanonicalize` via a tiny dump tool kept
  in `tools/v2-extract` (built against the external checkout and skipped when it is absent).
- Song level: the 15 acceptance songs, reports committed.

### D11. Size of the V2 work for songs without V2 (task 6.3)

Measured with `research/us-size-probe/measure.sh` on the compo build (baked, exactly the song's features) against the
build before this change. The full table is in `tools/unisynth-parity/size.md`.

| | global-groove xz / squishy | worst non-V2 song |
|---|---|---|
| before | 15364 / 13824 | |
| first V2 build | 16912 / 14848 | +1848 xz (eladamri) |
| now | 15544 / 14336 | +468 xz (eladamri); squishy +0 on elysian and no-remorse, +512 on the others |

What brought it down:
- every V2 branch in the voice behind its feature's `cfg!`, including the checks of runtime values (`Kind::V2`, `EnvP.v2`);
- V2-only voice, slot, envelope and LFO state in `v2::Opt<T>`, zero-sized without the internal feature `us-v2-any`;
- `flt::Kind::V2(u8)` only with `us-flt-v2` (its payload cost every filter match);
- a `us-trusted` build without a kind's V2 feature decodes that choice with the old law (`kinds::decode_in`; exact for every old stored value); the same for effect modes;
- the chunk escapes as `us-chunk-wide`;
- `UInstrument::run_ext` and its forwarding only with `us-chan`.

Turtles (exported from `project/music/turtles.song.json`, as `measure.sh` now does without `ES4_SONGS`): 35836 / 27648 →
35988 / 28160. The remaining +150 to +470 B xz is the other voice-struct growth and the instrument's render split (`play`). The task
asked for ±0. The residue is recorded here, and further cuts would need a restructure of the voice structs.

## Risks / Trade-offs

- [The index escape adds a variable-length walk to the player's chunk reader] → measured by the size probe. Truncation
  and non-canonical form tests guard the decoder, and `doubt-driven-review` runs on it before anything depends on it.
- [Per-program tracks differ from V2's per-channel state] → measured per song in the parity report. If a song fails
  because of it, add a "program switch" mode to the instrument instead (later change).
- [The 32-frame control rate vs V2's 128-sample frame changes envelope/LFO stepping] → finer is closer to the
  intended curve. If parity fails on fast LFOs, step V2-mode modulators on V2's frame grid.
- [Speed: V2 runs per-sample filters, distortion and a channel strip per instrument] → measure with the existing speed
  report. The skill `performance-optimization` applies before optimising.
- [Old eras (v0–v4) differ in oscillators/FM] → D9 gates driven by failing acceptance songs. Version 2 has no known
  file and is not tested.
- [Song copyright] → songs and renders stay local and git-ignored. Only metrics are committed.
- [Mixer change touches every song] → old blobs must decode and render bit-identically (spec scenario, size-threads
  hash check, `us-bake-check`).

## Migration Plan

Additive. Old chunks (v1, indices ≤ 183), old song blobs (receive channels 0/2) and all existing effect modes are
unchanged, and the self-goldens and bake checks guard this. To roll back, revert the change: no stored format is
rewritten.

## Open Questions

- Fetching modland in bulk: limit to the acceptance set plus a sampled subset, or mirror the whole V2 folder (about
  60 artists)? This does not affect the specs. The default is acceptance set + the existing local 360.
