// Piano roll (spec synth-daw; contract for the SolidJS port, change daw-solid-ts 7.1): what editing.test.ts and
// controllers.test.ts do not cover. Note draw / delete by double-click, click + Shift-click + marquee selection, drag
// move (snapped, clamped to the clip), velocity lane (one stem, sweep paint), quantize (strength) and legato, zoom
// keeps the step under the pointer, opening a clip scrolls to its notes, a long clip (118 bars, 950 notes) opens
// fast. Notes are checked in the song/1 source: [tick, length, pitch, velocity], 240 ticks per 1/16 step.
import { afterAll, beforeAll, expect, test } from "bun:test";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Browser, Page } from "playwright";
import { startServer, type Server } from "../../cli/src/server.ts";
import { buildDaw, DIST } from "../build.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const root = mkdtempSync(join(tmpdir(), "daw-roll-"));
let s: Server, browser: Browser, page: Page;
const errors: string[] = [];
const STEP = 22, KH = 12, HI = 96, VH = 84;  // default zoom: px per step, px per key row; top key; velocity lane height

beforeAll(async () => {
  mkdirSync(join(root, "project/music"), { recursive: true });
  const demo = JSON.parse(readFileSync(join(ROOT, "project/music/demo.song.json"), "utf8"));
  writeFileSync(join(root, "project/music/demo.song.json"), JSON.stringify(demo, null, 1));
  // a long clip: 118 bars of 950 notes on the Lead (one placement)
  const long = structuredClone(demo), notes: number[][] = [];
  for (let i = 0; i < 950; i++) notes.push([Math.floor((i * 118 * 3840) / 950), 200, 48 + ((i * 7) % 36), 60 + (i % 60)]);
  long.clips.push({ id: "long", length: 118 * 3840, notes });
  long.tracks.find((t: any) => t.id === "lead").placements = [{ clip: "long", start: 0, length: 118 * 3840, loop: false }];
  long.length = 118 * 3840;
  writeFileSync(join(root, "project/music/long.song.json"), JSON.stringify(long));
  await buildDaw({ wasm: false });
  s = await startServer({ port: 0, root, build: false, dawDist: DIST, headless: false });
  const { chromium } = await import("playwright");
  const channel = process.env.CHROME_CHANNEL ?? "chrome";
  browser = await chromium.launch({ channel: channel === "bundled" ? undefined : channel, headless: true });
  page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
}, 300_000);
afterAll(async () => {
  await browser?.close();
  await s?.stop();
  rmSync(root, { recursive: true, force: true });
});

const song = (): Promise<any> => page.evaluate(() => (window as any).DAW.debug.song());
const depth = (): Promise<number> => page.evaluate(() => (window as any).DAW.debug.undoDepth());
const notes = async (): Promise<number[][]> => (await song()).clips[0].notes;

/** A new song, its 1-bar clip open in the roll, scrolled so key 70 is the top row; returns the pixel of (step, key). */
async function fresh() {
  await page.goto(`${s.url}daw/?song=project/music/demo.song.json`);
  await page.waitForSelector(".thdr.track");
  await page.locator("#new-btn").click();
  await page.waitForTimeout(300);
  const lane = (await page.locator(".lane.track").first().boundingBox())!;
  await page.mouse.dblclick(lane.x + 10, lane.y + lane.height / 2);
  await page.waitForTimeout(300);
  const top = await page.evaluate(() => { const g = document.getElementById("grid")!; g.scrollLeft = 0; g.scrollTop = (96 - 70) * 12; return g.scrollTop; });
  await page.waitForTimeout(100);
  const gr = (await page.locator("#grid").boundingBox())!;
  return (step: number, key: number) => [gr.x + step * STEP, gr.y + (HI - key) * KH - top + KH / 2] as const;
}
async function drag(a: readonly [number, number], b: readonly [number, number]) {
  await page.mouse.move(...a); await page.mouse.down(); await page.mouse.move(...b, { steps: 5 }); await page.mouse.up();
}
const status = () => page.locator("#edit-status").textContent();

