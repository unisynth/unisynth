// DAW editing tests (change add-synth-daw, tasks 5.4a–5.4f, 5.6a, 5.6b): the reviewed mockup interactions on the
// real model, checked through the song/2 source the editor would save (DAW.debug.song()).
import { afterAll, beforeAll, expect, test } from "bun:test";
import { cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Browser, Page } from "playwright";
import { startServer, type Server } from "../../cli/src/server.ts";
import { buildDaw, DIST } from "../build.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const root = mkdtempSync(join(tmpdir(), "daw-edit-"));
let s: Server, browser: Browser, page: Page;
const errors: string[] = [];
const BW = 44; // px per bar at the default zoom

beforeAll(async () => {
  mkdirSync(join(root, "project"), { recursive: true });
  mkdirSync(join(root, "project/music"), { recursive: true });
  cpSync(join(ROOT, "project/music/demo.song.json"), join(root, "project/music/demo.song.json"));
  cpSync(join(ROOT, "project/music/patches"), join(root, "project/music/patches"), { recursive: true });
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

async function open() {
  await page.goto(`${s.url}daw/?song=project/music/demo.song.json`);
  await page.waitForSelector(".thdr.track");
  await page.waitForTimeout(200);
}
const song = (): Promise<any> => page.evaluate(() => (window as any).DAW.debug.song());
const track = (sg: any, id: string) => sg.tracks.find((t: any) => t.id === id);
const clip = (sg: any, id: string) => sg.clips.find((c: any) => c.id === id);
/** Click a lane of `id` at bar `bar` (1-based), vertically in its clip body. */
async function laneAt(id: string, bar: number, opts: { dbl?: boolean; y?: number } = {}) {
  const b = (await page.locator(`.lane.track[data-track="${id}"]`).boundingBox())!;
  const x = b.x + (bar - 1) * BW + 2, y = b.y + (opts.y ?? b.height * 0.7);
  if (opts.dbl) await page.mouse.dblclick(x, y);
  else await page.mouse.click(x, y);
}
function noOverlap(t: any) {
  const ps = [...t.placements].sort((a: any, b: any) => a.start - b.start);
  for (let i = 1; i < ps.length; i++) expect(ps[i - 1].start + ps[i - 1].length).toBeLessThanOrEqual(ps[i].start);
}

/** The sampler's stored oscillator type (`Choice(13)`, option 4). */
const SAMPLER = 4 / 12;

test("5.4a paste trims what it covers (no overlapping clips) and undo restores", async () => {
  await open();
  await laneAt("lead", 5.5); // select lead-a (bars 5–9)
  await page.keyboard.press("Control+c");
  await laneAt("lead", 3); // insert cursor at bar 3 on the lead lane (empty space)
  await page.keyboard.press("Control+v");
  const sg = await song(), lead = track(sg, "lead");
  expect(lead.placements.length).toBeGreaterThanOrEqual(2);
  noOverlap(lead);
  expect(lead.placements.some((p: any) => p.start === 2 * 3840)).toBe(true);
  await page.locator('[aria-label="Undo"]').click();
  expect(track(await song(), "lead").placements.length).toBe(1);
}, 60_000);

test("5.4b note commands: select all, transpose, delete, undo — in the clip's notes", async () => {
  await open();
  await laneAt("bass", 2, { dbl: true }); // open bass-a in the piano roll
  await page.waitForTimeout(200);
  const before = clip(await song(), "bass-a").notes.map((n: number[]) => n[2]);
  await page.locator("#gridc").focus();
  await page.keyboard.press("Control+a");
  await page.keyboard.press("ArrowUp");
  expect(clip(await song(), "bass-a").notes.map((n: number[]) => n[2])).toEqual(before.map((p: number) => p + 1));
  await page.keyboard.press("Delete");
  expect(clip(await song(), "bass-a").notes.length).toBe(0);
  await page.keyboard.press("Control+z");
  await page.keyboard.press("Control+z");
  expect(clip(await song(), "bass-a").notes.map((n: number[]) => n[2])).toEqual(before);
}, 60_000);

test("5.4c linked duplicate shares one clip; a note edit reaches both placements", async () => {
  await open();
  await laneAt("lead", 6);
  await page.keyboard.press("Control+Shift+d"); // duplicate linked
  let sg = await song();
  const lead = track(sg, "lead");
  expect(lead.placements.length).toBe(2);
  expect(new Set(lead.placements.map((p: any) => p.clip)).size).toBe(1);
  noOverlap(lead);
  await laneAt("lead", 6, { dbl: true });
  await page.waitForTimeout(200);
  await page.locator("#gridc").focus();
  await page.keyboard.press("Control+a");
  await page.keyboard.press("ArrowDown");
  sg = await song();
  const ids = track(sg, "lead").placements.map((p: any) => p.clip);
  expect(ids[0]).toBe(ids[1]);
  expect(clip(sg, ids[0]).notes[0][2]).toBe(71); // 72 − 1
  expect(sg.length).toBeGreaterThanOrEqual(12 * 3840); // the duplicate after bar 9 grew the song
}, 60_000);

test("5.4f track commands and clip loop toggle", async () => {
  await open();
  const n0 = (await song()).tracks.length;
  await page.locator('#arr-corner button[aria-label="Add track"]').click();
  let sg = await song();
  expect(sg.tracks.length).toBe(n0 + 1);
  const added = sg.tracks.find((t: any) => !["kick", "bass", "lead", "pad", "verb"].includes(t.id));
  expect(added.devices[0].kind).toBe(64);
  await page.locator(`.thdr.track[data-track="${added.id}"]`).focus();
  await page.keyboard.press("F2");
  await page.keyboard.type("Arp");
  await page.keyboard.press("Enter");
  expect(track(await song(), added.id).name).toBe("Arp");
  await page.locator(`.thdr.track[data-track="${added.id}"]`).focus();
  await page.keyboard.press("Control+Delete");
  expect((await song()).tracks.length).toBe(n0);
  // the kick clip loops (demo); its menu turns the loop off
  const b = (await page.locator('.lane.track[data-track="kick"] .clip').first().boundingBox())!;
  await page.mouse.click(b.x + 30, b.y + b.height * 0.7, { button: "right" });
  await page.locator('.menu [role="menuitem"]:has-text("Loop off")').click();
  expect(track(await song(), "kick").placements[0].loop).toBe(false);
}, 60_000);

test("5.6a fixed slots: clear in place keeps other slots and their mod targets; filters swap with their lanes", async () => {
  await open();
  await page.locator('.thdr.track[data-track="bass"]').click();
  await page.locator("#tab-dev").click();
  const names: string[] = await page.evaluate(() => (window as any).DAW.tables[0].params.find((p: any) => p.name === "mod1.target").options);
  const vals = async () => track(await song(), "bass").devices[0].values;
  const lanes = async () => track(await song(), "bass").lanes.map((l: any) => l.param);
  // every slot is on the panel, unset ones collapsed
  expect(await page.locator('#device .grp.slot').count()).toBe(3 + 4 + 4 + 2);
  expect(await page.locator('#device .grp.slot[aria-label="Osc 2"]').getAttribute("class")).toContain("closed");
  await page.locator('#device select[aria-label$=" osc2.type"]').selectOption({ label: "Phase mod" }); // set from the collapsed header
  const osc2 = (await vals())["osc2.type"];
  expect(osc2).toBeGreaterThan(0);
  expect(await page.locator('#device .grp.slot[aria-label="Osc 2"]').getAttribute("class")).not.toContain("closed");
  await page.locator('#device select[aria-label$=" mod1.source"]').selectOption({ label: "LFO 1" });
  await page.locator('#device select[aria-label$=" mod1.target"]').selectOption({ label: "osc2.gain" });
  expect(await page.locator('#device .grp.slot[aria-label="LFO 1"] .rt').textContent()).toBe("→ osc2.gain");
  await page.locator('#device button[aria-label="Clear Osc 1"]').click();
  let d = await vals();
  expect(d["osc1.type"] ?? 0).toBe(0);
  expect(d["osc2.type"]).toBe(osc2);
  expect(names[Math.round(d["mod1.target"] * 255)]).toBe("osc2.gain"); // not renumbered
  // filter 1 holds the automated cutoff: clearing it removes that lane, undo brings it back
  expect(await lanes()).toEqual(["filt1.cutoff"]);
  await page.locator('#device button[aria-label="Clear Filter 1"]').click();
  expect(await lanes()).toEqual([]);
  await page.locator('[aria-label="Undo"]').click();
  expect(await lanes()).toEqual(["filt1.cutoff"]);
  // swap filters 1 and 2: values, lane and mod target follow
  await page.locator('#device select[aria-label$=" mod1.target"]').selectOption({ label: "filt1.cutoff" });
  await page.locator('#device button[aria-label="Swap Filter 1 and Filter 2"]').click();
  d = await vals();
  expect([d["filt1.type"] ?? 0, d["filt2.type"]]).toEqual([0, 0.5]); // the 4-pole low-pass (Choice(11))
  expect(await lanes()).toEqual(["filt2.cutoff"]);
  expect(names[Math.round(d["mod1.target"] * 255)]).toBe("filt2.cutoff");
  // no add/move buttons for other groups
  expect(await page.locator('#device button:has-text("+ Osc"), #device button[aria-label^="Move Osc"]').count()).toBe(0);
}, 60_000);

test("5.6c collapsed slots expand when set; a collapsed block survives a track change and is not saved", async () => {
  await open();
  await page.locator('.thdr.track[data-track="lead"]').click();
  await page.locator("#tab-dev").click();
  await page.locator('#device select[aria-label$=" filt3.type"]').selectOption({ index: 1 });
  expect(await page.locator('#device .grp.slot[aria-label="Filter 3"]').getAttribute("class")).not.toContain("closed");
  expect(await page.locator('#device .grp.slot[aria-label="Filter 2"]').getAttribute("class")).toContain("closed");
  expect(await page.locator('#device .blk[data-k="filt"] .cnt').textContent()).toBe("2/4");
  const before = JSON.stringify(await song());
  await page.locator('#device button[aria-label="Collapse Mod matrix"]').click();
  await page.locator('.thdr.track[data-track="bass"]').click();
  await page.locator('.thdr.track[data-track="lead"]').click();
  expect(await page.locator('#device .blk[data-k="mod"]').getAttribute("class")).toContain("closed");
  expect(JSON.stringify(await song())).toBe(before);
  await page.locator('#device button[aria-label="Expand Mod matrix"]').click();
}, 60_000);

test("5.6b knobs follow automation at the playhead; a hand edit overrides until re-enabled", async () => {
  await open();
  await page.locator('.thdr.track[data-track="bass"]').click();
  await page.locator("#tab-dev").click();
  const k = page.locator('#device .knob[data-param="filt1.cutoff"]');
  // the bass cutoff lane: 0.25 at bar 1, 0.6 at bar 5, 0.3 at bar 9
  const ruler = (await page.locator("#ruler").boundingBox())!;
  await page.mouse.click(ruler.x + 4 * BW + 1, ruler.y + ruler.height - 4); // seek to bar 5
  await page.waitForTimeout(100);
  expect(Number(await k.getAttribute("aria-valuenow"))).toBeCloseTo(0.6, 1);
  await page.mouse.click(ruler.x + 0 * BW + 1, ruler.y + ruler.height - 4); // bar 1
  await page.waitForTimeout(100);
  expect(Number(await k.getAttribute("aria-valuenow"))).toBeCloseTo(0.25, 1);
  await k.focus();
  await k.press("End");
  expect(await page.locator("#reauto").count()).toBe(1);
  await page.locator("#reauto").click();
  await page.mouse.click(ruler.x + 4 * BW + 1, ruler.y + ruler.height - 4);
  await page.waitForTimeout(100);
  expect(Number(await page.locator('#device .knob[data-param="filt1.cutoff"]').getAttribute("aria-valuenow"))).toBeCloseTo(0.6, 1);
}, 60_000);

test("5.4e zoom keeps the bar under the pointer", async () => {
  await open();
  await page.locator("#len-in").fill("32"); // room to scroll: the demo is 8 bars, narrower than the zoomed view
  await page.locator("#len-in").press("Enter");
  const lw = page.locator("#lanes-wrap");
  const box = (await lw.boundingBox())!;
  const x = box.x + 8 * BW + 3; // bar 9 at the default zoom
  await page.mouse.move(x, box.y + 40);
  await page.keyboard.down("Control");
  await page.mouse.wheel(0, -300);
  await page.keyboard.up("Control");
  await page.waitForTimeout(150);
  const bar9 = await page.evaluate(() => { const n = [...document.querySelectorAll("#ruler .bar.num")].find((e) => e.textContent === "9") as HTMLElement; return n.getBoundingClientRect().left; });
  expect(Math.abs(bar9 - (x - 3))).toBeLessThan(BW); // still under the pointer after zooming in
  const w2 = await page.evaluate(() => { const a = [...document.querySelectorAll("#ruler .bar.num")] as HTMLElement[]; const b2 = a.find((e) => e.textContent === "2")!, b3 = a.find((e) => e.textContent === "3")!; return b3.offsetLeft - b2.offsetLeft; });
  expect(w2).toBeGreaterThan(BW);
}, 60_000);

test("no page errors", () => expect(errors).toEqual([]));

test("the piano roll draws only what is visible and hit-tests correctly when scrolled", async () => {
  await open();
  await page.locator("#new-btn").click();
  await page.waitForTimeout(300);
  const lane = (await page.locator(".lane.track").first().boundingBox())!;
  await page.mouse.dblclick(lane.x + 10, lane.y + lane.height / 2); // a 1-bar clip, open in the roll
  await page.waitForTimeout(300);
  for (let i = 0; i < 6; i++) await page.locator('#roll-tools [data-zoom="t+"]').click(); // 120 px per step
  const v = await page.evaluate(() => {
    const g = document.getElementById("grid")!, c = document.getElementById("gridc") as HTMLCanvasElement;
    g.scrollLeft = 600; const row = (96 - 60) * 12; g.scrollTop = Math.max(0, row - 60);
    return { sw: g.scrollWidth, cw: g.clientWidth, y: row - g.scrollTop + 6 };
  });
  await page.waitForTimeout(150);
  expect(v.sw).toBeGreaterThanOrEqual(16 * 120); // the scroll range is the whole clip
  const canvasW = await page.evaluate(() => (document.getElementById("gridc") as HTMLCanvasElement).style.width);
  expect(parseFloat(canvasW)).toBeLessThanOrEqual(v.cw + 1); // the canvas is only the visible part
  const gr = (await page.locator("#grid").boundingBox())!;
  await page.mouse.dblclick(gr.x + 70, gr.y + v.y); // clip x = 600 + 70 = 670 px → step 5
  await page.waitForTimeout(200);
  const notes = (await song()).clips[0].notes;
  expect(notes.map((n: number[]) => n[0])).toEqual([5 * 240]);
}, 60_000);

test("resizing one of several selected notes scales every selected length by the same factor (Alt: same amount)", async () => {
  await open();
  await page.locator("#new-btn").click();
  await page.waitForTimeout(300);
  const lane = (await page.locator(".lane.track").first().boundingBox())!;
  await page.mouse.dblclick(lane.x + 10, lane.y + lane.height / 2); // a 1-bar clip, open in the roll
  await page.waitForTimeout(300);
  const STEP = 22; // px per 1/16 at the default zoom
  const y = await page.evaluate(() => { const g = document.getElementById("grid")!; g.scrollLeft = 0; const row = (96 - 60) * 12; g.scrollTop = Math.max(0, row - 60); return row - g.scrollTop + 6; });
  await page.waitForTimeout(150);
  const gr = (await page.locator("#grid").boundingBox())!;
  const at = (step: number, dy = 0) => [gr.x + step * STEP, gr.y + y + dy] as const;
  const drag = async (from: number, to: number, dy = 0, alt = false) => {
    if (alt) await page.keyboard.down("Alt");
    await page.mouse.move(...at(from, dy)); await page.mouse.down(); await page.mouse.move(...at(to, dy), { steps: 4 }); await page.mouse.up();
    if (alt) await page.keyboard.up("Alt");
  };
  // C4 at step 0 lengthened to 4 steps; B3 (one row below) at step 8 lengthened to 2 steps
  await page.mouse.dblclick(...at(0.5)); await page.waitForTimeout(450);
  await drag(0.9, 3.9);
  await page.mouse.dblclick(...at(8.5, 12)); await page.waitForTimeout(450);
  await drag(8.9, 9.9, 12);
  const lens = async () => (await song()).clips[0].notes.map((n: number[]) => n[1]);
  expect(await lens()).toEqual([960, 480]);
  await page.locator("#gridc").press("ControlOrMeta+a");
  // C4 4 → 3 steps (−25%): B3 2 → 1.5 steps
  await drag(3.9, 2.9);
  expect(await lens()).toEqual([720, 360]);
  // Alt: every selected note by the same amount (+1 step)
  await drag(2.9, 3.9, 0, true);
  expect(await lens()).toEqual([960, 600]);
}, 60_000);

test("loop is off by default; the computer keyboard plays the selected track (tracker layout)", async () => {
  await open();
  expect(await page.locator("#loop-btn").getAttribute("aria-pressed")).toBe("false");
  await page.locator('.thdr.track[data-track="pad"] .name').click(); // selecting a track arms it
  expect(await page.locator("#armed-info").textContent()).toContain("Pad");
  await page.locator("#lanes-wrap").focus();
  await page.keyboard.down("q"); // Q = C one octave above Z
  await page.waitForTimeout(600);
  const pk = await page.evaluate(() => [...(window as any).DAW.engine.status.peaks]);
  expect(await page.locator("#keys button.on").count()).toBe(1);
  await page.keyboard.up("q");
  expect(pk[2 * 3]).toBeGreaterThan(0.005); // Pad (track 3) sounds
  expect(await page.locator("#keys button.on").count()).toBe(0);
}, 60_000);

test("record: a knob turned while playing writes its lane (created), one undo step", async () => {
  await open();
  await page.locator('.thdr.track[data-track="lead"] .name').click();
  await page.locator("#tab-dev").click();
  expect(track(await song(), "lead").lanes.length).toBe(0);
  await page.locator("#rec").click();
  await page.locator("#play").click();
  await page.waitForTimeout(400);
  const kb = (await page.locator('.knob[data-param="filt1.cutoff"]').first().boundingBox())!;
  await page.mouse.move(kb.x + kb.width / 2, kb.y + kb.height / 2);
  await page.mouse.down();
  for (let i = 0; i < 12; i++) { await page.mouse.move(kb.x + kb.width / 2, kb.y + kb.height / 2 - i * 4); await page.waitForTimeout(50); }
  await page.mouse.up();
  await page.locator("#play").click();
  await page.locator("#rec").click();
  const ln = track(await song(), "lead").lanes;
  expect(ln.map((l: any) => l.param)).toEqual(["filt1.cutoff"]);
  expect(ln[0].points.length).toBeGreaterThan(5);
  expect(ln[0].points.at(-1)[1]).toBeGreaterThan(ln[0].points[0][1]); // the turn went up
  expect(await page.locator('.lane.auto[aria-label^="Lead "]').count()).toBe(1); // shown in the track list
  await page.locator('[aria-label="Undo"]').click();
  expect(track(await song(), "lead").lanes.length).toBe(0);
}, 60_000);

test("Import sample encodes a GSM sample into the instrument; Remove unused drops samples no slot plays", async () => {
  // 0.5 s stereo 48 kHz WAVs (the browser resamples to 44.1 kHz and mixes to mono); two pitches = two samples
  const tone = (hz: number) => {
    const n = 24000, wav = Buffer.alloc(44 + n * 4);
    wav.write("RIFF", 0); wav.writeUInt32LE(36 + n * 4, 4); wav.write("WAVEfmt ", 8); wav.writeUInt32LE(16, 16);
    wav.writeUInt16LE(1, 20); wav.writeUInt16LE(2, 22); wav.writeUInt32LE(48000, 24); wav.writeUInt32LE(192000, 28); wav.writeUInt16LE(4, 32); wav.writeUInt16LE(16, 34);
    wav.write("data", 36); wav.writeUInt32LE(n * 4, 40);
    for (let i = 0; i < n; i++) { const v = Math.round(12000 * Math.sin((i * 2 * Math.PI * hz) / 48000)); wav.writeInt16LE(v, 44 + i * 4); wav.writeInt16LE(v, 46 + i * 4); }
    const file = join(root, `tone${hz}.wav`); writeFileSync(file, wav); return file;
  };
  await open();
  await page.locator('.thdr.track[data-track="lead"] .name').click();
  await page.locator("#tab-dev").click();
  // the Lead has no sampler: Import sample… uses a free oscillator slot; the second import leaves the first unused
  for (const [btn, hz] of [[".devbox .impsmp", 220], [".sampler .gsmimport", 330]] as const) {
    const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.locator(btn).first().click()]);
    await chooser.setFiles(tone(hz));
    await page.waitForTimeout(600);
    expect(await page.locator("#edit-status").textContent()).toMatch(new RegExp(`tone${hz}\\.wav: 0\\.50 s → [\\d.]+ KB GSM`));
  }
  let d = track(await song(), "lead").devices[0];
  expect(d.samples).toHaveLength(2);
  await page.locator(".sampler .gsmclean").click();
  d = track(await song(), "lead").devices[0];
  expect(d.samples).toHaveLength(1);
  const slot = [1, 2, 3].find((i) => d.values[`osc${i}.type`] === SAMPLER)!;
  expect([d.values[`osc${slot}.s7`], d.values[`osc${slot}.sample`]]).toEqual([0, 1]);
  expect(await page.evaluate(() => (window as any).DAW.engine.validate(JSON.stringify((window as any).DAW.debug.song())))).toEqual([]);
  // the imported sample sounds on the Lead
  await page.locator("#lanes-wrap").focus();
  await page.keyboard.down("q");
  await page.waitForTimeout(400);
  const pk = await page.evaluate(() => { const P = (window as any).DAW.engine.status.peaks; return Math.max(P[4], P[5]); });
  await page.keyboard.up("q");
  expect(pk).toBeGreaterThan(0.02);
}, 60_000);

