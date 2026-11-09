# packages/daw/src/ui/transport

Transport bar (play, position, tempo, signature, loop, follow, undo buttons).

| File | Purpose |
|---|---|
| `TransportBar.tsx` | `mountTransport(header, actions)` (daw-solid-ts 3.3) replaces the `<header>` content. Position/time = memos of `live.pos` × tempo × signature (tempo change updates the time at once); load meter from `live.load`; loop field written unless focused; invalid loop/length/tempo input restored from the signal. `TransportActions` (play, stop, toggleRec/Loop/Follow, zoom, tempo, sig, snap, loopText → bool, length, buffer, openFilters, undo, redo). Bounce/Export/Save call `bounce`/`exportSong`/`save` actions; Save's `dirty` class + label from `ui/state/files.ts`. First: `#browse-btn` ☰ Songs (shown at phone width only, `aria-expanded` = layout `drawer`). |