test("draw, select, move, delete: double-click adds a note on the grid, double-click on it deletes; one undo step each", async () => {
  const at = await fresh();
  const d0 = await depth();
  await page.mouse.dblclick(...at(2.5, 60)); await page.waitForTimeout(450);
  await page.mouse.dblclick(...at(6.5, 64)); await page.waitForTimeout(450);
  expect((await notes()).map((n) => [n[0], n[1], n[2]])).toEqual([[480, 240, 60], [1440, 240, 64]]);
  expect(await depth()).toBe(d0 + 2);
  // click selects one note (the other drops out); Delete removes the selection
  await page.mouse.click(...at(2.5, 60)); await page.waitForTimeout(450);
  await page.keyboard.press("Delete");
  expect((await notes()).map((n) => n[2])).toEqual([64]);
  await page.keyboard.press("Control+z");
  // Shift-click adds to the selection
  await page.mouse.click(...at(2.5, 60)); await page.waitForTimeout(450);
  await page.keyboard.down("Shift"); await page.mouse.click(...at(6.5, 64)); await page.keyboard.up("Shift"); await page.waitForTimeout(450);
  await page.keyboard.press("Delete");
  expect(await notes()).toEqual([]);
  await page.keyboard.press("Control+z");
  // marquee from an empty spot around both notes
  await drag(at(1, 66), at(9, 58));
  expect(await status()).toBe("2 notes selected");
  await page.keyboard.press("Escape");
  // drag a note: snapped to steps, one row up, one undo step; clamped at the clip end
  const d1 = await depth();
  await drag(at(2.5, 60), at(4.6, 61));
  expect((await notes()).find((n) => n[1] === 240 && n[0] !== 1440)!.slice(0, 3)).toEqual([960, 240, 61]);
  expect(await depth()).toBe(d1 + 1);
  await drag(at(4.5, 61), at(40, 61));
  expect((await notes()).find((n) => n[2] === 61)![0]).toBe(15 * 240);  // last step of the 16-step clip
  await page.mouse.dblclick(...at(15.5, 61)); await page.waitForTimeout(450);
  expect((await notes()).map((n) => n[2])).toEqual([64]);
}, 60_000);

test("velocity lane: drag one stem, sweep-paint from an empty spot; one undo step each", async () => {
  const at = await fresh();
  await page.mouse.dblclick(...at(2.5, 60)); await page.waitForTimeout(450);
  await page.mouse.dblclick(...at(6.5, 64)); await page.waitForTimeout(450);
  await page.keyboard.press("Escape");
  expect(await page.locator("#cc-kind").inputValue()).toBe("vel");
  const v = (await page.locator("#velc").boundingBox())!, y = (val: number) => v.y + VH - 4 - val * (VH - 10), x = (step: number) => v.x + step * STEP + 3.5;
  const d0 = await depth();
  await drag([x(2), y(0.5)], [x(2), y(1)]);  // the first note's stem, up to the top
  expect((await notes()).map((n) => n[3])).toEqual([127, (await notes())[1]![3]]);
  expect(await depth()).toBe(d0 + 1);
  await drag([x(0.5), y(0.2)], [x(10), y(0.2)]);  // a sweep across both stems
  const vs = (await notes()).map((n) => n[3]);
  expect(vs.every((q) => q > 15 && q < 35)).toBe(true);
  expect(await depth()).toBe(d0 + 2);
  await page.keyboard.press("Control+z");
  expect((await notes())[0]![3]).toBe(127);
}, 60_000);

test("quantize (100% and 50%) and legato, on the selection or every note", async () => {
  const at = await fresh();
  await page.mouse.dblclick(...at(2.5, 60)); await page.waitForTimeout(450);
  await page.mouse.dblclick(...at(8.5, 64)); await page.waitForTimeout(450);
  // Snap Off: a drag lands off the grid
  await page.locator("#snap").selectOption("Off");
  await drag(at(2.5, 60), at(2.9, 60));
  const s0 = (await notes()).find((n) => n[2] === 60)![0]!;
  expect(s0 % 240).not.toBe(0);
  await page.locator("#snap").selectOption("1/16");
  await page.locator("#qstr").selectOption(".5");
  await page.locator('#roll-tools [data-roll="quant"]').click();  // the selected note, halfway to the grid
  const target = Math.round(s0 / 240) * 240, s1 = (await notes()).find((n) => n[2] === 60)![0]!;
  expect(Math.abs(s1 - (s0 + (target - s0) / 2))).toBeLessThanOrEqual(1);
  await page.locator("#qstr").selectOption("1");
  await page.locator('#roll-tools [data-roll="quant"]').click();
  expect((await notes()).find((n) => n[2] === 60)![0]).toBe(target);
  // legato with nothing selected: every note reaches the next start, the last one the clip end
  await page.locator("#gridc").press("Escape");
  await page.locator('#roll-tools [data-roll="legato"]').click();
  const ns = (await notes()).slice().sort((a, b) => a[0]! - b[0]!);
  expect(ns.map((n) => n[0]! + n[1]!)).toEqual([ns[1]![0], 16 * 240]);
}, 60_000);