test("active slots show screens: oscillator waveform, filter response with its cutoff mark, LFO shape", async () => {
  await open();
  await page.locator('.thdr.track[data-track="lead"] .name').click();
  await page.locator("#tab-dev").click();
  expect(await page.locator('.grp.slot[aria-label="Osc 1"] svg.scr path.ln').count()).toBe(1);
  const mark = () => page.locator('.grp.slot[aria-label="Filter 1"] svg.scr line.mk').getAttribute("x1");
  const curve = () => page.locator('.grp.slot[aria-label="Filter 1"] svg.scr path.ln').getAttribute("d");
  const [m0, c0] = [await mark(), await curve()];
  await page.locator('.knob[data-param="filt1.cutoff"]').focus();
  for (let i = 0; i < 10; i++) await page.keyboard.press("ArrowUp");
  await page.waitForTimeout(100);
  expect(+(await mark())!).toBeGreaterThan(+m0!);
  expect(await curve()).not.toBe(c0);
  // an LFO gets its shape once it is set
  await page.locator('#device select[aria-label$=" lfo1.shape"]').selectOption({ index: 1 });
  expect(await page.locator('.grp.slot[aria-label="LFO 1"] svg.scr path.ln').count()).toBe(1);
}, 60_000);

test("envelope curves redraw from their knobs; sampler settings are named controls with a draggable sample view", async () => {
  await open();
  await page.locator('.thdr.track[data-track="pad"] .name').click();
  await page.locator("#tab-dev").click();
  const env = () => page.locator('.grp[aria-label="Env 1"] svg.env polyline').getAttribute("points");
  const e0 = await env();
  await page.locator('.knob[data-param="env1.release"]').focus();
  for (let i = 0; i < 10; i++) await page.keyboard.press("ArrowDown");
  await page.waitForTimeout(100);
  expect(await env()).not.toBe(e0);
  // Tunefish mode: the time knobs read the Tunefish times, not the WaveSabre law
  const decay = () => page.locator('.knob[data-param="env1.decay"]').getAttribute("aria-valuetext");
  const ws = await decay();
  await page.locator('select[aria-label$="env1.law"]').selectOption({ label: "Rate" });
  await page.waitForTimeout(100);
  expect(await decay()).not.toBe(ws);
  await page.locator('select[aria-label$="env1.law"]').selectOption({ label: "Seconds" });
  // a sampler slot from an imported sample: loop mode / region / interp / reverse are controls, not bare knobs
  const n = 22050, wav = Buffer.alloc(44 + n * 2);
  wav.write("RIFF", 0); wav.writeUInt32LE(36 + n * 2, 4); wav.write("WAVEfmt ", 8); wav.writeUInt32LE(16, 16);
  wav.writeUInt16LE(1, 20); wav.writeUInt16LE(1, 22); wav.writeUInt32LE(44100, 24); wav.writeUInt32LE(88200, 28); wav.writeUInt16LE(2, 32); wav.writeUInt16LE(16, 34);
  wav.write("data", 36); wav.writeUInt32LE(n * 2, 40);
  for (let i = 0; i < n; i++) wav.writeInt16LE(Math.round(9000 * Math.sin(i / 20)), 44 + i * 2);
  const file = join(root, "loop.wav"); writeFileSync(file, wav);
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.locator(".devbox .impsmp").first().click()]);
  await chooser.setFiles(file);
  await page.waitForTimeout(600);
  const d = track(await song(), "pad").devices[0], i = [1, 2, 3].find((k) => d.values[`osc${k}.type`] === SAMPLER)!;
  const grp = page.locator(`.grp[aria-label="Osc ${i}"]`);
  expect(await grp.locator(".knob").evaluateAll((ks) => ks.map((k) => k.getAttribute("aria-label")).filter((x) => /start|loop/.test(x!)))).toEqual(["start"]);
  await page.locator(`select[aria-label="Osc ${i} loop"]`).selectOption("1");
  await page.locator(`select[aria-label="Osc ${i} region"]`).selectOption("1");
  expect(await grp.locator(".knob").evaluateAll((ks) => ks.map((k) => k.getAttribute("aria-label")).filter((x) => /start|loop/.test(x!)))).toEqual(["start", "loop start", "loop length"]);
  const w = page.locator(".smpwave");
  await w.scrollIntoViewIfNeeded();
  const bx = (await w.boundingBox())!;
  await page.mouse.move(bx.x + 1, bx.y + 20); await page.mouse.down();
  await page.mouse.move(bx.x + bx.width * 0.5, bx.y + 20, { steps: 4 }); await page.mouse.up();
  await page.waitForTimeout(200);
  const v = track(await song(), "pad").devices[0].values;
  expect([v[`osc${i}.s1`], v[`osc${i}.s2`]]).toEqual([0.5, 1]);
  expect(v[`osc${i}.s0`] > 0.4 || v[`osc${i}.s4`] > 0.4).toBe(true);  // the start line or the loop end moved to the middle
  await page.keyboard.press("Control+z");
  expect(JSON.stringify(track(await song(), "pad").devices[0].values)).not.toBe(JSON.stringify(v));
}, 60_000);
