import { expect, test } from "bun:test";
import { anchorScroll, clamp, wheelF } from "./zoom.ts";

test("anchorScroll keeps the content under the anchor", () => {
  // 44 px per bar, scrolled 300, pointer at 200: bar 1 + 500/44 under it; at 88 px per bar the same bar is at 200
  const s = anchorScroll(300, 200, 44, 88);
  expect((s + 200) / 88).toBeCloseTo(500 / 44, 9);
  expect(anchorScroll(0, 0, 10, 30)).toBe(0);
});
test("wheelF: notch up zooms in, down out, clamped; line mode scales", () => {
  expect(wheelF({ deltaY: -100, deltaMode: 0 })).toBeCloseTo(Math.exp(0.48), 9);
  expect(wheelF({ deltaY: 100, deltaMode: 0 })).toBeCloseTo(Math.exp(-0.48), 9);
  expect(wheelF({ deltaY: -3, deltaMode: 1 })).toBeCloseTo(Math.exp(0.48), 9);
  expect(wheelF({ deltaY: 10, deltaMode: 0 })).toBeCloseTo(Math.exp(-0.08), 9);
});
test("clamp", () => { expect(clamp(5, 0, 3)).toBe(3); expect(clamp(-1, 0, 3)).toBe(0); expect(clamp(2, 0, 3)).toBe(2); });
