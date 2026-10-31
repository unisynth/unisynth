// Compact vs layered conversion of the three TBL songs (change compact-cauldron-xm-import): each song converted by
// `synth-convert xm` by default (layers dropped, pitch tracks packed, their echo and room as a stereo delay and
// reverb), with `--dry` (no effects for the dropped layers) and with `--keep-layers --split-pitches` (the parity
// layout), rendered by `unisynth`, judged against the `openmpt123` reference with the parity metrics; tracks,
// song bytes, xz -9e bytes and render time per song. Writes `cauldron-compact.md` (metrics only); the songs, WAVs,
// MP3s and a listening page (`index.html`: reference, layered, compact) go to the git-ignored
// `research/tbl-mini-cauldron/out/compact/`.
//
//   bun tools/unisynth-parity/cauldron-compact.ts [--report path] [song…]
// Exit 0 (a measurement, not a gate); skips when the reference material is absent.
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { readWav } from "../synth-parity/metrics.ts";
import { dropLead, judgeSong, leadFrames, OUT, PARITY_OPTIONS, SONGS } from "./cauldron.ts";
import { PROFILE } from "./parity.ts";

const ROOT = resolve(import.meta.dir, "../..");
const RESEARCH = join(ROOT, "research/tbl-mini-cauldron");
const MODES: Record<string, string[]> = { layered: PARITY_OPTIONS, dry: ["--dry"], compact: [] };

