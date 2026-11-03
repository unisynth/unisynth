// Listening page for every song in project/music (the demo songs and project/music/released/**). Each song is exported
// to its compact binary form (`song export`, a few KB) and played in the browser by the synth itself: a worker renders the
// song ahead with the engine module (synth.wasm, `bounce_open`/`bounce_step`, equal to `song render`) and an AudioWorklet
// plays the rendered part, so playback starts at once and a seek inside it is instant. One static page with a shared
// player (play, pause, seek), search, credits and a link per song (`#<slug>`, `#<slug>@<seconds>`).
//
//   bun tools/song-page/build.ts [outDir]     # default tools/song-page/out (git-ignored)
//   BANK_URL=<url>   where the page loads the GM bank for songs with GM.DLS samplers (default ../daw/data/bank.dls,
//                    the bank published with the DAW on unisynth.exceed.hu; serve.ts serves GM_DLS there locally)
//   JOBS=8           parallel exports
//   CONTACT=<text>   who to ask for a song's removal, shown on the page
//   ONLY=<regex>     only songs whose path matches (a quick try; the page lists only those)
//
// Exports are cached by the song file's sha256 (out/cache.json); song files of songs that are gone are dropped.
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { basename, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { buildSynthWasm } from "../../packages/cli/src/synth.ts";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));
const OUT = process.argv[2] ?? join(ROOT, "tools/song-page/out");
const MUSIC = join(ROOT, "project/music");
const SONG = join(ROOT, "target/release/song");
const BANK_URL = process.env.BANK_URL ?? "../daw/data/bank.dls";
const JOBS = Number(process.env.JOBS ?? 8);
const RATE = 44100;

export type Entry = {
  slug: string; file: string; group: string; title: string; author: string; description: string;
  url?: string; copyright?: string; seconds: number; blob: string; bytes: number; bank: boolean;
};

/** True when a sampler of the song reads the GM.DLS bank: an oscillator of type sampler (1/3) whose source (`s7`) is a
 * DLS instrument (0.5) or DLS wave (1.0) and whose sample index is set. Only these songs fetch the bank. */
export function needsBank(song: any): boolean {
  for (const t of song.tracks ?? [])
    for (const d of t.devices ?? []) {
      const v = d.values ?? {};
      for (const o of [1, 2, 3])
        if (Math.abs((v[`osc${o}.type`] ?? -1) - 1 / 3) < 1e-4 && (v[`osc${o}.s7`] ?? 0) >= 0.5 && (v[`osc${o}.sample`] ?? 0) > 0) return true;
    }
  return false;
}

/** Every song file under project/music (the top folder and released/**), sorted by path. */
export function songFiles(dir = MUSIC): string[] {
  const out: string[] = [];
  const walk = (d: string) => {
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const f = join(d, e.name);
      if (e.isDirectory()) walk(f);
      else if (e.name.endsWith(".song.json")) out.push(f);
    }
  };
  walk(dir);
  return out.sort();
}

/** The page group of a song, from where it lives and where its meta says it came from. */
export function groupOf(rel: string, url = ""): string {
  if (!rel.startsWith("released/")) return "Demo songs";
  if (rel.startsWith("released/v2/")) {
    if (url.includes("v2m-player")) return "V2M collection (via modland)";
    return "Farbrausch productions";
  }
  if (basename(rel).startsWith("tbl-")) return "The Black Lotus, 1997";
  return "64K intros and executable music (WaveSabre)";
}

export const GROUPS = ["Demo songs", "64K intros and executable music (WaveSabre)", "The Black Lotus, 1997", "Farbrausch productions", "V2M collection (via modland)"];

