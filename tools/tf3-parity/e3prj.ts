// Tunefish 3 project (.e3prj, Enigma Studio 3 XML) reader for the opt-in TF3 tools (change add-tf3-import).
// Laws and names: research/tunefish3-functional-spec.md §5 and §6. Our own MIT code; the format is read, not copied.
import { readFileSync } from "node:fs";

/** Public parameter names in index order (spec §5); projects and presets key values by them. */
export const NAMES: readonly string[] = [
  "OscPoints",
  ...[1, 2, 3, 4].flatMap((i) => [`OscP${i}Off`, `OscP${i}Val`, `OscP${i}Int`]),
  "OscFiInt", "OscVol", "OscFreq", "OscPan", "OscDetun", "OscPoly", "OscUnisn", "OscSpread", "OscSubOsc", "OscGlide",
  "OscDrive", "OscOctave", "OscOffset", "OscSlop",
  "AddVolume", "AddProfile", "AddOctave", "AddBandw", "AddDamp", "AddHarmncs", "AddScale", "AddDrive",
  "NoiseOn", "NoiseFreq", "NoiseBw",
  "FltLPOn", "FltLPCut", "FltLPRes", "FltHPOn", "FltHPCut", "FltHPRes",
  "FltBPOn", "FltBPCut", "FltBPQ", "FltNTOn", "FltNTCut", "FltNTQ",
  ...[1, 2].flatMap((i) => [`Env${i}Att`, `Env${i}Dec`, `Env${i}Sus`, `Env${i}Rel`, `Env${i}Slp`]),
  ...[1, 2].flatMap((i) => [`Lfo${i}Rate`, `Lfo${i}Dep`, `Lfo${i}Shp`, `Lfo${i}Sync`]),
  ...Array.from({ length: 10 }, (_, i) =>
    i < 9 ? [`Mm${i + 1}Source`, `Mm${i + 1}Mod`, `Mm${i + 1}Target`] : ["Mm10Sourc", "Mm10Mod", "Mm10Targe"]).flat(),
  ...Array.from({ length: 10 }, (_, i) => `Effect${i + 1}`),
  "DistAmnt", "ChrsRate", "ChrsDep", "DelayL", "DelayR", "DelayDc", "RevRoom", "RevDamp", "RevWet", "RevWidth",
  "FlangLfo", "FlangFreq", "FlangAmp", "FlangWet", "GainAmount", "ChrsGain", "FrmtType", "FrmtWet",
  "EqLow", "EqMid", "EqHigh",
];
export const COUNT = 129;

/** Stored value range per parameter (spec §5, column R): byte = round(p·R), 0 → hundredths. */
export const RANGE: readonly number[] = (() => {
  const r = new Array<number>(COUNT).fill(0);
  const set = (name: string, v: number) => (r[NAMES.indexOf(name)] = v);
  set("OscPoints", 3);
  for (const i of [1, 2, 3, 4]) set(`OscP${i}Int`, 2);
  set("OscFiInt", 2); set("OscPoly", 15); set("OscUnisn", 9); set("OscSubOsc", 1); set("OscOctave", 8);
  set("AddProfile", 3); set("AddOctave", 8);
  for (const f of ["LP", "HP", "BP", "NT"]) set(`Flt${f}On`, 1);
  for (const i of [1, 2]) { set(`Lfo${i}Shp`, 4); set(`Lfo${i}Sync`, 1); }
  for (let i = 0; i < 10; i++) { r[68 + 3 * i] = 9; r[70 + 3 * i] = 25; r[98 + i] = 10; }
  set("FrmtType", 4);
  return r;
})();

/** x87 rounding (round half to even), the intro platform's float → int conversion (spec "fi"). */
export function fi(x: number): number {
  const f = Math.floor(x), d = x - f;
  return d > 0.5 ? f + 1 : d < 0.5 ? f : f % 2 === 0 ? f : f + 1;
}

/** The value the intro played: the stored byte read back (spec §5). */
export function quantise(index: number, p: number): number {
  const r = RANGE[index];
  return r === 0 ? Math.min(255, fi(Math.fround(p) * 100)) / 100 : Math.min(255, fi(Math.fround(p) * r)) / r;
}

// ---------- minimal XML ----------

export interface El { tag: string; attrs: Record<string, string>; kids: El[] }

