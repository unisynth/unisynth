#!/usr/bin/env python3
"""Find the Tunefish 3 synth block in an unpacked Enigma Studio 3 intro image (from unpack.py) and write it as an
.e3prj project that `synth-convert e3prj` and tools/tf3-parity read. MIT, written from observed data.

  python3 tools/tf3-extract/extract.py <image> <out.e3prj> [--names <project.e3prj>]

The block is the `Song` parameter of the intro's demo operator: 32 instrument-present bytes, then each parameter for
every present instrument (parameter-major), then the song (u32 id, u16 bpm, u16 name length + name, u16 pattern count,
patterns of u8 rows, u8 tracks, u16 id and rows x tracks cells of u8 note, u8 instrument, u8 velocity, u16 effect,
u16 instance count, instances of u16 row offset, u8 track, u8 pattern id). Little-endian.
Parameter bytes come in two variants, both seen in released intros:
  ranged:  value = byte / range (range = the parameter's step count, or 100 for continuous ones)  -- Transplant
  percent: value = byte / 100 for every parameter                                                  -- Train Control
A candidate is accepted only when the instrument count, every parameter byte and the whole song parse consistently.

Older layout (Pandora, 2011; research/tunefish3-functional-spec.md section 10), tried when no current block is found,
read through a bit reader (LSB first; byte-aligned reads are little-endian): u8 instrument count, per instrument 99
parameter bytes (value = byte / 100; byte 98 is GainAmount, no effect slots) and its u8 slot; then u32 id, u16 bpm,
u8 name length + name, u16 pattern count P, packed arrays of P row counts and P track counts, u16 cell count C, a
packed array of C cell words, u16 instance count K, packed arrays of K pattern indices, K row offsets, K tracks. A
packed array is u32 min, u32 step, u32 (unused), u32 bits, then bits-wide values (value = min + v * step). A cell
word's byte 3 counts the empty rows before its note; bytes 2, 1, 0 are note, instrument + 1, velocity + 1. The song
must consume every cell exactly. Its laws differ from the 2012 engine: convert with `synth-convert e3prj --engine 2011`.
"""
import json, os, re, struct, subprocess, sys
from xml.sax.saxutils import quoteattr

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def tables():
    """NAMES and RANGE (step counts, 0 = continuous) from tools/tf3-parity/e3prj.ts."""
    src = 'import {RANGE,NAMES} from "./tools/tf3-parity/e3prj.ts"; console.log(JSON.stringify({RANGE,NAMES}))'
    t = json.loads(subprocess.run(["bun", "-e", src], cwd=ROOT, check=True, capture_output=True, text=True).stdout)
    return t["NAMES"], t["RANGE"]


NAMES, RANGE = tables()
P = len(NAMES)


def parse_song(d, o, n_patterns_max=255):
    sid, bpm, ln = struct.unpack_from("<IHH", d, o); o += 8
    name = d[o:o + ln]
    if not (1 <= bpm <= 400 and ln <= 64 and all(32 <= c < 127 for c in name)):
        raise ValueError("song header")
    o += ln
    pc, = struct.unpack_from("<H", d, o); o += 2
    if not 1 <= pc <= n_patterns_max:
        raise ValueError("pattern count")
    pats = []
    for _ in range(pc):
        rows, tracks, pid = struct.unpack_from("<BBH", d, o); o += 4
        if rows == 0 or tracks > 16:
            raise ValueError("pattern header")
        tr = []
        for _ in range(tracks):
            cells = []
            for r in range(rows):
                note, ins, vel, eff = struct.unpack_from("<BBBH", d, o); o += 5
                if note > 128:
                    raise ValueError("note")
                cells.append((r, note, ins, vel, eff))
            tr.append(cells)
        pats.append((pid, rows, tr))
    ic, = struct.unpack_from("<H", d, o); o += 2
    ids = {p[0] for p in pats}
    insts = []
    for _ in range(ic):
        ro, trk, pid = struct.unpack_from("<HBB", d, o); o += 4
        if pid not in ids or trk > 15:
            raise ValueError("instance")
        insts.append((ro, trk, pid))
    if not insts:
        raise ValueError("no instances")
    return dict(id=sid, bpm=bpm, name=name.decode(), patterns=pats, instances=insts, end=o)


