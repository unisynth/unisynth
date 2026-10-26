# Open-source softsynths for 64k intros (2026-10-25)

Goal: pick or borrow a synth for the 64k soft synth (cargo native side, Windows
size-critical, macOS x86_64/arm64 dev targets).

## Brain Control — Tunefish 4 (the one asked about)

- Repo: https://github.com/paynebc/tunefish — **GPL-3.0**, C++, ~300★. Site: https://tunefish-synth.com
- Virtual-analog synth. Synth core is designed to fit in **~10 KB of compressed machine code**.
  Made for Brain Control's 64k "Turtles all the way down".
- Released as a VST2/VST3/AU plugin (JUCE UI) for Win/macOS/Linux, v4.2.0 (2018).
  Open-sourced in 2017.
- Repo layout (from cloning it):
  - `src/tunefish4/Source/synth/tf4.cpp` (1761 lines), `tf4fx.cpp` (702) — DSP core, no JUCE dependency
  - `src/tunefish4/Source/runtime/` — small runtime (SSE `simd.hpp`: `_mm_*`, `_mm_blend_ps` = SSE4.1)
  - `src/tunefish4/Source/synth/tfrecorder.cpp` — records DAW notes into the `.tfm` song format
  - `src/tunefish4player/` — intro-style player (`tf4player.cpp`, DirectSound `tf4dx.cpp`, Windows/VS only)
  - `media/tf4modules/*.tfm(.h/.js)` — example songs exported as a C header and JS
- **Catch:** GPL-3 is copyleft. Linking the core into the intro means the intro must be
  GPL too (source release). Fine for an open-source intro; not if we want to keep it closed.
  The SSE intrinsics need a NEON path for the arm64 Mac.

## Other open-source demoscene synths

