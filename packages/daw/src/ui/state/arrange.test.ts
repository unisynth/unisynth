// Arrangement windowing (change daw-solid-ts 5.2, design D6): which rows exist for a scroll position.
import { expect, test } from "bun:test";
import { barWindow, rowHeight, segments, windowRows } from "./arrange.ts";

const H = Array.from({ length: 50 }, () => 56);  // 50 closed tracks

test("row height: track, plus its automation lanes and the add row when open", () => {
  expect(rowHeight(false, 3)).toBe(56);
  expect(rowHeight(true, 0)).toBe(56 + 24);
  expect(rowHeight(true, 2)).toBe(56 + 80 + 24);
});

test("window: rows meeting the view plus one screen above and below", () => {
  // view 560 px (10 rows) at the top: rows 0..19 (view + one screen below)
  expect(windowRows(H, 0, 560, [])).toEqual(Array.from({ length: 20 }, (_, i) => i));
  // scrolled to row 20: rows 10..39
  const w = windowRows(H, 20 * 56, 560, []);
  expect(w[0]).toBe(10); expect(w[w.length - 1]).toBe(39);
});

test("pinned rows (selected, focused) are always included, in order", () => {
  expect(windowRows(H, 20 * 56, 560, [2, 48]).filter((i) => i < 10 || i > 39)).toEqual([2, 48]);
  expect(windowRows(H, 0, 560, [5])).toEqual(windowRows(H, 0, 560, []));  // already inside
});

test("segments: gaps keep every row at its offset and the total height", () => {
  const rows = [2, 10, 11];
  expect(segments(H, rows)).toEqual([{ gap: 112 }, 2, { gap: 7 * 56 }, 10, 11, { gap: 38 * 56 }]);
  const total = (s: ReturnType<typeof segments>) => s.reduce((n: number, x) => n + (typeof x === "number" ? H[x] : x.gap), 0);
  expect(total(segments(H, rows))).toBe(50 * 56);
  expect(segments(H, [0, 1])).toEqual([0, 1, { gap: 48 * 56 }]);
});

test("bar window: pages of the view width around the view, stable within a page", () => {
  expect(barWindow(0, 1000, 100)).toEqual([-9, 31]);
  expect(barWindow(999, 1000, 100)).toEqual(barWindow(0, 1000, 100));
  expect(barWindow(1000, 1000, 100)).toEqual([1, 41]);
});