def find(d):
    """All consistent synth blocks: (start, variant, instruments {index: values}, song)."""
    out = []
    st = 0
    while True:
        st = d.find(b"\x01", st)
        if st < 0 or st + 32 > len(d):
            break
        fl = d[st:st + 32]
        n = sum(fl)
        if max(fl) <= 1 and n:
            body = d[st + 32:st + 32 + P * n]
            for variant in ("ranged", "percent"):
                lim = [100 if variant == "percent" or not r else r for r in RANGE]
                if len(body) == P * n and all(body[j * n + i] <= lim[j] for j in range(P) for i in range(n)):
                    try:
                        song = parse_song(d, st + 32 + P * n)
                    except (ValueError, struct.error):
                        continue
                    idx = [i for i in range(32) if fl[i]]
                    ins = {i: [body[j * n + k] / lim[j] for j in range(P)] for k, i in enumerate(idx)}
                    out.append((st, variant, ins, song))
                    break
        st += 1
    return out


class Bits:
    """The older layout's bit reader (LSB first), bounded by the image: reading past the end raises ValueError."""

    def __init__(self, d, pos):
        self.d, self.p, self.bit, self.cur = d, pos, 8, 0

    def read(self, n):
        v = 0
        for i in range(n):
            if self.bit == 8:
                if self.p >= len(self.d):
                    raise ValueError("end of image")
                self.cur, self.p, self.bit = self.d[self.p], self.p + 1, 0
            v |= (self.cur >> self.bit & 1) << i
            self.bit += 1
        return v

    def array(self, count):
        lo, step, _, bits = self.read(32), self.read(32), self.read(32), self.read(32)
        if bits > 32:
            raise ValueError("packed array width")
        return [lo if bits == 0 else lo + self.read(bits) * step for _ in range(count)]


OLD_P = 99  # parameters per instrument in the older layout
OLD_GAIN = 98  # its last parameter: GainAmount


def parse_old(d, st):
    """The older-layout block at `st`: (instruments {slot: 99 bytes}, song). ValueError unless it parses throughout."""
    b = Bits(d, st)
    n = b.read(8)
    if not 1 <= n <= 32:
        raise ValueError("instrument count")
    ins = {}
    for _ in range(n):
        vals = [b.read(8) for _ in range(OLD_P)]
        slot = b.read(8)
        if slot >= 32 or slot in ins or max(vals) > 100:
            raise ValueError("instrument")
        ins[slot] = vals
    sid, bpm, ln = b.read(32), b.read(16), b.read(8)
    name = bytes(b.read(8) for _ in range(ln))
    if not (1 <= bpm <= 400 and all(32 <= c < 127 for c in name)):
        raise ValueError("song header")
    pc = b.read(16)
    if not 1 <= pc <= 1024:
        raise ValueError("pattern count")
    rows, tracks = b.array(pc), b.array(pc)
    if not all(1 <= r <= 256 for r in rows) or not all(t <= 16 for t in tracks):
        raise ValueError("pattern header")
    words = b.array(b.read(16))
    k, pats = 0, []
    for pid in range(pc):
        tr = []
        for _ in range(tracks[pid]):
            if k >= len(words):
                raise ValueError("cells")
            cells, skip = [], words[k] >> 24
            for r in range(rows[pid]):
                if skip:
                    skip -= 1
                    continue
                w = words[k]
                note, ins_, vel = w >> 16 & 255, (w >> 8 & 255) - 1, (w & 255) - 1
                if note > 128:
                    raise ValueError("note")
                cells.append((r, note, ins_ & 255, vel & 255, 0))
                if r != rows[pid] - 1:
                    k += 1
                    if k >= len(words):
                        raise ValueError("cells")
                    skip = words[k] >> 24
            k += 1
            tr.append(cells)
        pats.append((pid, rows[pid], tr))
    if k != len(words):
        raise ValueError("cells left over")
    ic = b.read(16)
    pi, ro, trk = b.array(ic), b.array(ic), b.array(ic)
    if not ic or any(p >= pc for p in pi) or any(t > 15 for t in trk):
        raise ValueError("instance")
    song = dict(id=sid, bpm=bpm, name=name.decode(), patterns=pats, instances=list(zip(ro, trk, pi)), end=b.p)
    return ins, song


