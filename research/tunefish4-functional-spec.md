> **Functional specification of Tunefish 4 (GPL-3, paynebc/tunefish), verified against the source. Our reimplementation is released under MIT: use the source to understand the algorithms, but write our own code. No copied code, comments, tables or identifiers.**

# Tunefish 4: Functional Specification

Status: behavioural spec for a clean-room reimplementation in the 64k player (Rust).
It describes what the synth does and the algorithms it uses, in our own words and in
standard DSP notation. Where an original value only matters because it is part of the
observable behaviour (parameter ranges, mapping curves, block sizes, file layout), the
value is stated. Where behaviour depends on hand-tuned constant tables, the spec says what
the table is for, its shape, and the public reference it comes from. It does not
reproduce the table.

Notation:
- `p` is a normalized parameter value in `[0, 1]` as stored in presets and songs.
- `M(target)` is the multiplicative mod-matrix factor for a destination (§3.8). It is
  1 when nothing is routed.
- `sr` is the sample rate. `N` is the processing block length in samples.
- `trunc` rounds toward zero. `round(x)` = `floor(x + 0.5)`.
- A "block" is one call of the instrument renderer. Many things update once per
  block (control rate), not per sample.

---

## 1. Overview and global constants

Tunefish 4 is a subtractive/additive hybrid. Each voice has one **spectral wavetable
oscillator** (the "generator"). It builds a magnitude spectrum from a few parameters,
inverse-FFTs it into a single-cycle 512-sample table, and plays that table back with
unison, detune and stereo spread. Each voice also has a stereo white-noise source and
four optional filters (LP, HP, BP, notch). Modulation comes from two ADSRs, two LFOs
and the mod wheel, routed through a small multiplicative mod matrix. Each instrument
has up to 10 insert effects in series.

| Constant | Value | Notes |
|---|---|---|
| Sample rate | host-defined; player uses **44 100 Hz** | Several parts are **not** rate-independent (§7). |
| Block size, plugin | **256** samples | Plugin renders in fixed 256-sample chunks through an adapter buffer. |
| Block size, song player | **512** samples | Control-rate behaviour therefore differs slightly between plugin and player (§7). |
| Max block size | 4096 | Upper bound for internal buffers. |
| Voices per instrument | 16 max; per-patch limit 1..16 (`GenPoly`) | |
| Instruments per song | 32 max | One plugin instance = one instrument. |
| Unison layers | 1..10 | |
| Max harmonics | 64 (+1, so 1..65 partials) | |
| Wavetable (IFFT) size | 512 complex points; playback uses the real part, 512 samples | |
| Noise table | 65 536 samples, looped | |
| LFO noise table | 256 random steps | |
| Mod matrix | 10 slots exposed as parameters, **only the first 8 evaluated** | See §7. |
| Effect slots | 10 per instrument, serial | |
| Parameters per instrument | **112** | §5 |
| Arithmetic | 32-bit float everywhere. Formant filter uses 64-bit state. Song player output is interleaved stereo signed 16-bit. | Flush-to-zero is enabled during rendering. |
| Tuning | 12-TET, A4 = 440 Hz (MIDI 0 ≈ 8.1758 Hz, MIDI n = 8.1758·2^(n/12)) | 128-entry note→Hz table. |

---

## 2. Signal flow

### 2.1 Per voice (one block)

```
 ┌─────────────────────── control rate (once per block) ───────────────────────┐
 │ Mod matrix: run the LFO1/LFO2/ADSR1/ADSR2/ModWheel sources used by slots 1..8 │
 │ → per-destination multiplicative factors M(·)                                  │
 │ Pitch: note→Hz, pitch-bend lerp, + slop, glide → currentFreq                    │
 │ Every 4th block: rebuild spectrum → phase "modulation" → IFFT → normalise      │
 └────────────────────────────────────────────────────────────────────────────────┘

  ┌───────────┐   overwrite   ┌───────────────────┐
  │  Noise    │──────────────▶│ voice stereo buf  │
  │ (+own LP/ │               │                   │◀── add ── ┌──────────────────────┐
  │   HP BP)  │               └─────────┬─────────┘           │ Generator × unison   │
  └───────────┘                         │                     │ (table read → drive  │
                                        ▼                     │  hard-clip → vol/pan │
                               [LP 4-pole ladder] if on       │  ramp)               │
                                        ▼                     └──────────────────────┘
                               [HP biquad]        if on
                                        ▼
                               [BP biquad]        if on
                                        ▼
                               [Notch biquad]     if on
                                        ▼
                               × Gain curve → add into instrument bus
```

### 2.2 Per instrument

```
 voices 0..15 (active only) ──Σ──▶ FX slot 1 ─▶ FX slot 2 ─▶ … ─▶ FX slot 10 ─▶ instrument out
                                     (each slot: none | distortion | delay | chorus |
                                      flanger | reverb | formant | EQ)
```

Effects run only while the instrument has not been silent for 2 s or more (§4.0).

### 2.3 Song player

```
 for each 512-sample block:
   fire all events whose time ∈ [t, t + 512/sr)   (per instrument, in stored order)
   for each instrument: render into a zeroed temp → add to master with mix factor 11
   master × (1000 · 2.0 · playerVolume) → clamp to int16 → interleaved output
   t += 512/sr
```

`playerVolume` defaults to 0.1, so the float→int16 factor is 200. Combined with the
mix factor of 11, a float sample of 1.0 inside an instrument becomes about 2200 LSB.
The plugin passes its float output straight to the host (no ×11, no ×200). Player
output is therefore about 23.5 dB below plugin full scale at default volume.

---

## 3. Modules

### 3.1 Generator: spectral wavetable oscillator

This is a parametric additive spectrum turned into a wavetable with an inverse FFT. It
is related to PADsynth (Nasca), where each partial is a bandwidth-widened bump in a large
spectrum. The differences: the table is small (one period), the bump shape is a Laplacian
(two-sided exponential) instead of a Gaussian, and the phases are not uniformly random
(see 3.1.3).

