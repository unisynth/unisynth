// DAW file API (change add-synth-daw, tasks 4.1/4.1a): list/read/write under project/music, validation before
// write, confinement, size limit, library sources, GM bank route, /daw token page, foreign Origin.
import { afterAll, beforeAll, expect, test } from "bun:test";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { MAX_BYTES, bankPath, confineMusic, library, musicApi } from "./music.ts";
import { loadSynth } from "./synth.ts";
import { ROOT, startServer, type Server } from "./server.ts";

let root: string;
const synth = () => loadSynth();
const demo = () => readFileSync(join(ROOT, "project/music/demo.song.json"), "utf8");
const api = (path: string, init?: RequestInit, env: Record<string, string | undefined> = { GM_DLS: "", SYNTH_LAB: "/nonexistent" }) => {
  const url = new URL("http://127.0.0.1/" + path.replace(/^\//, ""));
  return musicApi(root, new Request(url, init), url, synth, env as NodeJS.ProcessEnv) as Promise<Response>;
};

beforeAll(() => {
  root = mkdtempSync(join(tmpdir(), "daw-music-"));
  mkdirSync(join(root, "project/music/patches"), { recursive: true });
  cpSync(join(ROOT, "project/music/demo.song.json"), join(root, "project/music/demo.song.json"));
  cpSync(join(ROOT, "project/music/patches"), join(root, "project/music/patches"), { recursive: true });
  mkdirSync(join(root, "outside"));
  writeFileSync(join(root, "outside/secret.song.json"), "{}");
  symlinkSync(join(root, "outside"), join(root, "project/music/link"));
});
afterAll(() => rmSync(root, { recursive: true, force: true }));

test("lists songs, presets and blobs; reads one file", async () => {
  const l = await (await api("/api/music")).json();
  expect(l.map((f: any) => [f.path, f.kind])).toEqual([
    ["project/music/demo.song.json", "song"],
    ["project/music/patches/acid-bass.patch.json", "patch"],
    ["project/music/patches/glass-pad.patch.json", "patch"],
  ]);
  const r = await api("/api/music?path=project/music/demo.song.json");
  expect(await r.text()).toBe(demo());
  expect((await api("/api/music?path=project/music/nope.song.json")).status).toBe(404);
});

test("a valid song is written atomically; an invalid one is refused with paths and not written", async () => {
  const path = "project/music/new.song.json";
  const ok = await api(`/api/music?path=${path}`, { method: "PUT", body: demo() });
  expect(ok.status).toBe(200);
  expect(readFileSync(join(root, path), "utf8")).toBe(demo());
  expect(readdirSync(join(root, "project/music")).some((f) => f.endsWith(".tmp"))).toBe(false);
  const bad = JSON.parse(demo());
  bad.tracks[0].devices[0].values["osc1.cutof"] = 0.5;
  const r = await api(`/api/music?path=${path}`, { method: "PUT", body: JSON.stringify(bad) });
  expect(r.status).toBe(422);
  const body = await r.json();
  expect(body.errors[0]).toEqual({ path: "/tracks/0/devices/0/values/osc1.cutof", message: "unknown parameter" });
  expect(readFileSync(join(root, path), "utf8")).toBe(demo());
  // presets are validated too; blobs are stored as bytes
  const preset = await api("/api/music?path=project/music/patches/x.patch.json", { method: "PUT", body: JSON.stringify({ format: "us-patch/1", name: "x", kind: 64, values: { nope: 1 } }) });
  expect(preset.status).toBe(422);
  const blob = await api("/api/music?path=project/music/demo.song.bin", { method: "PUT", body: new Uint8Array([1, 2, 3]) });
  expect(blob.status).toBe(200);
});

test("paths outside project/music, symlinks out and other extensions are refused", async () => {
  for (const p of ["../x.song.json", "project/music/../../x.song.json", "/etc/passwd", "project/music/notes.txt", "project/music/link/secret.song.json", "project/intro.json", "project/music/.hidden.song.json"]) {
    expect(confineMusic(root, p)).toBeNull();
    expect((await api(`/api/music?path=${encodeURIComponent(p)}`)).status).toBe(403);
    expect((await api(`/api/music?path=${encodeURIComponent(p)}`, { method: "PUT", body: demo() })).status).toBe(403);
  }
  expect(readFileSync(join(root, "outside/secret.song.json"), "utf8")).toBe("{}");
  expect(confineMusic(root, "project/music/sub/new.song.json")).toBe(join(root, "project/music/sub/new.song.json"));
});

test("bodies over the size limit are refused", async () => {
  const r = await api("/api/music?path=project/music/big.song.json", { method: "PUT", body: "x", headers: { "content-length": String(MAX_BYTES + 1) } });
  expect(r.status).toBe(413);
  expect(existsSync(join(root, "project/music/big.song.json"))).toBe(false);
});

test("library lists available sources with counts and skips missing ones", async () => {
  const none = library(root, loadSynth(), { GM_DLS: "", SYNTH_LAB: "/nonexistent" } as NodeJS.ProcessEnv);
  expect(Object.keys(none).sort()).toEqual(["counts", "gsm", "project", "repo"]);
  expect(none.counts.presets).toBe(2);
  expect(none.project.presets.map((p: any) => p.name).sort()).toEqual(["Acid Bass", "Glass Pad"]);
  const bank = bankPath({} as NodeJS.ProcessEnv);
  if (bank) {
    const withBank = library(root, loadSynth(), { GM_DLS: bank, SYNTH_LAB: "/nonexistent" } as NodeJS.ProcessEnv);
    expect(withBank.gm.instruments.length).toBeGreaterThan(100);
    expect(withBank.counts.gm).toBe(withBank.gm.instruments.length);
    expect(withBank.gm.instruments[0]).toMatchObject({ name: expect.any(String), bank: expect.any(Number), prog: expect.any(Number) });
  }
});

test("library lists the GSM samples stored in project songs, once per distinct sample", () => {
  // a GSM chunk: comp, uncomp (samples), WAVEFORMATEX (0x31, mono, 44100, 8957, 65, 0, cb 2, 320), one 65-byte block
  const head = new DataView(new ArrayBuffer(28));
  [[0, 65], [4, 320]].forEach(([o, v]) => head.setInt32(o, v, true));
  [[8, 0x31], [10, 1], [20, 65], [22, 0], [24, 2], [26, 320]].forEach(([o, v]) => head.setUint16(o, v, true));
  head.setUint32(12, 44100, true); head.setUint32(16, 8957, true);
  const chunk = Buffer.concat([Buffer.from(head.buffer), Buffer.alloc(65, 7)]).toString("base64");
  const song = JSON.parse(readFileSync(join(root, "project/music/demo.song.json"), "utf8"));
  const bass = song.tracks.find((t: any) => t.id === "bass");
  bass.devices[0].samples = [chunk];
  song.tracks.find((t: any) => t.id === "lead").devices[0].samples = [chunk]; // the same sample again
  writeFileSync(join(root, "project/music/gsm.song.json"), JSON.stringify(song));
  try {
    const cat = library(root, loadSynth(), { GM_DLS: "", SYNTH_LAB: "/nonexistent" } as NodeJS.ProcessEnv);
    expect(cat.gsm.samples).toHaveLength(1);
    expect(cat.gsm.samples[0]).toMatchObject({ path: "project/music/gsm.song.json", track: "bass", device: 0, sample: 0, bytes: 93, seconds: 320 / 44100, uses: 2 });
    expect(cat.counts.gsm).toBe(1);
  } finally {
    rmSync(join(root, "project/music/gsm.song.json"));
  }
});

test("library attaches converted entries only from a us-library/2 file (unify-v2-algorithms 6.9)", () => {
  const lab = mkdtempSync(join(tmpdir(), "daw-lab-"));
  const progs = join(lab, "tunefish/patches/tf4programs");
  mkdirSync(progs, { recursive: true });
  writeFileSync(join(progs, "program7.txt"), "Lead\n");
  const f = join(root, "project/music/.library.json");
  const entry = { devices: [{ kind: 64, values: { "osc1.type": 0.25 } }] };
  const conv = (format: string) => {
    writeFileSync(f, JSON.stringify({ format, tunefish: { "7": entry }, wavesabre: {} }));
    return library(root, loadSynth(), { GM_DLS: "", SYNTH_LAB: lab } as NodeJS.ProcessEnv).tunefish.programs[0].conv;
  };
  try {
    expect(conv("us-library/2")).toEqual(entry);
    expect(conv("us-library/1")).toBeUndefined(); // the old layout's values: left out until rebuilt
  } finally {
    rmSync(f);
    rmSync(lab, { recursive: true, force: true });
  }
});

test("/api/bank is 404 without a bank and the bytes with one", async () => {
  expect((await api("/api/bank")).status).toBe(404);
  const fake = join(root, "fake.dls");
  writeFileSync(fake, new Uint8Array([82, 73, 70, 70, 1, 2]));
  const r = await api("/api/bank", undefined, { GM_DLS: fake });
  expect(r.status).toBe(200);
  expect([...new Uint8Array(await r.arrayBuffer())]).toEqual([82, 73, 70, 70, 1, 2]);
});

let s: Server;
let dawDist: string;
beforeAll(async () => {
  dawDist = mkdtempSync(join(tmpdir(), "daw-dist-"));
  writeFileSync(join(dawDist, "index.html"), "<!doctype html><html><head><title>DAW</title></head><body></body></html>");
  writeFileSync(join(dawDist, "main.js"), "console.log(1)");
  s = await startServer({ port: 0, build: false, root, dawDist });
});
afterAll(async () => {
  await s?.stop();
  rmSync(dawDist, { recursive: true, force: true });
});

test("/daw serves the page with the server token; foreign origins cannot write music", async () => {
  expect((await fetch(`${s.url}daw`, { redirect: "manual" })).headers.get("location")).toBe("/daw/");
  const page = await (await fetch(`${s.url}daw/`)).text();
  expect(page).toMatch(/<meta name="x-64k-token" content="[0-9a-f-]{36}">/);
  expect(await (await fetch(`${s.url}daw/main.js`)).text()).toBe("console.log(1)");
  // cross-origin isolation (change add-wasm-threads): page COOP + COEP, every DAW file CORP
  const head = (await fetch(`${s.url}daw/`)).headers;
  expect([head.get("cross-origin-opener-policy"), head.get("cross-origin-embedder-policy")]).toEqual(["same-origin", "require-corp"]);
  expect((await fetch(`${s.url}daw/main.js`)).headers.get("cross-origin-resource-policy")).toBe("same-origin");
  expect((await fetch(`${s.url}daw/../package.json`)).status).toBe(404);
  const evil = await fetch(`${s.url}api/music?path=project/music/evil.song.json`, { method: "PUT", body: demo(), headers: { origin: "https://evil.example" } });
  expect(evil.status).toBe(403);
  expect(existsSync(join(root, "project/music/evil.song.json"))).toBe(false);
  const same = await fetch(`${s.url}api/music?path=project/music/ok.song.json`, { method: "PUT", body: demo(), headers: { origin: s.url.replace(/\/$/, "") } });
  expect(same.status).toBe(200);
});
