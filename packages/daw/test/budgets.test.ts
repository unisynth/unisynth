// Spec budgets on a 50-track song (change daw-solid-ts 9.2; spec "Edit response budget", "Playback display budget").
// Edits: each is timed in the page from the event to the end of its display update (handler, Solid updates, microtask
// commits, then a forced layout), median of 7, against one 60 Hz frame; a MutationObserver checks that only the
// controls showing the changed value change. Playback: 5 s with the mixer, then the device panel (live view on), shown;
// ≥ 95 % of frame intervals under 20 ms, nothing rebuilt. Headless Chrome against the real server, generated song
// (fixtures.ts `bigSong`: the demo's bass track 50 times).
import { afterAll, beforeAll, expect, test } from "bun:test";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Browser, Page } from "playwright";
import { startServer, type Server } from "../../cli/src/server.ts";
import { buildDaw, DIST } from "../build.ts";
import { BIG_TRACKS, bigSong, denseSong } from "./fixtures.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const root = mkdtempSync(join(tmpdir(), "daw-budget-"));
const music = join(root, "project/music");
let s: Server, browser: Browser, page: Page;
const errors: string[] = [];
const FRAME = 1000 / 60;
const report: string[] = [];

beforeAll(async () => {
  mkdirSync(music, { recursive: true });
  cpSync(join(ROOT, "project/music/demo.song.json"), join(music, "demo.song.json"));
  const demo = JSON.parse(readFileSync(join(music, "demo.song.json"), "utf8"));
  writeFileSync(join(music, "big.song.json"), JSON.stringify(bigSong(demo), null, 1));
  writeFileSync(join(music, "dense.song.json"), JSON.stringify(denseSong(demo)));
  await buildDaw({ wasm: false });
  s = await startServer({ port: 0, root, build: false, dawDist: DIST, headless: false });
  const { chromium } = await import("playwright");
  const channel = process.env.CHROME_CHANNEL ?? "chrome";
  browser = await chromium.launch({ channel: channel === "bundled" ? undefined : channel, headless: true, args: ["--autoplay-policy=no-user-gesture-required"] });
  page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  await page.addInitScript(() => localStorage.setItem("daw.verifySnap", "0"));  // measuring: no snapshot cross-check
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto(`${s.url}daw/?song=project/music/big.song.json&tab=mix`);
  await page.waitForSelector(".thdr.track");
  await page.waitForTimeout(300);
  expect(await page.evaluate(() => (window as any).DAW.debug.song().tracks.length)).toBe(BIG_TRACKS);
}, 300_000);
afterAll(async () => {
  console.log("budgets (median / max ms):\n  " + report.join("\n  "));
  await browser?.close();
  await s?.stop();
  rmSync(root, { recursive: true, force: true });
});

const strip = (name: string) => page.getByRole("group", { name: `${name} channel strip` });

/** Runs `edit` (page-side, given the run index) 7 times, a frame apart; each from the event to a forced layout. */
async function timeEdit<A>(name: string, edit: (i: number, arg: A) => void, arg?: A): Promise<number> {
  const times: number[] = await page.evaluate(async ([src, a]) => {
    const f = new Function("i", "a", `(${src})(i, a)`) as (i: number, a: unknown) => void, out: number[] = [];
    const frame = () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    for (let i = 0; i < 7; i++) {
      await frame();
      const t0 = performance.now(); f(i, a); await null; void document.body.offsetHeight;
      out.push(performance.now() - t0);
    }
    return out;
  }, [edit.toString(), arg] as const);
  const sorted = [...times].sort((a, b) => a - b), med = sorted[3]!;
  report.push(`${name.padEnd(12)} ${med.toFixed(2)} / ${sorted[6]!.toFixed(2)}`);
  return med;
}
/** Elements (by `key`) that mutate under `root` while `fn` runs. */
async function mutatedBy(root: string, key: string, fn: () => Promise<void>): Promise<string[]> {
  await page.evaluate(([r, k]) => {
    const w = window as any; w.__hit = new Set<string>();
    w.__mo = new MutationObserver((rs) => { for (const m of rs) { const el = (m.target.nodeType === 1 ? m.target : m.target.parentElement) as Element; w.__hit.add(el.closest(k)?.getAttribute("aria-label") ?? `(${el.tagName.toLowerCase()}.${el.getAttribute("class") ?? ""})`); } });
    w.__mo.observe(document.querySelector(r)!, { subtree: true, childList: true, attributes: true, characterData: true });
  }, [root, key]);
  await fn();
  return page.evaluate(() => { const w = window as any; w.__mo.takeRecords(); w.__mo.disconnect(); return [...w.__hit] as string[]; });
}
/** A native range input's key step (`a` = selector): value ±0.1, then input and change (as the browser does). */
const rangeStep = (i: number, sel: string): void => {
  const el = document.querySelector(sel) as HTMLInputElement; el.value = String(+el.value + (i % 2 ? -0.1 : 0.1));
  el.dispatchEvent(new Event("input", { bubbles: true })); el.dispatchEvent(new Event("change", { bubbles: true }));
};