#### 3.1.1 Spectrum construction

The spectrum is held as a 512-bin complex array. **Bin k becomes k cycles per table
period**, so on playback at fundamental f it sounds at k·f.

Inputs per voice (each clamped after mod-matrix multiplication):
- harmonics `η = clamp(p_GenHarmn · M(NumHarmonics), 0, 1)` gives partial count
  `H = 1 + min(trunc(64·η), 64)` (1..65).
- bandwidth `β = clamp(p_GenBandw · M(Bandwidth), 0, 1)`.
- damp `δ = clamp(p_GenDamp · M(Damp), 0, 1)`.
- scale `s = clamp(4·p_GenScale · M(Scale), 0, 4)`.
- active bin count `K` (a band-limit heuristic):
  `r = clamp((currentFreq − 8)/2000, 0, 1)`, `K = max(4, trunc(512 · (1 − r)^3))`.
  Low notes fill all 512 bins. Notes at or above about 2008 Hz fill only 4.
  `currentFreq` here is the glided note frequency **before** the octave multiplier and
  ignores fixed-frequency mode (§7).

For each partial h = 1..H:
- centre position, in fractional bins: `c_h = 1 + s·(2h − 1)`
  (s = 0 puts every partial on bin 1. s = 1 puts partials on even bins 2, 4, 6…, which
  is a harmonic series an octave up. s = 0.5 puts them half-way between bins h and h+1.
  Other values give inharmonic, stretched or compressed spectra).
- width, in bins: `w_h = 0.3 + β·h` (width grows with partial number, as in PADsynth).
- amplitude: `a_h = h^−(1+δ)` (1/h rolloff at δ = 0, 1/h² at δ = 1).

For each bin k = 0..K−1 the magnitude is

  `A_k = Σ_h a_h · exp(−|k − c_h| / w_h)`, summing only terms with `|k − c_h| / w_h < 5`.

Bins ≥ K are zero. The exponential can be a lookup table with ≥ 4096 points over
`[0, 5)`. The original quantizes the exponent argument to 1/819 steps.

Both the real and imaginary part of bin k are set to `A_k` (constant 45° phase before
the phase stage). Bin 0 (DC) is forced to 1 + 0i.

The spectrum is rebuilt only when H, K, β, δ or s changed since the last rebuild
(float tolerance 1e-5). All three steps below happen on the same every-4th-block
cadence.

#### 3.1.2 Update cadence

The table pipeline (spectrum → phase stage → IFFT → normalise) runs on the first block
after note-on and then every 4th block (blocks 1, 5, 9… counted from note-on). That is
every 1024 samples in the plugin and every 2048 in the player. Between updates the voice
plays the last table. A new table replaces the old one instantly with no crossfade, which
can produce audible stepping under fast modulation. This is part of the character.

#### 3.1.3 Phase stage ("GenMod")

Each voice keeps a running phase-animation value `μ`. It is initialised to a uniform
random value in [0, 100) on every note-on. The synth also keeps a global per-bin random
vector `ρ_k` for k < 512. Each entry is `sin(uniform[0, 2π))`, fixed at synth start.

Let `g = p_GenMod`. For each bin k = 0..511:

- If g = 0, the phase index is `φ_k = 0`.
- Otherwise `φ_k = trunc( μ·512·(1 − g·ρ_k) · (k/512) ) mod 512`.
- Real part ← `Re_k · S(φ_k)`; imaginary part ← `Im_k · S((φ_k + 128) mod 512)`,
  where `S(i) = sin(2π·i/4096)`.

DC is then forced back to 1 + 0i.

**Important quirk (must be reproduced for sonic parity):** the sine table is indexed with
a period of 512 but was built with a period of 4096. Both multipliers therefore only
cover sin(0..π/4), i.e. the range 0..≈0.707. They are never negative. So the "phase"
stage does not rotate phases around the circle. It applies a pseudo-random, bin-dependent
**gain tilt and partial real/imag rebalance**. With g = 0 every real part becomes 0 and every
imaginary part is scaled by the constant sin(π/16) ≈ 0.195, so every bin has the same
phase. That gives a deterministic waveform. Treat this as specified behaviour. A
"correct" full-circle version sounds different.

After each update `μ ← μ + g³/100`, wrapping at 100. So `GenMod` sets both the amount of
per-bin randomisation (g) and the slow drift speed of the timbre (g³).

#### 3.1.4 Inverse FFT and normalisation

- Standard radix-2 complex FFT of size 512 with positive exponent (inverse direction).
  No 1/N scaling.
- Keep only the **real part** of the 512 outputs as the wavetable. The input spectrum is
  not Hermitian, so this real part is the cosine/sine mix implied by the complex bins.
  Bins above 256 act as their mirrored frequency (512 − k).
- Peak-normalise the table to a maximum absolute value of 1 (peak floor 1e-5). Then
  subtract the table mean (DC removal after normalisation, so the peak can drift slightly
  from 1).

#### 3.1.5 Playback, pitch, unison, stereo

Per block:

- Unison count `U = round(9·p_GenUnis) + 1` (1..10).
- Detune, Hz: `D = (p_GetDetn · M(Detune))³ · 10`.
- Drive gain: `G = 1 + 32 · p_GenDriv · M(Drive)`.
- Spread, Hz per layer step: `Δ = (p_GenSprd · M(Spread))⁴ · 10`.
- Octave: `o = round(8·p_GenOct)` gives multiplier `2^(4 − o)` (×16 … ×1/16; p = 0.5 is ×1).
- Frequency mode: if `p_GenFreq ≥ 1e-5` the oscillator is **fixed** at
  `F = p_GenFreq² · 1000` Hz for both channels. It ignores note, octave and detune;
  spread still applies. Otherwise it **tracks the keyboard**:
  left base `F_L = currentFreq·2^(4−o) + D`, right base `F_R = currentFreq·2^(4−o) − D`.
- Both bases are multiplied by `M(Freq)`. Negative frequencies are made positive.
- Unison layer j = 0..U−1 uses left `F_L + j·Δ` and right `F_R − j·Δ`.
  Left and right are **separate phase accumulators**, so detune and spread make the
  stereo image.
