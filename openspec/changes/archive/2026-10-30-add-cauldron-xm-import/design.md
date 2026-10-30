# Design

## Context

- **Source material.** `research/tbl-mini-cauldron/mini-cauldron-spec.md` (algorithm, §§ 3–8) and
  `ref/patches_{jizz,stash}.json` (12 v1 and 26 v2 records). The bit-exact references `ref/mcauldron{1,2}.py` render
  every phrase. `tools/rebuild_xm.py` puts those renders into the XM templates, and the result is byte-identical to
  what the intros build in memory.
- **What the songs do with the phrases** (measured on the three XMs):
  - **Jizz** is plain: 276 of 305 triggers of generated samples are at C#5 (native speed), and 214 of 220 measured
    plays run all 16 steps. Effects on those notes: `8xx`, `E1x`, `Cxx`, `Fxx`.
  - **Stash** plays phrases at several pitches. Song 1 mostly uses C#6 with relative note −12/−24/0, which is native
    speed for each length class, plus C#4…A#6. BLUISH uses D-6 (+1 semitone) and D-0/D-1 (5 octaves down: slow
    textures). It uses `3xx` (223 times), `9xx`, `E9x`, `EDx`, `E1x`, gate-like volume envelopes, and ping-pong loops
    on 13 samples. In song 1, 60 of 234 triggers of ping-pong-looped samples run into the loop, adding up to about
    7 minutes of channel time.
- **unisynth today.**
  - Oscillator types PM, BLIT, wavetable, sampler and V2, with an expanded choice that has free odd positions.
  - SVF/ladder/RBJ filters, WaveSabre/Tunefish/V2 envelopes.
  - Mono-legato **slide**: linear, `time(slide)` seconds.
  - Voice distortion with a clip mode, which runs with `v2.voice` off.
  - Effects: tempo delay with feedback and dry/wet; flanger with 2 modes; distortion with 3 modes.
  - Sampler: GSM samples from the chunk.
  - Song model: tracks with note lanes, per-parameter automation (discrete steps allowed), tempo.
- **The converter** is host-only (`crates/synth-convert`), with one module per source format, a `Patch` builder,
  automation lanes from `auto.rs`, and voice capacity from `capacity.rs`.

## Goals / Non-Goals

**Goals:**
- Every generated sample of both intros plays as live notes on unified instruments.
- The three songs pass the parity metrics against the bit-exact reference.
- No stored phrase audio in the converted song. Only the stored drums and vocals stay samples, as in the original.

**Non-Goals:**
- A general XM importer: only the XM features these three songs use are replayed, and anything else is an error.
- Sample-accurate reproduction of FT2/libopenmpt mixing (interpolation, volume ramping).
- Keeping the decimation aliasing, the 8-bit wrap of the echo, or stale buffer bytes.

## Decisions

### D1 · Unroll phrases at conversion time, not in the instrument

The sequencer becomes notes in the song, and the instrument stays a normal instrument. The alternative, an
instrument-side 16-step sequencer triggered by one note, would hide the music from the DAW and break the
"like other unisynth tracks" goal.

The converter runs an **XM timeline replay**: a small FT2-semantics player that produces control data instead of
audio. Per channel and tick it tracks:
- `(sample, position in internal frames, playback ratio r, volume, pan, key-off/fade state, loop direction)`

