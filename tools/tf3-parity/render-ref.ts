// Renders a Tunefish 3 project with the original synth and player (reference, opt-in; change add-tf3-import).
//   bun tools/tf3-parity/render-ref.ts <in.e3prj> <out.wav> [--editor] [--song <id>] [--tail <s>]
//   TF3_REF  Enigma-Studio-3 checkout with build/render-ref-{player,editor} (tools/tf3-parity/build-ref.sh); absent → skip
// Player mode (default) quantises every parameter to the byte the intro stored (spec §5); --editor keeps the floats.
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { COUNT, loadProject, playable, quantise, type Project, type Song } from "./e3prj.ts";

export const refDir = (): string => process.env.TF3_REF ?? "/tmp/tf3-ref";
export const renderer = (editor: boolean): string => join(refDir(), `build/render-ref-${editor ? "editor" : "player"}`);

/** The harness's plain-text song (render-ref.cpp header). */
export function harnessText(proj: Project, song: Song, editor: boolean): string {
  const lines = [`bpm ${song.bpm}`];
  for (const inst of proj.instruments) {
    const v = inst.params.map((p, i) => (editor ? p : quantise(i, p)));
    if (v.length !== COUNT) throw new Error(`instrument ${inst.id}: ${v.length} values`);
    lines.push(`instrument ${inst.id} ${v.map((x) => Math.fround(x)).join(" ")}`);
  }
  for (const p of song.patterns) {
    lines.push(`pattern ${p.id} ${p.rows} ${p.tracks.length}`);
    p.tracks.forEach((t, j) => t.forEach((e) =>
      lines.push(`ev ${p.id} ${j} ${e.row} ${e.note} ${e.instrument} ${e.velocity} ${e.effect}`)));
  }
  for (const i of song.instances) lines.push(`instance ${i.rowOffset} ${i.track} ${i.patternId}`);
  return lines.join("\n") + "\n";
}

export function pickSong(proj: Project, id?: number): Song {
  const songs = playable(proj);
  const song = id === undefined ? songs[0] : songs.find((s) => s.id === id);
  if (!song) throw new Error(id === undefined ? "project has no playable song" : `no playable song with id ${id}`);
  return song;
}

/** Renders `project` to `wav`; returns false (skipped) when the reference renderer is absent. */
export function renderRef(project: string, wav: string, opts: { editor?: boolean; song?: number; tail?: number } = {}): boolean {
  const exe = renderer(!!opts.editor);
  if (!existsSync(exe)) return false;
  const proj = loadProject(project);
  const text = join(mkdtempSync(join(tmpdir(), "tf3-ref-")), "song.txt");
  writeFileSync(text, harnessText(proj, pickSong(proj, opts.song), !!opts.editor));
  execFileSync(exe, [text, wav, String(opts.tail ?? 4)], { stdio: "inherit" });
  return true;
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  const flag = (name: string) => { const i = args.indexOf(name); return i < 0 ? undefined : args.splice(i, 2)[1]; };
  const editor = args.includes("--editor");
  if (editor) args.splice(args.indexOf("--editor"), 1);
  const song = flag("--song"), tail = flag("--tail");
  if (args.length !== 2) {
    console.error("usage: render-ref.ts <in.e3prj> <out.wav> [--editor] [--song <id>] [--tail <s>]");
    process.exit(2);
  }
  const ok = renderRef(args[0], args[1], { editor, song: song === undefined ? undefined : Number(song), tail: tail === undefined ? undefined : Number(tail) });
  console.log(ok ? `tf3 render-ref: wrote ${args[1]}` : `tf3 render-ref: skipped (no ${renderer(editor)}; run tools/tf3-parity/build-ref.sh)`);
}
