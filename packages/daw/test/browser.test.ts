// DAW browser tests (change add-synth-daw, task 5.10): headless Chrome against the real server, engine module and
// a temporary project/music. Draw a note → heard in the bounce; a knob (keyboard only) changes the output; undo
// restores it exactly; save round trip; bounce == `song render`; axe accessibility check.
import { afterAll, beforeAll, expect, test } from "bun:test";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import type { Browser, Page } from "playwright";
import { startServer, type Server } from "../../cli/src/server.ts";
import { buildDaw, DIST } from "../build.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const root = mkdtempSync(join(tmpdir(), "daw-browser-"));
let s: Server;
let browser: Browser;
let page: Page;
const errors: string[] = [];

beforeAll(async () => {
  mkdirSync(join(root, "project"), { recursive: true });
  mkdirSync(join(root, "project/music"), { recursive: true });
  cpSync(join(ROOT, "project/music/demo.song.json"), join(root, "project/music/demo.song.json"));
  cpSync(join(ROOT, "project/music/patches"), join(root, "project/music/patches"), { recursive: true });
  await buildDaw({ wasm: false });
  s = await startServer({ port: 0, root, build: false, dawDist: DIST, headless: false });
  const { chromium } = await import("playwright");
  const channel = process.env.CHROME_CHANNEL ?? "chrome";
  browser = await chromium.launch({ channel: channel === "bundled" ? undefined : channel, headless: true, args: ["--autoplay-policy=no-user-gesture-required"] });
  page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, acceptDownloads: true });
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
}, 300_000);

afterAll(async () => {
  await browser?.close();
  await s?.stop();
  rmSync(root, { recursive: true, force: true });
});

async function open(song = "project/music/demo.song.json") {
  await page.goto(`${s.url}daw/?song=${song}`);
  await page.waitForSelector(".thdr.track");
  await page.waitForTimeout(300);
}

/** Bounces the whole song through the UI and returns the WAV bytes. */
async function bounce(): Promise<Uint8Array> {
  await page.locator("#bounce-btn").click();
  await page.locator('dialog.dlg button:has-text("Render")').click();
  const href = await page.locator("dialog.dlg .bres a").getAttribute("href", { timeout: 60_000 });
  const bytes: number[] = await page.evaluate(async (u) => [...new Uint8Array(await (await fetch(u!)).arrayBuffer())], href);
  await page.keyboard.press("Escape");
  return new Uint8Array(bytes);
}

/** RMS of the 16-bit stereo samples of a WAV between two times (s). */
function rms(wav: Uint8Array, from = 0, to = Infinity): number {
  const dv = new DataView(wav.buffer, wav.byteOffset + 44);
  const n = Math.min(dv.byteLength / 2, Math.floor(to * 44100) * 2);
  let sum = 0, k = 0;
  for (let i = Math.floor(from * 44100) * 2; i < n; i++, k++) sum += (dv.getInt16(2 * i, true) / 32768) ** 2;
  return Math.sqrt(sum / Math.max(1, k));
}

test("a drawn note is heard", async () => {
  await open();
  await page.locator("#new-btn").click();
  await page.waitForTimeout(300);
  expect(await page.locator(".thdr.track .name").allTextContents()).toEqual(["Track 1"]);
  // double-click the lane at bar 1 → a clip opens in the piano roll; double-click C4 at step 0 → a note
  const lane = (await page.locator(".lane.track").first().boundingBox())!;
  await page.mouse.dblclick(lane.x + 10, lane.y + lane.height / 2);
  await page.waitForTimeout(300);
  expect(await page.locator("#p-roll").isHidden()).toBe(false);
  const y = await page.evaluate(() => { const g = document.getElementById("grid")!; const row = (96 - 60) * 12; g.scrollTop = Math.max(0, row - 60); return row - g.scrollTop + 6; });
  const grid = (await page.locator("#gridc").boundingBox())!;
  const gr = (await page.locator("#grid").boundingBox())!;
  await page.mouse.dblclick(gr.x + 11, gr.y + y);
  await page.waitForTimeout(200);
  expect(grid.width).toBeGreaterThan(0);
  const wav = await bounce();
  expect(rms(wav, 0, 0.1)).toBeGreaterThan(0.01); // the note (beat 1, a sixteenth) sounds
  expect(rms(wav, 3, 4)).toBeLessThan(0.001); // and nothing else does
}, 120_000);

