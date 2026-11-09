> **Functional specification of Tunefish 3 (Brain Control, Enigma Studio 3, `enigmastudio/Enigma-Studio-3` @ `85d65b6`,
> no licence stated, so treat it as all rights reserved), written from a reading of the source for behaviour only. Our
> code is MIT and our own: no copied code, comments, tables or identifiers. Public parameter names are kept because
> projects and presets store them.**

# Tunefish 3: Functional Specification

Status: the behavioural spec for importing Tunefish 3 songs into `unisynth` (change `add-tf3-import`). It mirrors the
structure of `research/tunefish4-functional-spec.md` ("TF4 spec"). Each section ends with a **Differs from TF4** note.

Notation as in the TF4 spec: `p` is a normalized value in `[0, 1]`; `M(t)` is the product mod-matrix factor of target
`t` (1 when nothing routes there); `N` is the block length; `sr` the sample rate. Two rounding helpers matter here,
because the released intros are 32-bit Windows builds:

- `fi(x)`: float → int as the x87 `fistp` does in the default control word, i.e. **round half to even**. The original
  calls this conversion everywhere an index, count or choice is derived (`trunc` is only right on other platforms).
- `rnd(x)`: x87 `frndint`, also round half to even.
- `log` is **log₁₀** (the original's "log" helper divides by log₂10), as in Tunefish 4. `pow` is computed in f64.

Only the `ePLAYER` build is what the intros played; §2.4 lists what it compiles out.

---

## 1. Overview and global constants

Each voice has a **point oscillator** (a waveform drawn through up to 4 control points, with unison and a
sub-oscillator), a **PAD table** (one static PADsynth table per instrument) and a stereo noise source. The oscillator and
the PAD table run, when the note is high, at up to 8× the sample rate. They always pass a fixed decimation low-pass.
Four optional filters follow (LP ladder, RBJ HP, BP, notch). Two ADSRs and two LFOs reach the parameters only through a
10-slot multiplicative mod matrix. Ten insert effects run on the instrument sum.

| Constant | Value | Notes |
|---|---|---|
| Sample rate | 44 100 Hz | Several laws are not rate-independent. |
| Block length | **384** frames | Song player and instrument. Control (matrix, envelopes, LFOs, filter coefficients, spline) runs once per block. |
| Voices per instrument | 16; limit `fi(15p + 1)` | |
| Instruments per song | 32 | |
| Unison layers | 1..11 (`fi(10p) + 1`) | Stored `p` is `k/9`, so 6 layers are unreachable (§7). |
| Sub-oscillator passes | 1 or 2 | |
| Point oscillator table | 257 samples (256 + guard) | Rebuilt from the spline, mean removed. |
| PAD table | 2¹⁷ floats allocated, 2¹⁶ used | §3.2 |
| Oversampling | 1..8, per voice and block | §3.3 |
| Noise table | 65 536 samples, looped | |
| LFO noise table | 256 steps | |
| Mod matrix | 10 slots, **all evaluated** | |
| Effect slots | 10 per instrument, serial | |
| Parameters | **129** | §5 |
| Tuning | 12-TET from 6.875·2^(3/12) Hz (MIDI 0 = 8.1758 Hz) | 128 entries, f64-computed then f32. |

**Differs from TF4:** two sound sources instead of one spectral generator; block 384 instead of 512; 129 instead of 112
parameters; all 10 matrix slots run; an always-on decimation low-pass on the oscillators; unison 1..11 (TF4 1..10).

---

## 2. Signal flow

### 2.1 Per voice (one block)

```
 control: matrix (sources only if a slot uses them) → velocity rule → noise filter coefficients
          pitch = Hz(note) + slop; oversampling factor k from pitch; glide (per block)

  noise (+own LP→HP) ───────────────────────────────────────────┐ (written at 1×)
                                                                  ▼
  point osc × unison × sub ─┐                                     voice buffer (stereo)
     (drive, clip, balance) ├─(k× rate)→ LP ladder 12.83 kHz ×2 ─→ average k → + ┘
  PAD table (drive, clip) ──┘                                                   │
                                                  [LP ladder] if on              ▼
                                                  [HP biquad] if on
                                                  [BP biquad] if on   (editor only)
                                                  [notch biquad] if on (editor only)
                                                  × gain curve → instrument bus
```

### 2.2 Per instrument

Voices that are held or sounding render in index order into the bus; then effect slots 1..10 run on it, **every block,
also when silent** (no auto-sleep).

### 2.3 Song player

```
 every block of 384 frames (t = frames/sr, f32):
   row = fi(t · bpm / 60 · 4) mod songLengthRows
   if row ≠ previous row: fire that row on every sequence track (§6.2)
   each instrument renders into a zeroed buffer; bus added to the master × 11
   master × 5000 · volume (volume = 1) → clamp ±32767/−32768 → fi → i16
```

So a float of 1.0 inside an instrument becomes 55 000 LSB before the clamp (TF4: 2 200). Instruments are mixed at a
high level and the songs are written so they do not clip much.

### 2.4 The player build (`ePLAYER`)

The intro binary leaves out: the band-pass and notch filters, the MOD1–4 matrix inputs (the note effect word), and the
chorus, flanger and EQ effects. A project that uses them sounds different in the editor than in the intro. In the player
build an instrument's parameters start at 0 (the editor starts from a default program) but every parameter is
stored, so this has no effect on songs.

**Differs from TF4:** TF4's player has all modules; TF4 masters at ×11 × 200, TF3 at ×11 × 5000.

---

## 3. Modules

### 3.1 Point oscillator

#### 3.1.1 Cycle

The cycle is a curve over `x ∈ [0, 1]` through points `P0 = (0, 0)`, user points `P1..Pn` and `P(n+1) = (1, 0)`, with
`n = fi(3 · p_OscPoints) + 1` (1..4).

- User point i (1-based) at `x_i = x_(i−1) + (1 − x_(i−1)) · clamp(p_OscPiOff · M(Pi offset), 0.01, 0.99)` (each offset
  is a fraction of what is left), value `y_i = 2 · p_OscPiVal · M(Pi value) − 1`.
- Segment ending at point i (i = 1..n) uses point i's interpolation mode `fi(2 · p_OscPiInt)`; the last segment (to
  `P(n+1)`) uses `fi(2 · p_OscFiInt)`. Modes: 0 **step** (hold the left point's value), 1 **linear**, 2 **spline**.
- **Spline**: one natural cubic spline through all points (second derivative 0 at both ends), solved with a tridiagonal
  sweep over the segment widths `dx_i = x_(i+1) − x_i + 1e-4` and rises `dy_i = y_(i+1) − y_i + 1e-4` (both nudged by
  1e-4, which tilts the curve very slightly). Segment k evaluates `y_k + c_k t + b_k t² + a_k t³` with `t = x − x_k`.
  The spline is solved over all points even if only some segments use mode 2. **Phantom point:** the solve runs
  over one segment more than the curve has, to a point after `(1, 0)` that is never set: `(0, 0)` (the vector
  type zero-initialises; with 4 points the read lands on the zeroed mode words just past the array). So the right
  end is not natural: the last real segment bends toward a point at x = 0 (`dx` ≈ −1).
- Point lookup: the segment is the first i with `x ≤ x_i` (so `x = x_i` belongs to the segment ending at i).

The table holds `T[j] = curve(j/256)` for j = 0..256, minus `(Σ_{j=0..256} T[j]) / 256` (257 terms over 256: the mean
removal is very slightly too large).

**Rebuild rule:** the cycle is rebuilt in a block when the point count, a point position or value changed (exact float
compare), or **whenever any segment uses spline mode**. A change of a non-spline mode alone does not rebuild (quirk 3).

**Sharing (quirk 1):** one cycle exists per **instrument**. Each voice updates it with its own matrix state before
rendering, so with several voices that modulate points differently the last update wins for the rest of the block, and
voices later in index order hear the cycle of the voice before them.

#### 3.1.2 Pitch, unison, sub-oscillator

Per voice and block (`base` = glided frequency in cycles per sample at the oversampled rate, §3.3):

- Octave multiplier `O[fi(8 · p_OscOctave)]`, `O = [16, 8, 4, 2, 1, ½, ¼, ⅛, 1/16]` (p = 0.5 → 1).
- Detune `D = (p_OscDetun · M(detune))³ / 5000 / k` cycles per sample (≈ 8.82 Hz · p³ at any k).
- Fixed frequency if `p_OscFreq ≥ 1e-5`: `F = (p_OscFreq · M(freq))² / 100 / k` cycles per sample (≈ 0..441 Hz) for both
  channels, ignoring note, octave and detune (`M(freq)` acts inside the square).
- Keyboard frequency otherwise: left `(base·O + D) · M(freq)`, right `(base·O − D) · M(freq)`.
- Spread `S = (p_OscSpread · M(spread))⁴ · 1e-4 / k` cycles per sample (≈ 0..4.41 Hz per layer).
- Unison `U = fi(10 · p_OscUnisn) + 1`; sub passes `B = fi(p_OscSubOsc) + 1`.
- For pass b = 0..B−1 and layer j = 0..U−1: left frequency `f_L + j·S`, right `f_R − j·S`, gain `1/(b+1)`; after each
  pass `f_L ← f_L · (0.5 − D)`, `f_R ← f_R · (0.5 + D)` (the second pass sits an octave down, detuned by D in ratio).
- Each (pass, layer) has its own left and right phase in `[0, 1)`; table read `T[fi(phase · 256)]`, no interpolation;
  the phase wraps when it exceeds 1 (so 1.0 itself is read once as T[256]).
- Start phases (note-on): layer 0 of pass 0 starts at 0 / 0. Every other (pass, layer) slot draws its left and right
  phase from a generator seeded with the system tick count, so in practice **all of them start at the same two phases**
  (quirk 4).

#### 3.1.3 Level, drive, balance

- `V = p_OscVol · velocity · M(volume)`, velocity = vel/128 (§3.7).
- Drive `G = 32 · (p_OscDrive · M(drive))³ + 1`.
- The summed passes and layers are scaled by G, **hard-clipped to ±1**, then multiplied by a linear balance:
  `π = p_OscPan · M(pan)`, left `V · (π > 0.5 ? 2(1 − π) : 1)`, right `V · (π < 0.5 ? 2π : 1)`.
- The left and right gains ramp linearly over the block (`k·N` samples) from the previous block's gains. They reset to
  0 at every note-on, so each note fades in over its first block.
- The oscillator runs in a block if the target or either previous gain exceeds 0.01; otherwise it is silent and
  reports "not sounding".

**Differs from TF4:** TF4 has no point oscillator. TF4's detune and spread are in Hz with 10p³ / 10p⁴ laws; TF3's are
p³/5000 and p⁴/10⁴ cycles per sample (8.82 Hz and 4.41 Hz full scale). TF4 drive is `1 + 32p`; TF3's oscillator drive is
`1 + 32p³` (its PAD drive is `1 + 32p`). TF4 fixed frequency is `1000p²` Hz; TF3 441p² Hz.

### 3.2 PAD table

#### 3.2.1 Table build

Built per **instrument** when the instrument loads and again whenever any of its inputs changes (checked every block):

- Inputs: bandwidth `β = p_AddBandw`, scale `s = p_AddScale²`, damp `δ = p_AddDamp`, harmonics `η = p_AddHarmncs / 4`,
  profile `fi(3 · p_AddProfile)`: 0 **single** (`1/b` for `|x| ≤ 0.1`), 1 **detune** (`1/b` for `0.4 ≤ |x| ≤ 0.5`),
  2 **spread** (`1/b` for `|x| ≤ 5`), 3 **gauss** (`e^(−x²)/b`, cut where `x² > 14.7128`). Here `x = (bin − centre)/b`.
- Reference frequency `f0 = Hz(MIDI 29) ≈ 43.654` Hz.
- Partial count `H = 16 + min(fi(255·η), fi(sr/f0))` (16..80); partial h = 1..H−1 is used (H−1 partials).
- Amplitude: `A_h = 1/(1 + h(1 − δ))` for odd h, `2/(1 + h(1 − δ))` for even h.
- Width: `bw_h = (2^(256β/1200) − 1) · f0 · h^(1 + 19s)` Hz, in table units `b_h = bw_h / (2·sr)`; centre
  `c_h = f0 · h / sr`. Over the first 2¹⁶ bins i (bin frequency `i / 2¹⁷` in the same units),
  `amp_i = Σ_h A_h · profile(i/2¹⁷ − c_h, b_h)`.
- Phases: per bin a value from a 65 536-entry table of uniform `[0, 2π)` values that is filled once per program run from
  the tick-count seed; a global counter walks it across table builds (so two builds differ in phase).
- Transform: the 2¹⁶ bins `amp_i · (cos φ_i, sin φ_i)` go through a **2¹⁶-point complex FFT with positive exponent**
  (no Hermitian symmetry); the table is the real part of the 2¹⁶ outputs. It is then scaled by
  `1/(1.4142 · max(|t|, 1e-5))` (peak over the 2¹⁶ samples). The remaining 2¹⁶ floats of the allocation are never
  written.

#### 3.2.2 Playback

- Runs if `V_pad = p_AddVolume · M(volume) ≥ 0.01`. **Velocity does not apply.**
- Frequency: octave `O[fi(8 · p_AddOctave)]`; left `(base·O + D)/260`, right `(base·O − D)/260` (D is the oscillator's
  detune with its own `M(detune)`, §3.1.2), or the fixed frequency `F/260` for both (`F` as §3.1.2, with `M(freq)`
  inside the square). The keyboard path has **no** `M(freq)` factor (the oscillator's has).
- Phase in `[0, 1)` per channel, random at note-on; read `table[fi(phase · 2¹⁶)]` (index 2¹⁶ reads one unwritten float,
  quirk 6); wrap with `while phase > 1: phase −= 1`.
- Output per channel: `clip((32 · p_AddDrive · M(drive) + 1) · t, −1, 1) · V_pad`. No pan, no ramp.
- Pitch: one table period (2¹⁶ samples) holds partial h at `2¹⁶ · c_h · 2 = 129.74 · h` cycles. Played at
  `f_note/260` periods per sample, the fundamental sounds at `129.74/260 · f_note ≈ 0.4990 · f_note`: **an octave
  below the note, 3.5 cents flat**, before the octave multiplier.

**Differs from TF4:** TF4 rebuilds a small (512-bin) table per voice every 4 blocks from a Laplacian spectrum with an
animated phase stage. TF3 builds one large static PADsynth table per instrument (four profiles, random phases) and
plays it without unison or pan. The TF4 shape parameters have different laws (§5).

### 3.3 Oversampling and the decimation filter

- Per voice and block, from the slopped note frequency `f` (Hz, before glide): `k` is the smallest integer ≥ 1 with
  `sr·k ≥ 64·f`, at most 8. So `k = 1` below 689 Hz; `k = 8` from 4 823 Hz up.
- The oscillator and PAD table render `k·N` samples into a zeroed buffer at rate `k·sr` (their per-sample frequencies are
  divided by k).
- That buffer passes **two** cascaded ladder low-passes (§3.5 LP law with p = 0.8, q = 0) designed at rate `k·sr`: cutoff
  `0.8²·20000 + 30 = 12 830` Hz each. **This runs for every voice, also at k = 1.**
- Decimation: each output sample is the mean of k consecutive filtered samples, **added** to the voice buffer that
  already holds the noise.
- The two filter states persist across blocks (reset at note-on); because k can change between blocks, the filter
  coefficients then change too.

**Differs from TF4:** TF4 has no oversampling and no fixed low-pass on its generator.

### 3.4 Noise

- Table: 65 536 values `(6 · c · u − 32 766)/32 767` with `c = 10 923` and u uniform in (0, 1) (one draw used three times:
  uniform in about ±1, as in TF4). Filled once from a Park–Miller generator seeded with 2 (deterministic).
- Left and right read the table from start offsets drawn per note-on in `[0, 32 768)` (tick-count seed).
- Level `n = p_NoiseOn · velocity · M(noise)`; silent when `n ≤ 0.01`. The noise is **added** into the zeroed voice
  buffer (at rate sr, not oversampled).
- Colour when `p_NoiseBw < 0.99`: the ladder LP of §3.5 with cutoff parameter `NoiseFreq + NoiseBw` and resonance 0.05,
  then the HP biquad with cutoff parameter `NoiseFreq − NoiseBw` and resonance 0.05 (clamped to [0, 1] each).

**Differs from TF4:** the same structure. TF4 overwrites the buffer with noise; TF3 adds (same result, it is first).

### 3.5 Filters (per voice, stereo, coefficients once per block)

Order LP → HP → BP → notch; each on when its switch is > 0.5. Cutoff `f = clamp(p · M(cutoff), 0, 1)`, resonance or Q
`q = p · M(res)`.

- **LP: ladder.** The TF4 ladder exactly (four one-poles with the two-tap form, `k = 3.6F − 1.6F² − 1`,
  `p = (k + 1)/2`, feedback `r = q·e^(1.386249(1 − p))`, output `y4 − y4³/6`), with `fc = 20 000 f² + 30` Hz,
  `F = 2fc/sr`, and **q clamped to [0, 0.95]**.
- **HP: RBJ high-pass**, `f0 = 10 000 f² + 30` Hz, `BW = 1 − clamp(q, 0, 0.95)`,
  `α = sin ω0 · sinh(log₁₀2/2 · BW · ω0 / sin ω0)` (the TF4 log₁₀ quirk).
- **BP: RBJ band-pass, constant skirt gain** (`b0 = sin ω0 / 2`), same `f0`, with
  `α = sin ω0 / 2 · √((A + 1/A)(1/BW − 1) + 2)`, `A = 1.059253725` (the cookbook's shelf-slope formula, used here with
  BW in place of the slope).
- **Notch: RBJ notch** (`b = 1, −2cos ω0, 1`), same `f0` and α as BP.
- All biquads: Direct Form I, normalised by `a0 = 1 + α`.
- State resets at note-on (all four filters and the two decimation filters).

**Differs from TF4:** TF4 clamps resonance at 0.85, uses the bandwidth α for BP, and has a resonator notch with
`20 000 f² + 30` Hz and one sample of latency. TF3's notch and BP are RBJ biquads at `10 000 f² + 30` Hz with the
shelf-slope α. TF4 keeps filter state across notes.

### 3.6 ADSR envelopes (2 per voice)

There is no amplitude envelope: an ADSR reaches the sound only through the matrix. An ADSR is **evaluated only if a
matrix slot uses it as a source** (otherwise it stays where it was).

`σ = 0.0005 · N` (N = 384). Values a, d, s, r, slope are the five `EnvX…` parameters; `d ← d · M(ADSRx decay)`.

- Attack step `α = −log(0.94 · max(a³, 1e-9)) · σ`; linear rise to 1, then Decay.
- Decay step `δ = ¼ · log(0.94 · d³) · σ` if `d > 0`, else `δ = −1` (instant).
- Release step `ρ = ¼ · log(0.94 · max(r³, 1e-9)) · σ`.
- Sustain `S = max(s, 0)` (**no upper clamp**).
- Decay: `v ← v + δ · ((0.01 + v − S)/(1 − S))^slope`; at `v ≤ S` set S, go to Sustain. With S = 1 the division is
  by 0: the step is infinite and the decay ends in its first block (or, with slope 0, the step is δ).
- Sustain: moves toward S by `|δ|` per block without overshoot.
- Release (note-off from any state): `v ← v + ρ · v^slope`; at `v ≤ 0.001` set 0 and Finished.
- **Note-on enters Attack from the current level** (no reset to 0); a never-used envelope starts at 0.

**Differs from TF4:** same laws except: TF3 has no 0.99 sustain clamp, ends the release at 0.001 (TF4 1e-5), retriggers
from the current level (TF4 from 0), and N = 384.

### 3.7 Velocity and voice life

- `velocity = vel/128` (a song note without velocity plays 128 → 1.0).
- If the voice is released **and** the matrix reports no running ADSR, velocity is 0 for the block. "Running" means:
  of the ADSRs the matrix evaluates, the one evaluated last in slot order (each is evaluated at its first slot) had
  not finished **before** this block's step. With no ADSR in the matrix, a released voice gets velocity 0 at once.
- Velocity 0 silences the noise and ramps the oscillator to 0 over one block. **The PAD table ignores velocity**: a
  released voice with a PAD table and no envelope on its volume keeps sounding until it is stolen (quirk 5).
- A voice renders while held or sounding. Sounding = noise, oscillator or PAD ran in the last block.
- Allocation: candidates 0..P−1, `P = fi(15 · p_OscPoly + 1)`; the first that is neither held nor sounding, else the
  one with the largest age (blocks rendered since note-on; ties to the lowest index). Age counts blocks while
  rendering.
- Note-on (on a stolen voice too): ADSRs to Attack, LFO phases set (§3.8), noise offsets, slop, oscillator and PAD
  phases redrawn, the oscillator's gain ramp set to 0, filters reset. ADSR levels and the glide frequency are kept.
- Note-off releases every held voice with that note number. Velocity 0 in a song is not used (§6.2).
- Per-voice output × gain curve `g ≤ 0.5: (2g)²; else 1 + 20(g − 0.5)` of `p_GainAmount` into the bus.

**Differs from TF4:** TF4 decides "sounding" from the output level (Σ|x| > 1) and its generator has velocity.

### 3.8 LFOs (2 per voice)

Evaluated once per block, only when a matrix slot uses them.

- Rate: phase advances `50 · p_rate² · N / sr` radians per block (`≈ 7.96 p²` Hz), wraps at 2π.
- Shape by threshold on p (stored values are 0, ¼, ½, ¾, 1): `< 0.2` **sine as `sin φ + 0.5`** (range −0.5..1.5,
  quirk 7), `< 0.4` rising ramp `φ/π` (0..2), `< 0.6` falling `1 − φ/π` (1..−1), `< 0.8` square (1 then 0), else stepped
  random (256 values, uniform 0..1, from a Park–Miller generator seeded with 2: the same sequence as the noise table).
- Output `d · shape + (1 − d)` with `d = p_depth²` (depth is not a matrix target).
- Sync `> 0.5`: phase 0 at note-on. Otherwise the new voice starts at the instrument's stored phase, which every voice
  overwrites with its own after its matrix step (the last voice processed wins).

**Differs from TF4:** TF4's sine is mapped to 0..1 and its ramps to 0..1; TF3's sine (a precedence slip) and ramps
reach outside 0..1, so a TF3 LFO can push a target above its value or flip its sign. TF4 has depth targets.

### 3.9 Mod matrix

Ten slots `(source, amount, target)`, all evaluated, in order, once per block per voice.

- Sources `fi(9p)`: 0 none, 1 LFO1, 2 LFO2, 3 ADSR1, 4 ADSR2, 5–8 MOD1–MOD4 (the note's effect word, §6.2; editor
  only), 9 wheel (**no effect**: never fed).
- Targets `fi(25p)`: 0 none, 1 P1 offset, 2 P1 value, 3 P2 offset, 4 P2 value, 5 P3 offset, 6 P3 value, 7 P4 offset,
  8 P4 value, 9 volume, 10 frequency, 11 pan, 12 detune, 13 spread, 14 drive, 15 noise amount, 16 LP cutoff,
  17 LP resonance, 18 HP cutoff, 19 HP resonance, 20 BP cutoff, 21 BP Q, 22 notch cutoff, 23 notch Q, 24 ADSR1 decay,
  25 ADSR2 decay.
- Amount `A = (20(p − 0.5))²` (0..100, **symmetric**: p = 0.45 and 0.55 both give 1, p = 0.5 gives 0).
- Slot result `R_i = A_i · src_i`; a slot with no source (or the wheel) has `R_i = 1`. `M(t) = ∏ R_i` over slots
  aimed at t.
- Each source is computed once per block, at the first slot that uses it. An ADSR's decay reads `M(ADSRx decay)` at that
  moment: slots before it with this block's results, slots after it with last block's.
- MOD1–4 hold 1 until a note sets one (per voice; §6.2).
- Volume (9) scales the oscillator and the PAD table; frequency (10), detune (12) and drive (14) also reach both.
  Pan and spread reach the oscillator only.

**Differs from TF4:** TF4 evaluates slots 1–8, has amount `2p` / `1 + 9(2p − 1)²`, mod-wheel and self-modulation sources
and targets and LFO-depth targets, and generator-shape targets. TF3 has point targets, MOD1–4 sources and a symmetric
squared amount.

### 3.10 Pitch: slop and glide

- `Hz(note & 127)` from the table, plus slop `ξ · p_OscSlop³ · 8` Hz with ξ uniform in [−1, 1] per note-on.
- Oversampling factor k from this frequency (§3.3), then to cycles per sample at rate `k·sr`.
- Glide when `p_OscGlide > 0` and the voice's current frequency is > 0: `cur ← cur + (target − cur)/(10g + 1)` per
  block. Because the target is in cycles per sample at the oversampled rate, **a change of k makes the glide jump**
  (quirk 8). The glide state is per voice and kept across notes.
- No pitch bend.

**Differs from TF4:** same slop and glide laws; no bend; N = 384.

---

## 4. Effects

### 4.0 Slot mechanics

- Slot type `fi(10p)`: 0 none, 1 distortion, 2 delay, 3 chorus (editor only), 4 flanger (editor only), 5 reverb,
  6 formant, 7 EQ (editor only), 8 gain and 9 filter (declared, **never created**: silent), 10 reserved.
- One parameter set per instrument shared by all slots; a type change makes a fresh instance.
- No auto-sleep: effects run every block.

### 4.1 Distortion

`y = sign(x) · T[fi(min(|x|, 1) · 32767)]`, `T[i] = (i/32768)^(1 − p_DistAmnt)` (32 768 entries). As TF4.

### 4.2 Delay

- Line of sr samples per channel; delay length `L = fi(clamp(p · sr, 0, sr))` per channel (`DelayL`, `DelayR`), feedback
  `k = p_DelayDc`.
- Per sample: `b[w] ← k·(b[w] + x)`; `y = x + b[w − 1]`; read and write advance together and wrap at L.
- So `y[n] = x[n] + k·x[n−1] + k²·x[n−1−L] + k³·x[n−1−2L] + …`: a one-sample copy with gain k, then echoes from
  `L + 1` samples on. L = 0 is degenerate (the indices reset every sample).

**Differs from TF4:** TF4 quantises to whole ms and adds the k-copy with no delay (echoes at L); TF3 has sample
resolution and a one-sample offset.

### 4.3 Reverb

Freeverb as in TF4 (8 combs into 4 all-passes per channel, stock 44.1 kHz lengths, input `(L + R)·0.015`, all-pass
feedback 0.5, `out = buf − in`, comb feedback `0.7 + 0.28p_RevRoom`, damping `0.4p_RevDamp`, wet `3p_RevWet`, dry
`2(1 − p_RevWet)`), except:

- **both** the right combs and the right all-passes are 23 samples longer;
- width does nothing: each channel outputs `(W1 + W2)·rev + dry·x = W·rev + dry·x` (no cross-feed).

### 4.4 Chorus (editor only)

One delay tap per channel, buffer sr/10. Per sample `d = 1 + (sin(t·π·p_ChrsRate/sr) + 1)/2 · (fi(50 · p_ChrsDep²))` ms,
`fi(d · sr/1000)` samples; the line stores `x · p_ChrsGain`, the output is `x + line[w − d]`. Both channels use the same
time counter, so they move together. **Differs from TF4:** TF4 has three cascaded taps per channel with block-rate LFOs.

### 4.5 Flanger (editor only)

The TF4 flanger (§4.5 there), same laws.

### 4.6 Formant

As TF4 (musicdsp vowel set A/E/I/O/U, f64 state, `y = (1 − w)x + w·filtered`), vowel `fi(4p)` (TF4 truncates).

### 4.7 EQ (editor only)

As TF4 (880 Hz / 5 kHz splitter, gains `2p` / `1 + 10(2p − 1)²`).

---

## 5. Parameter table

Index order is the instrument's internal order and the binary stream order. **Storage quantisation** (binary stream of the
player): a parameter with a value range R > 0 is stored as `rnd(p·R)` and read back as `byte/R`; R = 0 means `rnd(100p)/100`.
Projects (`.e3prj`) store floats by name; the intro played the quantised values.

| # | Name | R | Law |
|---|---|---|---|
| 0 | OscPoints | 3 | points `fi(3p) + 1` |
| 1, 4, 7, 10 | OscP1Off … OscP4Off | 0 | §3.1.1 offset (fraction of the rest, clamp 0.01..0.99) |
| 2, 5, 8, 11 | OscP1Val … OscP4Val | 0 | value `2p − 1` |
| 3, 6, 9, 12 | OscP1Int … OscP4Int | 2 | mode `fi(2p)`: step, linear, spline |
| 13 | OscFiInt | 2 | last segment's mode |
| 14 | OscVol | 0 | oscillator level `p · velocity` |
| 15 | OscFreq | 0 | fixed `441p²` Hz when ≥ 1e-5 |
| 16 | OscPan | 0 | linear balance |
| 17 | OscDetun | 0 | `8.82p³` Hz (L +, R −) |
| 18 | OscPoly | 15 | voices `fi(15p + 1)` |
| 19 | OscUnisn | 9 | layers `fi(10p) + 1` |
| 20 | OscSpread | 0 | `4.41p⁴` Hz per layer |
| 21 | OscSubOsc | 1 | passes `fi(p) + 1` |
| 22 | OscGlide | 0 | glide divisor `10p + 1` per block |
| 23 | OscDrive | 0 | `32p³ + 1`, hard clip |
| 24 | OscOctave | 8 | `16 / 2^fi(8p)` |
| 25 | OscOffset | 0 | unused |
| 26 | OscSlop | 0 | `±8p³` Hz per note |
| 27 | AddVolume | 0 | PAD level (no velocity) |
| 28 | AddProfile | 3 | single, detune, spread, gauss |
| 29 | AddOctave | 8 | as OscOctave |
| 30 | AddBandw | 0 | `256p` cents |
| 31 | AddDamp | 0 | §3.2.1 |
| 32 | AddHarmncs | 0 | `16 + min(fi(255p/4), fi(sr/f0))` |
| 33 | AddScale | 0 | width exponent `1 + 19p²` |
| 34 | AddDrive | 0 | `32p + 1`, hard clip |
| 35 | NoiseOn | 0 | level `p · velocity` |
| 36 | NoiseFreq | 0 | §3.4 |
| 37 | NoiseBw | 0 | off at ≥ 0.99 |
| 38, 41, 44, 47 | FltLPOn, FltHPOn, FltBPOn, FltNTOn | 1 | on > 0.5 |
| 39 | FltLPCut | 0 | `20000p² + 30` Hz |
| 40 | FltLPRes | 0 | ladder `q ≤ 0.95` |
| 42, 45, 48 | FltHPCut, FltBPCut, FltNTCut | 0 | `10000p² + 30` Hz |
| 43, 46, 49 | FltHPRes, FltBPQ, FltNTQ | 0 | `BW = 1 − min(p, 0.95)` |
| 50–54, 55–59 | Env1Att … Env1Slp, Env2Att … Env2Slp | 0 | §3.6 |
| 60, 64 | Lfo1Rate, Lfo2Rate | 0 | `7.96p²` Hz |
| 61, 65 | Lfo1Dep, Lfo2Dep | 0 | `d = p²` |
| 62, 66 | Lfo1Shp, Lfo2Shp | 4 | §3.8 |
| 63, 67 | Lfo1Sync, Lfo2Sync | 1 | restart > 0.5 |
| 68 + 3i | Mm(i+1)Source | 9 | §3.9 |
| 69 + 3i | Mm(i+1)Mod | 0 | `(20(p − 0.5))²` |
| 70 + 3i | Mm(i+1)Target | 25 | §3.9 |
| 98–107 | Effect1 … Effect10 | 10 | §4.0 |
| 108 | DistAmnt | 0 | exponent `1 − p` |
| 109, 110 | ChrsRate, ChrsDep | 0 | §4.4 |
| 111, 112, 113 | DelayL, DelayR, DelayDc | 0 | §4.2 |
| 114–117 | RevRoom, RevDamp, RevWet, RevWidth | 0 | §4.3 |
| 118–121 | FlangLfo, FlangFreq, FlangAmp, FlangWet | 0 | TF4 §4.5 |
| 122 | GainAmount | 0 | voice gain curve |
| 123 | ChrsGain | 0 | §4.4 |
| 124 | FrmtType | 4 | vowel `fi(4p)` |
| 125 | FrmtWet | 0 | linear mix |
| 126–128 | EqLow, EqMid, EqHigh | 0 | `2p` / `1 + 10(2p − 1)²` |

All 129 indices are covered: 0–37 one row per parameter or group, 38–49 the four filters, 50–59 envelopes, 60–67 LFOs,
68–97 the matrix (i = 0..9), 98–128 effects and gain.

---

## 6. Song format and player scheduling

### 6.1 Project (`.e3prj`, XML)

```
<enigma version="3.11b">
  … <songs> <song id bpm name> <patterns>
        <pattern patternid rowcount trackcount> <tracks> <track> <ne r n i v e/> … </track> … </tracks> </pattern> …
      </patterns>
      <patterninstances> <patterninstance rowoffset track patternid/> … </patterninstances>
    </song> … </songs>
  … <songlist> <song id name/> … </songlist>
  <instruments> <instrument id name> <parameters> <param name value/> … </parameters> </instrument> … </instruments>
</enigma>
```

- `ne`: row `r`, `n` = note byte (0 empty, 128 note-off, else MIDI note + 1), `i` instrument (−1 none), `v` velocity
  (−1 → 128), `e` 16-bit effect word (high byte = MOD slot 1–4, low byte/128 = value). Empty events are not written.
- Instruments: `id` 0..31; parameters by name; unknown names are ignored, missing ones keep the editor default.
- The `<songlist>` entries carry only ids and names; the playable song is the `<songs>` entry with patterns.
- Binary form in the intro: `tfSong` stream (`id` u32, `bpm` u16, name, `u16` pattern count, per pattern `u8 rows`,
  `u8 tracks`, `u16 id`, then rows × 5-byte events per track, then `u16` instance count and `u16 rowOffset, u8 track,
  u8 patternId` each) and the instrument stream (32 presence bytes, then parameter-major bytes per §5). No magic bytes.

### 6.2 Timing and events

- Rows: `row(t) = fi(t · bpm / 60 · 4)` with `t = frames/sr` in f32, evaluated at each block start. Because `fi`
  rounds, a row starts **half a row early** (quirk 9). The song loops at `length = max(rowOffset + rowCount)`.
- When the row changes, every sequence track 0..15 fires that row: for the first pattern instance on the track that
  covers the row (in file order), for each pattern column j:
  - a note (`n ≠ 0`, `i ≥ 0`): release the column's previous note (note-off of its note number on its instrument);
    then, if `n ≠ 128`, note-on `n − 1` with velocity `v < 0 ? 128 : v` and the effect word, and remember it;
  - `n = 128` with `i < 0`: release the column's previous note;
  - a note with `i < 0` and `n ∉ {0, 128}` does nothing.
- Only one row fires per block (a block is 8.7 ms; rows are always longer in practice).
- Release is by note number on the instrument: it releases every held voice of that note.

**Differs from TF4:** TF4 is an event list per instrument; TF3 is a tracker with columns that release their own previous
note. TF4 fires events whose time falls in the block; TF3 fires one row per block, half a row early.

---

## 7. Known quirks

1. The point cycle is shared by all voices of an instrument (§3.1.1).
2. Spline mode re-solves every block; the 1e-4 nudge tilts every segment slightly.
3. A mode change between step and linear does not rebuild the cycle by itself.
4. Unison and sub-oscillator start phases come from a tick-count seed and are equal for all but the first layer.
5. A PAD voice without an envelope on volume drones after note-off until stolen (no velocity, no release).
6. The PAD read can hit index 2¹⁶, one past the written table.
7. LFO sine is `sin φ + 0.5`; ramps reach 0..2 and 1..−1.
8. Glide works in oversampled units, so it jumps when k changes.
9. Rows start half a row early (rounding in `row(t)`).
10. Unison 6 is unreachable from stored values (`fi(10·k/9) + 1`).
11. The reverb's width parameter has no audible effect.
12. Effect types 8 and 9 (gain, filter) exist as names only.
13. `fi` rounds on the intro's platform and truncates elsewhere; a reference built on another OS must round explicitly,
    and must use log₁₀ for "log".

---

## 8. Open questions (from the change design)

Answered:

1. **Matrix rule** (design D6): `M(t) = ∏ A_i · src_i` with `A = (20(p − 0.5))²` (§3.9), not `1 + A·src`.
2. **Value ranges** (`TF_PARAM_VALUERANGE`): the R column of §5.
3. **Oversampling** (design D3): always on in the intro build; k from the note frequency; two 12.83 kHz ladders
   at k·sr even when k = 1. The decimation low-pass is therefore part of the sound and must be modelled in the point and
   PAD slots, not made optional.
4. **Editor-only modules**: BP, notch, MOD1–4, chorus, flanger, EQ (§2.4). Which of them Transplant uses: §9
   (an LFO → BP route on instrument 9, inert because its BP filter is off, and an EQ on instrument 10).
   **The released intro played the player build:** against the YouTube capture of Transplant (design D2a), the
   reference render in player mode matches every octave band within 0.20 dB, in editor mode only within 1.30 dB
   (2026-11-09, `tools/tf3-parity/capture.ts`).

Open:

5. **Per-voice point cycle** vs the shared one: decided per voice (design D3); measure on Transplant.
6. **Tick-count phases**: we use the instrument rng; the "all layers equal" start (quirk 4) is reproducible and should
   be kept if parity needs it.
7. **PAD phases**: random per build; parity is statistical.

---

## 9. Feature survey

Generated by `bun tools/tf3-parity/survey.ts --write` from the Enigma-Studio-3 checkout. Values are
quantised as the intro stored them (§5). Editor-only features are marked; the intro did not play them.

### transplant.e3prj

Format version 3.11b; 11 instruments; 1 song entries, 1 playable.

- Song 1 "New song": 140 bpm, 2096 rows (224.6 s), 58 patterns, 351 instances, 2348 notes, 547 note-offs, 0 notes with an effect word (editor only), 0 notes without instrument (ignored); instruments played: 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10.

| Instrument | Played | Features |
|---|---|---|
| 0 Basedrum 2 | yes | osc: 2 points; osc mode: spline; osc fixed frequency; osc detune; noise band; filter HP; slop; gain ≠ unity; ADSR1; ADSR2; route ADSR1 → frequency ×1.00; route ADSR2 → volume ×1.00; route ADSR2 → noise ×1.00; fx distortion; fx reverb |
| 1 Snare 4 | yes | osc: 2 points; osc mode: spline; osc fixed frequency; osc detune; noise band; filter HP; slop; gain ≠ unity; ADSR1; ADSR2; route ADSR1 → noise ×1.00; route ADSR2 → frequency ×1.00; fx distortion; fx reverb |
| 2 MAD_Snare2_2 | yes | noise band; slop; gain ≠ unity; ADSR1; ADSR2; route ADSR1 → noise ×1.44; route ADSR2 → frequency ×1.44; fx distortion; fx reverb; fx delay |
| 3 MAD_Hihat_Open | yes | noise band; filter LP; slop; gain ≠ unity; ADSR1; route ADSR1 → noise ×1.44 |
| 4 MAD_Hihat_Open_2 | yes | noise band; slop; gain ≠ unity; ADSR1; route ADSR1 → noise ×1.44 |
| 5 MAD_Noise | yes | noise band; filter LP; slop; gain ≠ unity; LFO1: sine; route LFO1 → noise ×1.44; fx delay |
| 6 MAD_Pad_Bright | yes | pad: gauss; filter LP; slop; gain ≠ unity; ADSR1; ADSR2; route ADSR1 → volume ×1.44; route ADSR2 → LP cutoff ×1.44; fx delay; fx reverb |
| 7 Short Lead | yes | osc: 4 points; osc mode: step; osc mode: spline; osc unison 11; osc drive; osc detune; osc spread; osc octave -1; pad: single; filter LP; slop; gain ≠ unity; ADSR1; ADSR2; route ADSR1 → volume ×1.44; route ADSR2 → LP cutoff ×1.44; fx delay; fx reverb |
| 8 MAD_Bass_LFO | yes | osc: 4 points; osc mode: step; osc mode: linear; osc unison 3; osc detune; osc spread; osc octave -2; pad: gauss; pad drive; filter LP; slop; gain ≠ unity; ADSR1; ADSR2; LFO1: ramp down; LFO1 sync; LFO2: sine; route ADSR1 → volume ×1.44; route ADSR2 → LP cutoff ×1.44; route LFO1 → volume ×1.44; route LFO2 → LP cutoff ×1.44; fx reverb; fx delay |
| 9 MAD_Fatpad | yes | osc: 2 points; osc mode: step; osc mode: spline; osc unison 3; osc detune; osc spread; osc octave -1; filter LP; slop; gain ≠ unity; ADSR1; ADSR2; LFO1: sine; LFO1 sync; route ADSR1 → volume ×1.44; route LFO1 → BP cutoff ×1.44; BP/notch target (editor only); route ADSR2 → LP cutoff ×1.44; fx delay; fx formant; fx reverb |
| 10 MAD_Bass | yes | osc: 2 points; osc mode: step; osc mode: linear; osc unison 5; osc drive; osc detune; osc spread; osc octave -2; filter LP; slop; gain ≠ unity; ADSR1; ADSR2; LFO1: sine; LFO2: ramp down; route ADSR1 → volume ×1.44; route ADSR2 → LP cutoff ×1.44; route LFO1 → volume ×1.44; route LFO2 → LP cutoff ×1.44; fx delay; fx EQ (editor only); fx distortion; fx reverb |

Editor-only features in played instruments: BP/notch target (editor only); fx EQ (editor only).

### default.e3prj

Format version 3.11b; 19 instruments; 1 song entries, 1 playable.

- Song 2 "New song": 125 bpm, 64 rows (7.7 s), 1 patterns, 1 instances, 0 notes, 0 note-offs, 0 notes with an effect word (editor only), 0 notes without instrument (ignored); instruments played: none.

| Instrument | Played | Features |
|---|---|---|
| 0 Fieser Bass | no | osc: 4 points; osc mode: step; osc mode: spline; osc unison 2; osc detune; osc spread; filter LP; slop; gain ≠ unity; ADSR1; ADSR2; route ADSR1 → volume ×1.00; route ADSR2 → LP cutoff ×1.00; fx distortion; fx delay; fx reverb; fx chorus (editor only); fx EQ (editor only) |
| 1 Bass 4 | no | osc: 2 points; osc mode: step; osc unison 3; osc spread; osc octave -1; filter LP; slop; gain ≠ unity; ADSR1; ADSR2; LFO1: sine; route ADSR1 → volume ×1.00; route LFO1 → LP cutoff ×1.00; route ADSR2 → LP cutoff ×1.00; fx distortion |
| 2 Pad, BP | no | osc: 2 points; osc mode: step; osc mode: spline; osc unison 3; osc detune; osc spread; osc octave 1; pad: gauss; filter LP; glide; slop; gain ≠ unity; ADSR1; ADSR2; route ADSR1 → volume ×1.00; route ADSR2 → BP cutoff ×0.36; BP/notch target (editor only); route ADSR2 → LP cutoff ×1.00; route MOD1 → ADSR2 decay ×1.00; MOD source (editor only); fx chorus (editor only); fx EQ (editor only); fx delay; fx reverb |
| 3 Basedrum | no | osc: 2 points; osc mode: spline; osc fixed frequency; osc detune; noise band; filter LP; filter HP; slop; gain ≠ unity; ADSR1; ADSR2; route ADSR1 → frequency ×1.00; route ADSR2 → volume ×1.00; route ADSR2 → noise ×1.00; fx distortion; fx reverb |
| 4 Hihat Open | no | noise band; filter LP; slop; gain ≠ unity; ADSR1; route ADSR1 → noise ×1.44; fx reverb |
| 5 Bass | no | osc: 2 points; osc mode: linear; osc unison 3; osc detune; osc spread; osc octave -1; filter LP; slop; gain ≠ unity; ADSR1; ADSR2; route ADSR1 → volume ×1.00; route ADSR2 → LP cutoff ×1.00; fx chorus (editor only); fx delay; fx EQ (editor only); fx reverb |
| 6 Bing | no | pad: gauss; filter LP; slop; gain ≠ unity; ADSR1; ADSR2; LFO1: sine; route ADSR1 → volume ×1.00; route ADSR2 → BP cutoff ×1.00; BP/notch target (editor only); route LFO1 → BP cutoff ×1.00; BP/notch target (editor only); route ADSR2 → LP cutoff ×1.00; fx delay; fx reverb |
| 7 Lead | no | osc: 2 points; osc mode: linear; osc mode: step; osc unison 2; osc detune; osc spread; osc octave -1; filter LP; slop; gain ≠ unity; ADSR1; ADSR2; route ADSR1 → volume ×1.00; route ADSR2 → LP cutoff ×1.00; fx delay; fx reverb |
| 8 Snare 2 | no | osc: 2 points; osc mode: spline; osc fixed frequency; osc detune; noise band; filter HP; slop; gain ≠ unity; ADSR1; ADSR2; route ADSR1 → noise ×1.00; route ADSR2 → frequency ×1.00; fx distortion; fx reverb; fx EQ (editor only) |
| 9 Strings | no | osc: 4 points; osc mode: step; osc mode: spline; osc unison 4; osc detune; osc spread; filter LP; slop; gain ≠ unity; ADSR1; LFO1: sine; LFO1 sync; route ADSR1 → volume ×1.00; route LFO1 → LP cutoff ×1.00; fx delay; fx reverb |
| 10 String, Short | no | osc: 4 points; osc mode: step; osc mode: linear; osc mode: spline; osc unison 3; osc detune; osc spread; filter LP; gain ≠ unity; ADSR1; ADSR2; route ADSR1 → volume ×1.00; route ADSR2 → LP cutoff ×1.00; fx delay; fx reverb |
| 11 Asdf | no | pad: gauss; pad drive; filter LP; slop; gain ≠ unity; ADSR1; ADSR2; LFO1: sine; LFO2: sine; route ADSR2 → volume ×1.00; route ADSR1 → LP cutoff ×1.00; route LFO1 → LP cutoff ×1.00; route LFO2 → BP cutoff ×1.00; BP/notch target (editor only); fx delay; fx reverb |
| 12 Fieser Bass | no | osc: 4 points; osc mode: step; osc mode: spline; osc unison 2; osc detune; osc spread; filter LP; slop; gain ≠ unity; ADSR1; ADSR2; route ADSR1 → volume ×1.00; route ADSR2 → LP cutoff ×1.00; fx distortion; fx delay; fx reverb; fx chorus (editor only); fx EQ (editor only) |
| 13 Bass | no | osc: 2 points; osc mode: step; osc unison 2; osc detune; osc spread; osc octave -2; filter LP; slop; gain ≠ unity; ADSR1; ADSR2; route ADSR1 → volume ×1.00; route MOD1 → ADSR2 decay ×1.00; MOD source (editor only); route ADSR2 → LP cutoff ×1.00; fx delay; fx EQ (editor only); fx distortion |
| 14 Fieser Bass | no | osc: 2 points; osc mode: step; osc mode: spline; osc unison 2; osc detune; osc spread; osc octave -1; slop; gain ≠ unity; ADSR1; route ADSR1 → volume ×1.00 |
| 15 Snare 2 | no | osc: 2 points; osc mode: spline; osc fixed frequency; osc detune; noise band; filter HP; slop; gain ≠ unity; ADSR1; ADSR2; route ADSR1 → noise ×1.00; route ADSR2 → frequency ×1.00; fx distortion; fx reverb |
| 16 Pad | no | pad: gauss; pad drive; filter LP; slop; gain ≠ unity; ADSR1; ADSR2; LFO1: sine; LFO2: sine; route ADSR2 → volume ×1.00; route ADSR1 → LP cutoff ×1.00; route LFO1 → LP cutoff ×1.00; route LFO2 → BP cutoff ×1.00; BP/notch target (editor only); fx chorus (editor only); fx reverb; fx delay |
| 17 Bass, Short | no | osc: 2 points; osc mode: linear; osc mode: step; osc mode: spline; osc unison 5; osc detune; osc spread; osc octave -2; pad: gauss; slop; gain ≠ unity; ADSR1; route ADSR1 → volume ×1.00 |
| 18 Bing | no | pad: gauss; filter LP; slop; gain ≠ unity; ADSR1; ADSR2; LFO1: sine; route ADSR1 → volume ×1.00; route ADSR2 → BP cutoff ×1.00; BP/notch target (editor only); route LFO1 → BP cutoff ×1.00; BP/notch target (editor only); route ADSR2 → LP cutoff ×1.00; route MOD1 → HP cutoff ×1.00; MOD source (editor only); fx delay; fx reverb |

Editor-only features in played instruments: none.

### Presets (`binary/tf3programs`)

128 files, 53 distinct; the largest group of identical files has 75 (init patches). Feature counts over all files:

| Feature | Presets |
|---|---:|
| slop | 126 |
| osc detune | 115 |
| osc mode: spline | 101 |
| osc: 2 points | 100 |
| ADSR1 | 52 |
| gain ≠ unity | 42 |
| route ADSR1 → volume ×1.00 | 35 |
| ADSR2 | 31 |
| fx reverb | 31 |
| osc spread | 30 |
| osc mode: step | 25 |
| fx delay | 24 |
| filter LP | 23 |
| fx distortion | 19 |
| noise band | 17 |
| osc: 4 points | 16 |
| BP/notch target (editor only) | 14 |
| osc unison 3 | 14 |
| LFO1: sine | 13 |
| osc unison 2 | 13 |
| route ADSR2 → LP cutoff ×1.00 | 13 |
| fx flanger (editor only) | 12 |
| pad: gauss | 12 |
| filter HP | 11 |
| osc fixed frequency | 10 |
| osc mode: linear | 10 |
| osc octave -1 | 10 |
| route ADSR1 → noise ×1.00 | 10 |
| filter BP (editor only) | 8 |
| LFO2: sine | 8 |
| route ADSR2 → frequency ×1.00 | 7 |
| route LFO1 → LP cutoff ×1.00 | 7 |
| pad drive | 6 |
| route ADSR2 → BP cutoff ×1.00 | 6 |
| route ADSR2 → volume ×1.00 | 6 |
| route LFO2 → BP cutoff ×1.00 | 6 |
| fx EQ (editor only) | 5 |
| osc point modulation | 5 |
| route ADSR1 → LP cutoff ×1.00 | 5 |
| route LFO1 → BP cutoff ×1.00 | 5 |
| fx chorus (editor only) | 4 |
| route ADSR1 → BP cutoff ×1.00 | 4 |
| route ADSR1 → frequency ×1.00 | 4 |
| LFO1 sync | 3 |
| osc octave -2 | 3 |
| osc unison 4 | 3 |
| route ADSR1 → noise ×1.44 | 3 |
| route ADSR1 → P1 offset ×1.00 | 3 |
| route ADSR2 → noise ×1.00 | 3 |
| LFO1: ramp down | 2 |
| pad: single | 2 |
| route ADSR2 → P1 offset ×1.00 | 2 |
| route LFO1 → HP cutoff ×1.00 | 2 |
| glide | 1 |
| MOD source (editor only) | 1 |
| osc octave 1 | 1 |
| route ADSR2 → BP cutoff ×0.36 | 1 |
| route ADSR2 → spread ×1.00 | 1 |
| route LFO1 → P1 offset ×1.00 | 1 |
| route LFO1 → volume ×1.00 | 1 |
| route LFO2 → ADSR1 decay ×1.00 | 1 |
| route LFO2 → LP cutoff ×1.00 | 1 |
| route MOD3 → volume ×1.00 | 1 |