test("opening a clip scrolls its notes into view", async () => {
  await page.goto(`${s.url}daw/?song=project/music/demo.song.json`);
  await page.waitForSelector(".thdr.track");
  const lane = (await page.locator('.lane.track[data-track="bass"]').boundingBox())!;
  await page.mouse.dblclick(lane.x + 44 + 10, lane.y + lane.height * 0.7);
  await page.waitForTimeout(300);
  // the bass clip's notes (C2–C3, keys 36–48) are inside the visible rows
  const vis = await page.evaluate(() => { const g = document.getElementById("grid")!; return [96 - (g.scrollTop + g.clientHeight) / 12, 96 - g.scrollTop / 12]; });
  expect(vis[0]).toBeLessThanOrEqual(36);
  expect(vis[1]).toBeGreaterThanOrEqual(48);
}, 60_000);

/** Opens the long clip (Lead, 118 bars) in the roll. */
async function openLong(): Promise<void> {
  await page.goto(`${s.url}daw/?song=project/music/long.song.json`);
  await page.waitForSelector(".thdr.track");
  await page.waitForTimeout(300);
  const lane = (await page.locator('.lane.track[data-track="lead"]').boundingBox())!;
  await page.mouse.dblclick(lane.x + 20, lane.y + lane.height * 0.7);
  await page.waitForTimeout(300);
}

test("Ctrl+wheel zoom keeps the step under the pointer", async () => {
  await openLong();
  await page.evaluate(() => { document.getElementById("grid")!.scrollLeft = 3000; });
  await page.waitForTimeout(100);
  const gr = (await page.locator("#grid").boundingBox())!, px = 300;
  const frac = () => page.evaluate((x) => { const g = document.getElementById("grid")!; return [(g.scrollLeft + x) / g.scrollWidth, g.scrollWidth]; }, px);
  const [f0, w0] = await frac();
  await page.mouse.move(gr.x + px, gr.y + 100);
  await page.keyboard.down("Control"); await page.mouse.wheel(0, -300); await page.keyboard.up("Control");
  await page.waitForTimeout(150);
  const [f1, w1] = await frac();
  expect(w1).toBeGreaterThan(w0! * 1.1);  // zoomed in
  expect(Math.abs(f1! - f0!) * 118 * 16).toBeLessThan(0.5);  // within half a step
}, 60_000);

test("a 118-bar clip with 950 notes opens in the roll fast and draws only the visible part", async () => {
  await page.goto(`${s.url}daw/?song=project/music/long.song.json`);
  await page.waitForSelector(".thdr.track");
  await page.waitForTimeout(300);
  // from the double-click's last pointerdown to two frames after the roll panel shows
  await page.evaluate(() => {
    const w = window as any, roll = document.getElementById("p-roll")!;
    document.addEventListener("pointerdown", () => { w.__t0 = performance.now(); }, true);
    new MutationObserver(() => { if (!roll.hidden && w.__t1 == null) requestAnimationFrame(() => requestAnimationFrame(() => { w.__t1 = performance.now(); })); })
      .observe(roll, { attributes: true, attributeFilter: ["hidden"] });
  });
  const lane = (await page.locator('.lane.track[data-track="lead"]').boundingBox())!;
  await page.mouse.dblclick(lane.x + 20, lane.y + lane.height * 0.7);
  await page.waitForFunction(() => (window as any).__t1 != null);
  const ms: number = await page.evaluate(() => (window as any).__t1 - (window as any).__t0);
  expect(await page.locator("#p-roll").isHidden()).toBe(false);
  const w = await page.evaluate(() => [parseFloat((document.getElementById("gridc") as HTMLCanvasElement).style.width), document.getElementById("grid")!.clientWidth, document.getElementById("grid")!.scrollWidth]);
  expect(w[0]).toBeLessThanOrEqual(w[1]! + 1);
  expect(w[2]).toBeGreaterThan(118 * 16 * 3);  // the scroll range spans the clip
  console.log(`long clip opens in ${ms.toFixed(0)} ms`);
  expect(ms).toBeLessThan(1000);  // about 0.3 s before the port (design: open time ≤ baseline)
}, 60_000);

test("no page errors", () => expect(errors).toEqual([]));