- Initial phases (every note-on): for each layer a uniform random base b ∈ [0, 1).
  Left starts at b, right at b + uniform[0, 0.1). Phases wrap into [0, 1) after each
  increment.
- Table read: non-interpolated. Index `trunc(phase · 511)` into the 512-sample table.
- Per-sample output per layer: `clip(G · table[·], −1, 1) · v(t)`, summed over layers.
  There is no 1/U normalisation, so more unison means louder.
- Volume: `V = p_GenVol · 4 · velocity · M(Volume) · 0.1`, with velocity = MIDI vel/128.
  Pan `π = clamp(p_GenPan · M(Pan), 0, 1)` uses a linear balance law:
  left = V·(π ≤ 0.5 ? 1 : 2(1 − π)), right = V·(π ≥ 0.5 ? 1 : 2π).
  The stereo gain ramps linearly across the block from the previous block's gains to the
  new ones (de-zippering). Gains reset to 0 at note-on, so every note fades in over its
  first block.
- If both the target and previous volumes are below 0.001 the generator is skipped for
  the block.

### 3.2 Noise

- Source: a 65 536-sample looped white-noise table, approximately **uniform** in
  [−1, 1] (see §7: the intended triangular distribution degenerates to uniform). Left and
  right read the same table from independent random start offsets in [0, 32768),
  re-randomised at each note-on, so the stereo noise is decorrelated.
- Level: `n = p_NoiseOn · velocity · M(NoiseAmount)`. If `n ≤ 0.01` the noise stage
  writes silence. The noise stage **overwrites** the voice buffer; the generator is added
  on top afterwards.
- Colouring: if `p_NoiseBw < 0.99`, the noise goes through its own LP and then HP,
  forming a band-pass. Both use the voice filter algorithms in §3.3 with resonance
  parameter 0.05. LP cutoff parameter = `NoiseFreq + NoiseBw`, HP cutoff parameter =
  `NoiseFreq − NoiseBw`, each clamped to [0, 1] and then mapped through that filter's own
  cutoff curve. `p_NoiseBw ≥ 0.99` means unfiltered.
- Noise does not go through drive or pan. It does go through the voice filters.

### 3.3 Filters (per voice, stereo, coefficients updated once per block)

Each filter has an On switch (on when p > 0.5). The cutoff and resonance/Q parameters
are each multiplied by their mod-matrix factor, then clamped: cutoff to [0, 1] and
resonance/Q to [0, 0.85]. Fixed order: **LP → HP → BP → Notch**.

**LP: 4-pole "Moog-style" ladder.** This is the widely published musicdsp.org "Moog VCF"
digital approximation: four cascaded one-pole sections with a two-tap (current + previous
input) structure, global negative feedback from the 4th stage, and a cubic soft
saturation `y − y³/6` on the output only (the feedback path is not saturated).
- Cutoff: `fc = p² · 20 000 + 30` Hz; `f = 2·fc/sr`.
- Tuning polynomial: `k = 3.6f − 1.6f² − 1`; `p = (k + 1)/2`; resonance
  `r = q · e^{1.386249·(1 − p)}`, with q the clamped resonance parameter.
- Each stage computes `y_i = p·x_i + p·x_i[n−1] − k·y_i[n−1]`, and the input is
  `x_1 = in − r·y_4`.
- Self-oscillation is possible near q = 0.85 at low cutoffs.

**HP and BP: RBJ Audio-EQ-Cookbook biquads** (Bristow-Johnson), Direct Form I.
- Cutoff: `f0 = p² · 10 000 + 30` Hz (note: half the LP range). `ω0 = 2π·f0/sr`.
- The bandwidth form of α is used with bandwidth `BW = 1 − q`, but with **log₁₀ 2**
  where the cookbook has ln 2:
  `α = sin ω0 · sinh( (log₁₀2 / 2) · BW · ω0 / sin ω0 )`. This makes the effective
  bandwidth about 0.434× the cookbook value, so these filters are more resonant than the
  labels suggest. Reproduce it.
- HP: cookbook high-pass. BP: cookbook band-pass, "constant skirt gain, peak gain = Q"
  variant.

**Notch: two-pole/two-zero resonator notch.**
- Cutoff: `f0 = p² · 20 000 + 30` Hz.
- Zeros on the unit circle at ±ω0. Poles at the same angle with radius q (the clamped
  Q parameter, 0..0.85). Higher Q gives a narrower notch.
  Denominator `1 − 2q·cos ω0·z⁻¹ + q²·z⁻²`. Numerator `b0·(1 − 2cos ω0·z⁻¹ + z⁻²)`,
  with the heuristic passband gain `b0 = (1 − q)² / (2(|cos ω0| + 1)) + q`.
- The original adds one sample of latency to this filter (its input history is one
  sample behind). This is inaudible, but it matters for bit-exact comparison.

### 3.4 ADSR envelopes (2 per voice)

**There is no hard-wired amplitude envelope.** An envelope only exists for a voice if a
mod-matrix slot (1..8) uses it as a source. Typical patches route ADSR1 → Volume.

Envelopes are evaluated **once per block** and hold their value for the block. The
generator's per-block volume ramp smooths the steps.

Let `σ = 0.0005 · N · (sr / 44 100)` (per-block step scale; see §7 about the sr factor).
Parameters a, d, s, r and slope are the five `EnvX…` values. `m_d = M(ADSRx_Decay)`.

- Attack increment per block: `α = −log₁₀(0.94·max(a³, 1e-9)) · σ`.
  Linear rise from 0 to 1, then go to Decay.
  At 44.1 kHz with N = 512: a = 0 reaches 1 in one block; a = 1 takes about 145 blocks
  (≈ 1.7 s).
- Decay base step: `δ = ¼ · log₁₀(0.94·max((d·m_d)³, 1e-9)) · σ`. This is negative for
  d·m_d ≲ 1.02.
