// Per-song drift of every tracked song across the format switch (change unify-v2-algorithms, task 6.2 / 6.9):
// renders each song in project/music/** (and each patch in project/music/patches as a one-note phrase) with this
// tree's `song render`, and stores its drift profile (length, loudness, octave bands: drift.ts `profile`) instead of
// the audio, which would be ~15 GB for the whole set.
//
//   bun tools/unisynth-parity/songs-drift.ts --baseline <dir>    # store <dir>/<slug>.json per song
//   bun tools/unisynth-parity/songs-drift.ts --against <dir> [--report <md>]   # drift per song; LISTEN past D7.3
//   --json <file>: the rows as JSON ({ file, loudness, band, flagged, note }) for the quirk survey
//   JOBS=8 (parallel renders, --threads 2 each), GM_DLS=<gm.dls>, ONLY=<regex on the path>, SONG=<song binary>
// Not a gate (D7.3): flagged songs are listed for listening. A song that fails to render, or is missing from either
// side, is an error.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { readWav } from "../synth-parity/metrics.ts";
import { slugOf, songFiles } from "../song-page/build.ts";
import { driftOf, formatDrift, profile, type Drift, type Profile } from "./drift.ts";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));
const MUSIC = join(ROOT, "project/music");
const SONG = process.env.SONG ?? join(ROOT, "target/release/song");
const BANK = process.env.GM_DLS ?? join(process.env.HOME ?? "", "Downloads/64k-tools/synth-lab/ws/gm.dls");
const JOBS = Number(process.env.JOBS ?? 8);

export type Item = { slug: string; file: string; json: string };
export type Stored = { file: string; sha256: string; profile: Profile };

/** A song that plays `patch` (us-patch/2) for one note (C4, one bar at 120 BPM) and one bar of tail. */
export function patchSong(patch: { kind: number; values: Record<string, number>; samples?: string[]; speech?: string[] }): string {
  const dev: Record<string, unknown> = { kind: patch.kind, values: patch.values };
  if (patch.samples?.length) dev.samples = patch.samples;
  if (patch.speech?.length) dev.speech = patch.speech;
  return JSON.stringify({
    format: "song/2", rate: 44100, tempo: 120, signature: [4, 4], ticks_per_beat: 960, length: 7680,
    clips: [{ id: "c", length: 3840, notes: [[0, 3840, 60, 100]] }],
    tracks: [{ id: "p", name: "Patch", volume: 1, mute: false, receives: [], devices: [dev], placements: [{ clip: "c", start: 0, length: 3840, loop: false }], lanes: [] }],
    master: { id: "master", name: "Master", volume: 1, mute: false, receives: [{ from: "p", channel: "main", volume: 1 }], devices: [], placements: [], lanes: [] },
  });
}

/** The DAW's instrument templates (a new instrument, the new song's instrument), read from its source so the run
 * after the format switch plays the switched values. Fails when a template is no longer found there. */
export function templates(): Item[] {
  const src: [string, string, RegExp][] = [
    ["template-init-instrument", "packages/daw/src/app.js", /const initInstrument = \(\) => \(\{ kind: 64, preset: "init", values: (\{[^}]*\}) \}\);/],
    ["template-new-song", "packages/daw/src/main.ts", /devices: \[\{ kind: 64, preset: "init", values: (\{[^}]*\}) \}\]/],
  ];
  return src.map(([slug, file, re]) => {
    const m = re.exec(readFileSync(join(ROOT, file), "utf8"));
    if (!m) throw new Error(`${file}: the instrument template ${slug} is not found (update songs-drift.ts templates())`);
    return { slug, file, json: patchSong({ kind: 64, values: JSON.parse(m[1]) }) };
  });
}

/** Every tracked song and patch, and the DAW's instrument templates, each as song JSON text. */
export function items(only?: RegExp): Item[] {
  const out: Item[] = songFiles(MUSIC).map((f) => ({ slug: slugOf(relative(MUSIC, f)), file: relative(ROOT, f), json: readFileSync(f, "utf8") }));
  const pdir = join(MUSIC, "patches");
  if (existsSync(pdir)) {
    for (const f of new Bun.Glob("*.patch.json").scanSync(pdir)) {
      out.push({ slug: "patch-" + f.replace(/\.patch\.json$/, ""), file: relative(ROOT, join(pdir, f)), json: patchSong(JSON.parse(readFileSync(join(pdir, f), "utf8"))) });
    }
  }
  out.push(...templates());
  return out.filter((i) => !only || only.test(i.file)).sort((a, b) => a.slug.localeCompare(b.slug));
}

