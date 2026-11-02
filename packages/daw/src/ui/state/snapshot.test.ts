// Undo snapshots (change daw-solid-ts 9.3, design D2 "Snapshot cost"): a structured snapshot whose lane points are
// cached per array, so a gesture re-serializes only the track bodies and the lanes it replaced. Real Solid runtime
// via the test preload.
import { expect, test } from "bun:test";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fromSong } from "../../model.ts";
import { createSong, raw, restoreSong } from "./song.ts";
import { makeSnap, primeSnap, setVerify, snapEqual, snapState, snapStats } from "./snapshot.ts";

const DEMO = JSON.parse(readFileSync(new URL("../../../../../project/music/demo.song.json", import.meta.url), "utf8"));
const setup = () => {
  const song = createSong(fromSong(DEMO));
  const snap = () => makeSnap(raw(song.tracks), raw(song.master), { sel: "bass", armed: "bass" }, 0);
  const bass = () => song.tracks.find((t) => t.id === "bass")!;
  return { song, snap, bass };
};

test("round trip: the snapshot's state equals the song (tracks, lanes and points, master, meta)", () => {
  const { song, snap } = setup();
  const st = snapState(snap());
  expect(st.tracks).toEqual(JSON.parse(JSON.stringify(raw(song.tracks))));
  expect(st.master).toEqual(JSON.parse(JSON.stringify(raw(song.master))));
  expect([st.sel, st.armed]).toEqual(["bass", "bass"]);
});

test("an unchanged lane reuses its cached points; a body edit and a replaced points array are seen", () => {
  const { snap, bass } = setup();
  const a = snap(), n0 = snapStats.pts;
  const b = snap();
  expect(snapStats.pts).toBe(n0);  // no lane re-serialized
  expect(snapEqual(a, b)).toBe(true);
  bass().vol -= 1;
  expect(snapEqual(a, snap())).toBe(false);
  bass().vol += 1;
  const ln = bass().lanes[0]!;
  ln.pts = [...ln.pts.slice(0, -1), [ln.pts.at(-1)![0], 0.123]];  // copy-on-write edit
  const c = snap();
  expect(snapStats.pts).toBe(n0 + 1);
  expect(snapEqual(a, c)).toBe(false);
  expect(snapState(c).tracks.find((t) => t.id === "bass")!.lanes[0]!.pts.at(-1)![1]).toBe(0.123);
});

test("verify mode: points changed in place throw instead of reusing the stale string", () => {
  const { snap, bass } = setup();
  snap();
  bass().lanes[0]!.pts[0] = [1, 0.99];  // in place: not allowed
  setVerify(true);
  try { expect(() => snap()).toThrow(/in place/); } finally { setVerify(false); }
});

test("restoreSong drops the cache (reconcile keeps array identity with new contents); priming restores it", () => {
  const { song, snap, bass } = setup();
  const before = snap();
  const ln = bass().lanes[0]!;
  ln.pts = ln.pts.map(([b, v]) => [b, v / 2]);
  const edited = snap();
  const ptsObj = raw(bass().lanes[0]!.pts);
  restoreSong(song, snapState(before));
  expect(raw(bass().lanes[0]!.pts)).toBe(ptsObj);  // same array, old values again
  expect(snapEqual(snap(), before)).toBe(true);    // not the edited string
  expect(snapEqual(snap(), edited)).toBe(false);
  restoreSong(song, snapState(edited));
  primeSnap(raw(song.tracks), raw(song.master), edited);
  const n0 = snapStats.pts;
  expect(snapEqual(snap(), edited)).toBe(true);
  expect(snapStats.pts).toBe(n0);  // primed: nothing re-serialized
});

test("no in-place lane point writes in the session code (they would bypass the snapshot cache)", () => {
  const ui = new URL("..", import.meta.url).pathname, hits: string[] = [];
  const walk = (d: string) => { for (const f of readdirSync(d)) { const p = join(d, f);
    if (statSync(p).isDirectory()) walk(p); else if (/\.tsx?$/.test(f) && !f.endsWith(".test.ts")) readFileSync(p, "utf8").split("\n").forEach((l, i) => {
      if (/\bln\.pts\s*\.(push|splice|sort|unshift|pop|shift|reverse|fill|copyWithin)\(|\bln\.pts\s*\[[^\]]*\]\s*(\[[^\]]*\]\s*)?=[^=]/.test(l)) hits.push(`${p.slice(ui.length)}:${i + 1}: ${l.trim().slice(0, 100)}`);
    }); } };
  walk(ui);
  expect(hits).toEqual([]);
});
