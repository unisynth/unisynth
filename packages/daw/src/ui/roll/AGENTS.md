# packages/daw/src/ui/roll

Piano-roll panel (mounted only while shown).

| File | Purpose |
|---|---|
| `Roll.tsx` | `mountRoll(panel, RollActions)` (daw-solid-ts 7.2, D4/D5): `#p-roll` content (note toolbar, audition keys `KEY_HI`..`KEY_LO`, `#grid`/`#gridc`, `#vel-lbl`, `#vel`/`#velc`) only while `rollShown()`. `onMount` → `attach(RollEls)`, cleanup → `detach()`: ui/app/roll.ts binds its handlers and paints the visible window (canvases stay imperative). |
