// Files, dialogs, menus, library (spec synth-daw; contract for the SolidJS port, change daw-solid-ts 4.1): the song
// list, the save prompt when switching songs, save (incl. the server's 422 path list), export, import, new song, the
// bounce dialog, the track menu by keyboard, and the library tree/inspector/audition/load. Complements
// browser.test.ts (save round-trip, local validation) and library.test.ts (converted legacy entries).
// Headless Chrome against the real server on a temporary project/music.
import { afterAll, beforeAll, expect, test } from "bun:test";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Browser, Page } from "playwright";
import { startServer, type Server } from "../../cli/src/server.ts";
import { buildDaw, DIST } from "../build.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const root = mkdtempSync(join(tmpdir(), "daw-files-"));
const music = join(root, "project/music");
let s: Server;
let browser: Browser;
let page: Page;
const errors: string[] = [];

beforeAll(async () => {
  mkdirSync(join(music, "patches"), { recursive: true });
  for (const f of ["demo.song.json", "global-groove.song.json", "patches/acid-bass.patch.json"]) cpSync(join(ROOT, "project/music", f), join(music, f));
  await buildDaw({ wasm: false });
  s = await startServer({ port: 0, root, build: false, dawDist: DIST, headless: false });
  const { chromium } = await import("playwright");
  const channel = process.env.CHROME_CHANNEL ?? "chrome";
  browser = await chromium.launch({ channel: channel === "bundled" ? undefined : channel, headless: true, args: ["--autoplay-policy=no-user-gesture-required"] });
  page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, acceptDownloads: true });
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

const current = () => page.locator("#songlist button[aria-current=true]").textContent();
const dialog = () => page.locator("dialog.dlg");
const saveDirty = () => page.locator("#save").evaluate((b) => b.classList.contains("dirty"));
const undoDepth = () => page.evaluate(() => (window as any).DAW.debug.undoDepth());
/** An undoable edit: mute the selected-track header's M button twice would cancel out, so toggle the lead's mute. */
async function edit() {
  await page.locator('.thdr.track[data-track="lead"] [aria-label^="Mute"]').first().click();
  await page.waitForTimeout(50);  // the mute commit is deferred
}

test("song list: title, length and composer per song (file name when there is no author), the current song marked with its track count; an edit adds the unsaved dot", async () => {
  expect(await page.locator("#songlist button").evaluateAll((bs) => bs.map((b) => b.getAttribute("data-file")))).toEqual(["project/music/demo.song.json", "project/music/global-groove.song.json"]);
  const row = (f: string) => page.locator(`#songlist button[data-file="project/music/${f}"]`);
  // demo has no meta: its file name as title and byline, 32 beats at 128 BPM = 0:15
  expect(await row("demo.song.json").locator(".tt").textContent()).toBe("demo");
  expect(await row("demo.song.json").locator(".du").textContent()).toBe("0:15");
  expect(await row("demo.song.json").locator(".by").textContent()).toBe("demo.song.json");
  // global-groove: meta title, composer without the source note (full author in the tooltip), 16.25 beats at 120 BPM = 0:08
  expect(await row("global-groove.song.json").locator(".tt").textContent()).toBe("Global Groove");
  expect(await row("global-groove.song.json").locator(".du").textContent()).toBe("0:08");
  expect(await row("global-groove.song.json").locator(".by").textContent()).toBe("WaveSabre Team");
  expect(await row("global-groove.song.json").getAttribute("title")).toContain("composer is not named");
  expect(await current()).toMatch(/^demo0:15demo\.song\.json\d+ tr$/);
  expect(await saveDirty()).toBe(false);
  await edit();
  expect(await current()).toMatch(/^• demo/);
  expect(await saveDirty()).toBe(true);
});

