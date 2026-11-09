// Tunefish 3 parity (change add-tf3-import, task 5.1): synthetic one-instrument projects (one per module, mode and
// profile) and Transplant, rendered by the original player (tools/tf3-parity/render-ref, player build) and, converted
// by `synth-convert e3prj`, by `unisynth`; compared with the song settings of `parity.ts` (1 dB loudness, 3 dB per
// octave band 63 Hz–16 kHz with the 24 dB floor, onset-strength alignment within one 384-frame block). Converted
// Transplant is also checked against the YouTube capture of the released intro (tools/tf3-parity/capture.ts). Writes
// `tf3-report.md` (metrics, never audio).
//
//   bun tools/unisynth-parity/tf3.ts [--report path] [--reuse] [item…]
//     TF3_REF (default /tmp/tf3-ref) with build/render-ref-player; TF3_CAPTURES for the capture
// Exit 0 when every item that ran passed (or the checkout is absent and everything skipped), 1 on a failing item.
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { compare, DEFAULT_TOL, readWav, type Audio, type Report } from "../synth-parity/metrics.ts";
import { capturesDir, compareCapture, formatCapture, type CaptureReport } from "../tf3-parity/capture.ts";
import { NAMES } from "../tf3-parity/e3prj.ts";
import { refDir, renderer, renderRef } from "../tf3-parity/render-ref.ts";
import { BAND_FLOOR_DB, MIN_ALIGNMENT, PROFILE } from "./parity.ts";

const ROOT = resolve(import.meta.dir, "../..");
export const ONSET_FRAMES = 384;

export interface Tf3Row {
  name: string;
  accepted?: string;
  error?: string;
  report?: Report;
  secs?: number;
  warnings?: string[];
}

export interface Tf3Run {
  rows: Tf3Row[];
  capture?: CaptureReport | { skipped: string };
  skipped?: string;
  generated: string;
}

export const passes = (r: Tf3Row) => !r.error && !!r.report?.pass;

// ---------- synthetic projects ----------

type Values = Record<string, number>;

/** A playable base: a 2-point spline cycle, ADSR 1 on volume, 4 voices, centred, unity gain. */
const BASE: Values = {
  OscPoints: 1 / 3, OscP1Off: 0.25, OscP1Val: 1, OscP1Int: 1, OscP2Off: 0.66, OscP2Val: 0, OscP2Int: 1, OscP3Off: 0.6, OscP4Off: 0.8,
  OscP4Val: 0.2, OscFiInt: 1, OscVol: 0.5, OscPan: 0.5, OscPoly: 0.2, OscOctave: 0.5, AddOctave: 0.5, NoiseBw: 1, GainAmount: 0.5,
  Env1Att: 0, Env1Dec: 0.5, Env1Sus: 0.5, Env1Rel: 0.3, Env1Slp: 0.5, Env2Dec: 0.5, Env2Sus: 0.5, Env2Rel: 0.3, Env2Slp: 0.5,
  Mm1Source: 3 / 9, Mm1Mod: 0.55, Mm1Target: 9 / 25, Mm2Mod: 0.55, Mm3Mod: 0.55, Mm4Mod: 0.55,
};

/** `n` notes `gap` rows apart over a two-octave set. */
const many = (n: number, gap: number): [number, number][] => Array.from({ length: n }, (_, k) => [k * gap, [48, 52, 55, 60, 64, 67, 72, 43][k % 8]]);

/**
 * Accepted wider tolerances for PAD items: the table's phases and each note's start phase are random in Tunefish 3 too
 * (it seeds them from the tick count), so its level (peak normalisation of a random-phase table) and the slow beating a
 * note samples vary between runs. Measured (task 5.1): the original against itself with other tick seeds differs by up
 * to 1.88 dB loudness (pad-gauss) and 3.65 dB in one octave band (pad-detune, 125 Hz).
 */
const PAD_LEVEL = { loudnessDb: 2, bandDb: 4.5, why: "random PAD and note phases: the original differs from itself by up to 1.9 dB loudness and 3.7 dB per band" };