test("select a track from its mixer strip: selection and strips within one frame", async () => {
  const ms = await timeEdit("select", (i) => (document.querySelector(`#mixer [aria-label="Select Track ${7 + (i % 2)}"]`) as HTMLElement).click());
  expect(ms).toBeLessThan(FRAME);
  expect(await page.evaluate(() => (window as any).DAW.debug.selected())).toBe("t7");
  expect(await strip("Track 7").getAttribute("class")).toContain("sel");
  expect(await strip("Track 8").getAttribute("class")).not.toContain("sel");
});

test("fader step: one frame; its dB readout and the header fader follow, no other strip changes", async () => {
  const sel = '#mixer [aria-label="Track 3 volume"]';
  const v0 = Number(await page.locator(sel).inputValue());
  let ms = 0;
  const hit = await mutatedBy("#mixer", '[role="group"]', async () => { ms = await timeEdit("fader", rangeStep, sel); });
  expect(ms).toBeLessThan(FRAME);
  expect(hit).toEqual(["Track 3 channel strip"]);
  const v = Number(await page.locator(sel).inputValue());
  expect(v).toBeCloseTo(v0 + 0.1, 5);  // 7 alternating steps end one step up
  expect(await strip("Track 3").locator(".db").textContent()).toBe(`${v.toFixed(1)} dB`);
  expect(Number(await page.locator('.thdr.track[data-track="t3"] input[aria-label="Track 3 volume"]').inputValue())).toBeCloseTo(v, 5);
});

test("mute and solo: one frame each, only that strip changes", async () => {
  let ms = 0;
  const hitM = await mutatedBy("#mixer", '[role="group"]', async () => { ms = await timeEdit("mute", () => (document.querySelector('#mixer [aria-label="Mute Track 4"]') as HTMLElement).click()); });
  expect(ms).toBeLessThan(FRAME);
  expect(hitM).toEqual(["Track 4 channel strip"]);
  await page.waitForTimeout(50);  // the mute commits are deferred
  const hitS = await mutatedBy("#mixer", '[role="group"]', async () => { ms = await timeEdit("solo", () => (document.querySelector('#mixer [aria-label="Solo Track 5"]') as HTMLElement).click()); });
  expect(ms).toBeLessThan(FRAME);
  expect(hitS).toEqual(["Track 5 channel strip"]);
  expect(await strip("Track 4").getByRole("button", { name: "Mute Track 4" }).getAttribute("aria-pressed")).toBe("true");  // 7 toggles
  expect(await strip("Track 5").getByRole("button", { name: "Solo Track 5" }).getAttribute("aria-pressed")).toBe("true");
});

test("send level step: one frame, only the sending strip changes", async () => {
  await strip("Track 1").getByRole("button", { name: "Add send from Track 1" }).click();
  await strip("Track 1").getByRole("combobox", { name: "Send target for Track 1" }).selectOption("Track 2|main");
  const sel = '#mixer [aria-label="Send Track 1 to Track 2 main level"]';
  await page.waitForSelector(sel);
  let ms = 0;
  const hit = await mutatedBy("#mixer", '[role="group"]', async () => { ms = await timeEdit("send level", rangeStep, sel); });
  expect(ms).toBeLessThan(FRAME);
  expect(hit).toEqual(["Track 1 channel strip"]);
  expect(await page.locator(sel).getAttribute("aria-valuetext")).toBe("-11.9 dB");
});

// The instrument header's "preset init" gains its `*` at the first edit (the preset's edited state changed).
test("knob step in the device panel: one frame, only the knob, its filter screen and the preset's edited mark change", async () => {
  await page.locator("#tab-dev").click();
  await page.locator('.thdr.track[data-track="t2"]').click();
  const knob = '#device [data-param="filt1.resonance"]';
  await page.waitForSelector(knob);
  let ms = 0;
  const hit = await mutatedBy("#device", `[data-param], svg.scr`, async () => {
    ms = await timeEdit("knob", (i) => { const k = document.querySelector('#device [data-param="filt1.resonance"]') as HTMLElement;
      k.focus(); k.dispatchEvent(new KeyboardEvent("keydown", { key: i % 2 ? "ArrowDown" : "ArrowUp", bubbles: true })); });
  });
  expect(ms).toBeLessThan(FRAME);
  const label = await page.locator(knob).getAttribute("aria-label");
  expect(hit.filter((h) => h !== label && !["(svg.scr)", "(span.kind)"].includes(h))).toEqual([]);
});

// The Device tab rebuilds its panel for the selected track (design D5); that rebuild is the edit's display update.
test("select a track with the Device tab shown: one frame, the panel shows the selected track's chain", async () => {
  await page.locator("#tab-dev").click();
  const ms = await timeEdit("select dev", (i) => (document.querySelector(`.thdr.track[data-track="t${2 + (i % 2)}"]`) as HTMLElement).click());
  expect(ms).toBeLessThan(FRAME);
  expect(await page.evaluate(() => (window as any).DAW.debug.selected())).toBe("t2");
  expect(await page.locator('#device [data-param="filt1.resonance"]').count()).toBe(1);
  expect(await page.locator("#device .mm select").count()).toBeGreaterThan(0);  // the mod matrix is part of the panel
});

