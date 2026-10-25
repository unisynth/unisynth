import { expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { TOL } from "./parity.ts";

test("onset tolerance is one 512-frame block, the rest the shared defaults", () => {
  expect(TOL).toMatchObject({ loudnessDb: 1, bandDb: 3, onsetFrames: 512 });
});

test("skips cleanly without the reference renderer", () => {
  const r = spawnSync("bun", [join(import.meta.dir, "parity.ts")], {
    env: { ...process.env, TF_REF: "/nonexistent/tf-ref" },
    encoding: "utf8",
  });
  expect(r.status).toBe(0);
  expect(r.stdout).toContain("skipped");
});

// Full run (≈ 1 min): TF_PARITY=1 TF_REF=… bun test tools/tf-parity
test.if(!!process.env.TF_PARITY)("reference songs pass", () => {
  const r = spawnSync("bun", [join(import.meta.dir, "parity.ts")], { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
  console.log(r.stdout);
  expect(r.status).toBe(0);
}, 900_000);
