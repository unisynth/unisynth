// Library state (change daw-solid-ts 4.2): every instrument source in one searchable tree (/api/library). Project
// presets load as us-patch/1; the GM bank sets a sampler wave; Tunefish and WaveSabre entries load once synth-convert
// has converted them (project/music/.library.json). Pure helpers + the signals ui/library/Library.tsx renders.
import { createSignal } from "solid-js";
import { createStore } from "solid-js/store";

export interface Source { id: string; label: string; note: string; lic: string }
export interface LibItem { name: string; tag: string; [k: string]: any }

export const SOURCES: Source[] = [
  { id: "project", label: "Project presets", note: "project/music/patches · us-patch/1", lic: "ours" },
  { id: "gm", label: "GM.DLS bank", note: "OS bank read at runtime (Win gm.dls / macOS gs_instruments.dls); never shipped in the intro", lic: "OS bank" },
  { id: "gsm", label: "GSM samples", note: "stored in the instruments of project/music songs · GSM 06.10, about 9 KB per second", lic: "from project songs: each song's meta gives its licence" },
  { id: "tunefish", label: "Tunefish 4 programs", note: "tf4programs/*.txt → synth-convert → unisynth instrument + insert effects", lic: "GPL-3 reference: converted and re-saved as an own patch" },
  { id: "wavesabre", label: "WaveSabre songs", note: "instruments inside the reference songs → synth-convert → unisynth instrument", lic: "WaveSabre reference songs (MIT)" },
  { id: "tests", label: "Test patches", note: "crates/wavesabre/tests, crates/tfsynth/tests (synthetic, cover every module)", lic: "ours" },
];
/** Entries listed per open group; more asks to refine the search. */
export const LIST_CAP = 300;

export function libItems(cat: any, src: string): LibItem[] {
  if (!cat) return [];
  if (src === "project") return (cat.project?.presets || []).map((p: any) => ({ name: p.name, tag: "patch", preset: p }));
  if (src === "gm") return (cat.gm?.instruments || []).map((g: any) => ({ name: g.name, tag: g.drums ? "kit" : `${String(g.prog).padStart(3, "0")}${g.bank ? "·b" + g.bank : ""}`, gm: g }));
  if (src === "gsm") return (cat.gsm?.samples || []).map((g: any) => ({ name: `${g.song} · ${g.trackName}${g.sample ? ` #${g.sample + 1}` : ""}`, tag: `${g.seconds.toFixed(2)} s`, gsm: g }));
  if (src === "tunefish") return (cat.tunefish?.programs || []).map((p: any) => ({ name: p.name, tag: "#" + p.n, conv: p.conv }));
  if (src === "wavesabre") return (cat.wavesabre?.songs || []).flatMap((s: any) => s.instruments.filter((d: any) => !d.dup).map((d: any, i: number) => ({ name: `${s.song} · ${d.kind} ${i + 1}`, tag: d.kind === "Adultery" ? `wave ${d.wave}` : d.kind.slice(0, 5).toLowerCase(), ws: d, song: s.song, conv: d.conv })));
  if (src === "tests") return [...(cat.repo?.["wavesabre-tests"] || []).map((n: string) => ({ name: n, tag: "ws" })), ...(cat.repo?.["tfsynth-tests"] || []).map((n: string) => ({ name: n, tag: "tf" }))];
  return [];
}

/** All sources while the catalogue loads; then the project presets and the sources the catalogue has. */
export const visibleSources = (cat: any) => SOURCES.filter((s) => !cat || s.id === "project" || cat[s.id === "tests" ? "repo" : s.id]);

export function filterItems(items: LibItem[], q: string): LibItem[] {
  const k = q.trim().toLowerCase();
  return k ? items.filter((i) => (i.name + " " + i.tag).toLowerCase().includes(k)) : items;
}

/** Open groups after a search (those with hits) or a request to show one source (`openLibrary`). */
export function openGroups(cat: any, q: string, want: string | null): Record<string, boolean> {
  const k = q.trim(), o: Record<string, boolean> = {};
  for (const s of visibleSources(cat)) o[s.id] = k ? filterItems(libItems(cat, s.id), k).length > 0 : s.id === want;
  return o;
}

/** What the inspector says about an entry; `hasInst`: the selected track has an instrument to load into. */
export function describe(src: Source, it: LibItem, hasInst: boolean) {
  const legacy = src.id === "tests" || ((src.id === "tunefish" || src.id === "wavesabre") && !it.conv?.devices?.length);
  const what = src.id === "gm" ? `${it.gm.drums ? "drum kit" : "melodic"} · bank ${it.gm.bank} · prog ${it.gm.prog} · ${it.gm.regions.length} regions` :
    src.id === "gsm" ? `${it.gsm.seconds.toFixed(2)} s · ${(it.gsm.bytes / 1024).toFixed(1)} KB · ${it.gsm.path}${it.gsm.uses > 1 ? ` · used ${it.gsm.uses}×` : ""}` :
    src.id === "wavesabre" ? `${it.ws.kind} in “${it.song}”${it.ws.kind === "Adultery" ? ` · plays GM wave ${it.ws.wave}` : ""}` : src.note;
  const n = it.conv?.devices?.length ?? 0, w: string[] = it.conv?.warnings ?? [];
  const conv = n ? `Converted by synth-convert: ${n > 1 ? `instrument + ${n - 1} insert effect${n > 2 ? "s" : ""}` : "instrument"}${w.length ? ` · ${w.length} note${w.length > 1 ? "s" : ""}: ${w.slice(0, 2).join("; ")}` : ""}` : "";
  return { what, legacy, canLoad: hasInst && !legacy, conv };
}

// ---- signals (module scope: a component hot reload keeps them) ----
export const [catalog, setCatalog] = createSignal<any>(null);
export const [query, setQuery] = createSignal("");
export const [open, setOpen] = createStore<Record<string, boolean>>({});
/** The inspected entry. */
export const [current, setCurrent] = createSignal<{ src: Source; it: LibItem } | null>(null);
