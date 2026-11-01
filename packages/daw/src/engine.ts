// The engine (design D6/D7): one compiled engine module (crates/synth-wasm) used three ways — a main-thread
// instance for song functions (tables, validation, export/import, presets), the AudioWorklet live host, and a
// Worker for offline bounce. The UI clock follows the worklet's reported position (D8e).
import type { Device, Song } from "./model.ts";
import { bounceThreaded, mtModule, mtWorkers, type MtOptions } from "./bounce-mt.ts";

type X = Record<string, any> & { memory: WebAssembly.Memory };
export type Issue = { path: string; message: string };
/** `probe`: the watched track's instrument (layout in crates/synth-wasm `probe_len`), null when nothing is watched. */
export type Status = { position: number; playing: boolean; peaks: Float32Array; probe?: Float32Array | null; watch?: number; load: number; worst: number; gen?: number };
export type Table = { kind: number; mode: number; label: string; params: Param[] };
export type Param = { index: number; name: string; group: string; default: number; law: string; unit: string; min: number; max: number; options: string[]; structural: boolean; raw: boolean };

export class Engine {
  readonly module: WebAssembly.Module;
  readonly x: X;
  base: string;
  ctx: AudioContext | null = null;
  node: AudioWorkletNode | null = null;
  status: Status = { position: 0, playing: false, peaks: new Float32Array(), load: 0, worst: 0 };
  onStatus: ((s: Status) => void) | null = null;
  bank: Uint8Array | null = null;
  private replies = new Map<number, (r: any) => void>();
  private nextId = 1;

  private constructor(module: WebAssembly.Module, base: string) {
    this.module = module;
    this.base = base;
    this.x = new WebAssembly.Instance(module, { env: { host_now_ms: () => performance.now() } }).exports as X;
  }

  static async create(base = "./"): Promise<Engine> {
    const module = await WebAssembly.compileStreaming(fetch(base + "synth.wasm"));
    return new Engine(module, base);
  }

  // ---- song functions (main thread, synchronous) ----
  private call(fn: string, bytes: Uint8Array): { ok: boolean; out: Uint8Array } {
    const p = this.x.alloc(bytes.length);
    new Uint8Array(this.x.memory.buffer, p, bytes.length).set(bytes);
    const ok = this.x[fn](p, bytes.length);
    this.x.dealloc(p, bytes.length);
    return { ok: ok === 1, out: new Uint8Array(this.x.memory.buffer, this.x.out_ptr(), this.x.out_len()).slice() };
  }
  private text(b: Uint8Array) {
    return new TextDecoder().decode(b);
  }
  tables(): Table[] {
    this.x.song_tables();
    return JSON.parse(this.text(new Uint8Array(this.x.memory.buffer, this.x.out_ptr(), this.x.out_len())));
  }
  physical(kind: number, mode: number, index: number, p: number): number {
    return this.x.song_physical(kind, mode, index, p);
  }
  validate(json: string): Issue[] {
    const r = this.call("song_validate", new TextEncoder().encode(json));
    return JSON.parse(this.text(r.out)).errors ?? [];
  }
  /** A stored GSM sample chunk → mono f32 PCM at 44.1 kHz (null when it is not a GSM sample). */
  gsmDecode(chunk: Uint8Array): Float32Array | null {
    const r = this.call("gsm_decode", chunk);
    return r.ok ? new Float32Array(r.out.buffer, r.out.byteOffset, r.out.length >> 2) : null;
  }
  /** Mono f32 PCM at 44.1 kHz → a GSM sample chunk for a device's `samples` (GSM 06.10, about 9 KB per second). */
  gsmEncode(pcm: Float32Array): Uint8Array {
    const n = pcm.length * 4, p = this.x.alloc(Math.max(4, n));
    new Float32Array(this.x.memory.buffer, p, pcm.length).set(pcm);
    this.x.gsm_encode(p, pcm.length);
    this.x.dealloc(p, Math.max(4, n));
    return new Uint8Array(this.x.memory.buffer, this.x.out_ptr(), this.x.out_len()).slice();
  }
  /** A song file read as the current layout: `song/1` is migrated to `song/2` (change unify-v2-algorithms), `song/2`
   * comes back as it is. Errors name what does not migrate. */
  migrate(text: string): { song?: Song; errors?: Issue[] } {
    const r = this.call("song_migrate", new TextEncoder().encode(text));
    const v = JSON.parse(this.text(r.out));
    return r.ok ? { song: v } : { errors: v.errors };
  }
  exportBlob(json: string): { blob?: Uint8Array; errors?: Issue[] } {
    const r = this.call("song_export", new TextEncoder().encode(json));
    return r.ok ? { blob: r.out } : { errors: JSON.parse(this.text(r.out)).errors };
  }
  importBlob(bytes: Uint8Array): { json?: string; error?: string } {
    const r = this.call("song_import", bytes);
    return r.ok ? { json: this.text(r.out) } : { error: JSON.parse(this.text(r.out)).errors?.[0]?.message ?? "import failed" };
  }
  presetSave(device: Device, name: string): string {
    return this.text(this.call("preset_save", new TextEncoder().encode(JSON.stringify({ device, name }))).out);
  }
  presetLoad(text: string): { device?: Device; errors?: Issue[] } {
    const r = this.call("preset_load", new TextEncoder().encode(text));
    const v = JSON.parse(this.text(r.out));
    return r.ok ? { device: v } : { errors: v.errors };
  }

