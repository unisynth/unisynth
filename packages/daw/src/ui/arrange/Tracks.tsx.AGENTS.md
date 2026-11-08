# arrange/Tracks.tsx (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## API
- `mountTracks(hdrs, lanes, scroller, tracks, actions)`; `Track`, `TrackActions`.
- `TrackActions`: devLabel, about, rename, pattern, autoLane, addAutoLane, mute, solo, arm, volume (live / `done` = one undo step), click, key (native `on:keydown` on the header: target-phase order kept), menu, startRename. No document delegates.

## Behaviour
- Windowed rows in two columns (`#hdrs`, `#lanes > .rows`); `.gap` spacers keep offsets + scroll extent.
- Header (`.thdr.track`, M/S/arm/A, volume, name + `Rename` input, `err` + title from validation, device label on `labelRev`) and lane (`.lane.track`, `.sel`, opacity when muted, static bus note) are reactive on the song store + view state.
- Lanes render `Clip`s culled to `barWindow` (view-width pages ± 1), only while the row is in the view window (pinned-only rows stay empty); `laneEl` registered by the lane.
- Open automation group (`AutoGroup`): `mapArray(t.lanes)` → `autoLane(t, ln)` pairs (`ah` in `#hdrs`, `al` in `#lanes`) + '+ lane' row (`addAutoLane`); adding/removing a lane needs no rebuild; refilled on `rev` (undo reconciles lanes by position) or its track's `trackRev`.
- Rows entering on scroll fill / draw from a 6 ms/frame queue (`fillLater`, `drain`); loads, edits and undo fill at once.
- No `aria-posinset`: not allowed on `role=group`.

## History
- See change: daw-solid-ts (5.2, 5.4, 9.5, D6).
