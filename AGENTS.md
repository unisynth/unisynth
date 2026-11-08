# unisynth

One soft synth for a 64k intro (by robson / Exceed): the unified synth (`crates/unisynth`) on a shared no_std core
(`crates/synth-core`), legacy engines it replaces and checks against (`wavesabre`, `tfsynth`, `glorp`), the song
format + converter (`song`, `synth-convert`), the browser DAW (`packages/daw`, engine `crates/synth-wasm`) and the
public site (`site/`, GitHub Pages at unisynth.exceed.hu). Split out of the 64k intro repo with its history; 64k
consumes these crates as git dependencies pinned by `rev`.

Stack: **cargo** (nightly, `rust-toolchain.toml`), **pnpm** owns JS dependencies, **bun** runs scripts and tests. Do not
run `bun install`. Procedural GM bank (`dls`, `gmgen`) comes from https://github.com/unisynth/gm-procedural (pinned).

## Commands

```bash
pnpm install --frozen-lockfile && cargo fetch          # install
cargo test --workspace && bun test                     # test
bun packages/daw/build.ts && bun packages/cli/src/cli.ts serve   # DAW at http://127.0.0.1:6464/daw/
scripts/build-daw.sh && scripts/build-songs.sh         # rebuild site/daw and site/songs (set GM_DLS, see NOTICE.md)
```

## Rules

- Think before coding; state assumptions; confirm the plan before major changes.
- Minimum code; surgical changes; match existing style.
- Tests first (TDD); verify with the commands above.
- Synth docs: `docs/synth/` (source of truth = crates; update the doc when a crate's behaviour changes).
- Per-directory `AGENTS.md` files hold the per-file records; keep this root file lean.

## OpenSpec

Spec-driven changes live in `openspec/changes/<name>/`; specs in `openspec/specs/`.
