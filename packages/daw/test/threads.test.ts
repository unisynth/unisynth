// Threaded bounce in the browser (change add-wasm-threads, spec synth-wasm-threads): headless Chrome against the real
// server (cross-origin isolated) and the built engine modules. Threaded == single-thread == `song render`; silent
// fallback when not isolated or a helper fails; cancel stops every worker.
import { afterAll, beforeAll, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import type { Browser, Page } from "playwright";
import { startServer, type Server } from "../../cli/src/server.ts";
import { buildDaw, DIST } from "../build.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const tmp = mkdtempSync(join(tmpdir(), "daw-threads-"));
let s: Server;
let browser: Browser;
const errors: string[] = [];

beforeAll(async () => {
  await buildDaw({ wasm: false });
  s = await startServer({ port: 0, root: ROOT, build: false, dawDist: DIST, headless: false });
  const { chromium } = await import("playwright");
  const channel = process.env.CHROME_CHANNEL ?? "chrome";
  browser = await chromium.launch({ channel: channel === "bundled" ? undefined : channel, headless: true });
}, 600_000);

afterAll(async () => {
  await browser?.close();
  await s?.stop();
  rmSync(tmp, { recursive: true, force: true });
});

async function open(isolated = true): Promise<Page> {
  const page = await browser.newPage();
  page.on("pageerror", (e) => errors.push(e.message));
  if (!isolated) {
    // the same page without the isolation headers (e.g. served by a host that does not send them)
    await page.route("**/daw/**", async (route) => {
      const r = await route.fetch();
      const headers = { ...r.headers() };
      delete headers["cross-origin-opener-policy"];
      delete headers["cross-origin-embedder-policy"];
      await route.fulfill({ response: r, headers });
    });
  }
  await page.goto(`${s.url}daw/`);
  await page.waitForFunction(() => (window as any).DAW?.engine, null, { timeout: 60_000 });
  return page;
}

/** Bounces `song` (project/music name) in the page; returns the workers used and the SHA-256 of the f32 samples. */
async function bounce(page: Page, song: string, range: [number, number] | null, mt: Record<string, unknown> = {}): Promise<{ workers: number; hash: string; frames: number }> {
  const json = readFileSync(join(ROOT, `project/music/${song}.song.json`), "utf8");
  const r = await page.evaluate(async ([json, range, mt]) => {
    const E = (window as any).DAW.engine;
    E.mt = mt;
    const ex = E.exportBlob(json);
    if (!ex.blob) throw new Error(JSON.stringify(ex.errors));
    const samples: Float32Array = await E.bounce(ex.blob, range, () => {}).done;
    const d = new Uint8Array(await crypto.subtle.digest("SHA-256", samples));
    return { workers: E.lastBounceWorkers, hash: [...d].map((b) => b.toString(16).padStart(2, "0")).join(""), frames: samples.length / 2 };
  }, [json, range, mt] as const);
  return r;
}

/** SHA-256 of the f32 samples `song render --f32` writes for `song`. */
function cliHash(song: string): string {
  const out = join(tmp, `${song}.wav`);
  const r = spawnSync(join(ROOT, "target/release/song"), ["render", join(ROOT, `project/music/${song}.song.json`), "-o", out, "--f32"]);
  if (r.status !== 0) throw new Error(String(r.stderr));
  return createHash("sha256").update(readFileSync(out).subarray(44)).digest("hex");
}

test("isolated: threaded bounce == single-thread bounce == song render (whole song, dense)", async () => {
  expect(spawnSync("cargo", ["build", "-q", "--release", "-p", "song"], { cwd: ROOT, env: { ...process.env, PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH}` } }).status).toBe(0);
  const page = await open();
  expect(await page.evaluate(() => crossOriginIsolated)).toBe(true);
  for (const song of ["demo", "eladamri"]) {
    const mt = await bounce(page, song, null, { workers: 4 });
    expect(mt.workers).toBe(4);
    const one = await bounce(page, song, null, { workers: 1 });
    expect(one.workers).toBe(1);
    expect(mt.hash).toBe(one.hash);
    expect(mt.hash).toBe(cliHash(song));
  }
  // default worker count on this machine: more than one
  expect((await bounce(page, "demo", null)).workers).toBeGreaterThan(1);
  await page.close();
}, 600_000);

test("isolated: loop range threaded == single-thread", async () => {
  const page = await open();
  const range: [number, number] = [3 * 44100 + 77, 9 * 44100];
  const mt = await bounce(page, "elysian", range, { workers: 3 });
  const one = await bounce(page, "elysian", range, { workers: 1 });
  expect(mt.workers).toBe(3);
  expect(mt.frames).toBe(range[1] - range[0]);
  expect(mt.hash).toBe(one.hash);
  await page.close();
}, 600_000);

test("not isolated: the DAW loads and bounces single-threaded without errors", async () => {
  const before = errors.length;
  const page = await open(false);
  expect(await page.evaluate(() => crossOriginIsolated)).toBe(false);
  const r = await bounce(page, "demo", null);
  expect(r.workers).toBe(1);
  expect(r.hash).toBe(cliHash("demo"));
  expect(errors.slice(before)).toEqual([]);
  await page.close();
}, 600_000);

test("a helper that fails to start: the bounce falls back and still equals song render", async () => {
  const page = await open();
  const r = await bounce(page, "demo", null, { workers: 4, helperUrl: "missing-helper.js" });
  expect(r.workers).toBe(1);
  expect(r.hash).toBe(cliHash("demo"));
  await page.close();
}, 600_000);

test("cancel terminates every worker of the bounce; a new bounce works", async () => {
  const page = await open();
  const json = readFileSync(join(ROOT, "project/music/elysian.song.json"), "utf8");
  await page.evaluate((json) => {
    const E = (window as any).DAW.engine;
    E.mt = { workers: 4 };
    const ex = E.exportBlob(json);
    (window as any).JOB = E.bounce(ex.blob, null, (v: number) => ((window as any).PROGRESS = v));
    (window as any).JOB.done.catch((e: Error) => ((window as any).CANCELLED = e.message));
  }, json);
  await page.waitForFunction(() => (window as any).PROGRESS > 0, null, { timeout: 120_000 });
  expect(page.workers().length).toBe(4); // bounce Worker + 3 helpers
  await page.evaluate(() => (window as any).JOB.cancel());
  await page.waitForFunction(() => (window as any).CANCELLED === "cancelled");
  for (let i = 0; i < 50 && page.workers().length > 0; i++) await page.waitForTimeout(100);
  expect(page.workers().length).toBe(0);
  const r = await bounce(page, "demo", null, { workers: 4 });
  expect(r.workers).toBe(4);
  expect(r.hash).toBe(cliHash("demo"));
  await page.close();
}, 600_000);