async function render(it: Item, tmp: string): Promise<Profile> {
  const src = join(tmp, `${it.slug}.song.json`), wav = join(tmp, `${it.slug}.wav`);
  writeFileSync(src, it.json);
  const p = Bun.spawn([SONG, "render", src, "-o", wav, "--threads", "2", ...(existsSync(BANK) ? ["--bank", BANK] : [])], { stdout: "ignore", stderr: "pipe" });
  const err = await new Response(p.stderr).text();
  try {
    if ((await p.exited) !== 0) throw new Error(`${it.file}: ${err.trim().split("\n").slice(-2).join(" | ")}`);
    return profile(readWav(wav));
  } finally {
    rmSync(src, { force: true });
    rmSync(wav, { force: true });
  }
}

async function main() {
  const arg = (k: string) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : undefined; };
  const base = arg("--baseline"), against = arg("--against"), report = arg("--report"), json = arg("--json");
  if (!base === !against) throw new Error("usage: songs-drift.ts --baseline <dir> | --against <dir> [--report <md>]");
  if (!existsSync(SONG)) throw new Error(`build the renderer first: cargo build --release -p song (${SONG})`);
  const list = items(process.env.ONLY ? new RegExp(process.env.ONLY) : undefined);
  const tmp = join(tmpdir(), `songs-drift-${process.pid}`);
  mkdirSync(tmp, { recursive: true });
  if (base) mkdirSync(base, { recursive: true });
  const rows: { it: Item; d?: Drift; note?: string }[] = [];
  const errors: string[] = [];
  const todo = [...list];
  let done = 0;
  const worker = async () => {
    for (let it = todo.shift(); it; it = todo.shift()) {
      try {
        const pr = await render(it, tmp);
        if (base) {
          const s: Stored = { file: it.file, sha256: createHash("sha256").update(it.json).digest("hex"), profile: pr };
          writeFileSync(join(base, `${it.slug}.json`), JSON.stringify(s));
        } else {
          const f = join(against!, `${it.slug}.json`);
          if (!existsSync(f)) errors.push(`${it.file}: no baseline (${f})`);
          else {
            const b: Stored = JSON.parse(readFileSync(f, "utf8"));
            const d = driftOf(pr, b.profile);
            const note = pr.frames !== b.profile.frames ? `length ${b.profile.frames} → ${pr.frames} frames` : undefined;
            rows.push({ it, d: note ? { ...d, flagged: true } : d, note });
          }
        }
      } catch (e) {
        errors.push((e as Error).message);
      }
      if (++done % 25 === 0 || done === list.length) console.log(`${done}/${list.length}`);
    }
  };
  await Promise.all(Array.from({ length: JOBS }, worker));
  rmSync(tmp, { recursive: true, force: true });
  if (against) {
    rows.sort((a, b) => a.it.file.localeCompare(b.it.file));
    const flagged = rows.filter((r) => r.d!.flagged);
    const lines = [
      `# Per-song drift (${new Date().toISOString().slice(0, 10)})`, "",
      `${rows.length} songs against \`${against}\`; ${flagged.length} past the D7.3 thresholds (listen), ${errors.length} errors.`, "",
      ...flagged.map((r) => `- ${formatDrift(r.it.file, r.d)}${r.note ? ` (${r.note})` : ""}`),
      ...(errors.length ? ["", "## Errors", "", ...errors.map((e) => `- ${e}`)] : []),
      "", "## All songs", "", ...rows.map((r) => `- ${formatDrift(r.it.file, r.d)}${r.note ? ` (${r.note})` : ""}`), "",
    ];
    if (report) writeFileSync(report, lines.join("\n"));
    if (json) writeFileSync(json, JSON.stringify(rows.map((r) => ({ file: r.it.file, loudness: r.d!.loudness, band: r.d!.band, flagged: r.d!.flagged, note: r.note }))));
    console.log(lines.slice(0, 4 + flagged.length).join("\n"));
  } else console.log(`${list.length - errors.length} profiles stored in ${base}`);
  if (errors.length) {
    console.error(errors.join("\n"));
    process.exit(1);
  }
}

if (import.meta.main) await main();
