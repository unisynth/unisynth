# unisynth.md (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## Scope
`crates/unisynth` + `crates/synth-convert` (changes add-unified-synth, add-v2-import, add-cauldron-xm-import, add-plain-xm-import, unify-v2-algorithms). Source of truth = crates; page ≈ 120 KB, grows per task group.

## Sections (current)
- Where it sits; instrument slot diagram (kind 64 instrument, kinds 65–73 effects).
- Parameters: v3 layout, 294 values in module blocks + add-tf3-import point fields 294–320 (321 total; osc type expanded `Choice(25)`, id = position) (indices < 240 one chunk byte, rest two, `us-chunk-wide`); laws table. Pre-v3 layout (260 values, chunk versions 1/2) frozen host-side as `params::old`; player rejects it.
- Chunk v3 (v4 adds speech text) format + validation flow; migration from the old layout (`unisynth::migrate`, host only).
- Voice framework: events → 32-frame control grid, fine window + inaudible releases, voice control segments, envelopes, LFOs. Mod matrix (evaluation, target table).
- Oscillator slots: phase-mod operator (`us-osc-pm`), band-limited saw/pulse (`us-osc-blit`), spectral wavetable (`us-osc-wt`), sampler (`us-osc-sample`), clocked (`us-osc-clocked`), controllers wheel/bend/bend range, DLS instruments (`us-sample-dls`), stereo DLS pairs (`us-sample-stereo`).
- Noise (`us-noise`), filter slots, open filters not run.
- Mono-sum voice, drive, channel strip, speech, provenance (V2 modules, `us-v2`).
- Module features, song map, songs as Rust (`synth-convert rust`), elementary functions (`fm.rs`), goldens, editor probe (`probe`, host only), memory.
- Effects (kinds 65–73, modes).
- Converter: V2 songs (`.v2m`), mini-cauldron XMs, generator maps for plain XMs (`xm-gen/1`, the note byte contract, replay additions; add-plain-xm-import), WaveSabre instruments, Tunefish instrument + `.tfm` songs, walkthrough (CLI examples), feature list (`features`), demo songs (`tools/demo-songs/build.ts`).
- Parity, speed and size (`tools/unisynth-parity`).
