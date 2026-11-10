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
  // the stored value of mode 1 (its position: a plain choice, or the expanded one of add-elevated-import)
  const d1: Device = { kind: multi.kind, values: { mode: P.modeValue(multi.kind, 1) } };
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

test("osc type labels: the engine's expanded choice, new kinds by label, gaps never a kind", () => {
  const o = P.ip("osc1.type").options!;
  expect(o.length).toBe(25);
  expect([o[0], o[1], o[3], o[5], o[8], o[24]]).toEqual(["Off", "Points", "PAD table", "Pair", "Sampler", "Clocked"]);
  // 7 since add-fudge-softsynth took positions 7 and 9
  expect(o.filter((x) => x === P.GAP).length).toBe(7);
  for (const [label, kind] of [["Sampler", 4], ["Points", 13], ["PAD table", 14], ["Clocked", 12], ["Pair", 15]] as const)
    expect(P.oscType(inst({ "osc1.type": P.optValue("osc1.type", label) }), 1)).toBe(kind);
});
test("a Points slot's controls exist and its drawn cycle is the engine's (pts::cycle fixtures)", () => {
  for (const f of ["points", "mode1", "mode2", "mode3", "mode4", "mode_last", "layers", "layer_detune", "layer_spread"]) expect(P.ip(`osc1.${f}`)).toBeDefined();
  expect(P.ip("osc1.mode1").options).toEqual(["Step", "Linear", "Spline"]);
  // samples 0, 17, 64, 100, 128, 200, 256 of crates/unisynth pts::cycle for three shapes
  const cases: [number, number[], number[], number[], number[]][] = [
    [1, [.5, .5, .5, .5], [1, .5, .5, .5], [0, 0, 0, 0, 0], [-.5, -.5, -.5, -.5, -.5, .5, .5]],
    [2, [.3, .6, .5, .5], [.9, .1, .5, .5], [1, 2, 1, 1, 2], [0.03770714, 0.21479045, 0.7043738, 0.60742676, 0.10498731, -0.7821499, 0.037177075]],
    [4, [.2, .4, .3, .8], [0, 1, .25, .7], [2, 2, 2, 2, 2], [0.045901593, -0.5209182, -0.67650574, 0.7082743, 1.1371987, -0.2485463, 0.046822038]],
  ];
  for (const [n, o, v, m, want] of cases) {
    const { wave, pts } = P.pointCycle(n, o, v, m);
    expect(wave.length).toBe(257);
    [0, 17, 64, 100, 128, 200, 256].forEach((j, k) => expect(Math.abs(wave[j]! - want[k]!)).toBeLessThan(1e-4));
    expect(pts.length).toBe(n);
  }
});
test("a PAD slot's table inputs are load-time values: no automation, rebuild on change; the profile shows its name", () => {
  const ty = P.ip("osc1.type"), pad = inst({ "osc1.type": ty.options.indexOf("PAD table") / (ty.options.length - 1) });
  expect(P.oscType(pad, 1)).toBe(14);
  for (const k of [0, 1, 2, 3, 4]) expect(P.loadTime(pad, P.ip(`osc1.s${k}`))).toBe(true);
  expect(P.loadTime(pad, P.ip("osc1.s5"))).toBe(false);  // drive plays live
  expect(P.loadTime(inst(), P.ip("osc1.s0"))).toBe(false);
  const auto = P.autoParams(pad).map((p) => p.name);
  expect(auto).not.toContain("osc1.s0");
  expect(auto).toContain("osc1.s5");
  expect(auto).toContain("osc2.s0");
  expect(P.fmtVal(pad, P.ip("osc1.s4"), 1)).toBe("gauss");
  expect(P.fmtVal(pad, P.ip("osc1.s4"), 1 / 3)).toBe("detune");
});

test("Elevated's fields show: the pair's controls, the drop, the linear envelope, the new effect modes", () => {
  for (const n of ["drop.base", "drop.rate", "env1.linear", "env1.hold", "env4.hold"]) expect(P.ip(n)).toBeDefined();
  expect(P.ip("env1.linear").options).toEqual(["Off", "On"]);
  expect(P.ip("drop.base").unit).toBe("Hz");
  // their texts: the drop's base in Hz, its rate as a halving time, a linear envelope's times without the 1 ms floor
  const d = inst({ "drop.base": 50 / 20000, "env1.linear": 1 });
  expect(P.fmtVal(d, P.ip("drop.base"), 50 / 20000)).toBe("50.0 Hz");
  expect(P.fmtVal(d, P.ip("drop.rate"), 0)).toBe("off");
  expect(P.fmtVal(d, P.ip("drop.rate"), 1 + Math.log10(0.0007) / 6)).toBe("½ in 22 ms");
  expect(P.fmtVal(d, P.ip("env1.attack"), Math.sqrt(3 / 44.1 / 5000))).toBe("0 ms");
  expect(P.fmtVal(d, P.ip("env1.hold"), Math.sqrt(500 / 5000))).toBe("500 ms");
});

