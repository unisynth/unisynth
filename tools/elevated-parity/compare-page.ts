// A/B listening page for the Elevated import (change add-elevated-import): the original code (build-ref.sh), the
// unisynth render of the converted song and, for full mixes, the YouTube capture, as MP3s on one shared transport.
// The audio is not ours to publish: everything goes to $ELEVATED_COMPARE (default ~/Downloads/64k-tools/elevated-compare),
// never into the repository.
//
//   bun tools/elevated-parity/compare-page.ts     (after build-ref.sh SOLO=1 and the renders in <out>/wav/<tag>_unisynth.wav)
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { capturesDir, checkCapture } from "./capture.ts";
import { refWav } from "./parity.ts";

const OUT = process.env.ELEVATED_COMPARE ?? join(homedir(), "Downloads/64k-tools/elevated-compare");
const SONGS: [string, string][] = [["atrium", "Atrium (TBC & Loonies, 2008)"], ["elevated", "Elevated (RGBA & TBC, 2009)"]];

interface Src { label: string; file: string; offset: number; gainDb: number }
interface Item { song: string; name: string; srcs: Src[]; metrics: string }

/** report.md rows by render name: result, loudness Δ, worst band, alignment. */
function metrics(): Map<string, string> {
  const m = new Map<string, string>();
  const p = join(import.meta.dir, "report.md");
  if (!existsSync(p)) return m;
  for (const l of readFileSync(p, "utf8").split("\n")) {
    const c = l.split("|").map((x) => x.trim());
    if (c.length > 6 && /^(PASS|FAIL)$/.test(c[2])) m.set(c[1], `${c[2]} · loudness Δ ${c[3]} dB · worst band ${c[4]} Hz · alignment ${c[5]}${c[6] ? ` · capture ${c[6]}` : ""}`);
  }
  return m;
}

function mp3(wav: string, name: string): string {
  const out = join(OUT, "mp3", `${name}.mp3`);
  WAVS.set(`mp3/${name}.mp3`, wav);
  if (!existsSync(out)) {
    const p = Bun.spawnSync(["ffmpeg", "-v", "error", "-y", "-i", wav, "-codec:a", "libmp3lame", "-b:a", "192k", out]);
    if (p.exitCode !== 0) throw new Error(`ffmpeg ${wav}: ${p.stderr}`);
  }
  return `mp3/${name}.mp3`;
}

/** A source as base64 Opus (encoded from its WAV, cached in <out>/opus) for the single-file pages. */
function opus(file: string, kbps: number): string {
  const tag = file.replace(/^mp3\/|\.mp3$/g, ""), out = join(OUT, "opus", `${tag}.webm`);
  mkdirSync(join(OUT, "opus"), { recursive: true });
  if (!existsSync(out)) {
    const p = Bun.spawnSync(["ffmpeg", "-v", "error", "-y", "-i", WAVS.get(file)!, "-c:a", "libopus", "-b:a", `${kbps}k`, out]);
    if (p.exitCode !== 0) throw new Error(`ffmpeg opus ${file}: ${p.stderr}`);
  }
  return readFileSync(out).toString("base64");
}
const WAVS = new Map<string, string>(); // mp3 path -> source wav