const route = (slot: number, src: number, dst: number, amount = 0.55): Values => ({
  [`Mm${slot}Source`]: src / 9, [`Mm${slot}Mod`]: amount, [`Mm${slot}Target`]: dst / 25,
});
const PAD = (profile: number): Values => ({
  OscVol: 0, AddVolume: 0.4, AddProfile: profile / 3, AddBandw: 0.3, AddDamp: 0.5, AddHarmncs: 0.2, AddScale: 0.1, Env1Sus: 0.8, Env1Rel: 0.2,
});
const LONG = { notes: many(16, 14), hold: 12, accept: PAD_LEVEL };

/** The synthetic items: name → instrument values over `BASE`, and the notes `(row, MIDI note)` (released 2 rows later). */
/**
 * The synthetic items: name → instrument values over `BASE`, the notes `(row, MIDI note)` released `hold` rows later
 * (default 2), and an accepted wider loudness tolerance. Unison and the sub-oscillator start their layers at random
 * phases in both engines, so they play 32 notes; PAD items hold 16 long notes (a short note plays a random part of
 * the table's slow beating).
 */
export const ITEMS: Record<string, { v: Values; notes?: [number, number][]; hold?: number; accept?: { loudnessDb: number; bandDb: number; why: string } }> = {
  "points-step": { v: { OscPoints: 0, OscP1Off: 0.5, OscP1Val: 1, OscP1Int: 0, OscFiInt: 0 } },
  "points-linear": { v: { OscP1Int: 0.5, OscP2Int: 0.5, OscFiInt: 0.5 } },
  "points-spline": { v: {} },
  "points-4-mixed": { v: { OscPoints: 1, OscP1Off: 0.06, OscP2Off: 0.64, OscP2Val: 0.06, OscP3Off: 0.79, OscP3Val: 0.98, OscP1Int: 0, OscP2Int: 0.5, OscP3Int: 1, OscP4Int: 1, OscFiInt: 0.5 } },
  "unison-spread": { v: { OscUnisn: 4 / 9, OscDetun: 0.3, OscSpread: 0.4 }, notes: many(32, 3) },
  "sub-oscillator": { v: { OscSubOsc: 1, OscUnisn: 2 / 9, OscDetun: 0.2 }, notes: many(32, 3) },
  "high-notes": { v: {}, notes: [[0, 84], [4, 91], [8, 96], [12, 100]] },
  "drive": { v: { OscDrive: 0.6 } },
  "octave-down": { v: { OscOctave: 0.75 } },
  "pad-single": { v: PAD(0), ...LONG },
  "pad-detune": { v: PAD(1), ...LONG },
  "pad-spread": { v: PAD(2), ...LONG },
  "pad-gauss": { v: PAD(3), ...LONG },
  "points-and-pad": { v: { ...PAD(3), OscVol: 0.4 }, ...LONG },
  "noise-band": { v: { OscVol: 0, NoiseOn: 0.5, NoiseFreq: 0.5, NoiseBw: 0.2 } },
  "lowpass-env": { v: { FltLPOn: 1, FltLPCut: 0.6, FltLPRes: 0.4, ...route(2, 4, 16) } },
  "highpass": { v: { FltHPOn: 1, FltHPCut: 0.4, FltHPRes: 0.3 } },
  "gate": { v: { Mm1Source: 0, Mm1Target: 0, ...route(2, 4, 16), FltLPOn: 1, FltLPCut: 0.7 } },
  "lfo-sine-cutoff": { v: { FltLPOn: 1, FltLPCut: 0.7, Lfo1Rate: 0.5, Lfo1Dep: 0.6, Lfo1Shp: 0, ...route(2, 1, 16) } },
  "lfo-ramp-volume": { v: { Lfo1Rate: 0.6, Lfo1Dep: 0.5, Lfo1Shp: 0.25, Lfo1Sync: 1, ...route(2, 1, 9) } },
  "lfo-square-pan": { v: { Lfo2Rate: 0.6, Lfo2Dep: 0.7, Lfo2Shp: 0.75, ...route(2, 2, 11) } },
  "matrix-points": { v: { Lfo1Rate: 0.4, Lfo1Dep: 0.6, Lfo1Shp: 0, ...route(2, 1, 2), ...route(3, 1, 3) } },
  "fixed-kick": { v: { OscFreq: 0.4, OscP1Int: 1, Env2Dec: 0.4, Env2Sus: 0.1, ...route(2, 4, 10) }, notes: [[0, 36], [4, 36], [8, 36], [12, 36]] },
  "glide-mono": { v: { OscPoly: 0, OscGlide: 0.3 } },
  "fx-distortion": { v: { Effect1: 0.1, DistAmnt: 0.5 } },
  "fx-delay": { v: { Effect1: 0.2, DelayL: 0.25, DelayR: 0.3, DelayDc: 0.4 } },
  "fx-reverb": { v: { Effect1: 0.5, RevRoom: 0.6, RevDamp: 0.4, RevWet: 0.3, RevWidth: 0.8 } },
  "fx-formant": { v: { Effect1: 0.6, FrmtType: 0.5, FrmtWet: 0.7 } },
  "velocity": { v: {}, notes: [[0, 60], [4, 64], [8, 67], [12, 72]] },
};

