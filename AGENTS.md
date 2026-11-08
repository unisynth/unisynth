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
When authoring a proposal, add a `## Discipline Skills` line to `proposal.md`
naming the `eng-disciplines` skills its tasks will trigger (mapped via the
checkpoint table below); omit only when none apply. This needs no edit to any
openspec skill — the implement loop reads the proposal artifact unchanged, so
the named skills enter its context and get invoked.

## Discipline Skills

During implementation, invoke the matching `eng-disciplines` skill when a task
signal appears. Skills auto-trigger on natural language, but the implement loop
may never utter the phrase — this table makes the mapping explicit (signals are
observable in the diff / `tasks.md`, not vague intent):

| Task signal (in diff / tasks.md) | Skill |
|---|---|
| touches auth, untrusted input, secrets, webhooks, PII | `security-hardening` |
| spec has a latency/throughput budget, or a large-data / high-traffic path | `performance-optimization` |
| new endpoint, job, external call, or "can't tell what happened in prod" | `observability-instrumentation` |
| non-trivial/irreversible step (migration, public API, cross-boundary) BEFORE it stands | `doubt-driven-review` |
| a bug surfaces mid-implementation | `systematic-debugging` |
| runtime state opaque, `console.log` insufficient | `node-inspect-debugger` |
| feature works + tests pass but the implementation feels heavy | `code-simplification` |

The end gates (`code-review`, `code-quality`) remain unchanged and run at completion before commit.

<!-- dox-doctrine -->

## DOX doctrine

Per-turn DOX doctrine — the kb-first READ discipline and the directory `AGENTS.md`
WRITE discipline — is injected by the `pi-dashboard-kb-extension`. Tune it in
`.pi/dashboard/knowledge_base.json` under the `doctrine` key (`inject`, `write`).
