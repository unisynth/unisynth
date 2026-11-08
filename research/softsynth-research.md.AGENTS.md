# softsynth-research.md (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## Sections
- Survey: Tunefish4 (GPL-3), WaveSabre/64klang/Sointu/4klang (MIT), Oidos (zlib), V2 (Artistic). Takeaways for 64k.
- WaveSabre vs 64klang source comparison: block float devices + track/automation layer vs per-sample `__m128d` node graph; common ground; merge verdict (shared no_std DSP prims, graph device, WaveSabre song layer, 64klang split-delta encoding).
- GM.DLS: runtime-read OS bank (Win `drivers\gm.dls`, macOS `CoreAudio.component/.../gs_instruments.dls` 495 waves 8-bit), license, free SF2 alternatives; `crates/dls` step 1; gm.dls vs gs_instruments.dls measured.
- Integration checks (measured 2026-10-25): Tunefish 4 (decision: own MIT reimplementation; reading source allowed), 64klang, WaveSabre (incl. native macOS build), sizes.
- TF4 + WaveSabre Rust share analysis: shared Freeverb comb/allpass, ring delay, DF1 biquad, math, voice pool, event queue, Device trait, WaveSabre song layer; engine-specific osc/env/filters/FX; decision: combine via shared core, options 1 + 2.
- WaveSabre Rust port (add-wavesabre-rs): 100-frame reference step rule; per-device table (12/16 identical, rest ≥ 100 dB); song metric table (4 songs pass); Elysian 5.7× vs upstream 7×; size probe (`ws-all` 71,168 / 34,472 / 27,648 B raw / xz / squishy; Slaughter 28,672 / 15,968 / 14,336).
- `tfsynth` (add-tf-synth): parity (no_remorse/tf4testsong pass, within the reference's own run-to-run spread), speed 12.9× (FTZ fix), size table (all effects 41,472 / 21,668 / 18,944 B; none 30,208 / 15,800 / 14,848), flanger spec correction.
- `unisynth` (add-unified-synth): 49/49 parity pass, reference-song table, speed table (Elysian 9.3× / no_remorse 10.9× speed build; 8.5× size build), size (all modules 104,448 raw / 39,424 squishy); packed size under six compile configurations (bz row, legacy vs unified per song, before/after shrink-unified-synth).
