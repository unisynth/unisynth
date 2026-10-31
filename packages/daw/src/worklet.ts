// AudioWorklet processor (design D6): instantiates the engine module synchronously from the compiled module passed
// in processorOptions and calls the live host's render(128) per quantum. Commands arrive on the port; status
// (position, playing, per-track peaks, load, the watched instrument's probe) goes back about every 30 ms. No
// SharedArrayBuffer.
declare const sampleRate: number;
declare class AudioWorkletProcessor {
  port: MessagePort;
  constructor(opts?: any);
}
declare function registerProcessor(name: string, ctor: any): void;

export {};
type X = Record<string, any> & { memory: WebAssembly.Memory };

class Synth extends AudioWorkletProcessor {
  x: X;
  since = 0;
  worst = 0;
  /** Render time × frames over the status window: the mean load. The worklet clock is Date.now (1 ms), so one
   * 0.1 ms call reads as 0 or 1 ms (0% or 34% of a 128-frame quantum); the mean over many calls is unbiased. */
  busy = 0;
  /** A track is watched: each status carries its instrument probe (change add-device-live-view D6). */
  watching = false;
  /** The watched track (−1: none), stamped on each status so the main thread can drop a probe sent before a change. */
  watchTrack = -1;
  /** Generation of the last play/stop seen (echoed in status). */
  gen = 0;
  constructor(opts: any) {
    super();
    const now = () => ((globalThis as any).performance?.now?.() ?? Date.now());
    this.x = new WebAssembly.Instance(opts.processorOptions.module, { env: { host_now_ms: now } }).exports as X;
    this.port.onmessage = (e) => {
      if (this.dead) return;
      try {
        this.msg(e.data);
      } catch (err) {
        this.die(err);
      }
    };
  }
  put(b: Uint8Array): [number, number] {
    const p = this.x.alloc(b.length);
    new Uint8Array(this.x.memory.buffer, p, b.length).set(b);
    return [p, b.length];
  }
  /** Reply bytes (JSON); decoded on the main thread (AudioWorkletGlobalScope has no TextDecoder). */
  out(): Uint8Array {
    return new Uint8Array(this.x.memory.buffer, this.x.out_ptr(), this.x.out_len()).slice();
  }
  msg(m: any) {
    const x = this.x;
    switch (m.t) {
      case "load":
      case "update": {
        const [p, n] = this.put(m.blob);
        const ok = x[m.t](p, n);
        x.dealloc(p, n);
        this.port.postMessage({ t: m.t, ok: !!ok, reply: this.out(), id: m.id });
        break;
      }
      case "bank": {
        const [p, n] = this.put(m.bytes);
        x.set_bank(p, n);
        x.dealloc(p, n);
        break;
      }
      case "play": x.play(m.frame); this.gen = m.gen ?? this.gen; break;
      case "stop": x.stop(); this.gen = m.gen ?? this.gen; break;
      case "seek": x.seek(m.frame); break;
      case "loop": x.set_loop(m.a, m.b, m.on ? 1 : 0); break;
      case "on": x.note_on(m.track, m.note, m.vel); break;
      case "off": x.note_off(m.track, m.note); break;
      case "param": x.set_param(m.track, m.dev, m.param, m.value); break;
      // an engine built without the probe has no probe exports: watching then stays off
      case "openflt": x.run_open_filters?.(m.on ? 1 : 0); break;
      case "watch": this.watching = m.track >= 0 && !!x.probe_watch; this.watchTrack = this.watching ? m.track : -1; x.probe_watch?.(m.track); break;
    }
  }
  /** An engine panic traps the module (its state is no longer trustworthy): report it once and go silent; the page
   * shows it and builds a new audio graph on the next Play. Covers commands (play/seek/update reset devices) and
   * render. */
  dead = false;
  die(e: unknown) {
    if (this.dead) return;
    this.dead = true;
    this.port.postMessage({ t: "error", message: String((e as Error)?.message ?? e) });
  }
  process(_in: Float32Array[][], outputs: Float32Array[][]) {
    const out = outputs[0];
    const n = out[0].length;
    const x = this.x;
    if (this.dead) return false;
    let ptr: number;
    try {
      ptr = x.render(n);
    } catch (e) {
      this.die(e);
      return false;
    }
    const f = new Float32Array(x.memory.buffer, ptr, 2 * n);
    const l = out[0], r = out[1] ?? out[0];
    for (let i = 0; i < n; i++) {
      l[i] = f[2 * i];
      r[i] = f[2 * i + 1];
    }
    const load = x.load_ratio();
    if (load > this.worst) this.worst = load;
    this.busy += load * n;
    this.since += n;
    if (this.since >= sampleRate * 0.03) {
      const peaks = new Float32Array(x.memory.buffer, x.peaks_ptr(), x.peaks_len()).slice();
      const pn = this.watching ? x.probe_len() : 0;
      const probe = pn ? new Float32Array(x.memory.buffer, x.probe_ptr(), pn).slice() : null;
      this.port.postMessage({ t: "status", position: x.position(), playing: !!x.is_playing(), peaks, probe, watch: this.watchTrack, load: this.busy / this.since, worst: this.worst, gen: this.gen },
        probe ? [probe.buffer] : []);
      this.since = 0;
      this.worst = 0;
      this.busy = 0;
    }
    return true;
  }
}

registerProcessor("synth", Synth);
