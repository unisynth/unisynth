// Audition, zoom and splitters (contract for the SolidJS port, change daw-solid-ts 8.2): the footer's audition
// controls (armed track, ⌨, Web MIDI, MW/PB), anchored arrangement zoom, and the two splitters with their persisted
// sizes. MW/PB to the engine: controllers.test.ts; roll Ctrl+wheel: roll.test.ts; keys: keys.test.ts.
// Headless Chrome against the real server on a temporary project/music; Web MIDI is a stub with one input.
import { afterAll, beforeAll, expect, test } from "bun:test";
import { cpSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Browser, Page } from "playwright";
import { startServer, type Server } from "../../cli/src/server.ts";
import { buildDaw, DIST } from "../build.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const root = mkdtempSync(join(tmpdir(), "daw-ctl-"));
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
  // Web MIDI stub: one input; the test sends through window.__midi
  await page.addInitScript(() => {
    const input: any = { onmidimessage: null };
    (window as any).__midi = (...data: number[]) => input.onmidimessage?.({ data: new Uint8Array(data) });
    (navigator as any).requestMIDIAccess = () => Promise.resolve({ inputs: new Map([["in1", input]]), onstatechange: null });
  });
}, 300_000);

afterAll(async () => {
  await browser?.close();
  await s?.stop();
  rmSync(root, { recursive: true, force: true });
});

async function fresh() {
  await page.goto(`${s.url}daw/?song=project/music/demo.song.json`);
  await page.waitForSelector(".thdr.track");
}
const text = (sel: string) => page.locator(sel).textContent();
const depth = (): Promise<number> => page.evaluate(() => (window as any).DAW.debug.undoDepth());
const now = (sel: string) => page.locator(sel).getAttribute("aria-valuenow").then(Number);

test("the armed track follows selection; ⌨ toggles the computer keyboard and says so", async () => {
  await fresh();
  expect(await text("#armed-info")).toBe("Armed: Kick · octave C3 · vel 100");  // the first track
  await page.locator('.thdr.track[data-track="lead"] .name').click();
  expect(await text("#armed-info")).toBe("Armed: Lead · octave C3 · vel 100");
  expect(await page.locator("#kb-btn").getAttribute("aria-pressed")).toBe("true");
  await page.locator("#kb-btn").click();
  expect(await page.locator("#kb-btn").getAttribute("aria-pressed")).toBe("false");
  expect(await text("#live")).toBe("computer keyboard off: letters are shortcuts again");
  await page.locator("#kb-btn").click();
  expect(await text("#live")).toBe("computer keyboard plays the selected track (Z–M, Q–P)");
}, 60_000);

test("Web MIDI: connect names the inputs and the armed track; notes, CC1 and pitch bend reach the audition", async () => {
  await fresh();
  await page.locator('.thdr.track[data-track="pad"] .name').click();
  await page.locator("#tab-roll").click();
  await page.locator("#midi-info").click();
  await page.waitForFunction(() => document.getElementById("midi-info")!.textContent!.startsWith("MIDI: 1 input"));
  expect(await text("#midi-info")).toBe("MIDI: 1 input → Pad");
  const d = await depth();
  await page.evaluate(() => (window as any).__midi(0x90, 60, 100));
  await page.waitForTimeout(100);
  expect(await page.locator("#keys button.on").getAttribute("aria-label")).toBe("Audition C4");
  await page.evaluate(() => (window as any).__midi(0x80, 60, 0));
  expect(await page.locator("#keys button.on").count()).toBe(0);
  await page.evaluate(() => (window as any).__midi(0x90, 62, 90));
  await page.evaluate(() => (window as any).__midi(0x90, 62, 0));  // note-on with velocity 0 = off
  expect(await page.locator("#keys button.on").count()).toBe(0);
  await page.evaluate(() => (window as any).__midi(0xb0, 1, 64));
  expect(await page.locator("#mw").inputValue()).toBe("64");
  await page.evaluate(() => (window as any).__midi(0xe0, 0, 96));  // (96 << 7) - 8192 = 4096
  expect(await page.locator("#pb").inputValue()).toBe("4096");
  expect(await depth()).toBe(d);  // audition is never recorded
}, 60_000);

test("Ctrl+wheel zooms the arrangement at the pointer: the bar under it stays", async () => {
  await fresh();
  for (let i = 0; i < 4; i++) await page.locator("#zin").click();  // the 8-bar song wider than the view, so it can scroll
  await page.evaluate(() => { document.getElementById("lanes-wrap")!.scrollLeft = 300; });
  const lw = page.locator("#lanes-wrap"), r = (await lw.boundingBox())!;
  const barAt = (x: number) => page.evaluate((x) => {
    const el = document.getElementById("lanes-wrap")!, bw = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--bar-w"));
    return 1 + (el.scrollLeft + x - el.getBoundingClientRect().left) / bw;
  }, x);
  const x = r.x + 300, b0 = await barAt(x);
  await page.mouse.move(x, r.y + 40);
  await page.keyboard.down("Control"); await page.mouse.wheel(0, -300); await page.keyboard.up("Control");
  await page.waitForTimeout(100);
  expect(parseFloat(await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--bar-w")))).toBeGreaterThan(44);
  expect(Math.abs((await barAt(x)) - b0)).toBeLessThan(0.05);
}, 60_000);

test("splitters: arrows (Shift = coarser) and drag resize within limits; sizes survive a reload", async () => {
  await page.evaluate(() => { localStorage.removeItem("daw--browser-w"); localStorage.removeItem("daw--arr-h"); });
  await fresh();
  const v = page.locator("#split-v"), h = page.locator("#split-h");
  expect(await v.getAttribute("aria-valuemin")).toBe("140");
  expect(await v.getAttribute("aria-valuemax")).toBe("360");
  expect(await h.getAttribute("aria-valuemin")).toBe("120");
  expect(await h.getAttribute("aria-valuemax")).toBe("700");
  const v0 = await now("#split-v");
  await v.focus(); await page.keyboard.press("ArrowRight");
  expect(await now("#split-v")).toBe(v0 + 10);
  await page.keyboard.press("Shift+ArrowLeft");
  expect(await now("#split-v")).toBe(v0 - 30);
  for (let i = 0; i < 12; i++) await page.keyboard.press("Shift+ArrowRight");
  expect(await now("#split-v")).toBe(360);
  const hb = (await h.boundingBox())!;
  await page.mouse.move(hb.x + hb.width / 2, hb.y + hb.height / 2); await page.mouse.down();
  await page.mouse.move(hb.x + hb.width / 2, hb.y - 80, { steps: 4 }); await page.mouse.up();
  const h1 = await now("#split-h");
  expect(h1).toBeGreaterThanOrEqual(120);
  expect(h1).toBeLessThan(700);
  expect((await page.locator("#arr").boundingBox())!.height).toBeCloseTo(h1, -1);
  await fresh();
  expect(await now("#split-v")).toBe(360);
  expect(await now("#split-h")).toBe(h1);
}, 60_000);

test("no page errors", () => {
  expect(errors).toEqual([]);
});