  // ---- live host (AudioWorklet) ----
  /** Output buffer request: a larger buffer rides out longer pauses of the audio thread (other load on the machine)
   * at the cost of later keyboard audition. Takes effect at the next `start` (see `restart`). */
  /** Output underruns of this context so far (`AudioContext.playbackStats`, Chrome 13x+): count and seconds of silence;
   * null where the browser has no such counter. A new context (engine restart) starts from zero. */
  drops(): { events: number; seconds: number } | null {
    const s = (this.ctx as any)?.playbackStats;
    return s && typeof s.underrunEvents === "number" ? { events: s.underrunEvents, seconds: s.underrunDuration } : null;
  }
  latency: AudioContextLatencyCategory | number = "playback";
  /** The audio engine stopped (a trap in the worklet); the next `start` builds a new graph. */
  dead = false;
  onError?: (message: string) => void;
  /** A start in flight: calls while it runs share it (each early key press starts the engine; without this every
   *  one made its own AudioContext and the extra worklets posted empty statuses). */
  private starting: Promise<void> | null = null;
  /** Creates the audio graph (call from a user gesture). */
  start(rate = 44100): Promise<void> {
    return this.starting ??= (this.dead ? this.restart1(rate) : this.start1(rate)).finally(() => { this.starting = null; });
  }
  private async start1(rate: number): Promise<void> {
    if (this.node) {
      await this.ctx!.resume();
      return;
    }
    const ctx = new AudioContext({ sampleRate: rate, latencyHint: this.latency });
    await ctx.audioWorklet.addModule(this.base + "worklet.js");
    const node = new AudioWorkletNode(ctx, "synth", { numberOfInputs: 0, numberOfOutputs: 1, outputChannelCount: [2], processorOptions: { module: this.module } });
    node.connect(ctx.destination);
    const died = (message: string) => {
      if (this.node !== node) return;
      console.error("[daw] audio engine stopped:", message);
      this.dead = true;
      this.status = { ...this.status, playing: false, load: 0, worst: 0, peaks: new Float32Array(this.status.peaks.length) };
      this.onError?.(message);
    };
    node.onprocessorerror = () => died("processor error");
    node.port.onmessage = (e) => {
      if (this.node && this.node !== node) return;  // a replaced engine's messages
      const m = e.data;
      if (m.t === "error") died(m.message);
      else if (this.dead) return; // messages still in flight from an engine that has stopped
      else if (m.t === "status") {
        // a status in flight from before the latest watch() carries another track's probe (or one after unwatching)
        if (m.watch !== this.watched) m.probe = null;
        this.status = m;
        this.onStatus?.(m);
      } else if (m.id && this.replies.has(m.id)) {
        if (m.reply instanceof Uint8Array) m.reply = JSON.parse(new TextDecoder().decode(m.reply) || "{}");
        this.replies.get(m.id)!(m);
        this.replies.delete(m.id);
      }
    };
    this.ctx = ctx;
    this.node = node;
    if (this.bank) this.post({ t: "bank", bytes: this.bank });
    if (this.watched >= 0) this.post({ t: "watch", track: this.watched });
    if (this.runOpen) this.post({ t: "openflt", on: true });
    await ctx.resume();
  }
  /** Closes the audio graph and builds it again with the current `latency` (the song must be loaded again). */
  restart(rate = 44100): Promise<void> {
    return this.starting ??= this.restart1(rate).finally(() => { this.starting = null; });
  }
  private async restart1(rate: number): Promise<void> {
    const old = this.ctx;
    this.node?.disconnect();
    this.node = null;
    this.ctx = null;
    this.dead = false;
    await old?.close();
    await this.start1(rate);
  }
  /** Seconds of output buffering the browser granted (0 before start). */
  get bufferSeconds() {
    return this.ctx?.baseLatency ?? 0;
  }
  get rate() {
    return this.ctx?.sampleRate ?? 44100;
  }
  private post(m: any, transfer: Transferable[] = []) {
    this.node?.port.postMessage(m, transfer);
  }
  private ask(m: any): Promise<any> {
    if (!this.node) return Promise.resolve({ ok: false, reply: { errors: [{ path: "", message: "audio not started" }] } });
    const id = this.nextId++;
    return new Promise((res) => {
      this.replies.set(id, res);
      this.post({ ...m, id });
    });
  }
  load(blob: Uint8Array) {
    return this.ask({ t: "load", blob });
  }
  update(blob: Uint8Array) {
    return this.ask({ t: "update", blob });
  }
  /** Transport generation: bumped by play/stop and echoed in every status, so a status the worklet sent before it
   * saw the latest play/stop (still in flight) is recognisable (`status.gen !== gen`). */
  gen = 0;
  play(frame: number) {
    this.post({ t: "play", frame, gen: ++this.gen });
  }
  stop() {
    this.post({ t: "stop", gen: ++this.gen });
  }
  seek(frame: number) {
    this.post({ t: "seek", frame });
  }
  setLoop(a: number, b: number, on: boolean) {
    this.post({ t: "loop", a, b, on });
  }
  noteOn(track: number, note: number, vel: number) {
    this.post({ t: "on", track, note, vel });
  }
  noteOff(track: number, note: number) {
    this.post({ t: "off", track, note });
  }
  setParam(track: number, dev: number, param: number, value: number) {
    this.post({ t: "param", track, dev, param, value });
  }
  /** Listening switch: fully open filters run instead of being skipped (as WaveSabre/Tunefish do); live playback
   * only, kept across engine restarts. */
  runOpen = false;
  runOpenFilters(on: boolean) {
    this.runOpen = on;
    this.post({ t: "openflt", on });
  }
  /** Track index whose instrument the live displays watch (−1 = none); kept across engine restarts. */
  watched = -1;
  watch(track: number) {
    if (track === this.watched) return;
    this.watched = track;
    this.status = { ...this.status, probe: null }; // the last probe was another track's
    this.post({ t: "watch", track });
  }
  setBank(bytes: Uint8Array) {
    this.bank = bytes;
    this.post({ t: "bank", bytes });
  }

