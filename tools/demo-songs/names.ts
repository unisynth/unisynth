// Refreshes the track names in tools/demo-songs/meta/<name>.json from the original WaveSabre projects: the converter
// test refs (`Tests/WaveSabreConvertTests/Refs/<original>.ref`, JSON with `Tracks[].Name` in blob track order, master
// last) hold the names the composer gave the tracks. Descriptions (`about`) already in the file are kept by index.
//   WAVESABRE=<WaveSabre checkout> bun tools/demo-songs/names.ts [name ...]
// Repeated names get " 2", " 3" (the DAW addresses sends by track name).
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { info, type Manifest } from "./build";

const here = new URL("./", import.meta.url).pathname;

/** Track names of a WaveSabre ref without the master, repeats numbered. */
export function refNames(ref: { Tracks: { Name: string }[] }): string[] {
  const seen = new Map<string, number>();
  return ref.Tracks.slice(0, -1).map((t) => {
    const name = t.Name.trim() || "Track";
    const k = (seen.get(name) ?? 0) + 1;
    seen.set(name, k);
    return k === 1 ? name : `${name} ${k}`;
  });
}

if (import.meta.main) {
  const ws = process.env.WAVESABRE;
  if (!ws) {
    console.error("set WAVESABRE to a WaveSabre checkout");
    process.exit(2);
  }
  const manifest: Manifest = JSON.parse(readFileSync(join(here, "songs.json"), "utf8"));
  const only = process.argv.slice(2);
  mkdirSync(join(here, "meta"), { recursive: true });
  for (const e of manifest.songs.filter((e) => e.source === "wavesabre" && (!only.length || only.includes(e.name)))) {
    const ref = join(ws, "Tests/WaveSabreConvertTests/Refs", `${e.original}.ref`);
    if (!existsSync(ref)) {
      console.log(`skip  ${e.name}: no ${ref}`);
      continue;
    }
    const names = refNames(JSON.parse(readFileSync(ref, "utf8")));
    const cur = info(e.name);
    const tracks = names.map((name, i) => ({ name, ...(cur.tracks?.[i]?.about ? { about: cur.tracks[i].about } : {}) }));
    writeFileSync(join(here, "meta", `${e.name}.json`), JSON.stringify({ ...cur, tracks }, null, 2) + "\n");
    console.log(`ok    ${e.name}: ${names.length} track names`);
  }
}
