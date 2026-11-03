# packages/daw/src/ui/files

Song list and file commands panel.

| File | Purpose |
|---|---|
| `Files.tsx` | `mountFiles(section, actions)` (daw-solid-ts 4.2): songs section (`#songlist` from `files.songs`, current `aria-current` + `trackCount()` tr, '• ' when dirty; `#imp-btn`/`#imp-file`, `#new-btn`). `FilesActions` open, importFile, newSong, trackCount. |
| `dialogs.tsx` | File dialogs on `openDialog` (daw-solid-ts 4.2): `confirmSave(file, why, save, go)`, `showErrors(errs, what, pick)`, `notice`, `exportDialog(facts, write, download)` (table needs explicit `<tbody>`: parser-inserted tbody breaks Solid's template walk), `bounceDialog(ranges, start)` (range radios, progress, result link, Cancel stops a render else closes; `start` → `BounceJob` or null). |
