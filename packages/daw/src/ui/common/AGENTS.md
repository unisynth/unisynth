# packages/daw/src/ui/common

Shared components: knob, splitter, menus, dialogs.

| File | Purpose |
|---|---|
| `Dialog.tsx` | `openDialog(title, body, buttons)` → {el, close} (daw-solid-ts 4.2): `<dialog class="dlg">` mounted only while open, one at a time; button `run` returning false keeps it open; Escape closes (`cancel` handled); removed synchronously in `close()` (not on the async close event); native focus return to the opener. |
| `Knob.tsx` | `Knob` (daw-solid-ts 6.2): `role=slider`, value from an accessor (store / automation), `.trk`/`.arc`/`.ptr` paths coloured by style.css (block, `.mod`, `.auto`, `.ovr` rules win); keys ±0.01 (Shift 0.002)/Home/End, drag 160 px = range (Shift 800), double-click → `def`; one undo step per gesture via `begin()`/`end(before, msg)` (keys commit in a microtask); `aria` overrides the accessible name. |
| `Menu.tsx` | `popMenu(x, y, items, label, opener)` (daw-solid-ts 4.2): `role=menu` mounted only while open, one at a time; items `[label, hint, fn, disabled]` or `"-"`; first enabled item focused, ↑/↓ wrap over enabled, Escape / pointer down outside close, choose → close then run; focus back to opener; kept inside the window. Outside-pointerdown listener attached at once, not in a timer (Chrome runs input before timers). |
| `Splitter.tsx` | `Splitter` / `mountSplitter(slot, SplitterProps)` (daw-solid-ts 8.2): `.splitter` separator (`id`, `aria-orientation`, `aria-valuenow/min/max`) sizing a root CSS var via `state/layout.ts` `sizeOf`; drag (`at(e)`) or arrows (Shift 40 px), clamped; unset size = `measure()`. Mounted in a `.slot` (`display: contents`). |
