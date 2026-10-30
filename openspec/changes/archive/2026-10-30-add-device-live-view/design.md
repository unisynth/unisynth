# Design

## Context

- The device panel is generated in `packages/daw/src/app.js` (`instrumentBox`, `pgroup`, `slotRack`). Racks list only
  used slots (`slotUsed`); remove compacts and move swaps through `remapSlots`, which also rewrites mod targets and
  automation lanes (add-synth-daw D8a). The song stores plain slot indices, so the fixed layout needs no format change.
- The engine runs in an AudioWorklet (`worklet.ts`) over `crates/synth-wasm` (`Host` holds each track's concrete
  `UDevice`). About every 30 ms the worklet posts `status` (position, peaks, load). Nothing below the track level is
  visible to the UI.
- `unisynth` voices keep their control state in `Ctl` (`env: [Env; 4]` with `value()`/`stage()`, `lfo: [Lfo; 2]`
  with a phase, the mod matrix) and sampler `smp::State` per slot; filters run per voice in series. LFOs are per voice.
- The reviewed mockup is `openspec/changes/archive/2026-10-30-add-synth-daw/mockup/device-fixed-slots.html`; its simulation (voice
  colours, beads, spectra) is the reference for the UI.

## Goals / Non-Goals

**Goals:**
- Fixed slot layout with collapse, in-place clear, filter-only swap; block colours and static screens.
- Live per-voice displays fed from the real engine, with zero cost when nothing is watched and no change to output.

**Non-Goals:**
- Displays for effect devices (kinds 65–73) or the mixer.
- Oscilloscope/spectrum of the track output (only filter taps).
- Editing by dragging live markers.
- Any change to the player build, the chunk format or the song format.

## Decisions

### D1. Fixed blocks reuse the slot model; compaction is removed
Every slot renders through one `slotCard` (header: toggle, title, type control, routing hint, ✕) in a fixed column
grid (voice+noise | osc | filters | envelopes | LFOs + mod matrix) with fixed column widths so a type change never
reflows other columns. `remapSlots` stays: clear = `map(i) → 0` for the cleared slot and identity for the rest;
filter swap = transposition of two neighbours. Move for other groups and "+ slot" buttons are deleted.
*Alternative*: keep racks and add "show empty slots" — rejected, positions would still shift on add.

### D2. Collapse state
Slot default = expanded iff set (`slotUsed`). A user toggle overrides it for that slot in an in-memory map keyed by
track id and slot (session only). Block toggles persist in `store` (`daw.blk.<name>`), which already falls back to
memory in a sandboxed frame. Not in the song, not in undo.

### D3. Colours
One CSS custom property `--acc` per block (`data-k` attribute), used for header strip, tint, knob arcs and screen
curves; unset slots use the neutral palette. A separate 16-entry voice palette (fixed hues, checked ≥ 3:1 against the
screen background and distinct from all block accents) colours live markers and chips. Token values live in
`style.css` with the existing tokens.

### D4. Static screens from parameters
One SVG per active slot: oscillator (type waveform: PM sine with modulation, saw/pulse with pulse width, wavetable
frame, sampler waveform — already drawn for samplers), filter (magnitude response of the slot's type at its cutoff and
resonance on a 20 Hz–20 kHz log axis), envelope (existing `envCurve`), LFO (one cycle of the shape). They redraw
through the existing `redrawGroup` path (knob change, automation, selects).

### D5. Probe in `unisynth` behind feature `probe` (implies `std`)
- `UInstrument::probe(&self, out: &mut Probe)` fills a fixed-size snapshot after a render call: per active voice
  `{ id, note, amp, released, age_s, release_age_s, env[4]{level, stage}, lfo[2]{phase}, smp[3]{pos 0..1}, cutoff_hz[4] }`.
  `id` = pool slot + note-on counter so the UI can keep a colour per voice.
- Filter taps: `set_taps(bool)`. When on, each filter slot accumulates `(in, out)` mono samples summed over voices into
  a 2048-frame ring owned by the instrument (heap, host-only); see the review notes below for the cost.
- The probe only reads voice state and writes its own buffers, so output stays bit-identical; the goldens are run with
  the feature on and taps on to prove it.
*Alternative*: compute displays in JS from parameters and note events (as the mockup does) — rejected: it would not
show stealing, glide, mod-matrix effects, Tunefish envelope timing or real sampler loops, i.e. it would lie.

