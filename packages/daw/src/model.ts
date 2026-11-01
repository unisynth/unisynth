// song/1 ⇄ editor model (change add-synth-daw). The editor keeps the mockup's working model — bars (1-based) and
// sixteenth steps, dB volumes, sends stored on the sending track, one pattern per clip name per track — and this
// adapter turns it into the song/1 source (receives on the target, ticks, linear volumes) and back. Devices are
// song/1 devices already (`{kind, values}` by parameter name), so the adapter passes them through.

export const TPB = 960;

export type Device = { kind: number; values: Record<string, number>; samples?: string[]; speech?: string[]; bypass?: boolean; preset?: string; trailer?: number };
export type SongNote = [number, number, number, number];
export type Point = [number, number];
/** An editor lane point: [bar, value] and, while unmoved, the tick it was loaded from. */
export type LanePoint = [number, number, number?];
export type Song = {
  format: "song/1";
  /** Title, author, licence notes (crates/song `Meta`); kept as loaded. */
  meta?: Record<string, unknown>;
  /** What the song is (genre, style, tempo; crates/song `About`); kept as loaded. */
  about?: About;
  rate: number;
  tempo: number;
  signature: [number, number];
  ticks_per_beat: number;
  length: number;
  clips: { id: string; length: number; notes: SongNote[]; wheel?: Point[]; bend?: Point[] }[];
  tracks: SongTrack[];
  master: SongTrack;
};
/** crates/song `About`: a song's genre or a track's role, style, tempo/rhythm, engine, similar instruments. */
export type About = { type: string; style?: string; tempo?: string; engine?: string; similar_to?: string[]; description: string };
export type SongTrack = {
  id: string;
  name: string;
  about?: About;
  color?: string;
  volume: number;
  mute: boolean;
  receives: { from: string; channel: "main" | "side"; volume: number }[];
  devices: Device[];
  placements: { clip: string; start: number; length: number | null; loop: boolean }[];
  lanes: { device: number; param: string; points: Point[] }[];
};

/** Editor note: step (1/16 bar), length in steps, pitch, velocity 0..1. */
export type UINote = { s: number; l: number; n: number; v: number; sel?: boolean };
/** `id`: the song/1 clip id it was loaded from (kept on save). */
export type UIPattern = { len: number; notes: UINote[]; cc?: { wheel: Point[]; bend: Point[] }; id?: string };
export type UIClip = { start: number; len: number; name: string; sel?: boolean; loop?: boolean };
export type UITrack = {
  id: string;
  name: string;
  /** Description as loaded (role, sound, similar instruments); written back unchanged. */
  about?: About;
  /** Display colour (CSS variable); `color` is the song/1 value, kept as loaded. */
  col: string;
  color?: string;
  vol: number;
  mute?: boolean;
  solo?: boolean;
  bus?: boolean;
  master?: boolean;
  clips: UIClip[];
  pats?: Record<string, UIPattern>;
  devices: Device[];
  /** [target track name, "main" | "side", dB, linear gain as loaded (kept while dB is unchanged)] */
  sends: [string, string, number, number?][];
  /** Linear volume as loaded, kept while `vol` (dB) is unchanged, so an unedited save is exact. */
  gain0?: [number, number];
  /** Sending track ids in the order this track received them when loaded (summing order). */
  recv0?: string[];
  /** Master only: its receives as loaded (sources, channels, volumes, order) and the track ids present then. */
  mrecv0?: SongTrack["receives"];
  ids0?: string[];
  lanes: { dev: number; param: string; pts: LanePoint[] }[];
  pat?: string;
  open?: boolean;
};
/** `length0`: [bars, ticks] as loaded; the tick length is kept while the bar count is unchanged. Lane points may
 * carry their loaded tick as a third element, kept while the point's bar is unchanged. */
export type UIState = { tracks: UITrack[]; master: UITrack; tempo: number; sig: [number, number]; bars: number; rate?: number; meta?: Record<string, unknown>; about?: About; length0?: [number, number] };

/** A device as song/1 stores it (editor-only fields such as `_edited` are dropped). */
export function cleanDevice(d: Device): Device {
  const out: Device = { kind: d.kind, values: { ...d.values } };
  if (d.samples?.length) out.samples = [...d.samples];
  if (d.speech?.length) out.speech = [...d.speech];
  if (d.bypass) out.bypass = true;
  if (d.preset) out.preset = d.preset;
  if (d.trailer !== undefined) out.trailer = d.trailer;
  return out;
}

