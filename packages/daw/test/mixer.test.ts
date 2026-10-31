// Mixer panel (spec synth-daw; contract for the SolidJS port, change daw-solid-ts): one strip per track + master,
// fader / mute / solo / sends / inserts / bypass edit the song and undo, meters follow the engine while playing.
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
const root = mkdtempSync(join(tmpdir(), "daw-mix-"));
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
  await page.locator("#tab-mix").click();
}, 300_000);

afterAll(async () => {
  await browser?.close();
  await s?.stop();
  rmSync(root, { recursive: true, force: true });
});

const song = () => page.evaluate(() => (window as any).DAW.debug.song());
const track = async (id: string) => { const sg = await song(); return id === "master" ? sg.master : sg.tracks.find((t: any) => t.id === id); };
const strip = (name: string) => page.getByRole("group", { name: `${name} channel strip` });
const undo = () => page.keyboard.press("Control+z");

test("one strip per track, the master last, + Bus before it; inserts and sends listed", async () => {
  const names = await page.locator("#mixer .strip .sn").allTextContents();
  expect(names).toEqual(["Kick", "Bass", "Lead", "Pad", "Verb", "Master"]);
  expect(await page.locator("#mixer > *").nth(5).getAttribute("class")).toBe("addbus");
  expect(await strip("Bass").locator(".inslist .ins").count()).toBe(1);
  expect(await strip("Kick").locator(".inslist .none").count()).toBe(1);
  expect(await strip("Lead").locator(".sendlist li").first().textContent()).toContain("→ Verb");
  expect(await strip("Master").locator(".sendlist").count()).toBe(0);
});

test("fader (keyboard) changes the volume, its dB readout and the header fader; one undo per key step", async () => {
  const before = (await track("kick")).volume;
  const f = strip("Kick").locator(".fader");
  const db0 = Number(await f.inputValue());
  await f.focus();
  for (let i = 0; i < 10; i++) await page.keyboard.press("ArrowDown");
  await f.blur();
  expect(Number(await f.inputValue())).toBeCloseTo(db0 - 1, 5);
  expect(await strip("Kick").locator(".db").textContent()).toBe(`${(db0 - 1).toFixed(1)} dB`);
  expect(Number(await page.locator('.thdr.track[data-track="kick"] input[aria-label="Kick volume"]').inputValue())).toBeCloseTo(db0 - 1, 5);
  expect((await track("kick")).volume).toBeLessThan(before);
  for (let i = 0; i < 10; i++) await undo();
  expect((await track("kick")).volume).toBe(before);
  expect(Number(await strip("Kick").locator(".fader").inputValue())).toBeCloseTo(db0, 5);
});

test("mute and solo toggle aria-pressed and the track; undo restores", async () => {
  await strip("Kick").getByRole("button", { name: "Mute Kick" }).click();
  expect(await strip("Kick").getByRole("button", { name: "Mute Kick" }).getAttribute("aria-pressed")).toBe("true");
  expect((await track("kick")).mute).toBe(true);
  await page.waitForTimeout(50); // the mute commit is deferred (setTimeout) in the click handler
  await undo();
  expect((await track("kick")).mute).toBe(false);
  await strip("Pad").getByRole("button", { name: "Solo Pad" }).click();
  expect(await strip("Pad").getByRole("button", { name: "Solo Pad" }).getAttribute("aria-pressed")).toBe("true");
  await strip("Pad").getByRole("button", { name: "Solo Pad" }).click();
  expect(await strip("Pad").getByRole("button", { name: "Solo Pad" }).getAttribute("aria-pressed")).toBe("false");
});

test("add a send (no loop targets offered), change its level, remove it; undo brings it back", async () => {
  await strip("Kick").getByRole("button", { name: "Add send from Kick" }).click();
  const pick = strip("Kick").getByRole("combobox", { name: "Send target for Kick" });
  const opts = await pick.locator("option").allTextContents();
  expect(opts).toContain("Verb · main");
  expect(opts.some((o) => o.startsWith("Kick"))).toBe(false);
  await pick.selectOption("Verb|main");
  const recv = async () => (await track("verb")).receives.filter((r: any) => r.from === "kick");
  expect((await recv()).length).toBe(1);
  const lvl = strip("Kick").getByRole("slider", { name: "Send Kick to Verb main level" });
  await lvl.focus();
  for (let i = 0; i < 5; i++) await page.keyboard.press("ArrowUp");
  await lvl.blur();
  expect(await lvl.getAttribute("aria-valuetext")).toBe("-11.5 dB");
  await strip("Kick").getByRole("button", { name: "Remove send Kick to Verb" }).click();
  expect((await recv()).length).toBe(0);
  expect(await strip("Kick").locator(".sendlist .none").count()).toBe(1);
  await undo();
  expect((await recv()).length).toBe(1);
  for (let i = 0; i < 5; i++) await undo(); // one undo per key step of the send level
  expect(await lvl.getAttribute("aria-valuetext")).toBe("-12.0 dB");
  await undo();
  expect((await recv()).length).toBe(0);
});

