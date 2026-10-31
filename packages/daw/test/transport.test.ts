// Transport bar (spec synth-daw: Transport bar, Loop editing, Song length, Playback display; contract for the
// SolidJS port, change daw-solid-ts 3.1): buttons, fields and readouts keep their names, values and behaviour.
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
const root = mkdtempSync(join(tmpdir(), "daw-tr-"));
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
  await page.goto(`${s.url}daw/?song=project/music/demo.song.json`);
  await page.waitForSelector(".thdr.track");
}, 300_000);

afterAll(async () => {
  await browser?.close();
  await s?.stop();
  rmSync(root, { recursive: true, force: true });
});

const pressed = (sel: string) => page.locator(sel).getAttribute("aria-pressed");
const text = (sel: string) => page.locator(sel).textContent();
const bars = () => page.evaluate(() => (window as any).DAW.debug.bars());
const body = () => page.locator("body").focus().catch(() => page.evaluate(() => (document.activeElement as HTMLElement)?.blur()));
/** Clicks the ruler below the loop strip at a bar (seek). */
async function seekTo(bar: number) {
  const box = (await page.locator(`#ruler .bar:text-is("${bar}")`).boundingBox())!;
  await page.mouse.click(box.x + 0.5, box.y + box.height - 2);
}

test("initial state: loop off, follow on, record off, position at the start, length = song bars", async () => {
  expect(await pressed("#loop-btn")).toBe("false");
  expect(await pressed("#follow")).toBe("true");
  expect(await pressed("#rec")).toBe("false");
  expect(await text("#pos")).toBe("1.1.0");
  expect(await text("#time")).toBe("0:00.000");
  expect(Number(await page.locator("#len-in").inputValue())).toBe(await bars());
  expect(await page.getByRole("spinbutton", { name: "Tempo in BPM" }).count()).toBe(1);
  expect(await page.locator("#play").getAttribute("aria-label")).toBe("Play (Space)");
});

test("seek on the ruler; the time follows the tempo field", async () => {
  await seekTo(3);
  expect(await text("#pos")).toMatch(/^3\.1\./);
  const tempo = page.getByRole("spinbutton", { name: "Tempo in BPM" });
  await tempo.fill("120"); await tempo.press("Enter"); await body();
  await seekTo(3);   // (the old bar refreshes the time only on the next position update)
  const t120 = Number((await text("#time"))!.split(":")[1]);
  await tempo.fill("240"); await tempo.press("Enter"); await body();
  await seekTo(3);
  const t240 = Number((await text("#time"))!.split(":")[1]);
  expect(t120 / t240).toBeCloseTo(2, 2);
  await tempo.fill("128"); await tempo.press("Enter"); await body();
});

test("signature sets the beat grid and the beats of the position", async () => {
  await page.selectOption("#sig", "3/4");
  expect(await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--beats").trim())).toBe("3");
  await seekTo(2);
  expect(await text("#pos")).toMatch(/^2\.[123]\./);
  await page.selectOption("#sig", "4/4");
});

test("loop field: valid range turns the loop on, invalid input is restored; ⟲ toggles", async () => {
  const loopIn = page.locator("#loop-in");
  await loopIn.fill("5 – 9"); await loopIn.press("Enter");
  expect(await pressed("#loop-btn")).toBe("true");
  expect(await page.locator(".loopstrip [role=group], .loopstrip + *, [aria-label^='Loop 5 to 9']").first().getAttribute("aria-label")).toContain("Loop 5 to 9");
  await loopIn.fill("9 – 5"); await loopIn.press("Enter");
  expect(await loopIn.inputValue()).toBe("5 – 9");
  await page.locator("#loop-btn").click();
  expect(await pressed("#loop-btn")).toBe("false");
  expect(await page.locator("[aria-label^='Loop 5 to 9']").getAttribute("aria-label")).toContain(", off");
});

test("length field: grows the song, never below the last clip", async () => {
  const len = page.locator("#len-in"), b0 = await bars();
  await len.fill(String(b0 + 8)); await len.press("Enter");
  expect(await bars()).toBe(b0 + 8);
  await len.fill("1"); await len.press("Enter");
  const kept = await bars();
  expect(kept).toBeGreaterThan(1);
  expect(Number(await len.inputValue())).toBe(kept);
});

test("follow and record toggle (buttons and F / Shift+R)", async () => {
  await page.locator("#follow").click();
  expect(await pressed("#follow")).toBe("false");
  await body(); await page.keyboard.press("f");
  expect(await pressed("#follow")).toBe("true");
  await body(); await page.keyboard.press("Shift+R");
  expect(await pressed("#rec")).toBe("true");
  await page.locator("#rec").click();
  expect(await pressed("#rec")).toBe("false");
});

test("play: position and time move, the load readout shows; an edit shows 'edited since play'; stop twice returns to the start", async () => {
  await seekTo(1);
  await body(); await page.keyboard.press("Space");
  await page.waitForTimeout(1200);
  expect(await page.locator("#play").getAttribute("aria-label")).toBe("Pause (Space)");
  expect(await text("#pos")).not.toBe("1.1.0");
  expect(await text("#time")).not.toBe("0:00.000");
  // the demo is light: its load can round to 0 %, so only the readout's form is checked while playing
  expect(Number(await page.locator(".load").getAttribute("aria-valuenow"))).toBeGreaterThanOrEqual(0);
  expect(await page.locator(".load + .num").textContent()).toMatch(/^\d+%$/);
  expect(await page.locator("#edited").isHidden()).toBe(true);
  await page.locator("#tab-mix").click();
  const f = page.locator('#mixer input[aria-label="Kick volume"]');
  await f.focus(); await page.keyboard.press("ArrowDown"); await f.blur();
  await page.waitForTimeout(300);
  expect(await page.locator("#edited").isVisible()).toBe(true);
  await page.locator("#stop").click();
  expect(await page.locator("#play").getAttribute("aria-label")).toBe("Play (Space)");
  const stoppedAt = await text("#pos");
  await page.waitForTimeout(300);
  expect(await text("#pos")).toBe(stoppedAt);
  expect(stoppedAt).not.toBe("1.1.0");
  // (no load check after the first Stop: the tails still ring, the engine still renders)
  await page.locator("#stop").click();
  expect(await text("#pos")).toBe("1.1.0");
  await page.keyboard.press("Control+z");
});

test("buffer and open-filter settings are remembered", async () => {
  await page.selectOption("#buffer", "balanced");
  await page.locator("#openflt").check();
  expect(await page.evaluate(() => [localStorage.getItem("daw.buffer"), localStorage.getItem("daw.openflt")])).toEqual(["balanced", "1"]);
  await page.reload(); await page.waitForSelector(".thdr.track");
  expect(await page.locator("#buffer").inputValue()).toBe("balanced");
  expect(await page.locator("#openflt").isChecked()).toBe(true);
  await page.selectOption("#buffer", "playback"); await page.locator("#openflt").uncheck();
});

test("header Undo / Redo buttons undo and redo the last edit", async () => {
  await page.locator("#tab-mix").click();
  const f = page.locator('#mixer input[aria-label="Bass volume"]'), v0 = await f.inputValue();
  await f.focus(); await page.keyboard.press("ArrowDown"); await f.blur();
  const v1 = await f.inputValue();
  expect(v1).not.toBe(v0);
  await page.getByRole("button", { name: "Undo" }).click();
  expect(await f.inputValue()).toBe(v0);
  await page.getByRole("button", { name: "Redo" }).click();
  expect(await f.inputValue()).toBe(v1);
  expect(errors).toEqual([]);
});
