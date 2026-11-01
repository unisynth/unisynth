// Device parameter helpers on the real engine tables (change daw-solid-ts 9.1, step 1): the synth-wasm module's
// `song_tables` / `song_physical`, instantiated here without the browser engine.
import { beforeAll, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { buildSynthWasm } from "../../../../cli/src/synth.ts";
import type { Device } from "../../model.ts";
import type { Table } from "../../engine.ts";
import * as P from "./params.ts";
import { cleanDev, engineDev, instOf, links, patOf } from "./track.ts";
import { b64ToBytes, bytesToB64, gmInstrument } from "./bytes.ts";

let tables: Table[] = [];
beforeAll(() => {
  const x = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(buildSynthWasm())), { env: { host_now_ms: () => 0 } }).exports as any;
  x.song_tables();
  tables = JSON.parse(new TextDecoder().decode(new Uint8Array(x.memory.buffer, x.out_ptr(), x.out_len())));
  P.initParams(tables, (k, m, i, p) => x.song_physical(k, m, i, p));
});
const inst = (values: Record<string, number> = {}): Device => ({ kind: 64, values });

test("choice rounds a 0–1 value to an option index", () => {
  expect(P.choice(0, 4)).toBe(0); expect(P.choice(1, 4)).toBe(3); expect(P.choice(0.5, 3)).toBe(1); expect(P.choice(2, 3)).toBe(2);
});
test("instrument lookup: ip by name, defaults, option names, osc type", () => {
  expect(P.INST().kind).toBe(64);
  const ty = P.ip("osc1.type");
  expect(ty.name).toBe("osc1.type");
  expect(P.vget(inst(), ty)).toBe(ty.default);
  const sampler = ty.options.indexOf("Sampler");
  const d = inst({ "osc1.type": sampler / (ty.options.length - 1) });
  expect(P.optName(d, "osc1.type")).toBe("Sampler");
  expect(P.oscType(d, 1)).toBe(4);
  expect(P.slotUsed(d, "osc", 1)).toBe(true);
  expect(P.slotUsed(inst({ "osc2.type": 0 }), "osc", 2)).toBe(false);
  expect(P.slotKeys("filt", 1).every((k) => k.startsWith("filt1."))).toBe(true);
});
test("effects: mode picks the table; devName and effectChoices", () => {
  const multi = tables.find((t) => t.kind !== 64 && P.nModes(t.kind) > 1)!;
  const n = P.nModes(multi.kind), d1: Device = { kind: multi.kind, values: { mode: 1 / (n - 1) } };
  expect(P.modeOf(d1)).toBe(1);
  expect(P.tableOf(d1)!.mode).toBe(1);
  expect(P.devName(d1)).toBe(P.tableOf(d1)!.label);
  expect(P.devName({ kind: 64, values: {}, preset: "Bass" })).toBe("Bass");
  expect(P.effectChoices().some((t) => t.kind === 64)).toBe(false);
  expect(P.autoParams(d1).every((p) => !p.structural && !p.raw)).toBe(true);
});
test("fmtVal: options, pan, classic envelope time, gain", () => {
  const d = inst();
  const ty = P.ip("osc1.type");
  expect(P.fmtVal(d, ty, 0)).toBe(ty.options[0]!);
  expect(P.panText(0.5)).toBe("C"); expect(P.panText(0)).toBe("L100"); expect(P.panText(0.75)).toBe("R50");
  const att = P.ip("env2.attack");
  if (att.law === "EnvTime") expect(P.fmtVal(inst({ "env2.law": 0 }), att, 0)).toBe("1 ms");
  const gain = tables.flatMap((t) => t.params.map((p) => [t, p] as const)).find(([, p]) => p.law === "Gain");
  if (gain) expect(P.fmtVal({ kind: gain[0].kind, values: {} }, gain[1], 0)).toMatch(/dB$/);
});
test("envelopes: classic curve rises to 1 then holds sustain; Tunefish times grow with the value", () => {
  const c = P.envCurve(false, 0.1, 0.2, 0.5, 0.3, 1);
  expect(c.pts[0]).toEqual([0, 0]); expect(c.pts[1]![1]).toBe(1);
  expect(c.pts.find(([t]) => t === c.off)![1]).toBe(0.5);
  expect(P.envMs(0)).toBe(1); expect(P.envMs(1)).toBe(5001);
  expect(Number.isFinite(P.tfEnvSeconds("attack", 0.5, 0.5, 1))).toBe(true);
  expect(P.tfEnvSeconds("decay", 0.9, 0.5, 1)).toBeGreaterThan(P.tfEnvSeconds("decay", 0.5, 0.5, 1));
  expect(P.tfEnvSeconds("release", 0.9, 0, 1)).toBeGreaterThan(P.tfEnvSeconds("release", 0.5, 0, 1));
});
test("track helpers: instrument, engine index skips bypassed, patterns created once, UI fields dropped", () => {
  const i0 = inst(), fx: Device = { kind: 66, values: {}, bypass: true }, fx2: Device = { kind: 66, values: {} };
  const t: any = { id: "a", name: "A", col: "", vol: 0, devices: [fx, i0, fx2], clips: [{ start: 1, len: 2, name: "c" }, { start: 5, len: 3, name: "c" }], sends: [], lanes: [] };
  expect(instOf(t)).toBe(i0);
  expect(engineDev(t, fx2)).toBe(1); expect(engineDev(t, fx)).toBe(-1);
  expect(links(t, "c")).toBe(2);
  const p = patOf(t, "c"); expect(p.len).toBe(3); expect(patOf(t, "c")).toBe(p);
  expect(cleanDev({ ...i0, _edited: true, _dev: 1 } as any)).toEqual(i0);
});
test("bytes: base64 round trip; GM index", () => {
  const b = new Uint8Array(70000).map((_, i) => i * 7);
  expect(b64ToBytes(bytesToB64(b))).toEqual(b);
  expect(gmInstrument(0)).toBeNull();
  expect(gmInstrument(1 + 128 * 2 + 5)).toEqual({ drums: false, bank: 2, prog: 5 });
  expect(gmInstrument(1 + 16384 + 3)!.drums).toBe(true);
});
