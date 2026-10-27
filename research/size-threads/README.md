# size-threads

Gates and measurements for change `add-synth-threads` (opt-in multi-threaded song rendering).

## Size gate

`./hashes.sh` builds the i686 `no_std` size probes **without** the `threads` feature (`ws-size-probe` `all` /
`slaughter`, `us-size-probe` all modules on the converted `showcase` song, and one `us-size-probe` per
`project/music/*.song.json` with exactly its `synth-convert features`) and prints each executable's SHA-256. `/Brepro`
replaces the link timestamp with a content hash, so equal code gives equal bytes (checked: two builds after `touch`ing
the crates give the same hashes).

`baseline.txt` = the probes of commit `ebf516c` (before add-synth-threads), built in a clean `git worktree`. Another
change (`shrink-unified-synth`) edits `unisynth` and `us-size-probe` in the same working tree, so a before/after
comparison in place mixes both changes. The gate is therefore run in isolation:

```bash
git worktree add --detach /tmp/wt HEAD            # tree without either change
(cd /tmp/wt && /path/to/research/size-threads/hashes.sh > /tmp/A.txt)
# apply only the add-synth-threads hunks (synth-core, unisynth/tfsynth Cargo.toml + device.rs) to /tmp/wt
(cd /tmp/wt && /path/to/research/size-threads/hashes.sh > /tmp/B.txt)
diff /tmp/A.txt /tmp/B.txt                        # must print nothing
```

Result (task 5.1): identical for all 11 probes; the all-modules probe imports only `kernel32!ExitProcess`.

## Benchmark

Benchmark songs are the dense converted songs in `project/music` (unified synth only, many tracks with buses and
sidechain receives). Single-thread wall time of `song render <song> -o x.wav` (release, before the change, Intel
i9-9900K, 8 cores / 16 threads, macOS):

| Song | Tracks | Receives | Time (s) |
|---|---|---|---|
| `engage` | 42 | 63 | 52.9 |
| `elysian` | 32 | 42 | 40.3 |
| `turtles` | 20 | 20 | 47.3 |
| `no-remorse` | 16 | 16 | 29.8 |
| `eladamri` | 9 | 9 | 14.1 |

## Parallel render (after the change)

`./bench.sh` renders each song with no flag and `--threads 1/2/4/8/16` (release, same machine), times the whole CLI
call (export excluded: the blob is exported first) and compares every WAV byte for byte with the single-thread one:

| Song | flag none | 1 | 2 | 4 | 8 | 16 | best speed-up | WAVs equal |
|---|---|---|---|---|---|---|---|---|
| `engage` | 48.3 | 46.4 | 25.9 | 14.5 | 10.7 | 12.9 | 4.51× | yes |
| `elysian` | 35.6 | 36.7 | 23.5 | 15.5 | 13.9 | 17.2 | 2.57× | yes |
| `turtles` | 42.1 | 41.7 | 23.0 | 15.4 | 11.9 | 13.8 | 3.54× | yes |
| `no-remorse` | 26.7 | 27.0 | 15.8 | 10.0 | 8.3 | 9.8 | 3.20× | yes |
| `eladamri` | 12.7 | 12.6 | 8.2 | 7.5 | 7.4 | 10.2 | 1.71× | yes |

- `--threads 1` is the old single-thread path: same time as before the change within run-to-run noise (± ~5 % on
  this machine; the "before" column above was measured with `/usr/bin/time` including export).
- Best at 8 workers = physical cores. 16 is slower: helpers share cores with busy workers (hyperthreads).
- Speed-up is bounded by the heaviest track in each dependency level (load imbalance) and by one barrier per level
  per 100-frame sub-block; `eladamri` (9 tracks) has little to spread.
- Wait policy of the std pool (`crates/song/src/workers.rs`): spin 64, then up to 2000 `yield_now`, then park. Tried
  on `engage`/`elysian` at 8 / 16 workers: busy spin 2¹⁴ gives 9.6–10.3 s / 16–63 s (16 workers collapse), 256 spin
  10.4 / 14.0 s, 64 + 2000 yields 9.4 / 10.9 s.
