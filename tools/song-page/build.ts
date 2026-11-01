// Listening page for every song in project/music (the demo songs and project/music/released/**): each song is rendered
// once by `song render`, encoded to AAC (fast start, so the browser seeks before the download ends), and listed on one
// static page with a shared player (play, pause, seek), search, credits and a link per song (`#<slug>`).
//
//   bun tools/song-page/build.ts [outDir]     # default tools/song-page/out (git-ignored)
//   GM_DLS=<gm.dls>  the Windows GM bank for WaveSabre Adultery instruments (default ~/Downloads/64k-tools/synth-lab/ws/gm.dls)
//   JOBS=8           parallel renders (each with --threads 2)
//   CONTACT=<text>   who to ask for a song's removal, shown on the page
//   ONLY=<regex>     only songs whose path matches (a quick try; the page lists only those)
//
// Renders are cached by the song file's sha256 (out/cache.json): a re-run renders only new or changed songs and drops
// audio of songs that are gone. Only rendered audio is published, never the GM bank.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));
const OUT = process.argv[2] ?? join(ROOT, "tools/song-page/out");
const MUSIC = join(ROOT, "project/music");
const SONG = join(ROOT, "target/release/song");
const BANK = process.env.GM_DLS ?? join(process.env.HOME ?? "", "Downloads/64k-tools/synth-lab/ws/gm.dls");
const JOBS = Number(process.env.JOBS ?? 8);

export type Entry = {
  slug: string; file: string; group: string; title: string; author: string; description: string;
  url?: string; copyright?: string; seconds: number; audio: string; bytes: number;
};

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

async function run(cmd: string[]): Promise<void> {
  const p = Bun.spawn(cmd, { stdout: "ignore", stderr: "pipe" });
  const err = await new Response(p.stderr).text();
  if ((await p.exited) !== 0) throw new Error(`${cmd.slice(0, 2).join(" ")} ${basename(cmd[2] ?? "")}: ${err.trim().split("\n").slice(-3).join(" | ")}`);
}

function wavSeconds(f: string): number {
  const b = readFileSync(f);
  const rate = b.readUInt32LE(24), align = b.readUInt16LE(32);
  let p = 12;
  while (p + 8 <= b.length) {
    const id = b.toString("ascii", p, p + 4), n = b.readUInt32LE(p + 4);
    if (id === "data") return n / align / rate;
    p += 8 + n + (n & 1);
  }
  throw new Error(`${f}: no data chunk`);
}

async function main() {
  if (!existsSync(SONG)) throw new Error(`build the renderer first: cargo build --release -p song (${SONG})`);
  const bank = existsSync(BANK) ? BANK : undefined;
  if (!bank) console.warn(`no GM bank at ${BANK}: songs with GM.DLS instruments render with the fallback bank`);
  mkdirSync(join(OUT, "audio"), { recursive: true });
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
    const hash = createHash("sha256").update(bytes).update(bank ? "bank" : "nobank").digest("hex");
    const e: Entry = {
      slug, file: rel, group: groupOf(rel, m.url), title: m.title ?? basename(rel, ".song.json"), author: m.author ?? "",
      description: m.description ?? "", url: m.url, copyright: m.copyright, seconds: 0, audio: `audio/${slug}.m4a`, bytes: 0,
    };
    entries.push(e);
    const c = cache[slug];
    if (c && c.hash === hash && existsSync(join(OUT, e.audio))) e.seconds = c.seconds;
    else todo.push({ e, hash });
  }
  // drop audio of songs that are gone
  if (!only) {
    for (const a of readdirSync(join(OUT, "audio"))) if (!seen.has(a.replace(/\.m4a$/, ""))) rmSync(join(OUT, "audio", a));
    for (const k of Object.keys(cache)) if (!seen.has(k)) delete cache[k];
  }

  console.log(`${entries.length} songs, ${todo.length} to render (${JOBS} jobs)`);
  const tmp = join(tmpdir(), `song-page-${process.pid}`);
  mkdirSync(tmp, { recursive: true });
  let done = 0;
  const failed: string[] = [];
  const worker = async () => {
    for (let t = todo.shift(); t; t = todo.shift()) {
      const { e, hash } = t;
      const wav = join(tmp, `${e.slug}.wav`);
      try {
        await run([SONG, "render", join(MUSIC, e.file), "-o", wav, "--threads", "2", ...(bank ? ["--bank", bank] : [])]);
        e.seconds = wavSeconds(wav);
        await run(["ffmpeg", "-y", "-loglevel", "error", "-i", wav, "-c:a", "aac_at", "-b:a", "128k", "-movflags", "+faststart", join(OUT, e.audio)]);
        cache[e.slug] = { hash, seconds: e.seconds };
        writeFileSync(cacheFile, JSON.stringify(cache, null, 1));
      } catch (err) {
        failed.push(`${e.file}: ${(err as Error).message}`);
      } finally {
        rmSync(wav, { force: true });
      }
      console.log(`[${++done}] ${e.file} ${Math.round(e.seconds)} s`);
    }
  };
  await Promise.all(Array.from({ length: JOBS }, worker));
  rmSync(tmp, { recursive: true, force: true });
  if (failed.length) {
    console.error(`${failed.length} failed:\n${failed.join("\n")}`);
    process.exit(1);
  }
  for (const e of entries) e.bytes = statSync(join(OUT, e.audio)).size;
  writeFileSync(join(OUT, "songs.json"), JSON.stringify(entries));
  writeFileSync(join(OUT, "index.html"), page(entries));
  const hours = entries.reduce((a, e) => a + e.seconds, 0) / 3600;
  const mb = entries.reduce((a, e) => a + e.bytes, 0) / 1e6;
  console.log(`wrote ${join(OUT, "index.html")}: ${entries.length} songs, ${hours.toFixed(1)} h, ${mb.toFixed(0)} MB audio`);
}

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

