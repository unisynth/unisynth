// Sampler sources (change daw-solid-ts 9.1, step 7): what a sampler slot plays, decoded GSM samples, library GSM
// entries, sample preview. Reads only; the commands that change samples are in ./device.ts.
import type { Device } from "../../model.ts";
import { choice, ip, oscType, slotUsed, vget } from "../model/params.ts";
import { b64ToBytes, gmInstrument } from "../model/bytes.ts";
import * as lib from "../state/library.ts";
import { api, E, songText } from "./session.ts";
import { say } from "./core.ts";

type GmInst = { name: string; drums?: boolean; bank: number; prog: number; regions: number[][] };

// Sampler slot (unisynth smp_source): `osc<i>.s7` picks the source — choice of 3: GSM sample of the device (0), GM.DLS
// instrument (0.5; a region per note: index = 1 + drums·16384 + bank·128 + program) or one GM.DLS wave (1.0; index =
// wave + 1) — and `osc<i>.sample` holds the raw index (0 = none). Both are structural (the engine rebuilds the device).
/** `?nobank`: act as if there were no GM bank. */
export const NOBANK = typeof location !== "undefined" && new URLSearchParams(location.search).has("nobank");
/** What a sampler slot plays (ui/device/Sampler.tsx shows it); store and catalog reads, so the line follows them. */
export function smpWhat(d: Device, i: number): string {
  const src = choice(vget(d, ip(`osc${i}.s7`)), 3), idx = Math.round(vget(d, ip(`osc${i}.sample`))), nGsm = d.samples?.length ?? 0, gm = lib.catalog()?.gm as { instruments: GmInst[] } | undefined;
  if (src === 0) { const kb = idx && idx <= nGsm ? ` · ${(d.samples![idx - 1]!.length * 3 / 4 / 1024).toFixed(1)} KB` : "";
    return idx ? `GSM sample ${idx} of ${nGsm} in this instrument${idx > nGsm ? " (missing: plays silence)" : kb}` : `no GSM sample chosen (${nGsm} in this instrument)`; }
  if (src === 1) { const k = gmInstrument(idx), g = k && gm?.instruments.find(x => !!x.drums === k.drums && x.bank === k.bank && x.prog === k.prog);
    return k ? `GM.DLS ${k.drums ? "drum kit" : "instrument"} ${g ? `“${g.name}”` : ""} bank ${k.bank} program ${k.prog}${g ? ` · ${g.regions.length} regions` : gm ? " (not in this bank)" : ""}` : "no GM instrument chosen"; }
  const g = gm?.instruments.find(x => x.regions.some(r => r[2] === idx - 1)); return idx ? `GM.DLS wave ${idx - 1}${g ? ` · from ${g.name}` : ""}` : "no wave chosen";
}
const pcmCache = new Map<string, Float32Array | null>();
/** Decoded GSM sample (44.1 kHz mono), cached. */
export const pcmOf = (b64: string): Float32Array | null => { if (!pcmCache.has(b64)) pcmCache.set(b64, E.gsmDecode(b64ToBytes(b64))); return pcmCache.get(b64)!; };
// ---- GSM samples: stored (base64) in the instrument's `samples`; a sampler slot plays one by 1-based number. From the
// library (samples in project songs), or imported from an audio file: decoded and resampled to 44.1 kHz by the
// browser, mixed to mono, encoded to GSM 06.10 by the engine (bit-exact libgsm port, about 9 KB per second). ----
const gsmSongs = new Map<string, any>();  // song path → parsed song (library loads)
/** A library GSM entry's sample (base64), read from its song. */
export async function gsmOf(g: { path: string; track: string; device: number; sample: number }): Promise<string | null> {
  if (!gsmSongs.has(g.path)) gsmSongs.set(g.path, songText(await api.read(g.path)));
  const s = gsmSongs.get(g.path), t = [...s.tracks, s.master].find((x: { id: string }) => x.id === g.track);
  return t?.devices?.[g.device]?.samples?.[g.sample] ?? null;
}
let previewCtx: AudioContext | null = null, previewSrc: AudioBufferSourceNode | null = null;
/** Plays a GSM sample once (preview; not through the engine). */
export async function playSample(b64: string): Promise<void> {
  const pcm = E.gsmDecode(b64ToBytes(b64)); if (!pcm) { say("not a GSM sample"); return; }
  previewCtx ??= new AudioContext({ sampleRate: 44100 }); await previewCtx.resume();
  try { previewSrc?.stop(); } catch {}
  const buf = previewCtx.createBuffer(1, pcm.length, 44100); buf.copyToChannel(pcm as Float32Array<ArrayBuffer>, 0);
  previewSrc = previewCtx.createBufferSource(); previewSrc.buffer = buf; previewSrc.connect(previewCtx.destination); previewSrc.start();
  say(`playing the sample (${(pcm.length / 44100).toFixed(2)} s)`);
}
/** The slot a new GSM sample goes to: a GSM sampler, else any sampler, else a free slot, else 1. */
export const gsmSlot = (d: Device): number => [1, 2, 3].find(i => oscType(d, i) === 4 && choice(vget(d, ip(`osc${i}.s7`)), 3) === 0) || [1, 2, 3].find(i => oscType(d, i) === 4) || [1, 2, 3].find(i => !slotUsed(d, "osc", i)) || 1;
/** Sample numbers the GSM sampler slots play. */
export function gsmUsed(d: Device): Set<number> { return new Set([1, 2, 3].filter(i => oscType(d, i) === 4 && choice(vget(d, ip(`osc${i}.s7`)), 3) === 0).map(i => Math.round(vget(d, ip(`osc${i}.sample`))))); }
