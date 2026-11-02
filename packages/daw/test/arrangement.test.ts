// Arrangement contract (spec synth-daw; SolidJS port, change daw-solid-ts 5.1): what editing.test.ts does not cover.
// Header controls (select, arm, automation toggle, rename, keyboard between tracks), clip gestures (double-click
// create, drag the title to move, drag the edge to resize, Alt+edge to loop, marquee), duplicate and align, automation
// point editing (mouse and keyboard, target, add/remove lane), follow paging, and a generated 50-track song.
// Each gesture is one undo step. Headless Chrome against the real server on a temporary project/music.
import { afterAll, beforeAll, expect, test } from "bun:test";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Browser, Page } from "playwright";
import { startServer, type Server } from "../../cli/src/server.ts";
import { buildDaw, DIST } from "../build.ts";
import { BIG_TRACKS, bigSong } from "./fixtures.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const root = mkdtempSync(join(tmpdir(), "daw-arr-"));
const music = join(root, "project/music");
let s: Server, browser: Browser, page: Page;
const errors: string[] = [];
const BW = 44; // px per bar at the default zoom

beforeAll(async () => {
  mkdirSync(music, { recursive: true });
  cpSync(join(ROOT, "project/music/demo.song.json"), join(music, "demo.song.json"));
  writeFileSync(join(music, "big.song.json"), JSON.stringify(bigSong(JSON.parse(readFileSync(join(music, "demo.song.json"), "utf8"))), null, 1));
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

async function open(file = "demo") {
  await page.goto(`${s.url}daw/?song=project/music/${file}.song.json`);
  await page.waitForSelector(".thdr.track");
  await page.waitForTimeout(200);
}
const song = (): Promise<any> => page.evaluate(() => (window as any).DAW.debug.song());
const track = (sg: any, id: string) => sg.tracks.find((t: any) => t.id === id);
const depth = (): Promise<number> => page.evaluate(() => (window as any).DAW.debug.undoDepth());
const selected = (): Promise<string> => page.evaluate(() => (window as any).DAW.debug.selected());
const hdr = (id: string) => page.locator(`.thdr.track[data-track="${id}"]`);
const lane = (id: string) => page.locator(`.lane.track[data-track="${id}"]`);
const clipEl = (id: string) => lane(id).locator(".clip").first();
const focusedTrack = () => page.evaluate(() => (document.activeElement as HTMLElement)?.dataset?.track);
/** Drag from (x0, y0) to (x1, y1) in steps (pointer events, as a hand would). */
async function drag(x0: number, y0: number, x1: number, y1: number, mods: string[] = []) {
  for (const m of mods) await page.keyboard.down(m);
  await page.mouse.move(x0, y0); await page.mouse.down();
  for (let i = 1; i <= 6; i++) await page.mouse.move(x0 + (x1 - x0) * i / 6, y0 + (y1 - y0) * i / 6);
  await page.mouse.up();
  for (const m of mods) await page.keyboard.up(m);
}
const bars = (ticks: number) => ticks / 3840;

test("headers: click selects (header + lane marked), arm is exclusive, the A button shows and hides the lanes", async () => {
  await open();
  await hdr("lead").click({ position: { x: 60, y: 10 } });
  expect(await selected()).toBe("lead");
  expect(await hdr("lead").getAttribute("aria-current")).toBe("true");
  expect(await lane("lead").evaluate((l) => l.classList.contains("sel"))).toBe(true);
  expect(await hdr("bass").getAttribute("aria-current")).toBeNull();
  // selecting an instrument track arms it; the arm button arms another
  expect(await hdr("lead").locator(".arm").getAttribute("aria-pressed")).toBe("true");
  await hdr("pad").locator(".arm").click();
  expect(await hdr("pad").locator(".arm").getAttribute("aria-pressed")).toBe("true");
  expect(await hdr("lead").locator(".arm").getAttribute("aria-pressed")).toBe("false");
  expect(await hdr("verb").locator(".arm").count()).toBe(0);  // bus: no arm
  // bass has one automation lane, open by default
  const a = hdr("bass").locator(".autobtn");
  expect(await a.textContent()).toBe("A1");
  expect(await a.getAttribute("aria-expanded")).toBe("true");
  expect(await page.locator(".lane.auto").count()).toBe(1);
  expect(await page.locator(".lane.auto").isVisible()).toBe(true);
  await a.click();
  expect(await a.getAttribute("aria-expanded")).toBe("false");
  expect(await page.locator(".lane.auto").isVisible()).toBe(false);
  await a.click();
  expect(await page.locator(".lane.auto").isVisible()).toBe(true);
  expect(await depth()).toBe(0);  // view-only
});

test("headers: double-click the name renames (Escape cancels); arrows move focus and selection between tracks", async () => {
  await hdr("pad").locator(".name").dblclick();
  const inp = hdr("pad").locator(".name input");
  await inp.fill("Strings"); await page.keyboard.press("Escape");
  expect(track(await song(), "pad").name).toBe("Pad");
  await hdr("pad").locator(".name").dblclick();
  await hdr("pad").locator(".name input").fill("Strings"); await page.keyboard.press("Enter");
  expect(track(await song(), "pad").name).toBe("Strings");
  expect(await hdr("pad").locator(".name").textContent()).toBe("Strings");
  await page.keyboard.press("Control+z");
  expect(track(await song(), "pad").name).toBe("Pad");
  await hdr("kick").focus();
  await page.keyboard.press("ArrowDown");
  expect(await focusedTrack()).toBe("bass");
  expect(await selected()).toBe("bass");
  await page.keyboard.press("ArrowUp"); await page.keyboard.press("ArrowUp");  // clamps at the first track
  expect(await focusedTrack()).toBe("kick");
});

test("clips: double-click an empty lane creates a one-bar clip there; double-click a clip opens the piano roll", async () => {
  const d0 = await depth();
  const b = (await lane("lead").boundingBox())!;
  await page.mouse.dblclick(b.x + 2 * BW + 10, b.y + b.height * 0.7);  // bar 3 (lead's clip starts at bar 5)
  const ps = track(await song(), "lead").placements;
  expect(ps.length).toBe(2);
  expect(ps.map((p: any) => [bars(p.start), bars(p.length)])).toContainEqual([2, 1]);
  expect(await depth()).toBe(d0 + 1);
  expect(await page.locator("#p-roll").isVisible()).toBe(true);
  await page.keyboard.press("Control+z");
  expect(track(await song(), "lead").placements.length).toBe(1);
  await page.locator("#tab-dev").click();
});

test("clips: drag the title moves (snapped, one undo); drag the edge resizes; Alt+edge loops", async () => {
  const cb = (await clipEl("lead").boundingBox())!;  // lead: bars 5–9
  const d0 = await depth();
  await drag(cb.x + 10, cb.y + 5, cb.x + 10 - 2 * BW, cb.y + 5);
  expect(bars(track(await song(), "lead").placements[0].start)).toBe(2);  // bar 3
  expect(await depth()).toBe(d0 + 1);
  await page.keyboard.press("Control+z");
  expect(bars(track(await song(), "lead").placements[0].start)).toBe(4);
  // the right edge: 4 bars → 2 bars (crop)
  let rz = (await clipEl("lead").locator(".rz").boundingBox())!;
  await drag(rz.x + rz.width / 2, rz.y + rz.height / 2, rz.x + rz.width / 2 - 2 * BW, rz.y + rz.height / 2);
  const p1 = track(await song(), "lead").placements[0];
  expect(bars(p1.length)).toBe(2);
  await page.keyboard.press("Control+z");
  // lead loops in the demo: switch the loop off, then Alt+drag the edge turns it on again while extending
  await page.mouse.click(cb.x + 30, cb.y + cb.height * 0.7, { button: "right" });
  await page.locator('.menu [role="menuitem"]:has-text("Loop off")').click();
  expect(track(await song(), "lead").placements[0].loop).toBe(false);
  rz = (await clipEl("lead").locator(".rz").boundingBox())!;
  await drag(rz.x + rz.width / 2, rz.y + rz.height / 2, rz.x + rz.width / 2 + 2 * BW, rz.y + rz.height / 2, ["Alt"]);
  const p2 = track(await song(), "lead").placements[0];
  expect(bars(p2.length)).toBe(6);
  expect(p2.loop).toBe(true);
  await page.keyboard.press("Control+z"); await page.keyboard.press("Control+z");
  expect(track(await song(), "lead").placements[0]).toMatchObject({ length: 4 * 3840, loop: true });
}, 30_000);

test("clips: a marquee from an empty spot selects the clips it touches; Ctrl+D duplicates them after themselves", async () => {
  const lb = (await lane("lead").boundingBox())!, kb = (await lane("kick").boundingBox())!;
  // from lead's empty bar 1 up to the kick lane at bar 6: kick, bass and lead clips touch the box
  await drag(lb.x + 10, lb.y + lb.height * 0.7, kb.x + 5 * BW, kb.y + kb.height * 0.5);
  const sel = await page.locator('.lane.track .clip[aria-label$="selected"]').count();
  expect(sel).toBe(3);
  expect(await page.locator('.lane.track[data-track="pad"] .clip[aria-label$="selected"]').count()).toBe(0);
  // Escape clears; a click on a clip body selects just that clip
  await page.locator("#lanes-wrap").focus(); await page.keyboard.press("Escape");
  expect(await page.locator('.lane.track .clip[aria-label$="selected"]').count()).toBe(0);
  const cb = (await clipEl("lead").boundingBox())!;
  await page.mouse.click(cb.x + 30, cb.y + cb.height * 0.7);
  expect(await page.locator('.lane.track .clip[aria-label$="selected"]').count()).toBe(1);
  const d0 = await depth();
  await page.keyboard.press("Control+d");
  const ps = track(await song(), "lead").placements.map((p: any) => [bars(p.start), bars(p.length)]);
  expect(ps).toEqual([[4, 4], [8, 4]]);
  expect(await depth()).toBe(d0 + 1);
  await page.keyboard.press("Control+z");
  expect(track(await song(), "lead").placements.length).toBe(1);
});

test("clips: with snap Off a dragged clip lands off the grid; Q aligns the selection to bars", async () => {
  await page.locator("#snap").selectOption("Off");
  const cb = (await clipEl("lead").boundingBox())!;
  await drag(cb.x + 10, cb.y + 5, cb.x + 10 - 0.4 * BW, cb.y + 5);
  const off = bars(track(await song(), "lead").placements[0].start);
  expect(off).toBeGreaterThan(3.5); expect(off).toBeLessThan(3.7);
  await page.locator("#kb-btn").click();  // Q is a note while the computer keyboard plays the track
  await page.locator("#lanes-wrap").focus();
  await page.keyboard.press("q");
  expect(bars(track(await song(), "lead").placements[0].start)).toBe(4);
  await page.keyboard.press("Control+z"); await page.keyboard.press("Control+z");
  await page.locator("#snap").selectOption("1/16");
  await page.locator("#kb-btn").click();
  expect(bars(track(await song(), "lead").placements[0].start)).toBe(4);
});

test("automation: click adds a point, arrows edit it, Delete removes it, the last point stays; target and lanes", async () => {
  const pts = async () => track(await song(), "bass").lanes[0].points as [number, number][];
  const n0 = (await pts()).length;
  const al = page.locator(".lane.auto").first(), ab = (await al.boundingBox())!;
  const d0 = await depth();
  await page.mouse.click(ab.x + 6 * BW, ab.y + ab.height / 2);  // bar 7, mid value
  expect((await pts()).length).toBe(n0 + 1);
  expect(await depth()).toBe(d0 + 1);
  const focused = () => page.evaluate(() => document.activeElement?.getAttribute("aria-valuenow"));
  expect(await page.evaluate(() => document.activeElement?.getAttribute("role"))).toBe("slider");
  const v0 = Number(await focused());
  await page.keyboard.press("ArrowUp");
  expect(Number(await focused())).toBeCloseTo(v0 + 0.05, 3);
  await page.keyboard.press("Shift+ArrowDown");
  expect(Number(await focused())).toBeCloseTo(v0 + 0.045, 3);
  expect(await depth()).toBe(d0 + 3);
  await page.keyboard.press("Delete");
  await page.waitForTimeout(50);  // the delete commit is deferred
  expect((await pts()).length).toBe(n0);
  expect(await depth()).toBe(d0 + 4);
  // the lane header names its target; changing it retargets the lane (one undo)
  const tgt = page.locator(".thdr.auto select").first();
  expect(await tgt.getAttribute("aria-label")).toBe("Bass automation target");
  await tgt.focus();  // the full target list is built when the select is first used
  const other = await tgt.locator("option:not([disabled]):not(:checked)").first().getAttribute("value");
  await tgt.selectOption(other!);
  expect(track(await song(), "bass").lanes[0].param).toBe(other!.split(":")[1]);
  await page.keyboard.press("Control+z");
  expect(track(await song(), "bass").lanes[0].param).toBe("filt1.cutoff");
  // + lane adds the next free parameter; ✕ removes a lane; the A count follows
  await page.locator('button[aria-label="Add automation lane to Bass"]').click();
  expect(track(await song(), "bass").lanes.length).toBe(2);
  expect(await hdr("bass").locator(".autobtn").textContent()).toBe("A2");
  await page.locator(".thdr.auto .rm").last().click();
  expect(track(await song(), "bass").lanes.length).toBe(1);
  expect(await hdr("bass").locator(".autobtn").textContent()).toBe("A1");
}, 30_000);

test("follow: while playing, the view pages when the playhead reaches the right edge; a hand scroll pauses follow", async () => {
  for (let i = 0; i < 4; i++) await page.locator("#zin").click();  // ≈ 220 px per bar: the song is wider than the view
  const lw = page.locator("#lanes-wrap");
  const view = await lw.evaluate((el) => ({ w: el.clientWidth, sw: el.scrollWidth }));
  expect(view.sw).toBeGreaterThan(view.w * 1.5);
  await lw.evaluate((el) => { el.scrollLeft = 0; });  // zoom keeps the centre bar in place
  await page.waitForTimeout(50);
  // seek just before the right edge, then play
  const rb = (await page.locator("#ruler").boundingBox())!, wb = (await lw.boundingBox())!;
  await page.mouse.click(wb.x + view.w - 80, rb.y + rb.height - 2);
  expect(await lw.evaluate((el) => el.scrollLeft)).toBe(0);
  await page.locator("#play").click();
  await page.waitForFunction(() => document.getElementById("lanes-wrap")!.scrollLeft > 0, null, { timeout: 15_000 });
  // a hand scroll holds follow (the button says so) until the playhead reaches the right edge again
  await page.mouse.move(wb.x + 200, wb.y + 60);
  await page.mouse.wheel(-400, 0);
  await page.waitForFunction(() => document.getElementById("follow")!.classList.contains("held"), null, { timeout: 5000 });
  await page.locator("#stop").click(); await page.locator("#stop").click();
  await page.locator("#zfit").click();
}, 60_000);

test("50 tracks: every header and lane in order; Arrow Down walks to the last track, which scrolls into view", async () => {
  await open("big");
  expect(await page.evaluate(() => (window as any).DAW.debug.song().tracks.length)).toBe(BIG_TRACKS);
  const lw = page.locator("#lanes-wrap");
  const ext = await lw.evaluate((el) => ({ h: el.clientHeight, sh: el.scrollHeight }));
  expect(ext.sh).toBeGreaterThan(ext.h * 2);  // the scroll extent covers all 50 tracks
  await hdr("t1").focus();
  for (let i = 1; i < BIG_TRACKS; i++) await page.keyboard.press("ArrowDown");
  expect(await focusedTrack()).toBe(`t${BIG_TRACKS}`);
  expect(await selected()).toBe(`t${BIG_TRACKS}`);
  const vis = await page.evaluate((id) => {
    const w = document.getElementById("lanes-wrap")!.getBoundingClientRect(), h = document.querySelector(`.thdr.track[data-track="${id}"]`)!.getBoundingClientRect();
    const l = document.querySelector(`.lane.track[data-track="${id}"]`)!.getBoundingClientRect();
    return { hdr: h.top >= w.top - 1 && h.bottom <= w.bottom + 1, aligned: Math.abs(h.top - l.top) < 2 };
  }, `t${BIG_TRACKS}`);
  expect(vis).toEqual({ hdr: true, aligned: true });
}, 60_000);

test("header controls drive the mixer strip (store): fader keys are one undo step each, mute is one", async () => {
  await open();
  await page.locator("#tab-mix").click();
  const strip = page.locator('[role="group"][aria-label="Lead channel strip"]');
  const hf = hdr("lead").locator('input[aria-label="Lead volume"]');
  const d0 = await depth();
  await hf.focus(); await page.keyboard.press("ArrowLeft"); await page.keyboard.press("ArrowLeft");
  const v = track(await song(), "lead").volume;
  expect(await strip.locator(".fader").getAttribute("aria-valuetext")).toBe(await hf.getAttribute("aria-valuetext"));
  expect(await hdr("lead").locator(".vol .num").textContent()).toMatch(/ dB$/);
  expect(await depth()).toBe(d0 + 2);
  await hdr("lead").locator('[aria-label="Mute Lead"]').click();
  expect(await strip.locator('[aria-label="Mute Lead"]').getAttribute("aria-pressed")).toBe("true");
  await page.waitForTimeout(50);  // the mute commit runs after the click
  expect(await depth()).toBe(d0 + 3);
  await page.locator('[aria-label="Undo"]').click();
  expect(await strip.locator('[aria-label="Mute Lead"]').getAttribute("aria-pressed")).toBe("false");
  expect(track(await song(), "lead").volume).toBe(v);
  await page.locator("#tab-dev").click();
}, 30_000);

// spec synth-daw "Off-screen arrangement content" (daw-solid-ts 5.2): windowed rows
test("50 tracks: fewer than 50 lanes exist and the scroll height covers all 50 (spec: Large song)", async () => {
  await open("big");
  expect(await hdr("t1").getAttribute("aria-current")).toBe("true");  // selected at load, marked without a click
  const lw = page.locator("#lanes-wrap");
  const st = await lw.evaluate((el) => ({ h: el.clientHeight, sh: el.scrollHeight, lanes: document.querySelectorAll(".lane.track").length, hdrs: document.querySelectorAll(".thdr.track").length }));
  expect(st.lanes).toBeLessThan(BIG_TRACKS);
  expect(st.hdrs).toBe(st.lanes);
  // every track open with its one automation lane: 56 + 40 + 24 px per row
  expect(st.sh).toBeGreaterThanOrEqual(BIG_TRACKS * 120);
  // scrolled to the end: the last rows exist, the first ones (except the selected track) do not; headers stay aligned
  await lw.evaluate((el) => { el.scrollTop = el.scrollHeight; });
  await page.waitForTimeout(100);
  const end = await page.evaluate((n) => {
    const ids = [...document.querySelectorAll<HTMLElement>(".lane.track")].map((l) => l.dataset.track);
    const w = document.getElementById("lanes-wrap")!.getBoundingClientRect();
    const h = document.querySelector(`.thdr.track[data-track="t${n}"]`)!.getBoundingClientRect(), l = document.querySelector(`.lane.track[data-track="t${n}"]`)!.getBoundingClientRect();
    return { ids, inView: l.bottom <= w.bottom + 1 && l.top >= w.top - 200, aligned: Math.abs(h.top - l.top) < 2 };
  }, BIG_TRACKS);
  expect(end.ids).toContain(`t${BIG_TRACKS}`);
  expect(end.ids).toContain("t1");      // selected: always mounted
  expect(end.ids).not.toContain("t2");  // far above the view
  expect(end.ids.length).toBeLessThan(BIG_TRACKS);
  expect(end).toMatchObject({ inView: true, aligned: true });
  // the rows that left hold nothing; the selected row, mounted off screen, holds no clips until it is in view
  await page.waitForTimeout(200);  // rows entering while scrolling fill from a per-frame queue
  expect(await lane(`t${BIG_TRACKS}`).locator(".clip").count()).toBe(1);
  expect(await lane("t1").locator(".clip").count()).toBe(0);
  expect(await page.locator(".clip").count()).toBe(end.ids.length - 1);
}, 60_000);

test("50 tracks: Arrow Down past the view scrolls, and the next header exists, has focus and is selected (spec: Keyboard to an off-screen track)", async () => {
  await open("big");
  await hdr("t1").focus();
  const lw = page.locator("#lanes-wrap");
  for (let i = 2; i <= 20; i++) {
    await page.keyboard.press("ArrowDown");
    expect(await focusedTrack()).toBe(`t${i}`);
  }
  expect(await selected()).toBe("t20");
  expect(await lw.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
  expect(await page.locator(".lane.track").count()).toBeLessThan(BIG_TRACKS);
}, 60_000);

test("50 tracks: no critical or serious accessibility violations (axe)", async () => {
  await page.addScriptTag({ path: join(ROOT, "node_modules/axe-core/axe.min.js") });
  const res: any = await page.evaluate(async () => (window as any).axe.run(document.getElementById("arr"), { resultTypes: ["violations"] }));
  const bad = res.violations.filter((v: any) => v.impact === "critical" || v.impact === "serious");
  if (bad.length) console.error(bad.map((v: any) => `${v.id} (${v.impact}): ${v.nodes.length} × ${v.nodes[0]?.target}`).join("\n"));
  expect(bad.map((v: any) => v.id)).toEqual([]);
}, 60_000);

test("no page errors", () => expect(errors).toEqual([]));