- Sustain level `S = min(s, 0.99)`.
- Release base step: `ρ = ¼ · log₁₀(0.94·max(r³, 1e-9)) · σ`.
- **Decay:** each block, `v ← v + δ · ((0.01 + v − S)/(1 − S))^slope`. When v ≤ S,
  clamp to S and go to Sustain. With slope = 0 the decay is linear. With slope = 1 it
  approaches S roughly exponentially. Values in between interpolate the curvature.
- **Sustain:** follows changes in S. If v differs from S it moves toward S by |δ| per
  block without overshooting.
- **Release** (on note-off, from any phase): `v ← v + ρ · v^slope`. When v ≤ 1e-5, set
  to 0 and Finished.
- **Note-on** always restarts at v = 0 in Attack (hard retrigger, even on a stolen voice).

### 3.5 LFOs (2 per voice)

Also evaluated once per block, and only if some active mod slot uses them as a source.

- Rate: phase advances by `50·p_rate² · N / sr` radians per block, so
  `f_LFO = 50·p² / 2π ≈ 7.96·p²` Hz (0..≈8 Hz). Phase wraps at 2π.
- Shape `round(4·p_shape)`:
  0 = sine mapped to [0, 1];
  1 = rising ramp [0, 1);
  2 = falling ramp (1, 0];
  3 = square (1 in the first half-cycle, 0 in the second);
  4 = stepped random (256 random values in [0, 1) indexed by phase, so the pattern
  repeats every cycle; the table is regenerated at synth start).
- Depth: effective depth `d = (p_depth · M(LFOx_Depth))²`. Output `= d·shape + (1 − d)`,
  which lies in `[1 − d, 1]`. The LFO is **unipolar and multiplicative**: it can only
  scale a destination down from its set value (unless the slot amount > 1, §3.6).
- Sync (`LfoXSync`): if < 0.5, a new voice starts at the instrument's most recently
  stored LFO phase. After every voice renders, the instrument stores that voice's phase,
  so the last voice processed in the block wins. This approximates a free-running LFO.
  If ≥ 0.5, the phase resets to 0 at note-on.

### 3.6 Mod matrix

Ten slots appear in the parameter set (`MmNSource`, `MmNMod`, `MmNTarget`). **Only slots
1–8 are evaluated.**

Sources, `round(14·p)`: 0 none, 1 LFO1, 2 LFO2, 3 ADSR1, 4 ADSR2, 5 ModWheel (CC1/127),
6–14 reserved (no effect).

Targets, `round(39·p)`:

| idx | target | idx | target |
|---|---|---|---|
| 0 | none | 16 | BP cutoff |
| 1 | Gen bandwidth | 17 | BP Q |
| 2 | Gen damp | 18 | Notch cutoff |
| 3 | Gen num harmonics | 19 | Notch Q |
| 4 | Gen scale | 20 | ADSR1 decay |
| 5 | Gen volume | 21 | ADSR2 decay |
| 6 | Gen frequency | 22–29 | Mod amount of slot 1..8 |
| 7 | Gen pan | 30 | LFO1 depth |
| 8 | Gen detune | 31 | LFO2 depth |
| 9 | Gen spread | 32–39 | reserved |
| 10 | Gen drive | | |
| 11 | Noise amount | | |
| 12 | LP cutoff | | |
| 13 | LP resonance | | |
| 14 | HP cutoff | | |
| 15 | HP resonance | | |

Amount mapping (`MmNMod`): `A = 2p` for p ≤ 0.5 (0..1); otherwise
`A = 1 + 9·(2p − 1)²` (1..10).

Evaluation, once per block per voice:
1. For each slot i = 1..8 in order, its result is `R_i = A_i · src_i · Σself_i`. Each
   source is computed at most once per block (on its first use). `Σself_i` is the
   self-modulation factor from the previous block (initially 1). Slots with no or
   reserved source have `R_i = 1`.
2. `M(target) = ∏ R_i` over all slots whose target matches. It is 1 if none match.
3. After all slots: `Σself_i ← M(ModAmount_i)` for the next block (one-block latency).

So modulation is **purely multiplicative**: `effective = parameter × ∏(amount × source)`.
With amount 0.5 (→ A = 1) and an ADSR source, the destination equals parameter ×
envelope. A slot only lowers its destination unless its amount is above 0.5.
`M(LFOx_Depth)` and `M(ADSRx_Decay)` are read while slots are still being updated within
the same block, so slots earlier in the list affect them this block and later slots
next block.

The original also has signed/inverted range modes for reading M, but no destination uses
them. Only the plain product above is needed.

### 3.7 Voice allocation, stealing, lifecycle

- Polyphony limit `P = trunc(15·p_GenPoly + 1)` (1..16). Only voices 0..P−1 are
  candidates.
- Note-on: take the first candidate that is neither held nor sounding. If none is free,
  **steal the candidate with the largest age** (age = blocks rendered since its last
  note-on; ties go to the lowest index). The stolen voice hard-restarts: envelopes from
  0, new random oscillator phases, new noise offsets, volume ramp from 0. Filter state is
  kept.
- Note-off (velocity 0 in songs): every held voice playing that note number is released.
- A voice renders while it is held **or** while it is "sounding". After each block,
  "sounding" is set to `Σ|L| + Σ|R| > 1.0` over the block's mixed voice output.
- Release without an envelope: once the note is off and no envelope it uses is still
  running, the voice's effective velocity becomes 0. Generator and noise then fade to
  silence over one block and the voice frees itself. So a patch with no ADSR routed
  behaves like an organ gate.
- All-notes-off releases (it does not kill) all held voices. Panic only affects held
  voices.
- Per-voice mix: the voice buffer is scaled by the **Gain curve**
  `g ≤ 0.5: (2g)²; g > 0.5: 1 + 20(g − 0.5)` (0..11), then summed into the instrument bus.

### 3.8 Pitch: note, bend, slop, glide

Computed once per block per voice:

