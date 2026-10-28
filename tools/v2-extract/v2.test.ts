import { expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { ACCEPTANCE, acceptancePath, check, type Dump, features, V2REDUX } from "./v2.ts";

const patch = (over: Record<number, number>) => {
  const p = Array(89).fill(0);
  p[72] = 64; // chorus amount neutral
  for (const [k, v] of Object.entries(over)) p[+k] = v;
  return p;
};

test("features names oscillator, ring, parallel filters, LFO env mode, speech", () => {
  const d: Dump = {
    version: 6, gdnum: 1, chans: 1, notes: 1, pb: 0, cc: [0, 0, 0, 0, 0, 0, 0], speech: 20, globals: Array(23).fill(0),
    patches: [{ n: 0, p: patch({ 2: 1, 8: 5, 9: 1, 20: 1, 23: 3, 26: 2, 44: 4, 46: 1 }), m: [[10, 100, 21]] }],
  };
  expect(features(d)).toEqual(["flt:high", "flt:low", "flt:parallel", "lfo:env", "lfo:s+h", "osc:fm", "osc:ring", "osc:saw/tri", "speech"]);
});

test.skipIf(!existsSync(join(V2REDUX, "src/v2load.h")) || !existsSync(acceptancePath("fr08")))("acceptance checksums match PROVENANCE.md", () => {
  const r = check();
  expect(r.length).toBe(ACCEPTANCE.length);
  expect(r.filter((x) => !x.ok)).toEqual([]);
});