function sh(cmd: string[]): { ok: boolean; out: string; err: string } {
  const p = Bun.spawnSync(cmd, { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
  return { ok: p.exitCode === 0, out: p.stdout.toString(), err: p.stderr.toString().trim() };
}

interface Row {
  song: string;
  mode: string;
  tracks: string;
  layers: string;
  bytes: number;
  xz: number;
  renderS: number;
  loudness: number;
  worst: string;
  corr?: number;
}

function run(song: string, mode: string, dir: string): Row {
  const s = SONGS[song];
  const base = join(dir, `${song}.${mode}`);
  const c = sh([join(ROOT, "target", PROFILE, "synth-convert"), "xm", join(OUT, "listen", s.xm), join(RESEARCH, "ref", s.patches), `${base}.song`, ...MODES[mode]]);
  if (!c.ok) throw new Error(`${song} ${mode}: ${c.err.split("\n").pop()}`);
  const t0 = performance.now();
  const r = sh([join(ROOT, "target", PROFILE, "examples", "render"), `${base}.song`, `${base}.wav`]);
  const renderS = (performance.now() - t0) / 1000;
  if (!r.ok) throw new Error(`${song} ${mode} render: ${r.err.slice(-300)}`);
  sh(["ffmpeg", "-y", "-loglevel", "error", "-i", `${base}.wav`, "-b:a", "192k", `${base}.mp3`]);
  const xz = sh(["sh", "-c", `xz -9e -c '${base}.song' | wc -c`]);
  const rep = judgeSong(dropLead(readWav(`${base}.wav`), leadFrames(c.err)), readWav(join(OUT, "listen/ref44", `${song}.wav`)));
  const judged = rep.bands.filter((b) => Number.isFinite(b.diff));
  const worst = judged.reduce((m, b) => (Math.abs(b.diff) > Math.abs(m.diff) ? b : m), judged[0]);
  return {
    song,
    mode,
    tracks: /tracks (.*)/.exec(c.err)?.[1] ?? "",
    layers: /layers (\d+) dropped/.exec(c.err)?.[1] ?? "0",
    bytes: statSync(`${base}.song`).size,
    xz: Number(xz.out.trim()),
    renderS,
    loudness: rep.loudness.diff,
    worst: `${worst.diff.toFixed(2)} @ ${worst.fc}`,
    corr: rep.onsets.corr,
  };
}

function page(rows: Row[]): string {
  const songs = [...new Set(rows.map((r) => r.song))];
  const player = (label: string, src: string) => `<p>${label}<br><audio controls preload="none" src="${src}"></audio></p>`;
  return `<!doctype html><meta charset="utf-8"><title>TBL cauldron: compact vs layered</title>
<style>body{font:15px system-ui,sans-serif;max-width:52rem;margin:2rem auto;padding:0 1rem}audio{width:100%}h2{margin-top:2rem}</style>
<h1>TBL cauldron conversion: compact vs layered</h1>
<p>Reference: openmpt123 playing the rebuilt XM. Layered: <code>synth-convert xm --keep-layers --split-pitches --exact-chain</code> (the parity layout). Dry: <code>--dry</code> (stereo/echo layers dropped, pitch tracks packed). Compact: the default (as dry, plus a stereo delay from the dropped echo taps and a small reverb on the layered tracks).</p>
${songs
  .map((s) => {
    const r = (m: string) => rows.find((x) => x.song === s && x.mode === m)!;
    return `<h2>${s}</h2>
${player("Reference (openmpt123)", `../listen/${s}.mp3`)}
${player(`Layered: ${r("layered").tracks}, ${r("layered").xz} B xz`, `${s}.layered.mp3`)}
${player(`Compact, dry (--dry): ${r("dry").tracks}, ${r("dry").xz} B xz, ${r("dry").layers} layers dropped`, `${s}.dry.mp3`)}
${player(`Compact (default: stereo delay + reverb for the dropped layers): ${r("compact").tracks}, ${r("compact").xz} B xz`, `${s}.compact.mp3`)}`;
  })
  .join("\n")}
`;
}

function main() {
  const a = Bun.argv.slice(2);
  const opt = (k: string) => (a.includes(k) ? a.splice(a.indexOf(k), 2)[1] : undefined);
  const reportPath = opt("--report") ?? join(import.meta.dir, "cauldron-compact.md");
  if (!existsSync(join(OUT, "listen/ref44"))) {
    console.log(`skip: mini-cauldron reference material absent (${OUT}/listen/ref44)`);
    process.exit(0);
  }
  const b = sh(["cargo", "build", "-q", "--profile", PROFILE, "-p", "synth-convert", "--bin", "synth-convert"]);
  const e = sh(["cargo", "build", "-q", "--profile", PROFILE, "-p", "unisynth", "--features", "std", "--example", "render"]);
  if (!b.ok || !e.ok) throw new Error(`cargo build failed:\n${b.err}\n${e.err}`);
  const dir = join(OUT, "compact");
  mkdirSync(dir, { recursive: true });
  const songs = Object.keys(SONGS).filter((n) => !a.length || a.includes(n));
  const rows: Row[] = [];
  for (const s of songs)
    for (const m of Object.keys(MODES)) {
      const r = run(s, m, dir);
      console.log(`${s} ${m}: ${r.tracks}, ${r.bytes} B, xz ${r.xz} B, render ${r.renderS.toFixed(1)} s, loudness ${r.loudness.toFixed(2)} dB, worst band ${r.worst}`);
      rows.push(r);
    }
  writeFileSync(join(dir, "index.html"), page(rows));
  const f = (x: number, d = 2) => x.toFixed(d);
  const L = [
    "# Mini-cauldron: compact vs layered conversion (change compact-cauldron-xm-import)",
    "",
    `Generated ${new Date().toISOString().slice(0, 10)} by \`bun tools/unisynth-parity/cauldron-compact.ts\`. Layered: \`synth-convert xm --keep-layers --split-pitches --exact-chain\` (the parity layout); dry: \`--dry\`; compact: the default (dry plus the stereo delay and reverb). Both rendered by \`unisynth\` (\`--profile ${PROFILE}\`) and judged against the \`openmpt123\` reference with the parity metrics (loudness, octave bands, onset alignment). Audio and the listening page stay in \`research/tbl-mini-cauldron/out/compact/\`.`,
    "",
    "| song | mode | tracks | layers dropped | song bytes | xz -9e bytes | render s | loudness Δ dB | worst band Δ dB | alignment corr |",
    "|---|---|---|---:|---:|---:|---:|---:|---:|---:|",
    ...rows.map((r) => `| ${r.song} | ${r.mode} | ${r.tracks} | ${r.layers} | ${r.bytes} | ${r.xz} | ${f(r.renderS, 1)} | ${f(r.loudness)} | ${r.worst} | ${r.corr === undefined ? "" : f(r.corr)} |`),
    "",
  ];
  if (!a.length) writeFileSync(reportPath, L.join("\n"));
  else console.log(L.join("\n"));
}

if (import.meta.main) main();