/** Plays 5 s with `tab` shown; frame intervals and whether the given elements were replaced. */
async function play5s(tab: string, keep: string): Promise<{ n: number; under: number; p95: number; same: boolean }> {
  await page.locator(tab).click();
  await page.waitForTimeout(300);
  const r = await page.evaluate(async (k) => {
    const before = Array.from(document.querySelectorAll(k)), iv: number[] = [];
    (document.getElementById("play") as HTMLElement).click();
    await new Promise<void>((done) => { let last = performance.now(); const t0 = last;
      const f = () => { const now = performance.now(); iv.push(now - last); last = now; if (now - t0 < 5000) requestAnimationFrame(f); else done(); };
      requestAnimationFrame(f); });
    const after = Array.from(document.querySelectorAll(k));
    return { iv, same: before.length > 0 && before.length === after.length && before.every((el, i) => el === after[i]) };
  }, keep);
  await page.locator("#play").click();  // pause
  await page.waitForTimeout(300);
  const iv = r.iv.slice(1), sorted = [...iv].sort((a, b) => a - b);
  return { n: iv.length, under: iv.filter((x) => x < 20).length / iv.length, p95: sorted[Math.floor(sorted.length * .95)]!, same: r.same };
}

test("playback with the mixer shown: ≥ 95 % of frames under 20 ms, only meters change, no strip rebuilt", async () => {
  await page.locator("#tab-mix").click();  // mounted before observing: only playback's changes count
  let r = { n: 0, under: 0, p95: 0, same: false };
  const hit = await mutatedBy("#mixer", ".meter", async () => { r = await play5s("#tab-mix", '#mixer [role="group"]'); });
  report.push(`play-mix     ${(r.under * 100).toFixed(1)} % < 20 ms, p95 ${r.p95.toFixed(2)} (${r.n} frames)`);
  expect(r.n).toBeGreaterThan(200);
  expect(r.under).toBeGreaterThanOrEqual(0.95);
  expect(r.same).toBe(true);
  expect(hit.length).toBeGreaterThan(0);  // Track 5 (soloed above) sounds
  expect(hit.filter((h) => !/ level (left|right)$/.test(h))).toEqual([]);
}, 30_000);

test("playback with the device panel shown and the live view on: ≥ 95 % of frames under 20 ms, no knob rebuilt", async () => {
  expect(await page.evaluate(() => localStorage.getItem("daw.live"))).not.toBe("0");
  const r = await play5s("#tab-dev", "#device [data-param]");
  report.push(`play-dev     ${(r.under * 100).toFixed(1)} % < 20 ms, p95 ${r.p95.toFixed(2)} (${r.n} frames)`);
  expect(r.n).toBeGreaterThan(200);
  expect(r.under).toBeGreaterThanOrEqual(0.95);
  expect(r.same).toBe(true);
  expect(errors).toEqual([]);
}, 30_000);

test("dense automation (≈ 6 MB of lane points, like tbl-stash-over2): knob step and mute each within one frame", async () => {
  await page.goto(`${s.url}daw/?song=project/music/dense.song.json&tab=dev`);
  await page.waitForSelector(".thdr.track");
  await page.waitForTimeout(500);
  const size = await page.evaluate(() => JSON.stringify((window as any).DAW.debug.song().tracks.map((t: any) => t.lanes)).length);
  expect(size).toBeGreaterThan(4e6);
  await page.locator('.thdr.track[data-track="t2"]').click();
  await page.waitForSelector('#device [data-param="filt1.resonance"]');
  const knob = await timeEdit("dense knob", (i) => { const k = document.querySelector('#device [data-param="filt1.resonance"]') as HTMLElement;
    k.focus(); k.dispatchEvent(new KeyboardEvent("keydown", { key: i % 2 ? "ArrowDown" : "ArrowUp", bubbles: true })); });
  const d0: number = await page.evaluate(() => (window as any).DAW.debug.undoDepth());
  const mute = await timeEdit("dense mute", () => (document.querySelector('.thdr.track[data-track="t3"] [aria-label^="Mute"]') as HTMLElement).click());
  await page.waitForTimeout(50);  // deferred mute commits
  expect(await page.evaluate(() => (window as any).DAW.debug.undoDepth())).toBe(d0 + 7);
  expect(knob).toBeLessThan(FRAME);
  expect(mute).toBeLessThan(FRAME);
  expect(errors).toEqual([]);
}, 60_000);
// Undo on the dense song: the snapshot side is cheap since 9.3 (per-track restore, ~9 ms reconcile), but a restore
// rebuilds the mounted rows and redraws every automation lane as an SVG with one element per point (7 000 here):
// ~1.4 s. Lane rendering, not undo state; see openspec daw-solid-ts tasks.
test.todo("dense automation: undo of one step within one frame");

