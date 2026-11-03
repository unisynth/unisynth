# packages/daw/src/ui/library

Library browser (presets, GM instruments, samples).

| File | Purpose |
|---|---|
| `Library.tsx` | `mountLibrary(section, actions)`, `openLibrary(src)` (daw-solid-ts 4.2): `#lib-q`, `#lib-tree` groups (entries rendered only while open, ≤ `LIST_CAP`), `#lib-insp` inspector follows the selected track (`hasInst()`, `target()` reactive; `#lib-target`). `LibraryActions` hasInst, target, audition(src, it, done), load. |