function main() {
  mkdirSync(join(OUT, "mp3"), { recursive: true });
  const met = metrics(), items: Item[] = [];
  for (const [song] of SONGS) {
    for (let k = -1; k === -1 || existsSync(refWav(song, k)); k++) {
      const tag = k < 0 ? song : `${song}-solo${k}`, ours = join(OUT, "wav", `${tag}_unisynth.wav`);
      if (!existsSync(ours) || !existsSync(refWav(song, k < 0 ? undefined : k))) continue;
      const srcs: Src[] = [
        { label: "Original", file: mp3(refWav(song, k < 0 ? undefined : k), `${tag}_original`), offset: 0, gainDb: 0 },
        { label: "unisynth", file: mp3(ours, `${tag}_unisynth`), offset: 0, gainDb: 0 },
      ];
      const cap = join(capturesDir(), `${song}.wav`);
      if (k < 0 && existsSync(cap)) {
        const c = checkCapture(ours, cap); // offset: capture frame at our frame 0; gain: ours − capture
        srcs.push({ label: "Capture", file: mp3(cap, `${tag}_capture`), offset: c.report.offset / 44100, gainDb: c.report.gainDb });
      }
      items.push({ song, name: k < 0 ? "Full mix" : `Synth ${k} solo`, srcs, metrics: met.get(k < 0 ? song : `${song}/solo${k}`) ?? "" });
      console.log(`${tag}: ${srcs.length} sources`);
    }
  }
  writeFileSync(join(OUT, "index.html"), page(items));
  console.log(`wrote ${join(OUT, "index.html")}`);
  // single-file pages (no server, open anywhere): the audio inlined as Opus; mixes 96 kb/s, solos 48 kb/s
  const mixes = items.filter((it) => it.name === "Full mix");
  for (const [name, list] of [["elevated-ab.html", items], ["elevated-ab-mixes.html", mixes]] as const) {
    const files: Record<string, string> = {};
    for (const it of list) for (const src of it.srcs) files[src.file] ??= opus(src.file, it.name === "Full mix" ? 96 : 48);
    writeFileSync(join(OUT, name), page(list, files));
    console.log(`wrote ${join(OUT, name)} (${(Bun.file(join(OUT, name)).size / 1e6).toFixed(0)} MB)`);
  }
}

