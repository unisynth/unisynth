// Library: converted legacy entries load into a track (change add-synth-daw task 5.7b). A Tunefish program listed
// from the lab and converted in project/music/.library.json (here a fixture: instrument + one insert delay) loads
// into the selected track as its instrument followed by the insert, as one undo step; an unconverted entry stays
// disabled and says how to convert. Headless Chrome against the real server on temporary dirs.
import { afterAll, beforeAll, expect, test } from "bun:test";
import { cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Browser, Page } from "playwright";
import { startServer, type Server } from "../../cli/src/server.ts";
import { buildDaw, DIST } from "../build.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const root = mkdtempSync(join(tmpdir(), "daw-lib-"));
const lab = mkdtempSync(join(tmpdir(), "daw-lab-"));
let s: Server;
let browser: Browser;
let page: Page;
const errors: string[] = [];
const labWas = process.env.SYNTH_LAB;

beforeAll(async () => {
  mkdirSync(join(root, "project/music"), { recursive: true });
  cpSync(join(ROOT, "project/music/demo.song.json"), join(root, "project/music/demo.song.json"));
  const progs = join(lab, "tunefish/patches/tf4programs");
  mkdirSync(progs, { recursive: true });
  writeFileSync(join(progs, "program7.txt"), "Converted Lead\n");
  writeFileSync(join(progs, "program8.txt"), "Not Converted\n");
  writeFileSync(
    join(root, "project/music/.library.json"),
    JSON.stringify({
      format: "us-library/1",
      tunefish: { "7": { devices: [{ kind: 64, values: { "osc1.type": 2 / 12, "filt1.type": 1 / 10, "filt1.cutoff": 0.3 } }, { kind: 68, values: { mode: 0 } }], warnings: ["fixture note"] } },
      wavesabre: {},
    }),
  );
  process.env.SYNTH_LAB = lab;
  await buildDaw({ wasm: false });
  s = await startServer({ port: 0, root, build: false, dawDist: DIST, headless: false });
  const { chromium } = await import("playwright");
  const channel = process.env.CHROME_CHANNEL ?? "chrome";
  browser = await chromium.launch({ channel: channel === "bundled" ? undefined : channel, headless: true });
  page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.on("pageerror", (e) => errors.push(e.message));
}, 300_000);

afterAll(async () => {
  if (labWas === undefined) delete process.env.SYNTH_LAB;
  else process.env.SYNTH_LAB = labWas;
  await browser?.close();
  await s?.stop();
  rmSync(root, { recursive: true, force: true });
  rmSync(lab, { recursive: true, force: true });
});

test("a converted Tunefish program loads as instrument + insert effects; an unconverted one is disabled", async () => {
  await page.goto(`${s.url}daw/?song=project/music/demo.song.json`);
  await page.waitForSelector(".thdr.track");
  await page.locator('.thdr.track[data-track="lead"]').click();
  await page.locator("#lib-q").fill("Converted");
  await page.locator('#lib-tree button:has-text("Not Converted")').click();
  await expect(page.locator("#lib-insp .loadin").isDisabled()).resolves.toBe(true);
  expect(await page.locator("#lib-insp").textContent()).toContain("example library");
  await page.locator('#lib-tree button:has-text("Converted Lead")').click();
  expect(await page.locator("#lib-insp").textContent()).toContain("instrument + 1 insert effect");
  await page.locator("#lib-insp .loadin").click();
  await page.waitForTimeout(300);
  // the device panel shows the converted instrument then the delay before the lead's own effects
  const panel = await page.locator("#p-dev").textContent();
  expect(panel).toContain("Converted Lead");
  expect(panel).toMatch(/Delay/);
  // one undo step back to the demo's lead
  await page.locator('[aria-label="Undo"]').click();
  await page.waitForTimeout(200);
  expect(await page.locator("#p-dev").textContent()).not.toContain("Converted Lead");
  expect(errors).toEqual([]);
}, 120_000);
