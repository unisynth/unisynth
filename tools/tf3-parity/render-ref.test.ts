import { describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loudness, readWav } from "../synth-parity/metrics.ts";
import { NAMES } from "./e3prj.ts";
import { renderer, renderRef } from "./render-ref.ts";

/** A one-instrument project: a 2-point spline cycle, ADSR1 on volume, one note at row 0 released at row 8. */
export function oneNoteProject(extra: Record<string, number> = {}): string {
  const v: Record<string, number> = {
    OscPoints: 1 / 3, OscP1Off: 0.25, OscP1Val: 1, OscP1Int: 1, OscP2Off: 0.66, OscP2Val: 0, OscP2Int: 1, OscFiInt: 1,
    OscVol: 0.5, OscPan: 0.5, OscPoly: 1, OscOctave: 0.5, GainAmount: 0.5, NoiseBw: 1,
    Env1Att: 0, Env1Dec: 0.5, Env1Sus: 0.5, Env1Rel: 0.2, Env1Slp: 0.5, Mm1Source: 3 / 9, Mm1Mod: 0.55, Mm1Target: 9 / 25,
    ...extra,
  };
  const params = NAMES.map((n) => `<param value="${v[n] ?? 0}" name="${n}"/>`).join("");
  return `<?xml version="1.0"?><enigma version="3.11b"><songs><song bpm="120" id="1" name="t"><patterns>
<pattern rowcount="16" patternid="0" trackcount="1"><tracks><track>
<ne i="0" n="58" r="0" v="-1" e="0"/><ne i="-1" n="128" r="8" v="-1" e="0"/></track></tracks></pattern></patterns>
<patterninstances><patterninstance rowoffset="0" patternid="0" track="0"/></patterninstances></song></songs>
<instruments><instrument id="0" name="t"><parameters>${params}</parameters></instrument></instruments></enigma>`;
}

describe("tf3 reference renderer", () => {
  test("build-ref skips without a checkout when offline", () => {
    const r = spawnSync(join(import.meta.dir, "build-ref.sh"), [], { env: { ...process.env, TF3_REF: "/nonexistent/tf3", TF3_OFFLINE: "1" }, encoding: "utf8" });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain("skipped");
  });

  test("render-ref skips without the renderer", () => {
    const r = spawnSync("bun", [join(import.meta.dir, "render-ref.ts"), "x.e3prj", "x.wav"], { env: { ...process.env, TF3_REF: "/nonexistent/tf3" }, encoding: "utf8" });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain("skipped");
  });

  test.skipIf(!existsSync(renderer(false)))("one note renders a non-silent WAV", () => {
    const dir = mkdtempSync(join(tmpdir(), "tf3-test-"));
    writeFileSync(join(dir, "one.e3prj"), oneNoteProject());
    expect(renderRef(join(dir, "one.e3prj"), join(dir, "one.wav"), { tail: 1 })).toBe(true);
    const wav = readWav(join(dir, "one.wav"));
    expect(wav.ch[0].length).toBeGreaterThan(44100 * 2);
    expect(loudness(wav)).toBeGreaterThan(-40);
  });
});
