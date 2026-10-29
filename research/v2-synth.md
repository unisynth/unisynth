# Farbrausch V2: songs, versions and the map to `unisynth` (2026-10-28)

Goal: bring V2 songs (`.v2m`) into this project. Steps: extract them, convert them to
unified songs (kinds 64–73), and add the V2 functions `unisynth` lacks. Songs are
checked against a reference render, the same way WaveSabre and Tunefish were.

## 1. What exists

Farbrausch released **one** synth: **V2**, by Tammo "kb" Hinrichs (2000–2008).
"v1/v2/v3" are the **format versions 0–6** of `.v2m` files and patches. Each version
adds parameters. Later engines also changed behaviour.

| Source | Where (local) | License | Use |
|---|---|---|---|
| `farbrausch/fr_public` `v2/` (`synth.asm`, ryg's C++ port `synth_core.cpp`, `ronan.cpp`, `sounddef.h`, `v2mconv.cpp`, `v2mplayer.cpp`) | `~/Downloads/64k-tools/synth-lab/fr_public` | **public domain** (kb, `v2/LICENSE.txt`) | Original algorithms. Code may be ported directly. |
| `spheenik/v2redux`: C++17 player, plays every version **at its own era's behaviour**, deterministic across hosts, Ronan speech included | `~/Downloads/64k-tools/synth-lab/v2redux` (built: `build/v2dump song.v2m out.wav [secs]`) | **CC0** port. Its bundled songs are © their authors (test vectors only). | **Reference renderer** (oracle-proven against period binaries: v0/v1/v6 bit-exact, v3–v5 at a measured ε floor). Also carving tools (`toolkit/era unpack/carve`) that pull songs out of packed demo `.exe` files. |
| `jgilje/v2m-player` `v2m/` | `~/Downloads/64k-tools/synth-lab/v2m-player` | player GPL-ish, songs © authors | 325 songs (`.v2mz` = gzip), mostly from modland |
| modland `pub/modules/V2/` | online | songs © authors | larger public collection (about 60 artist folders) |

A native macOS build of the original `synth_core.cpp` needs many Win32 shims
(`__int64`, `vsprintf_s`, `ZeroMemory`, MSVC inline asm in `v2mplayer.cpp` and
`ronan.cpp`). v2redux already solves this, so the reference is **v2redux**, not
a port of our own (attempt in `synth-lab/v2mac`, abandoned).

### Format versions (from `sounddef.h` `version` fields)

| ver | patch params | globals | added | Reference binary (v2redux) |
|---|---|---|---|---|
| 0 | 68 | 12 | base | fr-08 *.the .product*, bit-exact |
| 1 | 78 | 21 | channel compressor, boost, sum compressor | *flybye*, bit-exact |
| 2 | 82 | 21 | voice transpose, ring mod ×3 | no file known |
| 3 | 83 | 21 | filter balance | fr-014, ε floor |
| 4 | 83 | 22 | reverb low cut | fr-019, ε floor |
| 5 | 85 | 22 | LFO polarity ×2 | candytron, fr-024, fr-029, kkrieger, ε floor |
| 6 | 89 | 23 | aux A/B send and receive | `synth.asm` 2004, bit-exact |

The version is found from the patch size and the global size. Old files are
upgraded by inserting defaults (`v2initsnd`), as `conv2m` does. Engine behaviour
also changed (oscillators, FM scheme, noise, voice count, DC offset). v2redux
handles this per detected version (`v2eras.h`). The converter must record the
source version so a unified song can keep the era behaviour where it is audible.

### `.v2m` layout (after upgrade to v6)

`timediv u32, maxtime u32, gdnum u32`, then the global event planes
(`10·gdnum`: time ×3 planes, usecs/beat u32, num, den, tpq). Then 16 channels, each
`notenum u32` [+ notes 5 B, program changes 4 B, pitch bends 5 B, 7 controllers × 4 B,
all byte-plane delta coded]. Then `globals size + 23 bytes`, then
`patchmap size + (u32 offsets + patches: 89 params, modnum, modnum × (src, amount, dest))`,
then the speech size and the speech text (Ronan phonemes). Events are MIDI-like:
note, velocity, program, CC 1–7, bend.

## 2. Corpus survey

Tool: `~/Downloads/64k-tools/synth-lab/v2survey` (`dump` = v2redux loader → JSON
per song, `agg.py` counts features). It covers 360 files: v2redux corpus (15),
fr_public (16) and v2m-player (325), with 10 duplicates. 359 parse. One file,
the flybye loader, has no detectable version.

Versions: v5 296, v6 40, v3 8, v1 8, v0 4, v4 3. Speech text is present in 264
songs; some of these may be empty boilerplate. Pitch bend appears in 119 songs,
CC lanes in 279.

The table counts **songs** that use a feature in a used patch (n = 359).
Features below 5 songs are omitted.

| Area | Feature (songs) |
|---|---|
| Oscillators | saw/tri 348, sin 347, noise 338, pulse 333, **ring mod 184**, **FM 176**, **aux A/B as oscillator 11/6** |
| Filters | LP 358, HP 340, BP 332, notch 109, **all-pass 155**, **Moog LP 7**, **Moog HP 5**; routing: serial 340, **parallel 303** (with balance), single 121 |
| Voice distortion | **OD 273**, HPF 167, LPF 141, crush 126, clip 106, NoF 69, dec 65, BPF 59, APF 9 |
| Envelopes | **sustain-time slope**: amp EG 294, EG2 238 |
| LFOs | tri 355, sin 314, saw 169, **S+H 154**, pulse 75; **env mode (one-shot) 322**; **polarity − 115 / ± 231** |
| Mod matrix | every song; sources vel, LFO1/2, EG2, amp EG, mod wheel, CC2–6, volume, note. Additive, **any parameter is a target**, up to 255 entries |
| Voice | poly > 1 in 357, mono in 313, transpose 309, keysync osc 335 / full 3 |
| Channel strip | reverb send 354, delay send 341, chorus/flanger 334, **boost 329**, **channel compressor peak 314 / RMS 29**, channel distortion (OD 212, HPF 186, LPF 132, crush 121, dec 106, clip 90, BPF 83), FX order chorus→dist 260, **aux bus send/receive 13/10** |
| Globals | **V2 reverb + modulated stereo delay** every song, post low/high cut 316, **sum compressor 228** |
| Speech | **Ronan** formant speech channel (fr-08, kkrieger and others) |

Conclusion: nearly every V2 module is used widely. A "map onto existing
modules" converter would get few songs right. V2 needs its own modes inside
`unisynth`, the same way WaveSabre and Tunefish each kept their algorithms.

## 3. Gap analysis vs `unisynth` (kinds 64–73, chunk v1, 184 params)

✅ = an existing mode can carry it with a parameter map. ➕ = a new mode or module is needed.

| V2 | unisynth today | Verdict |
|---|---|---|
| 3 oscillators: saw↔tri morph (color), pulse width (color), sin, V2 noise (color = filter), naive + V2 anti-alias box filter | 3 slots: PM sine, BLIT saw/pulse, wavetable, sampler; separate noise source | ➕ `us-osc-v2` oscillator type (V2 waveform core incl. noise). BLIT is not V2's sound. |
| FM (osc 2/3 modulated by the previous oscillator's output) | PM send between slots | ➕ inside the V2 oscillator type (V2 FM law, era variants) |
| Ring mod (multiply into the running sum) | — | ➕ slot mix mode "ring" |
| Aux A/B as oscillator input (reads a global aux bus) | instruments do not read buses | ➕ architectural: aux buses + instrument input. Used by 11 songs; phase 2. |
| 2 filters, single/serial/**parallel + balance** | 4 slots in series | ➕ routing mode for slots 1–2 (parallel + balance) |
| SVF LP/BP/HP/notch/**all-pass** (V2 coefficient law) | SVF LP/HP/BP/notch (WaveSabre law), ladder, RBJ | ➕ `us-flt-v2` (V2 SVF law + all-pass), ➕ V2 Moog LP/HP |
| Voice distortion: OD, clip, crush, decimate, LPF/BPF/HPF/NoF/APF/MoogL | none in the voice | ➕ voice-path distortion stage `us-vdist-v2` (shares code with the channel distortion) |
| ADSR + **sustain time** (sustain slope), V2 curves, amplify | timed (WaveSabre) / rate (Tunefish) | ➕ envelope mode 3 `v2` |
| LFO saw/tri/pulse/sin/**S+H**, keysync, **env mode**, **polarity**, phase, amplify | sine/ramp/square/random, sync/free | ➕ V2 LFO shapes + one-shot + polarity (or an LFO mode `v2`) |
| Mod matrix: 255 additive entries, any param, amount −64..63, in param space 0..127 | 12 slots, scale/offset, fixed target list | ➕ the hardest gap. Option A: V2 matrix mode (additive, raw-param space, own target table of the V2 params). Option B: fold into the 12 slots + targets (lossy semantics). Measured: 4 363 used patches, max 14 entries, p95 8, only 3 patches > 12, so the slot **count** fits; the **semantics** (additive, raw 0..127 space, any param) do not. |
| Polyphony, mono (no glide), keysync none/osc/full | voice pool, mono, glide, start phase | ✅ mostly. Keysync "full" = restart envelopes and LFOs too (check). |
| Channel: volume, boost (bass shelf), channel distortion, chorus/flanger (V2), channel compressor (peak/RMS, couple, auto-gain, lookahead), FX order | effects kinds 65–73 (WaveSabre/Tunefish modes) | ➕ modes: `distortion v2`, `flanger v2` (chorus/flanger), `compressor v2`, `eq v2-boost`. Order = device order. ✅ |
| Reverb/delay sends, aux sends | synth-mix tracks + receives | ✅ send tracks; aux = track receives |
| Global reverb (V2), modulated stereo delay, post low/high cut, sum compressor | freeverb/compact/rooms, tempo/ms delay, EQ, compressor | ➕ `reverb v2`, `delay v2` (modulated), low/high cut via `eq` or a new mode, ✅ compressor v2 on master |
| Ronan speech | formant effect (Tunefish) | ➕ `us-speech-ronan`: new instrument source (phoneme table + text in the chunk). Phase 3. |
| Song: 16 MIDI channels, program changes mid-song, CC1–7, bend, tempo map | synth-mix tracks, note lanes, automation, bend/wheel | ✅ one track per (channel, program) or patch switch; CCs → automation lanes (sources become matrix inputs); tempo map → the song tempo (check whether multiple tempi occur) |
| Era behaviour (v0–v6) | — | ➕ an `era` param on the V2 modes (only where audible) |

Cost note: every new module is a `us-*` cargo feature. Songs that do not use V2
pay nothing (the feature gating from `shrink-unified-synth`).

## 4. Proposed plan (OpenSpec change `add-v2-import`, phased)

1. **Corpus + reference** (local, git-ignored): `tools/v2-extract` fetches
   fr_public, v2redux and modland songs, carves demo `.exe` files with v2redux
   `toolkit/era`, and renders references with `v2dump`. Records version,
   features and provenance in `research/v2-intros.md` (as `wavesabre-intros.md`).
2. **Parser**: `synth-convert v2m`. Native `.v2m` reader, version detection,
   upgrade to the v6 layout. Golden tests compare the canonical bytes with v2redux.
3. **Voice modes**: V2 oscillator, filters + parallel routing, voice
   distortion, V2 envelope, V2 LFO, V2 mod matrix. Port from `synth_core.cpp`
   (public domain), v2redux as the bit reference. Each gets a parity test per
   module against v2redux.
4. **Channel/global effects**: modes distortion v2, chorus/flanger v2,
   compressor v2, boost, reverb v2, delay v2, low/high cut.
5. **Song mapping + parity gate**: `tools/unisynth-parity` gains V2 songs
   (band/loudness metric, the same D11 thresholds), feature map, renders in
   `renders/unisynth/`.
6. Later: aux-bus oscillators, Ronan speech, era-specific behaviours beyond v5/v6.

**Decided 2026-10-28** (user): parity metrics (not bit-exact); extend the one mod matrix (Add operation, controller
sources, appended targets, 16 slots); append parameters after 183, with UTF-8-like escaped indices (<240 one byte, 0xF0+bank two bytes) and counts, so old chunks are unchanged; the 15 acceptance
songs; Ronan in scope. Planned as OpenSpec change `openspec/changes/archive/2026-10-29-add-v2-import/`.

Open decisions at the time of the survey (answered above):

- **Fidelity target**: (a) the parity metric already in use (loudness/bands
  within 1 dB/3 dB), or (b) near-bit-exact per module against v2redux.
  (b) is possible because the code is public domain, but it costs speed and size.
- **Mod matrix**: a V2-native additive matrix mode (12 slots suffice for all
  but 3 patches), or fold V2 entries into the existing scale/offset matrix (lossy)?
- **Chunk**: append V2 parameters after index 183 (as noise width did), or
  start a separate V2 parameter block (chunk v2)?
- **Scope of phase 1**: which songs are the acceptance set? Suggestion: the 13
  v2redux demo songs (fr-08 … kkrieger) and pzero/zeitmaschine.
- **Song licensing**: songs stay local and git-ignored, like the WaveSabre intro
  songs. Only the conversion code and the reports are committed.

## 5. Outcome (2026-10-28, change `add-v2-import`)

- **Parity.** All 15 acceptance songs pass against v2redux (`tools/unisynth-parity/v2-report.md`). Loudness is within
  0.9 dB, the worst octave band within 1.8 dB (fr027 at 16 kHz), and onset alignment correlation is 0.82–0.99. That
  covers format versions 0, 1, 3, 4, 5 and 6. No version 2 song is known.
- **What parity needed**, in the order it was found: events on V2's 128-frame grid (256 below version 2); the master
  chain (aux buses, reverb, delay, EQ cut, sum compressor); the chorus taps and round-to-nearest bitcrusher; the
  year-2000 envelope laws and the 4× box-filtered oscillators below version 3; the MSVC LCG noise below version 5;
  Ronan; and `fistp` rounding with the integer-indefinite collapse in the pre-v6 FM. kkrieger matches the default
  (Auto) engine profile, not `kkrieger2004`.
- **Aux buses** (task 5.2). V2 clears aux A/B every frame and renders channels in order, voices before the channel
  stage. So an aux oscillator or aux receive on channel c hears exactly the aux sends of the lower channels. The
  converter wires each reader straight to those senders' output pairs, with no bus track and no cycle. A send or
  receive level counts as used when it is above 0 or an Add route moves it (often CC7). Of the 12 version 6 corpus
  songs with aux readers or aux oscillators, 11 pass (`v2-report.md` lists debris_ost, fr-052_platinum and stars).
  Testing them found a broken V2 Moog filter (feedback sign, double un-bias), now fixed. No acceptance song uses the
  Moog filters.
- **Known gap.** `Quickyman--tristar_bp2008-unsuspecting` fails: −1.7 dB, 63 Hz −5.5 dB. Its aux use is
  self-send/receive only, so the cause is elsewhere (channels 5 and 6, FM and noise patches). A follow-up.
- **Not done.** Bulk Modland fetch. The A/B listening is approved (`v2-signoff.json`, 2026-10-28).
- **Size.** V2 songs build to 27.6–35.3 KB squishy (`tools/unisynth-parity/size.md`). Next to the build before the V2
  work, songs without V2 first cost up to +1.8 KB xz. After gating (design D11) they cost +180 to +470 B xz, and
  squishy is +0 or +512 B.
