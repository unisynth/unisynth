// Listening page for every song in project/music (the demo songs and project/music/released/**). Each song is exported
// to its compact binary form (`song export`, tens of KB) and played in the browser by the synth itself: a worker renders the
// song ahead with the engine module (synth.wasm, `bounce_open`/`bounce_step`, equal to `song render`) and an AudioWorklet
// plays the rendered part, so playback starts at once and a seek inside it is instant. One static page with a shared
// player deck (play, pause, seek bar with live spectrum, loop), a library (search, collection chips, sort) and a detail
// panel per song (stats, credits, links `#<slug>`, `#<slug>@<seconds>`); layout and tokens as gm-procedural.exceed.hu/songs.
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
  url?: string; copyright?: string; seconds: number; blob: string; bytes: number; bank: boolean; tracks: number; devices: number;
};

/** Tracks and devices of a song source (the detail panel's stat cards). */
export function sizeOf(song: any): { tracks: number; devices: number } {
  const t = song.tracks ?? [];
  return { tracks: t.length, devices: t.reduce((n: number, x: any) => n + (x.devices?.length ?? 0), 0) };
}

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
      description: m.description ?? "", url: m.url, copyright: m.copyright, seconds: 0, blob: `songs/${slug}.bin`, bytes: 0, bank: needsBank(s), ...sizeOf(s),
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
  const sizes = entries.map((e) => e.bytes).sort((a, b) => a - b), median = sizes.length ? sizes[sizes.length >> 1] : 0;
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

// Group accents (the site's module colours) and short chip labels.
const ACCENT: Record<string, string> = {
  "Demo songs": "env", "64K intros and executable music (WaveSabre)": "osc", "The Black Lotus, 1997": "mod",
  "Farbrausch productions": "filt", "V2M collection (via modland)": "lfo",
};
const CHIP: Record<string, string> = {
  "Demo songs": "demo", "64K intros and executable music (WaveSabre)": "WaveSabre 64K", "The Black Lotus, 1997": "TBL 1997",
  "Farbrausch productions": "Farbrausch", "V2M collection (via modland)": "V2M collection",
};
const ICON = `data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'%3E%3Ctitle%3Eunisynth%3C/title%3E%3Cdefs%3E%3ClinearGradient id='t' x1='0' y1='0' x2='0' y2='1'%3E%3Cstop offset='0' stop-color='%231b1e23'/%3E%3Cstop offset='1' stop-color='%230e1013'/%3E%3C/linearGradient%3E%3ClinearGradient id='w' x1='8' y1='0' x2='56' y2='0' gradientUnits='userSpaceOnUse'%3E%3Cstop offset='.12' stop-color='%23f0a04b'/%3E%3Cstop offset='.36' stop-color='%233fc1c9'/%3E%3Cstop offset='.64' stop-color='%233fc1c9'/%3E%3Cstop offset='.88' stop-color='%236fd08c'/%3E%3C/linearGradient%3E%3C/defs%3E%3Crect x='1' y='1' width='62' height='62' rx='14' fill='url(%23t)' stroke='%232c3139' stroke-width='1.5'/%3E%3Cpath d='M8 33.4L8.8 33.5L9.5 33.4L10.3 33.2L11 32.8L11.8 32.3L12.5 31.7L13.3 30.9L14 30L14.8 29L15.5 27.9L16.3 26.7L17 25.5L17.8 24.3L18.5 23.1L19.3 22.2L20 21.5L20.8 21L21.5 21L22.3 21.3L23 22.2L23.8 23.5L24.5 25.3L25.3 27.5L26 30L26.8 32.7L27.5 35.6L28.3 38.3L29 40.8L29.8 43L30.5 44.6L31.3 45.7L32 46L32.8 45.7L33.5 44.6L34.3 43L35 40.8L35.8 38.3L36.5 35.6L37.3 32.7L38 30L38.8 27.5L39.5 25.3L40.3 23.5L41 22.2L41.8 21.3L42.5 21L43.3 21L44 21.5L44.8 22.2L45.5 23.1L46.3 24.3L47 25.5L47.8 26.7L48.5 27.9L49.3 29L50 30L50.8 30.9L51.5 31.7L52.3 32.3L53 32.8L53.8 33.2L54.5 33.4L55.3 33.5L56 33.4' fill='none' stroke='url(%23w)' stroke-width='5.5' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E`;
const GH = `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38v-1.33c-2.23.48-2.7-1.07-2.7-1.07-.36-.92-.89-1.17-.89-1.17-.73-.5.06-.49.06-.49.8.06 1.23.83 1.23.83.72 1.22 1.88.87 2.34.66.07-.52.28-.87.5-1.07-1.78-.2-3.65-.89-3.65-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82a7.6 7.6 0 0 1 4 0c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48v2.2c0 .21.15.46.55.38A8 8 0 0 0 16 8c0-4.42-3.58-8-8-8"/></svg>`;

