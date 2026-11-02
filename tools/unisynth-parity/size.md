i686 `no_std` probe, opt-level z, LTO, static base (`research/us-size-probe`, `measure.sh`), the song built in as Rust (`synth-convert rust`,
`us-trusted`). Raw and packed bytes include the song's own bytes (the "song" column); the build lists exactly the features
`synth-convert features` names for the song.

| build | song B | raw B | xz -9e B | squishy 0.1.3 B | features |
|---|---:|---:|---:|---:|---|
| all-modules | 3401 | 155136 | 75740 | 56832 | us-all |
| global-groove | 559 | 31744 | 16264 | 14336 | us-noise,us-flt-svf,us-env-seconds |
| elysian | 63301 | 123904 | 53908 | 45568 | us-osc-pm,us-osc-blit,us-osc-sample,us-sample-gsm,us-noise,us-flt-svf,us-env-seconds,us-fx-distortion-shaper,us-fx-crusher,us-fx-eq-parametric,us-fx-delay-tempo,us-fx-reverb-rooms,us-fx-compressor,us-voice-lfo,us-voice-unison,us-voice-oneshot,us-matrix,us-automation |
| engage | 80254 | 152064 | 48192 | 38912 | us-osc-pm,us-osc-blit,us-osc-sample,us-sample-gsm,us-sample-dls,us-noise,us-flt-svf,us-env-seconds,us-fx-distortion-shaper,us-fx-crusher,us-fx-eq-parametric,us-fx-delay-tempo,us-fx-flanger-phaser,us-fx-reverb-freeverb,us-fx-reverb-rooms,us-fx-compressor,us-voice-lfo,us-voice-unison,us-voice-oneshot,us-matrix,us-automation |
| eladamri | 32247 | 94208 | 39404 | 31744 | us-osc-pm,us-osc-blit,us-osc-sample,us-sample-gsm,us-noise,us-flt-svf,us-env-seconds,us-fx-distortion-shaper,us-fx-eq-parametric,us-fx-delay-tempo,us-fx-reverb-rooms,us-fx-compressor,us-voice-lfo,us-voice-glide,us-voice-mono,us-voice-unison,us-matrix,us-automation |
| no-remorse | 57534 | 106496 | 28548 | 23552 | us-osc-wt,us-noise,us-flt-ladder,us-flt-rbj,us-env-seconds,us-env-rate,us-fx-distortion-power,us-fx-eq-3band,us-fx-delay-ms,us-fx-reverb-compact,us-voice-lfo,us-matrix |
| tf4testsong | 12279 | 60928 | 26720 | 22016 | us-osc-wt,us-noise,us-flt-ladder,us-flt-rbj,us-env-seconds,us-env-rate,us-fx-distortion-power,us-fx-eq-3band,us-fx-delay-ms,us-fx-reverb-compact,us-voice-lfo,us-matrix |
| v2-fr08 | 288888 | 355840 | 49288 | 35840 | us-voice-lfo,us-matrix,us-automation,us-matrix-add,us-env-frame,us-osc-naive,us-flt-feedback,us-drive,us-lfo-frame,us-voice-out,us-strip,us-fx-eq-cut,us-fx-delay-modulated,us-fx-reverb-comb,us-fx-compressor-lookahead,us-chunk-wide,us-quirk-exact-math,us-quirk-fixed-seed,us-quirk-lcg-noise,us-quirk-coarse-env |
| v2-brullwurfel | 43495 | 110592 | 35896 | 28672 | us-voice-lfo,us-matrix,us-automation,us-matrix-add,us-env-frame,us-osc-naive,us-flt-feedback,us-lfo-frame,us-voice-out,us-strip,us-fx-eq-cut,us-fx-delay-modulated,us-fx-reverb-comb,us-fx-compressor-lookahead,us-quirk-exact-math,us-quirk-fixed-seed |
| v2-candytron | 80043 | 159232 | 47380 | 36352 | us-voice-lfo,us-matrix,us-automation,us-matrix-add,us-speech,us-env-frame,us-osc-naive,us-flt-feedback,us-flt-parallel,us-drive,us-lfo-frame,us-voice-out,us-strip,us-fx-eq-cut,us-fx-delay-modulated,us-fx-reverb-comb,us-fx-compressor-lookahead,us-chunk-wide,us-quirk-exact-math,us-quirk-integer-fm,us-quirk-fixed-seed,us-osc-combine |
| v2-kkrieger | 37417 | 116736 | 41260 | 32256 | us-voice-lfo,us-matrix,us-automation,us-matrix-add,us-speech,us-env-frame,us-osc-naive,us-flt-feedback,us-flt-parallel,us-drive,us-lfo-frame,us-voice-out,us-strip,us-fx-eq-cut,us-fx-delay-modulated,us-fx-reverb-comb,us-fx-compressor-lookahead,us-chunk-wide,us-quirk-exact-math,us-quirk-integer-fm,us-quirk-fixed-seed,us-osc-combine |
| turtles | 98154 | 146432 | 37224 | 28672 | us-osc-wt,us-noise,us-flt-ladder,us-flt-rbj,us-env-seconds,us-env-rate,us-fx-distortion-power,us-fx-delay-ms,us-fx-reverb-compact,us-voice-lfo,us-voice-glide,us-matrix |

