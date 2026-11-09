// Feature survey of Tunefish 3 material (task 1.2 of add-tf3-import): which modules, modes, profiles, matrix routes and
// effects Transplant, the default project and the 128 presets use, on the values the intro played (quantised, spec §5).
//   bun tools/tf3-parity/survey.ts [--write]     (--write replaces §9 of research/tunefish3-functional-spec.md)
//   TF3_REF  Enigma-Studio-3 checkout (default /tmp/tf3-ref); absent → skip, exit 0
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { fi, lengthRows, loadProject, NAMES, playable, quantise, readPreset, type Song } from "./e3prj.ts";

const ROOT = resolve(import.meta.dir, "../..");
const SPEC = join(ROOT, "research/tunefish3-functional-spec.md");

export const refDir = (): string => process.env.TF3_REF ?? "/tmp/tf3-ref";

const P = (name: string): number => NAMES.indexOf(name);
const MODES = ["step", "linear", "spline"];
const PROFILES = ["single", "detune", "spread", "gauss"];
const SHAPES = ["sine", "ramp up", "ramp down", "square", "random"];
const SOURCES = ["none", "LFO1", "LFO2", "ADSR1", "ADSR2", "MOD1", "MOD2", "MOD3", "MOD4", "wheel"];
const TARGETS = ["none", "P1 offset", "P1 value", "P2 offset", "P2 value", "P3 offset", "P3 value", "P4 offset",
  "P4 value", "volume", "frequency", "pan", "detune", "spread", "drive", "noise", "LP cutoff", "LP res", "HP cutoff",
  "HP res", "BP cutoff", "BP Q", "notch cutoff", "notch Q", "ADSR1 decay", "ADSR2 decay"];
const EFFECTS = ["none", "distortion", "delay", "chorus", "flanger", "reverb", "formant", "EQ", "gain", "filter", "reserved"];
const EDITOR_ONLY_FX = new Set([3, 4, 7]);

/** Features of one instrument's quantised values, as short tags (stable, sortable). */
export function features(raw: readonly number[]): string[] {
  const v = raw.map((p, i) => quantise(i, p));
  const t: string[] = [];
  const mm = Array.from({ length: 10 }, (_, i) => ({
    src: fi(9 * v[68 + 3 * i]), amt: (20 * (v[69 + 3 * i] - 0.5)) ** 2, dst: fi(25 * v[70 + 3 * i]) }));
  const routed = (dst: number) => mm.some((m) => m.dst === dst && m.src !== 0 && m.src !== 9);
  const used = (src: number) => mm.some((m) => m.src === src && m.dst !== 0);
  if (v[P("OscVol")] > 0.01) {
    const n = fi(3 * v[P("OscPoints")]) + 1;
    t.push(`osc: ${n} point${n > 1 ? "s" : ""}`);
    const modes = [...Array.from({ length: n }, (_, i) => fi(2 * v[P(`OscP${i + 1}Int`)])), fi(2 * v[P("OscFiInt")])];
    for (const m of new Set(modes)) t.push(`osc mode: ${MODES[m]}`);
    const uni = fi(10 * v[P("OscUnisn")]) + 1;
    if (uni > 1) t.push(`osc unison ${uni}`);
    if (fi(v[P("OscSubOsc")]) > 0) t.push("osc sub-oscillator");
    if (v[P("OscDrive")] > 0) t.push("osc drive");
    if (v[P("OscFreq")] >= 1e-5) t.push("osc fixed frequency");
    if (v[P("OscDetun")] > 0) t.push("osc detune");
    if (v[P("OscSpread")] > 0 && uni > 1) t.push("osc spread");
    if (fi(8 * v[P("OscOctave")]) !== 4) t.push(`osc octave ${4 - fi(8 * v[P("OscOctave")])}`);
    if ([1, 2, 3, 4, 5, 6, 7, 8].some(routed)) t.push("osc point modulation");
  }
  if (v[P("AddVolume")] > 0) {
    t.push(`pad: ${PROFILES[fi(3 * v[P("AddProfile")])]}`);
    if (v[P("AddDrive")] > 0) t.push("pad drive");
    if (!routed(9)) t.push("pad without volume route (drones after note-off)");
  }
  if (v[P("NoiseOn")] > 0.01) t.push(v[P("NoiseBw")] < 0.99 ? "noise band" : "noise");
  for (const [f, name] of [["LP", "LP"], ["HP", "HP"], ["BP", "BP (editor only)"], ["NT", "notch (editor only)"]])
    if (v[P(`Flt${f}On`)] > 0.5) t.push(`filter ${name}`);
  if (v[P("OscGlide")] > 0) t.push("glide");
  if (v[P("OscSlop")] > 0) t.push("slop");
  if (v[P("GainAmount")] !== 0.5) t.push("gain ≠ unity");
  for (const [i, s] of [[1, 3], [2, 4]] as const) if (used(s)) t.push(`ADSR${i}`);
  for (const [i, s] of [[1, 1], [2, 2]] as const)
    if (used(s)) {
      t.push(`LFO${i}: ${SHAPES[[0.2, 0.4, 0.6, 0.8].filter((x) => v[P(`Lfo${i}Shp`)] >= x).length]}`);
      if (v[P(`Lfo${i}Sync`)] > 0.5) t.push(`LFO${i} sync`);
    }
  for (const m of mm) {
    if (m.src === 0 || m.dst === 0) continue;
    t.push(`route ${SOURCES[m.src]} → ${TARGETS[m.dst]}${m.amt === 1 ? "" : ` ×${m.amt.toFixed(2)}`}`);
    if (m.src >= 5 && m.src <= 8) t.push("MOD source (editor only)");
    if (m.dst >= 20 && m.dst <= 23) t.push("BP/notch target (editor only)");
  }
  for (let i = 0; i < 10; i++) {
    const fx = fi(10 * v[98 + i]);
    if (fx === 0) continue;
    t.push(`fx ${EFFECTS[fx]}${EDITOR_ONLY_FX.has(fx) ? " (editor only)" : fx >= 8 ? " (silent)" : ""}`);
  }
  return t;
}