test("song search filters by title, composer and file name; Escape clears it", async () => {
  const q = page.locator("#song-q");
  const shown = () => page.locator("#songlist button").evaluateAll((bs) => bs.map((b) => b.getAttribute("data-file")));
  await q.fill("wavesabre");
  expect(await shown()).toEqual(["project/music/global-groove.song.json"]);
  await q.fill("DEMO.song");
  expect(await shown()).toEqual(["project/music/demo.song.json"]);
  await q.fill("nothing like this");
  expect(await shown()).toEqual([]);
  expect(await page.locator("#songs-none").textContent()).toBe("No song matches.");
  await q.press("Escape");
  expect(await q.inputValue()).toBe("");
  expect((await shown()).length).toBe(2);
});

test("switching with unsaved changes asks: Cancel stays, Don't save opens the other song", async () => {
  await page.locator('#songlist button[data-file$="/global-groove.song.json"]').click();
  await expect(dialog().getAttribute("aria-label")).resolves.toBe("Save changes to demo.song.json?");
  expect(await dialog().locator(".dacts button").allTextContents()).toEqual(["Save", "Don't save", "Cancel"]);
  expect(await dialog().locator("button.primary").textContent()).toBe("Save");
  await dialog().getByRole("button", { name: "Cancel" }).click();
  expect(await dialog().count()).toBe(0);
  expect(await current()).toMatch(/^• demo/);
  // Escape cancels too
  await page.locator('#songlist button[data-file$="/global-groove.song.json"]').click();
  await page.keyboard.press("Escape");
  expect(await dialog().count()).toBe(0);
  expect(await current()).toMatch(/^• demo/);
  await page.locator('#songlist button[data-file$="/global-groove.song.json"]').click();
  await dialog().getByRole("button", { name: "Don't save" }).click();
  await page.waitForFunction(() => document.querySelector("#songlist button[aria-current=true]")?.getAttribute("data-file")?.endsWith("/global-groove.song.json"));
  expect(await undoDepth()).toBe(0);
  // the opened song's own master, not the previous song's (demo's master has an effect, global-groove's has none)
  const masterOf = (f: string) => JSON.parse(readFileSync(join(music, f), "utf8")).master;
  const m = await page.evaluate(() => (window as any).DAW.debug.song().master);
  expect(m.devices.length).toBe(masterOf("global-groove.song.json").devices.length);
  expect(m.volume).toBeCloseTo(masterOf("global-groove.song.json").volume, 6);
  // the discarded edit is not on disk
  expect(readFileSync(join(music, "demo.song.json"), "utf8")).toBe(readFileSync(join(ROOT, "project/music/demo.song.json"), "utf8"));
  // back to demo, clean: no prompt
  await page.locator('#songlist button[data-file$="/demo.song.json"]').click();
  await page.waitForFunction(() => document.querySelector("#songlist button[aria-current=true]")?.getAttribute("data-file")?.endsWith("/demo.song.json"));
  expect(await dialog().count()).toBe(0);
});

test("save: clean afterwards; a 422 from the server lists the JSON paths and a path button selects its track", async () => {
  await edit();
  await page.locator("#save").click();
  await page.waitForFunction(() => !document.querySelector("#save")!.classList.contains("dirty"));
  expect(await current()).toMatch(/^demo0:15/);
  // the server rejects (rules the page does not check): the dialog lists each error with its path
  await page.route("**/api/music?path=*", (r) => r.request().method() === "PUT"
    ? r.fulfill({ status: 422, contentType: "application/json", body: JSON.stringify({ ok: false, errors: [{ path: "/tracks/1/devices/0", message: "bad device" }, { path: "/master/volume", message: "too loud" }] }) })
    : r.continue());
  await edit();
  await page.keyboard.press("Control+s");
  await expect(dialog().getAttribute("aria-label")).resolves.toBe("Save blocked: 2 validation errors");
  expect(await dialog().locator(".errs button").allTextContents()).toEqual(["/tracks/1/devices/0 bad device", "/master/volume too loud"]);
  expect(await saveDirty()).toBe(true);
  const second = await page.evaluate(() => (window as any).DAW.debug.song().tracks[1].id);
  await dialog().locator(".errs button").first().click();
  expect(await dialog().count()).toBe(0);
  expect(await page.evaluate(() => (window as any).DAW.debug.selected())).toBe(second);
  expect(await page.evaluate(() => (document.activeElement as HTMLElement).dataset.track)).toBe(second);
  await page.unroute("**/api/music?path=*");
  await page.locator("#save").click();
  await page.waitForFunction(() => !document.querySelector("#save")!.classList.contains("dirty"));
});