/** Parses the element tree of the subset Enigma Studio writes (no CDATA, no comments needed). */
export function parseXml(text: string): El {
  const root: El = { tag: "#root", attrs: {}, kids: [] };
  const stack: El[] = [root];
  const re = /<(\/?)([A-Za-z_][\w.-]*)((?:\s+[\w.-]+\s*=\s*"[^"]*")*)\s*(\/?)>|<\?[^>]*\?>|<!--[\s\S]*?-->/g;
  for (let m: RegExpExecArray | null; (m = re.exec(text)); ) {
    if (!m[2]) continue; // declaration or comment
    const [, close, tag, rawAttrs, selfClose] = m;
    if (close) {
      const top = stack.pop();
      if (!top || top.tag !== tag) throw new Error(`xml: unexpected </${tag}> at ${m.index}`);
      continue;
    }
    const attrs: Record<string, string> = {};
    for (const a of rawAttrs.matchAll(/([\w.-]+)\s*=\s*"([^"]*)"/g)) attrs[a[1]] = unescape(a[2]);
    const el: El = { tag, attrs, kids: [] };
    stack[stack.length - 1].kids.push(el);
    if (!selfClose) stack.push(el);
  }
  if (stack.length !== 1) throw new Error(`xml: <${stack[stack.length - 1].tag}> not closed`);
  return root;
}

function unescape(s: string): string {
  return s.replace(/&(lt|gt|amp|quot|apos|#\d+);/g, (_, e: string) =>
    e === "lt" ? "<" : e === "gt" ? ">" : e === "amp" ? "&" : e === "quot" ? '"' : e === "apos" ? "'" :
      String.fromCharCode(Number(e.slice(1))));
}

export const kids = (el: El, tag: string): El[] => el.kids.filter((k) => k.tag === tag);
export const kid = (el: El, tag: string): El | undefined => el.kids.find((k) => k.tag === tag);

// ---------- project ----------

export interface NoteEvent { row: number; note: number; instrument: number; velocity: number; effect: number }
export interface Pattern { id: number; rows: number; tracks: NoteEvent[][] }
export interface Instance { rowOffset: number; track: number; patternId: number }
export interface Song { id: number; name: string; bpm: number; patterns: Pattern[]; instances: Instance[] }
export interface Instrument { id: number; name: string; params: number[]; unknown: string[] }
export interface Project { version: string; songs: Song[]; instruments: Instrument[] }

const int = (el: El, name: string): number => {
  const v = el.attrs[name];
  if (v === undefined || !/^-?\d+$/.test(v.trim())) throw new Error(`<${el.tag}> needs integer ${name}`);
  return Number(v);
};

/**
 * Reads a project. Instruments start from `defaults` (editor default program or zeros) and take every known name;
 * unknown names are listed, as the editor ignores them.
 */
export function readProject(text: string, defaults: readonly number[] = new Array(COUNT).fill(0)): Project {
  const doc = kid(parseXml(text), "enigma");
  if (!doc) throw new Error("not an Enigma Studio project (no <enigma>)");
  const songs: Song[] = [];
  for (const s of kids(kid(doc, "songs") ?? { tag: "", attrs: {}, kids: [] }, "song")) {
    const patterns = kids(kid(s, "patterns") ?? { tag: "", attrs: {}, kids: [] }, "pattern").map((p): Pattern => {
      const rows = int(p, "rowcount");
      const tracks = kids(kid(p, "tracks") ?? { tag: "", attrs: {}, kids: [] }, "track").map((t) =>
        kids(t, "ne").map((n): NoteEvent => {
          const ev = { row: int(n, "r"), note: int(n, "n"), instrument: int(n, "i"), velocity: int(n, "v"), effect: int(n, "e") };
          if (ev.row < 0 || ev.row >= rows) throw new Error(`pattern ${p.attrs.patternid}: event row ${ev.row} outside ${rows} rows`);
          return ev;
        }));
      if (tracks.length !== int(p, "trackcount")) throw new Error(`pattern ${p.attrs.patternid}: trackcount ${p.attrs.trackcount} but ${tracks.length} tracks`);
      return { id: int(p, "patternid"), rows, tracks };
    });
    const instances = kids(kid(s, "patterninstances") ?? { tag: "", attrs: {}, kids: [] }, "patterninstance").map(
      (i): Instance => ({ rowOffset: int(i, "rowoffset"), track: int(i, "track"), patternId: int(i, "patternid") }));
    songs.push({ id: int(s, "id"), name: s.attrs.name ?? "", bpm: int(s, "bpm"), patterns, instances });
  }
  const instruments: Instrument[] = [];
  for (const el of kids(kid(doc, "instruments") ?? { tag: "", attrs: {}, kids: [] }, "instrument")) {
    const params = [...defaults];
    const unknown: string[] = [];
    for (const p of kids(kid(el, "parameters") ?? { tag: "", attrs: {}, kids: [] }, "param")) {
      const i = NAMES.indexOf(p.attrs.name ?? "");
      const v = Number(p.attrs.value);
      if (i < 0) unknown.push(p.attrs.name ?? "?");
      else if (Number.isFinite(v)) params[i] = v;
    }
    instruments.push({ id: int(el, "id"), name: el.attrs.name ?? "", params, unknown });
  }
  return { version: doc.attrs.version ?? "", songs, instruments };
}

export const loadProject = (path: string, defaults?: readonly number[]): Project =>
  readProject(readFileSync(path, "latin1"), defaults);

/** A preset file (`String, Short` header, then `name;value` lines) as 129 values from `defaults`. */
export function readPreset(text: string, defaults: readonly number[] = new Array(COUNT).fill(0)): { params: number[]; unknown: string[] } {
  const params = [...defaults];
  const unknown: string[] = [];
  for (const line of text.split(/\r?\n/).slice(1)) {
    const [name, value] = line.split(";");
    if (!name?.trim()) continue;
    const i = NAMES.indexOf(name.trim());
    if (i < 0) unknown.push(name.trim());
    else if (Number.isFinite(Number(value))) params[i] = Number(value);
  }
  return { params, unknown };
}

/** The playable songs: those with patterns and instances (the `<songlist>` copies have none). */
export const playable = (p: Project): Song[] => p.songs.filter((s) => s.patterns.length > 0 && s.instances.length > 0);

/** Song length in rows: the end of the latest pattern instance (spec §6.2). */
export function lengthRows(song: Song): number {
  const rows = new Map(song.patterns.map((p) => [p.id, p.rows]));
  return Math.max(0, ...song.instances.map((i) => i.rowOffset + (rows.get(i.patternId) ?? 0)));
}
