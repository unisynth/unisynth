// Playback never edits the song (change daw-solid-ts 2.4, design D3): automation values, meters, position and load
// are display state. Playing an automated song changes no saved value and adds no undo step, while the automated
// knob follows its lane. Headless Chrome against the real server on a temporary project/music.
import { afterAll, beforeAll, expect, test } from "bun:test";
import { cpSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Browser, Page } from "playwright";
import { startServer, type Server } from "../../cli/src/server.ts";
import { buildDaw, DIST } from "../build.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const root = mkdtempSync(join(tmpdir(), "daw-play-"));
let s: Server;
let browser: Browser;
let page: Page;
const errors: string[] = [];

beforeAll(async () => {
  mkdirSync(join(root, "project/music"), { recursive: true });
  cpSync(join(ROOT, "project/music/demo.song.json"), join(root, "project/music/demo.song.json"));
  await buildDaw({ wasm: false });
  s = await startServer({ port: 0, root, build: false, dawDist: DIST, headless: false });
  const { chromium } = await import("playwright");
  const channel = process.env.CHROME_CHANNEL ?? "chrome";
  browser = await chromium.launch({ channel: channel === "bundled" ? undefined : channel, headless: true, args: ["--autoplay-policy=no-user-gesture-required"] });
  page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
}, 300_000);

afterAll(async () => {
  await browser?.close();
  await s?.stop();
  rmSync(root, { recursive: true, force: true });
});

test("playing an automated song changes no saved value and adds no undo step; the automated knob follows", async () => {
  await page.goto(`${s.url}daw/?song=project/music/demo.song.json&tab=dev`);
  await page.waitForSelector(".thdr.track");
  await page.locator('.thdr.track[data-track="bass"]').click();
  const knob = page.locator("#p-dev .knob.auto").first();
  await knob.waitFor();
  const dbg = () => page.evaluate(() => { const d = (window as any).DAW.debug; return { song: JSON.stringify(d.song()), undo: d.undoDepth() }; });
  const before = await dbg();
  const seen = new Set<string>();
  await page.keyboard.press("Space");
  for (let i = 0; i < 20; i++) { seen.add((await knob.getAttribute("aria-valuenow"))!); await page.waitForTimeout(150); }
  await page.keyboard.press("Space");
  expect(seen.size).toBeGreaterThan(1);           // the knob moved with its lane
  const after = await dbg();
  expect(after.undo).toBe(before.undo);
  expect(after.song).toBe(before.song);
  expect(errors).toEqual([]);
}, 30_000);