/** song/1 JSON in the layout `crates/song` writes: two-space indent, arrays of scalars on one line. */
export function prettyJson(v: unknown): string {
  const scalar = (x: unknown) => x === null || typeof x !== "object";
  const go = (x: unknown, ind: number): string => {
    const pad = (n: number) => "  ".repeat(n);
    if (Array.isArray(x)) {
      if (!x.length) return "[]";
      if (x.every(scalar)) return "[" + x.map((e) => JSON.stringify(e)).join(", ") + "]";
      return "[\n" + x.map((e) => pad(ind + 1) + go(e, ind + 1)).join(",\n") + "\n" + pad(ind) + "]";
    }
    if (x && typeof x === "object") {
      const es = Object.entries(x).filter(([, e]) => e !== undefined);
      if (!es.length) return "{}";
      return "{\n" + es.map(([k, e]) => pad(ind + 1) + JSON.stringify(k) + ": " + go(e, ind + 1)).join(",\n") + "\n" + pad(ind) + "}";
    }
    return JSON.stringify(x);
  };
  return go(v, 0) + "\n";
}

export const dbToGain = (db: number) => (db <= -60 ? 0 : Math.pow(10, db / 20));
export const gainToDb = (g: number) => (g <= 0.001 ? -60 : 20 * Math.log10(g));
/** Linear gain for `db`, or the loaded gain `g0` while `db` still equals its loaded dB value. */
const gainOf = (db: number, g0?: [number, number]) => (g0 && g0[0] === db ? g0[1] : dbToGain(db));
export const ticksPerBar = (sig: [number, number]) => (TPB * 4 * sig[0]) / sig[1];

/** Live-preview audibility: not muted, and when any track is soloed, soloed itself, a send source of a soloed track
 * (a kick feeding a soloed pad's sidechain keeps feeding it), or a bus a soloed track sends to. */
export function audible(tracks: UITrack[], t: UITrack): boolean {
  if (t.mute) return false;
  const solos = tracks.filter((x) => x.solo);
  if (!solos.length || t.solo) return true;
  if (t.sends.some((s) => solos.some((x) => x.name === s[0]))) return true;
  return !!t.bus && solos.some((x) => x.sends.some((s) => s[0] === t.name));
}

/** The editor model as a song/1 source. `audible(t)` (solo/mute for live preview) defaults to `!t.mute`. */
export function toSong(ui: UIState, audible?: (t: UITrack) => boolean): Song {
  const bar = ticksPerBar(ui.sig), step = bar / 16;
  const tick = (b: number) => Math.round((b - 1) * bar);
  const clips: Song["clips"] = [];
  // clip ids: the loaded id while it is free, else `<track>/<name>` (patterns are per track in the editor)
  const used = new Set<string>();
  const ids = new Map<string, string>();
  const pid = (t: UITrack, name: string) => {
    const key = `${t.id}\u0000${name}`;
    if (!ids.has(key)) {
      let id = t.pats?.[name]?.id ?? `${t.id}/${name}`;
      if (used.has(id)) id = `${t.id}/${name}`;
      for (let k = 2; used.has(id); k++) id = `${t.id}/${name} ${k}`;
      used.add(id);
      ids.set(key, id);
    }
    return ids.get(key)!;
  };
  const songTrack = (t: UITrack): SongTrack => {
    const placements = t.bus || t.master ? [] : t.clips.map((c) => ({ clip: pid(t, c.name), start: tick(c.start), length: Math.max(1, Math.round(c.len * bar)), loop: !!c.loop }));
    if (!t.bus && !t.master) {
      for (const name of new Set(t.clips.map((c) => c.name))) {
        const pt = t.pats?.[name];
        const len = Math.max(1, Math.round((pt?.len ?? Math.max(...t.clips.filter((c) => c.name === name).map((c) => c.len))) * bar));
        const notes: SongNote[] = (pt?.notes ?? [])
          .map((n) => [Math.max(0, Math.round(n.s * step)), Math.max(1, Math.round(n.l * step)), Math.min(127, Math.max(0, Math.round(n.n))), Math.min(127, Math.max(1, Math.round(n.v * 127)))] as SongNote)
          .filter((n) => n[0] < len)
          .sort((a, b) => a[0] - b[0] || a[2] - b[2]);
        // controller envelopes are written when they do something (a flat default is no envelope); a song with one
        // needs the instrument's ctl.wheel / ctl.bend (validation says so instead of the save dropping them)
        const wheel = pt?.cc?.wheel ?? [], bend = pt?.cc?.bend ?? [];
        const env = (pts: Point[], def: number) => (pts.some((q) => q[1] !== def) ? pts.map(([st, v]) => [Math.max(0, Math.round(st * step)), v] as Point) : null);
        const w = env(wheel, 0), bd = env(bend, 0.5);
        clips.push({ id: pid(t, name), length: len, notes, ...(w ? { wheel: w } : {}), ...(bd ? { bend: bd } : {}) });
      }
    }
    return {
      id: t.id,
      name: t.name,
      ...(t.about ? { about: t.about } : {}),
      ...(t.color !== undefined ? { color: t.color } : {}),
      volume: gainOf(t.vol, t.gain0),
      mute: audible ? !audible(t) : !!t.mute,
      receives: [],
      devices: t.devices.map(cleanDevice),
      placements,
      lanes: t.lanes.map((ln) => ({ device: ln.dev, param: ln.param, points: ln.pts.map(([b, v, tk]) => [tk !== undefined && 1 + tk / bar === b ? tk : Math.max(0, (b - 1) * bar), v] as Point) })),
    };
  };
  const tracks = ui.tracks.map(songTrack);
  const master = songTrack(ui.master);
  // sends → receives on the target, in the order the target received them when loaded (summing order)
  for (const t of ui.tracks) {
    for (const [to, ch, db, g] of t.sends) {
      const dst = tracks.find((x) => x.name === to);
      if (dst) dst.receives.push({ from: t.id, channel: ch === "side" ? "side" : "main", volume: g !== undefined && Math.abs(gainToDb(g) - db) < 1e-9 ? g : dbToGain(db) });
    }
  }
  ui.tracks.forEach((t, i) => {
    const order = t.recv0;
    if (order) tracks[i].receives.sort((a, b) => (order.indexOf(a.from) + 1 || 1e9) - (order.indexOf(b.from) + 1 || 1e9));
  });
  // the master: its receives as loaded (for tracks that still exist), plus every track made in the editor
  const live = new Set(ui.tracks.map((t) => t.id)), was = new Set(ui.master.ids0 ?? []);
  if (ui.master.mrecv0) master.receives.push(...ui.master.mrecv0.filter((r) => live.has(r.from)).map((r) => ({ ...r })));
  for (const t of ui.tracks) if (!ui.master.mrecv0 || !was.has(t.id)) master.receives.push({ from: t.id, channel: "main", volume: 1 });
  const length = ui.length0 && ui.length0[0] === ui.bars ? ui.length0[1] : Math.round(ui.bars * bar);
  return { format: "song/1", ...(ui.meta ? { meta: ui.meta } : {}), ...(ui.about ? { about: ui.about } : {}), rate: ui.rate ?? 44100, tempo: ui.tempo, signature: ui.sig, ticks_per_beat: TPB, length, clips, tracks, master };
}

