# unify-v3-layout (sidecar: full detail of the AGENTS.md rows)

Pull-only (not auto-injected).

## Files
- `v3-layout.tsv`: v3 index map (index, name, law, default), 294 rows.
- `old-layout.tsv`: old layout dumped before the switch (index, name, law, default, pre-1.3 name), 260 rows.
- `pairs.tsv`: per tracked song (`project/music/**`): instruments, old/v3 pair counts, old/v3 chunk bytes, two-byte indices.
- `layout.py`: frozen design artifact; reference value rules. Commands: `map` (print v3 index map); `pairs [--json out]` (pair counts + chunk bytes per tracked song); `blob <in.song> <out.song>` (migrate instrument chunks, lane indices, effect tag); `rust <old.rs> <v3.song> <out.rs>` (swap v3 sections into `synth-convert rust` output). `migrate(old)` = reference migration, cross-check for the Rust migration (`crates/unisynth/src/migrate`).
- `crosscheck.py full|sample <out>`: per instrument of tracked songs, old pairs | v3 pairs; feeds `crates/unisynth/src/migrate/crosscheck.txt` (opt-in test `MIGRATE_CROSSCHECK=<out>`).
- `probe.sh`: compo probe of each reference song (and V2 probe songs) built twice from the same player code, own bytes vs v3 bytes (`layout.py blob`) + 34 added defaults (`--cfg us_v3_pad` in `us-size-probe`); prints raw / xz -9e / squishy. `ONLY=<regex>` subsets.

## History
See change: unify-v2-algorithms (6.4 freeze gate, 6.5–6.8 format switch).
