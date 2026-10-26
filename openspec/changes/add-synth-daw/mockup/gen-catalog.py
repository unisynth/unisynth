#!/usr/bin/env python3
"""Build catalog.json for the DAW mockup's instrument library from the local synth lab.

Inputs (never committed; paths are the lab defaults, override with env):
  GM_DLS     Windows gm.dls (default ~/Downloads/64k-tools/gmdls/gm.dls), read via `cargo run -p dls --example dump`
  SYNTH_LAB  ~/Downloads/64k-tools/synth-lab (ws/refs/*.bin, ws/*.bin, tunefish/patches/tf4programs/*.txt,
             tunefish/media/tf4modules/*.tfm)
Output: catalog.json and catalog.js (same data, for sandboxed frames) next to this script. Names and counts only (metadata); no sample data, no parameter values.
Run from the repo root: python3 openspec/changes/add-synth-daw/mockup/gen-catalog.py
"""
import glob
import json
import os
import struct
import subprocess
from collections import Counter

HOME = os.path.expanduser("~")
LAB = os.environ.get("SYNTH_LAB", f"{HOME}/Downloads/64k-tools/synth-lab")
GM = os.environ.get("GM_DLS", f"{HOME}/Downloads/64k-tools/gmdls/gm.dls")
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "catalog.json")
WS_KIND = ["Falcon", "Slaughter", "Thunder", "Scissor", "Leveller", "Crusher", "Echo", "Smasher", "Chamber",
           "Twister", "Cathedral", "Adultery", "Specimen"]
WS_INSTR = {"Falcon", "Slaughter", "Thunder", "Adultery", "Specimen"}


def gm_bank():
    env = dict(os.environ, PATH=f"{HOME}/.cargo/bin:" + os.environ["PATH"])
    tsv = subprocess.run(["cargo", "run", "-q", "-p", "dls", "--example", "dump", "--", GM],
                         capture_output=True, text=True, env=env, check=True).stdout
    ins, waves = [], 0
    for line in tsv.splitlines():
        p = line.split("\t")
        if p[0] == "ins":
            regions = [(r.split(":")[0], int(r.split(":")[1])) for r in p[7].split()] if len(p) > 7 else []
            ins.append({"name": p[4], "drums": p[1] != "0", "bank": int(p[2]), "prog": int(p[3]),
                        "regions": [[int(k.split("-")[0]), int(k.split("-")[1]), w] for k, w in regions]})
        elif p[0] == "wave":
            waves += 1
    return {"file": os.path.basename(GM), "instruments": ins, "waves": waves}


def ws_songs():
    out = []
    for f in sorted(glob.glob(f"{LAB}/ws/refs/*.bin")):
        b = open(f, "rb").read()
        tempo, = struct.unpack_from("<i", b, 0)
        n, = struct.unpack_from("<i", b, 16)
        o, devs = 20, []
        for _ in range(n):
            k = b[o]
            sz, = struct.unpack_from("<i", b, o + 1)
            name = WS_KIND[k] if k < len(WS_KIND) else f"kind {k}"
            entry = {"kind": name}
            if name == "Adultery" and sz >= 4:
                entry["wave"] = int(struct.unpack_from("<f", b, o + 5)[0]) - 1
            devs.append(entry)
            o += 5 + sz
        title = os.path.basename(f).rsplit(".", 2)[0].replace("_", " ")
        out.append({"song": title, "tempo": tempo,
                    "instruments": [d for d in devs if d["kind"] in WS_INSTR],
                    "effects": dict(Counter(d["kind"] for d in devs if d["kind"] not in WS_INSTR))})
    return out


def tf_programs():
    files = glob.glob(f"{LAB}/tunefish/patches/tf4programs/program*.txt")
    files.sort(key=lambda p: int(os.path.basename(p)[7:-4]))
    return [{"n": int(os.path.basename(f)[7:-4]), "name": open(f).readline().strip()} for f in files]


def main():
    cat = {
        "gm": gm_bank(),
        "tunefish": {"programs": tf_programs(),
                     "songs": [os.path.basename(f) for f in sorted(glob.glob(f"{LAB}/tunefish/media/tf4modules/*.tfm"))]},
        "wavesabre": ws_songs(),
        "repo": {
            "wavesabre-tests": ["falcon", "slaughter_automation", "slaughter_mono_legato", "thunder", "specimen_loop",
                                "adultery", "adultery_no_bank"],
            "tfsynth-tests": ["gen_spectrum", "gen_unison_inharmonic", "gen_drive_octave_slop", "gen_fixed_freq",
                              "gen_mono_glide", "noise_bandpass", "filter_lp_hp", "filter_bp_notch",
                              "env_adsr_routes", "lfo_sine", "lfo_ramp_up", "lfo_ramp_down", "lfo_square",
                              "lfo_random", "mod_wheel_self_mod"],
        },
    }
    json.dump(cat, open(OUT, "w"), separators=(",", ":"))
    # catalog.js: same data for sandboxed frames where fetch() of catalog.json is blocked (origin "null").
    open(OUT[:-2], "w").write("window.DAW_CATALOG=" + json.dumps(cat, separators=(",", ":")) + ";\n")
    print(f"{OUT}: {len(cat['gm']['instruments'])} GM instruments, {len(cat['tunefish']['programs'])} Tunefish "
          f"programs, {len(cat['wavesabre'])} WaveSabre songs "
          f"({sum(len(s['instruments']) for s in cat['wavesabre'])} instruments)")


if __name__ == "__main__":
    main()