Each generated-sample trigger then walks the phrase's step grid in **phrase time**. Phrase time advances by
`R·r(t)·dt`, with R = 35 441.2 Hz and r = 2^((note + relative − C#5)/12) on the linear frequency table, times the
finetune. It emits a note at every step boundary the position crosses:
- **Untied step:** note-on with the step's retrigger byte (D3) and the phrase position as the velocity, and a
  note-off at the step end.
- **Tied step:** a legato note (overlapping), so the mono pool slides.
- `9xx`, retriggers and loops jump the phrase position. The next note starts at the step under the new position,
  with its remaining length. Its retrigger counter (D3) is the one of that step.

**Replay (task 5.2).** `xm::replay(xm, rate)` plays the order list tick by tick with FT2 semantics and returns the
triggers (channel, start tick/frame, instrument, sample, note, offset) with one control entry per tick (frequency,
final volume, pan). A tick is `trunc(rate·2.5/bpm)` frames, as openmpt renders (730 at BPM 151, not 730.13): with
the exact 730.13 the triggers drifted to 1 300 frames late by the end of Jizz. The effect set is the one the songs use
(0–4, 8, 9, A, C, D, F, G, E1, E2, E9, EC, ED, EE; volume column set volume and panning); anything else, and
auto-vibrato, is an error naming the pattern, row and channel. **Verified** against openmpt123 playing each XM with
every sample replaced by a click (`cauldron.ts --onsets`): 100 % (Jizz, BLUISH) and 99.9 % (song 1) of our trigger
clicks within 100 frames, median offset 0; the song lengths equal openmpt's to the frame (it renders 0.1 s on).
Slid periods clamp to FT2's 1–31 999: BLUISH slides a C-1 phrase up by `130` for rows, which unclamped ran to
4·10⁹ Hz.

**Unrolling (task 5.3).** `cauldron::import::path` integrates the stored-sample position per tick (constant frequency
within a tick, as the tracker resamples) through the offset, forward loops, ping-pong turns and the sample's end;
phrase frames are stored samples × the record's decimation. `unroll` walks `phrase_events` along it, mapping each
event's phrase frame to output time inside its piece. At a jump it ends the sounding note and enters at the step
under the position: a note-on with the retrigger byte of that step's tie-chain head, the step's pitch and phase byte,
and the position as velocity, if the original still sounds there (before its note end, or within the release: a v1
step's fade, a v2 `release`), followed by an immediate note-off when entering a fade. Approximations at an entry
inside a step: the filter envelope and second-saw phase restart from the step's start values. The XMs' native note
plays at ratio 1.0000 exactly (finetunes give 1.0036–1.0109 elsewhere). Measured: Jizz 502 phrase triggers,
5 475 notes, no ping-pong; song 1 681 / 8 800, 473.5 s backward; BLUISH 329 / 6 035, 160.6 s backward.
**Open for 5.4:** song 1 and BLUISH slide some phrases by up to ±73 semitones (risers from C-1 to the period limit);
there a step lasts under the 128-frame lane lead and the bend exceeds 24 semitones.

### D2 · One track per (record, XM channel, ratio class)

- **Mono legato** needs one phrase at a time per instrument, but the same record can sound on two channels at once.
- The **echo, comb and doubler** times are fixed in phrase frames, so they scale with 1/r.

The converter therefore groups triggers into tracks by `(record, channel, r rounded to a semitone)`. Each track has
the record's instrument and effect chain with times for that r. Tracks whose instrument and effects come out
identical are merged where they never overlap. The alternative, one track per record with automated effect times,
glitches the delay lines and fails the overlap case.

Pitch slides (`3xx`, `1xx/2xx`, `E1x/E2x`) inside a trigger do not create tracks. The instrument's pitch bend is
automated (bend range set to the slide's span, at most 24 semitones, warn beyond), and the step clock follows
`r(t)` exactly. The effect times keep the trigger's starting r.

**Implemented (task 5.4).** `import::group` keys triggers by (record, start ratio's semitone class, channel) and
merges the channel groups of one (record, class) whose sounding spans (the unroll path, not the replay's
until-next-note span) never overlap. On the three songs the track count equals the lower bound, the most triggers of
a (record, class) sounding at once: Jizz 58 tracks (15 pairs; it layers the same phrase on up to ~4 channels),
song 1 99 (38), BLUISH 55 (26). `import::controls` gives per-tick points where a value changes: gain (the replay's
final volume), pan, and bend in semitones from the class (the trigger's fine offset, up to 0.19 in Jizz, rides in
the bend too). Decided: the risers of song 1 and BLUISH (up to ±73 semitones) clamp at ±24 with a warning naming
record, channel and time (14 and 16 triggers); revisit if their passages fail the song parity.

### D3 · The compounding cutoff ramp: a retrigger byte per note (revised in task 4.4)

At retrigger *k* of a patch:

`A_k = A·mul^k`, `B_k = B·mul^k`, rounded as stored (f64 for v1, f32 for v2).

k counts every retrigger since the patch's first render, including the double retrigger at each variant start
(spec §4.4). For XM sample `ProbeNV` / slot-variant v, the k at step s is known in closed form.

**First plan, dropped: A_k and B_k as automation values.** Song automation stores one *byte* per point
(`synth-mix`), so a value law over c = 1e-4..4.7 would step by several percent; the ramp is 10 % per retrigger.

**Decision.** The slot holds the uncompounded A, B and `mul` (f32 chunk values) and compounds them itself. Each
note carries:
- **Retrigger byte** (`shape::CD_RETRIG`, one automation step before the note-on): `2k + accent`. k ≤ 96 in both
  intros, so a byte holds it exactly. The slot multiplies A and B by `mul` k times, rounding each product to f32 as
  the original stores it, then by the accent (×4 v1, ×2 v2). Against the reference trace: within 1e-5 (A, B and
  `mul` are f32 chunk values; v1 kept doubles).
- **Phrase position** (the note's velocity, `1 + eighth steps`): the spike's frame counter `t` at the note-on. Step
  starts are exact; a jumped start (`9xx`, loops) is within ⅛ step. This replaces the oscillator's `p·2¹⁷` field
  from task 2.3, which a byte would have quantized to 514 frames. The voice's velocity sensitivity is 0.

The alternative, velocity alone for the ramp (one source, 128 levels), cannot carry a ramp of up to 1.7^31 across 32
retriggers either.

### D4 · Filter: the original filter in the cauldron slot (measured in task 4.3)

The original is a damped mass–spring after each oscillator's gain:
- `v1 ·= d` per frame, `c = v1 + v2`
- `y = (1 − q)·(y − c·(x + prev))`, output `prev = y + prev` (DC gain −1)
- at a retrigger: `prev = 0`, `v1 = A_k`, `v2 = B_k` (× accent); `y` runs on

**First plan, dropped: an SVF with a fitted envelope.** A per-record fit of `fc(t)` to a Tunefish-mode envelope
(measured over the longest tie chain of each of the 38 records) gave:
- 26 records within 2 %, 5 at 2.7–4 %;
- 7 records with `decay` above 1, whose cutoff *rises* the longer a note is held: 1.15× to 284× over a tie chain.
  A decaying envelope cannot make that. They are the big filter-opening sweeps (Jizz record 11, BLUISH 7 and 9,
  song0 2, 4, 5 and 15).

**Decision: the original recurrence as part of the cauldron slot** (`us-osc-cauldron`, user decision after the
measurement). It runs on the slot's output before the slot gain, which is the original's order since the filter is
linear.
- **Fields.** It needs base, envelope amount, decay, damping and the phrase clock. A filter slot has only cutoff and
  resonance; the cauldron slot has the clock and four free shape fields. So shape 5–8 hold base `8p`, amount `8p`,
  decay `1 + (0.5 − p)/100` and damping `p`, in phrase-frame units. Base and amount 0 (the defaults) turn it off, so
  existing patches are unchanged. The compounding factor `mul` (D3) is in the slot's `FEEDBACK` field (`4p`), which
  the cauldron slot does not otherwise read; the V2 slot reuses fixed fields the same way.
- **No separate feature.** The filter is part of the cauldron oscillator (about 40 lines); every converted
  instrument uses both, so a separate `us-flt-cauldron` would only add a module id and `needs` rules.
- **Note-on** is the retrigger (`prev` 0, `v1` = amount, `y` runs on); a legato note keeps decaying `v1`. That
  covers falling and rising cutoffs, tie chains, accents and resonance exactly, with no fitting.
- **Rate (revised in task 4.6).** The whole slot (oscillator and filter) runs at its phrase clock, R × ratio, with
  the original coefficients, and a 4-point cubic resamples it to the output rate (2 phrase frames of latency); see
  D5. The filter vectors are within 1e-4 of peak. (First built as a pole mapping of the coefficients to the output
  rate; dropped with the resampler, which also fixes the oscillator's aliasing.)
- **Inversion.** The −1 DC gain is kept: the original's stored samples have it too.

### D5 · New oscillator type `cauldron` (`us-osc-cauldron`, osc type id 6, stored at odd position 3)

None of the band-limited types reproduces these sounds: the u⁷ spike, the half-clipped spike2, PWM from the sign of
two summed saws, and naive aliasing. The aliasing is part of the sound, unlike the decimation, because it happens at
the synth's own rate.

- The slot runs the original per-frame waveform code on a 16-bit phase **at its phrase clock** (R·r frames per
  second) and resamples to the song rate with a 4-point cubic (revised in task 4.6). First built at the song rate:
  its naive waveforms then aliased at 44.1 kHz instead of the original's 35.4 kHz, and Jizz record 4 read +3.7 dB
  in the 63 Hz band (+0.6 dB when rendered at the original rate and resampled).
- Shape fields:
  - `shape[0]` wave 0–5 (saw, square, spike, pulse, noise, fold)
  - `shape[1]` voices 0–10
  - `shape[2]` version, a choice of 3: 0 = v1, 1 = v2 (v1 spike sign handling vs v2 spike2 and the pre-filter
    clamp), ½ = v2 in a glide (task 4.7, below)
- At note-on (non-legato) phase is −32766 and phase2 is kept, as at a retrigger, unless the note's **phase byte**
  sets it (revised in task 4.6): the slot's `PM_OUT` field (the cauldron slot sends nothing into the next slot)
  holds `1 + round(255·phase2/65536)` mod 255, 0 = keep. The converter replays the original's
  `phase2 += fistp(pitch·1.02)` per frame (glides included) to write it before each retrigger, within 2 of 65536 of
  the reference renders. The second saw's starting phase decides how a low note's 2 % detuned pair beats (≈1 Hz at
  53.5 Hz): starting it at 0 made Jizz record 0 variant 3 read +12.4 dB at 63 Hz. The spike and noise waves do not
  read it.
- A voice with a cauldron slot slides **linearly in frequency** (`Move::Hz`), as the original glides its pitch word;
  a semitone-linear slide lingers at the low end of an octave glide (Jizz record 1: +3.5 dB at 63 Hz).
- **Unsigned words (task 4.7).** Stash stores its pitch words unsigned: a word above 32767 runs the 16-bit phase
  backwards, and the second saw steps `fistp(w·1.02)` of the unsigned word (52223 for 51199, i.e. −13313), not the
  2 % detune of the alias. The converter therefore asks for the unsigned word's frequency `w·R·r/65536` (up to
  33 kHz; the slot runs at its phrase clock) and the slot steps exactly like the original. Words span up to 172
  semitones (3 to 60736) and notes 128, so each record's instrument centres its words with a static shift (octave and
  `1 + ratio_fine`) and a word still outside 0–127 moves by the slot's coarse pitch (±24, a per-note byte lane).
  Playing the alias instead made song0 record 3 (word 51199 throughout) uncorrelated with the reference.
- **The glide's signed register (task 4.7).** In a tied step's first 1764 frames the original stores
  `fistp(interpolated signed words)`, so a word above 32767 is −14337, not 51199, there: the phase steps the same,
  but the second saw steps `fistp(−14337·1.02)` = −14624 (and the spike reads the dword). Version ½ makes the slot
  hold the signed word; the converter sets it over each glide window that has a word above 32767 (a lane stepped
  64 frames early, centring the player's 100-frame automation step on the edge). Glides whose words change sign
  (bluish record 5, song0 record 5) interpolate through 0 in the original and linearly in frequency here.
- **Word byte (task 4.7).** The fine byte resolves ±0.39 cents: ±11.6 words at 51199, and where the sound is mostly
  aliases of high harmonics, harmonic n moves by n times that (song0 record 3: every band within 1 dB with the exact
  word, 4–10 dB off without). The slot reads `o::FIXED_HZ` as a word byte (it always tracks the note): b ≥ 1 makes
  its word `rint(f·65536/clock) + b − 128`, the original's integer word; 0, the default, leaves the frequency's
  word (bends stay smooth). The converter writes it per note next to the fine byte, picking the fine byte whose word
  lies ≥ 0.2 from a rounding edge (the slot's f32 path is within ~0.005 words), so every word is exact (tested for
  every record at ratios 1 and 2). Word 0 (a frozen phase) rounds to 0.
- **The echo's wrap (open).** The original's echo is circular over the phrase: steps 0–2 take `e·` the un-echoed
  steps 13–15 (and pass it on through the feedback). A real-time delay cannot hear a phrase's end before its start,
  and a track's receives enter its input, so a "ghost" tail cannot feed the delay wet only. In a song, back-to-back
  triggers of one record get the previous trigger's echoed tail there instead. Single triggers miss it.
- The spike's second voice follows the original `off = t·k·pitch>>7`, with t counting frames since the note-on of the
  phrase's first step. This is passed as the voice's age since its first non-legato note. v2 applies the `&0xFFFF`
  truncation.
- The output is scaled 32767 → 1.0.

**Doubt review (task 2.1, 2026-10-30)**, fresh-context reviewer. Outcome: design kept, with these changes.
- *Stored position 3 (0.375) decoded as "none" before.* This is accepted policy: the add-v2-import design (D4a) says
  unused odd ids act as off, and that a value no encoder writes may decode to a later kind. The oscillator type is
  also structural (`params::is_structural`): `set_param` ignores it and it cannot be automated, so only encoder
  values reach it, and no encoder writes 0.375 today.
- *Trusted builds with two later kinds.* `osc_type::decode` passes `built = us-osc-naive || us-osc-cauldron`, the
  pattern `fx::mode_of_value` uses. A trusted build compiles every module its song needs (`compiled_all`), so the
  mixed cases are unreachable.
- *`Kind::from_type`* gets explicit cauldron arms (`Ok` / `missing(OSC_CAULDRON)`) before the sampler fallback. A
  test covers every type id × feature on/off with the exact module id.
- *Shape fields* decode with `choice`/`round`, so any stored value clamps into range (wave 0–5, voices 0–10,
  version 1–2).
- Tests asserting `Kinds { old: 5, all: 6 }` move to 7, and `encode(CAULDRON) == 3/8` is asserted.
- *Out of scope, reported to the user:* the DAW (`packages/daw/src/app.js`) reads oscillator types with the
  pre-expansion law `choice(p, 5)`, so it already shows V2 slots as PM. This is a pre-existing bug and needs its own
  fix for cauldron slots to display.

### D6 · Amplitude: note length + release, output clip (revised in task 4.6)

- **v1 fade:** each step fades linearly unless the next step is tied. That becomes a note-off at the start of the last
  step of a tie chain, with a WaveSabre release whose energy matches the linear fade: `T_q = 5/3·T_lin`.
- **v2 gate:** a note-off after `hold` frames, with `release` frames of release. This uses the same energy-match rule.
- **Linear release, tried and dropped:** the rate-mode envelope with slope 0 releases linearly, but its fastest
  attack is ~5 ms, so single-step notes (off 64 frames after their on) lost up to 11 dB.
- **One voice** (capacity 1): a retrigger restarts the same voice, so the filter's `y`, the second saw and the
  resampler run on as in the original's retrigger. The previous note's release is cut there, where the original's
  fade has already reached 0. With a fresh voice per note, records 2, 5, 6 and 9 lost 3–5 dB of broadband energy
  below their fundamental: the original's retrigger transient.
- **Instant attack:** in a voice with a cauldron slot, a timed attack of 0 is instant instead of WaveSabre's 1 ms
  (`EnvP::instant`); the original has no attack. With 1 ms, records 5 and 6 missed the 250 Hz band by 0.1–0.5 dB.
- **16-bit clamp:** the shaper's clipper at full scale, the **first effect** on the track: after the voice's
  release, as the original fades before it clamps. As a voice clip before the release (first build), bright
  variants lost up to 4 dB (Jizz record 0 variant 3: 0.333 vs 0.515 RMS on its first step). It keeps the hard
  clipping of the gain-11 Stash patches and the resonant peaks of the low-damping patches.
- **8-bit clamps (task 4.7):** the original also clamps after its comb (`x[i+k] − x[i]` to −126..127) and after
  every echo add (±127). Both are shaper clippers on the track: at full scale after the comb, at the echo's wet
  level `1/(1 + e)` after the echo. Without them the comb's difference reached twice full scale: Stash 28 → 40 of
  53 phrases with them, Jizz unchanged.
- **Level:** the centred slot pans at √½ per channel; the voice master is √2.
- **Bandwidth low-pass** (decimated phrases): the original decimates after the clamp and the effects, so the
  Nyquist low-pass is the last effect of the track (D7), not a voice filter before the clip.

### D7 · Effects per track

| Original | unisynth |
|---|---|
| echo (3 steps, feedback `echo`) | delay `tempo` mode: feedback `e`, dry/wet `e/(1 + e)` (the mix is a crossfade: this is `x + e·d` over `1 + e`; the track gain makes up the level, or the wrap's drive when a wrap follows), open cuts, time = 3·3529/(R·r) s as quarter beats + whole ms at the song tempo (within 0.5 ms) |
| comb `x[i+k] − x[i]` | new flanger mode **comb**: `y[n] = x[n] − x[n−K]`, K = k/r frames at the song rate. Fixed delay, gain −1. |
| doubler (in place, read at 0.99699×) | the same comb mode with **sweep**: `y = (x[n−D] + x[n−D+s·t])/2`, s = 0.00301, t restarted by a parameter step at each phrase trigger. D = the largest sweep (≈ 175 phrase frames). The converter shifts that track's notes earlier by D to cancel the latency. |
| stash specials (slot 9 ×1.25 wrap; slots 11, 15 +48 wrap) | new distortion mode **wrap**: `v = v·drive + offset; v = ((v + 1) mod 2) − 1`, with drive and offset parameters. Slot 9 only: slots 11 and 15 never wrap in the original (the spike wave's DC keeps them under 80, stored samples min −79, no wrap jump), so their +48 is a DC shift and is dropped; ours, the DC lost to the echo's low cut, wrapped every peak (song 1 18–25 s crackle, record 10 +5 to +21 dB above 1.6 kHz → within ±2.5 dB to 4 kHz) |
| decimation 2:1 / 4:1 | dropped. A low-pass at the stored Nyquist `R·r/(2·decim)`, last on the track, keeps the bandwidth (proposal default); none when that is above 20 kHz. It is the tempo delay as a filter (no time, fully wet, no feedback: its 12 dB/octave SVF high cut, 1 frame of latency): the parametric EQ's fixed 20 Hz low cut is bent by WaveSabre's table cosine into a resonant ~45 Hz corner (+2.8 dB at 63 Hz, measured). |

**Stored values.**
- Comb takes flanger's free odd position 1, and wrap takes distortion's position 1 on the 5-position scale. Existing
  stored values (shaper 0, curve 0.5, power 1.0, phaser 0, ring 1.0) keep their meaning.
- `fx::mode_of_value`'s trusted-build shortcut must also consider the two new features.

**Doubler timing (measured in task 3.2).** The sweep matches the original in-place doubler when:
- the sweep rate is `s' = 65536/0xFF3B − 1` (≈ 0.0030196, not `1 − 0xFF3B/65536`);
- `t` restarts K frames after the phrase start (the restart step goes at the unshifted phrase start, while the notes
  move K + 1 frames earlier to cancel the latency).

With that alignment the error is 31 dB below the signal on a phrase-like test signal.

**Order on a track:** instrument → clip (D6) → comb → delay → sweep → wrap → Nyquist low-pass (decim > 1, D6).
- The original order is 16-bit store → comb → echo → doubler → specials → decimation. It is kept as is: the doubler and wrap are not linear and
  time-invariant, so moving them would change the sound.
- A track with both comb and doubler gets two flanger instances.

### D8 · Stored samples via the existing GSM sampler

The drum kit (7 samples, 14 654 B) and Jizz's vocals (14 032 B) are encoded to GSM 6.10 at their playback rate, as
the WaveSabre Specimen path does. One sampler instrument per stored sample keeps XM loops (forward) and envelopes as
sampler loop settings and gain automation.

Alternative: a raw PCM8 sample source, which is better quality but 5× larger. Revisit if parity on the drum bands
fails.

**Implemented (task 5.5).** `import::sampler(&Sample, rate)` keeps the stored PCM at its own rate (no resampling):
the sampler steps one sample per frame at note 60, so XM note `n` becomes unisynth note `n + shift` with
`11 + relative + finetune/128 + 12·log2(8363/rate)` split into the slot's octave (notes 1–96 stay in 0–127) and fine.
Loops as manual bounds (`trunc(len·p)` lands on the XM's sample), forward → repeat, ping-pong → ping-pong; capacity
1, velocity 0 (gain is automation), master √2, gate envelope with a 1 ms release. Measured on the three songs:
rendered C-4 against the decoded GSM ≥ 0.95 correlation; Stash's stored samples are exactly its drum kit (7 / 6).
GSM size: Jizz 14 032 samples → 3 130 B, song 1 14 654 → 3 381 B, BLUISH 14 341 → 3 288 B. The codec itself is
lossy on these (waveform correlation with the XM PCM 0.39–0.99; loudness −0.5 to −4.6 dB, octave bands mostly
within ±5 dB); the song parity (6.2/6.3) decides whether PCM8 is needed.

### D12 · The song (task 6.1)

`import::convert` puts it together: the call is Jizz's, or the Stash call whose generated + drum samples count the
XM's non-empty ones; every trigger's path; phrase tracks (`group`) with the record's instrument at the class ratio
and its chain, notes `lead − latency` early (one global `lead` = the longest chain latency + 2·`LANE_LEAD`, reported
as `lead-frames` for parity), the doubler restart toggled at each trigger; stored-sample tracks per (instrument,
sample) merged the same way, note on at the trigger, off at its path end, a `SMP_START` lane only for offsets; gain
(× √2 × `XM_GAIN`, the calibration of 6.2), pan and bend (range per track) as instrument automation `LANE_LEAD/2`
early; a master track with every track as a receive. **Tempo:** the tempo delay sizes every line for 16 eighths +
200 ms at the song tempo, so the song gets the fastest tempo reaching its longest echo up to `ECHO_MAX_MS` 4 s
(`tempo_for`); a longer echo (the risers' C-1 starts, ratio down to 0.015 → 20 s) clamps with a warning.
Measured (fast profile, one core): Jizz 58 + 13 tracks, 329 KB song, renders 166 s in 42 s at 91 MB; song 1 99 + 13
in 116 s for 204 s at 238 MB; BLUISH 55 + 12 in 26 s for 84 s at 76 MB. **Open (size/CPU, for the intro build):**
each stored-sample track carries its own copy of the GSM sample (Jizz: 13 tracks for 5 samples), and every phrase
track its own effect chain. First song check: Jizz is a uniform +7.6 dB over the reference (bands +5.4 to +8.6): the
calibration of 6.2.

### D13 · Calibration and the song parity (tasks 6.2, 6.3)

- **openmpt's level and pan law.** openmpt plays a file it detects as "FastTracker 2 or compatible" (all three songs)
  with the equal-power pan law: a full-volume channel is `XM_GAIN·(√(1 − p), √p)`, `XM_GAIN` = 0.375, whatever the
  channel count (measured on a one-note XM made from song 1: L² + R² constant across pans, pan 64 gives L/R = √3,
  a centred sample plays at 0.267 of its RMS per side). A file it cannot place, or takes for early ModPlug, it pans
  linearly at 0.375 or 0.25 hard: synthetic test XMs (one channel, or any odd count) are not detected as FT2, which
  first misled the calibration into a linear law at 0.5 and left hard-panned channels 3 dB hot. Pan is `pan/256`.
- **Volume after the chain** (phrases). The original's comb, echo and clamps are inside the sample the XM volume
  scales: an XM fade silences the echo with it. The voice runs at `1/max(√(1 − p), √p)` (√2 centred: its louder
  channel reaches the chain's clamps at the original's level), the pan lane is `p`, and a post-chain gain (EQ 3band,
  all bands equal: flat, 3 frames late, `POST_DELAY`) takes `vol·max(√(1 − p), √p)`, track volume `XM_GAIN`. The
  post gain steps a lane lead before a trigger and drops to 0 at its path end unless the next trigger starts by then.
  With the volume at the voice, song 1's echo tails rang on for seconds after the XM had faded them (+9 dB).
  Stored samples (no chain): the voice master takes `vol`, track volume `XM_GAIN`.
- **Slides resample.** An XM pitch slide plays the precalc sample faster or slower: pitch, filter, echo position
  and fades all move together. The cauldron slot's phrase clock and the voice's envelopes follow the voice-wide
  transpose + bend (unisynth), so a slide is a playback rate; a track's class (its built ratio) is the middle of the
  trigger's sounding ratios, raised so the top stays within reach, at most `MAX_CLASS` (+24); transpose (±24, its
  byte rounded away from zero) plus bend (±24) carry `±MAX_SHIFT` (48) from it. A trigger whose sounding ticks span
  more than `SEGMENT_SPAN` (94) **hands over**: `handovers` cuts it at tick boundaries into segments that each fit,
  every segment a trigger of its own (grouped and classed alone) whose path is the original's clipped to its time
  (`clip`: the first piece an entry at the step under it, envelope aged). The class is at least `MIN_CLASS` (−48: a
  track's comb and doubler delays, and the song's lead, grow as 1/ratio; an unfloored 5 Hz segment made song 1's
  lead 12 s); ticks past its reach (ratio < 1/256, sub-audio) stay silent with a warning, as do slides past the top.
  Song 1's muted time fell from 12.3 s (riser lows) to sub-audio depths and a stored drum's 179-semitone slide.
  Limit: an extreme slow-down's echo — baked into the original's sample and slowed with it (70× at song 1's 21.5 s,
  a 21 s delay) — no real-time delay can follow: the original's gaps between notes are filled by it, ours are
  near-silent there (record 1, 21.5–22 s, −38 vs −22 dB). Silent ticks (volume 0, or the 0 Hz a held ping-pong
  ends on) move neither class nor mute. Before this, a riser from 73 semitones down ran at a 520 Hz phrase clock
  and its bend up aliased into sub-bass.
- **Clicks** (song 1, heard at 15–19 s). Three sources, all band-limiting: (1) the post gain is an effect
  parameter, which steps once per sub-block: EQ 3band now ramps a gain change over its block (unisynth). (2) A slot
  whose phrase clock is below the output rate left the cubic's images above the clock's Nyquist: the cauldron slot
  low-passes its resampled output at `0.45·clock` (4th-order Butterworth, `CauldronLp`; bypassed at clock ≥ 1). (3)
  The original's clamps and wraps ran on the sample before decimation and playback, ours at the output rate: the
  band-limiting low-pass is two SVF stages (`LP_STAGES`, 24 dB/octave) and a sliding track gets it even when not
  decimated (`slide_lowpass`), its cut automated to the stored Nyquist times the playing ratio. Song 1's record 10
  (+48 wrap, decimated 4) went from 98 HF bursts to the reference's count. A cut restart: a ping-pong's reversed
  steps join unrelated step ends, and the one voice restarted with its filter output at 0, dropping the sounding
  note in a frame; the voice now carries the step from the cut note's last frame to the new note's first and fades
  it out over 1 ms (unisynth restart declick; a note from silence carries nothing): record 5 from 26 HF
  bursts to the reference's count, the goldens and phrases unchanged. Left: records 6 and 8 (8 / 82 vs 0 / 57):
  a reversed step's attack belongs at its end (D9: ours plays it forwards, the attack at its start); record 13
  (70 vs 42): openmpt ramps each new sample start (≈ 0.36 ms) and ours starts sharper; the drums at 24 s
  (interpolation edges of a 15.8 kHz playback that openmpt's sinc smooths).
- **Pan after the chain** (song 1, heard at 8–22 s). XM pan is the tracker's, applied to the finished sample; the
  voice pan put it before the comb, echo and doubler, which smeared each tick's pan into both sides (record 1's
  tremor pans hard left/right every tick: ours played it on both sides, +4 dB). A phrase voice now plays centred (√2,
  as the phrase tests) and the post-chain EQ 3band takes the gain (`vol`) and the pan (its new 4th parameter, an
  equal-power byte 1–255, 0 = off). Record 1 +4 → +1.7 dB.
- **Entering a phrase mid-way ages the filter envelope.** A jump (offset, loop, a ping-pong pass's reversed step)
  retriggers like the step that started the tie but restarted the cutoff envelope at the amount: a late step came out
  as bright as the first, and a low-damped record (song 1's record 14, damp 0.05) rang at 2.5–3.5 kHz (+30 dB) in its
  reversed passes. The entry note now carries the envelope's age, `1 + eighth steps` since that retrigger
  (`o::TRACKING` on a cauldron slot, `PEv::On::age`); the slot starts at `a·d^n`.
- **Controls only while sounding.** The replay runs a trigger on until its channel's next note; the lanes take only
  the ticks before the trigger's path end (another trigger merged onto the track sounds there).
- **Stored samples' GSM.** At its own rate GSM kept Jizz's vocals at 0.4 waveform correlation and lost 1–5 dB:
  `gsm_upsampled` encodes at the smallest ×1–4 windowed-sinc upsampling reaching 0.95 (the samples land at ×1–3,
  0.96–0.99) and the slot gain restores the level the codec loses. Stored-sample GSM: ~8 KB per song (was ~3.3 KB).
- **Filter saturation** (unisynth): see D4 / `docs/synth/unisynth.md` — Stash song 1's record 4, held 30 s at a
  rising cutoff, went to infinity and silenced the rest of the song.
- **Result** (`cauldron-report.md`): Jizz +0.45 dB, worst −2.94 dB at 16 kHz, alignment 0.65; song 1 +0.53 dB, worst
  +2.16 dB at 16 kHz, 0.65; BLUISH +0.36 dB, worst +1.26 dB at 2 kHz, 0.63; phrases 83 of 88. Two-second windows
  against the reference: Jizz 0 of 83 off by > 2 dB or a strong band > 4 dB, BLUISH 6 of 42, song 1 37 of 101.
  After the click fixes phrases are 85 of 88 (BLUISH 7/3 and 8/2 now pass); songs: Jizz +0.48 dB, song 1 +0.75 dB,
  BLUISH +0.31 dB. After pan-after-chain and the aged entry: Jizz +0.33, song 1 +0.38 (worst band 1.31 dB), BLUISH
  +0.16. After the handover (song 1 106 phrase tracks, BLUISH 64): song 1 +0.37 dB (worst band 1.30), BLUISH +0.13.
  After the portamento fix (Replay, below): song 1 +0.23 dB, worst band 0.73 dB.
- **Tone portamento keeps the playing instrument** (song 1, 38–51 s: a bright drum loop where openmpt is silent).
  FT2 (openmpt in FT2 mode, checked on a one-channel copy of the song) keeps the sounding sample *and* its instrument
  when a `3xx` cell carries another instrument number; the number only resets the volume and restarts that
  instrument's envelope. The replay took the new instrument's (no envelope), reviving channel 6's long-silent
  ping-pong drum loop at full volume for 12 s. Tried and dropped: a stored-sample low-pass at the playback Nyquist
  (it removed interpolation images above 16 kHz, but openmpt's sinc keeps part of them: the songs' 16 kHz bands
  fell 1–12 dB under) and two-voice retrigger crossfades (no measurable change: the loud hit edges start from
  silence).
  Song 1's by record (isolated renders): records 3, 4 and 10 carry their phrase deviations (D5/D11: 26–48 s, −2 to
  −4 dB); record 7's riser (92–98 s, 180 s) plays +2 to +6 dB and record 1's (16 s) +4 dB, open; the intro (0–8 s)
  and the last seconds differ in the top bands.

### D9 · Ping-pong loops play the steps in reverse order

A synth cannot play a phrase backwards. On a backward pass the converter emits the loop region's steps in reverse
order, each step playing forwards. Steps cut by the loop bounds keep their partial length. The converter warns once
per affected sample, and the parity report lists the seconds involved.

Alternatives considered:
- Holding the last step: silent motion.
- Rendering only those loops to samples: breaks "no precalc".

### D10 · XM instrument level → track gain

XM sample volume, volume column, `Axx`/`Cxx`, volume envelopes and fadeout all multiply to one gain. It is written as
voice-master automation at tick resolution, with points only where it changes. Panning (`8xx`, panning envelopes)
goes to voice pan the same way.

A single calibration constant maps the original 8-bit level × XM gain to unisynth's full scale. It is measured once
on the per-phrase suite.

### D11 · Reference and parity

**Reference.** `openmpt123 --render` of the XMs rebuilt from `ref/mcauldron*.py` (44.1 kHz). The **per-phrase**
reference is each phrase render at its native rate, resampled to 44.1 kHz.

**Metrics** (existing ones): loudness ≤ 1 dB, octave bands 63 Hz–16 kHz ≤ 3 dB, onsets within 100 frames.
- The phrase references are mono: our mid channel `(L + R)/2` is judged (BS.1770 loudness sums channels, so a
  stereo render of the same signal read +3 dB).
- `xm-phrase` starts the phrase after a lead-in (the chain's latency and the first lane step) and prints
  `lead-frames N`; the suite drops those frames.
- The per-phrase suite judges only octave bands lying wholly below the phrase's stored Nyquist (revised in task
  4.7; first: bands centred above it). Past the Nyquist, and in a band straddling it, the level is the playback and
  anti-alias filters' (our cubic resampler and 12 dB/octave low-pass, the reference's brick wall or ffmpeg's
  resampler), not the synth's: undecimated phrases flipped around −3.0 dB at the 16 kHz band with tiny changes.
- **Alias-free phrase references (task 4.7).** A decimated phrase is judged against the original at its full
  internal rate, taken just before the decimation and cut at the stored Nyquist (`ref44bw/`,
  `research/tbl-mini-cauldron/tools/fullrate_phrases.py`). The stored phrases fold up to +7 dB of aliasing into the
  octave bands below their Nyquist (measured per phrase), which the port drops by design. Undecimated phrases keep
  `ref44/`.
- The song suite reports the D9 seconds separately.
- The suite skips with a message when `research/tbl-mini-cauldron/out/` is absent.

## Risks / Trade-offs

- **[Resonant peaks above full scale]** (q = 0.001, e.g. Jizz rec 1: about 5× full scale before the clip) → the
  instrument's output clip at full scale stands in for the original's 16-bit store, as D6 plans.
- **[Many tracks]** (D2) → host-side only. The merge rule keeps the count down. The report prints the track count per
  song.
- **[Phrase timing drift]** Step boundaries computed from `r(t)` integration accumulate rounding → keep the phrase
  position in f64 internal frames, and place events on the nearest output frame.
- **[XM replay mismatch with libopenmpt]** in edge cases (`E9x` with volume, `EDx` past the row end) → onset metric
  catches it. The replay follows FT2 semantics as libopenmpt documents them, and only the effects listed in Context
  are accepted.
- **[Ping-pong approximation audible]** on the slow pads → reported, not hidden. A later change can add a
  per-sample exception list if the user wants those rendered.
- **[Copyright]** Converted songs are derivatives of the intros → they live in `out/` and are never committed, like
  the V2 corpus.

## Migration Plan

Additive: a new CLI subcommand, a new oscillator type at a free position, and new effect modes at free positions.
Existing songs keep their bytes and render unchanged. The existing goldens prove it.

## Open Questions

- The calibration constant (D10) is measured during implementation.
  They do not change the approach.

## Results: size and speed (task 7.3)

Code: `us-osc-cauldron` +13.8 K raw / +7.5 K xz / +5.6 K squishy, `us-fx-flanger-comb` +1.0 K / +0.7 K / +0.5 K,
`us-fx-distortion-wrap` +0.5 K / +0.4 K / 0; songs without them +512 raw / +256 xz, squishy unchanged. Whole
builds (squishy): Jizz 40.4 K, BLUISH 49.2 K, song 1 54.8 K, almost all of it the songs' per-tick automation. Against the original
(data, xz -9e): Jizz template 14.8 K + 1.5 K patches vs our song 17.9 K; Stash (both songs) 16.7 K + 2.7 K vs 60.3 K;
synth code ≈ 1.4 / 2 K vs ≈ 8.7 K xz. Speed:
render time / audio time Jizz 0.46, BLUISH 0.61, song 1 1.02 (one thread: song 1 is slower than real time). Neither fits a 64k intro as converted: the songs are
a faithful reference port; a compo use needs coarser automation and fewer tracks. Details:
`tools/unisynth-parity/size.md`.
