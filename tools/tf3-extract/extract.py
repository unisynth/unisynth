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
Pandora (2011) uses an older layout (about 100 parameters per instrument, instrument-major) and is not found here.
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
    hits = find(d)
    if len(hits) != 1:
        sys.exit(f"{a[0]}: {len(hits)} synth blocks found (need exactly one): {[hex(h[0]) for h in hits]}")
    st, variant, ins, song = hits[0]
    open(a[1], "w").write(to_e3prj(ins, song, names))
    notes = sum(1 for p in song["patterns"] for t in p[2] for c in t if 0 < c[1] < 128)
    print(f"{a[0]}: block at {st:#x} ({variant}), {len(ins)} instruments, {song['bpm']} bpm, "
          f"{len(song['patterns'])} patterns, {len(song['instances'])} instances, {notes} note cells -> {a[1]}")


if __name__ == "__main__":
    main()