export function page(entries: Entry[], bankUrl = "../daw/data/bank.dls", wasmUrl = "synth.wasm"): string {
  const contact = process.env.CONTACT;
  const groups = GROUPS.filter((g) => entries.some((e) => e.group === g));
  const songs = entries.map(({ slug, group, title, author, description, url, copyright, seconds, blob, bytes, bank, tracks, devices }) =>
    ({ slug, group, title, author, description, url, copyright, seconds, blob, bytes, bank, tracks, devices }));
  const hours = entries.reduce((a, e) => a + e.seconds, 0) / 3600;
  const sizes = entries.map((e) => e.bytes).sort((a, b) => a - b), median = sizes.length ? sizes[sizes.length >> 1] : 0;
  const js = (v: unknown) => JSON.stringify(v).replace(/</g, "\\u003c");
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>Songs · unisynth</title>
<link rel="icon" type="image/svg+xml" href="${ICON}">
<meta name="theme-color" content="#0e1013">
<style>
/* Same design language as unisynth.exceed.hu and gm-procedural.exceed.hu/songs (palette = the DAW's tokens). */
:root {
  color-scheme: dark;
  --bg: #0e1013; --bg-2: #121417; --panel: #1b1e23; --panel-2: #20242a; --line: #2c3139; --line-2: #23272e;
  --fg: #e4e8ee; --dim: #8a94a3; --faint: #5f6875; --well: #0b0c0f;
  --ok: #5fd38d; --warn: #e7b84c; --err: #ff6b6b; --focus: #6aa7ff; --sel: #263040; --playhead: #e7b84c;
  --osc: #f0a04b; --filt: #3fc1c9; --env: #6fd08c; --lfo: #a98bf5; --mod: #ee7aa8; --voice: #8fa6c4;
  --mono: ui-monospace, "SF Mono", SFMono-Regular, Menlo, Consolas, monospace;
  --sans: system-ui, -apple-system, "Segoe UI", sans-serif;
  --w: 1240px;
}
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--fg); font: 16px/1.6 var(--sans); }
a { color: inherit; text-decoration-color: var(--line); text-underline-offset: 3px; }
a:hover { text-decoration-color: var(--focus); }
:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; }
.wrap { max-width: var(--w); margin: 0 auto; padding: 0 28px; }
.wrap > * { min-width: 0; }
.skip { position: absolute; left: -999px; top: 8px; background: var(--panel); padding: 6px 10px; border-radius: 4px; z-index: 30; }
.skip:focus { left: 8px; }
.sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); }
kbd { font: 11.5px var(--mono); border: 1px solid var(--line); border-bottom-width: 2px; border-radius: 4px; padding: 0 5px; color: var(--fg); }

.transport { position: fixed; inset: 0 0 auto 0; z-index: 20; background: color-mix(in srgb, var(--bg-2) 88%, transparent); backdrop-filter: blur(10px); border-bottom: 1px solid var(--line); }
.transport .wrap { display: flex; align-items: center; gap: 16px; height: 52px; }
.logo { font: 600 15px var(--mono); text-decoration: none; display: flex; align-items: center; gap: 9px; white-space: nowrap; }
.logo img { display: block; }
.sp { flex: 1; }
.tlinks { display: flex; gap: 6px; align-items: center; }
.tlinks a { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; text-decoration: none; padding: 5px 10px; border-radius: 5px; border: 1px solid var(--line); }
.tlinks a:hover, .tlinks a[aria-current] { background: var(--sel); }
.tlinks .live b { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 8px var(--ok); }
.tlinks svg { width: 15px; height: 15px; fill: currentColor; }

