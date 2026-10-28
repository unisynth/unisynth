// Diagnostic: plays a song in the real page (worklet) and prints the engine load every second.
// SONG=project/music/x.song.json TRACK=<id> LIVE=0|1 SECS=60
import { chromium } from "playwright";
import { startServer } from "../../cli/src/server.ts";
import { DIST } from "../build.ts";
const s = await startServer({ port: 0, build: false, dawDist: DIST, headless: false });
const b = await chromium.launch({ channel: "chrome", args: ["--autoplay-policy=no-user-gesture-required"] });
const p = await b.newPage();
p.on("pageerror", (e) => console.log("pageerror", e.message));
await p.goto(`${s.url}daw/?song=${process.env.SONG ?? "project/music/demo.song.json"}`);
await p.waitForSelector(".thdr.track");
await p.evaluate((on) => localStorage.setItem("daw.live", on), process.env.LIVE ?? "1");
await p.reload(); await p.waitForSelector(".thdr.track"); await p.waitForTimeout(500);
if (process.env.TRACK) await p.locator(`.thdr.track[data-track="${process.env.TRACK}"] .name`).click();
await p.locator("#tab-dev").click();
await p.keyboard.press("Escape");
await p.locator("#lanes-wrap").focus();
if (process.env.LOOP) await p.keyboard.press("l");
await p.keyboard.press("Space");
const secs = Number(process.env.SECS ?? 40);
for (let i = 0; i < secs; i++) {
  await p.waitForTimeout(1000);
  const st = await p.evaluate(() => { const s = (window as any).DAW.engine.status; return { pos: s.position, pl: s.playing, load: s.load, worst: s.worst, pr: s.probe ? s.probe[1] : -1, txt: document.querySelector("#load, .load, [id*=load]")?.textContent }; });
  console.log(i, JSON.stringify(st));
}
await b.close(); s.stop?.(); process.exit(0);
