# daw-research.md (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## Content
- §1 DAW parts: transport/PPQ, tracks, clips, piano roll vs pattern, devices, mixer, automation, RT engine, undo.
- §2 Ableton Session vs Arrangement; MOD/ProTracker/Renoise patterns + order list.
- §3 demoscene authoring: WaveSabre VST + converter, Tunefish/Renoise, Sointu own tracker + wasm.
- §4 AudioWorklet + wasm: 128-frame quantum, SAB needs COOP/COEP.
- §5 gaps: blob is player format (needs `song.json` source); `Player::seek` re-renders from 0 (needs live sequencer or pool snapshots).
- §6 options A plugin / B web DAW (recommended) / C native; §7 open questions.
- §8 (2026): alternatives to own wasm DAW: WebCLAP (openDAW host, wclap-bridge for native DAWs), DAWproject single importer. Recommendation: WCLAP build of `unisynth` + DAWproject → `song.json` import; `packages/daw` stays source of truth.