  // ---- offline bounce (Worker) ----
  /** Threaded bounce test hooks (change add-wasm-threads); `lastBounceWorkers` = workers the last bounce used. */
  mt: MtOptions = {};
  lastBounceWorkers = 0;

  /**
   * Threaded when the page is cross-origin isolated, `synth-mt.wasm` compiles and every helper starts (design D5);
   * otherwise, or after any failure of the threaded attempt, the single-thread bounce. Same samples either way.
   */
  bounce(blob: Uint8Array, range: [number, number] | null, onProgress: (v: number) => void): { done: Promise<Float32Array>; cancel: () => void } {
    let cancelled = false;
    let cancel = () => {};
    const single = () => {
      this.lastBounceWorkers = 1;
      const j = this.bounceSingle(blob, range, onProgress);
      cancel = j.cancel;
      return j.done;
    };
    const done = (mtWorkers(this.mt) < 2 ? Promise.resolve(null) : mtModule(this.base)).then((module) => {
      if (cancelled) throw new Error("cancelled");
      if (!module) return single();
      const j = bounceThreaded(this.base, module, blob, this.bank, range, onProgress, this.mt);
      this.lastBounceWorkers = j.workers;
      cancel = j.cancel;
      return j.done.then((r) => {
        if (cancelled) throw new Error("cancelled");
        return r ?? single();
      });
    });
    return { done, cancel: () => { cancelled = true; cancel(); } };
  }

  private bounceSingle(blob: Uint8Array, range: [number, number] | null, onProgress: (v: number) => void): { done: Promise<Float32Array>; cancel: () => void } {
    const w = new Worker(this.base + "bounce-worker.js", { type: "module" });
    let reject: (e: Error) => void = () => {};
    const done = new Promise<Float32Array>((res, rej) => {
      reject = rej;
      w.onmessage = (e) => {
        const m = e.data;
        if (m.t === "progress") onProgress(m.value);
        else if (m.t === "done") {
          w.terminate();
          res(m.samples);
        } else if (m.t === "error") {
          w.terminate();
          rej(new Error(m.message));
        }
      };
      w.onerror = (e) => rej(new Error(e.message));
    });
    w.postMessage({ module: this.module, blob, bank: this.bank ?? undefined, from: range?.[0] ?? 0, to: range?.[1] ?? 0 });
    return { done, cancel: () => { w.terminate(); reject(new Error("cancelled")); } };
  }
}

/** 16-bit stereo WAV (the player's conversion `clamp(trunc(x·32767))`, product rounded to f32). */
export function wav16(samples: Float32Array, rate: number): Blob {
  const n = samples.length, dv = new DataView(new ArrayBuffer(44 + n * 2));
  const w = (o: number, s: string) => [...s].forEach((c, i) => dv.setUint8(o + i, c.charCodeAt(0)));
  w(0, "RIFF"); dv.setUint32(4, 36 + n * 2, true); w(8, "WAVEfmt "); dv.setUint32(16, 16, true); dv.setUint16(20, 1, true); dv.setUint16(22, 2, true);
  dv.setUint32(24, rate, true); dv.setUint32(28, rate * 4, true); dv.setUint16(32, 4, true); dv.setUint16(34, 16, true); w(36, "data"); dv.setUint32(40, n * 2, true);
  // the product in f32 like the player's `(x * 32767.0) as i32`, so the file equals `song render` byte for byte
  for (let i = 0; i < n; i++) dv.setInt16(44 + 2 * i, Math.max(-32768, Math.min(32767, Math.trunc(Math.fround(samples[i] * 32767)))), true);
  return new Blob([dv], { type: "audio/wav" });
}
