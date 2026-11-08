# src/engine.ts (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## API
- `Engine.create(base)` compiles `synth.wasm` (main-thread instance `x`).
- Song functions on the main-thread module: `tables`, `physical`, `validate`, `exportBlob`, `importBlob`, `presetSave`/`presetLoad`, `migrate(text)` → `{song}` | `{errors}` (`song_migrate`), `gsmDecode(chunk)` → Float32Array | null, `gsmEncode(pcm)` → chunk bytes.
- Audio: `start(rate)` builds the AudioWorklet node (`processorOptions.module`); `restart()` rebuilds the graph; `load`/`update` (replies decoded here); `play`/`stop`/`seek`/`setLoop`/`noteOn`/`noteOff`/`setParam`/`setBank`; `status` + `onStatus`.
- `latency` (default `"playback"`, ~23 ms) → `latencyHint`; `bufferSeconds` = granted `baseLatency`; `rate`.
- `bounce(blob, range, progress)` → `{done, cancel}`; `lastBounceWorkers`, `mt` (`MtOptions` test hooks).
- `watch(track)` (−1 none); `Status.probe` = watched instrument probe (Float32Array | null); `Status.watch` = track the probe belongs to.
- `drops()` → `{events, seconds}` from `playbackStats` of this context (null if unsupported).
- `runOpen` / `runOpenFilters(on)` → worklet `openflt`.
- `wav16(samples, rate)`: 16-bit stereo WAV, `clamp(trunc(fround(x·32767)))` (f32 product like the player) → equals `song render` bytes.

## Behaviour
- `gen` bumped by `play`/`stop`; echoed by the worklet in status.
- `dead` + `onError` on a worklet error (`t:"error"` or `onprocessorerror`); later messages from that node are ignored; `start()` on a dead engine restarts.
- `start`/`restart` share one in-flight start (`starting`): concurrent early key presses make one AudioContext; messages from a replaced node are ignored.
- `watched` and `runOpen` are re-sent on `start`; `watch()` clears the stored probe at once; a status whose `watch` differs from `watched` has its probe dropped.
- `bounce`: threaded via `bounce-mt.ts` when isolated + `synth-mt.wasm` compiles + workers ≥ 2; any failure → `bounceSingle` (same samples).

## History
- See change: add-wasm-threads (bounce), add-device-live-view (watch/probe), unify-v2-algorithms 6.5–6.8 (the format switch: `migrate`).
