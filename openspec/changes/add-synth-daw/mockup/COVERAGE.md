# Mockup coverage of the add-synth-daw proposal

Every requirement in `proposal.md` and `specs/*/spec.md` mapped to the mockup (`index.html`). Each row was checked in
headless Chrome: a probe clicks or presses every control and confirms that something changes, and feature tests check
the results. Run the mockup with `serve_mockup` on this directory; `?tab=roll|dev|mix` opens a panel, and `?nobank`
simulates a missing GM bank.

The mockup also runs in a sandboxed frame with origin `null`, such as the dashboard canvas. The library loads from
`catalog.js` (same data as `catalog.json`; `gen-catalog.py` writes both). Splitter sizes fall back to memory when
`localStorage` is blocked.

| Status | Meaning |
|---|---|
| ✅ | Works in the mockup |
| ⚙️ | Backend or engine only; the mockup shows it through a labelled stand-in |

## synth-daw (browser UI)

| Requirement | | How to try it in the mockup |
|---|---|---|
| Serving and file access | ⚙️ | The song list stands in for `/api/music`; songs are kept in memory |
| Read-only library and bank routes | ⚙️ | `catalog.json` (from `gen-catalog.py`) stands in for `/api/library`; `?nobank` simulates a 404 from `/api/bank` |
| Open, save and export | ✅ | Song list (demo, intro-theme, outro), New song, Import blob…, Save / Ctrl+S, Export. "•" marks unsaved changes in the title, list and footer; switching songs or leaving asks first; validation errors list JSON paths and block saving |
| Transport bar | ✅ | Play/stop, loop, position and time, tempo, time signature (beat grid and positions), follow, snap, engine load |
| Loop editing; Song length; Playback display | ✅ | Ruler strip, Loop field, L, Ctrl+L; Length field; playhead, meters, follow |
| Zoom; Zoom to fit; No text selection; Follow and manual scrolling | ✅ | Pinch, Ctrl/⌘+wheel, ± keys, ⤢; follow pauses while you scroll |
| Arrangement view | ✅ | + / track menu (right-click or Shift+F10): add, rename (F2 / double-click), duplicate, move (Alt+↑↓), delete (Ctrl+Delete). Double-click a lane to create a clip; clip menu: open, duplicate (linked), make unique, loop. Alt+drag a clip edge loops it (⟳); a plain drag crops |
| Clip selection; Selecting from track headers; Clip editing; No overlapping clips; Align | ✅ | |
| Knob values persist; Knobs follow automation | ✅ | Automation values are display state and never mark the song as changed |
| Automation lanes; point editing; removal and keyboard | ✅ | Every point edit is one undo step |
| Track selection | ✅ | Header, clip, strip name (master included) |
| Piano roll; Clip notes and roll length; thumbnails; note selection / editing / quantize / scrolling | ✅ | Clicking a key auditions it; a looped clip shows its pattern ("⟳ 4-bar loop ×1.5") |
| Clipboard and undo | ✅ | |
| Velocity editing; nudge and field; Mod wheel and pitch bend lanes | ✅ | |
| Audition controllers | ✅ | MW adds vibrato and PB bends the held audition notes; Web MIDI CC1 and pitch bend move the sliders |
| Device panel; Slot racks; Slot remove and move | ✅ | Type and mode selects are stored in the patch; "+ Mod slot" and ✕ per row |
| Device chain and presets | ✅ | Browse, Save preset (adds it to Project presets), ⏻ bypass for the instrument and each effect, ◀ ▶ reorder (automation lanes follow), ✕ remove (its lanes go with it) |
| Instrument library; Loading from the library | ✅ | Licence note per source; Audition plays C3; each Load is one undo step |
| GM sampler editor | ✅ | Note that the OS bank differs per platform; with `?nobank`, a warning that the bank is missing |
| Mixer; Insert effects; Sends and bus tracks | ✅ | Faders synced with the headers; mute and solo (a soloed bus keeps its sources); insert bypass; + send (targets exclude tracks that already feed this one), level, ✕; + Bus |
| Computer keyboard and MIDI audition | ✅ | With a track armed (●): A W S E D F T G Y H U J K O L P ; ' play notes, Z/X octave, C/V velocity, M or ⌨ switches it off (letters become shortcuts again); "MIDI: connect" in the footer |
| Undo and redo | ✅ | Whole-song snapshots: notes, clips, tracks, devices, knobs, automation, mixer, loads |
| Offline bounce | ✅ | Whole song or loop, progress, Cancel, WAV download (the mockup renders its preview voices) |
| Accessibility and layout | ✅ | Keyboard everywhere, menus with arrow keys, dialogs; splitters remember their size |

## song-source (format, export/import, CLI)

| Requirement | | Stand-in in the mockup |
|---|---|---|
| Song source file; Devices, clips, placements, lanes | ⚙️ | In-memory model; Export → "Download song.json" shows it |
| Validation with paths | ✅ | Empty or duplicate track names, missing send targets, duplicate lanes → `$.tracks[i]…` errors |
| Export to the song blob; Time and value conversion | ✅ | Export dialog: size, devices, note events (linked and looped clips flattened, muted tracks silent), 256-step automation, length at the tempo |
| Buses and master effects; Master is the end of the graph | ✅ | |
| Clip controller envelopes | ✅ | |
| Import from the song blob | ✅ | Import blob… creates a song with one clip per lane (stand-in content; the DAW runs `song import`) |
| Patch presets | ✅ | Save preset → `project/music/patches/<name>.patch.json` |
| Song command-line tool | ⚙️ | CLI only |

## synth-live-host (engine)

| Requirement | | Stand-in in the mockup |
|---|---|---|
| Same sound as the player; Render in fixed-size calls | ⚙️ | WebAudio preview voices, clearly labelled as not the engine |
| Edits while playing; Transport; Meters and load | ✅ | |
| Audition | ✅ | Computer keyboard, roll keys, library, MIDI |
| Parameter tables | ⚙️ | Knob tables in the mockup |