/** A URL-safe id from the path relative to project/music (`released/v2/foo.song.json` → `v2-foo`). */
export function slugOf(rel: string): string {
  return rel.replace(/^released\//, "").replace(/\.song\.json$/, "").replace(/[^A-Za-z0-9]+/g, "-").replace(/^-|-$/g, "").toLowerCase();
}

async function run(cmd: string[]): Promise<string> {
  const p = Bun.spawn(cmd, { stdout: "pipe", stderr: "pipe" });
  const [out, err] = [await new Response(p.stdout).text(), await new Response(p.stderr).text()];
  if ((await p.exited) !== 0) throw new Error(`${cmd.slice(0, 2).join(" ")} ${basename(cmd[2] ?? "")}: ${err.trim().split("\n").slice(-3).join(" | ")}`);
  return out;
}

async function main() {
  if (!existsSync(SONG)) throw new Error(`build the song tool first: cargo build --release -p song (${SONG})`);
  mkdirSync(join(OUT, "songs"), { recursive: true });
  rmSync(join(OUT, "audio"), { recursive: true, force: true }); // the AAC renders of the earlier page
  const cacheFile = join(OUT, "cache.json");
  const cache: Record<string, { hash: string; seconds: number }> = existsSync(cacheFile) ? JSON.parse(readFileSync(cacheFile, "utf8")) : {};

  const only = process.env.ONLY ? new RegExp(process.env.ONLY) : undefined;
  const files = songFiles().filter((f) => !only || only.test(relative(MUSIC, f)));
  const entries: Entry[] = [];
  const todo: { e: Entry; hash: string }[] = [];
  const seen = new Set<string>();
  for (const f of files) {
    const rel = relative(MUSIC, f);
    const bytes = readFileSync(f);
    const s = JSON.parse(bytes.toString("utf8"));
    const m = s.meta ?? {};
    const slug = slugOf(rel);
    if (seen.has(slug)) throw new Error(`duplicate slug ${slug} (${rel})`);
    seen.add(slug);
    const hash = createHash("sha256").update(bytes).digest("hex");
    const e: Entry = {
      slug, file: rel, group: groupOf(rel, m.url), title: m.title ?? basename(rel, ".song.json"), author: m.author ?? "",
      description: m.description ?? "", url: m.url, copyright: m.copyright, seconds: 0, blob: `songs/${slug}.bin`, bytes: 0, bank: needsBank(s),
    };
    entries.push(e);
    const c = cache[slug];
    if (c && c.hash === hash && existsSync(join(OUT, e.blob))) e.seconds = c.seconds;
    else todo.push({ e, hash });
  }
  if (!only) {
    for (const a of readdirSync(join(OUT, "songs"))) if (!seen.has(a.replace(/\.bin$/, ""))) rmSync(join(OUT, "songs", a));
    for (const k of Object.keys(cache)) if (!seen.has(k)) delete cache[k];
  }

  console.log(`${entries.length} songs, ${todo.length} to export (${JOBS} jobs)`);
  const failed: string[] = [];
  const worker = async () => {
    for (let t = todo.shift(); t; t = todo.shift()) {
      const { e, hash } = t;
      try {
        const bin = join(OUT, e.blob);
        await run([SONG, "export", join(MUSIC, e.file), "-o", bin]);
        // `song check` loads the blob into the player: "<f>: loads (blob, <frames> frames)"
        const frames = /(\d+) frames/.exec(await run([SONG, "check", bin]))?.[1];
        if (!frames) throw new Error(`${e.file}: no length from song check`);
        e.seconds = +frames / RATE;
        cache[e.slug] = { hash, seconds: e.seconds };
      } catch (err) {
        failed.push(`${e.file}: ${(err as Error).message}`);
      }
    }
  };
  await Promise.all(Array.from({ length: JOBS }, worker));
  writeFileSync(cacheFile, JSON.stringify(cache, null, 1));
  if (failed.length) {
    console.error(`${failed.length} failed:\n${failed.join("\n")}`);
    process.exit(1);
  }
  copyFileSync(buildSynthWasm(true), join(OUT, "synth.wasm"));
  // Content-hash query strings: Pages caches files for 10 min, and an old blob with a new engine (or the reverse) fails
  const v = (f: string) => createHash("sha256").update(readFileSync(join(OUT, f))).digest("hex").slice(0, 10);
  for (const e of entries) e.bytes = statSync(join(OUT, e.blob)).size;
  const wasmUrl = `synth.wasm?v=${v("synth.wasm")}`;
  const versioned = entries.map((e) => ({ ...e, blob: `${e.blob}?v=${v(e.blob)}` }));
  writeFileSync(join(OUT, "songs.json"), JSON.stringify(entries));
  writeFileSync(join(OUT, "index.html"), page(versioned, BANK_URL, wasmUrl));
  const hours = entries.reduce((a, e) => a + e.seconds, 0) / 3600;
  const mb = entries.reduce((a, e) => a + e.bytes, 0) / 1e6;
  const nb = entries.filter((e) => e.bank).length;
  console.log(`wrote ${join(OUT, "index.html")}: ${entries.length} songs, ${hours.toFixed(1)} h, ${mb.toFixed(1)} MB song files, ${nb} use the GM bank`);
}

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

// The render worker: instantiates the engine module, opens a bounce of the song blob and renders it to the end as fast
// as it can, sending 1-second 16-bit chunks straight to the player worklet (its port) and the progress to the page.
const WORKER = `onmessage = async (e) => {
  const { module, blob, bank, port } = e.data;
  const x = (await WebAssembly.instantiate(module, { env: { host_now_ms: () => performance.now() } })).exports;
  const put = (b) => { const q = x.alloc(b.length); new Uint8Array(x.memory.buffer, q, b.length).set(b); return [q, b.length]; };
  if (bank) x.set_bank(...put(bank));
  const total = x.bounce_open(...put(blob));
  if (!total) { postMessage({ error: new TextDecoder().decode(new Uint8Array(x.memory.buffer, x.out_ptr(), x.out_len()).slice()) }); return; }
  postMessage({ total });
  const C = ${RATE};
  let buf = new Int16Array(2 * C), k = 0, done = 0;
  while (done < total) {
    const n = Math.min(4096, total - done);
    const a = new Float32Array(x.memory.buffer, x.bounce_step(n), 2 * n);
    for (let i = 0; i < 2 * n; i++) {
      const v = Math.trunc(Math.fround(a[i] * 32767)); // the player's conversion (f32 product, truncated), as \`song render\` writes it
      buf[k++] = v > 32767 ? 32767 : v < -32768 ? -32768 : v;
      if (k === buf.length) { port.postMessage({ chunk: buf }, [buf.buffer]); buf = new Int16Array(2 * C); k = 0; postMessage({ rendered: done + (i + 1) / 2 }); }
    }
    done += n;
  }
  if (k) port.postMessage({ chunk: buf.slice(0, k) });
  port.postMessage({ done: true });
  postMessage({ rendered: total, done: true });
};`;

// The player worklet: keeps the rendered chunks (chunk i = frames [i·C, (i+1)·C)), plays from \`pos\` while \`on\`, outputs
// silence while the render has not reached \`pos\` yet, and reports the position about 10 times a second.
const WORKLET = `class SongPlayer extends AudioWorkletProcessor {
  constructor() {
    super();
    this.ch = []; this.n = 0; this.pos = 0; this.on = false; this.done = false; this.feed = null; this.t = 0;
    this.port.onmessage = (e) => {
      const m = e.data;
      if (m.reset) { if (this.feed) this.feed.onmessage = null; this.feed = null; this.ch = []; this.n = 0; this.pos = m.pos; this.done = false; }
      if (m.feed) {
        this.feed = m.feed;
        m.feed.onmessage = (f) => { const d = f.data; if (d.chunk) { this.ch.push(d.chunk); this.n += d.chunk.length / 2; } if (d.done) this.done = true; };
      }
      if ("on" in m) this.on = m.on;
      if ("seek" in m) this.pos = m.seek;
    };
  }
  process(_, outs) {
    const L = outs[0][0], R = outs[0][1] ?? L, C = ${RATE};
    if (this.on) {
      for (let i = 0; i < L.length && this.pos < this.n; i++, this.pos++) {
        const c = this.ch[(this.pos / C) | 0], k = 2 * (this.pos % C);
        L[i] = c[k] / 32768; R[i] = c[k + 1] / 32768;
      }
      if (this.done && this.pos >= this.n) { this.on = false; this.port.postMessage({ ended: true }); }
    }
    if ((++this.t & 31) === 0) this.port.postMessage({ pos: this.pos, waiting: this.on && this.pos >= this.n });
    return true;
  }
}
registerProcessor("song-player", SongPlayer);`;

export function page(entries: Entry[], bankUrl = "../daw/data/bank.dls", wasmUrl = "synth.wasm"): string {
  const contact = process.env.CONTACT;
  const counts = GROUPS.map((g) => [g, entries.filter((e) => e.group === g).length] as const).filter(([, n]) => n);
  const songs = entries.map(({ slug, group, title, author, description, url, seconds, blob, bank }) => ({ slug, group, title, author, description, url, seconds, blob, bank }));
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>64k: converted songs</title>
<style>
:root { color-scheme: light dark; --bg: #fafaf8; --fg: #1d1d1b; --muted: #5c5c57; --line: #dcdcd6; --hi: #0b5cad; --row: #f0f0ec; }
@media (prefers-color-scheme: dark) { :root { --bg: #161615; --fg: #ececea; --muted: #a3a39d; --line: #34342f; --hi: #7db4f0; --row: #22221f; } }
* { box-sizing: border-box; }
body { margin: 0; font: 15px/1.5 system-ui, -apple-system, "Segoe UI", sans-serif; background: var(--bg); color: var(--fg); }
header, main { max-width: 60rem; margin: 0 auto; padding: 0 1rem; }
header { padding-top: 1.5rem; }
h1 { font-size: 1.4rem; margin: 0 0 .5rem; }
h2 { font-size: 1.05rem; margin: 2rem 0 .5rem; border-bottom: 1px solid var(--line); padding-bottom: .25rem; }
p.note { color: var(--muted); max-width: 46rem; margin: .25rem 0; }
a { color: var(--hi); }
:focus-visible { outline: 2px solid var(--hi); outline-offset: 2px; }
#player { position: sticky; top: 0; z-index: 1; background: var(--bg); border-bottom: 1px solid var(--line); padding: .6rem 1rem; }
#player .in { max-width: 60rem; margin: 0 auto; }
#now { font-weight: 600; min-height: 1.5em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
#now span { font-weight: 400; color: var(--muted); }
.bar { display: grid; grid-template-columns: 2.25rem 1fr auto; gap: .5rem; align-items: center; margin-top: .25rem; }
#toggle { width: 2rem; height: 2rem; border-radius: 50%; border: 1px solid var(--hi); background: none; color: var(--hi); cursor: pointer; font-size: .8rem; }
#seek { width: 100%; accent-color: var(--hi); height: 1.25rem; margin: 0; background: transparent; }
#seek::-webkit-slider-runnable-track { height: 4px; border-radius: 2px; background: var(--track, var(--line)); }
#seek::-moz-range-track { height: 4px; border-radius: 2px; background: var(--track, var(--line)); }
#seek::-webkit-slider-thumb { margin-top: -5px; }
#time { color: var(--muted); font-variant-numeric: tabular-nums; font-size: .9rem; white-space: nowrap; }
#search { width: 100%; max-width: 24rem; padding: .4rem .6rem; font: inherit; border: 1px solid var(--line); border-radius: 4px; background: var(--bg); color: var(--fg); margin: 1rem 0 0; }
ol { list-style: none; margin: 0; padding: 0; }
li { border-bottom: 1px solid var(--line); }
li, h2 { scroll-margin-top: 8rem; }
li.on { background: var(--row); }
.row { display: grid; grid-template-columns: 2.25rem 1fr auto; gap: .5rem; align-items: center; padding: .35rem .25rem; }
.row button.play { width: 2rem; height: 2rem; border-radius: 50%; border: 1px solid var(--line); background: none; color: var(--fg); cursor: pointer; font-size: .8rem; }
li.on button.play { border-color: var(--hi); color: var(--hi); }
.t { font-weight: 600; }
.a { color: var(--muted); }
.d { color: var(--muted); font-variant-numeric: tabular-nums; font-size: .9rem; }
details { padding: 0 .25rem .5rem 2.75rem; color: var(--muted); font-size: .9rem; }
details summary { cursor: pointer; width: fit-content; }
details p { margin: .25rem 0; }
.link { font-size: .85rem; margin-left: .5rem; }
footer { max-width: 60rem; margin: 2rem auto; padding: 0 1rem; color: var(--muted); font-size: .85rem; }
</style>
</head>
<body>
<div id="player" role="region" aria-label="Player"><div class="in">
<div id="now" aria-live="polite">Pick a song</div>
<div class="bar">
<button id="toggle" aria-label="Play">▶</button>
<input id="seek" type="range" min="0" max="0" step="0.1" value="0" aria-label="Position" disabled>
<span id="time">0:00 / 0:00</span>
</div>
</div></div>
<header>
<h1>Converted songs</h1>
<p class="note">${entries.length} songs converted to the 64k project's own synthesizer. Your browser plays each one live with
that synthesizer (WebAssembly), from a song file of a few kilobytes; there are no audio recordings here. Each is an adapted
version, not the original recording, and sounds close to but not identical with it. No licence is stated for the released
songs: the music belongs to its authors. They are published here while the authors are asked for permission, and a song
is removed at any author's request.${contact ? ` To ask, contact ${esc(contact)}.` : ""}</p>
<p class="note">${counts.map(([g, n]) => `<a href="#g-${slugOf(g)}">${esc(g)}</a> (${n})`).join(" · ")}</p>
<label for="search" class="note">Search title, author or text</label><br>
<input id="search" type="search" autocomplete="off">
</header>
<main id="list"></main>
<footer>Space plays or pauses; ← → seek 10 s; N / P next / previous song. Link to a song: copy its "link".
The song is rendered ahead while it plays: the lighter part of the bar is ready, and a seek past it waits for the render.</footer>
<script>
const SONGS = ${JSON.stringify(songs).replace(/</g, "\\u003c")};
const GROUPS = ${JSON.stringify(GROUPS)};
const BANK_URL = ${JSON.stringify(bankUrl).replace(/</g, "\\u003c")};
const WORKER = ${JSON.stringify(WORKER).replace(/</g, "\\u003c")};
const WORKLET = ${JSON.stringify(WORKLET).replace(/</g, "\\u003c")};
const RATE = ${RATE};
const WASM_URL = ${JSON.stringify(wasmUrl)};
const $ = (s) => document.querySelector(s);
const list = $("#list"), now = $("#now"), toggleB = $("#toggle"), seekR = $("#seek"), timeE = $("#time");
const fmt = (s) => Math.floor(s / 60) + ":" + String(Math.floor(s % 60)).padStart(2, "0");
const gid = (g) => "g-" + g.replace(/[^A-Za-z0-9]+/g, "-").replace(/^-|-$/g, "").toLowerCase();
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const blobUrl = (code, type) => URL.createObjectURL(new Blob([code], { type }));

// --- engine: one AudioContext + player worklet, one render worker per song ---
let ctx = null, node = null, module = null, bank = null, worker = null, ready = null;
let cur = -1, playing = false, waiting = false, pos = 0, total = 0, rendered = 0, pending = 0, dragging = false, status = "";
function init() {
  return ready ??= (async () => {
    ctx = new AudioContext({ sampleRate: RATE });
    ctx.onstatechange = show;
    await ctx.audioWorklet.addModule(blobUrl(WORKLET, "text/javascript"));
    node = new AudioWorkletNode(ctx, "song-player", { numberOfInputs: 0, outputChannelCount: [2] });
    node.connect(ctx.destination);
    node.port.onmessage = (e) => {
      const m = e.data;
      if (m.ended) { playing = false; shown.length > 1 ? step(1) : (seekTo(0), mark(), show()); return; }
      if (m.pos !== undefined) { pos = m.pos; waiting = m.waiting; show(); }
    };
    module = await WebAssembly.compileStreaming(fetch(WASM_URL));
  })();
}
async function load(i, at) {
  const s = SONGS[i];
  worker?.terminate();
  total = Math.round(s.seconds * RATE); rendered = 0; pos = Math.min(Math.round(at * RATE), total); status = "loading";
  node.port.postMessage({ reset: true, pos, on: false });
  show();
  const [blob, b] = await Promise.all([
    fetch(s.blob).then((r) => r.arrayBuffer()),
    s.bank ? (bank ??= fetch(BANK_URL).then((r) => { if (!r.ok) throw new Error("GM bank: HTTP " + r.status); return r.arrayBuffer(); })) : null,
  ]);
  if (i !== cur) return; // another song was picked meanwhile
  const ch = new MessageChannel();
  worker = new Worker(blobUrl(WORKER, "text/javascript"));
  worker.onmessage = (e) => {
    const m = e.data;
    if (m.error) { status = "cannot play: " + m.error; playing = false; mark(); }
    if (m.total) total = m.total;
    if (m.rendered) { rendered = m.rendered; if (m.done) status = ""; else status = "rendering"; }
    show();
  };
  worker.onerror = (e) => { status = "cannot play: " + e.message; show(); };
  worker.postMessage({ module, blob: new Uint8Array(blob), bank: b && new Uint8Array(b), port: ch.port1 }, [ch.port1]);
  node.port.postMessage({ feed: ch.port2, on: playing }, [ch.port2]);
}
function setPlaying(on) {
  playing = on;
  if (on) ctx.resume();
  node.port.postMessage({ on });
  mark(); show();
}
function seekTo(sec) {
  if (cur < 0 || !node) return;
  pos = Math.max(0, Math.min(total, Math.round(sec * RATE)));
  node.port.postMessage({ seek: pos });
  show();
}
function show() {
  if (!dragging) { seekR.max = String(total / RATE); seekR.value = String(pos / RATE); }
  seekR.disabled = cur < 0;
  const r = total ? (100 * rendered) / total : 0, p = total ? (100 * pos) / total : 0;
  seekR.style.setProperty("--track", "linear-gradient(to right, var(--hi) " + p + "%, color-mix(in srgb, var(--hi) 35%, transparent) " + p + "% " + r + "%, var(--line) " + r + "%)");
  let t = fmt(pos / RATE) + " / " + fmt(total / RATE);
  if (status.startsWith("cannot")) t = status;
  else if (playing && ctx?.state === "suspended") t += " · the browser blocked sound: press play again";
  else if (playing && waiting) t += " · waiting for the render";
  else if (status === "loading") t += " · loading";
  else if (status === "rendering") t += " · rendered " + Math.floor(r) + "%";
  timeE.textContent = t;
  toggleB.textContent = playing ? "❚❚" : "▶";
  toggleB.setAttribute("aria-label", playing ? "Pause" : "Play");
}

// --- list ---
let shown = SONGS.map((_, i) => i);
function render(q) {
  q = q.trim().toLowerCase();
  shown = [];
  let html = "";
  for (const g of GROUPS) {
    const rows = SONGS.map((s, i) => [s, i]).filter(([s]) => s.group === g && (!q || (s.title + " " + s.author + " " + s.description).toLowerCase().includes(q)));
    if (!rows.length) continue;
    html += '<h2 id="' + gid(g) + '">' + esc(g) + " (" + rows.length + ")</h2><ol>";
    for (const [s, i] of rows) {
      shown.push(i);
      html += '<li id="' + s.slug + '" data-i="' + i + '"' + (i === cur ? ' class="on"' : "") + '><div class="row">' +
        '<button class="play" aria-label="Play ' + esc(s.title) + '">' + (i === cur && playing ? "❚❚" : "▶") + "</button>" +
        '<div><span class="t">' + esc(s.title) + '</span> <span class="a">' + esc(s.author) + "</span></div>" +
        '<span class="d">' + fmt(s.seconds) + "</span></div>" +
        "<details><summary>About</summary><p>" + esc(s.description) + "</p><p>" +
        (s.url ? '<a href="' + esc(s.url) + '" rel="noopener">source</a>' : "") +
        '<a class="link" href="#' + s.slug + '">link</a></p></details></li>';
    }
    html += "</ol>";
  }
  list.innerHTML = html || '<p class="note">No song matches.</p>';
}
async function play(i, at = 0) {
  if (i < 0 || i >= SONGS.length) return;
  await init();
  if (i === cur && status !== "pick") { setPlaying(!playing); return; }
  if (i === cur) at = at || pending; // a #slug@seconds link
  cur = i; status = "";
  const s = SONGS[i];
  now.innerHTML = esc(s.title) + " <span>" + esc(s.author) + "</span>";
  document.title = s.title + " · converted songs";
  history.replaceState(null, "", "#" + s.slug);
  setPlaying(true);
  await load(i, at);
}
function mark() {
  for (const li of list.querySelectorAll("li")) {
    const on = +li.dataset.i === cur;
    li.classList.toggle("on", on);
    li.querySelector("button.play").textContent = on && playing ? "❚❚" : "▶";
  }
}
function step(d) {
  const k = shown.indexOf(cur);
  play(shown[k < 0 ? 0 : (k + d + shown.length) % shown.length]);
}
list.addEventListener("click", (ev) => {
  const b = ev.target.closest("button.play");
  if (b) play(+b.closest("li").dataset.i);
});
toggleB.addEventListener("click", () => (cur < 0 ? play(shown[0]) : status === "pick" ? play(cur, pending) : play(cur)));
seekR.addEventListener("input", () => { dragging = true; timeE.textContent = fmt(+seekR.value) + " / " + fmt(total / RATE); });
seekR.addEventListener("change", () => { dragging = false; status === "pick" ? (pending = +seekR.value) : seekTo(+seekR.value); });
$("#search").addEventListener("input", (ev) => render(ev.target.value));
document.addEventListener("keydown", (ev) => {
  const el = ev.target instanceof Element ? ev.target : null;
  if (el?.closest("input, textarea") || ev.ctrlKey || ev.metaKey || ev.altKey) return;
  // Space on a focused button, link or summary activates it natively
  if (ev.key === " " && el?.closest("button, a, summary")) return;
  if (ev.key === " ") { ev.preventDefault(); toggleB.click(); }
  else if (ev.key === "ArrowRight") seekTo(pos / RATE + 10);
  else if (ev.key === "ArrowLeft") seekTo(pos / RATE - 10);
  else if (ev.key === "n" || ev.key === "N") step(1);
  else if (ev.key === "p" || ev.key === "P") step(-1);
});
render("");
show();
// #slug selects that song (and #slug@seconds starts there); browsers block autoplay, so it waits for a click
const [h, t] = decodeURIComponent(location.hash.slice(1)).split("@");
const j = SONGS.findIndex((s) => s.slug === h);
if (j >= 0) {
  cur = j; status = "pick"; pending = +t || 0;
  total = Math.round(SONGS[j].seconds * RATE); pos = Math.round(pending * RATE);
  now.innerHTML = esc(SONGS[j].title) + " <span>" + esc(SONGS[j].author) + " · press play</span>";
  mark(); show();
  document.getElementById(h)?.scrollIntoView({ block: "center" });
}
</script>
</body>
</html>
`;
}

if (import.meta.main) await main();
