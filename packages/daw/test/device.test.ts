// Device panel (spec synth-daw; contract for the SolidJS port, change daw-solid-ts 6.1): what editing.test.ts and
// browser.test.ts do not cover. Knob drag (fine with Shift), double-click reset and value text; effect chain add,
// mode change (keeps only the mode), reorder (automation lanes follow), bypass, remove (with its lanes); preset save
// to project/music/patches and loading it elsewhere; mod matrix rows (source → row style, target, amount). Each edit
// is one undo step, checked through the song/1 source (DAW.debug.song()).
import { afterAll, beforeAll, expect, test } from "bun:test";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Browser, Page } from "playwright";
import { startServer, type Server } from "../../cli/src/server.ts";
import { buildDaw, DIST } from "../build.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const root = mkdtempSync(join(tmpdir(), "daw-device-"));
let s: Server, browser: Browser, page: Page;
const errors: string[] = [];

beforeAll(async () => {
  mkdirSync(join(root, "project/music"), { recursive: true });
  cpSync(join(ROOT, "project/music/patches"), join(root, "project/music/patches"), { recursive: true });
  // the demo with a lane on Bass's Crusher (device 1) besides the instrument's cutoff lane
  const demo = JSON.parse(readFileSync(join(ROOT, "project/music/demo.song.json"), "utf8"));
  demo.tracks.find((t: any) => t.id === "bass").lanes.push({ device: 1, param: "dry_wet", points: [[0, 0.4], [15360, 0.8]] });
  writeFileSync(join(root, "project/music/demo.song.json"), JSON.stringify(demo, null, 1));
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

async function open(id = "bass") {
  await page.goto(`${s.url}daw/?song=project/music/demo.song.json`);
  await page.waitForSelector(".thdr.track");
  await page.waitForTimeout(200);
  await page.locator(`.thdr.track[data-track="${id}"]`).click();
  await page.locator("#tab-dev").click();
}
const song = (): Promise<any> => page.evaluate(() => (window as any).DAW.debug.song());
const depth = (): Promise<number> => page.evaluate(() => (window as any).DAW.debug.undoDepth());
const track = async (id: string) => (await song()).tracks.find((t: any) => t.id === id);
const undo = () => page.locator('[aria-label="Undo"]').click();
const knobRes = () => page.locator(`#device [data-param="filt1.resonance"]`);  // not automated in the demo

test("knob: drag up (160 px = full range, Shift 5× finer) is one undo step; text follows; double-click resets", async () => {
  await open();
  const k = knobRes(), d0 = await depth();
  const v0 = Number(await k.getAttribute("aria-valuenow"));
  expect(await k.getAttribute("aria-valuetext")).toBe(await k.locator(".v").textContent());
  const b = (await k.boundingBox())!, x = b.x + b.width / 2, y = b.y + b.height / 2;
  await page.mouse.move(x, y); await page.mouse.down();
  await page.mouse.move(x, y - 20, { steps: 4 }); await page.mouse.move(x, y - 40, { steps: 4 });
  await page.mouse.up();
  const v1 = Number(await k.getAttribute("aria-valuenow"));
  expect(v1).toBeCloseTo(v0 + 40 / 160, 2);
  expect((await track("bass")).devices[0].values["filt1.resonance"]).toBeCloseTo(v1, 3);
  expect(await k.getAttribute("aria-valuetext")).toBe(await k.locator(".v").textContent());
  expect(await depth()).toBe(d0 + 1);
  // Shift: 800 px per full range
  await page.keyboard.down("Shift");
  await page.mouse.move(x, y); await page.mouse.down(); await page.mouse.move(x, y - 40, { steps: 4 }); await page.mouse.up();
  await page.keyboard.up("Shift");
  expect(Number(await k.getAttribute("aria-valuenow"))).toBeCloseTo(v1 + 40 / 800, 2);
  expect(await depth()).toBe(d0 + 2);
  // double-click: the parameter default, one step
  const def: number = await page.evaluate(() => (window as any).DAW.tables[0].params.find((p: any) => p.name === "filt1.resonance").default);
  await k.dblclick();
  expect(Number(await k.getAttribute("aria-valuenow"))).toBeCloseTo(def, 3);
  expect((await track("bass")).devices[0].values["filt1.resonance"]).toBeCloseTo(def, 3);
  await undo(); await undo(); await undo();
  expect((await track("bass")).devices[0].values["filt1.resonance"]).toBeCloseTo(v0, 3);
  expect(Number(await knobRes().getAttribute("aria-valuenow"))).toBeCloseTo(v0, 3);
}, 30_000);

test("effects: add (with a mode), mode change keeps only the mode, reorder moves lanes, bypass, remove drops its lanes", async () => {
  await open();
  const boxes = () => page.locator("#device .devbox").evaluateAll((bs) => bs.map((b) => b.getAttribute("aria-label")));
  expect(await boxes()).toEqual(["Instrument", "Effect Crusher"]);
  const d0 = await depth();
  await page.locator("#device .addfx").click();
  await page.locator('select[aria-label="Effect to add to Bass"]').selectOption("Distortion · Shaper");
  expect(await depth()).toBe(d0 + 1);
  let bs = await boxes();
  expect(bs.length).toBe(3);
  const shaper = bs[2]!.slice("Effect ".length), sbox = page.locator(`#device .devbox[aria-label="Effect ${shaper}"]`);
  let devs = (await track("bass")).devices;
  expect(devs.map((d: any) => d.kind)).toEqual([64, 66, devs[2].kind]);
  // the mode select (family) picks another parameter set: only the mode value stays
  const mode = sbox.locator('select[aria-label$=" mode"]');
  expect(await mode.count()).toBe(1);
  const k = sbox.locator('[role="slider"]').first();
  await k.focus(); await page.keyboard.press("ArrowUp");
  expect(Object.keys((await track("bass")).devices[2].values).length).toBeGreaterThan(1);
  await mode.selectOption({ index: 1 });
  devs = (await track("bass")).devices;
  expect(Object.keys(devs[2].values)).toEqual(["mode"]);
  expect(await depth()).toBe(d0 + 3);
  // reorder: the effect moves earlier, the Crusher's lane follows it to device 2; the instrument stays first
  const name2 = (await boxes())[2]!.slice("Effect ".length);
  expect(await page.locator(`#device [aria-label="Move ${name2} later"]`).isDisabled()).toBe(true);
  expect(await page.locator('#device [aria-label="Move Crusher earlier"]').isDisabled()).toBe(true);
  await page.locator(`#device [aria-label="Move ${name2} earlier"]`).click();
  let bass = await track("bass");
  expect(bass.devices.map((d: any) => d.kind)).toEqual([64, devs[2].kind, 66]);
  expect(bass.lanes.map((l: any) => [l.device, l.param])).toEqual([[0, "filt1.cutoff"], [2, "dry_wet"]]);
  expect(await boxes()).toEqual(["Instrument", `Effect ${name2}`, "Effect Crusher"]);
  // bypass: the box dims, the song says so, one step
  await page.locator('#device [aria-label="Bypass Crusher"]').click();
  expect(await page.locator('#device .devbox[aria-label="Effect Crusher"]').getAttribute("class")).toContain("byp");
  expect(await page.locator('#device [aria-label="Bypass Crusher"]').getAttribute("aria-pressed")).toBe("true");
  expect((await track("bass")).devices[2].bypass).toBe(true);
  // remove: its lane goes too
  await page.locator('#device [aria-label="Remove Crusher"]').click();
  bass = await track("bass");
  expect(bass.devices.length).toBe(2);
  expect(bass.lanes.map((l: any) => l.param)).toEqual(["filt1.cutoff"]);
  expect(await page.locator("#live").textContent()).toContain("with 1 automation lane");
  expect(await depth()).toBe(d0 + 6);
  for (let i = 0; i < 6; i++) await undo();
  bass = await track("bass");
  expect(bass.devices.map((d: any) => d.kind)).toEqual([64, 66]);
  expect(bass.lanes.map((l: any) => [l.device, l.param])).toEqual([[0, "filt1.cutoff"], [1, "dry_wet"]]);
  expect(await boxes()).toEqual(["Instrument", "Effect Crusher"]);
}, 30_000);

test("instrument bypass and preset save: written to project/music/patches, listed, loadable on another track", async () => {
  await open();
  const inst = page.locator('#device .devbox[aria-label="Instrument"]');
  await inst.locator('[aria-label="Bypass instrument"]').click();
  expect((await track("bass")).devices[0].bypass).toBe(true);
  expect(await inst.getAttribute("class")).toContain("byp");
  await undo();
  expect((await track("bass")).devices[0].bypass).toBeFalsy();
  // edit a knob: the preset reads as edited
  await knobRes().focus(); await page.keyboard.press("ArrowUp");
  expect(await page.locator('#device .devbox[aria-label="Instrument"] h3 .kind').textContent()).toContain("preset Acid Bass*");
  await page.locator("#device .savepre").click();
  const dlg = page.getByRole("dialog", { name: "Save preset" });
  await dlg.locator(".pname").fill("Test Patch 1");
  expect(await dlg.locator(".pfile").textContent()).toBe("test-patch-1.patch.json");
  await dlg.locator(".pname").fill("");
  expect(await dlg.locator(".ok").isDisabled()).toBe(true);
  await dlg.locator(".pname").fill("Test Patch 1");
  const d0 = await depth();
  await dlg.locator(".ok").click();
  await page.waitForFunction((d) => (window as any).DAW.debug.undoDepth() === d + 1, d0);
  expect(await dlg.count()).toBe(0);
  expect(await page.evaluate(() => document.activeElement?.className)).toBe("savepre");  // (lost to <body> before 6.2)
  const file = join(root, "project/music/patches/test-patch-1.patch.json");
  expect(existsSync(file)).toBe(true);
  const patch = JSON.parse(readFileSync(file, "utf8")), bassVals = (await track("bass")).devices[0].values;
  expect(patch.name).toBe("Test Patch 1");
  expect(patch.values["filt1.resonance"]).toBeCloseTo(bassVals["filt1.resonance"], 6);  // f32 in the patch
  expect((await track("bass")).devices[0].preset).toBe("Test Patch 1");
  expect(await page.locator('#device .devbox[aria-label="Instrument"] h3 .kind').textContent()).toContain("preset Test Patch 1");
  expect(await page.locator('#device .devbox[aria-label="Instrument"] h3 .kind').textContent()).not.toContain("*");
  // the library lists it; load onto Pad
  await page.locator('.thdr.track[data-track="pad"]').click();
  await page.locator("#lib-q").fill("test patch");
  await page.locator('#lib-tree details[data-src="project"] button:has-text("Test Patch 1")').click();
  await page.locator("#lib-insp .loadin").click();
  await page.waitForFunction(() => (window as any).DAW.debug.song().tracks.find((t: any) => t.id === "pad").devices[0].preset === "Test Patch 1");
  const pad = (await track("pad")).devices[0].values;
  expect(pad["filt1.resonance"]).toBeCloseTo(bassVals["filt1.resonance"], 6);
  await page.locator("#lib-q").fill("");
}, 30_000);

test("preset save: Escape cancels (nothing written), focus back on the button", async () => {
  await open("lead");
  await page.locator("#device .savepre").click();
  const dlg = page.getByRole("dialog", { name: "Save preset" });
  await dlg.locator(".pname").fill("Never Saved");
  await page.keyboard.press("Escape");
  expect(await dlg.count()).toBe(0);
  expect(await page.evaluate(() => document.activeElement?.className)).toBe("savepre");
  expect(existsSync(join(root, "project/music/patches/never-saved.patch.json"))).toBe(false);
}, 30_000);

test("mod matrix: a row lights up with its source kind; target and amount are one undo step each", async () => {
  await open();
  const row = page.locator("table.mm tbody tr").first(), opts = (await page.evaluate(() => (window as any).DAW.tables[0].params.find((p: any) => p.name === "mod1.source").options)) as string[];
  expect(await page.locator("table.mm tbody tr").count()).toBeGreaterThanOrEqual(4);
  expect(await row.locator('[role="slider"]').getAttribute("aria-label")).toBe("mod1 amount");  // (empty before 6.2)
  expect(await row.getAttribute("class")).toBe("off");
  const d0 = await depth();
  const env = opts.findIndex((o) => /^Env/.test(o)), lfo = opts.findIndex((o) => /^LFO/.test(o));
  await row.locator('select[aria-label$=" mod1.source"]').selectOption(String(env));
  const r1 = page.locator("table.mm tbody tr").first();  // the panel rebuilds after a source change
  expect(await r1.getAttribute("class")).toBe("");
  expect(await r1.getAttribute("data-src")).toBe("env");
  await r1.locator('select[aria-label$=" mod1.target"]').selectOption("2");
  const amt = page.locator("table.mm tbody tr").first().locator('[role="slider"]');
  await amt.focus(); await page.keyboard.press("End");
  let v = (await track("bass")).devices[0].values;
  const n = opts.length;
  expect(v["mod1.source"]).toBeCloseTo(env / (n - 1), 6);
  expect(v["mod1.amount"]).toBe(1);
  expect(await depth()).toBe(d0 + 3);
  await page.locator("table.mm tbody tr").first().locator('select[aria-label$=" mod1.source"]').selectOption(String(lfo));
  expect(await page.locator("table.mm tbody tr").first().getAttribute("data-src")).toBe("lfo");
  for (let i = 0; i < 4; i++) await undo();
  v = (await track("bass")).devices[0].values;
  expect(v["mod1.source"] ?? 0).toBe(0);
  expect(await page.locator("table.mm tbody tr").first().getAttribute("class")).toBe("off");
}, 30_000);

test("playback: automation changes only the automated knob and the screen drawn from it", async () => {
  await open();
  await page.evaluate(() => localStorage.setItem("daw.live", "0"));  // no live overlays: only automation moves things
  await open();
  const k = page.locator('#device [data-param="filt1.cutoff"]');
  expect(await k.getAttribute("class")).toContain("auto");
  const v0 = await k.getAttribute("aria-valuenow");
  await page.evaluate(() => {
    const w = window as any; w.__mut = [];
    new MutationObserver((ms) => { for (const m of ms) { const n = m.target as Element, el = n.nodeType === 1 ? n : n.parentElement!;
      w.__mut.push(el.closest('[data-param], svg[data-live]')?.getAttribute("data-param") ?? el.closest("svg[data-live]")?.getAttribute("data-live") ?? el.outerHTML.slice(0, 80)); } })
      .observe(document.getElementById("device")!, { subtree: true, childList: true, attributes: true, characterData: true });
  });
  await page.keyboard.press("Space");
  await page.waitForTimeout(1500);
  await page.keyboard.press("Space");
  const touched: string[] = await page.evaluate(() => [...new Set((window as any).__mut as string[])]);
  expect(await k.getAttribute("aria-valuenow")).not.toBe(v0);
  expect(touched.sort()).toEqual(["dry_wet", "filt1", "filt1.cutoff"]);  // both lanes of the fixture
  await page.evaluate(() => localStorage.removeItem("daw.live"));
}, 30_000);

test("no page errors", () => expect(errors).toEqual([]));
