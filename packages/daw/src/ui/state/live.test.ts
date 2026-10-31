// Display state (change daw-solid-ts 2.4): values change at frame rate, readers update only on a shown change.
import { expect, test } from "bun:test";
import { createEffect, createRoot } from "solid-js";
import { load, peaks, playing, pos, setLoad, setPeak, setPlaying, setPos } from "./live.ts";

const settle = () => new Promise((r) => setTimeout(r, 0));
function counter(read: () => unknown) { const c = { n: 0 }; createRoot(() => createEffect(() => { read(); c.n++; })); return c; }

test("an unchanged value does not notify; a changed one does", async () => {
  const p = counter(() => pos()), pl = counter(() => playing()), l = counter(() => load());
  await settle();
  setPos(pos()); setPlaying(playing()); setLoad(load()); await settle();
  expect([p.n, pl.n, l.n]).toEqual([1, 1, 1]);
  setPos(3.25); setPlaying(true); setLoad(42); await settle();
  expect([p.n, pl.n, l.n]).toEqual([2, 2, 2]);
});

test("a meter notifies only beyond the shown resolution, per track and channel", async () => {
  setPeak("a", 0.5, 0.5);
  const a = counter(() => peaks.a?.[0]), b = counter(() => peaks.b?.[0]);
  await settle();
  setPeak("a", 0.501, 0.5); await settle();   // < 2e-3: not shown
  expect(a.n).toBe(1);
  setPeak("a", 0.6, 0.5); await settle();
  expect(a.n).toBe(2);
  expect(b.n).toBe(1);                          // another track's meter is untouched
  expect(peaks.a).toEqual([0.6, 0.5]);
});