1. `base = Hz(note)` from the 12-TET table (note masked to 0..127).
2. **Pitch bend:** up range `U = trunc(12·p_OscPWup + 1)` semitones, down range
   `Dn = trunc(12·p_OscPWdw + 1)` (1..13). Let b ∈ [−1, 1] be the bend
   (MSB-derived: `(msb/127 − 0.5)·2`; the LSB is ignored).
   For b < 0 interpolate **linearly in Hz** between `base·2^(−Dn/12)` and base. For
   b > 0 interpolate linearly between base and `base·2^(U/12)`. Linear-in-Hz means
   mid-bend pitches are slightly flat versus an exponential bend. The bend applies to all
   16 voices, held or not. The song player never sends bend.
3. **Slop:** `+ ξ · p_GenSlop³ · 8` Hz, with ξ uniform in [−1, 1] drawn per note-on. A
   constant per-note detune in Hz, so it matters more at low notes.
4. **Glide:** if `p_GenGlid > 0` and the voice's current frequency is > 0:
   `cur ← cur + (target − cur)/(10·p_GenGlid + 1)` once per block (a one-pole slew at
   block rate). Otherwise `cur = target`. Glide state is **per voice** and not reset at
   note-on, so the glide starts from whatever that voice last played (voice-allocation
   dependent; it glides from the previous note only when the same voice is reused,
   e.g. with `GenPoly` = 1). Initialise to 0 so that a voice's first note does not glide.

Mod wheel: CC1 → value/127, available as mod source 5. It is instrument-wide and
takes effect at the next block.

---

## 4. Effects

### 4.0 Slot mechanics

- Ten slots, processed in order 1 → 10 on the summed stereo instrument bus after all
  voices.
- Slot type `round(10·p)`: 0 none, 1 Distortion, 2 Delay, 3 Chorus, 4 Flanger,
  5 Reverb, 6 Formant, 7 EQ, 8–10 reserved (= none).
- The same type can appear in several slots. Each slot has its own state but **all
  instances share the instrument's single set of effect parameters**.
- Changing a slot's type destroys its state and creates a fresh, zeroed instance.
- Auto-sleep: each block, the mean absolute level of the instrument output (after
  effects, averaged over L and R) is measured. If it is below 1e-5, an inactivity timer
  increases by N/sr; it resets to 0 whenever any voice renders. Once the timer reaches
  **2 s**, effects are no longer processed and their state stays frozen (stale delay or
  reverb contents can resurface when they wake up).

### 4.1 Distortion: power-law waveshaper

`y = sign(x) · min(|x|, 1)^(1 − p_DistAmnt)` (sign(0) = 0). At p = 0 it is transparent
for |x| ≤ 1 (and hard-clips beyond 1). As p → 1 it approaches a square wave (|y| → 1 for
any nonzero x). The original uses a 32 768-entry lookup, so |x| is quantised to 15 bits.
Lookup or direct pow is acceptable.

### 4.2 Delay: feedback delay, independent L/R times

- Left delay `L = trunc(sr · trunc(1000·p_DelayL)/1000)` samples, with ms quantised to
  integers (0..1000 ms, minimum 1 sample). Right is the same with `DelayR`.
- Feedback factor `k = p_DelayDc`.
- Per channel: the line stores `b[n] = k·(x[n] + b[n − L])` and outputs
  `y[n] = x[n] + b[n]`. Read and write are at the same position, so the dry signal also
  appears once immediately with gain k. **Quirk:** the dry is effectively boosted by
  (1 + k), and echoes at L, 2L, … have gains k², k³, ….
- Changing the time while running clamps the read and write positions into the new
  length without clearing the buffer (a glitch or jump, not a smooth pitch sweep).

### 4.3 Reverb: Freeverb topology (Jezar, public domain)

- Standard Freeverb: 8 parallel lowpass-feedback combs into 4 series Schroeder allpasses,
  per channel. Use **Freeverb's published comb and allpass lengths** (44.1 kHz values,
  not rescaled for other rates).
