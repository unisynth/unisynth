// Clip thumbnail geometry (change daw-solid-ts 5.3): the old per-note <i> layout, as rectangles for a canvas.
import { expect, test } from "bun:test";
import { thumb } from "./thumb.ts";

const N = (s: number, l: number, n: number) => ({ s, l, n });

test("notes: x by step, width by length (min 1.5 px), y by pitch over the clip's range (88 %)", () => {
  const r = thumb([N(0, 4, 60), N(8, 4, 72)], { len: 1, loop: false, patLen: 1, w: 160, h: 30 });
  expect(r.marks).toEqual([]);
  expect(r.notes).toEqual([{ x: 0, y: 26.4, w: 40 }, { x: 80, y: 0, w: 40 }]);
});

test("a looping clip repeats its pattern and marks each repeat; notes past the pattern end are not repeated", () => {
  const r = thumb([N(0, 2, 60), N(20, 2, 60)], { len: 3, loop: true, patLen: 1, w: 300, h: 30 });
  expect(r.marks).toEqual([100, 200]);
  expect(r.notes.map((n) => n.x)).toEqual([0, 100, 200]);
});

test("a cropped clip shows the pattern's start; a note crossing the end is cut", () => {
  const r = thumb([N(0, 4, 60), N(12, 8, 60)], { len: 1, loop: false, patLen: 2, w: 160, h: 30 });
  expect(r.notes).toEqual([{ x: 0, y: 0, w: 40 }, { x: 120, y: 0, w: 40 }]);
});

test("no notes: nothing", () => expect(thumb([], { len: 2, loop: true, patLen: 1, w: 100, h: 30 })).toEqual({ notes: [], marks: [50] }));