export function page(entries: Entry[]): string {
  const contact = process.env.CONTACT;
  const counts = GROUPS.map((g) => [g, entries.filter((e) => e.group === g).length] as const).filter(([, n]) => n);
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
audio { width: 100%; display: block; margin-top: .25rem; }
#search { width: 100%; max-width: 24rem; padding: .4rem .6rem; font: inherit; border: 1px solid var(--line); border-radius: 4px; background: var(--bg); color: var(--fg); margin: 1rem 0 0; }
ol { list-style: none; margin: 0; padding: 0; }
li { border-bottom: 1px solid var(--line); }
li, h2 { scroll-margin-top: 7rem; }
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
<audio id="audio" controls preload="none"></audio>
</div></div>
<header>
<h1>Converted songs</h1>
<p class="note">${entries.length} songs converted to the 64k project's own synthesizer and rendered by it. Each is an adapted version,
not the original recording, and sounds close to but not identical with it. No licence is stated for the released songs:
the music belongs to its authors. They are published here while the authors are asked for permission, and a song is
removed at any author's request.${contact ? ` To ask, contact ${esc(contact)}.` : ""}</p>
<p class="note">${counts.map(([g, n]) => `<a href="#g-${slugOf(g)}">${esc(g)}</a> (${n})`).join(" · ")}</p>
<label for="search" class="note">Search title, author or text</label><br>
<input id="search" type="search" autocomplete="off">
</header>
<main id="list"></main>
<footer>Space plays or pauses; ← → seek 10 s; N / P next / previous song. Link to a song: copy its "link".</footer>
<script>
const SONGS = ${JSON.stringify(entries.map(({ slug, group, title, author, description, url, seconds, audio }) => ({ slug, group, title, author, description, url, seconds, audio }))).replace(/</g, "\\u003c")};
const GROUPS = ${JSON.stringify(GROUPS)};
const $ = (s) => document.querySelector(s);
const audio = $("#audio"), list = $("#list"), now = $("#now");
const fmt = (s) => Math.floor(s / 60) + ":" + String(Math.floor(s % 60)).padStart(2, "0");
const gid = (g) => "g-" + g.replace(/[^A-Za-z0-9]+/g, "-").replace(/^-|-$/g, "").toLowerCase();
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
let cur = -1, shown = SONGS.map((_, i) => i);
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
        '<button class="play" aria-label="Play ' + esc(s.title) + '">' + (i === cur && !audio.paused ? "❚❚" : "▶") + "</button>" +
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
function play(i, at) {
  if (i < 0 || i >= SONGS.length) return;
  if (i === cur) { audio.paused ? audio.play() : audio.pause(); return; }
  cur = i;
  const s = SONGS[i];
  audio.src = s.audio;
  if (at) audio.addEventListener("loadedmetadata", () => { audio.currentTime = at; }, { once: true });
  audio.play();
  now.innerHTML = esc(s.title) + " <span>" + esc(s.author) + "</span>";
  document.title = s.title + " · converted songs";
  history.replaceState(null, "", "#" + s.slug);
  mark();
}
function mark() {
  for (const li of list.querySelectorAll("li")) {
    const on = +li.dataset.i === cur;
    li.classList.toggle("on", on);
    li.querySelector("button.play").textContent = on && !audio.paused ? "❚❚" : "▶";
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
audio.addEventListener("play", mark);
audio.addEventListener("pause", mark);
audio.addEventListener("ended", () => step(1));
$("#search").addEventListener("input", (ev) => render(ev.target.value));
document.addEventListener("keydown", (ev) => {
  const el = ev.target instanceof Element ? ev.target : null;
  if (el?.closest("input, textarea, audio") || ev.ctrlKey || ev.metaKey || ev.altKey) return;
  // Space on a focused button, link or summary activates it natively
  if (ev.key === " " && el?.closest("button, a, summary")) return;
  if (ev.key === " ") { ev.preventDefault(); cur < 0 ? play(shown[0]) : (audio.paused ? audio.play() : audio.pause()); }
  else if (ev.key === "ArrowRight") audio.currentTime = Math.min(audio.duration || 0, audio.currentTime + 10);
  else if (ev.key === "ArrowLeft") audio.currentTime = Math.max(0, audio.currentTime - 10);
  else if (ev.key === "n" || ev.key === "N") step(1);
  else if (ev.key === "p" || ev.key === "P") step(-1);
});
render("");
// #slug selects that song (and #slug@seconds starts there); browsers block autoplay, so it waits for a click
const [h, t] = decodeURIComponent(location.hash.slice(1)).split("@");
const j = SONGS.findIndex((s) => s.slug === h);
if (j >= 0) {
  cur = j; audio.src = SONGS[j].audio;
  if (t) audio.addEventListener("loadedmetadata", () => { audio.currentTime = +t; }, { once: true });
  now.innerHTML = esc(SONGS[j].title) + " <span>" + esc(SONGS[j].author) + " · press play</span>";
  mark();
  document.getElementById(h)?.scrollIntoView({ block: "center" });
}
</script>
</body>
</html>
`;
}

if (import.meta.main) await main();