Measured 2026-11-02 after the format switch (change unify-v2-algorithms, tasks 6.5–6.9; `measure.sh` re-converted every
song with this tree's converters). Against the previous table (from `4f74abf`), the reference songs are one squishy
step (512 B) larger and all-modules 7,680 B. Neither comes from the switch: on the same baked bytes the switch tree
matches the 6.4 tree in squishy on every reference song and on all-modules (+412 B xz), and each unify stage was gated
against the previous one (design, Gate 6). all-modules measured pre-unify (`e3fef8f`) 50,688, stage 5 56,832, now
56,832: `us-all` gained the strip, the wide chunk, quiet tracks and the clocked oscillator (cauldron) along that line.
The per-stage and switch figures are in `openspec/changes/unify-v2-algorithms/design.md`.

Speed after the switch (unisynth `render` example, median of 3, interleaved, against the 6.4 tree; outputs
byte-identical): elysian +2.9 %, eladamri +0.5 %, no-remorse +2.3 %, tf4testsong +2.2 %, kkrieger +1.0 %,
brullwurfel +2.1 %; no-remorse against the build before unify-v2-algorithms +5.0 % (median of 5).

turtles: without `ES4_SONGS` (`zoom.tf4m`), exported from `project/music/turtles.song.json` (98,154 B). Its bytes differ
from the `.tf4m` conversion of earlier tables (97,922 B: 142848 raw / 31164 xz / 25088 squishy), so compare it only
with the "before" below, built from the same bytes.

V2 rows (`v2-*`, change add-v2-import): converted with `synth-convert v2m`.

Cost of the V2 work for songs without V2 (xz -9e / squishy 0.1.3 bytes; "before" = the build before add-v2-import):

| song | before | V2 first measured | after gating | now |
|---|---|---|---|---|
| global-groove | 15364 / 13824 | 16912 / 14848 | 15996 / 14336 | 15544 / 14336 |
| elysian | 52680 / 45056 | 54436 / 46080 | 53560 / 45568 | 53044 / 45056 |
| engage | 46988 / 37888 | 48576 / 39424 | 47736 / 38912 | 47344 / 38400 |
| eladamri | 37840 / 30720 | 39688 / 32256 | 38740 / 31744 | 38308 / 31232 |
| no-remorse | 27152 / 22528 | 28760 / 23552 | 27848 / 23040 | 27360 / 22528 |
| tf4testsong | 25320 / 20992 | 26904 / 22528 | 26028 / 21504 | 25540 / 21504 |
| turtles (song.json) | 35836 / 27648 | | | 35988 / 28160 |

"After gating": every V2 branch behind its feature's `cfg!`; V2 voice and filter-route state in `v2::Opt`. "Now",
in addition: the V2 slot, envelope and LFO state in `v2::Opt`; the `flt::Kind::V2` variant only with `us-flt-feedback`; a
trusted build without the V2 kinds decodes choices and effect modes with the old law; chunk escapes behind
`us-chunk-wide` (5-byte pairs without it); `run_ext` only with `us-strip`. The rest (+150 to +470 B xz) is the other
voice-struct growth and the instrument's render split.

Aux buses and the Moog fix (task 5.2, commit `4b8d7ce`) vs `49185ef`, same compo probe in clean worktrees, each song
converted by its own tree (raw / xz -9e / squishy 0.1.3 bytes). The aux path is behind `us-strip`, so songs without V2
are byte-identical; every V2 song has `us-strip` and carries the aux-input plumbing even without an aux oscillator.

| song | before | after | Δ xz |
|---|---|---|---:|
| 7 songs without V2 | | identical | 0 |
| v2-candytron | 157184 / 46056 / 35328 | 157184 / 46148 / 35328 | +92 |
| v2-platinum (aux) | 161792 / 39228 / 30720 | 161792 / 39384 / 30720 | +156 |
| v2-brullwurfel | 109056 / 34820 / 27648 | 109056 / 34992 / 27648 | +172 |
| v2-debris (aux) | 508416 / 51076 / 38400 | 508928 / 51272 / 38400 | +196 |
| v2-kkrieger | 114688 / 39908 / 31744 | 114688 / 40112 / 31744 | +204 |
| v2-fr08 | 354304 / 47816 / 34816 | 354304 / 48192 / 34816 | +376 |

V2 oscillator pan in a unisynth voice (mixed patches), after `4b8d7ce`: v2-fr08 354816 raw / 48060 xz (−132),
v2-brullwurfel 109568 / 35084 (+92), global-groove unchanged (15544). V2-voice renders stay byte-identical.

Legacy figures (`research/softsynth-research.md`): WaveSabre port, all 13 devices + 1 KB test sample: 71,168 raw / 34,472 xz / 27,648 squishy;
Slaughter only: 28,672 / 15,968 / 14,336. Tunefish-compatible port: 41,472 raw with every effect, 30,208 without.

## Mini-cauldron songs (change add-cauldron-xm-import, measured 2026-10-30)

Same build as the table above (`measure.sh`'s flags, baked, `us-trusted`), songs from `synth-convert xm`
(`research/tbl-mini-cauldron/out/songs`, git-ignored). `measure.sh` does not build these rows: rerun with the probe's
`US_SONG` and the song's `synth-convert features`.

| build | song B | raw B | xz -9e B | squishy 0.1.3 B | features |
|---|---:|---:|---:|---:|---|
| tbl-jizz | 365125 | 425472 | 49000 | 40448 | us-osc-sample,us-sample-gsm,us-env-seconds,us-fx-distortion-shaper,us-fx-eq-3band,us-fx-delay-tempo,us-voice-glide,us-voice-mono,us-automation,us-osc-cauldron |
| tbl-stash-bluish | 616853 | 677888 | 62516 | 49152 | jizz's + us-fx-flanger-comb |
| tbl-stash-song1 | 1447168 | 1509376 | 70500 | 54784 | bluish's + us-fx-distortion-wrap |

The song bytes are almost all per-tick automation (XM volume, pan, bend, low-pass cut per track and tick): song 1
packs to 54.8 KB with squishy, music alone, so a converted TBL song does not fit a 64k intro as is.

Against the original (TBL 1997). The original ships an XM **template**: patterns, headers and the stored samples,
with every synthesized sample area zero (filled at startup by mini-cauldron), plus the synth code and its patch
records. Template = the XM cut from the unpacked intro image with the synthesized areas zeroed, xz -9e; ours = the
converted `.song`, xz -9e (data only, no player in either):

| | original: template (xz) | original: patches | original: synth code | ours: song (xz) |
|---|---:|---:|---:|---:|
| Jizz | 14 812 | 12 × 123 = 1 476 | ≈ 1.4 K (x87, v1) | 17 888 |
| Stash song 1 | 14 704 | | | 38 588 |
| Stash BLUISH | 13 688 | | | 30 552 |
| Stash (both, one intro) | 16 748 | 26 × 103 = 2 678 | ≈ 2 K (x87, v2) | 60 296 |

Code: the original's mini-cauldron is ≈ 1.4 / 2 K; ours, `us-osc-cauldron` + comb + wrap ≈ 15.3 K raw / 8.7 K xz on
top of the unified player (the songs' whole builds below). Not measured: the original's XM replay (its GUS player,
part of the intro code). Whole intros, graphics and code included, as released: `jizz.ixa` 59 275 B, `stash.ixa`
88 015 B. Jizz's data is close (+21 %); Stash's is 3.6× the original, the cost of per-tick automation for its
slides, tremor and ping-pong passages, where the original's tracker replays a few pattern effects.

Per-feature code cost (each added to a song that does not use it, same tree; raw / xz / squishy bytes):

| feature | on | raw | xz -9e | squishy |
|---|---|---:|---:|---:|
| `us-osc-cauldron` | global-groove | +13 824 | +7 536 | +5 632 |
| `us-fx-flanger-comb` | tbl-jizz | +1 024 | +732 | +512 |
| `us-fx-distortion-wrap` | tbl-jizz | +512 | +404 | 0 |

Cost to songs without these features: global-groove 30 720 / 15 796 / 14 336 before the change (`60e9be5`), now
31 232 / 16 052 / 14 336 (+512 raw, +256 xz, squishy unchanged), from the cauldron commit (`4daefd2`, which also
carries unify-v2-algorithms' unisynth side). The restart declick costs non-users nothing (its state exists only with
`us-osc-cauldron`; ungated it was +512 raw / +32 xz). The song rows were built before the declick's last form;
the cauldron row after it.

Render speed (release `render` example, one thread, i9-9900K; render time / audio time, below 1 = faster than real
time): Jizz 166.3 s in 76.1 s (0.46, 2.2× real time, pool 47 MB), BLUISH 84.4 s in 51.3 s (0.61, 1.6×, 153 MB),
song 1 203.7 s in 206.9 s (**1.02: slower than real time**, 291 MB). With the quiet-track skip (`us-quiet`, change
add-quiet-track-skip; tracks sound only 12–21 % of the time): Jizz 32.6 s (0.20), BLUISH 32.6 s (0.39), song 1 88.1 s
(0.43); output within one 16-bit step of the unskipped render (4–603 frames). Size: `us-quiet` on Jizz +512 raw /
+484 xz / +512 squishy; songs without it +512 raw / +128 xz / 0 squishy (global-groove). The
cost is the track count (song 1: 106 phrase tracks, one cauldron voice each at the phrase clock, plus the comb,
echo, doubler and two low-pass stages per track) and the delay lines the pool holds.
