import { describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { COUNT, fi, lengthRows, NAMES, parseXml, playable, quantise, RANGE, readPreset, readProject } from "./e3prj.ts";
import { features, songStats } from "./survey.ts";

const project = (instruments: string, song = "") => `<?xml version="1.0"?>
<enigma version="3.11b"><songs>${song}</songs><instruments>${instruments}</instruments></enigma>`;

const SONG = `<song bpm="140" id="1" name="Test"><patterns>
  <pattern rowcount="16" patternid="0" trackcount="2"><tracks>
    <track><ne i="0" n="61" r="0" v="-1" e="0"/><ne i="-1" n="128" r="4" v="-1" e="0"/></track>
    <track><ne i="0" n="65" r="2" v="64" e="258"/></track>
  </tracks></pattern></patterns>
  <patterninstances><patterninstance rowoffset="0" patternid="0" track="0"/>
    <patterninstance rowoffset="32" patternid="0" track="1"/></patterninstances></song>`;

describe("e3prj", () => {
  test("129 names and ranges in spec order", () => {
    expect(NAMES.length).toBe(COUNT);
    expect(new Set(NAMES).size).toBe(COUNT);
    expect(NAMES[68]).toBe("Mm1Source");
    expect(NAMES[97]).toBe("Mm10Targe");
    expect(NAMES[128]).toBe("EqHigh");
    expect(RANGE[NAMES.indexOf("OscUnisn")]).toBe(9);
    expect(RANGE[NAMES.indexOf("Mm3Target")]).toBe(25);
  });

  test("fi rounds half to even like x87", () => {
    expect([0.5, 1.5, 2.5, 2.4, 2.6, -1.5].map(fi)).toEqual([0, 2, 2, 2, 3, -2]);
  });

  test("quantise reads back the stored byte", () => {
    expect(quantise(NAMES.indexOf("OscVol"), 0.123456)).toBeCloseTo(0.12, 9);
    expect(quantise(NAMES.indexOf("OscUnisn"), 0.5)).toBeCloseTo(4 / 9, 9);
    expect(quantise(NAMES.indexOf("Effect1"), 0.52)).toBeCloseTo(0.5, 9);
  });

  test("project: instruments by name, unknown names listed, song structure", () => {
    const p = readProject(project(
      `<instrument id="3" name="Bass"><parameters><param value="0.7" name="OscVol"/><param value="1" name="Nope"/></parameters></instrument>`,
      SONG));
    expect(p.version).toBe("3.11b");
    expect(p.instruments[0].id).toBe(3);
    expect(p.instruments[0].params[NAMES.indexOf("OscVol")]).toBe(0.7);
    expect(p.instruments[0].unknown).toEqual(["Nope"]);
    const s = playable(p)[0];
    expect(s.bpm).toBe(140);
    expect(s.patterns[0].tracks[1][0]).toEqual({ row: 2, note: 65, instrument: 0, velocity: 64, effect: 258 });
    expect(lengthRows(s)).toBe(48);
    expect(songStats(s)).toMatchObject({ rows: 48, notes: 4, offs: 2, effects: 2, noInstrument: 0, instruments: [0] });
  });

  test("malformed input names the problem", () => {
    expect(() => readProject("<foo/>")).toThrow(/no <enigma>/);
    expect(() => parseXml("<a><b></a>")).toThrow(/unexpected <\/a>/);
    expect(() => readProject(project("", SONG.replace('trackcount="2"', 'trackcount="3"')))).toThrow(/trackcount 3 but 2/);
    expect(() => readProject(project("", SONG.replace('r="4"', 'r="16"')))).toThrow(/row 16 outside 16 rows/);
    const missing = readProject(project("", SONG.replace('rowoffset="32" patternid="0"', 'rowoffset="32" patternid="9"')));
    expect(() => songStats(playable(missing)[0])).toThrow(/missing pattern 9/);
  });

  test("preset text", () => {
    const { params, unknown } = readPreset("String, Short\nOscPoints;1\nOscVol;0.5\nWhat;1\n");
    expect(params[0]).toBe(1);
    expect(params[NAMES.indexOf("OscVol")]).toBe(0.5);
    expect(unknown).toEqual(["What"]);
  });

  test("features of a step-oscillator patch with a PAD drone", () => {
    const v = new Array(COUNT).fill(0);
    v[NAMES.indexOf("OscVol")] = 0.5;
    v[NAMES.indexOf("OscPoints")] = 1;
    v[NAMES.indexOf("OscUnisn")] = 1;
    v[NAMES.indexOf("AddVolume")] = 0.5;
    v[NAMES.indexOf("AddProfile")] = 1;
    v[NAMES.indexOf("Effect1")] = 0.3;
    expect(features(v)).toEqual(["osc: 4 points", "osc mode: step", "osc unison 11", "osc octave 4",
      "pad: gauss", "pad without volume route (drones after note-off)", "gain ≠ unity", "fx chorus (editor only)"]);
  });

  test("survey skips without the checkout", () => {
    const r = spawnSync("bun", [join(import.meta.dir, "survey.ts")], { env: { ...process.env, TF3_REF: "/nonexistent/tf3" }, encoding: "utf8" });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain("tf3-survey: skipped");
  });
});