| Synth | Group | License | Lang | Target | Notes for us |
|---|---|---|---|---|---|
| **WaveSabre** — github.com/logicomacorp/WaveSabre | Logicoma | MIT | C++ (+C# tools) | 64k, Windows x86/x64 | Most used modern 64k synth. VSTs plus a "Project Manager" that exports songs. Rust `no_std` wrapper exists: github.com/Kwarf/wavesabre-rs (Windows only). Uses Windows APIs (e.g. GM.DLS samples) |
| **64klang** — github.com/gopher-atz/64klang | Alcatraz | MIT | C++/asm | 64k, Windows | Modular node-graph synth that grew out of 4klang. VST3 GUI is Windows-only |
| **Tunefish 4** | Brain Control | GPL-3 | C++ | 64k, cross-platform plugin | See above |
| **V2 / libv2** — github.com/farbrausch/fr_public (`v2/`), revived build: github.com/EZForever/Viruz2 | Farbrausch | Artistic 2.0 / BSD parts | C++/x86 asm | 64k, 32-bit Windows | Classic synth from 2005–2015. Low CPU use. Old 32-bit asm |
| **Sointu** — github.com/vsariola/sointu | pestis | MIT | Go tools; output is asm/WASM | 4k/8k (also bigger) | 4klang fork. Targets 386, amd64 and WASM. Tracker runs on Win/macOS/Linux. Its WASM output fits a three.js/web editor well |
| **4klang** — github.com/gopher-atz/4klang | Alcatraz | MIT | asm | 4k | Ancestor of 64klang and Sointu. 32-bit replayer |
| **Oidos** — github.com/askeksa/Oidos | Loonies (Blueberry) | zlib | asm + Python/Rust tools | 4k/8k | Additive synth, used with Renoise. 32-bit replayer, heavy precalc |
| Clinkster | Loonies | — | asm | 4k | 32-bit only |

Lists: https://64k-scene.github.io/resources.html (Music section), https://in4k.github.io/wiki/sound

## Takeaways for 64k

1. Size budget: music usually takes about 16 KB in a 64k intro. Tunefish's ~10 KB core is in line with that.
2. Licenses: WaveSabre, 64klang, Sointu and 4klang are MIT, Oidos is zlib. These are safe to port or borrow
   from in a Rust synth. Tunefish (GPL-3) is safe only if the intro is released as GPL.
3. Every candidate except Sointu ships a Windows-only or x86-only replayer. None gives us a ready
   **Rust no_std player that runs on Windows and macOS arm64**. To get one, either:
   - (a) port one DSP core to Rust (Tunefish `tf4.cpp` is the smallest readable VA core, about 2.5k LOC;
     WaveSabre devices if we need MIT), or
   - (b) write our own synth and use these as references, with the TS editor twin running the same
     DSP through WASM.
4. Next probe to settle this: build the Tunefish core or WaveSabre `WaveSabreCore` as a static lib for i686,
   link it into `research/bootstrap-probe`, and measure the size after squishy/kkrunchy.

## WaveSabre vs 64klang — compared from source (cloned 2026-10-25)

Sources: `logicomacorp/WaveSabre@master`, `gopher-atz/64klang@master` (64klang3). Both MIT.

| Aspect | WaveSabre | 64klang3 |
|---|---|---|
| Model | Fixed **devices** (one VST each): Falcon (2-op FM), Slaughter (3-osc subtractive), Specimen (GSM sampler), Adultery (GM.DLS sampler), and FX Scissor, Leveller, Crusher, Echo, Smasher (sidechain compressor), Chamber, Cathedral (reverbs), Twister | One plugin. A **modular node graph** of about 60 primitives (osc, ADSR, LFO, biquad/SVF/onepole, delay/fbdelay/comb, reverb, EQ, formant, bowed/STK, formula, sampler, GM.DLS, SAPI TTS, …) |
| Processing | **Block** based: `Device::Run(songPos, float** in, float** out, n)`, float32 buffers | **Per sample**: whole graph ticked each sample. Allows sample-exact feedback, physical modelling |
| Numeric type | float32 buffers, double phase. Helpers use x87 asm (`fsin`, `f2xm1`) and a sine table | `sample_t` = `__m128d` (stereo double), SSE4.1; NEON path in `platform.h` |
| Voices | `SynthDevice` with 256 voices, mono/poly, unison/detune/pan, slide | Graph hierarchy SynthRoot → ChannelRoot×16 → VoiceManager → VoiceRoot. Voice subgraphs are instanced per note |
| Mixing / routing | **Track graph**: per-track device chain, receives/sends with volume, sidechain. Converter flattens the DAW mixer | Everything lives inside the one graph (channel roots feed synth root) |
| Automation | Per-track lanes: (device, param, delta-time, 8-bit value) | Song stream carries only note on/off. Modulation is built in the graph (EventSignal/MidiSignal/TriggerSeq) |
| Song format | Binary: tempo, devices (id + param chunk), MIDI lanes (delta + note/vel), tracks (receives, devices, automation). Documented in `Docs/Serialized-song-format.md` | Generated C headers `64k2Song.h` (delta-encoded, split into timestamp/note/velocity streams, 64-sample quantised: compresses well) + `64k2Patch.h` (node array, `NID` packed ids, const pool). Also `.blob` runtime format |
| Authoring | Musician uses any DAW (Renoise, Ableton Live, FL, Reaper) with WaveSabre VSTs. **WaveSabreConvert** (C#) parses the project file | Musician uses the 64klang3 VST3 with its ImGui node editor. The plugin exports the song/patch itself |
| Player | `SongRenderer` with N render threads (Win32 events) that render tracks in dependency order. PreRender/Realtime players, DirectSound | `_64klang_Render(float*)` prerenders the whole song single-threaded. Reference player uses waveOut |
| Core size (LOC) | WaveSabreCore ~3.9k + PlayerLib ~0.7k | SynthNode.cpp 5.8k + Synth.cpp 1.2k + sample_t ~0.9k (SynthController/TinyXML are plugin-only) |
| Windows-only bits | GSM decode via ACM (Specimen), GM.DLS file (Adultery), threads/DirectSound, x87 asm | GSM via ACM (Sampler), GM.DLS, SAPI TTS. Stubbed to silence on macOS/Linux |
| CPU | Cheap: block processing, float | Expensive: per-sample double graph. Needs prerender time |

### Common ground (what lines up 1:1)
- Same sample rate (44.1k), same "note events + compact patch data" idea, both prerender for intros.
- Shared DSP primitives exist in both: ADSR, biquad, state-variable filter, delays/comb/allpass, Schroeder-style reverb,
  distortion/clip, compressor, EQ, noise, FM/sine/saw/square oscillators, GSM sampler, GM.DLS sampler.
- Both need the same platform replacements: our own GSM 6.10 decoder (or no GSM), GM.DLS only on Windows.

### Merge verdict
A literal merge does not work: block/float devices and a per-sample double graph are different execution models.
A **layered merge** is natural and cheap:
1. **One DSP primitive library** (Rust `no_std`, f32 or f64 per feature flag) with osc/ADSR/biquad/SVF/delay/comb/allpass/reverb/shaper.
2. **Graph instrument** = 64klang-style per-sample node graph built from those primitives (the "64klang device").
3. **Fixed devices** = WaveSabre's Falcon/Slaughter/FX written against the same primitives. They are simply predefined graphs or
   hand-written block devices.
4. **Outer song layer from WaveSabre**: tracks, device chains, sends/sidechain, param automation, multithreaded track render.
   A 64klang graph becomes one device type inside a track.
5. **Song encoding from 64klang**: split delta streams, which compress better under squishy/kkrunchy.
Only the devices and nodes a song actually uses get linked, via Cargo features / codegen like `crates/effects`.

## GM.DLS — where to get it, licensing, open alternatives (2026-10-25)

How intros use it: **the bank is never shipped.** WaveSabre (Adultery) and 64klang (GMDLS node) read the copy that is
already on the user's machine at runtime, so it costs 0 bytes and needs no redistribution.
- Windows: `%SystemRoot%\System32\drivers\gm.dls`. This is Roland Sound Canvas "GS Sound Set/Microsoft (P) 1996 Roland",
  licensed "for use with Microsoft operating system products only". **Do not commit or download it from mirrors.** For dev,
  copy it from a Windows install we own. It is not in the Wine prefix `~/.wine-64k`.
- WaveSabre hard-codes `WaveListOffset = 0x44602`, `NumSamples = 495` for the Windows file. 64klang parses the RIFF
  (`lins`/`wvpl`/`ptbl`). **We must parse, not hard-code**, so that other banks also work.
- **macOS has an equivalent on every install:**
  `/System/Library/Components/CoreAudio.component/Contents/Resources/gs_instruments.dls` (1,996,068 B, RIFF `DLS `,
  INFO: "1997 Roland Corporation", "QuickTime Music Synthesizer"). Measured: **235 instruments, 495 waves** (same wave
  count as Windows gm.dls), PCM mono 8-bit, 22050 Hz (3 waves at 24000). It is very likely the same Roland GS set at
  8-bit. Wave/instrument index equivalence with gm.dls is unverified: diff it once we have a gm.dls copy.
- Linux: there is no system DLS. The intro can try common SF2 paths (`/usr/share/sounds/sf2/FluidR3_GM.sf2`, Debian
  `fluid-soundfont-gm`), which sound different, or fall back to silence or synth-only.

Open-source / free GM banks. They are too big to embed in 64k, but usable for the editor preview and as a Linux fallback:
| Bank | License | Format | Notes |
|---|---|---|---|
| GMGSx.sf2 (Kenneth Rundt, SynthFont) | public domain | SF2 | Built to sound like gm.dls / Roland GS |
| FluidR3_GM (Frank Wen) | MIT | SF2 (~140 MB) | Linux distros often install it |
| MuseScore_General | MIT | SF2/SF3 | Based on FluidR3 |
| GeneralUser GS 2.x (S. Christian Collins) | permissive custom (free use, modify, redistribute) | SF2 | ~30 MB, GS-compatible |
| sfzmaker/sfzpack-gm.dls | CC0 for the SFZ mapping only | SFZ | Samples are still Roland/Microsoft: not free |
Tools: Kermalis/DLS2 (MIT, C# DLS read/write), spessasus/SpessaFont (Apache-2, web SF2/DLS editor; good fit for the TS editor).

### Implemented: `crates/dls` (step 1)
A `no_std` zero-copy DLS reader. Run against the macOS bank it reads all 235 instruments and 495 waves, with none
unreadable. Drum kits follow the GS convention (bank 0 + percussion flag, kit = program: 0 Standard, 8 Room, 16 Power,
24 Electronic, 25 TR-808, 32 Jazz, 40 Brush, 48 Orchestra, 56 SFX). Fingerprint for the step-2 diff:
`cargo run -p dls --example dump -- <bank> > x.tsv`, then diff the macOS TSV against the gm.dls TSV.

### Step 2: gm.dls vs macOS gs_instruments.dls (measured)
Source: `Windows\System32\drivers\gm.dls` taken from the local VMware "Windows 10 x64" base disk, read-only, with
`~/Downloads/64k-tools/gmdls/vmdk_ntfs_get.py` (VMDK sparse → GPT → NTFS MFT). It sits in `~/Downloads/64k-tools/gmdls/`
(3,440,660 B, sha256 `3229b09b…f2c0`) and is also in `~/.wine-64k/.../system32/drivers/`. **Never commit it.**

| | Windows gm.dls | macOS gs_instruments.dls |
|---|---|---|
| Instruments | 235 | 235: same (drums, bank, program) keys and names |
| Waves | 495, 16-bit | 495, 8-bit. Same rate (22050; 3 at 24000), same lengths, same loops |
| Sample data | — | **same recordings**: per-wave correlation min 0.9994, median 1.0000 |
| Region→wave maps | | 199/235 identical; 36 differ (e.g. Glockenspiel, Piccolo, Detuned EP 2 have more regions on Mac) |
| wsmp tuning | | 65 waves differ in unity note and/or fine tune |
| Articulation | Piano: 14 blocks | Piano: 21 blocks |

Conclusion: the macOS bank is the same Roland GS sample set at 8-bit, with slightly different key mapping and tuning.
Wave **indices are portable**, so a WaveSabre-style "sample N" reference sounds the same on both, apart from 8-bit noise.
Instrument-level playback (regions + articulation) differs a little per OS. If exact parity matters, store wave index +
our own tuning/envelope in the song instead of relying on the bank's regions/articulation.
`GM_DLS=<path> cargo test -p dls` runs the gm.dls test.

## Tunefish 4: integration check (measured 2026-10-25)
Built the core outside JUCE on macOS x86_64 (clang++ -std=c++17): `synth/tf4.cpp`, `synth/tf4fx.cpp`, `runtime/*`,
`tunefish4player/tf4player.cpp` and `datastream.cpp` **compile unmodified**. A 20-line harness (`eTfPlayerInit` → `LoadSong(.tfm)` →
`Process` per 512-frame block) renders WAVs (harness in `/tmp/tfbuild/render.cpp`):
- `no_remorse.tfm` (201.6 s): rendered at **12× realtime**, peak 10776/32767.
- `tf4testsong.tfm` (58.1 s): segfaults upstream in `eTfPlayerGetSongLength` (`events[size-1]` on an instrument with
  no events). A one-line guard fixes it; it then renders at 35× realtime.
- Code size, `-Os` x86_64 `__text`: tf4.o 12.7 KB + tf4fx.o 5.0 KB ≈ 17.7 KB uncompressed (the "~10 KB compressed" claim is plausible).
- Song size: `.tfm` 8 KB / 39 KB raw → **0.7 KB / 1.3 KB xz**.
- Design: block based (512 frames), float32, 32 instruments × 16 voices, IFFT-spectral generator (PADsynth-like), 2 LFO,
  ADSR, mod matrix, noise, filters. FX: delay, reverb, distortion, formant, EQ, chorus, flanger. Uses `new` in
  LoadSong/Init (needs an allocator shim in no_std), `eArray`, and `<simd/simd.h>` float2 on Apple (so arm64 works) or SSE4.1 elsewhere.
- Authoring: JUCE VST2/VST3/AU on Windows, macOS and Linux, with a recorder producing `.tfm`. It runs on our Mac dev boxes today.

Blocker: **GPL-3.0.** Static linking or a line-by-line Rust port makes the distributed intro a GPL-3 work: full source must be
published, and our MIT crates must stay GPL-compatible (MIT is). Options: (a) release the intro as GPL-3, (b) ask Brain Control
(paynebc) for an MIT/zlib grant for the synth core + player, (c) clean-room reimplementation of the ideas (IFFT generator) from docs.

### Decision: own MIT reimplementation (option c). Superseded 2026-10-25: reading the source is allowed, no strict clean room
- Step 1 (done): a spec-writer agent read the GPL source and wrote `research/tunefish4-functional-spec.md`. It covers behaviour,
  algorithms (textbook names), the 112-parameter table with mapping formulas, the `.tfm` byte layout and 22 quirks. It contains
  no source code, no tables, no identifiers.
- Step 2: a separate implementer context writes the MIT Rust synth from the spec only. Verification is black-box: render
  the same `.tfm` with the original (`/tmp/tfbuild/render`) and with ours, then compare spectra/levels by measurement or by ear.
  Algorithms and file formats are not copyrightable; the expression is. Keeping the readers and the writers separate is our evidence.

## WaveSabre and 64klang: integration check (measured 2026-10-25)
Harnesses live in `~/Downloads/64k-tools/synth-lab/` (never in /tmp, which a reboot wipes). Windows builds use
docker `debian:bookworm-slim` + i686 mingw, and run under Wine32 with `gm.dls` and `libgsm1:i386`.
**Two kernel panics on the dev Mac happened during long Docker/Wine renders**, so treat long Wine renders as risky on this machine.

### 64klang (64klang3 master)
- The core (`Synth`, `SynthNode`, `SynthAllocator`, `sample_t`) **compiles on macOS** (clang, `-msse4.1`), and with MinGW for Win32/Win64.
- **Upstream bugs in the intro/player path** (the non-VST build):
  1. GM.DLS globals (`lpGMDLSSuffix`, `lpGMDLSBuffer`, `GMDLS_FILEBUFFER_SIZE`) are declared only under `COMPILE_VSTI`
     but used without it, so the player does not compile on Windows. Fix: 3 declarations (forced-include shim).
  2. The Sampler null checks (no wave table, empty source buffer) exist only under `COMPILE_VSTI`. A song whose
     Sampler source is missing segfaults: GSM/GM.DLS on macOS, SAPI TTS under Wine. The README's "silence on
     macOS/Linux" claim is true only for the plugin.
- After both fixes, the bundled example song (278.5 s) renders: macOS native **3.2× real time**, Win32 under Wine **3.3×**.
  Peak 1.52 (Mac, samplers silent) vs 1.83 (Win32). The engine is single-threaded, per-sample, stereo double.
- Code size, `-Os` x86_64, all node types: SynthNode 29.3 KB + Synth 1.7 KB + sample_t 1.5 KB ≈ **32.5 KB**
  (unused nodes can be skipped via `*_SKIP` defines from the exported patch header).
- Windows-only nodes: Sampler with stored GSM (ACM), GM.DLS sampler, SAPI TTS.

### WaveSabre
- **24 of 27 WaveSabreCore files compile on macOS unchanged.** The Windows-only ones are `GmDls.cpp` (Adultery) and
  `GsmSample.h` users (Specimen, Thunder: GSM via ACM). `WaveSabrePlayerLib` uses Win32 threads, events and DirectSound.
- MinGW needs case shims (`Windows.h`, `MSAcm.h`). `RealtimePlayer`/`PreRenderPlayer` fail on `min`/`max` but are not needed.
  `SongRenderer` + core + `CriticalSection` link fine.
- Test songs: `Tests/WaveSabreConvertTests/Refs/*.ref` are JSON dumps of real converted songs (SabreWulf, Elysian,
  Engage, Reverence…). `synth-lab/ws/json2bin.py` writes the documented binary format (automation floor(v·255)).
- Renders (Win32 under Wine, 1 render thread):
  - Global Groove (8.1 s, Slaughter only): **172× real time**.
  - Elysian (217.8 s, all 9 classic devices including Thunder GSM): **0.2× real time** (1010 s). Heavy songs need the multithreaded track renderer.
  - Engage (Adultery / GM.DLS): not finished (machine panicked).

### WaveSabre: native macOS build (no Docker/Wine), measured 2026-10-25
Lab: `~/Downloads/64k-tools/synth-lab/wsmac/`. Built with clang and **no source edits** to WaveSabre:
- `inc/Windows.h`: a ~50-line pthread shim (events, threads, critical sections, Interlocked*, packed 18-byte `WAVEFORMATEX`).
- `GsmSample_mac.cpp`: MS GSM 6.10 decoded with libgsm (`GSM_OPT_WAV49`, 65-byte blocks = 320 samples), replacing ACM.
- `GmDls_mac.cpp`: reads `$GM_DLS`. It needs the Windows gm.dls because of the fixed `0x44602` offset.
- The harness includes `WaveSabrePlayerLib/SongRenderer.h` directly (the umbrella header pulls in DirectSound).

| Song (devices) | Length | 1 thread | 4 threads | 8 threads | vs Win32/Wine render |
|---|---|---|---|---|---|
| Global Groove (Slaughter) | 8.1 s | 641× | | | 2 samples differ by 1 LSB (SNR 111 dB) |
| Elysian (9 classic incl. Thunder GSM) | 217.8 s | 7.7× (28 s) | 13.7× | 18.0× | per-4 s loudness equal to 0.1 dB; SNR 15–85 dB per window (phase drift, see below) |
| Engage (incl. Adultery / GM.DLS) | 217.9 s | 5.0× | | | — |
| eladamri (incl. Specimen GSM) | 191.2 s | 20.1× | | | — |

Wine's 0.2× was a Wine artefact: the same song runs at 7.7× natively.

Findings for our port:
- **Non-deterministic multithreaded output.** `Helpers::RandFloat` uses one global, unsynchronized seed, read by
  Slaughter/Falcon note-on phases and Slaughter noise. With N > 1 render threads, output differs run to run (a data race).
  Single-threaded output is repeatable. Our port must give each device or voice its own RNG.
- **Cross-platform phase drift.** Same loudness, different phases on long sustained oscillators. Likely cause: libm
  `exp`/`pow`/`sin` differences (macOS libm vs Wine msvcrt), accumulated in double phase accumulators. Our Rust port
  should use one fixed math implementation (`libm` crate) on every target.
- Code size, `-Os` x86_64 `__text`, whole core + SongRenderer: **≈36.8 KB**. Largest: Slaughter 3.7 KB,
  Specimen 2.9, Adultery 2.9, Falcon 2.9, SongRenderer 2.8, Twister 2.3, SynthDevice 2.1, Cathedral 1.8, Helpers 1.7.
  Every device is a separate unit, so only used devices link.

## Tunefish 4 + WaveSabre: both in Rust, what is shared (analysis 2026-10-25)
Plan: port both to Rust `no_std`. WaveSabre is MIT, so a direct port is fine if we keep its notice. Tunefish is GPL-3, so we
write our own version (see add-tf-synth). Compared from source plus `tunefish4-functional-spec.md`.

### Same algorithm in both: one implementation
| Primitive | WaveSabre | Tunefish | Shared form |
|---|---|---|---|
| Freeverb comb (lowpass feedback) | `Comb` (Cathedral) | reverb combs | identical; `Comb{fb, damp}` |
| Freeverb allpass `out=buf−in; buf=in+0.5·buf` | `AllPass` (fb set once to 0.5) | reverb allpasses | identical |
| Freeverb reverb | Cathedral: spread +23 on R combs **and** allpasses, gain 0.015, width wet1/wet2, + SVF cuts, pre-delay, freeze | spread only on R allpasses, gain 0.015, wet×3, dry×2 | one `Freeverb` + config (spread mask, wet/dry law); device adds the rest |
| Integer ring delay | `DelayBuffer` (Echo, Chamber, Smasher lookahead, pre-delay) | Delay, Chorus taps, Flanger 4096 ring | one `Ring` (read/write/clamp-resize) |
| Fractional delay read | `ResampleBuffer` (Twister) | — (TF does not interpolate) | `Ring::read_frac` |
| RBJ biquad, Direct Form I | `BiquadFilter` LP/HP/Peak (Leveller) | HP/BP (log₁₀2 bandwidth quirk) | one DF1 core + separate coefficient designers |
| Note→Hz, dB, clamp, mix, exp2 | `Helpers` | same formulas | `math` on `libm` |
| Denormal guard | `MxcsrFlagGuard` (FTZ/DAZ) | SSE flush mode | one guard (x86 MXCSR, AArch64 FPCR.FZ) |

### Same job, different behaviour: one framework, behaviour as a policy
- **Voice pool:** WaveSabre drops the note when no voice is free, and has unison (detune/pan spread), mono-legato and
  slide. Tunefish: polyphony limit 1–16 and steals the oldest voice (it has unison inside the generator, not the pool). One
  `VoicePool<V>` with `Steal::{None, Oldest}` and `Mode::{Poly, MonoLegato}`.
- **Event timing:** WaveSabre is sample-accurate (`DeltaSamples` inside `Run`). Tunefish fires at the start of a 512-sample
  block. One event queue with sample offsets; a Tunefish instrument rounds offsets to its block.
- **Device trait:** WaveSabre `Run(inputs, outputs, n)` for any n. A Tunefish instrument implements it by buffering its
  512-sample blocks internally. The 10 Tunefish effect slots stay inside the instrument, because they share one parameter set.
- **Song layer:** WaveSabre's model (tracks, device chains, sends/sidechain receives, byte automation, track dependency
  order) is a superset of Tunefish's 32 instruments → master. Use it as the shared mixer: a `.tfm` song converts into it,
  and one song can mix Tunefish and WaveSabre instruments.
- **Memory:** WaveSabre uses `new` everywhere. Both use the caller-pool design from add-tf-synth D2.
- **RNG:** both need one per instance (fixes the WaveSabre thread race). Each engine keeps its own generator law for parity.
  WaveSabre's is `seed *= 0x15a4e35; (seed % 255)/255` on a signed int. After overflow the remainder goes negative, so
  the real range is about [−1, 1], not [0, 1). Reproduce that.

### Different algorithms: stay engine-specific
- **Oscillators:** Tunefish uses spectral IFFT wavetables. WaveSabre has Slaughter (integrated BLIT saw/pulse, sin(x)/x via
  FastSin, leaky integrator 0.998) and Falcon (2-op phase modulation with feedback, `sin + Square35·waveform`).
- **Envelopes:** WaveSabre is per-sample, timed in ms, with quadratic decay/release. Tunefish runs once per block, with
  log₁₀ steps and a slope curve. Different curves, no shared code.
- **Filters:** WaveSabre uses a Chamberlin SVF (2× oversampled, coefficients from FastSin). Tunefish uses a Moog ladder
  and a resonator notch. Only the biquad is common.
- **Effects:** Scissor (shapers + 2×/4× oversampling) vs Tunefish's power-law distortion; Leveller (biquad chain) vs a
  3-band one-pole EQ; Echo (cross-feedback, filtered) vs Tunefish delay (dry boosted by 1+k); Twister (phaser/flanger with
  a fractional read) vs Tunefish flanger and chorus. Chamber, Crusher, Smasher: WaveSabre only. Formant: Tunefish only.
- **Samplers** (Specimen, Thunder: GSM 6.10; Adultery: GM.DLS): WaveSabre only. They need a Rust GSM 6.10 (WAV49)
  decoder, a port of permissively licensed libgsm, plus `crates/dls`.
- **Parity table:** WaveSabre's `FastSin` (a linear-interpolated sine table read through double mantissa bits) shapes its
  oscillators and filter coefficients, so it is a WaveSabre-specific table, kept separate from the shared `math`.

### How much is shared (code size, `-Os` x86_64)
- WaveSabre's shareable units are Helpers, Biquad, Comb, AllPass, DelayBuffer, ResampleBuffer, SynthDevice, Device,
  SongRenderer and the guard: ≈11 KB of 36.8 KB (≈30%).
- Tunefish's shareable part (reverb core, delay ring, biquad, math, voice pool) is ≈3–4 KB of 17.7 KB (≈20%).
- An intro that links both engines saves ≈3–5 KB.
- The bigger gains: one tested implementation, deterministic output on every target, and one song/mixer format.

### Decision (2026-10-25): combine via shared core, options 1 + 2
- (1) One song/mixer for all engines; (2) every effect usable on any track. The merged instrument (3) is deferred.
- OpenSpec: `add-wavesabre-rs` (first) adds `crates/synth-core` (capabilities `synth-core`, `synth-mix`) and
  `crates/wavesabre` (`ws-synth`). `add-tf-synth` is rebased onto it: Tunefish instrument = kind 32 with 512-frame event
  lookahead; its standalone effects = kinds 33–39.

## WaveSabre Rust port: parity and speed (measured 2026-10-25, change add-wavesabre-rs)

Setup: `crates/wavesabre` + `synth-core` (`examples/render`, release) vs upstream WaveSabre `9245fb8` built natively
on macOS (lab `wsmac/render_ws`, `WS_STEP=100`), both one thread, 100-frame steps, Windows `gm.dls`. i9-9900K.

**Reference renders must use upstream's 100-frame player step.** The first harness rendered in 4096-frame chunks and
Global Groove looked like 0.9 dB SNR. All voices of a synth draw from one random source and render span by span, so
the span length decides which voice gets which random number whenever two voices overlap. At 100 frames Groove is
bit-identical.

### Device parity (`tools/ws-parity`, synthetic songs from `crates/wavesabre/tests/songs.rs`)

12 of 16 songs bit-identical: Falcon, Slaughter (automation, mono legato), Thunder, Specimen (loop), Scissor, Crusher,
Echo, Chamber, Twister flanger, Adultery (repeat loop; ping-pong + reverse + manual bounds). The rest: Leveller
100.8 dB, Smasher (sidechain) 105.4 dB, Twister phaser 106.2 dB, Cathedral 118.5 dB. Threshold 60 dB.

### Song parity (`tools/synth-parity`: BS.1770 loudness ±1 dB, octave LTAS 63 Hz–16 kHz ±3 dB, onsets ±100 frames)

| Song | Length | Sample SNR | Loudness ours / upstream | Max octave diff | Onsets matched | Result |
|---|---|---|---|---|---|---|
| Global Groove | 8.1 s | identical | −26.06 / −26.06 LUFS | 0.00 dB | 32/32 | pass |
| Elysian | 217.8 s | 53.1 dB | −12.06 / −12.06 LUFS | 0.00 dB | 153/153 | pass |
| Engage | 217.9 s | 80.3 dB | −11.93 / −11.93 LUFS | 0.00 dB | 79/79 (1 extra) | pass |
| Eladamri | 191.2 s | 72.7 dB | −15.65 / −15.65 LUFS | 0.00 dB | 262/263 | pass |

The sample-level gaps are tiny rounding differences, not audible ones: Elysian is 77–94 dB per 8 s window with a few
39–54 dB windows, Eladamri a steady 72 dB, while every per-device render is ≥ 100 dB. Long double-precision phase and
feedback paths amplify last-bit differences (libm `pow`/`sin` vs Apple libm, f32 expression order), which the
perceptual metrics confirm are inaudible.

### Speed (release, one thread)

| Song | Rust port | Upstream native |
|---|---|---|
| Elysian 217.8 s | 37.3–38.7 s (5.6–5.8×) | 30.7–31.3 s (7.0–7.1×) |
| Engage 217.9 s | 46.1 s (4.7×) | 42.0 s (5.2×) |
| Eladamri 191.2 s | 13.0 s (14.7×) | 10.0 s (19.1×) |
| Global Groove 8.1 s | 0.02 s (384×) | 0.01 s |

Spec bar (Elysian ≥ 5×, ≤ 43.6 s) met; the port is about 25 % slower than upstream's C++ without any tuning.

### Size (`research/ws-size-probe`, i686 `no_std`, opt-level z, LTO, static base, measured 2026-10-25)

| Build | Raw | xz -9e | squishy 0.1.3 |
|---|---|---|---|
| `ws-all` (13 devices, song `showcase`) | 71,168 B | 34,472 B | 27,648 B |
| Slaughter only | 28,672 B | 15,968 B | 14,336 B |

Both include the song mixer, decoder, voice pool, the song blob (`ws-all` also the 1 KB GSM test sample) and pure-Rust
`libm` (`pow`, `fmod`, `sin`, …), which upstream gets from the CRT DLL. No `core::fmt` is linked. Slaughter-only
`.text` is 25.4 KB, almost all inlined into `_start`. Upstream's x86_64 `-Os` `__text` (36.8 KB all, about 7.5 KB
Slaughter + SynthDevice + Helpers) excludes CRT maths and is not directly comparable. Shrinking ideas, unmeasured: f32
`pow`/`exp2` approximations instead of libm f64, fewer f64 paths where parity tolerates it. Not run on i686 yet.

## Tunefish-compatible `tfsynth`: reference parity and speed (measured 2026-10-25, change add-tf-synth)

Setup: `crates/tfsynth` (`examples/render`, workspace release profile = `opt-level = "z"`) vs upstream Tunefish 4
`7e48ce8` built by `tools/tf-parity/build-ref.sh` with our MIT harness `render-ref.cpp` (512-frame blocks, 44.1 kHz,
player mix ×11 · ×200). Metrics: `tools/synth-parity` (BS.1770 loudness ±1 dB, octave LTAS ±3 dB, onsets ≤ 1 block).
The reference seeds its random source from system entropy, so it differs from itself between runs; "ref vs ref"
is two reference renders.

| Song | Comparison | Loudness diff | Octave bands 63 Hz–16 kHz (dB) | Onsets matched | Result |
|---|---|---|---|---|---|
| no_remorse (201.6 s, 16 instr.) | ours vs ref | −0.13 dB | −0.33 … +0.31 | 340/346, max 64 frames | pass |
| | ref vs ref | −0.04 dB | −0.14 … +0.02 | 337/346, max 64 frames | — |
| tf4testsong (58.1 s, 9 instr., one empty) | ours vs ref | −0.17 dB | −0.60 … +0.36 | 1/1 | pass |
| | ref vs ref | +0.03 dB | −0.62 … +0.22 | 1/1 | — |

Passed on the first run with no code changes; our differences are of the same order as the reference's own
run-to-run spread. Peaks: ours 11 089 / 6 750 vs 10 776 / 6 709 for the reference (earlier measurement).

**Speed** (release, one thread, i9-9900K): no_remorse 12.9× real time (reference −O2 C++: 12×), tf4testsong 28.6×.
The first build reached only 7.1×: profiling showed the 4-pole ladder dominating because decaying filter state
went subnormal. The reference enables flush-to-zero for every instrument render (spec §1); adding
`synth_core::denormal::Guard` to `Instrument::render` brought it to 12.9× (and changed one golden, the EQ, where
subnormals had occurred). Hoisting per-partial values out of the spectrum loop (identical output) removed the rest of
the `pow` cost.

**Spec correction found while implementing:** with FlangLfo = 0 the flanger's right channel is not static — both
channels' running delay term uses f, so L and R sweep together with a 480 s period (functional spec §4.5/§8.3
corrected).