test("a knob turned from the keyboard changes the output and undo restores it exactly", async () => {
  await open();
  const before = await bounce();
  await page.locator('.thdr.track[data-track="kick"]').click();
  await page.locator("#tab-dev").click();
  const k = page.locator('#device .knob[data-param="master"]');
  const v0 = await k.getAttribute("aria-valuenow");
  await k.focus();
  await k.press("Home");
  expect(Number(await k.getAttribute("aria-valuenow"))).toBe(0);
  expect(await k.getAttribute("aria-valuetext")).toBe("−∞ dB");
  const muted = await bounce();
  expect(rms(muted)).toBeLessThan(rms(before));
  await page.locator('[aria-label="Undo"]').click();
  await page.locator("#tab-dev").click();
  expect(await page.locator('#device .knob[data-param="master"]').getAttribute("aria-valuenow")).toBe(v0);
  const back = await bounce();
  expect(Buffer.from(back).equals(Buffer.from(before))).toBe(true);
}, 120_000);

test("save writes the same song back, and the bounce equals `song render`", async () => {
  const file = join(root, "project/music/demo.song.json");
  const original = JSON.parse(readFileSync(file, "utf8"));
  await open();
  // an edit and its undo leave the song unchanged; Ctrl+S writes it through the validating /api/music
  await page.locator('.thdr.track[data-track="bass"] button[aria-label="Mute Bass"]').click();
  await page.locator('[aria-label="Undo"]').click();
  await page.keyboard.press("Control+s");
  await page.waitForTimeout(500);
  expect(await page.locator("#saved-info").textContent()).toMatch(/^Saved/);
  expect(JSON.parse(readFileSync(file, "utf8"))).toEqual(original);
  const wav = await bounce();
  spawnSync("cargo", ["build", "-q", "-p", "song", "--release"], { cwd: ROOT, env: { ...process.env, PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH}` } });
  const out = join(root, "cli.wav");
  const r = spawnSync(join(ROOT, "target/release/song"), ["render", file, "-o", out], { env: { ...process.env, GM_DLS: "" } });
  expect(r.status).toBe(0);
  const cli = readFileSync(out);
  expect(wav.length).toBe(cli.length);
  expect(Buffer.from(wav).equals(cli)).toBe(true);
}, 180_000);

test("an invalid edit cannot be saved and points at its JSON path", async () => {
  await open();
  await page.locator('.thdr.track[data-track="lead"] .name').dblclick();
  await page.keyboard.type("Kick");
  await page.keyboard.press("Enter");
  await page.keyboard.press("Control+s");
  expect(await page.locator("dialog.dlg h3").textContent()).toContain("Save blocked");
  expect(await page.locator("dialog.dlg .errs li").first().textContent()).toContain("/tracks/2/name");
  await page.keyboard.press("Escape");
}, 60_000);

test("plays through the AudioWorklet: position advances and meters move", async () => {
  await open();
  await page.locator("#play").click();
  await page.waitForTimeout(1500);
  const st = await page.evaluate(() => { const E = (window as any).DAW.engine; return { pos: E.status.position, playing: E.status.playing, peaks: [...E.status.peaks] }; });
  expect(st.playing).toBe(true);
  expect(st.pos).toBeGreaterThan(20_000);
  expect(st.peaks.some((x: number) => x > 0.01)).toBe(true);
  await page.locator("#play").click();
}, 60_000);

test("the computer keyboard auditions the armed track while stopped (5.8)", async () => {
  await open();
  await page.locator('.thdr.track[data-track="pad"] .arm').click(); // a sustained instrument (Glass Pad)
  expect(await page.locator("#armed-info").textContent()).toContain("Pad");
  await page.locator("#lanes-wrap").focus();
  await page.keyboard.down("z"); // C on the armed track (tracker layout: Z = C)
  await page.waitForTimeout(800);
  const pk = await page.evaluate(() => [...(window as any).DAW.engine.status.peaks]);
  await page.keyboard.up("z");
  expect(pk[2 * 3]).toBeGreaterThan(0.005); // track 3 = Pad (left peak)
  expect(await page.evaluate(() => (window as any).DAW.engine.status.playing)).toBe(false);
  expect(await page.locator("#keys button.on").count()).toBe(0); // released
}, 60_000);

test("a watched track's chord comes back in the status as one probe record per voice", async () => {
  await open();
  await page.locator('.thdr.track[data-track="pad"] .arm').click();
  await page.locator("#lanes-wrap").focus();
  await page.evaluate(() => (window as any).DAW.engine.watch(3)); // the pad (track index 3)
  for (const k of ["z", "c", "b"]) await page.keyboard.down(k); // C, E, G
  await page.waitForFunction(() => ((window as any).DAW.engine.status.probe ?? [])[1] >= 3, null, { timeout: 10_000 });
  const p: number[] = await page.evaluate(() => [...((window as any).DAW.engine.status.probe ?? [])]);
  for (const k of ["z", "c", "b"]) await page.keyboard.up(k);
  expect(p.slice(0, 4)).toEqual([1, 3, 4, 2048]);
  const notes = [0, 1, 2].map((v) => p[4 + v * 22 + 1]).sort();
  expect(notes[1] - notes[0]).toBe(4); // a major triad
  expect(notes[2] - notes[0]).toBe(7);
  await page.evaluate(() => (window as any).DAW.engine.watch(-1));
  await page.waitForTimeout(200);
  expect(await page.evaluate(() => (window as any).DAW.engine.status.probe ?? null)).toBe(null);
}, 60_000);

test("live view off: no watch, no probe, no overlays; the choice survives a reload", async () => {
  await open();
  await page.locator('.thdr.track[data-track="lead"] .name').click();
  await page.locator("#tab-dev").click();
  const btn = page.locator('#device button.livev');
  expect(await btn.getAttribute("aria-pressed")).toBe("true");
  await btn.click();
  expect(await btn.getAttribute("aria-pressed")).toBe("false");
  await page.locator("#lanes-wrap").focus();
  for (const k of ["z", "c"]) await page.keyboard.down(k);
  await page.waitForTimeout(600);
  const probe = await page.evaluate(() => (window as any).DAW.engine.status.probe);
  const chips = await page.locator("#device .vox i").count();
  const live = await page.locator("#device svg g.live").count();
  for (const k of ["z", "c"]) await page.keyboard.up(k);
  expect(probe).toBeNull();
  expect(chips).toBe(0);
  expect(live).toBe(0);
  await page.reload();
  await page.locator('.thdr.track[data-track="lead"] .name').click();
  await page.locator("#tab-dev").click();
  expect(await page.locator('#device button.livev').getAttribute("aria-pressed")).toBe("false");
  await page.locator('#device button.livev').click(); // back on for the next tests
  expect(await page.locator('#device button.livev').getAttribute("aria-pressed")).toBe("true");
}, 60_000);

test("live displays: a chord shows one chip and one marker per voice; silence clears them", async () => {
  await open();
  await page.locator('.thdr.track[data-track="lead"] .name').click(); // selecting arms it; lead: 2 saws into a low-pass
  await page.locator("#tab-dev").click();
  await page.locator("#lanes-wrap").focus();
  for (const k of ["z", "c", "b", "m"]) await page.keyboard.down(k); // C E G B
  // the overlay follows the ~30 ms status; under a loaded test run it can take longer than a fixed wait
  await page.waitForFunction(() => document.querySelectorAll("#device .vox i").length >= 4, null, { timeout: 10_000 });
  await page.waitForTimeout(200);
  const chips = await page.locator("#device .vox i").allTextContents();
  const envDots = await page.locator('#device .grp.slot[aria-label="Env 1"] svg.env g.live .vm').count();
  const spec = await page.locator('#device .grp.slot[aria-label="Filter 1"] svg.scr g.live path.sout').count();
  const ticks = await page.locator('#device .grp.slot[aria-label="Filter 1"] svg.scr g.live line.ct').count();
  // the low-pass at work: the tapped output of filter 1 carries less energy than its input
  const [ein, eout] = await page.evaluate(() => {
    const p = (window as any).DAW.engine.status.probe as Float32Array, o = 4 + p[1] * 22, L = p[3];
    const e = (a: Float32Array) => a.reduce((s, x) => s + x * x, 0);
    return p[o] === 1 ? [e(p.subarray(o + 1, o + 1 + L)), e(p.subarray(o + 1 + L, o + 1 + 2 * L))] : [0, 0];
  });
  for (const k of ["z", "c", "b", "m"]) await page.keyboard.up(k);
  expect(chips.length).toBe(4);
  expect(new Set(chips.map((c) => c.replace(/\d+$/, "")))).toEqual(new Set(["C", "E", "G", "B"]));
  expect(envDots).toBeGreaterThanOrEqual(4);
  expect(spec).toBe(1);
  expect(ticks).toBe(4);
  expect(ein).toBeGreaterThan(0);
  expect(eout).toBeLessThan(ein);
  await page.waitForTimeout(2500); // release and tails
  expect(await page.locator("#device .vox i").count()).toBe(0);
  expect(await page.locator('#device svg g.live').count()).toBe(0);
}, 60_000);

test("no critical or serious accessibility violations (axe)", async () => {
  await open();
  await page.locator("#tab-dev").click();
  await page.addScriptTag({ path: join(ROOT, "node_modules/axe-core/axe.min.js") });
  const res: any = await page.evaluate(async () => (window as any).axe.run(document, { resultTypes: ["violations"] }));
  const bad = res.violations.filter((v: any) => v.impact === "critical" || v.impact === "serious");
  if (bad.length) console.error(bad.map((v: any) => `${v.id} (${v.impact}): ${v.nodes.length} × ${v.nodes[0]?.target}`).join("\n"));
  expect(bad.map((v: any) => v.id)).toEqual([]);
}, 60_000);

test("a song whose first track automates an envelope time opens (lane labels format before the helpers load)", async () => {
  const d = JSON.parse(readFileSync(join(root, "project/music/demo.song.json"), "utf8"));
  const t = d.tracks.find((x: any) => x.devices?.[0]?.kind === 64);
  t.lanes = [{ device: 0, param: "env1.attack", points: [[0, 0.1], [3840, 0.5]] }];
  writeFileSync(join(root, "project/music/env-lane.song.json"), JSON.stringify(d));
  await open("project/music/env-lane.song.json");
  expect(await page.locator("[role=alert]").count()).toBe(0);
  expect(await page.locator(".lane").count()).toBeGreaterThan(0);
});

test("no page errors", () => {
  expect(errors).toEqual([]);
});

// After "no page errors": the simulated failure below logs an error on purpose.
test("seeks while playing keep playing; a dead engine is reported, reads 0% and Play restarts it", async () => {
  await open();
  await page.locator("#play").click();
  await page.waitForTimeout(800);
  for (const f of [3, 1, 6, 2]) {
    await page.evaluate((fr) => (window as any).DAW.engine.seek(fr), f * 44100); // seeks reset every device
    await page.waitForTimeout(250);
  }
  const st = () => page.evaluate(() => { const E = (window as any).DAW.engine; return { dead: E.dead, playing: E.status.playing, pos: E.status.position }; });
  expect(await st()).toMatchObject({ dead: false, playing: true });
  // the worklet reports a trap (what an engine panic looks like) and stops
  await page.evaluate(() => (window as any).DAW.engine.node.port.onmessage({ data: { t: "error", message: "unreachable" } }));
  await page.waitForTimeout(300);
  expect(await page.locator("#edit-status").textContent()).toContain("audio engine stopped");
  expect(await page.locator(".load + .num").textContent()).toBe("0%");
  expect(await page.locator("#play").getAttribute("aria-label")).toContain("Play");
  await page.locator("#play").click();
  await page.waitForTimeout(1200);
  expect(await st()).toMatchObject({ dead: false, playing: true });
  await page.locator("#play").click();
}, 60_000);

test("audio drops (output underruns while playing) show next to the engine meter with where they happened", async () => {
  await open();
  const drops = page.locator("#drops");
  await page.locator("#play").click();
  await page.waitForFunction(() => (window as any).DAW.engine.ctx?.state === "running", null, { timeout: 15_000 });
  // the browser's own counter (AudioContext.playbackStats, seconds); a real underrun cannot be forced, so the test
  // replaces the live object on this context
  await page.evaluate(() => {
    const st = { underrunEvents: 0, underrunDuration: 0, totalDuration: 1, averageLatency: 0, minimumLatency: 0, maximumLatency: 0 };
    (window as any).__st = st;
    Object.defineProperty((window as any).DAW.engine.ctx, "playbackStats", { get: () => st, configurable: true });
  });
  await page.waitForTimeout(2000); // past the start-up grace (1.5 s after Play)
  expect(await drops.textContent()).toBe("0 drops");
  await page.evaluate(() => { const st = (window as any).__st; st.underrunEvents = 2; st.underrunDuration = 0.012; });
  await page.waitForFunction(() => document.getElementById("drops")!.textContent === "2 drops");
  expect(await drops.getAttribute("class")).toContain("bad");
  expect(await drops.getAttribute("title")).toMatch(/12 ms of silence, at \d+\.\d+\.\d+/);
  // a new Play counts from zero again
  await page.locator("#play").click();
  await page.locator("#play").click();
  await page.waitForTimeout(300);
  expect(await drops.textContent()).toBe("0 drops");
  await page.locator("#play").click();
}, 60_000);

test("Run open filters: a remembered listening switch that reaches the live engine and survives an engine restart", async () => {
  await open();
  const box = page.locator("#openflt");
  expect(await box.isChecked()).toBe(false);
  await page.locator("#play").click();
  await page.waitForFunction(() => (window as any).DAW.engine.ctx?.state === "running", null, { timeout: 15_000 });
  // what the worklet is told
  await page.evaluate(() => { const E = (window as any).DAW.engine; (window as any).__sent = []; const post = E.post.bind(E); E.post = (m: any) => { (window as any).__sent.push(m); post(m); }; });
  await box.check();
  expect(await page.evaluate(() => (window as any).__sent.filter((m: any) => m.t === "openflt"))).toEqual([{ t: "openflt", on: true }]);
  expect(await page.evaluate(() => localStorage.getItem("daw.openflt"))).toBe("1");
  await page.locator("#play").click();
  await page.reload();
  await page.waitForSelector(".thdr.track");
  expect(await page.locator("#openflt").isChecked()).toBe(true);
  expect(await page.evaluate(() => (window as any).DAW.engine.runOpen)).toBe(true);
  await page.locator("#openflt").uncheck();
}, 60_000);
