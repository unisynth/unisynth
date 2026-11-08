# packages/daw/src/ui/arrange

Arrangement components: track headers and lanes (`Tracks.tsx`, windowed rows), clips, thumbnails, automation lane point selection (`lanePlot.ts`).

| File | Purpose |
|---|---|
| `Clip.tsx` | `Clip` (daw-solid-ts 5.3): `.clip` with reactive left/width (`bw`), `linked`/`sel`, label, ARIA label; `_clip` on the element for the lane pointer handlers (ui/app/clips.ts); `<canvas class="notes">` thumbnail drawn from `thumb()` when `laneVer[t.id]`, len, loop, name or zoom change (deferred while scrolling); `PatternOf` reads the pattern without creating it. |
| `Tracks.tsx` | `mountTracks(hdrs, lanes, scroller, tracks, actions)` (daw-solid-ts 5.2): windowed track header + lane rows with `.gap` spacers, `Clip`s culled to `barWindow`, automation groups, queued row fill. `TrackActions`. → see `Tracks.tsx.AGENTS.md`. |
| `lanePlot.test.ts` | Unit: sparse lane keeps every point; dense keeps extremes and ends; window bounds handles, line one neighbour beyond; `laneLine` thinning. |
| `lanePlot.ts` | Which automation points a lane draws (daw-solid-ts 9.5): `laneLine(pts, x)` (whole line, first/lowest/highest/last per pixel column), `lanePlot(pts, w0, w1, x, handlePx, sel)` → `{ line, handles }` (handles: lowest + highest per `handlePx` column, bars in `[w0, w1]` only, `sel` always kept). Pure; points sorted by bar. |
| `thumb.test.ts` | Unit: note rects (x, y over pitch range, min width), loop repeats + marks, cropped clip, empty. |
| `thumb.ts` | `thumb(notes, {len, loop, patLen, w, h})` → note rects + loop marks for one canvas per clip (pure). |