/** An `.e3prj` text for one item (`velocity` varies the note velocities). */
export function itemProject(name: string): string {
  const it = ITEMS[name];
  const v = { ...BASE, ...it.v };
  const notes = it.notes ?? [[0, 48], [4, 55], [8, 60], [12, 67]];
  const hold = it.hold ?? 2;
  const rows = Math.max(...notes.map(([r]) => r)) + hold + 8;
  const cells = notes.flatMap(([r, n], k) => [
    `<ne i="0" n="${n + 1}" r="${r}" v="${name === "velocity" ? [127, 90, 60, 30][k] : -1}" e="0"/>`,
    `<ne i="-1" n="128" r="${r + hold}" v="-1" e="0"/>`,
  ]);
  const params = NAMES.map((n) => `<param value="${v[n] ?? 0}" name="${n}"/>`).join("");
  return `<?xml version="1.0"?><enigma version="3.11b"><songs><song bpm="140" id="1" name="${name}"><patterns>
<pattern rowcount="${rows}" patternid="0" trackcount="1"><tracks><track>${cells.join("")}</track></tracks></pattern></patterns>
<patterninstances><patterninstance rowoffset="0" patternid="0" track="0"/></patterninstances></song></songs>
<instruments><instrument id="0" name="${name}"><parameters>${params}</parameters></instrument></instruments></enigma>`;
}

// ---------- running ----------

const bin = (n: string) => join(ROOT, "target", PROFILE, n);
const ex = (n: string) => join(ROOT, "target", PROFILE, "examples", n);