const COLORS = ["--t-kick", "--t-bass", "--t-lead", "--t-pad", "--t-keys", "--t-hats"];

/** A song/1 source as the editor model. */
export function fromSong(s: Song): UIState {
  const sig = s.signature, bar = ticksPerBar(sig), step = bar / 16;
  const clip = new Map(s.clips.map((c) => [c.id, c]));
  const short = (id: string) => id.slice(id.indexOf("/") + 1) || id;
  const isBus = (t: SongTrack) => !t.devices.some((d) => d.kind === 64) && t.placements.length === 0;
  const ui = (t: SongTrack, i: number, master = false): UITrack => {
    const pats: Record<string, UIPattern> = {};
    const clips: UIClip[] = t.placements.map((p) => {
      const c = clip.get(p.clip);
      const name = short(p.clip);
      const ticksToSteps = (pts: Point[]) => pts.map(([tk, v]) => [tk / step, v] as Point);
      if (c && !pats[name]) pats[name] = { id: c.id, len: c.length / bar, notes: c.notes.map(([st, len, n, v]) => ({ s: st / step, l: len / step, n, v: v / 127 })), cc: { wheel: ticksToSteps(c.wheel ?? []), bend: ticksToSteps(c.bend ?? []) } };
      return { start: 1 + p.start / bar, len: (p.length ?? c?.length ?? bar) / bar, name, loop: p.loop };
    });
    return {
      id: t.id,
      name: t.name,
      ...(t.about ? { about: t.about } : {}),
      col: master ? "--fg" : t.color && t.color.startsWith("--") ? t.color : COLORS[i % COLORS.length],
      color: t.color,
      vol: gainToDb(t.volume),
      gain0: [gainToDb(t.volume), t.volume],
      mute: t.mute,
      bus: !master && isBus(t),
      master,
      clips,
      pats,
      devices: t.devices.map((d) => ({ ...d, values: { ...d.values } })),
      sends: [],
      lanes: t.lanes.map((l) => ({ dev: l.device, param: l.param, pts: l.points.map(([tk, v]) => [1 + tk / bar, v, tk] as LanePoint) })),
      pat: "",
      recv0: t.receives.map((r) => r.from),
      ...(master ? { mrecv0: t.receives.map((r) => ({ ...r })), ids0: s.tracks.map((x) => x.id) } : {}),
    };
  };
  const tracks = s.tracks.map((t, i) => ui(t, i));
  for (const t of s.tracks) {
    for (const r of t.receives) {
      const src = tracks.find((x) => x.id === r.from);
      if (src) src.sends.push([t.name, r.channel, gainToDb(r.volume), r.volume]);
    }
  }
  const bars = Math.max(1, Math.ceil(s.length / bar));
  return { tracks, master: ui(s.master, 0, true), tempo: s.tempo, sig, bars, rate: s.rate, length0: [bars, s.length], ...(s.meta ? { meta: s.meta } : {}), ...(s.about ? { about: s.about } : {}) };
}