def old_values(vals):
    """The current 129 parameters of an older-layout instrument: indices 0-97 as stored, byte 98 as GainAmount, the
    band-pass and notch switches and every effect slot off (that engine has neither), everything else 0."""
    p = [0.0] * P
    for j in range(OLD_GAIN):
        p[j] = vals[j] / 100
    p[NAMES.index("GainAmount")] = vals[OLD_GAIN] / 100
    p[NAMES.index("FltBPOn")] = p[NAMES.index("FltNTOn")] = 0.0
    return p


def find_old(d):
    """All consistent older-layout blocks: (start, "older", instruments {slot: values}, song)."""
    out = []
    for st in range(len(d) - 1):
        n = d[st]
        if not 1 <= n <= 32 or st + 1 + 100 * n + 7 > len(d):
            continue
        slots = d[st + 100:st + 1 + 100 * n:100]  # each record's last byte
        if max(slots) >= 32 or len(set(slots)) != n:
            continue
        h = st + 1 + 100 * n
        if not 1 <= d[h + 4] | d[h + 5] << 8 <= 400:
            continue
        try:
            ins, song = parse_old(d, st)
        except ValueError:
            continue
        out.append((st, "older", {i: old_values(v) for i, v in ins.items()}, song))
    return out


def s8(b):
    return b - 256 if b >= 128 else b


def to_e3prj(ins, s, names):
    L = ['<?xml version="1.0" encoding="UTF-8"?>', '<enigma version="3">', "  <songs>",
         f'    <song bpm="{s["bpm"]}" id="{s["id"]}" name={quoteattr(s["name"])}>', "      <patterns>"]
    for pid, rows, tracks in s["patterns"]:
        L += [f'        <pattern rowcount="{rows}" patternid="{pid}" trackcount="{len(tracks)}">', "          <tracks>"]
        for t in tracks:
            L.append("            <track>")
            L += [f'              <ne i="{s8(i)}" n="{n}" r="{r}" v="{s8(v)}" e="{e}"/>' for r, n, i, v, e in t if n or e]
            L.append("            </track>")
        L += ["          </tracks>", "        </pattern>"]
    L += ["      </patterns>", "      <patterninstances>"]
    L += [f'        <patterninstance rowoffset="{ro}" patternid="{pid}" track="{trk}"/>' for ro, trk, pid in s["instances"]]
    L += ["      </patterninstances>", "    </song>", "  </songs>", "  <instruments>"]
    for i, vals in ins.items():
        L += [f'    <instrument id="{i}" name={quoteattr(names.get(i, f"Instrument {i}"))}>', "      <parameters>"]
        L += [f'        <param value="{v:.6f}" name="{NAMES[j]}"/>' for j, v in enumerate(vals)]
        L += ["      </parameters>", "    </instrument>"]
    L += ["  </instruments>", "</enigma>", ""]
    return "\n".join(L)


def main():
    a = sys.argv[1:]
    if len(a) < 2:
        sys.exit(__doc__)
    names = {}
    if "--names" in a:
        k = a.index("--names")
        src = open(a[k + 1], encoding="latin1").read()
        names = {int(i): n for i, n in re.findall(r'<instrument id="(\d+)" name="([^"]*)"', src)}
        del a[k:k + 2]
    d = open(a[0], "rb").read()
    hits = find(d) or find_old(d)
    if len(hits) != 1:
        sys.exit(f"{a[0]}: {len(hits)} synth blocks found (need exactly one): {[hex(h[0]) for h in hits]}")
    st, variant, ins, song = hits[0]
    open(a[1], "w").write(to_e3prj(ins, song, names))
    notes = sum(1 for p in song["patterns"] for t in p[2] for c in t if 0 < c[1] < 128)
    print(f"{a[0]}: block at {st:#x} ({variant}), {len(ins)} instruments, {song['bpm']} bpm, "
          f"{len(song['patterns'])} patterns, {len(song['instances'])} instances, {notes} note cells -> {a[1]}")


if __name__ == "__main__":
    main()