test("an effect's mode reads through its stored position (the expanded distortion, EQ and delay choices)", () => {
  // distortion: 7 positions, the earlier modes at 2·id, Elevated's at the odd ones
  const v = (k: number, name: string) => P.modeValue(k, tables.find((t) => t.kind === k && t.label.endsWith(name))!.mode);
  for (const [k, name] of [[65, "Knee"], [65, "Sine"], [65, "Curve"], [65, "Wrap"], [67, "SVF LFO"], [67, "Cut"], [68, "Allpass"], [68, "Ms"]] as const) {
    const d: Device = { kind: k, values: { mode: v(k, name) } };
    expect(P.tableOf(d)!.label.endsWith(name)).toBe(true);
  }
  expect(P.modeValue(65, 1)).toBeCloseTo(2 / 6); // curve keeps the value a 4-mode choice gave it
});

// ---- engine types (change add-fudge-softsynth) ----
test("engine types: kinds by label, value text in their units, Fudge's defaults", () => {
  const o = P.ip("osc1.type").options!;
  expect([o[7], o[9]]).toEqual(["Shaper tone", "Sweep drum"]);
  expect(o.filter((x) => x === P.GAP).length).toBe(7);
  for (const [label, kind] of [["Shaper tone", P.TONE], ["Sweep drum", P.DRUM]] as const) {
    expect(P.oscType(inst({ "osc1.type": P.optValue("osc1.type", label) }), 1)).toBe(kind);
    expect(P.oscKindOf(P.optValue("osc1.type", label))).toBe(kind);
  }
  expect(P.oscKindOf(P.optValue("osc1.type", "Pair"))).toBe(15);
  // instrument 8 of Alien Sex Clone: volume 257, start frequency 0.11 rad, curve 1 − sin
  const drum = inst({ "osc1.type": P.optValue("osc1.type", "Sweep drum"), ...P.typeDefaults(P.DRUM, 1) });
  const txt = (k: number) => P.fmtVal(drum, P.ip(`osc1.s${k}`), P.vget(drum, P.ip(`osc1.s${k}`)));
  expect([txt(0), txt(1), txt(5), txt(6), txt(8)]).toEqual(["257", "0.11 rad", "50%", "100%", "1 − sin"]);
  expect(txt(7)).toBe("5.1e−4");  // per frame
  const tone = inst({ "osc2.type": P.optValue("osc2.type", "Shaper tone"), ...P.typeDefaults(P.TONE, 2) });
  const t2 = (k: number) => P.fmtVal(tone, P.ip(`osc2.s${k}`), P.vget(tone, P.ip(`osc2.s${k}`)));
  expect([t2(0), t2(2), t2(3), t2(6), t2(7)]).toEqual(["128", "1 rad", "110", "off", "×1"]);
  // the hidden start and direction bytes start at frame 0, forwards; other kinds keep their values
  expect([P.typeDefaults(P.DRUM, 3)["osc3.tracking"], P.typeDefaults(P.DRUM, 3)["osc3.pm_out"], P.typeDefaults(P.TONE, 3)["osc3.feedback"]]).toEqual([0, 0, 0]);
  expect(P.typeDefaults(15, 1)).toEqual({});
});
test("the engine drawing follows unisynth osc::engine (frames of Fudge's instruments 1, 17 and 8, within 2 LSB)", () => {
  const sh = (d: Record<string, number>) => [0, 1, 2, 3, 4, 5, 6, 7, 8].map((k) => d[`osc1.s${k}`] ?? 0);
  const near = (got: number[], at: number[], want: number[]) => at.forEach((i, j) => expect(Math.abs(got[i]! * 32768 - want[j]!)).toBeLessThanOrEqual(2));
  const at = [0, 1, 10, 50, 100, 200, 1000, 3000];
  near(P.engineFrames(P.TONE, sh(P.typeDefaults(P.TONE, 1)), 3001), at, [0, 1375, 12738, 16624, -23307, -10474, -22382, 10474]);
  // tone 17: depth 2.5, shape ½, FM on
  const t17 = [128 / 1024, 128 / 1024, 2.5 / 64, 110 / 256, 0, 0.5, 1, 1 / 16, 0];
  near(P.engineFrames(P.TONE, t17, 3001), at, [0, 3437, 22749, 21752, -24292, -22433, -19112, 24620]);
  // a period of 0 draws silence, as the engine's NaN → 0 conversion plays it
  expect(P.engineFrames(P.TONE, [0, 0, 1 / 64, 110 / 256, 0, 0, 1, 1 / 16, 0], 64).every((x) => x === 0)).toBe(true);
  near(P.engineFrames(P.DRUM, sh(P.typeDefaults(P.DRUM, 1)), 1380), [0, 1, 10, 50, 100, 200, 500, 1000, 1379], [0, 17958, 32126, -30235, -29807, 27246, 19821, 8816, 2353]);
});
