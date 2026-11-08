# src/cauldron/import.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). XM side of the mini-cauldron converter. See change: add-cauldron-xm-import, add-tbl-demo-songs, add-cauldron-control-rate, add-cauldron-lane-thinning, compact-cauldron-xm-import, slim-cauldron-chain.

## Phrase paths
- `identify(&Call, &Xm)` → per instrument/sample `Option<(record, variant)>`: Jizz name `Probe<rec+1><var+1>` (error on unknown); Stash render order slot by slot, drum slot = `drum_count` samples (`None`), empty samples skipped.
- `Loop {len, start, length, kind}` (stored samples). `path(&[TickCtl], offset, &Loop, decim, rate)` → `Path {pieces: Vec<Piece {t0, t1, x0, x1, cont}>, end, backward}`: position integrated per tick through offset, forward loop, ping-pong, sample end; a backward run becomes its steps in reverse order, each forwards over the time spent in it.
- `unroll(&Call, record, variant, ratio, &Path)` → `Vec<PhraseEv>` in output frames: `phrase_events` mapped along pieces; a jump offs the held note and enters at the step under the position (head retrigger, step pitch/phase, velocity = position, envelope age) if the original sounds there (before its off or within the release: v1 a step, v2 `release`); off right after when entering a fade; ends with an off at `Path::end`.

## Tracks
- `TrackPlan {record, class, triggers}` (`ratio()` = 2^(class/12)). `group(&Replay, ids, decim, span)` keys (record, class, channel) and `merge`s channel groups of one (record, class) whose `span`s (path ends) never overlap (used with `Options::split_pitches`).
- `class_of` = middle of sounding ratios (silent ticks via `sounds` ignored) raised so the top is within `MAX_SHIFT` 48, ≤ `MAX_CLASS` 24, ≥ `MIN_CLASS` −48. `MAX_BEND` 24.
- `handovers(trigger, decim, end)` (slide span > `SEGMENT_SPAN` 94 → segment start ticks) + `clip(&Path, a, b)` → segments become triggers before grouping; `absorbed` follows them.
- `split_shift(shift, lane)` → (transpose, bend), transpose byte rounded away from 0, zero = 128 when a lane automates it.
- `controls(&Replay, &TrackPlan, decim, end)` / `controls_of(.., reference, end)` → `Controls {gain, pan, bend, range, muted, master, post, post_pan, warnings}` as change-only points per tick, only ticks before the trigger's path end. Bend = semitones from class, clamped ±`MAX_SHIFT`; ticks past it silent + warning (`Converted::muted`). Pan = XM `pan/256`, passed through (openmpt pans FT2 files equal-power like unisynth). Phrase tracks: `master` = √2 constant (voice centred, chain clamps see the original level), `post` gain after the chain (EQ 3band via `post_gain`, `POST_DELAY` 3, first point `LANE_LEAD` early, 0 at path end unless next trigger starts by then) and `post_pan` → EQ 3band params 1–4. `post_value(g)` inverts the EQ gain law (g > 1: `0.5 + √((g−1)/10)/2`).

## Stored samples
- `sampler(&xm::Sample, rate)` → `Stored {values, gsm, shift, up}` (`note(xm_note)` = n + shift): GSM sample 1 of the chunk, octave/fine fold relative + finetune + 8363·up/rate, manual loop bounds `(x·up + ½)/len`, loop kind → repeat/ping-pong, capacity 1, velocity 0, master √2, 1 ms release, slot `GAIN` restores the codec's level loss.
- `gsm_upsampled(pcm)` → (×1–4 windowed-sinc factor, smallest reaching 0.95 correlation, chunk, level gain).

## Convert
- `pick_call(&Patches, &Xm)`: single call, or the Stash call whose sample count fits. `convert(&Patches, &Xm, rate)` = `convert_with(.., Options::default())` → `Converted {song, lead, warnings, pingpong, muted, tracks: (phrase, stored), records, names, dropped, ambience, idle_clips}`.
- `Options {controls: ControlRate, thin: u8, keep_layers, split_pitches, dry, exact_chain}` (default tick, 0, false ×4). `ControlRate {Tick, Row, Note}` (`parse`); `hold_controls(&mut Replay, rate)` holds each trigger's freq/vol/pan from its first tick (`Row`: each row's first tick) before everything else.
- Flow: replay → `hold_controls` → paths → `compact::layers` (unless `keep_layers`) → handovers/`clip` → `compact::pack` (phrase) / `pack_by` (stored), or `group`/`merge` with `split_pitches` → ambience per track (`compact::ambience`, none with `dry`) → `tempo_for(longest echo)` → chains (`effects`, `slide_lowpass` on sliding tracks, `compact::slim_lowpass` unless `exact_chain`) → phrase tracks built first (instrument + chain + post EQ + ambience devices) → idle clips found in parallel (`std::thread::scope`) and removed → stored tracks (on at trigger, off at path end, `SMP_START` lane for offsets; delay ambience) → master track receiving all.
- Phrase track: `note_lanes` of all triggers' `unroll`, doubler `restart` toggled per trigger, low-pass cut automated along the slide (`chain.lp_stages` lanes), track volume `XM_GAIN·halve/level·make-up`. Stored track volume `XM_GAIN·make-up`.
- Lanes: `v::MASTER`, `v::PAN` (centred for phrases), `ctl::BEND` (+ `BEND_RANGE`), `v::TRANSPOSE` (never thinned), post gain/pan, low-pass thinned by `thin_lane` (`thin`; bend breaks at transpose changes); never thinned: transpose, restart, starts. Control lanes a full `LANE_LEAD` early. Automation `device` indices are track-local.
- Consts: `XM_GAIN` 0.375 (openmpt's hard-left level for files it detects as FT2), `ECHO_MAX_MS` 4000, `tempo_for(ms)` = clamp(⌊240000/(ms − 200)⌋, 56, 999), `POST_DELAY` 3, `MIN_CLASS`/`MAX_CLASS`, `MAX_BEND`, `MAX_SHIFT`.
- Warnings: ping-pong sample (once per sample), bend/reach clamp, echo clamp.
- `track_names(&[(record, sample, class)])`: `<sample> (record r)`, `(record r, ±c)`, `<sample> (sample)`; `.smp` dropped; repeats numbered.
- `keep_layers` + `split_pitches` → byte-identical to the layered conversion.
