// Mod wheel and pitch bend (change add-synth-daw tasks 2.5a/5.5a): a pitch-bend point drawn in the controller lane
// is saved in the clip, heard in the bounce, and the bounce equals `song render` of the saved file (the clip
// envelope is flattened into ctl.bend automation on export); the audition sliders set the armed instrument's
// ctl.wheel / ctl.bend live. Headless Chrome against the real server on a temporary project/music.
import { afterAll, beforeAll, expect, test } from "bun:test";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
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
}, 300_000);

afterAll(async () => {
  await browser?.close();
  await s?.stop();
  rmSync(root, { recursive: true, force: true });
});

async function bounce(): Promise<Uint8Array> {
  await page.locator("#bounce-btn").click();
  await page.locator('dialog.dlg button:has-text("Render")').click();
  const href = await page.locator("dialog.dlg .bres a").getAttribute("href", { timeout: 60_000 });
  const bytes: number[] = await page.evaluate(async (u) => [...new Uint8Array(await (await fetch(u!)).arrayBuffer())], href);
  await page.keyboard.press("Escape");
  return new Uint8Array(bytes);
}

test("a pitch-bend point is saved, heard in the bounce, and the bounce equals `song render`", async () => {
  const file = join(root, "project/music/demo.song.json");
  await page.goto(`${s.url}daw/?song=project/music/demo.song.json&tab=roll`);
  await page.waitForSelector(".thdr.track");
  await page.locator('.thdr.track[data-track="lead"]').click();
  await page.waitForTimeout(300);
  const plain = await bounce();
  await page.selectOption("#cc-kind", "bend");
  const box = (await page.locator("#velc").boundingBox())!;
  await page.mouse.click(box.x + 4, box.y + 6); // near the clip start, at the top: +range
  await page.waitForTimeout(200);
  await page.keyboard.press("Control+s");
  await page.waitForTimeout(600);
  expect(await page.locator("#saved-info").textContent()).toMatch(/^Saved/);
  const saved = JSON.parse(readFileSync(file, "utf8"));
  const bent = saved.clips.filter((c: any) => c.bend?.length);
  expect(bent.length).toBe(1);
  expect(bent[0].bend[0][1]).toBeGreaterThan(0.9);
  const wav = await bounce();
  expect(Buffer.from(wav).equals(Buffer.from(plain))).toBe(false);
  spawnSync("cargo", ["build", "-q", "-p", "song", "--release"], { cwd: ROOT, env: { ...process.env, PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH}` } });
  const out = join(root, "cli.wav");
  const r = spawnSync(join(ROOT, "target/release/song"), ["render", file, "-o", out], { env: { ...process.env, GM_DLS: "" } });
  expect(r.status).toBe(0);
  expect(Buffer.from(wav).equals(readFileSync(out))).toBe(true);
}, 240_000);

test("the audition sliders set the armed instrument's ctl.wheel and ctl.bend live; PB springs back", async () => {
  await page.goto(`${s.url}daw/?song=project/music/demo.song.json`);
  await page.waitForSelector(".thdr.track");
  await page.locator('.thdr.track[data-track="lead"]').click();
  await page.keyboard.press("z"); // starts the audio engine (and plays C on the armed lead)
  await page.waitForTimeout(500);
  await page.keyboard.up("z");
  await page.evaluate(() => {
    const E = (window as any).DAW.engine;
    (window as any).calls = [];
    const orig = E.setParam.bind(E);
    E.setParam = (...a: unknown[]) => { (window as any).calls.push(a); return orig(...a); };
  });
  await page.evaluate(() => {
    const set = (id: string, v: number) => { const el = document.getElementById(id) as HTMLInputElement; el.value = String(v); el.dispatchEvent(new Event("input")); };
    set("mw", 127);
    set("pb", 8191);
    document.getElementById("pb")!.dispatchEvent(new PointerEvent("pointerup"));
  });
  const calls: [number, number, number, number][] = await page.evaluate(() => (window as any).calls);
  // [track, device, parameter index, value]: wheel 218 = 1, bend 219 = 1 then back to the centre (the v3 layout)
  expect(calls.map((c) => [c[2], +c[3].toFixed(3)])).toEqual([[218, 1], [219, 1], [219, 0.5]]);
  expect(await page.locator("#pb").inputValue()).toBe("0");
  expect(errors).toEqual([]);
}, 120_000);
