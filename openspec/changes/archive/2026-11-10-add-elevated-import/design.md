# Design

## Context

- Source material lives in `~/Downloads/zenekk/TBC - Atrium/` and is never committed:
  - `synth.asm` (Mentor's synth, header "Elevated", CC BY-NC-SA 3.0)
  - Atrium's `music.asm` + `music.h`
  - `hack.exe`, a debug build from a third-party Visual Studio rebuild, not used
- Elevated's `music.asm` comes from iq's public source release and is fetched by the parity script into
  `$ELEVATED_REF`.
- Engine summary, from a behavioural reading of `synth.asm`; the full notes go to `research/elevated-synth.md`:
  - 44.1 kHz. The song is rendered whole into float stereo stack buffers and converted to 16-bit at the end.
  - `_machine_tree` is a byte program: synth parameters (implicit opcode 0), then `opcode, params…` pairs, ending on a
    negative byte. The opcodes are 0 synth (72 B), 1 delay (12), 2 filter (32), 3 compressor (12), 4 mixer (8),
    5 sine distortion (8), 6 atan distortion (8) and 7 allpass (12).
  - Synth notes are rendered one after another, each into its own step slot. They *overwrite* the buffer (`fst`, not
    add), so each channel is monophonic with hard cuts.
  - Note value `2n` steps of `2^(1/24)` from 8.1758 Hz gives MIDI note n. `0x7F` selects zero envelope scales, which
    renders silence for the note's length.
  - The envelope has four linear segments with integer frame lengths and level steps `+1, −½, 0, −½`, so the sustain
    is fixed at ½.
  - x87 keeps the phase and envelope in 80-bit registers. The phase is never wrapped; `frndint` folds it at read time.
- Repo patterns this follows:
  - `add-v2-import` and `add-tf3-import`: converter in `synth-convert`, new behaviour as `us-*` modes, parity in
    `tools/`.
  - `add-cauldron-xm-import`: new effect modes appended to kinds, new oscillator type at a free choice position.
  - Material stays local.

## Goals / Non-Goals

**Goals**
- Atrium and Elevated pass the default parity metrics against the original code run under Wine.
- Every new algorithm is exact to the reference law (unit-tested against hand-computed values) and costs nothing in a
  song that does not use it.
- The DAW can open and edit the converted songs, with every new setting visible in the device panel through the
  existing metadata.

**Non-Goals**
- Bit-exact output. Noise order, x87 precision and the control grid differ.
- The original's int16 overflow (`fistp` writes −32768 past full scale). We clamp.
- Any standalone Elevated crate or player.

## Decisions

### D1. Import, not a legacy engine
The original is small, so its laws go straight into `unisynth` modes. The reference is the original code itself, so no
Rust port is needed to judge parity. *Alternative:* a `crates/elevated` port first. Rejected by the user; it would only
duplicate the reference.

### D2. Pair oscillator as one slot type
An Elevated instrument has 3 pairs = 6 oscillators, and `unisynth` has 3 slots. Voice-level unison cannot reproduce
the per-pair `(2 − d, d)` ratios. So one slot plays one whole pair, stored at the free type position 5:
- waveform = `shape[0]` (`Choice(3)`: sine, square, saw)
- phase shift = `shape[1]` (turns)
- detune `d` = `2·shape[2]` (0.5 is `d = 1`; both songs use 0.00049–1.5)
- polarity = `shape[3]` (`Choice(2)`: as is, negated)

The pair is naive (aliasing), as in the original. It keeps **two f64 phases**, one per copy, each advanced by
`inc·ratio` and wrapped. That equals reading `φ·ratio` from the original's unwrapped phase, including under a pitch
drop (`φ·r = Σ inc·r`); wrapping a single phase would jump by `integer·r`. Op 1 (nop) disables the slot, so the
converter turns the slot off.

**Subtract** (op 3) is an added pair with its polarity set: `acc − pair = acc + (−pair)`. Expanding the shared
`combine` choice was rejected (user decision 2026-11-10): `combine` is an ordinary, automatable choice, and a fourth
value would move the decoding of off-centre automation values (the reason add-tf3-import did not expand filter
types).

### D3. Pitch drop at control points
The voice frequency is `b + (f − b)·k^(n+1)` (Hz). `Pitch` gains a `Move::Drop`: at every control point it is
evaluated in closed form (`kⁿ` from the voice age, `exp2(n·log2 k)`) and turned into semitones. The slot increments
ramp linearly between control points, as every pitch move does. The error of the linear ramp against the exponential
is about `(32·ln k)²/8` relative: 1e-4 for Atrium's kick (k = 0.9993); the fine window after note-on halves the
segment. This was chosen over a per-sample path (user decision 2026-11-10): it needs no change to the oscillator
loop. Settings are appended voice fields: `drop.base` (Hz, `20 000·p`) and `drop.rate` (`1 − k = 10^(−6(1 − p))` for
p > 0; p = 0 is off). Glide and slide are unchanged when the drop is off.

### D4. Linear timed envelope (appended fields)
Each envelope gains appended `linear` (`Choice(2)`) and `hold` fields. Linear: attack 0 → 1, decay → sustain, hold for
`hold`, release → 0, all linear in level, times `5000·p²` ms with no 1 ms floor (Elevated attacks are 3 frames).
The envelope ends by itself, and a note-off ends it at once (level 0, finished). That gives Elevated's `0x7F` cut
without a new event kind: device events are only `NoteOn`/`NoteOff`, so "CC 120" would have needed a song-format
change. Expanding the `env_curve` choice was rejected for the same reason as D2. Stage boundaries were first left on the voice's
control points (the 8-frame fine window, else 32 frames; user decision 2026-11-10). Parity (task 5.1) showed the cost:
a 3-frame kick attack smeared over 8 frames lost its click (−5.9 dB at 8 kHz). So envelope 1, when linear, now ends
the voice's control segment at its next stage corner (`Env::linear_left`). The instrument ticks a voice whose segment
ended off the grid. Voices without a linear envelope still end on the grid, and every golden is byte-identical.

### D5. Monophonic hard cut
- **New notes:** capacity 1 with steal "oldest" already restarts the sounding voice at the new note's frame
  (`VoicePool::start` → `Voice::note_on` on the stolen voice). Nothing new is needed.
- **`0x7F`:** the converter emits a note-off for the sounding note. Its linear envelope 1 ends at once (D4), and the
  gain ramps to 0 over the rest of the control segment (at most 32 frames).

### D6. Global effects as modes, state in song time
Reuse first. The audit of existing code (2026-11-10) found:

| Elevated machine | Existing code | Same? | Plan |
|---|---|---|---|
| filter (Chamberlin) | `flt::run`, state-variable **input form**: `low += f·band; high = a·(x − c·band) − low; band += f·high`, `a = q, c = 1` | the step is identical (the V2/WaveSabre family) | reuse `flt::run`; add a **one-step** oversampling form (the shared filter always takes 2 half-steps); outputs low/high/band are `Kind::SvfLp/Hp/Bp` |
| atan distortion | `v2::DistP` overdrive: `gain2·atan(gain1·x + offs)`, `native_atan` = exact `atanf` | identical with `gain1 = 1/a`, `offs = 0`, `gain2 = b` | reuse the overdrive core with native atan |
| ping-pong delay | `fx/tempo.rs` cross-feedback (`bufL ← xL + fb·(… + rd·cross)`) | no: Elevated crosses the dry input too (the first echo lands on the other channel) and has no wet/dry or cuts | new mode on `synth-core`'s ring |
| allpass | `synth-core` `AllPass` (Freeverb: `out = buf − in`) | no: Freeverb's form is not a true allpass and does not cross | new mode on the ring |
| sine shaper, knee | none exact | — | new, a few lines each |

- `svf-lfo` (EQ 67): the coefficients `[g, q, 1, –, one-step]` are built by the mode itself, because `sv_coefs` maps
  resonance as `1 − res` clamped to 0..1, and Elevated's `q` is the raw multiplier. The one-step form uses the free
  coefficient slot `c[4]` (0 today) as a flag, so no stored parameter changes and voice filter slots are unaffected.
  It is gated by `FORMS`, like the other non-plain forms.
  - The coupled-form LFO `c ← c − r·s; s ← s + r·c` is a rotation with determinant 1. Its closed form is
    `s_n = A·sin(ω·n)` with `ω = 2·asin(r/2)` and `A = depth·r/sin ω`. Task 3.1 tests it against the recurrence over
    10⁷ samples. Computing it from the absolute frame keeps seeking and the DAW's scrubbing exact.
  - Filter state is ordinary effect state.
- `atan` (Distortion 65): a mode wrapper around the V2 overdrive arithmetic (native `atanf`). `sine` and `knee` are
  memoryless and new.
- `ping-pong`, `allpass` (Delay 68): one stereo ring of `length` frames from the caller's pool (max 65 536). The ring is
  zeroed at reset, so the original's initial counter value does not matter.
- **Mode storage (corrected 2026-11-10).** The original plan was to append the mode ids, but that would have
  re-decoded existing chunks: the mode is `Choice(n)` over the kind's modes, so a 4th → 7th distortion mode would move
  the stored "curve" value (1/3) to "power". Instead, `fx::MODES` rows carry a stored position. Distortion (4 → 7),
  EQ (3 → 5) and delay (3 → 5) are expanded, as add-tf3-import expanded the oscillator types: earlier modes keep their
  stored value at `2·id`, and the new modes sit at odd positions. EQ position 3 is free and gives `BadChunk`. Mode ids
  (the numbers code and converters use) are unchanged, and the new ones are appended. `migrate::old_fx_mode` decodes
  against `fx::migrated_total`, frozen at the counts before this change.

### D7. Machine tree to tracks
The converter runs the tree symbolically with a stack of track ids (see the `synth-convert` spec). Gains go to the
receive volumes of the new track (`synth-mix` receives carry a volume, negative included), so no gain device is
needed. The master is the last track. The song's tempo is
reported as `44100·60/(4·step)` BPM for the DAW grid. Events are placed by frame, so the BPM is display only.

### D8. Parser for `music.asm`
A small line parser: section and label lines, `DB` and `DD` lists (decimal, `0x` hex, negative values), comments after
`;`. `DD` values are read as i32 or as f32 bits, depending on the machine field. Floats are stored in `DB` bytes inside
`_machine_tree`, so the machine walker decodes little-endian f32 from bytes. **`music.h` is needed** (finding
2026-11-10): the synth strides `_sequence_data` by `NUM_ROWS` from `music.h`. `_tempo_data`'s row byte can differ
(Elevated: 113 vs `NUM_ROWS` 114), and the sequence can carry padding (Atrium: 904 bytes = 8 × 112 + 8). The
converter reads the `music.h` beside `music.asm` (its `static const int NAME equ VALUE;` lines).
**The opcode table is per build** (finding 2026-11-10): Atrium's `synth.asm` has 8 machines, while Elevated's has
atan commented out, so 6 is allpass there. Both trees end exactly on their terminator under their own table only; the
converter detects the table that way (`--table` overrides it). The parser rejects any section other than the four it knows only when that section is
required. Unknown sections are ignored.

### D9. Reference renderer
`tools/elevated-parity/`:
- `Dockerfile`: the `ws-extract` base plus `nasm` and `gcc-mingw-w64-i686`.
- `build-ref.sh`: copies `synth.asm` and a song's `music.asm`/`music.h` into `$ELEVATED_REF/<song>/src/`, because
  `synth.asm` includes `src\music.h`. It runs `nasm -f win32` and links `render-ref.c`.
- `render-ref.c` (MIT, ours): defines `MusicBuffer`, optionally walks `_machine_tree` to zero other synths' volume for
  `--solo`, calls `generateMusic()` and writes the WAV.
- `parity.ts`: renders ours via `cargo run -p unisynth --example render`, calls `compare` from `tools/synth-parity`,
  writes `report.md`, and skips when the sources are absent.

## Risks / Trade-offs

- **The overwrite semantics also cut tails that the next note does not cover.** A long release is cut by the next
  note on its channel. This is reproduced by mono hard cut. → Unit test on a synthetic tree.
- **The magic-circle closed form drifts from the recurrence** (f32 recurrence vs f64 closed form). → Over 10⁷ samples
  the difference must stay below 1e-4·depth. If not, run the recurrence from the last sought frame.
- **The pair detune range is too narrow for unknown songs.** → The converter warns, and the range is set from both
  songs with margin.
- **Noise differs** (LCG order across the whole song vs per-voice RNG). → Spectrally equal, so the metrics accept it.
- **The Elevated source release may use an older tree layout.** → `build-ref.sh` and the parser check the 8 opcodes.
  A mismatch is reported with the offset; supporting a second layout would be a follow-up change.
- **Licence (NC).** → Behaviour is reimplemented in MIT code; no original text is copied. The reference build stays in
  `/tmp`.

## Encoding review (task 2.6, 2026-11-10)

A doubt-driven review of the frozen encodings ran cross-model, on `@propose-review-1`. Its findings and how they were
handled:
- **Fixed:**
  - `song::features` did not report `us-osc-pair`, `us-voice-drop` or `us-env-linear`.
  - The song/DAW type list showed position 5 as "—".
  - `Move::Drop`'s frame counter could overflow (now `saturating_add`).
- **Accepted trade-offs:**
  - The new fields have two-byte indices. In a build without `us-chunk-wide`, a patch that sets them fails with
    `UnsupportedModule(CHUNK_WIDE)` before the specific module is named. This is the same convention as the
    add-tf3-import point fields.
  - `drop.*` and `envN.linear/hold` are read at note-on: mid-note automation takes effect at the next note.
  - The linear envelope ignores `M(decay)`. Elevated has no modulation.
  - A linear envelope ends by itself even while the key is held. That is the point: Elevated's notes are timed.
- **Transient:** effect module ids 69–74 have no `fx::MODES` rows until section 3 adds the modes.
- **Noise:** position 5 was a gap (rejected) before. That is by design: the add-tf3-import expansion reserved the odd
  positions for new types.
- **Measured (task 5.2):** the per-voice state of a build without the modules is unchanged: the pair state is zero-size,
  and `Move::Drop`, the `EnvP` fields and the off-grid tick are compiled only with their modules. Songs that use none
  of it still grow 84–172 xz bytes, from the appended parameters and the shared lookups (`research/elevated-synth.md`
  §11).

## Migration Plan

Additive. No song changes. The new features are off by default in compo builds and on in the DAW and wasm builds,
like the other modes.

## Open Questions

- ~~The steal setting and CC 120 handling for the hard cut (D5).~~ Settled 2026-11-10: existing steal oldest; `0x7F` is a
  note-off on a linear envelope (D4, D5).
- ~~The gain device for a negative mixer gain (D7).~~ Settled 2026-11-10: none is needed. A `synth-mix` receive has its own
  volume, which carries any mixer gain, negative included.
