# device/Device.tsx (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## API
- `mountDevice(el, DeviceActions)` → dispose. Exports `SLOTS` (from `model/params.ts`) and the narrow `Param`/`Table`/`Device`/`Track` interfaces.
- `DeviceActions` extends `SamplerActions`; built in `ui/app/device.ts` `deviceActions()`.

## Behaviour
- Shows the selected track's chain only while `deviceShown()` (design D5); hidden tab = no DOM.
- Components: `Chain` (instrument, `For` effects, `+ Effect` picker), `Instrument` (Voice/Noise, slot `Block`s, `ModTable`), `FxBox` (param set memo on `values.mode`), `PGroup` (choices in header, knobs grid), `PCtl` (select / raw number / `Knob`: value `autoValue ?? vget`, `.auto` from the track's lanes, `.ovr`).
- A collapsed slot or block renders only its header; screens and the sampler are built lazily while open. Slot chrome: twisty/clear/fixed/routes. Screens get `data-live` + `registerLive`; a redraw marks the overlay dirty.
- `.vox` chips from `chips()` (`Index`).
- Screens: elements from `ui/app/screens.ts`; `createEffect(() => el._draw())` redraws from the store values they read. Osc sampler block = `Sampler`.
- `devName` wrapper tracks mode + preset.
- `OSC_SHAPES`: v3 osc kinds (naive 5–11: colour only; 12 clocked; 13 Points: p1–p4 offset/value + drive at s8; 14 PAD table: bandwidth, scale, damp, harmonics, profile, drive).
- Points slot (add-tf3-import 6.3): `PointsGrid` (`post` of `GroupOpts`, after the screen): a `.pt` column per point (`pointCount` of `osc<i>.points`: offset `s(2k−2)`, value `s(2k−1)`, select `mode<k>` "to here") and an "end" column (`mode_last`); knobs grid holds count, drive (s8), `layers`, `layer_detune`, `layer_spread` (`POINT_LABELS`). The osc `ps` memo follows the count. PAD slot: s0–s5 knobs; profile text from `fmtVal`.
- `PCtl` selects with more than `LAZY_OPTIONS` (16) options (mod source/target) hold only the current option until first pointerdown/focus/keydown (tests `focus()` before `selectOption`).

## History
- See change: daw-solid-ts (6.2, 9.4), unify-v2-algorithms 6.5–6.8 (the format switch), add-tf3-import 6.3 (Points / PAD blocks).
