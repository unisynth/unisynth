// Builds the demo songs: converts the reference songs with synth-convert and saves them as song/1 in project/music with
// authorship metadata (author, description, copyright, license, url) that says they are adapted works.
//   WS_SONG_DIR=<dir with elysian.bin ...> TF_SONGS=<dir with *.tfm> ES4_SONGS=<Enigma-Studio-4/demos/zoom> bun tools/demo-songs/build.ts [name ...]
// Songs whose source is missing are skipped (the originals are not in the repository). `meta/<name>.json` (optional)
// adds the song's `about` and the track names and descriptions (`tracks: [{name, about}]`, blob order, no master).
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export type Source = { dir_env: string; convert: "ws" | "tfm" | "tf4m"; copyright: string; license: string; repo: string; pipeline: string; differences: string };
export type Entry = { name: string; source: string; file: string; original: string; title: string; author: string; about: string };
export type Manifest = { sources: Record<string, Source>; songs: Entry[] };

const root = new URL("../../", import.meta.url).pathname;

/** The meta object `song import --meta` takes. The description always says the work is adapted and how. */
export function meta(m: Manifest, e: Entry) {
  const s = m.sources[e.source];
  const url = s.repo + encodeURIComponent(e.original).replace(/%2F/g, "/");
  return {
    title: e.title,
    author: e.author,
    description: `${e.about} This is an adapted version, not the original recording: ${s.pipeline} ${s.differences}`,
    copyright: s.copyright,
    license: s.license,
    url,
    adapted_from: `${e.original} (${url})`,
  };
}

/** Track names and descriptions of a song (`meta/<name>.json`), when there are any. */
export type Info = { about?: Record<string, unknown>; tracks?: { name: string; about?: Record<string, unknown> }[] };
export function info(name: string): Info {
  const p = join(root, "tools/demo-songs/meta", `${name}.json`);
  return existsSync(p) ? JSON.parse(readFileSync(p, "utf8")) : {};
}

/** The import config: authorship plus `about` and `tracks` from `meta/<name>.json`. */
export function config(m: Manifest, e: Entry) {
  const i = info(e.name);
  return { ...meta(m, e), ...(i.about ? { about: i.about } : {}), ...(i.tracks ? { tracks: i.tracks } : {}) };
}

function run(cmd: string[]): { ok: boolean; err: string } {
  const p = Bun.spawnSync(cmd, { cwd: root, stdout: "pipe", stderr: "pipe" });
  return { ok: p.exitCode === 0, err: p.stderr.toString().trim() };
}

if (import.meta.main) {
  const manifest: Manifest = JSON.parse(readFileSync(join(root, "tools/demo-songs/songs.json"), "utf8"));
  const only = process.argv.slice(2);
  const built = run(["cargo", "build", "-q", "--release", "-p", "synth-convert", "-p", "song"]);
  if (!built.ok) {
    console.error(built.err);
    process.exit(1);
  }
  const bin = (n: string) => join(root, "target/release", n);
  const tmp = join(process.env.TMPDIR ?? "/tmp", `demo-songs-${process.pid}`);
  mkdirSync(tmp, { recursive: true });
  let failed = 0;
  for (const e of manifest.songs.filter((e) => !only.length || only.includes(e.name))) {
    const s = manifest.sources[e.source];
    const dir = process.env[s.dir_env];
    const src = dir && join(dir, e.file);
    if (!src || !existsSync(src)) {
      console.log(`skip  ${e.name}: ${s.dir_env} does not hold ${e.file}`);
      continue;
    }
    const blob = join(tmp, `${e.name}.bin`);
    const metaFile = join(tmp, `${e.name}.meta.json`);
    const out = join(root, "project/music", `${e.name}.song.json`);
    writeFileSync(metaFile, JSON.stringify(config(manifest, e), null, 2));
    const steps = [
      [bin("synth-convert"), s.convert, src, blob],
      [bin("song"), "import", blob, "-o", out, "--meta", metaFile],
      [bin("song"), "check", out],
    ];
    let ok = true;
    for (const c of steps) {
      const r = run(c);
      if (r.err) console.log(`      ${e.name}: ${r.err.split("\n").join(`\n      ${e.name}: `)}`);
      if (!r.ok) {
        console.error(`FAIL  ${e.name}: ${c.slice(0, 2).map((x) => x.split("/").pop()).join(" ")}`);
        ok = false;
        break;
      }
    }
    if (ok) console.log(`ok    ${e.name} → project/music/${e.name}.song.json (${(readFileSync(out).length / 1024).toFixed(0)} KB)`);
    else failed++;
  }
  rmSync(tmp, { recursive: true, force: true });
  process.exit(failed ? 1 : 0);
}