const page = (items: Item[], files?: Record<string, string>) => `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Elevated import: A/B</title>
<style>
:root{color-scheme:dark;--bg:#14161a;--card:#1d2026;--line:#2c3038;--fg:#e6e6e6;--dim:#8b919c;--on:#e8823a;--pass:#5fbf77;--fail:#e05d5d}
body{margin:0;font:14px/1.4 system-ui,sans-serif;background:var(--bg);color:var(--fg)}
header{position:sticky;top:0;z-index:2;background:#101216ee;border-bottom:1px solid var(--line);padding:12px 20px;display:flex;gap:14px;align-items:center;flex-wrap:wrap}
h1{font-size:16px;margin:0 12px 0 0}
button{background:var(--card);color:var(--fg);border:1px solid var(--line);border-radius:6px;padding:6px 12px;font:inherit;cursor:pointer}
button:hover{border-color:var(--dim)} button.on{background:var(--on);border-color:var(--on);color:#111;font-weight:600}
#seek{flex:1;min-width:200px;accent-color:var(--on)} #time{font-variant-numeric:tabular-nums;color:var(--dim);min-width:110px}
main{padding:16px 20px;max-width:1100px} h2{font-size:15px;margin:22px 0 8px;color:var(--dim);font-weight:600}
.row{display:grid;grid-template-columns:150px auto 1fr;gap:12px;align-items:center;background:var(--card);border:1px solid var(--line);border-radius:8px;padding:8px 12px;margin:6px 0}
.row.cur{border-color:var(--on)} .srcs{display:flex;gap:6px} .m{color:var(--dim);font-size:12px}
.m b.PASS{color:var(--pass)} .m b.FAIL{color:var(--fail)} .help{color:var(--dim);font-size:12px}
label{color:var(--dim);font-size:12px;display:flex;gap:6px;align-items:center}
</style></head><body>
<header>
  <h1>Elevated import · A/B</h1>
  <button id="play">▶ Play</button>
  <input id="seek" type="range" min="0" max="1000" value="0">
  <span id="time">0:00 / 0:00</span>
  <label><input id="match" type="checkbox" checked> level-match capture</label>
  <span class="help">Space play · 1/2/3 source · ←/→ 5 s · ↑/↓ row</span>
</header>
<main id="list"></main>
<script>
const ITEMS = ${JSON.stringify(items)};
const FILES = ${files ? JSON.stringify(files) : "null"}, URLS = {};
// inlined audio: decoded to a Blob once, on first use
function url(f) {
  if (!FILES) return f;
  if (!URLS[f]) { const b = atob(FILES[f]), a = new Uint8Array(b.length); for (let i = 0; i < b.length; i++) a[i] = b.charCodeAt(i); URLS[f] = URL.createObjectURL(new Blob([a], { type: "audio/webm" })); }
  return URLS[f];
}
const list = document.getElementById("list"), play = document.getElementById("play"), seek = document.getElementById("seek"), time = document.getElementById("time"), match = document.getElementById("match");
let row = 0, src = 0, t = 0, playing = false, audio = null, dragging = false;
const fmt = (s) => { s = Math.max(0, s); return Math.floor(s / 60) + ":" + String(Math.floor(s % 60)).padStart(2, "0"); };
const els = [];
let last = "";
ITEMS.forEach((it, i) => {
  if (it.song !== last) { last = it.song; const h = document.createElement("h2"); h.textContent = ${JSON.stringify(Object.fromEntries(SONGS))}[it.song]; list.append(h); }
  const d = document.createElement("div"); d.className = "row";
  const m = it.metrics.replace(/^(PASS|FAIL)/, '<b class="$1">$1</b>');
  d.innerHTML = '<b>' + it.name + '</b><span class="srcs">' + it.srcs.map((s, j) => '<button data-j="' + j + '">' + (j + 1) + " · " + s.label + "</button>").join("") + '</span><span class="m">' + m + "</span>";
  d.querySelectorAll("button").forEach((b) => b.onclick = () => select(i, +b.dataset.j));
  list.append(d); els.push(d);
});
function cur() { return ITEMS[row].srcs[Math.min(src, ITEMS[row].srcs.length - 1)]; }
function select(i, j) {
  const pos = audio ? audio.currentTime - curOffset : t;  // song time of the source playing now
  t = Math.max(0, pos);
  if (i !== row) { row = i; }
  src = j;
  load();
}
let curOffset = 0;
function load() {
  const s = cur(), was = playing;
  if (audio) { audio.ontimeupdate = audio.onended = null; audio.pause(); audio.removeAttribute("src"); audio.load(); }
  audio = new Audio(url(s.file)); audio.preload = "auto";
  curOffset = s.offset;
  // gainDb (capture only) = ours − capture: attenuate whichever side is louder
  const g = match.checked ? Math.max(0, ...ITEMS[row].srcs.map((x) => x.gainDb)) || Math.min(0, ...ITEMS[row].srcs.map((x) => x.gainDb)) : 0;
  audio.volume = Math.pow(10, -Math.abs(s.gainDb ? Math.min(0, g) : Math.max(0, g)) / 20);
  audio.addEventListener("loadedmetadata", () => { audio.currentTime = Math.max(0, t + curOffset); if (was) audio.play(); }, { once: true });
  audio.ontimeupdate = () => { if (!dragging) { t = audio.currentTime - curOffset; draw(); } };
  audio.onended = () => { playing = false; draw(); };
  els.forEach((e, i) => { e.classList.toggle("cur", i === row); e.querySelectorAll("button").forEach((b) => b.classList.toggle("on", i === row && +b.dataset.j === src)); });
  draw();
}
function dur() { return audio && isFinite(audio.duration) ? audio.duration - curOffset : 0; }
function draw() { time.textContent = fmt(t) + " / " + fmt(dur()); if (!dragging && dur()) seek.value = Math.round((t / dur()) * 1000); play.textContent = playing ? "❚❚ Pause" : "▶ Play"; }
function toggle() { playing = !playing; if (playing) audio.play(); else audio.pause(); draw(); }
function jump(s) { t = Math.max(0, s); if (audio) audio.currentTime = t + curOffset; draw(); }
play.onclick = toggle;
seek.oninput = () => { dragging = true; t = (seek.value / 1000) * dur(); time.textContent = fmt(t) + " / " + fmt(dur()); };
seek.onchange = () => { dragging = false; jump(t); };
match.onchange = () => load();
addEventListener("keydown", (e) => {
  if (e.target.tagName === "INPUT" && e.target.type !== "checkbox") return;
  if (e.key === " ") { e.preventDefault(); toggle(); }
  else if (/^[1-3]$/.test(e.key) && +e.key <= ITEMS[row].srcs.length) select(row, +e.key - 1);
  else if (e.key === "ArrowLeft") jump(t - 5);
  else if (e.key === "ArrowRight") jump(t + 5);
  else if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); const i = Math.max(0, Math.min(ITEMS.length - 1, row + (e.key === "ArrowDown" ? 1 : -1))); select(i, Math.min(src, ITEMS[i].srcs.length - 1)); els[i].scrollIntoView({ block: "nearest" }); }
});
load();
</script></body></html>
`;

if (import.meta.main) main();
