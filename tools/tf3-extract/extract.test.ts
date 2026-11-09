// tools/tf3-extract/extract.py on synthetic images (change add-tf3-pandora 2.2): the older (Pandora, 2011) layout with
// its song, a truncated older block (nothing written), and a current-format block still found. No intro bytes here.
import { expect, test } from "bun:test";
import { mkdtempSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { NAMES, readProject } from "../tf3-parity/e3prj.ts";

const ROOT = join(import.meta.dir, "..", "..");

/** LSB-first bit writer, the older layout's encoding. */
class Bits {
  bytes: number[] = [];
  private bit = 8;
  put(v: number, n: number) {
    for (let i = 0; i < n; i++) {
      if (this.bit === 8) { this.bytes.push(0); this.bit = 0; }
      this.bytes[this.bytes.length - 1] |= ((v >>> i) & 1) << this.bit++;
    }
  }
  /** A packed array: u32 min, u32 step, u32 unused, u32 width, then the values. */
  array(vals: number[], min: number, step: number, width: number) {
    for (const h of [min, step, 0, width]) this.put(h, 32);
    for (const v of vals) if (width) this.put((v - min) / step, width);
  }
}

/** Two instruments (slots 3 and 0) and a song: pattern 0 (4 rows, 2 tracks), pattern 1 (2 rows, 1 empty track). */
function olderBlock(): number[] {
  const b = new Bits();
  b.put(2, 8);
  for (const [slot, gain] of [[3, 54], [0, 20]]) {
    for (let j = 0; j < 99; j++) b.put(j === 98 ? gain : j === 44 ? 100 : j === 14 ? 50 : 0, 8); // 44 = band-pass on
    b.put(slot, 8);
  }
  b.put(7, 32); b.put(125, 16); b.put(3, 8); for (const c of "abc") b.put(c.charCodeAt(0), 8);
  b.put(2, 16);
  b.array([4, 2], 2, 2, 4); // rows, 4-bit: exercises unaligned reads
  b.array([2, 1], 0, 1, 8); // tracks
  // track 0: row 0 note 60 (instr 3, vel 100), rows 1-2 empty, row 3 note-off; track 1: rows 0-1 empty, row 2 note 62,
  // row 3 empty; pattern 1 track 0: empty
  const w = (skip: number, note: number, ins: number, vel: number) => ((skip << 24) | (note << 16) | ((ins + 1) << 8) | (vel + 1)) >>> 0;
  const cells = [w(0, 60, 3, 100), w(2, 128, -1, -1), w(2, 62, 0, -1), w(1, 0, 0, 0), w(2, 0, 0, 0)];
  b.put(cells.length, 16);
  b.array(cells, 0, 1, 32);
  b.put(2, 16);
  b.array([0, 1], 0, 1, 8); // pattern
  b.array([0, 4], 0, 4, 8); // row offset
  b.array([0, 0], 0, 1, 8); // track
  return b.bytes;
}

/** A current-format (2012) block: 32 presence bytes, 129 parameter bytes (all 0), a one-pattern song. */
function currentBlock(): number[] {
  const out = [1, ...Array(31).fill(0), ...Array(129).fill(0)];
  const u16 = (v: number) => [v & 255, v >> 8];
  out.push(1, 0, 0, 0, ...u16(140), ...u16(1), 0x78, ...u16(1));
  out.push(2, 1, ...u16(0)); // rows, tracks, id
  out.push(60, 0, 100, 0, 0, 0, 0, 0, 0, 0); // two cells: note 60 instr 0 vel 100, empty
  out.push(...u16(1), ...u16(0), 0, 0); // one instance
  return out;
}

function run(image: number[]) {
  const dir = mkdtempSync(join(tmpdir(), "tf3-extract-"));
  const img = join(dir, "x.img"), out = join(dir, "x.e3prj");
  // noise around the block so the scan has to find it
  const pad = Array.from({ length: 3000 }, (_, i) => (i * 97 + 13) % 251);
  writeFileSync(img, new Uint8Array([...pad, ...image, ...pad]));
  const p = Bun.spawnSync(["python3", "tools/tf3-extract/extract.py", img, out], { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
  return { code: p.exitCode, msg: p.stdout.toString() + p.stderr.toString(), out };
}

test("an older-layout block is found and written as a current-format project", () => {
  const r = run(olderBlock());
  expect(r.code, r.msg).toBe(0);
  expect(r.msg).toContain("(older), 2 instruments, 125 bpm, 2 patterns, 2 instances, 2 note cells");
  const p = readProject(readFileSync(r.out, "utf8"));
  const s = p.songs[0];
  expect([s.bpm, s.name, s.patterns.length]).toEqual([125, "abc", 2]);
  const pat = s.patterns[0];
  expect(pat.rows).toBe(4);
  expect(pat.tracks[0]).toEqual([{ row: 0, note: 60, instrument: 3, velocity: 100, effect: 0 }, { row: 3, note: 128, instrument: -1, velocity: -1, effect: 0 }]);
  expect(pat.tracks[1]).toEqual([{ row: 2, note: 62, instrument: 0, velocity: -1, effect: 0 }]);
  expect(s.patterns[1].tracks).toEqual([[]]);
  expect(s.instances).toEqual([{ rowOffset: 0, track: 0, patternId: 0 }, { rowOffset: 4, track: 0, patternId: 1 }]);
  const at = (id: number, name: string) => p.instruments.find((i) => i.id === id)!.params[NAMES.indexOf(name)];
  expect(at(3, "GainAmount")).toBeCloseTo(0.54, 6);
  expect(at(0, "GainAmount")).toBeCloseTo(0.2, 6);
  expect(at(3, "OscVol")).toBeCloseTo(0.5, 6);
  expect(at(3, "FltBPOn")).toBe(0); // this engine has no band-pass: switched off
  for (let k = 1; k <= 10; k++) expect(at(3, `Effect${k}`), `Effect${k}`).toBe(0);
});

test("a truncated older block is not found and nothing is written", () => {
  const b = olderBlock();
  const r = run(b.slice(0, b.length - 5));
  expect(r.code).not.toBe(0);
  expect(r.msg).toContain("0 synth blocks found");
  expect(existsSync(r.out)).toBe(false);
});

test("a current-format block is still found first", () => {
  const r = run(currentBlock());
  expect(r.code, r.msg).toBe(0);
  expect(r.msg).toContain("(ranged), 1 instruments, 140 bpm, 1 patterns, 1 instances, 1 note cells");
});