- Differences from stock Freeverb:
  - Both channels use **identical comb lengths**. The +23-sample stereo spread applies
    only to the right channel's allpasses.
  - Input to both channels is `(L + R) · 0.015`.
  - Allpass feedback is 0.5. Allpass form: `out = buf − in`, `buf ← in + 0.5·buf`
    (Freeverb's form).
- Parameter mapping:
  comb feedback = `0.7 + 0.28·p_RevRoom`; damping d = `0.4·p_RevDamp`, with comb
  lowpass `z ← (1 − d)·comb_out + d·z`; wet `W = 3·p_RevWet`;
  dry `= 2·(1 − p_RevWet)`; width w = `p_RevWidth`:
  `W1 = W·(w/2 + 0.5)`, `W2 = W·(1 − w)/2`.
- Output: `L' = W1·revL + W2·revR + dry·L` and `R' = W1·revR + W2·revL + dry·R`.
  At RevWet = 0 the dry signal is doubled (×2).

### 4.4 Chorus: three cascaded modulated taps per channel

- Six single-tap delay lines, three per channel. Lines 1, 3, 5 process left in series and
  lines 2, 4, 6 process right. Each line computes `y = x + g·x[n − d]` and feeds the next
  line, so the copies multiply (dry, three first-order copies, and their cross terms).
- Tap gain `g = 0.7·p_ChrsGain`. There is no feedback.
- Each line has its own sine LFO. Initial phase is uniform in [0, 1) **radians**
  (random per instance). Phase advance is `50·p_ChrsRate²·N/sr` rad per block (same
  rate law as the voice LFOs, ≈ 0..8 Hz).
- Delay per block, in ms: `d = clamp(1 + 9·p_ChrsDep·(sin φ + 0.5), 1, 10)`. The intended
  offset of the sine into [0, 1] was mis-scaled to `sin φ + 0.5` ∈ [−0.5, 1.5], so the
  lower part of each cycle is clamped at 1 ms (a flat spot). Reproduce it.
- The delay changes once per block as a non-interpolated jump of the read point, which
  gives slight zipper artefacts.

### 4.5 Flanger: negative-feedback modulated comb with clipping

- Per channel: `y = clip(x − wet·buf[pos − Δ], −1, 1)`. y is written back into a
  4096-sample ring buffer (feedback flanger with **inverted** feedback). The delay is
  integer and non-interpolated. `wet = p_FlangWet`.
- Delay: `Δ = 0.1 ms + p_FlangAmp · 12 ms · (1 − cos θ)/2`, converted to samples (range
  0.1..12.1 ms).
- Sweep: a bounce counter c rises and falls between 0 and an upper turn point.
  - It moves by `p_FlangLfo · 0.1 · N` **per sample**, which depends on block size.
  - Rising: when c > `p_FlangFreq` it jumps to 1 and starts falling.
  - Falling: when c < 0 it jumps to 0.01 and starts rising.
  - Let `f = c` (or 1 if c = 0).
  - The left sweep phase θ_L advances by about `π·f/(2·sr)` per sample. The right sweep
    phase θ_R advances by about `π·(1 − f)/(2·sr)`. This "fast" rate only applies on
    samples where f changed. While f is constant, the phases advance 120× slower
    (`π·f/(240·sr)`).
- Net observable behaviour: for almost any FlangLfo > 0 the counter flips between about
  1 and 0.01 every sample. Both channels then sweep at roughly **0.12–0.13 Hz**, nearly
  independent of FlangFreq. With FlangLfo = 0 the left channel sweeps extremely slowly
  (period ≈ 480 s), and so does the right channel, in step with it (corrected 2026-10-25: the running
    delay term of both channels uses f, not 1 − f; only the fold on a change of f uses 1 − f for the right).
  - **Requires measurement against the reference.** An implementer may model it as two
    slow cosine sweeps whose rates follow the formulas above.

### 4.6 Formant: vowel filter

- A **10th-order all-pole IIR** (direct form, 64-bit state per channel), one coefficient
  set per vowel. These are the widely circulated "Formant filter" coefficients for vowels
  **A, E, I, O, U** from the musicdsp.org archive (designed for 44.1 kHz). Obtain them
  from that public source. Use a gain coefficient plus 10 feedback coefficients per
  vowel.
- Vowel = `trunc(4·p_FrmtType)` (0..4; p = 1 → U).
- Mix: `y = (1 − w)·x + w·filtered`, with `w = p_FrmtWet`.
- The filter is not rate-compensated.

### 4.7 EQ: 3-band splitter (musicdsp "3 Band Equaliser" style)

- Low band: four cascaded one-pole lowpasses at 880 Hz, coefficient `2·sin(π·880/sr)`.
- High band: (input delayed 3 samples) minus four cascaded one-poles at 5000 Hz.
- Mid band = (input delayed 3 samples) − low − high.
- Band gains (Low, Mid, High): `p ≤ 0.5: 2p` (0..1); `p > 0.5: 1 + 10·(2p − 1)²` (1..11).
  p = 0.5 is unity.
- Output = sum of the three scaled bands. With all gains at 1 the output is not exactly
  the input: the low band is not delay-aligned, giving mild comb colouring.

---

## 5. Parameter table (index order = preset and song order)

All values are normalized p ∈ [0, 1]. The display name is the plugin's public parameter
name (keep it for preset compatibility; index 10 really is spelled `GetDetn`). In the
"Default" column, "—" means the engine has no built-in default: the plugin initialises
from its factory bank, program 0. Suggested neutral values for a new implementation are
given where useful.

| # | Name | Mapping (see sections) | Suggested neutral |
|---|---|---|---|
| 0 | Gain | voice gain `(2p)²` / `1+20(p−0.5)` | 0.5 (×1) |
| 1 | GenBandw | partial width `0.3+p·h` bins | — |
| 2 | GenHarmn | partials `1+trunc(64p)` | — |
| 3 | GenDamp | rolloff `h^−(1+p)` | — |
| 4 | GenMod | phase-tilt randomness p, drift p³/100 per update | 0 |
| 5 | GenVol | `0.4·p` × velocity × mod | — |
| 6 | GenPan | linear balance | 0.5 |
| 7 | GenSlop | ±`8p³` Hz per note | 0 |
| 8 | GenOct | `2^(4−round(8p))` | 0.5 |
| 9 | GenGlid | per-block slew `1/(10p+1)`; 0 off | 0 |
| 10 | GetDetn | ±`10p³` Hz L/R | 0 |
| 11 | GenFreq | 0 = key track; else fixed `1000p²` Hz | 0 |
| 12 | GenPoly | voices `trunc(15p+1)` | — |
| 13 | GenDriv | pre-clip gain `1+32p` | 0 |
| 14 | GenUnis | layers `round(9p)+1` | 0 |
| 15 | GenSprd | per-layer ±`10p⁴` Hz | 0 |
| 16 | GenScale | partial position `1+4p(2h−1)` | — (p = 0.25 gives s = 1: even bins, harmonic series an octave up) |
| 17 | NoiseOn | noise level p (≤0.01 = off) | 0 |
| 18 | NoiseFreq | band centre in filter-param domain | — |
| 19 | NoiseBw | half-width; ≥0.99 = unfiltered | 1 |
| 20 | FltLPOn | on if >0.5 | 0 |
| 21 | FltLPCut | `20000p²+30` Hz | — |
| 22 | FltLPRes | r from q=min(p,0.85) | — |
| 23 | FltHPOn | on if >0.5 | 0 |
| 24 | FltHPCut | `10000p²+30` Hz | — |
| 25 | FltHPRes | BW = 1−min(p,0.85) | — |
| 26–30 | Env1Att, Env1Dec, Env1Sus, Env1Rel, Env1Slp | §3.4 | — |
| 31–35 | Env2Att, Env2Dec, Env2Sus, Env2Rel, Env2Slp | §3.4 | — |
| 36–39 | Lfo1Rate, Lfo1Dep, Lfo1Shp, Lfo1Sync | §3.5 | — |
| 40–43 | Lfo2Rate, Lfo2Dep, Lfo2Shp, Lfo2Sync | §3.5 | — |
| 44–73 | Mm1Source, Mm1Mod, Mm1Target … Mm10Sourc, Mm10Mod, Mm10Targe | triplets (source, amount, target); §3.6. Slots 9 and 10 are inert. | source 0, amount 0.5, target 0 |
| 74–83 | Effect1 … Effect10 | type `round(10p)`; §4.0 | 0 |
| 84 | DistAmnt | exponent `1−p` | 0 |
| 85 | ChrsRate | ≈`7.96p²` Hz | — |
| 86 | ChrsDep | 1..10 ms swing | — |
| 87 | DelayL | `trunc(1000p)` ms | — |
| 88 | DelayR | `trunc(1000p)` ms | — |
| 89 | DelayDc | feedback p | — |
| 90 | RevRoom | `0.7+0.28p` | — |
| 91 | RevDamp | `0.4p` | — |
| 92 | RevWet | wet `3p`, dry `2(1−p)` | — |
| 93 | RevWidth | stereo width | — |
| 94 | FlangLfo | counter speed (§4.5) | — |
| 95 | FlangFreq | counter turn point | — |
| 96 | FlangAmp | sweep depth 0..12 ms | — |
| 97 | FlangWet | inverted feedback | — |
| 98 | ChrsGain | tap gain `0.7p` | — |
| 99 | FrmtType | vowel `trunc(4p)` A/E/I/O/U | — |
| 100 | FrmtWet | linear mix | — |
| 101 | EqLow | `2p` / `1+10(2p−1)²` | 0.5 |
| 102 | EqMid | same | 0.5 |
| 103 | EqHigh | same | 0.5 |
| 104 | OscPWup | `trunc(12p+1)` semitones | ≈0.083 (2 st) |
| 105 | OscPWdw | `trunc(12p+1)` semitones | ≈0.083 (2 st) |
| 106 | FltBPOn | on if >0.5 | 0 |
| 107 | FltBPCut | `10000p²+30` Hz | — |
| 108 | FltBPQ | BW = 1−min(p,0.85) | — |
| 109 | FltNTOn | on if >0.5 | 0 |
| 110 | FltNTCut | `20000p²+30` Hz | — |
| 111 | FltNTQ | pole radius min(p,0.85) | — |

Plugin preset state is stored as named float attributes, keyed by the display names
above. The factory bank has 1000 programs × 112 values. It is not reproduced here.

---

## 6. Song format (.tfm) and player scheduling

### 6.1 Byte layout

Little-endian throughout. No padding. Tags are 4 ASCII bytes.

| Order | Field | Type | Count | Meaning |
|---|---|---|---|---|
| 1 | instrument count `I` | u16 | 1 | Number of recorded instruments (≤ 32). Recorder slots are compacted in slot order. |
| 2 | tempo | u16 | 1 | BPM, integer. Truncated from the host BPM at recording time. |
| 3 | event counts | u16 | I | Events per instrument, `E_i` (≤ 65 535). |
| 4 | tag | 4 bytes | 1 | `"INST"` |
| 5 | parameters | u8 | I × 112 | Instrument-major (all 112 params of instrument 0, then instrument 1, …). Value = `trunc(p·100)` ∈ 0..100. Loader uses `p = byte/100`. |
| 6 | tag | 4 bytes | 1 | `"SONG"` |
| 7 | per instrument i = 0..I−1, three arrays in sequence: | | | |
| 7a | row deltas | u16 | E_i | Row delta from the previous event of the same instrument (first is from row 0). |
| 7b | notes | u8 | E_i | MIDI note 0..127. Note 0 is ignored on playback. |
| 7c | velocities | u8 | E_i | 1..127 = note-on with that velocity; 0 = note-off. |
| 8 | tag | 4 bytes | 1 | `"ENDS"` |

The structure-of-arrays layout (deltas, then notes, then velocities) is designed to
compress well with the executable packer. Total size =
`4 + 2I + 4 + 112I + 4 + Σ 4·E_i + 4` bytes.

Recorder-side preprocessing before writing parameters (zeroing unused values to improve
compression; it has no audible effect, with one exception noted in §7):
- noise freq/bw → 0 if noise amount < 0.01;
- LP/HP cutoff/res → 0 if that filter is off;
- generator params 1–4, 7–11, 13–16 → 0 if GenVol < 0.01;
- a mod slot's source, amount and target → 0 if its source or target < 0.01;
- all params of LFO1/LFO2/ADSR1/ADSR2 → 0 if no mod slot (out of all 10) uses that
  source.

### 6.2 Timing

- 4 rows per beat. `secondsPerRow = 60 / (tempo · 4)`.
- Event time = `absoluteRow · secondsPerRow + loadOffset`, where `loadOffset` is a
  caller-supplied start delay in seconds.
- Recorder: `row = round(eventTimeSeconds · rowsPerSecond)`. Events snap to 1/16 notes
  at the recorded tempo, so anything off-grid (swing, humanisation) is quantised. Tempo
  changes during recording are not represented. The last BPM seen is used.
- Player loop: each block covers `[t, t + 512/sr)`. Every event in that window fires at
  the **start of the block** (quantisation ≤ 11.6 ms at 44.1 kHz). For each instrument,
  events are scanned in stored order and applied in that order (note-on or note-off).
  Muted instruments skip event dispatch but keep rendering their tails.
- Seek sends all-notes-off and sets t. Stop sends all-notes-off.
- Song length = the latest last-event time across instruments. There are no explicit end
  markers. The demo player renders until that time (plus whatever tail the caller adds).
- Offline render helper: render `ceil`-ish (`trunc(sr·T/512) + 1`) blocks into a buffer.
  An optional "reverse" mode mirrors event times around the song length and repairs
  note-off ordering. Only the forward mode is needed for the intro.

---

## 7. Known bugs and quirks (decide per item: reproduce for parity, or fix)

1. **Song length with an empty instrument:** computing the song length reads the "last
   event" of every instrument. An instrument with zero events indexes before the start of
   its array: undefined behaviour or a crash. The reverse-events helper also underflows
   with fewer than 2 events. *Fix:* skip empty instruments.
2. **Only 8 of 10 mod-matrix slots are evaluated.** Slots 9–10 are saved, shown in the
   UI, and counted by the recorder's "is this source used" check, but they have no effect.
3. **Phase-stage sine-table mismatch** (§3.1.3): the multipliers cover only 0..π/4.
   *Reproduce* for sonic parity.
4. **Noise distribution:** the generator meant to sum three independent uniforms, but
   reuses one draw, so the noise is uniform rather than triangular-ish. Inaudible.
5. **Right-channel oscillator start phase can exceed 1.0** (base + up to 0.1) for the
   first sample, so the first table read can go past the table end (an out-of-bounds read
   in the original). *Fix:* wrap the initial phase.
6. **Envelope speed scales with sr²:** the per-block step multiplies by both N and
   sr/44 100, so at 48 kHz envelopes run ≈ 8.8 % faster than at 44.1 kHz, and at 96 kHz
   ≈ 2.2× faster. Tuned at 44.1 kHz. *Recommend* fixing the intro at 44.1 kHz.
7. **Block-size dependence:** plugin N = 256, player N = 512.
   - Glide coefficient is per block, so it runs twice as slow in time in the player.
   - Table refresh is every 4 blocks.
   - The flanger counter speed scales with N.
   - Envelopes, LFOs and chorus rates are N-compensated.
   - So songs can sound slightly different in the player than in the DAW. For parity
     with existing songs, use the player's 512.
8. **Decay can rise:** if the mod matrix pushes `d·M(decay)` above ≈ 1.02, the decay
   step becomes positive and volume never reaches sustain from above.
9. **Pitch bend is linear in Hz**, the LSB is ignored, and bend applies to all voices,
   including released ones.
10. **Band-limit heuristic ignores octave and fixed-frequency mode** (it uses the
    keyboard frequency), so GenOct up or a high GenFreq can alias.
11. **Glide is per voice and never reset**, so with polyphony > 1 the glide origin
    depends on which voice is allocated.
12. **Parameter quantisation in .tfm:** values are truncated to 1/100. Any patch value
    between steps is rounded down. Enumerated parameters (source, target, shape,
    effect type, octave) are chosen so that every option is reachable at 1/100
    resolution. Cutoff-like curves lose fine resolution at the low end.
13. **Delay effect:** an immediate (zero-latency) copy with gain k is added to the dry.
    Changing the delay time causes a jump, not a smooth change.
14. **Chorus sine offset bug** gives a clamped flat spot (§4.4). The chorus LFO phase is
    never wrapped, so float precision degrades after hours of running.
15. **Flanger sweep** is dominated by the counter-flip artefact (§4.5). Its
    sweep-position counter is a 32-bit integer that grows without bound.
16. **Effects auto-sleep** after 2 s of digital silence freezes state. Long reverb or
    delay tails that decay below 1e-5 mean level are cut.
17. **A 16-bit mixing helper** (not used by the player) clamps negative overflow to
    +32768, which wraps to −32768. Irrelevant unless reimplemented.
18. **Uninitialised voice fields** in the original (e.g. glide's current frequency, last
    stereo volume before the first note). *Fix:* zero-initialise everything.
19. **Event times are not sorted on load.** The recorder writes deltas as unsigned
    16-bit values, so a non-monotonic recording would wrap. Songs are assumed to be
    monotonic per instrument.
20. **Loudness mismatch between plugin and player** (§2.3).
21. **Notch filter** has one sample of extra latency (§3.3).
22. **HP/BP bandwidth** uses log₁₀ 2 instead of ln 2 (§3.3).

---

## 8. Open questions

Resolved by reading the source (2026-10-25):

1. **Formant coefficients:** these are exactly the public musicdsp.org "Formant filter" set (Alex @ smartelectronix): five
   vowels A/E/I/O/U, each an input gain plus 10 feedback taps, evaluated in f64, all-pole IIR. Take them from musicdsp.org,
   not from Tunefish. They are designed for 44.1 kHz. The vowel index is `trunc(4·p)`.
2. **Reverb tunings:** stock Freeverb comb and allpass lengths at 44.1 kHz. Comb lengths are **identical** on both channels.
   Only the right-channel allpasses get the +23-sample stereo spread (stock Freeverb spreads both). Lengths are not scaled with sr.
3. **Flanger:** the §4.5 derivation is confirmed. The bounce step is `p_FlangLfo · 0.1 · blockLen` per sample, so for
   FlangLfo > ~0.02 the counter alternates between 1 and 0.01 every sample. Each change resets the phase-step counter, so the
   effective rates are θ_L ≈ π·f̄/(2·sr) and θ_R ≈ π·(1 − f̄)/(2·sr) with f̄ ≈ 0.505, giving about 0.126 / 0.124 Hz.
   With FlangLfo = 0, f = 1 is constant: the fold never runs again, and both channels' running term advances at
   π/(240·sr), so L and R sweep together with a 480 s period (corrected 2026-10-25 while implementing add-tf-synth; the
   earlier "right static" reading applied the fold's 1 − f to the running term).
4. **GenScale:** partial h is centred at bin `1 + s(2h − 1)` of a 512-bin single-cycle spectrum, where `s = 4p`. Only
   p = 0.25 (s = 1) gives an exact harmonic series (bins 2, 4, 6, …, an octave above the table's bin-1 reference). Other
   values give inharmonic, stretched or compressed spectra by design. The profile is a two-sided exponential
   `exp(−|k − c|/w)`, cut at 5w (confirmed).

Still open (decide in the implementation change):

5. **Initial-phase randomness:** the original seeds from system entropy. We use a fixed seed for deterministic intro renders.
6. **Exact rounding** of the 0..1 → u8 export (truncation) versus plugin automation values. Verify by A/B rendering.
7. **Effect slots with reserved types 8–10** silently do nothing. Should we reject them?
8. **Max events:** u16 count and u16 row deltas cap a song at 65 535 events per instrument (enough).