**Size** (`research/tf-size-probe`, i686 `no_std`, opt-level z, LTO; engine + `.tfm` player + 13-instrument song blob):

| Build | Raw | xz -9e | squishy 0.1.3 |
|---|---|---|---|
| all effect kinds (`tf-fx-all`) | 41,472 B | 21,668 B | 18,944 B |
| no effects | 30,208 B | 15,800 B | 14,848 B |

`.text` 34.8 KB, `.rdata` 5.0 KB (song, formant coefficients, constants); no `core::fmt`. The 7 effects cost about
4 KB packed. Reference code size for comparison: x86_64 `-Os` `__text` ≈ 17.7 KB without player or libm.

## Unified synth `unisynth`: parity, speed and size (measured 2026-10-26, change add-unified-synth)

Setup: `tools/unisynth-parity` (`report.md` holds the full tables). Every legacy device test song (16 WaveSabre songs, 27 Tunefish
synthetic patches) and the six reference songs are rendered by the legacy crate and, converted by `synth-convert`, by
`unisynth`; metrics from `tools/synth-parity`: loudness ±1 dB, octave bands ±3 dB (bands more than 24 dB below the loudest
are skipped), onset lag within 100 / 512 frames with a curve correlation of at least 0.5. 49 of 49 pass; the retirement
verdict stays BLOCKED until the A/B listening is signed off.

