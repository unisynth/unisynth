#!/usr/bin/env python3
"""Fixtures for the Rust migration's cross-check (crates/unisynth/src/migrate/tests.rs): per instrument of the tracked
songs, its old pairs and the v3 pairs `layout.py` gives. `crosscheck.py full <out>` writes every instrument (opt-in
test, MIGRATE_CROSSCHECK=<out>); `crosscheck.py sample <out>` a small set that covers every format version and rule."""
import json, struct, sys
import layout as L

def bits(v):
    return struct.unpack("<I", struct.pack("<f", v))[0]

def line(old):
    op = L.pairs(old, L.OLD_DEFAULTS); np_ = L.pairs(L.migrate(old), L.NEW_DEFAULTS)
    f = lambda ps: " ".join(f"{i}:{bits(v):08x}" for i, v in ps)
    return f"{f(op)}|{f(np_)}"

def main():
    mode, out = sys.argv[1], sys.argv[2]
    lines, seen = [], set()
    for f in L.tracked():
        d = json.load(open(f))
        devs = [d] if f.endswith(".patch.json") else list(L.song_devices(d))
        for dev in devs:
            old = L.old_values_of(dev)
            if mode == "sample":
                # one instrument per distinct signature: format version, filter kinds, osc types/modes, env/lfo laws
                # each feature on its own: a new instrument is kept only if it brings an unseen (version, feature)
                v = L.era_settings(old)["v"]
                feats = {("f", L.kinds(old[L.OLD_INDEX[f"filt{s}.type"]], 9, 7, 0)) for s in range(1, 5)}
                feats |= {("o", L.kinds(old[L.OLD_INDEX[f"osc{s}.type"]], 5, 2, 0), L.raw(old[L.OLD_INDEX[f"osc{s}.s0"]]) & 7, L.raw(old[L.OLD_INDEX[f"osc{s}.s2"]]) & 1) for s in range(1, 4)}
                feats |= {("e", L.kinds(old[L.OLD_INDEX[f"env{s}.law"]], 2, 1, 0)) for s in range(1, 5)}
                feats |= {("l", L.choice(old[L.OLD_INDEX[f"lfo{s}.law"]], 2), L.raw(old[L.OLD_INDEX[f"lfo{s}.shape"]])) for s in range(1, 3)}
                feats |= {("k", L.raw(old[L.OLD_INDEX["voice.keysync"]]), L.raw(old[L.OLD_INDEX["voice.routing"]]))}
                new = {(v, x) for x in feats} - seen
                if not new:
                    continue
                seen |= new
                sig = None
            lines.append(line(old))
    open(out, "w").write("\n".join(lines) + "\n")
    print(len(lines), "instruments")

main()