function sh(cmd: string[]): { ok: boolean; err: string } {
  const p = Bun.spawnSync(cmd, { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
  return { ok: p.exitCode === 0, err: p.stderr.toString().trim() };
}

const cut = (a: Audio, n: number): Audio => ({ rate: a.rate, ch: a.ch.map((c) => c.subarray(0, n)) });

export function judge(ours: Audio, ref: Audio, accept?: { loudnessDb: number; bandDb: number }): Report {
  const n = Math.min(ours.ch[0].length, ref.ch[0].length);
  const tol = { ...DEFAULT_TOL, ...(accept ?? {}), onsetFrames: ONSET_FRAMES, bandFloorDb: BAND_FLOOR_DB, onsetAlignment: MIN_ALIGNMENT };
  return compare(cut(ours, n), cut(ref, n), tol);
}

/** Reference → convert → render → compare, for one project file. */
export function runProject(name: string, project: string, work: string, reuse = false): Tf3Row {
  const accept = ITEMS[name]?.accept;
  const row: Tf3Row = { name, accepted: accept ? `loudness ±${accept.loudnessDb} dB, bands ±${accept.bandDb} dB: ${accept.why}` : undefined };
  const refWav = join(work, `${name}_ref.wav`), conv = join(work, `${name}.song`), ours = join(work, `${name}_unisynth.wav`);
  try {
    if (!(reuse && existsSync(refWav))) renderRef(project, refWav, { tail: 2 });
    if (!(reuse && existsSync(ours))) {
      const c = sh([bin("synth-convert"), "e3prj", project, conv, "2"]);
      if (!c.ok) return { ...row, error: `convert: ${c.err.split("\n").pop()}` };
      row.warnings = c.err.split("\n").filter((l) => /warning/i.test(l)).map((l) => l.replace(/^synth-convert: warning: /, ""));
      const u = sh([ex("render"), conv, ours]);
      if (!u.ok) return { ...row, error: `render: ${u.err.slice(-300)}` };
    }
    const a = readWav(ours), r = readWav(refWav);
    row.secs = Math.min(a.ch[0].length, r.ch[0].length) / a.rate;
    row.report = judge(a, r, accept);
  } catch (e) {
    return { ...row, error: String((e as Error).message) };
  }
  return row;
}

export function run(names: string[], reuse: boolean): Tf3Run {
  const generated = new Date().toISOString().slice(0, 10);
  if (!existsSync(renderer(false))) {
    return { rows: [], generated, skipped: `no reference renderer at ${renderer(false)} (run tools/tf3-parity/build-ref.sh)` };
  }
  const work = join(ROOT, "build/tf3-parity/items");
  mkdirSync(work, { recursive: true });
  for (const c of [["cargo", "build", "-q", "--profile", PROFILE, "-p", "synth-convert", "--bin", "synth-convert"],
    ["cargo", "build", "-q", "--profile", PROFILE, "-p", "unisynth", "--features", "std", "--example", "render"]]) {
    const b = sh(c);
    if (!b.ok) throw new Error(`${c.join(" ")}: ${b.err}`);
  }
  const rows: Tf3Row[] = [];
  for (const name of names.filter((n) => n in ITEMS)) {
    const prj = join(work, `${name}.e3prj`);
    writeFileSync(prj, itemProject(name));
    rows.push(runProject(name, prj, work, reuse));
  }
  let capture: Tf3Run["capture"];
  const transplant = join(refDir(), "demos/transplant/transplant.e3prj");
  if (names.includes("transplant") && existsSync(transplant)) {
    const row = runProject("transplant", transplant, work, reuse);
    rows.push(row);
    const cap = join(capturesDir(), "transplant.wav");
    capture = !existsSync(cap) ? { skipped: `no capture ${cap} (tools/tf3-parity/captures.sh)` }
      : row.error ? { skipped: "the conversion failed" } : compareCapture(readWav(join(work, "transplant_unisynth.wav")), readWav(cap));
  }
  return { rows, capture, generated };
}

export function formatRun(run: Tf3Run): string {
  const f = (x: number) => (Number.isFinite(x) ? x.toFixed(2) : String(x));
  const lines = [
    "# Tunefish 3 parity (change add-tf3-import)",
    "",
    `Generated ${run.generated} by \`bun tools/unisynth-parity/tf3.ts\`. Reference: the original Tunefish 3 player built from the Enigma-Studio-3 checkout in its player configuration (\`tools/tf3-parity/build-ref.sh\`, player mode); ours: \`synth-convert e3prj\` (player mode) → \`unisynth\` (\`--profile ${PROFILE}\`). Metrics: loudness within 1 dB, octave bands 63 Hz–16 kHz within 3 dB (bands ${BAND_FLOOR_DB} dB under the loudest skipped), onset-strength alignment (lag within ${ONSET_FRAMES} frames, correlation ≥ ${MIN_ALIGNMENT}); renders cut to the shorter one. Projects, renders and captures stay outside the repository.`,
    "",
  ];
  if (run.skipped) return [...lines, `Skipped: ${run.skipped}.`, ""].join("\n");
  lines.push("| Item | Result | Loudness Δ dB | Worst judged band Δ dB | Alignment lag / corr | Seconds | Warnings |", "|---|---|---:|---:|---|---:|---|");
  for (const r of run.rows) {
    if (r.error || !r.report) {
      lines.push(`| ${r.name} | ERROR | | | | | ${(r.error ?? "").replace(/\|/g, "/")} |`);
      continue;
    }
    const rep = r.report;
    // the judged bands only: those within the floor of the loudest reference band
    const top = Math.max(...rep.bands.map((b) => b.ref));
    const worst = rep.bands
      .filter((b) => Number.isFinite(b.diff) && b.ref >= top - BAND_FLOOR_DB)
      .reduce((m, b) => (Math.abs(b.diff) > Math.abs(m.diff) ? b : m), { fc: 0, diff: 0 } as { fc: number; diff: number });
    lines.push(`| ${r.name} | ${rep.pass ? (r.accepted ? "pass (accepted)" : "pass") : "FAIL"} | ${f(rep.loudness.diff)} | ${f(worst.diff)} (${worst.fc} Hz) | ${rep.onsets.lag ?? "–"} / ${rep.onsets.corr === undefined ? "–" : f(rep.onsets.corr)} | ${f(r.secs ?? 0)} | ${(r.warnings ?? []).join("; ").replace(/\|/g, "/")} |`);
  }
  const failed = run.rows.filter((r) => !passes(r)).map((r) => r.name);
  lines.push("", `${run.rows.length - failed.length} of ${run.rows.length} items pass${failed.length ? `; failing: ${failed.join(", ")}` : ""}.`, "");
  const accepted = run.rows.filter((r) => r.accepted);
  if (accepted.length) lines.push("Accepted wider tolerances: " + accepted.map((r) => `${r.name} (${r.accepted})`).join("; ") + ".", "");
  if (run.capture) {
    lines.push("## Converted Transplant against the capture of the released intro", "");
    lines.push("skipped" in run.capture ? `Skipped: ${run.capture.skipped}.` : "```text\n" + formatCapture("transplant", run.capture) + "\n```");
    lines.push("");
  }
  lines.push(
    "## Known differences",
    "",
    "- The point cycle is per voice (Tunefish 3 shares one per instrument: a voice hears the cycle of the voice before it when their modulation differs). Measured effect on Transplant: none (no instrument modulates its points).",
    "- Drive, clip and the decimation stage run per slot (Tunefish 3 drives each sum, then filters the oscillator and PAD sum; its ladder saturates `y − y³/6`). Measured: a sub-oscillator at drive 0.6 differs by 0.59 dB loudness and at most 2.4 dB per band over 32 notes, as without drive (0.55 dB, 2.5 dB).",
    "- MOD1–4 (editor only) become per-instrument controllers; Tunefish 3 sets them per voice. Measured effect on Transplant: none (no effect words; player mode drops them).",
    "- LFO depths whose Tunefish 3 value goes below 0 (sine past depth 0.82, ramp down past 0.71) are clamped (a warning names them).",
    "- Slop, routes to detune or spread, and a PAD voice without a volume envelope (it drones in Tunefish 3) are not reproduced (warnings name them).",
    "- PAD phases are random in both engines: the table's magnitude spectrum matches, the waveform does not.",
    "",
  );
  return lines.join("\n");
}

if (import.meta.main) {
  const a = process.argv.slice(2);
  const opt = (k: string) => (a.includes(k) ? a.splice(a.indexOf(k), 2)[1] : undefined);
  const reportPath = opt("--report") ?? join(import.meta.dir, "tf3-report.md");
  const reuse = a.includes("--reuse") && a.splice(a.indexOf("--reuse"), 1).length > 0;
  const names = a.length ? a : [...Object.keys(ITEMS), "transplant"];
  const r = run(names, reuse);
  if (r.skipped) {
    console.log(`tf3 parity: skipped (${r.skipped})`);
    process.exit(0);
  }
  writeFileSync(reportPath, formatRun(r));
  for (const row of r.rows) console.log(`${passes(row) ? "pass" : "FAIL"}  ${row.name}${row.error ? `: ${row.error}` : ""}`);
  if (r.capture && !("skipped" in r.capture)) console.log(formatCapture("transplant", r.capture));
  const ok = r.rows.every(passes) && (!r.capture || "skipped" in r.capture || r.capture.pass);
  process.exit(ok ? 0 : 1);
}
