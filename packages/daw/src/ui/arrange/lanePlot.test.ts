import { expect, test } from "bun:test";
import { laneLine, lanePlot } from "./lanePlot.ts";

const x = (b: number) => (b - 1) * 10;  // 10 px per bar

test("sparse points: every point drawn and handled", () => {
  const pts: [number, number][] = [[1, 0], [2, 0.5], [3, 1]];
  expect(lanePlot(pts, 1, 10, x, 4, -1)).toEqual({ line: [0, 1, 2], handles: [0, 1, 2] });
});

test("dense points: line keeps first/min/max/last per pixel, handles min/max per handle column, extremes kept", () => {
  // 100 points in one bar (10 px): ~10 per pixel; a spike at 37
  const pts = Array.from({ length: 100 }, (_, k): [number, number] => [1 + k / 100, k === 37 ? 1 : 0.5]);
  const { line, handles } = lanePlot(pts, 1, 2, x, 4, -1);
  expect(line.length).toBeLessThan(45);
  expect(line).toContain(37);
  expect(line[0]).toBe(0);
  expect(line[line.length - 1]).toBe(99);
  expect(handles.length).toBeLessThanOrEqual(6);  // 3 columns of 4 px × (min, max)
  expect(handles).toContain(37);
  expect([...line].sort((a, b) => a - b)).toEqual(line);
});

test("window: handles only in [w0, w1], line one neighbour beyond; the selected point keeps its handle", () => {
  const pts = Array.from({ length: 20 }, (_, k): [number, number] => [1 + k, k / 20]);  // one per bar
  const { line, handles } = lanePlot(pts, 5, 8, x, 4, 15);
  expect(handles).toEqual([4, 5, 6, 7, 15]);
  expect(line).toEqual([3, 4, 5, 6, 7, 8]);
  expect(lanePlot([], 1, 2, x, 4, -1)).toEqual({ line: [], handles: [] });
});

test("laneLine: the whole lane, thinned per pixel, ends kept", () => {
  const pts = Array.from({ length: 1000 }, (_, k): [number, number] => [1 + k / 100, (k % 7) / 7]);  // 10 bars, 100 px
  const l = laneLine(pts, x);
  expect(l[0]).toBe(0);
  expect(l[l.length - 1]).toBe(999);
  expect(l.length).toBeLessThanOrEqual(4 * 101);
  expect(laneLine([], x)).toEqual([]);
});