/** Song statistics from its events. */
export function songStats(song: Song): { rows: number; notes: number; offs: number; effects: number; noInstrument: number; instruments: number[] } {
  const byId = new Map(song.patterns.map((p) => [p.id, p]));
  let notes = 0, offs = 0, effects = 0, noInstrument = 0;
  const instruments = new Set<number>();
  for (const inst of song.instances) {
    const p = byId.get(inst.patternId);
    if (!p) throw new Error(`song ${song.id}: instance at row ${inst.rowOffset} names missing pattern ${inst.patternId}`);
    for (const track of p.tracks)
      for (const ev of track) {
        if (ev.note === 0) continue;
        if (ev.note === 128) { offs++; continue; }
        if (ev.instrument < 0) { noInstrument++; continue; }
        notes++;
        instruments.add(ev.instrument);
        if (ev.effect !== 0) effects++;
      }
  }
  return { rows: lengthRows(song), notes, offs, effects, noInstrument, instruments: [...instruments].sort((a, b) => a - b) };
}

function tally(lists: string[][]): Map<string, number> {
  const m = new Map<string, number>();
  for (const l of lists) for (const t of new Set(l)) m.set(t, (m.get(t) ?? 0) + 1);
  return new Map([...m].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])));
}

export function survey(ref: string): string | null {
  const projects = [join(ref, "demos/transplant/transplant.e3prj"), join(ref, "binary/default.e3prj")].filter(existsSync);
  const presetDir = join(ref, "binary/tf3programs");
  if (projects.length === 0) return null;
  const out: string[] = [];
  out.push("## 9. Feature survey", "", `Generated by \`bun tools/tf3-parity/survey.ts --write\` from the Enigma-Studio-3 checkout. Values are`,
    "quantised as the intro stored them (§5). Editor-only features are marked; the intro did not play them.", "");
  for (const path of projects) {
    const proj = loadProject(path);
    const name = basename(path);
    out.push(`### ${name}`, "", `Format version ${proj.version}; ${proj.instruments.length} instruments; ` +
      `${proj.songs.length} song entries, ${playable(proj).length} playable.`, "");
    const used = new Set<number>();
    for (const song of playable(proj)) {
      const s = songStats(song);
      s.instruments.forEach((i) => used.add(i));
      out.push(`- Song ${song.id} "${song.name}": ${song.bpm} bpm, ${s.rows} rows ` +
        `(${(s.rows * 15 / song.bpm).toFixed(1)} s), ${song.patterns.length} patterns, ${song.instances.length} instances, ` +
        `${s.notes} notes, ${s.offs} note-offs, ${s.effects} notes with an effect word (editor only), ` +
        `${s.noInstrument} notes without instrument (ignored); instruments played: ${s.instruments.join(", ") || "none"}.`);
    }
    out.push("");
    const unknown = new Set(proj.instruments.flatMap((i) => i.unknown));
    if (unknown.size) out.push(`Unknown parameter names (ignored): ${[...unknown].join(", ")}.`, "");
    out.push("| Instrument | Played | Features |", "|---|---|---|");
    for (const inst of proj.instruments)
      out.push(`| ${inst.id} ${inst.name.replace(/\|/g, "/")} | ${used.has(inst.id) ? "yes" : "no"} | ${features(inst.params).join("; ")} |`);
    out.push("");
    const playedFeatures = proj.instruments.filter((i) => used.has(i.id)).map((i) => features(i.params));
    const editorOnly = [...tally(playedFeatures).keys()].filter((t) => t.includes("editor only"));
    if (playable(proj).length)
      out.push(`Editor-only features in played instruments: ${editorOnly.length ? editorOnly.join("; ") : "none"}.`, "");
  }
  if (existsSync(presetDir)) {
    const files = readdirSync(presetDir).filter((f) => f.endsWith(".txt"));
    const presets = files.map((f) => readPreset(readFileSync(join(presetDir, f), "latin1")));
    const groups = tally(presets.map((p) => [p.params.join(",")]));
    const blank = Math.max(...groups.values());
    const counts = tally(presets.map((p) => features(p.params)));
    out.push(`### Presets (\`binary/tf3programs\`)`, "", `${files.length} files, ${groups.size} distinct; the largest group of ` +
      `identical files has ${blank} (init patches). Feature counts over all files:`, "", "| Feature | Presets |", "|---|---:|");
    for (const [t, n] of counts) out.push(`| ${t} | ${n} |`);
    const unknown = new Set(presets.flatMap((p) => p.unknown));
    if (unknown.size) out.push("", `Unknown parameter names (ignored): ${[...unknown].join(", ")}.`);
    out.push("");
  }
  return out.join("\n");
}

if (import.meta.main) {
  const ref = refDir();
  const text = existsSync(ref) ? survey(ref) : null;
  if (text === null) {
    console.log(`tf3-survey: skipped (no Enigma-Studio-3 checkout at ${ref}; set TF3_REF or run tools/tf3-parity/build-ref.sh)`);
    process.exit(0);
  }
  if (process.argv.includes("--write")) {
    const spec = readFileSync(SPEC, "utf8");
    const at = spec.indexOf("## 9. Feature survey");
    if (at < 0) throw new Error("spec has no §9");
    writeFileSync(SPEC, spec.slice(0, at) + text);
    console.log(`tf3-survey: wrote §9 of ${SPEC}`);
  } else console.log(text);
}
