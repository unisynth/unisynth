# src/synth.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Shared 16-voice / 256-slot-queue scheduler `Synth<V: Voice<C>>` (`note_on`, `note_on_vel(note, velocity, offset)` (the velocity rides the queued `Event` to `Voice::start_vel`, default = `start`: only ID 20 overrides it), `note_off`, `process(ctx, l, r)` zeroes then renders; `Voice`, `ActiveNote` traits, `Error::{Note,Offset,QueueFull}`, `VOICES`, `QUEUE`, `MAX_NOTE`): first free queue slot, slot-order firing (a freed slot can be reused so a later event overtakes an equal-delay older one), sub-blocks split at the minimum pending delay, lowest-index inactive voice, no stealing (17th note lost), note-off releases every active voice of that note, voices render in index order and finish the sub-block they free themselves in.
- Poly mode by default; **mono note-stack mode** (`set_mono`, `is_mono`, `mono_depth`; 128-cell stack sharing its 129th cell with the depth, covered by `tests_mono.rs`; used by ID 14) — unison is not modelled; `process` asserts block ≤ `i32::MAX` samples (i32 delays).