test("export: summary dialog, Write blob writes <song>.song.bin; Close returns focus to Export", async () => {
  await page.locator("#export-btn").click();
  await expect(dialog().getAttribute("aria-label")).resolves.toBe("Export demo.song.bin");
  const kv = await dialog().locator("table.kv").textContent();
  expect(kv).toContain("project/music/demo.song.bin");
  expect(kv).toMatch(/Validation\s*passed/);
  await dialog().getByRole("button", { name: "Write blob" }).click();
  await page.waitForFunction(() => /exported project\/music\/demo\.song\.bin/.test(document.body.textContent!));
  expect(existsSync(join(music, "demo.song.bin"))).toBe(true);
  expect(await dialog().count()).toBe(0);
  expect(await page.evaluate(() => document.activeElement?.id)).toBe("export-btn");
});

test("import: a blob opens as a new unsaved song; a bad file says why", async () => {
  await page.locator("#imp-file").setInputFiles(join(music, "demo.song.bin"));
  // demo.song.bin → project/music/demo.song.json, unsaved (the file on disk is untouched until Save)
  await page.waitForFunction(() => document.querySelector("#songlist button[aria-current=true]")?.textContent?.startsWith("• demo"));
  expect(await undoDepth()).toBe(0);
  expect(await page.locator(".thdr.track").count()).toBeGreaterThan(1);
  await page.locator("#imp-file").setInputFiles({ name: "junk.bin", mimeType: "application/octet-stream", buffer: Buffer.from("not a song blob") });
  await expect(dialog().getAttribute("aria-label")).resolves.toBe("Cannot import junk.bin");
  await dialog().getByRole("button", { name: "Close" }).click();
}, 60_000);

test("new song: asks to save the unsaved import, then one instrument track as untitled-1", async () => {
  await page.locator("#new-btn").click();
  await expect(dialog().getAttribute("aria-label")).resolves.toMatch(/^Save changes to /);
  await dialog().getByRole("button", { name: "Don't save" }).click();
  await page.waitForFunction(() => document.querySelector("#songlist button[aria-current=true]")?.textContent?.includes("untitled-1"));
  expect(await page.locator(".thdr.track").count()).toBe(1);
  expect(await page.locator(".thdr.track .name").first().textContent()).toBe("Track 1");
  expect(await undoDepth()).toBe(0);
  // back to the demo for the remaining tests (untitled-1 was never saved: Don't save)
  await page.locator('#songlist button[data-file$="/demo.song.json"]').click();
  if (await dialog().count()) await dialog().getByRole("button", { name: "Don't save" }).click();
  await page.waitForFunction(() => document.querySelector("#songlist button[aria-current=true]")?.getAttribute("data-file")?.endsWith("/demo.song.json"));
});

