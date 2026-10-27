# WaveSabre songs in released productions (survey, October 2026)

Source list: Demozoo's `wavesabre` tag (50 entries) and the WaveSabre README. Downloads from scene.org / group sites,
song blobs extracted with `tools/ws-extract` (Wine + memory scan), converted with `synth-convert ws` + `song import`, and
rendered by both engines. **The music is the authors' work: blobs and renders stay local
(`~/Downloads/64k-tools/ws-intros/out`), never in the repository.**

Parity: `wavesabre` (our bit-exact WaveSabre port, GM.DLS from `synth-lab/ws/gm.dls`) vs `unisynth` (the converted
song, what the DAW plays): BS.1770 loudness Δ and the worst octave band 63 Hz–8 kHz (`tools/synth-parity/metrics.ts`).
16 kHz differs by up to 3 dB everywhere (a known high-band difference of the unified engine, not song-specific).

| Released | Production | By | Type | Tempo | Length | Tracks | Loudness Δ dB | Worst band Δ dB |
|---|---|---|---|---:|---:|---:|---:|---:|
| 2018-10 | Trashpanda | Logicoma | 64K intro | 168 | 3:03 | 50 | −0.06 | 0.55 |
| 2019-04 | Atlas | Macau Exports, Monad | 64K intro | 122 | 2:26 | 45 | −0.04 | 0.15 |
| 2019-04 | Brainworm | Polarity | 64K intro | 128 | 4:08 | 19 | 0.01 | 0.69 |
| 2019-04 | Dope On Wax | Logicoma | 64K intro | 93 | 3:35 | 44 | 0.06 | 0.55 |
| 2019-04 | The Secret Service Agency | H0ffman, Ivory Labs, Quadtrip | 64K intro | 135 | 3:44 | 37 | 0.11 | 0.60 |
| 2019-04 | Haec Diva | Darth Shader, Ferris, Wobble | 32K exe music | 96 | 3:10 | 39 | 0.00 | 0.09 |
| 2019-04 | Makeshift Reprise | H0ffman | 32K exe music | 140 | 3:46 | 19 | 0.06 | 1.08 |
| 2019-04 | Untie The Knot | liqube | 32K exe music | 120 | 4:12 | 19 | 0.06 | 0.12 |
| 2019-09 | Lint:C | Digital Dynamite | 64K intro | 144 | 2:42 | 19 | 0.18 | 1.07 |
| 2020-04 | Hydra | Teo | exe music | 128 | 3:08 | 32 | 0.04 | 0.70 |
| 2020-07 | Smoothie | Ninjadev | 64K intro | 190 | 2:16 | 37 | 0.02 | 0.48 |
| 2020-07 | When Everyone Else Has Gone to Sleep | Sigveseb | exe music | 120 | 2:12 | 22 | 0.04 | 0.84 |
| 2021-04 | Miami Night Cruise | Kirkwood West | 32K exe music | 100 | 3:17 | 25 | −0.03 | 0.83 |
| 2021-04 | Multiplicity | liqube | 32K exe music | 130 | 4:00 | 31 | 0.05 | 0.21 |
| 2021-04 | We're All Biohazards Now | Byproduct | 32K exe music | 180 | 3:03 | 44 | 0.46 | **3.07** (250 Hz) — not imported as a demo |
| 2022-01 | astrophage | Logicoma | 64K intro | 130 | 2:06 | 14 | −0.08 | 0.81 |
| 2023-04 | Try Stopping. | Eladamri | 32K exe music | 130 | 3:11 | 9 | −0.03 | 0.83 |
| 2023-07 | Ouefvoke | Rebels | 64K intro | 110 | 2:03 | 18 | −0.47 | 0.68 |
| 2024-03 | Skranglefant | Eladamri | 32K exe music | 174 | 3:11 | 15 | −0.15 | 0.91 |
| 2024-11 | Electromagination | Filippp | 32K exe music | 120 | 2:18 | 10 | −0.42 | 0.98 |
| 2025-04 | Breaking Waves | Eladamri | 32K exe music | 150 | 3:58 | 18 | −0.01 | 0.96 |
| 2025-04 | Indigo Shift | liqube | 32K exe music | 172 | 4:17 | 50 | 0.05 | 0.13 |
| 2026-04 | 4o4 Instance | MaV | exe music | 139 | 4:00 | 24 | 0.05 | 0.26 |
| 2026-04 | Nightform | Digital Dynamite, Lethargy | 64K intro | 120 | 2:34 | 25 | 0.03 | 0.92 |
| 2026-06 | Hold Da Bass Up | MaV | exe music | 139 | 4:09 | 22 | 0.19 | 0.68 |

## DAW demos

The 24 rows within ~1 dB (all but Biohazards) are imported as DAW songs in `project/music/released/` (git-ignored) by
`python3 tools/ws-extract/make-demos.py`, using `details.json` (Demozoo credits and placings) next to `catalog.json`.

## Not extracted

| Production | Why |
|---|---|
| Reverence (2015), Bare Faced Cheek (2016), Elysian (2016), Engage (2017), Soundproof Motion (2017), iota (2018), Bros Before Foes (2018) | pre-2019 private song format (the blob is found by tempo/rate/length, but device chunks carry no size); needs that format reverse-engineered. Elysian and Engage exist as projects in the WaveSabre test suite already. |
| Orthonormalizer, Bright Velvet, IO Transmitter, Dinette | 32-bit squishy builds crash inside Wine (ntdll fault) before unpacking finishes; need real Windows. |
| Cuboid (Rootkids) | blob found, but tracks 10 on and the master have volume 0 in it: silent as stored (a fork or runtime-set volumes). |
| Robo Outer Space Epos, Glorp | fork device ids 13 / 14, unknown to the converter. |
| Gotta Go Fast, Guttersnipe, Rubbik, Light Reading, invot teih martx | no reachable executable download (mediafire, none listed, PowerShell entry, dead link). |
