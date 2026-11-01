// Global keyboard shortcuts (docs/synth/daw.md key table; contract for ui/keys.ts, change daw-solid-ts 8.1): each
// global row, plus the precedence: text fields first, the computer keyboard's note keys before letter shortcuts,
// Ctrl/⌘+S from anywhere, undo/redo and zoom after the focused editor. Editor-local rows (clipboard, arrows, Q,
// F2, Alt+↑↓, Ctrl+Delete, Shift+F10) are covered by arrangement.test.ts, roll.test.ts and files.test.ts.
// Headless Chrome against the real server on a temporary project/music.
import { afterAll, beforeAll, expect, test } from "bun:test";
import { cpSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Browser, Page } from "playwright";
import { startServer, type Server } from "../../cli/src/server.ts";
import { buildDaw, DIST } from "../build.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const root = mkdtempSync(join(tmpdir(), "daw-keys-"));
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

async function fresh() {
  await page.goto(`${s.url}daw/?song=project/music/demo.song.json`);
  await page.waitForSelector(".thdr.track");
}
const pressed = (sel: string) => page.locator(sel).getAttribute("aria-pressed");
const text = (sel: string) => page.locator(sel).textContent();
const depth = (): Promise<number> => page.evaluate(() => (window as any).DAW.debug.undoDepth());
const body = () => page.evaluate(() => (document.activeElement as HTMLElement)?.blur());
const lane = (id: string) => page.locator(`.lane.track[data-track="${id}"]`);
const said = () => text("#live");

test("Space plays and pauses; Home goes to the song start; never from a text field", async () => {
  await fresh(); await body();
  await page.keyboard.press("Space");
  await page.waitForFunction(() => document.getElementById("play")!.getAttribute("aria-label") === "Pause (Space)");
  await page.waitForTimeout(400);
  await page.keyboard.press("Space");
  expect(await page.locator("#play").getAttribute("aria-label")).toBe("Play (Space)");
  expect(await text("#pos")).not.toBe("1.1.0");
  await page.keyboard.press("Home");
  expect(await text("#pos")).toBe("1.1.0");
  await page.locator("#len-in").focus();
  await page.keyboard.press("Space");
  await page.waitForTimeout(150);
  expect(await page.locator("#play").getAttribute("aria-label")).toBe("Play (Space)");
}, 60_000);

test("L toggles the loop, Ctrl+L loops the selected clip, F follow, Shift+R record; letters in a field type", async () => {
  await fresh(); await body();
  await page.keyboard.press("l"); expect(await pressed("#loop-btn")).toBe("true");
  await page.keyboard.press("l"); expect(await pressed("#loop-btn")).toBe("false");
  const cb = (await lane("lead").locator(".clip").first().boundingBox())!;
  await page.mouse.click(cb.x + 30, cb.y + cb.height * 0.7);
  await page.keyboard.press("Control+l");
  expect(await pressed("#loop-btn")).toBe("true");
  expect(await page.locator("#loop-in").inputValue()).toBe("5 – 9");
  await page.keyboard.press("f"); expect(await pressed("#follow")).toBe("false");
  await page.keyboard.press("f"); expect(await pressed("#follow")).toBe("true");
  await page.keyboard.press("Shift+R"); expect(await pressed("#rec")).toBe("true");
  await page.keyboard.press("Shift+R"); expect(await pressed("#rec")).toBe("false");
  await page.getByRole("spinbutton", { name: "Tempo in BPM" }).focus();
  await page.keyboard.press("l"); await page.keyboard.press("f");
  expect(await pressed("#loop-btn")).toBe("true");
  expect(await pressed("#follow")).toBe("true");
}, 60_000);

test("Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y undo and redo from any control, not inside a text field", async () => {
  await fresh();
  await page.locator('.thdr.track[data-track="bass"] [aria-label^="Mute"]').first().click();
  await page.waitForTimeout(50);
  const d = await depth();
  await page.locator("#tab-mix").focus();
  await page.keyboard.press("Control+z"); expect(await depth()).toBe(d - 1);
  await page.keyboard.press("Control+Shift+z"); expect(await depth()).toBe(d);
  await page.keyboard.press("Control+z"); await page.keyboard.press("Control+y"); expect(await depth()).toBe(d);
  await page.locator("#loop-in").focus();
  await page.keyboard.press("Control+z");
  expect(await depth()).toBe(d);
}, 60_000);

test("+ / − zoom and Shift+Z fits the area with focus: arrangement, then piano roll", async () => {
  await fresh();
  await page.locator("#lanes-wrap").focus();
  await page.keyboard.press("+"); expect(await said()).toBe("arrangement zoom 150%");
  await page.keyboard.press("-"); expect(await said()).toBe("arrangement zoom 100%");
  await page.keyboard.press("Shift+Z"); expect(await said()).toMatch(/^zoomed to (the song|\d+ clips)$/);
  await page.locator('.thdr.track[data-track="bass"] .name').click();
  await page.locator("#tab-roll").click();
  await page.locator("#gridc").focus();
  await page.keyboard.press("+"); expect(await said()).toBe("piano roll zoom 150% × 100%");
  await page.keyboard.press("Shift+Z"); expect(await said()).toBe("zoomed to the clip");
}, 60_000);

test("computer keyboard: note keys win over letter shortcuts; [ ] octave, ; ' velocity, ` on/off", async () => {
  await fresh();
  await page.locator('.thdr.track[data-track="pad"] .name').click();  // selecting arms
  await page.locator("#tab-roll").click();
  await page.locator("#lanes-wrap").focus();
  expect(await text("#armed-info")).toBe("Armed: Pad · octave C3 · vel 100");
  await page.keyboard.press("BracketRight"); await page.keyboard.press("Quote");
  expect(await text("#armed-info")).toBe("Armed: Pad · octave C4 · vel 116");
  await page.keyboard.press("BracketLeft"); await page.keyboard.press("BracketLeft"); await page.keyboard.press("Semicolon");
  expect(await text("#armed-info")).toBe("Armed: Pad · octave C2 · vel 100");
  const d = await depth();
  await page.keyboard.down("r");  // a note (E), not Shift+R / anything else
  await page.waitForTimeout(100);
  expect(await page.locator("#keys button.on").count()).toBe(1);
  await page.keyboard.up("r");
  expect(await page.locator("#keys button.on").count()).toBe(0);
  expect(await pressed("#rec")).toBe("false");
  expect(await depth()).toBe(d);
  await page.keyboard.press("Backquote"); expect(await pressed("#kb-btn")).toBe("false");
  await page.keyboard.down("z"); await page.waitForTimeout(100);
  expect(await page.locator("#keys button.on").count()).toBe(0);
  await page.keyboard.up("z");
  await page.keyboard.press("Backquote"); expect(await pressed("#kb-btn")).toBe("true");
}, 60_000);

test("Ctrl+S saves from anywhere, a text field included", async () => {
  await fresh();
  await page.locator('.thdr.track[data-track="bass"] [aria-label^="Mute"]').first().click();
  await page.waitForFunction(() => document.querySelector("#save")!.classList.contains("dirty"));
  await page.locator("#loop-in").focus();
  await page.keyboard.press("Control+s");
  await page.waitForFunction(() => !document.querySelector("#save")!.classList.contains("dirty"));
  expect(await said()).toMatch(/^saved project\/music\/demo\.song\.json/);
}, 60_000);

test("no page errors", () => {
  expect(errors).toEqual([]);
});