test("add an insert, bypass it, the strip follows; undo removes it", async () => {
  const n0 = (await track("pad")).devices.length;
  await strip("Pad").getByRole("button", { name: "Add insert effect to Pad" }).click();
  const pick = strip("Pad").getByRole("combobox", { name: "Effect to insert on Pad" });
  const fx = (await pick.locator("option").allTextContents())[1];
  await pick.selectOption(fx);
  expect((await track("pad")).devices.length).toBe(n0 + 1);
  await page.locator("#tab-mix").click();
  const byp = strip("Pad").locator(".byp").first();
  await byp.click();
  expect(await strip("Pad").locator(".byp").first().getAttribute("aria-pressed")).toBe("true");
  expect(await strip("Pad").locator(".inslist li").first().getAttribute("class")).toBe("off");
  await undo(); await undo();
  expect((await track("pad")).devices.length).toBe(n0);
});

test("clicking a strip name selects the track", async () => {
  await strip("Bass").getByRole("button", { name: "Select Bass" }).click();
  expect(await page.evaluate(() => (window as any).DAW.debug.selected())).toBe("bass");
  expect(await strip("Bass").getAttribute("class")).toContain("sel");
});

test("a hidden mixer has no DOM; showing it again rebuilds the strips", async () => {
  await page.locator("#tab-dev").click();
  expect(await page.locator("#mixer").evaluate((m) => m.childElementCount)).toBe(0);
  await page.locator("#tab-mix").click();
  expect(await page.locator("#mixer .strip").count()).toBe(6);
});

test("edits while the mixer is hidden show when it opens (header mute, then undo of a fader step)", async () => {
  await page.locator("#tab-dev").click();
  await page.locator('.thdr.track[data-track="lead"]').getByRole("button", { name: "Mute Lead" }).click();
  await page.locator("#tab-mix").click();
  expect(await strip("Lead").getByRole("button", { name: "Mute Lead" }).getAttribute("aria-pressed")).toBe("true");
  const f = strip("Lead").locator(".fader"), db0 = Number(await f.inputValue());
  await f.focus(); await page.keyboard.press("ArrowDown"); await f.blur();
  await page.locator("#tab-dev").click();
  await page.waitForTimeout(50);
  await page.keyboard.press("Control+z"); await page.waitForTimeout(50); await page.keyboard.press("Control+z");
  await page.locator("#tab-mix").click();
  expect(Number(await strip("Lead").locator(".fader").inputValue())).toBeCloseTo(db0, 5);
  expect(await strip("Lead").locator(".db").textContent()).toBe(`${db0.toFixed(1)} dB`);
  expect(await strip("Lead").getByRole("button", { name: "Mute Lead" }).getAttribute("aria-pressed")).toBe("false");
});

test("meters follow the engine while playing (meter attributes only, no strip rebuilt) and fall back after stop", async () => {
  await page.locator("#tab-mix").click();
  await page.evaluate(() => {
    const w = window as any; w.__mut = { nodes: 0, attrs: 0, other: 0 };
    w.__mo = new MutationObserver((rs) => { for (const r of rs) {
      if (r.type === "childList") w.__mut.nodes++;
      else if ((r.target as Element).closest(".meter")) w.__mut.attrs++;
      else w.__mut.other++;
    } });
    w.__mo.observe(document.getElementById("mixer"), { subtree: true, childList: true, attributes: true, characterData: true });
  });
  await page.keyboard.press("Space");
  await page.waitForTimeout(1500);
  const mut = await page.evaluate(() => { const w = window as any; w.__mo.disconnect(); return w.__mut; });
  expect(mut.attrs).toBeGreaterThan(0);
  expect(mut.nodes).toBe(0);
  expect(mut.other).toBe(0);
  const vals = await page.locator("#mixer .meter").evaluateAll((ms) => ms.map((m) => Number(m.getAttribute("aria-valuenow"))));
  expect(Math.max(...vals)).toBeGreaterThan(0.01);
  await page.keyboard.press("Space");
  await page.waitForTimeout(2500);
  const after = await page.locator("#mixer .meter").evaluateAll((ms) => ms.map((m) => Number(m.getAttribute("aria-valuenow"))));
  expect(Math.max(...after)).toBeLessThan(Math.max(...vals));
  expect(errors).toEqual([]);
});
