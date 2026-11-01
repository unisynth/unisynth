// Edit core on a session (change daw-solid-ts 9.1, step 2): commit/undo/redo restore the song and the selection,
// a no-op gesture is not a step, and the hooks run restore-then-change. Real Solid runtime via the test preload.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { fromSong } from "../../model.ts";
import * as core from "./core.ts";
import * as S from "./session.ts";

const initial = fromSong(JSON.parse(readFileSync(new URL("../../../../../project/music/demo.song.json", import.meta.url), "utf8")));
S.initSession({ initial } as unknown as S.Daw);
const calls: string[] = [];
core.initCore({ restored: () => calls.push("restored"), changed: () => calls.push("changed") });

test("session: first instrument track selected and armed; setters assign the live bindings", () => {
  expect(S.selected).toBe(S.tracks[0]!);
  expect(S.armed?.devices.some((d) => d.kind === 64)).toBe(true);
  S.setSelected(S.tracks[1]!); expect(S.selected).toBe(S.tracks[1]!);
  S.setSelected(S.tracks[0]!);
});

test("commit, undo, redo: values and selection come back; hooks restore then change; no-op is not a step", () => {
  const t = S.tracks[0]!, v0 = t.vol;
  const b0 = core.snap(); core.commit(b0, "nothing");
  expect(core.H.depth).toBe(0);
  const before = core.snap(); t.vol = v0 - 6; S.setSelected(S.tracks[2]!); core.commit(before, "vol");
  expect(core.H.depth).toBe(1); expect(calls).toEqual(["changed"]);
  calls.length = 0;
  core.undo();
  expect(S.tracks[0]!.vol).toBe(v0); expect(S.selected.id).toBe(S.tracks[0]!.id);
  expect(S.tracks[0]).toBe(t);  // identity kept (store reconcile by id)
  expect(calls).toEqual(["restored", "changed"]);
  core.redo();
  expect(S.tracks[0]!.vol).toBe(v0 - 6); expect(S.selected.id).toBe(S.tracks[2]!.id);
  core.undo(); core.undo();  // second: nothing to undo
  expect(S.tracks[0]!.vol).toBe(v0);
});