.app { padding-top: 52px; }
.deck { position: sticky; top: 52px; z-index: 10; background: color-mix(in srgb, var(--bg-2) 94%, transparent); backdrop-filter: blur(10px); border-bottom: 1px solid var(--line); }
.deck .wrap { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 10px 22px; padding-top: 14px; padding-bottom: 14px; align-items: center; }
.now { min-width: 0; }
.now .t { font: 650 19px/1.25 var(--sans); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; letter-spacing: -.01em; }
.now .b { font: 12.5px var(--mono); color: var(--dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.engine { font: 12px var(--mono); color: var(--dim); border: 1px solid var(--line); background: var(--well); border-radius: 8px; padding: 6px 12px; text-align: right; line-height: 1.35; white-space: nowrap; }
.engine b { color: var(--fg); font-weight: 600; }
.engine small { display: block; font-size: 10.5px; color: var(--faint); }
.tbar { grid-column: 1 / -1; display: grid; grid-template-columns: 38px 1fr auto auto; gap: 12px; align-items: center; }
.play { width: 38px; height: 38px; border-radius: 50%; border: 1px solid var(--ok); background: none; color: var(--ok); cursor: pointer; font-size: 13px; display: grid; place-items: center; }
.play:hover { background: color-mix(in srgb, var(--ok) 14%, transparent); }
.seek { position: relative; height: 34px; cursor: pointer; touch-action: none; }
.seek[aria-disabled="true"] { cursor: default; }
.seek canvas { position: absolute; inset: 0; width: 100%; height: 100%; display: block; }
.time { font: 13px var(--mono); color: var(--playhead); border: 1px solid var(--line); background: var(--well); border-radius: 5px; padding: 3px 10px; min-width: 116px; text-align: right; font-variant-numeric: tabular-nums; }
.loop { all: unset; cursor: pointer; font: 12px var(--mono); color: var(--dim); border: 1px solid var(--line); border-radius: 5px; padding: 4px 9px; }
.loop[aria-pressed="true"] { color: var(--fg); border-color: var(--dim); background: var(--sel); }
.loop:focus-visible { outline: 2px solid var(--focus); outline-offset: 1px; }

.lib { display: grid; grid-template-columns: 380px minmax(0, 1fr); gap: 28px; padding-top: 28px; padding-bottom: 40px; align-items: start; }
.side { position: sticky; top: 200px; max-height: calc(100vh - 220px); display: flex; flex-direction: column; border: 1px solid var(--line); border-radius: 12px; background: var(--bg-2); overflow: hidden; }
.side .tools { padding: 12px; border-bottom: 1px solid var(--line); display: grid; gap: 10px; }
.search { width: 100%; padding: 9px 12px; font: 14px var(--sans); border: 1px solid var(--line); border-radius: 7px; background: var(--well); color: var(--fg); }
.search:focus { border-color: var(--focus); outline: none; }
.chips { display: flex; flex-wrap: wrap; gap: 6px; }
.chips button { all: unset; cursor: pointer; font: 12px var(--mono); color: var(--dim); border: 1px solid var(--line); border-radius: 99px; padding: 3px 10px; }
.chips button i { display: inline-block; width: 7px; height: 7px; border-radius: 50%; background: var(--c); margin-right: 6px; vertical-align: 1px; }
.chips button[aria-pressed="true"] { color: var(--well); background: var(--fg); border-color: var(--fg); }
.chips button:focus-visible { outline: 2px solid var(--focus); outline-offset: 1px; }
.sort { display: flex; gap: 8px; align-items: center; font: 12px var(--mono); color: var(--faint); }
.sort select { font: 12px var(--mono); background: var(--well); color: var(--fg); border: 1px solid var(--line); border-radius: 5px; padding: 3px 6px; }
.list { overflow-y: auto; margin: 0; padding: 4px 0; list-style: none; }
.list li button { all: unset; box-sizing: border-box; cursor: pointer; width: 100%; display: grid; grid-template-columns: 1fr auto; gap: 2px 10px; padding: 9px 14px; border-left: 3px solid transparent; }
.list li button:hover { background: var(--panel); }
.list li button[aria-current="true"] { background: var(--sel); border-left-color: var(--ok); }
.list li button:focus-visible { outline: 2px solid var(--focus); outline-offset: -2px; }
.list .tt { font-size: 14.5px; font-weight: 550; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.list .tt .eq { color: var(--ok); font-size: 11px; margin-right: 6px; }
.list .by { font: 11.5px var(--mono); color: var(--faint); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.list .by i { display: inline-block; width: 6px; height: 6px; border-radius: 50%; background: var(--c); margin-right: 6px; vertical-align: 1px; }
.list .du { font: 12px var(--mono); color: var(--dim); text-align: right; }
.list .tags { display: flex; gap: 4px; justify-content: flex-end; }
.tag { font: 10.5px var(--mono); padding: 0 5px; border-radius: 3px; border: 1px solid currentColor; line-height: 16px; color: var(--warn); }
.count { font: 12px var(--mono); color: var(--faint); padding: 8px 14px; border-top: 1px solid var(--line); }
.empty { padding: 20px 14px; color: var(--dim); font-size: 14px; }

.kicker { font: 12px var(--mono); color: var(--c, var(--dim)); text-transform: uppercase; letter-spacing: .1em; margin: 0 0 8px; }
.detail h1 { font: 750 clamp(30px, 3.6vw, 48px)/1.04 var(--sans); letter-spacing: -.03em; margin: 0; }
.detail .cr { font: 13px var(--mono); color: var(--dim); margin: 10px 0 0; }
.stats { display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; margin: 24px 0 8px; }
.stats .m { border: 1px solid var(--line); border-top: 2px solid var(--c); border-radius: 10px; padding: 14px 16px; background: var(--panel); }
.stats .m .v { font: 700 28px/1 var(--mono); letter-spacing: -.03em; }
.stats .m .l { font-size: 12.5px; color: var(--dim); margin-top: 8px; line-height: 1.4; }
.about { font-size: 15px; color: var(--fg); margin-top: 22px; max-width: 46rem; }
.about p { margin: 0 0 10px; }
code { font: .86em var(--mono); background: var(--panel-2); border: 1px solid var(--line); border-radius: 4px; padding: 0 5px; }
.links { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 14px; }
.links a, .links button { all: unset; cursor: pointer; font: 12.5px var(--mono); color: var(--dim); border: 1px solid var(--line); border-radius: 5px; padding: 4px 10px; }
.links a:hover, .links button:hover { color: var(--fg); background: var(--sel); }
.links a:focus-visible, .links button:focus-visible { outline: 2px solid var(--focus); outline-offset: 1px; }
.intro h1 { font: 750 clamp(30px, 3.6vw, 48px)/1.04 var(--sans); letter-spacing: -.03em; margin: 0 0 14px; }
.intro .lead { color: var(--dim); max-width: 46rem; margin: 0; }
.cols { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 12px; margin-top: 24px; }
.cols button { all: unset; cursor: pointer; border: 1px solid var(--line); border-top: 2px solid var(--c); border-radius: 10px; background: var(--panel); padding: 14px 16px; }
.cols button:hover { background: var(--panel-2); }
.cols button:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; }
.cols .v { font: 700 28px/1 var(--mono); }
.cols .l { font-size: 13px; color: var(--dim); margin-top: 8px; line-height: 1.35; }
.help { font-size: 14px; color: var(--dim); border-top: 1px solid var(--line); margin-top: 28px; padding-top: 16px; }

@media (max-width: 1020px) {
  .lib { grid-template-columns: 1fr; }
  .side { position: static; max-height: 420px; }
  .stats { grid-template-columns: 1fr 1fr; }
}
@media (max-width: 640px) {
  .wrap { padding: 0 18px; }
  .tlinks .lbl { display: none; }
  .deck .wrap { grid-template-columns: 1fr; }
  .engine { display: none; }
  .tbar { grid-template-columns: 38px 1fr auto; } .tbar .loop { display: none; }
  .time { min-width: 0; }
}
</style>
</head>
<body>
<a class="skip" href="#detail">Skip to the song</a>
<header class="transport" id="transport">
  <div class="wrap">
    <a class="logo" href="../"><img src="${ICON}" alt="" width="22" height="22">unisynth</a>
    <span class="sp"></span>
    <div class="tlinks"><a href="../daw/">DAW</a><a class="live" href="./" aria-current="page"><b aria-hidden="true"></b>Songs</a><a href="https://github.com/unisynth/unisynth" aria-label="unisynth on GitHub">${GH}<span class="lbl">GitHub</span></a></div>
  </div>
</header>

<main class="app">
  <div class="deck" id="deck" role="region" aria-label="Player">
    <div class="wrap">
      <div class="now" aria-live="polite"><div class="t" id="nowT">Pick a song</div><div class="b" id="nowB">${entries.length} songs · ${hours.toFixed(1)} hours · played live by the synth in your browser</div></div>
      <div class="engine" id="engine"><b>unisynth</b> · WebAssembly<small>no audio files: rendered from the song</small></div>
      <div class="tbar">
        <button class="play" id="toggle" aria-label="Play">▶</button>
        <div class="seek" id="seek" role="slider" aria-label="Position" aria-valuemin="0" aria-valuemax="0" aria-valuenow="0" tabindex="0" aria-disabled="true"><canvas></canvas></div>
        <button class="loop" id="loop" aria-pressed="false" title="Loop (L)">loop</button>
        <output class="time" id="time">0:00 / 0:00</output>
      </div>
    </div>
  </div>

  <div class="wrap lib">
    <aside class="side" aria-label="Songs">
      <div class="tools">
        <label class="sr" for="q">Search songs</label>
        <input id="q" class="search" type="search" placeholder="Search title, author or text…" autocomplete="off">
        <div class="chips" id="chips" role="group" aria-label="Collection">
          <button data-v="" aria-pressed="true">all</button>${groups.map((g) => `<button data-v="${esc(g)}" aria-pressed="false" style="--c: var(--${ACCENT[g]})"><i></i>${esc(CHIP[g])}</button>`).join("")}
        </div>
        <div class="sort"><label for="sort">sort</label>
          <select id="sort"><option value="group">collection</option><option value="title">title</option><option value="author">author</option><option value="long">longest</option><option value="short">shortest</option><option value="small">smallest file</option></select></div>
      </div>
      <ol class="list" id="list"></ol>
      <div class="count" id="count"></div>
    </aside>

    <article class="detail" id="detail" tabindex="-1"></article>
  </div>
</main>

<script>
const SONGS = ${js(songs)};
const GROUPS = ${JSON.stringify(groups)};
const ACCENT = ${JSON.stringify(ACCENT)};
const CHIP = ${JSON.stringify(CHIP)};
const CONTACT = ${js(contact ?? "")};
const MEDIAN = ${median};
const BANK_URL = ${js(bankUrl)};
const WORKER = ${js(WORKER)};
const WORKLET = ${js(WORKLET)};
const RATE = ${RATE};
const WASM_URL = ${JSON.stringify(wasmUrl)};
const $ = (s) => document.querySelector(s);
const listE = $("#list"), countE = $("#count"), detailE = $("#detail"), nowT = $("#nowT"), nowB = $("#nowB");
const toggleB = $("#toggle"), seekE = $("#seek"), canvas = seekE.querySelector("canvas"), timeE = $("#time"), loopB = $("#loop"), qE = $("#q"), sortE = $("#sort");
const fmt = (s) => Math.floor(s / 60) + ":" + String(Math.floor(s % 60)).padStart(2, "0");
const kb = (b) => (b < 1024 ? b + " B" : (b / 1024).toFixed(b < 10240 ? 1 : 0) + " KB");
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const blobUrl = (code, type) => URL.createObjectURL(new Blob([code], { type }));
// description text: escaped, then \`code\` spans and bare http(s) URLs as links
const rich = (t) => esc(t).replace(/\`([^\`]+)\`/g, "<code>$1</code>").replace(/https?:\\/\\/[^\\s)<]+[^\\s)<.,;:]/g, (u) => '<a href="' + u + '" rel="noopener">' + u + "</a>");
const accent = (g) => "var(--" + ACCENT[g] + ")";
const cssVar = (g) => getComputedStyle(document.documentElement).getPropertyValue("--" + ACCENT[g]).trim();

// --- engine: one AudioContext + player worklet (→ analyser for the seek bar's spectrum), one render worker per song ---
let ctx = null, node = null, an = null, bins = null, module = null, bank = null, worker = null, ready = null;
let cur = -1, playing = false, waiting = false, pos = 0, total = 0, rendered = 0, pending = 0, dragging = null, status = "", loop = false;
function init() {
  return ready ??= (async () => {
    ctx = new AudioContext({ sampleRate: RATE });
    ctx.onstatechange = show;
    await ctx.audioWorklet.addModule(blobUrl(WORKLET, "text/javascript"));
    node = new AudioWorkletNode(ctx, "song-player", { numberOfInputs: 0, outputChannelCount: [2] });
    an = ctx.createAnalyser(); an.fftSize = 2048; an.smoothingTimeConstant = 0.75; bins = new Uint8Array(an.frequencyBinCount);
    node.connect(an); an.connect(ctx.destination);
    node.port.onmessage = (e) => {
      const m = e.data;
      if (m.ended) {
        if (loop) { seekTo(0); setPlaying(true); }
        else { playing = false; shown.length > 1 ? step(1) : (seekTo(0), mark(), show()); }
        return;
      }
      if (m.pos !== undefined) { pos = m.pos; waiting = m.waiting; show(); }
    };
    // streaming needs the server's application/wasm type; fall back to compiling the bytes when it is missing
    module = await WebAssembly.compileStreaming(fetch(WASM_URL)).catch(async () => WebAssembly.compile(await (await fetch(WASM_URL)).arrayBuffer()));
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
    if (m.rendered) { rendered = m.rendered; status = m.done ? "" : "rendering"; }
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
  if (cur < 0) return;
  if (status === "pick" || !node) { pending = Math.max(0, Math.min(total / RATE, sec)); pos = Math.round(pending * RATE); show(); return; }
  pos = Math.max(0, Math.min(total, Math.round(sec * RATE)));
  node.port.postMessage({ seek: pos });
  show();
}
function show() {
  const p = dragging != null ? dragging : pos;
  let t = fmt(p / RATE) + " / " + fmt(total / RATE);
  if (status.startsWith("cannot")) t = status;
  else if (playing && ctx?.state === "suspended") t += " · press play";
  else if (playing && waiting) t += " · rendering…";
  else if (status === "loading") t += " · loading";
  timeE.textContent = t;
  seekE.setAttribute("aria-disabled", String(cur < 0));
  seekE.setAttribute("aria-valuemax", String(Math.round(total / RATE)));
  seekE.setAttribute("aria-valuenow", String(Math.round(p / RATE)));
  seekE.setAttribute("aria-valuetext", fmt(p / RATE));
  toggleB.textContent = playing ? "❚❚" : "▶";
  toggleB.setAttribute("aria-label", playing ? "Pause" : "Play");
  const eng = $("#engine");
  if (cur >= 0) {
    const r = total ? Math.floor((100 * rendered) / total) : 0;
    eng.innerHTML = "<b>" + (status === "loading" ? "loading" : status === "pick" ? "ready" : r + "% rendered") + "</b> · " + kb(SONGS[cur].bytes) +
      "<small>" + (SONGS[cur].bank ? "+ GM.DLS samples · " : "") + "synth in WebAssembly</small>";
  }
}
// seek bar: 96 log-spaced spectrum columns (live while playing), the rendered part shaded, the playhead in yellow
function paint() {
  const dpr = devicePixelRatio || 1, w = canvas.clientWidth, h = canvas.clientHeight;
  if (canvas.width !== Math.round(w * dpr)) { canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr); }
  const g = canvas.getContext("2d"); g.setTransform(dpr, 0, 0, dpr, 0, 0); g.clearRect(0, 0, w, h);
  const col = cur >= 0 ? cssVar(SONGS[cur].group) || "#3fc1c9" : "#3fc1c9";
  const p = total ? Math.min(1, (dragging != null ? dragging : pos) / total) : 0, r = total ? Math.min(1, rendered / total) : 0;
  const live = an && playing && !waiting;
  if (live) an.getByteFrequencyData(bins);
  const n = 96, bw = w / n;
  for (let i = 0; i < n; i++) {
    const f0 = 40 * Math.pow(11000 / 40, i / n), bin = Math.min(bins ? bins.length - 1 : 0, Math.round((f0 / (RATE / 2)) * (bins ? bins.length : 0)));
    const v = live ? bins[bin] / 255 : 0, bh = Math.max(1, v * v * (h - 6));
    g.fillStyle = i / n < p ? col : "#2c3139"; g.globalAlpha = i / n < p ? 0.55 : 0.9;
    g.fillRect(i * bw + 1, h / 2 - bh / 2, bw - 2, bh);
  }
  g.globalAlpha = 1;
  g.fillStyle = "#3a414c"; g.fillRect(0, h / 2 - 1, w, 2);
  if (cur >= 0 && r < 1) { g.fillStyle = col; g.globalAlpha = 0.16; g.fillRect(0, 4, r * w, h - 8); g.globalAlpha = 1; }
  g.fillStyle = col; g.fillRect(0, h / 2 - 1, p * w, 2);
  if (cur >= 0) { g.fillStyle = "#e7b84c"; g.shadowColor = "#e7b84c"; g.shadowBlur = 10; g.fillRect(p * w - 1, 2, 2, h - 4); g.shadowBlur = 0; }
  requestAnimationFrame(paint);
}
const at = (ev) => { const b = seekE.getBoundingClientRect(); return Math.max(0, Math.min(1, (ev.clientX - b.left) / b.width)) * total; };
seekE.addEventListener("pointerdown", (ev) => { if (cur < 0) return; seekE.setPointerCapture(ev.pointerId); dragging = at(ev); show(); });
seekE.addEventListener("pointermove", (ev) => { if (dragging != null) { dragging = at(ev); show(); } });
seekE.addEventListener("pointerup", (ev) => { if (dragging == null) return; const s = at(ev) / RATE; dragging = null; seekTo(s); });
seekE.addEventListener("pointercancel", () => { dragging = null; show(); });
seekE.addEventListener("keydown", (ev) => {
  if (ev.key === "ArrowRight" || ev.key === "ArrowLeft") { ev.preventDefault(); ev.stopPropagation(); seekTo(pos / RATE + (ev.key === "ArrowRight" ? 10 : -10)); }
  else if (ev.key === "Home") { ev.preventDefault(); seekTo(0); }
});

// --- library: search, collection chips, sort ---
let shown = SONGS.map((_, i) => i), group = "";
function render() {
  const q = qE.value.trim().toLowerCase(), by = sortE.value;
  shown = SONGS.map((_, i) => i).filter((i) => {
    const s = SONGS[i];
    return (!group || s.group === group) && (!q || (s.title + " " + s.author + " " + s.description + " " + s.group).toLowerCase().includes(q));
  });
  const cmp = {
    group: (a, b) => GROUPS.indexOf(SONGS[a].group) - GROUPS.indexOf(SONGS[b].group) || a - b,
    title: (a, b) => SONGS[a].title.localeCompare(SONGS[b].title),
    author: (a, b) => (SONGS[a].author || "~").localeCompare(SONGS[b].author || "~") || SONGS[a].title.localeCompare(SONGS[b].title),
    long: (a, b) => SONGS[b].seconds - SONGS[a].seconds, short: (a, b) => SONGS[a].seconds - SONGS[b].seconds,
    small: (a, b) => SONGS[a].bytes - SONGS[b].bytes,
  }[by];
  shown.sort(cmp);
  listE.innerHTML = shown.length ? shown.map((i) => {
    const s = SONGS[i];
    return '<li><button data-i="' + i + '" style="--c: ' + accent(s.group) + '"' + (i === cur ? ' aria-current="true"' : "") + ">" +
      '<span class="tt">' + (i === cur && playing ? '<span class="eq" aria-hidden="true">▶</span>' : "") + esc(s.title) + "</span>" +
      '<span class="du">' + fmt(s.seconds) + "</span>" +
      '<span class="by"><i></i>' + esc(s.author || CHIP[s.group]) + "</span>" +
      '<span class="tags">' + (s.bank ? '<span class="tag" title="plays GM.DLS samples">GM</span>' : "") + "</span></button></li>";
  }).join("") : '<li class="empty">No song matches.</li>';
  countE.textContent = shown.length + " of " + SONGS.length + " songs · " + (shown.reduce((a, i) => a + SONGS[i].seconds, 0) / 3600).toFixed(1) + " h";
}
function mark() {
  for (const b of listE.querySelectorAll("button[data-i]")) {
    const i = +b.dataset.i, on = i === cur;
    b.toggleAttribute("aria-current", on); if (on) b.setAttribute("aria-current", "true");
    const tt = b.querySelector(".tt"), eq = tt.querySelector(".eq");
    if (on && playing && !eq) tt.insertAdjacentHTML("afterbegin", '<span class="eq" aria-hidden="true">▶</span>');
    if (!(on && playing) && eq) eq.remove();
  }
}
function intro() {
  const n = (g) => SONGS.filter((s) => s.group === g).length;
  detailE.innerHTML = '<div class="intro"><p class="kicker">Listening room</p><h1>Songs on unisynth</h1>' +
    '<p class="lead">' + SONGS.length + " songs converted to the 64k project's own synthesizer. Your browser plays each one live with " +
    "that synthesizer (WebAssembly), from its song file (median " + kb(MEDIAN) + "); there are no audio recordings here. Each is an adapted " +
    "version, not the original recording, and sounds close to but not identical with it. No licence is stated for the released " +
    "songs: the music belongs to its authors. They are published here while the authors are asked for permission, and a song " +
    "is removed at any author's request." + (CONTACT ? " To ask, contact " + esc(CONTACT) + "." : "") + "</p>" +
    '<div class="cols">' + GROUPS.map((g) => '<button data-g="' + esc(g) + '" style="--c: ' + accent(g) + '"><div class="v">' + n(g) +
      '</div><div class="l">' + esc(g) + "</div></button>").join("") + "</div>" + help() + "</div>";
}
function help() {
  return '<p class="help"><kbd>Space</kbd> plays or pauses, <kbd>←</kbd> <kbd>→</kbd> seek 10 s, <kbd>N</kbd> / <kbd>P</kbd> next / previous song, ' +
    "<kbd>L</kbd> loop, <kbd>/</kbd> search. The song is rendered ahead while it plays: the shaded part of the bar is ready, and a seek past it waits for the render.</p>";
}
function detail(i) {
  const s = SONGS[i];
  detailE.style.setProperty("--c", accent(s.group));
  detailE.innerHTML = '<p class="kicker">' + esc(s.group) + "</p><h1>" + esc(s.title) + "</h1>" +
    '<p class="cr">' + [s.author, fmt(s.seconds), s.tracks + " tracks"].filter(Boolean).map(esc).join(" · ") + "</p>" +
    '<div class="stats">' +
    '<div class="m"><div class="v">' + kb(s.bytes) + '</div><div class="l">song file the synth plays (exported <code>synth-mix</code> blob)</div></div>' +
    '<div class="m"><div class="v">' + fmt(s.seconds) + '</div><div class="l">length, rendered live as it plays</div></div>' +
    '<div class="m"><div class="v">' + s.tracks + '</div><div class="l">tracks</div></div>' +
    '<div class="m"><div class="v">' + s.devices + '</div><div class="l">devices (instruments and effects)' + (s.bank ? ", plus GM.DLS samples" : "") + "</div></div></div>" +
    '<div class="about">' + (s.description ? "<p>" + rich(s.description) + "</p>" : "") + (s.copyright ? '<p class="cr">© ' + esc(s.copyright) + "</p>" : "") + "</div>" +
    '<div class="links">' + (s.url ? '<a href="' + esc(s.url) + '" rel="noopener">source ↗</a>' : "") +
    '<button data-copy="' + esc(s.slug) + '">copy link</button><button data-copyat="' + esc(s.slug) + '">copy link at current time</button></div>' + help();
}
function select(i) {
  cur = i;
  const s = SONGS[i];
  nowT.textContent = s.title;
  nowB.textContent = [s.author, CHIP[s.group], fmt(s.seconds)].filter(Boolean).join(" · ");
  document.title = s.title + " · unisynth songs";
  history.replaceState(null, "", "#" + s.slug);
  detail(i); mark();
}
async function play(i, at = 0) {
  if (i == null || i < 0 || i >= SONGS.length) return;
  await init();
  if (i === cur && status !== "pick") { setPlaying(!playing); return; }
  if (i === cur) at = at || pending; // a #slug@seconds link
  select(i); status = "";
  setPlaying(true);
  await load(i, at);
}
function step(d) {
  const k = shown.indexOf(cur);
  play(shown[k < 0 ? 0 : (k + d + shown.length) % shown.length]);
}
listE.addEventListener("click", (ev) => { const b = ev.target.closest("button[data-i]"); if (b) play(+b.dataset.i); });
detailE.addEventListener("click", (ev) => {
  const g = ev.target.closest("button[data-g]");
  if (g) { setGroup(g.dataset.g); qE.focus(); return; }
  const c = ev.target.closest("button[data-copy], button[data-copyat]");
  if (c) {
    const u = location.origin + location.pathname + "#" + (c.dataset.copy || c.dataset.copyat + "@" + Math.floor(pos / RATE));
    navigator.clipboard?.writeText(u).then(() => { const t = c.textContent; c.textContent = "copied"; setTimeout(() => (c.textContent = t), 1200); });
  }
});
function setGroup(g) {
  group = g;
  for (const b of document.querySelectorAll("#chips button")) b.setAttribute("aria-pressed", String(b.dataset.v === g));
  render();
}
$("#chips").addEventListener("click", (ev) => { const b = ev.target.closest("button"); if (b) setGroup(b.dataset.v); });
qE.addEventListener("input", render);
sortE.addEventListener("change", render);
toggleB.addEventListener("click", () => (cur < 0 ? play(shown[0]) : status === "pick" ? play(cur, pending) : play(cur)));
loopB.addEventListener("click", () => { loop = !loop; loopB.setAttribute("aria-pressed", String(loop)); });
document.addEventListener("keydown", (ev) => {
  const el = ev.target instanceof Element ? ev.target : null;
  if (el?.closest("input, textarea, select") || ev.ctrlKey || ev.metaKey || ev.altKey) return;
  if (ev.key === " " && el?.closest("button, a")) return; // Space on a focused control activates it natively
  if (ev.key === " ") { ev.preventDefault(); toggleB.click(); }
  else if (ev.key === "ArrowRight") seekTo(pos / RATE + 10);
  else if (ev.key === "ArrowLeft") seekTo(pos / RATE - 10);
  else if (ev.key === "n" || ev.key === "N") step(1);
  else if (ev.key === "p" || ev.key === "P") step(-1);
  else if (ev.key === "l" || ev.key === "L") loopB.click();
  else if (ev.key === "/") { ev.preventDefault(); qE.focus(); }
});
render();
// #slug selects that song (and #slug@seconds starts there); browsers block autoplay, so it waits for a click
const [h, t] = decodeURIComponent(location.hash.slice(1)).split("@");
const j = SONGS.findIndex((s) => s.slug === h);
if (j >= 0) {
  select(j); status = "pick"; pending = +t || 0;
  total = Math.round(SONGS[j].seconds * RATE); pos = Math.round(pending * RATE);
  nowB.textContent += " · press play";
  listE.querySelector('[data-i="' + j + '"]')?.scrollIntoView({ block: "center" });
} else intro();
show();
requestAnimationFrame(paint);
</script>
</body>
</html>
`;
}

if (import.meta.main) await main();