**Review of the hook points (task 3.1, before code).**
- Voice state is readable without new fields: `Voice::seq` (note-on order) is the voice id and changes on a steal;
  `age` gives seconds since note-on; `end.env[i].value()/stage()`, `end.lfo[k].phase`, `slots[i].smp` (position, needs a
  read accessor) and `smp[i].src.len()` give the rest. Release age is not stored: the UI times it from the `released`
  flag (adding a voice field would grow the player's voice struct).
- The modulated cutoff is the `cut` closure in `Voice::targets`; it is lifted into one `#[inline]` function used by
  `targets` and the probe, so the two cannot drift. V2 filter kinds (raw V2 laws) report no cutoff.
- Filter taps sit in `Voice::render`'s filter loop, behind `#[cfg(feature = "probe")]` code and an extra `render`
  argument that exists only in probe builds, so the player's loop is the old code. In the DAW build the tap is one
  loop-invariant branch per filter slot. Two alternatives were measured and rejected (task 4.3): a generic `Tap` sink
  and a separate tapped copy of the loop each give the hot oscillator/filter functions a second caller, which changes
  their inlining and costs 11–18 % render time even with nothing watched. The branch costs about 5 % in the DAW (none in
  the player). `render_span` passes the ring and the frame offset. Parallel routing (filters 1–2) taps both slots from
  the same input.
- The probe reads the matrix directly and reports cutoffs as (normalized value after Scale routes, Offset in Hz);
  `probe::cutoff_hz` applies `law::cutoff` on the reader's side. Calling `added`/`tab`/`law::cutoff` from the probe
  would give them a second caller with the same inlining cost. Cutoffs moved by V2 Add routes are reported without that
  move.
- Taps scale by the voice amplitude so a released voice fades out of the spectrum as it does in the output.
- Risk checked: the `us-chan` channel stage runs after the voices and is not tapped (it is not a filter slot).

### D6. Host and worklet transport
`crates/synth-wasm` builds `unisynth` with `probe`; exports `probe_watch(track: i32)` (−1 = none; enables taps on that
instrument only), `probe_ptr()`/`probe_len()` (flat f32 layout: header, ≤ 64 voice records of fixed width, then for
each of the 4 filter slots an active flag and 2 × 2048 tap samples in time order). The worklet copies this slice into
the ~30 ms `status` message only while watching, as a transferable buffer (≤ ~70 KB per message). `engine.ts` exposes
`watch(trackIndex)`; the UI watches the selected track while the Device tab is visible and unwatches otherwise.

### D7. UI rendering
A `requestAnimationFrame` loop redraws only overlay groups (`<g class="live">`) of visible screens from the last
snapshot; static curves are untouched. Voices are sorted by amplitude and the top 16 drawn; the chip row shows "+N".
Colours are assigned by voice `id` from the free palette entries and released when the voice disappears. Coinciding
markers stack ("beads", ~4.5 px steps, away from the screen edge). Envelope dot x comes from `age_s` / `release_age_s`
mapped through the same geometry as `envCurve`, y from the reported level. Filter spectra: 2048-point Hann FFT per
tap on the main thread, log-frequency binning, a short peak-hold fall-off; input drawn grey, output in `--acc`.

### D8. Budgets
Unwatched: no added work in the render path beyond one branch per render call. Watched: taps ≤ 5 % of the render time
of `bench/dense.ts`; UI overlay + FFT ≤ 2 ms per frame at 16 voices and 4 active filters. Measured, not assumed
(tasks).

## Risks / Trade-offs

- [Taps add a store per sample per filter per voice in the hottest loop] → feature off in the player (verified: same
  i686 size); in the DAW measured: watched +0.5–3 %, unwatched about +5 % from the branch, accepted (task 4.3); the Live toggle turns watching off.
- [Message size at 30 ms] → only active filters are sent; voice records only for active voices.
- [Voice ids across steals] → id includes a note-on counter, so a stolen slot gets a new colour.
- [Removing compaction changes test 5.6a and a spec scenario] → rewritten under this change; the song format is
  unaffected, so existing songs open with slots where they always were.
- [Many voices clutter the screens] → 16 drawn, loudest first, "+N".

## Migration Plan

Editor-only behaviour change plus a host-only engine feature; no data migration. Rollback = revert the change.
