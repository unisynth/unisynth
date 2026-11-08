# packages/daw/src/ui/device

Device panel (mounted only while the Device tab shows, design D5): chain, instrument blocks, effects, sampler, live per-voice view.

| File | Purpose |
|---|---|
| `Device.tsx` | `mountDevice(el, DeviceActions)` (daw-solid-ts 6.2): selected track's chain only while `deviceShown()`; `Chain`, `Instrument`, `FxBox`, `PGroup`, `PCtl`, slot screens + live overlays; osc sampler block = `Sampler`. Exports `SLOTS`. → see `Device.tsx.AGENTS.md`. |
| `Sampler.tsx` | `Sampler` (daw-solid-ts 6.3): osc slot sampler block; source select (`osc<i>.s7`: GSM / GM instrument / GM wave), `smpWhat` line, GSM buttons (Play `Play GSM sample N`, Choose, Import WAV, `Remove unused (n)` from `gsmUnused`) or Choose GM + bank note; `Settings` for a chosen sample: loop / region (loop on) / interp selects + reverse checkbox (one undo each), canvas waveform drawn by an effect from `curVal` (D4), drag start / loop edges / loop body (`put` per step, `dragEnd` = one step incl. making the loop custom), `svg.smplive[data-live=smp<i>]` for the live view. `SamplerActions`. 9.4: waveform `Path2D` cached per decoded PCM (`peaksOf`, WeakMap); the GM-bank note is an HTML `.smpnote` overlay, not canvas text (setting `ctx.font` cost ~10 ms per mount). |
| `live.ts` | Live per-voice overlays (daw-solid-ts 6.4, D4; add-device-live-view D7): `initLive(LiveHost)`, `liveProbe(p)` (each status: parse, voice colours, filter spectra with peak hold), `liveClear`, `markLiveDirty`, `registerLive(el, "<g><i>")` (screens register while mounted, no DOM queries), `chips` signal (header voice chips), `liveStats` (`DAW.debug.live`). Draws `g.live` SVG strings per frame only when dirty; frame loop runs only while `deviceShown()`. |