test("bounce: Render the loop gives a named WAV link; Cancel stops a running render", async () => {
  await page.locator("#bounce-btn").click();
  await expect(dialog().getAttribute("aria-label")).resolves.toBe("Bounce to WAV");
  expect(await dialog().locator('input[name="rng"]:checked').getAttribute("value")).toBe("song");
  await dialog().locator('input[value="loop"]').check();
  await dialog().getByRole("button", { name: "Render" }).click();
  const link = dialog().locator(".bres a[download]");
  await link.waitFor({ timeout: 60_000 });
  expect(await link.getAttribute("download")).toBe("demo-bars1-5.wav");
  expect(await dialog().locator("progress").evaluate((p: HTMLProgressElement) => p.value)).toBe(1);
  // a second render, cancelled at once: the dialog stays open and says so
  await dialog().locator('input[value="song"]').check();
  await dialog().getByRole("button", { name: "Render" }).click();
  await dialog().getByRole("button", { name: "Cancel" }).click();
  expect(await dialog().locator(".bres").textContent()).toBe("cancelled");
  expect(await dialog().locator("progress").isHidden()).toBe(true);
  // Cancel with nothing running closes
  await dialog().getByRole("button", { name: "Cancel" }).click();
  expect(await dialog().count()).toBe(0);
  expect(await page.evaluate(() => document.activeElement?.id)).toBe("bounce-btn");
}, 120_000);

test("track menu by keyboard: Shift+F10 opens it on the first item, arrows skip disabled items, Escape returns focus", async () => {
  const first = page.locator(".thdr.track").first();
  await first.focus();
  await page.keyboard.press("Shift+F10");
  const menu = page.getByRole("menu");
  expect(await menu.getAttribute("aria-label")).toMatch(/ track menu$/);
  expect(await menu.getByRole("menuitem").allTextContents()).toEqual(["RenameF2 / double-click", "Duplicate track", "Move upAlt+↑", "Move downAlt+↓", "Add instrument track", "Add bus track", "Delete trackCtrl+Delete"]);
  expect(await menu.getByRole("menuitem", { name: /Move up/ }).isDisabled()).toBe(true);  // first track
  expect(await page.evaluate(() => document.activeElement?.textContent)).toMatch(/^Rename/);
  await page.keyboard.press("ArrowDown"); await page.keyboard.press("ArrowDown");  // Duplicate, then Move down (Move up disabled)
  expect(await page.evaluate(() => document.activeElement?.textContent)).toMatch(/^Move down/);
  await page.keyboard.press("ArrowUp"); await page.keyboard.press("ArrowUp"); await page.keyboard.press("ArrowUp");  // wraps to Delete
  expect(await page.evaluate(() => document.activeElement?.textContent)).toMatch(/^Delete track/);
  await page.keyboard.press("Escape");
  expect(await menu.count()).toBe(0);
  expect(await page.evaluate(() => (document.activeElement as HTMLElement).classList.contains("thdr"))).toBe(true);
  // choosing an item runs it and closes the menu
  const n = await page.locator(".thdr.track").count();
  await page.keyboard.press("Shift+F10");
  await menu.getByRole("menuitem", { name: /Duplicate track/ }).click();
  expect(await menu.count()).toBe(0);
  expect(await page.locator(".thdr.track").count()).toBe(n + 1);
  await page.keyboard.press("Control+z");
  expect(await page.locator(".thdr.track").count()).toBe(n);
  // a click outside closes it
  await first.click({ button: "right" });
  expect(await menu.count()).toBe(1);
  await page.mouse.click(700, 980);
  expect(await menu.count()).toBe(0);
}, 30_000);