| Reference song | Loudness diff | Worst band diff | Onset lag |
|---|---|---|---|
| Global Groove | −0.00 dB (8 seeds) | −2.3 dB | 0 |
| Elysian | +0.05 dB | +0.25 dB | 0 |
| Engage | +0.04 dB | +0.06 dB | 0 |
| Eladamri | −0.03 dB | −0.38 dB | 0 |
| no_remorse | −0.21 dB | −1.74 dB | 64 frames |
| tf4testsong | +0.04 dB | −1.30 dB | 0 |

What it took (task 7.2): WaveSabre's noise is one signal on both channels (new `noise.width` parameter, 3 dB in a mono
sum), 2 ms envelopes need an 8-frame control grid for 256 frames after a note edge (1.75 dB on Global Groove), Tunefish
releases need freeing when inaudible (33 voices against the legacy engine's 19), silent reverbs sleep, and the mod matrix
is scanned once per segment instead of once per target. The in-process parity tests of `crates/synth-convert` keep their
24 dB band cut; the parity tool uses the same.

**Speed** (one thread, real time = audio seconds per render second; speed build = release at opt-level 3):

| Song | Legacy (speed build) | Unified (speed build) | Legacy (size build) | Unified (size build) |
|---|---|---|---|---|
| Elysian 217.8 s | 7.1× | 9.3× | 5.2× | 6.4× |
| no_remorse 205.6 s | 15.4× | 10.9× | 14.2× | 8.5× |

Spec bars (Elysian ≥ 5×, no_remorse ≥ 10×): met on the speed build; on the size-optimised `release` profile no_remorse
reaches 8.5×. The unified synth is more generic (iterators, slots) and relies on the optimiser more. Profile of no_remorse at
the start: wavetable layers 22 %, filters 15 %, reverbs 12 %, control ticks 20 %.

**Size** (i686 `no_std`, opt-level z, LTO; `research/us-size-probe`): all modules 104,448 raw / 51,080 xz / 39,424 squishy
(song of 3.4 KB included), against 71,168 / 34,472 / 27,648 for WaveSabre with every device plus 41,472 raw for Tunefish with
every effect. Per-song builds with exactly the listed features, song bytes included: Global Groove 49,152 / 21,504, no_remorse
121,344 / 29,696 (a 57.5 KB song), tf4testsong 75,776 / 28,160, Eladamri 101,888 / 35,328, Elysian 134,144 / 49,664,
Engage 162,304 / 42,496 (raw / squishy). Not run on i686 yet (macOS Wine is 64-bit only).

### Unified against legacy: packed size under six compile configurations (2026-10-26)

`research/size-compare/compare.sh`, full tables in `tools/unisynth-parity/size-compare.md`. For each reference song the legacy engine
(only the devices that song uses, playing the original `.bin`/`.tfm`) and the unified synth (only its `synth-convert features`, playing the converted
song) are built as i686 `no_std` probes and packed with squishy 0.1.3. Squishy bytes, legacy / unified, configuration `bz` (core rebuilt with
`optimize_for_size`, opt-level z, `panic=immediate-abort`: the compo build):

| song | legacy | unified | unified vs legacy |
|---|---:|---:|---:|
| Global Groove | 14,336 | 16,384 | +14 % |
| Elysian | 43,520 | 45,568 | +5 % |
| Engage | 36,352 | 38,400 | +6 % |
| Eladamri | 30,208 | 31,232 | +3 % |
| no_remorse | 17,920 | 23,552 | +31 % |
| tf4testsong | 17,408 | 22,016 | +26 % |

(After `shrink-unified-synth` steps 1 and 4.1: module arms guarded by `cfg!(feature)` (−0.5 to −2 KB) and `libm` replaced by `unisynth::fm`
(11.8 KB → 0.3 KB of code, −3 to −4.5 KB packed). Before: Global Groove 20,992, Elysian 49,152, Engage 42,496, Eladamri 34,816, no_remorse 29,184,
tf4testsong 27,648. The legacy engines still carry their ~10 KB of `libm`.)

- **Compile configuration matters little for the ratio, a lot for the absolute size.** Opt-level z is smallest in all cases; s costs 0–5 %, opt-level 3
  costs 7–29 % for the legacy engines and 9–17 % for the unified synth. Rebuilding core with `optimize_for_size` and immediate-abort saves 0.5–1 KB (about 1–3 %). `-Zmerge-functions=aliases`
  and `-Zinline-mir=no` change nothing. pentium3 and x87-only builds do not compile on i686 (core needs `sse2`).
- **The gap is code, not song.** The converted songs are 0–2 KB larger packed (no_remorse 3.3 KB against 1.3 KB xz, tf4testsong 1.5 KB against 0.7 KB;
  WaveSabre songs within 1 KB), the rest is the engine: roughly 25 KB for the unified synth with the WaveSabre feature set against 20 KB for the legacy
  engine on Elysian; for Tunefish songs about 26 KB against 17 KB (wavetable, both ladder filters and RBJ, both envelope families and the shared matrix
  are generic; the Tunefish port is purpose-built).
- **The smallest case shows the fixed cost.** Global Groove (Slaughter only, one filter) is +6.7 KB: oscillator, matrix, voice framework and the
  generic parameter and chunk code are linked even for a patch that uses a fraction.
- **A unified build holding both families** (the all-modules probe) is 39,424 B squishy, against 27,648 for WaveSabre with every device plus
  about 19 KB for Tunefish: one engine instead of two is smaller than the two together, but not smaller than either alone.

**Where the unified bytes are** (`research/us-size-probe/attr.sh`, no LTO and no inlining, so ranks not totals; no_remorse, 64 KB code): `unisynth` 26.8 KB
(`Voice` 9.5, `UInstrument` 1.7, wavetable 1.5, matrix 1.4, reverb 1.3), `libm` 11.8 KB before `fm.rs` (`rem_pio2` 3.3, `pow` 2.1, `powf` 1.7; the legacy Tunefish probe has 10.2), 0.3 KB after,
`synth_core` 10.7 KB (mix player/song/mixer 7.9: legacy `.tfm` plays without them), trait impls 9.8 KB. Module cost (`marginal.sh`, raw/xz KB, all modules against one removed):
GSM 6.1/2.8, Leveller 5.1/1.5, Chamber 4.6/1.4, wavetable 4.1/2.2, Echo 3.6/1.1, chorus 3.1/1.3, Twister 3.1/1.0, Cathedral 3.1/0.6, DLS 2.6/1.4. Next steps: change `shrink-unified-synth`.

