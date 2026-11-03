# packages/daw/src/ui/footer

Footer: audition (armed track, computer keyboard, MIDI, mod wheel, pitch bend).

| File | Purpose |
|---|---|
| `Audition.tsx` | `mountAudition(slot, AuditionActions)` (daw-solid-ts 8.2): footer `#armed-info` (armed name, `C{kbOct-1}`, vel), `#kb-btn`, `#midi-info`, `.wheels` `#mw`/`#pb` from `state/audition.ts`; sliders use native `on:input`/`on:pointerup`/`on:keyup` (non-bubbling events count); PB springs back to 0. |