test("library: sources with counts, search filters and opens groups, inspect, audition, load a preset (one undo)", async () => {
  const summaries = () => page.locator("#lib-tree details > summary").allTextContents();
  expect((await summaries())[0]).toMatch(/^Project presets\s*1$/);
  await page.locator("#lib-q").fill("acid");
  expect((await summaries())[0]).toMatch(/^Project presets\s*1\/1$/);
  expect(await page.locator('#lib-tree details[data-src="project"]').getAttribute("open")).not.toBeNull();
  const entry = page.locator('#lib-tree details[data-src="project"] button:has-text("Acid Bass")');
  await page.locator('.thdr.track[data-track="lead"]').click();
  await entry.click();
  expect(await entry.getAttribute("aria-current")).toBe("true");
  expect(await page.locator("#lib-insp .iname").textContent()).toBe("Acid Bass");
  expect(await page.locator("#lib-insp").textContent()).toContain("Project presets");
  const aud = page.locator("#lib-insp .aud");
  expect(await aud.textContent()).toBe("▶ Audition C3");
  await aud.click();
  expect(await aud.textContent()).toBe("■ playing C3");
  await page.waitForFunction(() => document.querySelector("#lib-insp .aud")?.textContent === "▶ Audition C3", null, { timeout: 5000 });
  const d0 = await undoDepth();
  await page.locator("#lib-insp .loadin").click();
  await page.waitForFunction((d) => (window as any).DAW.debug.undoDepth() === d + 1, d0);
  const lead = () => page.evaluate(() => (window as any).DAW.debug.song().tracks.find((t: any) => t.id === "lead").devices[0].preset);
  expect(await lead()).toBe("Acid Bass");
  await page.keyboard.press("Control+z");
  expect(await lead()).not.toBe("Acid Bass");
  // clearing the search closes the groups again
  await page.locator("#lib-q").fill("");
  expect(await page.locator("#lib-tree details[open]").count()).toBe(0);
}, 30_000);

test("no critical or serious accessibility violations with a dialog or a menu open (axe)", async () => {
  await page.addScriptTag({ path: join(ROOT, "node_modules/axe-core/axe.min.js") });
  const check = async (what: string, sel: string) => {
    const res: any = await page.evaluate(async (sel) => (window as any).axe.run(document.querySelector(sel), { resultTypes: ["violations"] }), sel);
    const bad = res.violations.filter((v: any) => v.impact === "critical" || v.impact === "serious");
    if (bad.length) console.error(what, bad.map((v: any) => `${v.id} (${v.impact}): ${v.nodes.length} × ${v.nodes[0]?.target}`).join("\n"));
    expect(bad.map((v: any) => v.id)).toEqual([]);
  };
  await page.locator("#bounce-btn").click();
  await check("bounce dialog", "dialog.dlg");
  await page.keyboard.press("Escape");
  await page.locator(".thdr.track").first().focus();
  await page.keyboard.press("Shift+F10");
  await check("track menu", "[role=menu]");
  await page.keyboard.press("Escape");
}, 60_000);

test("no page errors", () => {
  expect(errors.filter((e) => !/Failed to load resource.*422/.test(e))).toEqual([]);
});

test("phone width: one column, the browser is a drawer (Songs button, Escape, scrim, picking a song closes it); desktop unchanged", async () => {
  await page.setViewportSize({ width: 390, height: 844 });
  try {
    const btn = page.locator("#browse-btn"), drawer = page.locator("#browser");
    expect(await btn.isVisible()).toBe(true);
    expect(await btn.getAttribute("aria-expanded")).toBe("false");
    await drawer.waitFor({ state: "hidden" });  // after the drawer transition that the resize starts
    // nothing wider than the screen; the work area gets the whole width; key hints are hidden
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
    expect((await page.locator("#work").boundingBox())!.width).toBeGreaterThan(380);
    expect(await page.locator("footer .hints").isVisible()).toBe(false);
    await btn.click();
    expect(await btn.getAttribute("aria-expanded")).toBe("true");
    await drawer.waitFor({ state: "visible" });
    expect(await page.evaluate(() => document.activeElement?.id)).toBe("song-q");
    await page.keyboard.press("Escape");
    await drawer.waitFor({ state: "hidden" });
    await btn.click();
    await page.locator("#scrim").click({ position: { x: 380, y: 400 } });
    await drawer.waitFor({ state: "hidden" });
    await btn.click();
    await page.locator("#songlist button[aria-current=true]").click();
    await drawer.waitFor({ state: "hidden" });
    expect(await btn.getAttribute("aria-expanded")).toBe("false");
  } finally {
    await page.setViewportSize({ width: 1440, height: 1000 });
  }
  expect(await page.locator("#browse-btn").isVisible()).toBe(false);
  expect(await page.locator("#browser").isVisible()).toBe(true);
  expect(await page.locator("footer .hints").isVisible()).toBe(true);
});
