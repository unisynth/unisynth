#!/usr/bin/env python3
"""The v3 instrument layout (change unify-v2-algorithms, task 6.4) and a reference migration old -> v3.

The frozen design artifact: the index map, defaults and value rules of design.md "v3 layout (frozen)", as data and
code. It measures the freeze gate (pair counts per tracked song, chunk bytes, the squishy probe's rewritten songs) and
later cross-checks the Rust migration of task 6.5 (same pairs for every tracked song).

    layout.py map                       print the v3 index map (index, name, law, default)
    layout.py pairs [--json out]        pair counts and chunk bytes per tracked song (project/music/**), old vs v3
    layout.py blob <in.song> <out.song> rewrite a song blob's instrument chunks, automation indices and effect
                                        chunks to v3 (the probe build: same player code, v3 data)
    layout.py rust <old.rs> <v3.song> <out.rs>
                                        `synth-convert rust`'s file for the old blob with the v3 blob's section bytes

The old layout is `old-layout.tsv` (index, name, law, default, pre-1.3 name), dumped from the tree before the switch.
"""
import glob, json, os, re, struct, sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "../.."))

def f32(x):
    return struct.unpack("<f", struct.pack("<f", x))[0]

# ---------------------------------------------------------------------------------------------------------- old layout
OLD = []  # (name, law, default)
for line in open(os.path.join(HERE, "old-layout.tsv")):
    i, name, law, d, _ = line.rstrip("\n").split("\t")
    OLD.append((name, law, f32(float(d))))
OLD_COUNT = len(OLD)
OLD_INDEX = {n: i for i, (n, _, _) in enumerate(OLD)}
OLD_DEFAULTS = [d for _, _, d in OLD]

def choice(p, n):
    top = max(n - 1, 0)
    return min(int(min(max(p, 0.0), 1.0) * top + 0.5), top)

def inv_choice(i, n):
    return 0.0 if n < 2 else f32(i / (n - 1))

def kinds(p, old, later, none):
    c = choice(p, 2 * old - 1)
    if c % 2 == 0:
        return c // 2
    return old + c // 2 if c // 2 < later else none

def raw(p):
    return int(f32(128.0 * p))  # `v2_raw(p) as i32`

# ----------------------------------------------------------------------------------------------------------- v3 layout
# (name, law, default). Laws: "choice:n", "steps" (7-bit step / 128), "raw" (sample index), or the old law's name
# (unchanged meaning). Choice option lists are in design.md.

VOICE = [n for n, _, _ in OLD[:16]]
VSET = [("voice.amp_source", "choice:2", 0.0), ("voice.pan_stage", "choice:2", 0.0), ("voice.dc_filter", "choice:2", 0.0),
        ("voice.law", "choice:2", 0.0), ("voice.keysync", "choice:3", 0.0), ("voice.routing", "choice:2", 0.0),
        ("voice.balance", "steps", 0.5)]
OSC_TYPES = ["off", "phase mod", "saw/pulse", "wavetable", "sampler", "naive saw/tri", "naive pulse", "sine",
             "coloured noise", "fm", "aux in A", "aux in B", "clocked"]
FILTER_TYPES = ["off", "state-variable low", "state-variable high", "state-variable band", "state-variable notch",
                "4-pole low", "RBJ high", "RBJ band", "resonator notch", "state-variable all-pass", "4-pole high"]
LFO_SHAPES = ["sine", "ramp up", "ramp down", "square", "random", "triangle", "sample-and-hold", "pulse"]
STRIP_HEAD = ["strip.on", "strip.volume", "strip.drive_mode", "strip.drive_gain", "strip.drive_p1", "strip.drive_p2",
              "strip.send_reverb", "strip.send_delay", "strip.boost", "strip.order", "strip.send_aux_a", "strip.send_aux_b",
              "strip.recv_aux_a", "strip.recv_aux_b"]

def build_layout():
    L = []
    for n in VOICE:
        i = OLD_INDEX[n]
        L.append((n, OLD[i][1], OLD[i][2]))
    L += VSET
    for s in range(1, 4):
        for fld in ["type", "tracking", "coarse", "fine", "octave", "ratio", "ratio_fine", "fixed_hz", "gain", "mix", "pan",
                    "pm_out", "feedback"] + [f"s{k}" for k in range(9)]:
            n = f"osc{s}.{fld}"
            o = OLD[OLD_INDEX[n]]
            L.append((n, f"choice:{len(OSC_TYPES)}" if fld == "type" else o[1], 0.0 if fld == "type" else o[2]))
        L.append((f"osc{s}.sample", "raw", 0.0))
        L.append((f"osc{s}.combine", "choice:3", 0.0))   # add, multiply, replace
        L.append((f"osc{s}.law", "choice:2", 0.0))       # unisynth, 7-bit steps
    for n in ["noise.band_on", "noise.hp", "noise.lp", "noise.width"]:
        o = OLD[OLD_INDEX[n]]
        L.append((n, o[1], o[2]))
    for s in range(1, 5):
        L.append((f"filt{s}.type", f"choice:{len(FILTER_TYPES)}", 0.0))
        L.append((f"filt{s}.cutoff", "Cutoff", 1.0))
        L.append((f"filt{s}.resonance", "Unit", 0.0))
        L.append((f"filt{s}.form", "choice:2", 0.0))       # input / feedback resonance; 4-pole: feedback before / after the clip
        L.append((f"filt{s}.oversample", "choice:2", 0.0)) # interpolated / held; 4-pole: 1 / 2 steps
        L.append((f"filt{s}.law", "choice:2", 0.0))        # Hz, 7-bit steps
    for s in range(1, 5):
        L += [(f"env{s}.law", "choice:3", 0.0), (f"env{s}.attack", "EnvTime", 0.0), (f"env{s}.decay", "EnvTime", f32(0.3)),
              (f"env{s}.sustain", "Unit", 1.0), (f"env{s}.release", "EnvTime", f32(0.1)),
              (f"env{s}.curve", "choice:4", 0.0),          # by law, quadratic, power, exponential
              (f"env{s}.curve_slope", "Unit", 0.5), (f"env{s}.sustain_slope", "steps", 0.5), (f"env{s}.amplify", "steps", 1.0)]
    for s in range(1, 3):
        L += [(f"lfo{s}.shape", "choice:8", 0.0), (f"lfo{s}.rate", "LfoRate", f32(0.6)), (f"lfo{s}.depth", "Unit", 1.0),
              (f"lfo{s}.sync", "choice:3", 1.0),          # song time, continue, restart
              (f"lfo{s}.law", "choice:2", 0.0), (f"lfo{s}.phase", "steps", 0.0), (f"lfo{s}.polarity", "choice:3", 0.0),
              (f"lfo{s}.one_shot", "choice:2", 0.0)]
    def mod(s):
        o = OLD[OLD_INDEX[f"mod{s}.target"]]
        return [(f"mod{s}.source", "choice:17", 0.0), (f"mod{s}.target", o[1], 0.0), (f"mod{s}.amount", "ModAmount", 0.0),
                (f"mod{s}.operation", "choice:3", 0.0)]
    for s in range(1, 11):
        L += mod(s)
    for n in ["ctl.wheel", "ctl.bend", "ctl.bend_range", "drive.mode", "drive.gain", "drive.p1", "drive.p2"]:
        o = OLD[OLD_INDEX[n]]
        L.append((n, o[1], o[2]))
    L += [("quirk.exact_math", "choice:2", 0.0), ("quirk.integer_fm", "choice:2", 0.0), ("quirk.fixed_seed", "choice:2", 0.0),
          ("quirk.lcg_noise", "choice:2", 0.0), ("quirk.coarse_env", "choice:2", 0.0)]
    # the channel strip: one block (`is_channel` = a strip index), its most-set fields first
    for n in STRIP_HEAD:
        o = OLD[OLD_INDEX[n]]
        L.append((n, "choice:2" if n == "strip.on" else o[1], o[2]))
    L.append(("strip.dc_filter", "choice:2", 0.0))
    for n in [OLD[i][0] for i in range(244, 260)]:  # chorus ×7, compressor ×9
        o = OLD[OLD_INDEX[n]]
        L.append((n, o[1], o[2]))
    for s in range(11, 17):
        L += mod(s)
    for k in range(2, 8):
        o = OLD[OLD_INDEX[f"ctl.cc{k}"]]
        L.append((f"ctl.cc{k}", o[1], o[2]))
    for n in ["speech.on", "speech.select", "speech.pitch"]:
        o = OLD[OLD_INDEX[n]]
        L.append((n, o[1], o[2]))
    return L

NEW = build_layout()
COUNT = len(NEW)
NEW_INDEX = {n: i for i, (n, _, _) in enumerate(NEW)}
assert len(NEW_INDEX) == COUNT
NEW_DEFAULTS = [f32(d) for _, _, d in NEW]

# --------------------------------------------------------------------------------------------------------- migration
class MigrationError(Exception):
    pass

def era_settings(old):
    k = kinds(old[OLD_INDEX["voice.era"]], 7, 1, 6)
    v, new_math = (5, True) if k == 7 else (k, k >= 6)
    return dict(v=v, fm_osc=v >= 4, aux_osc=v >= 6, four_pole=v >= 6, full_keysync=v >= 6, dc_filters=v >= 6,
                chan_voice_sources=v >= 5, lcg_noise=v < 5, exact_math=not new_math, integer_fm=not new_math,
                fixed_seed=v < 6, coarse_env=v < 2)

# old filter kinds -> (v3 type, form, oversample, law)
FILTER_RULE = {0: (0, 0, 0, 0), 1: (1, 0, 0, 0), 2: (2, 0, 0, 0), 3: (3, 0, 0, 0), 4: (4, 0, 0, 0),
               5: (5, 0, 0, 0), 6: (6, 0, 0, 0), 7: (7, 0, 0, 0), 8: (8, 0, 0, 0),
               9: (1, 1, 1, 1), 10: (3, 1, 1, 1), 11: (2, 1, 1, 1), 12: (4, 1, 1, 1), 13: (9, 1, 1, 1),
               14: (5, 1, 1, 1), 15: (10, 1, 1, 1)}
NAIVE_TYPE = {1: 5, 2: 6, 3: 7, 4: 8, 5: 9, 6: 10, 7: 11}  # V2 mode -> v3 osc type
FRAME_SHAPES = [1, 5, 7, 0, 6]  # lfo::FRAME_SHAPES: ramp up, triangle, pulse, sine, sample-and-hold
VOICE_SOURCES = range(1, 7)  # env 1-4, lfo 1-2: mod sources a channel route skips before version 5
CHANNEL_TARGETS = set(78 + k for k in range(40, 64))  # `modmx::target::PARAMS` entries of the strip (ch::VOL on)

def target_id(p):
    return int(min(max(p, 0.0), 1.0) * 255.0 + 0.5)

def migrate(old):
    """Full old value array -> full v3 value array (`old` holds every old index, defaults filled in)."""
    g = lambda n: old[OLD_INDEX[n]]
    new = list(NEW_DEFAULTS)
    def put(n, x):
        new[NEW_INDEX[n]] = f32(x)
    era = era_settings(old)
    for n in VOICE:
        put(n, g(n))
    stage = choice(g("voice.stage"), 2) == 1
    if stage:
        put("voice.amp_source", 1.0); put("voice.pan_stage", 1.0); put("voice.law", 1.0)
        if era["dc_filters"]:
            put("voice.dc_filter", 1.0)
    ks = max(0, min(2, raw(g("voice.keysync"))))
    if ks == 2 and not era["full_keysync"]:
        ks = 1
    put("voice.keysync", inv_choice(ks, 3))
    put("voice.routing", 1.0 if raw(g("voice.routing")) == 2 else 0.0)
    put("voice.balance", g("voice.balance"))
    used = dict(sine=False, fm=False, noise=False)
    for s in range(3):
        o = f"osc{s+1}."
        for fld in ["tracking", "coarse", "fine", "octave", "ratio", "ratio_fine", "fixed_hz", "gain", "mix", "pan",
                    "pm_out", "feedback"] + [f"s{k}" for k in range(9)]:
            put(o + fld, g(o + fld))
        put(o + "sample", g(o + "sample"))
        t = kinds(g(o + "type"), 5, 2, 0)
        if t == 5:  # the naive generator: its mode becomes the type, ring the combine, law 7-bit
            mode = raw(g(o + "s0")) & 7
            if (mode >= 6 and not era["aux_osc"]) or (mode == 5 and not era["fm_osc"]):
                mode = 0
            ring = raw(g(o + "s2")) & 1
            put(o + "s0", 0.0); put(o + "s2", 0.0)
            if mode == 0:
                put(o + "type", 0.0)
            else:
                put(o + "type", inv_choice(NAIVE_TYPE[mode], len(OSC_TYPES)))
                put(o + "combine", inv_choice(1 if ring else (2 if mode >= 5 else 0), 3))
                put(o + "law", 1.0)
                used["sine"] |= mode == 3; used["fm"] |= mode == 5; used["noise"] |= mode == 4
        else:
            put(o + "type", inv_choice({0: 0, 1: 1, 2: 2, 3: 3, 4: 4, 6: 12}[t], len(OSC_TYPES)))
    for n in ["noise.band_on", "noise.hp", "noise.lp", "noise.width"]:
        put(n, g(n))
    for s in range(4):
        f = f"filt{s+1}."
        k = kinds(g(f + "type"), 9, 7, 0)
        if k >= 14 and not era["four_pole"]:
            k = 0
        t, form, ovs, law = FILTER_RULE[k]
        put(f + "type", inv_choice(t, len(FILTER_TYPES)))
        put(f + "cutoff", g(f + "cutoff")); put(f + "resonance", g(f + "resonance"))
        put(f + "form", form); put(f + "oversample", ovs); put(f + "law", law)
    frame_env = False
    for s in range(4):
        e = f"env{s+1}."
        law = kinds(g(e + "law"), 2, 1, 0)
        frame_env |= law == 2
        put(e + "law", inv_choice(law, 3))
        for fld in ["attack", "decay", "sustain", "release"]:
            put(e + fld, g(e + fld))
        if law == 2:
            put(e + "sustain_slope", g(e + "curve_slope"))
        else:
            put(e + "curve_slope", g(e + "curve_slope"))
        put(e + "amplify", g(e + "amplify"))
    for s in range(2):
        l = f"lfo{s+1}."
        frame = choice(g(l + "law"), 2) == 1
        if frame:
            st = max(0, min(7, raw(g(l + "shape"))))
            shape = FRAME_SHAPES[st] if st < 5 else 1
            sync = 2 if raw(g(l + "sync")) != 0 else 1
        else:
            shape = choice(g(l + "shape"), 5)
            sync = 2 if choice(g(l + "sync"), 2) == 1 else 0
        put(l + "shape", inv_choice(shape, 8)); put(l + "sync", inv_choice(sync, 3)); put(l + "law", 1.0 if frame else 0.0)
        put(l + "rate", g(l + "rate")); put(l + "depth", g(l + "depth")); put(l + "phase", g(l + "phase"))
        put(l + "polarity", inv_choice(max(0, min(2, raw(g(l + "polarity")))), 3))
        put(l + "one_shot", 1.0 if raw(g(l + "one_shot")) != 0 else 0.0)
    for s in range(1, 17):
        m = f"mod{s}."
        src = kinds(g(m + "source"), 10, 7, 0)
        if not era["chan_voice_sources"] and target_id(g(m + "target")) in CHANNEL_TARGETS and src in VOICE_SOURCES:
            continue  # a channel route on a voice source does nothing before version 5: the slot stays off
        put(m + "source", inv_choice(src, 17)); put(m + "target", g(m + "target")); put(m + "amount", g(m + "amount"))
        put(m + "operation", inv_choice(kinds(g(m + "operation"), 2, 1, 0), 3))
    for n in ["ctl.wheel", "ctl.bend", "ctl.bend_range", "drive.mode", "drive.gain", "drive.p1", "drive.p2"] + \
             [f"ctl.cc{k}" for k in range(2, 8)] + ["speech.on", "speech.select", "speech.pitch"]:
        put(n, g(n))
    for n, _, _ in NEW:
        if n.startswith("strip.") and n != "strip.dc_filter":
            put(n, g(n))
    strip = choice(g("strip.on"), 2) == 1
    if strip and era["dc_filters"]:
        put("strip.dc_filter", 1.0)
    # quirks: only where their code is reached
    drive_od = raw(g("drive.mode")) == 1 or (strip and raw(g("strip.drive_mode")) == 1)
    if era["exact_math"] and (used["sine"] or used["fm"] or drive_od):
        put("quirk.exact_math", 1.0)
    if era["integer_fm"] and used["fm"]:
        put("quirk.integer_fm", 1.0)
    if era["fixed_seed"] and used["noise"]:
        put("quirk.fixed_seed", 1.0)
    if era["lcg_noise"] and used["noise"]:
        put("quirk.lcg_noise", 1.0)
    if era["coarse_env"] and frame_env:
        put("quirk.coarse_env", 1.0)
    return new

def check_routes(old):
    """A route on a naive slot's mode or ring cannot be expressed (they become structural): a named error."""
    for s in range(1, 17):
        if kinds(old[OLD_INDEX[f"mod{s}.source"]], 10, 7, 0) == 0:
            continue
        tid = target_id(old[OLD_INDEX[f"mod{s}.target"]])
        for k in range(3):
            if tid in (4 + 15 * k + 6, 4 + 15 * k + 8) and kinds(old[OLD_INDEX[f"osc{k+1}.type"]], 5, 2, 0) == 5:
                raise MigrationError(f"mod{s} targets osc{k+1}'s naive mode or ring")

def pairs(values, defaults):
    return [(i, v) for i, (v, d) in enumerate(zip(values, defaults)) if f32(v) != d]

# ---------------------------------------------------------------------------------------------------------- chunk I/O
def index_bytes(i):
    return bytes([i]) if i < 240 else bytes([0xF0 + (i - 240) // 256, (i - 240) % 256])

def encode_pairs(ps):
    n = len(ps)
    out = bytearray(bytes([n]) if n < 255 else bytes([0xFF]) + struct.pack("<H", n))
    for i, v in ps:
        out += index_bytes(i) + struct.pack("<f", v)
    return bytes(out)

def decode_chunk(b):
    """Old instrument chunk -> (version, {index: value}, rest bytes: samples, text)."""
    ver = b[0]; p = 1
    n = b[p]; p += 1
    if n == 0xFF:
        n = struct.unpack_from("<H", b, p)[0]; p += 2
    vals = {}
    for _ in range(n):
        x = b[p]; p += 1
        if x >= 0xF0:
            x = 240 + 256 * (x - 0xF0) + b[p]; p += 1
        vals[x] = struct.unpack_from("<f", b, p)[0]; p += 4
    return ver, vals, b[p:]

def full_old(vals):
    a = list(OLD_DEFAULTS)
    for i, v in vals.items():
        a[i] = f32(v)
    return a

def migrate_chunk(b):
    ver, vals, rest = decode_chunk(b)
    old = full_old(vals)
    check_routes(old)
    return bytes([ver + 2]) + encode_pairs(pairs(migrate(old), NEW_DEFAULTS)) + rest

SPLIT = re.compile(r"\.(type|law|shape|sync|polarity|one_shot|operation|source|keysync|routing|s0|s2)$|^voice\.")

def lane_index(i):
    """A lane on an old index -> the v3 index of the same name. Lanes on split or re-encoded choices need the point-by-
    point value rules (task 6.5); the tracked songs have none, so here they are an error."""
    n = OLD[i][0]
    if n in NEW_INDEX and not SPLIT.search(n):
        return NEW_INDEX[n]
    raise MigrationError(f"lane on {n} needs a value rule")

def migrate_blob(b):
    """Rewrite a song blob: instrument chunks and their lanes' indices; effect chunks get the version marker."""
    out = bytearray(b[:16]); p = 16
    nd = struct.unpack_from("<i", b, p)[0]; out += b[p:p + 4]; p += 4
    kinds_of = []
    for _ in range(nd):
        kind = b[p]; size = struct.unpack_from("<i", b, p + 1)[0]; chunk = b[p + 5:p + 5 + size]; p += 5 + size
        kinds_of.append(kind)
        if kind == 64:
            chunk = migrate_chunk(chunk)
        elif 65 <= kind <= 73:
            chunk = bytes([3]) + chunk  # the effect chunk's version marker (design: "Effect chunk version")
        out += bytes([kind]) + struct.pack("<i", len(chunk)) + chunk
    nl = struct.unpack_from("<i", b, p)[0]; q = p + 4
    for _ in range(nl):
        ne = struct.unpack_from("<i", b, q)[0]; q += 4
        for _ in range(ne):
            note = b[q + 4]; q += 5 + (0 if note & 0x80 else 1)
    out += b[p:q]; p = q
    nt = struct.unpack_from("<i", b, p)[0]; out += b[p:p + 4]; p += 4
    for _ in range(nt):
        q = p + 4
        nr = struct.unpack_from("<i", b, q)[0]; q += 4 + 12 * nr
        ndv = struct.unpack_from("<i", b, q)[0]; devs = struct.unpack_from(f"<{ndv}i", b, q + 4); q += 4 + 4 * ndv
        q += 4  # lane
        out += b[p:q]; p = q
        na = struct.unpack_from("<i", b, p)[0]; out += b[p:p + 4]; p += 4
        for _ in range(na):
            dev, param, npts = struct.unpack_from("<3i", b, p)
            if kinds_of[devs[dev]] == 64:
                param = lane_index(param)
            out += struct.pack("<3i", dev, param, npts) + b[p + 12:p + 12 + 5 * npts]; p += 12 + 5 * npts
    out += b[p:]
    return bytes(out)

def sections(b):
    """The blob's (count, bytes) sections as `Song::sections` gives them: devices, lanes, tracks."""
    p = 16
    nd = struct.unpack_from("<i", b, p)[0]; d0 = p + 4; p = d0
    for _ in range(nd):
        p += 5 + struct.unpack_from("<i", b, p + 1)[0]
    devices = (nd, b[d0:p])
    nl = struct.unpack_from("<i", b, p)[0]; l0 = p + 4; p = l0
    for _ in range(nl):
        ne = struct.unpack_from("<i", b, p)[0]; p += 4
        for _ in range(ne):
            p += 5 + (0 if b[p + 4] & 0x80 else 1)
    lanes = (nl, b[l0:p])
    nt = struct.unpack_from("<i", b, p)[0]
    return [devices, lanes, (nt, b[p + 4:])]

# --------------------------------------------------------------------------------------------------- tracked songs
def song_devices(o):
    if isinstance(o, dict):
        if o.get("kind") == 64 and isinstance(o.get("values"), dict):
            yield o
        for v in o.values():
            yield from song_devices(v)
    elif isinstance(o, list):
        for v in o:
            yield from song_devices(v)

def old_values_of(dev):
    a = list(OLD_DEFAULTS)
    for n, v in dev["values"].items():
        if n not in OLD_INDEX:
            raise MigrationError(f"unknown parameter {n}")
        a[OLD_INDEX[n]] = f32(v)
    return a

def chunk_size(ps):
    return 1 + (1 if len(ps) < 255 else 3) + sum(len(index_bytes(i)) + 4 for i, _ in ps)

def tracked():
    return sorted(glob.glob(os.path.join(ROOT, "project/music/**/*.song.json"), recursive=True)) + \
        sorted(glob.glob(os.path.join(ROOT, "project/music/patches/*.patch.json")))

def pair_table():
    rows = []
    for f in tracked():
        d = json.load(open(f))
        devs = [d] if f.endswith(".patch.json") else list(song_devices(d))
        r = dict(file=os.path.relpath(f, ROOT), instruments=len(devs), old_pairs=0, new_pairs=0, old_bytes=0, new_bytes=0,
                 two_byte=0)
        for dev in devs:
            old = old_values_of(dev)
            check_routes(old)
            op, np_ = pairs(old, OLD_DEFAULTS), pairs(migrate(old), NEW_DEFAULTS)
            r["old_pairs"] += len(op); r["new_pairs"] += len(np_)
            r["old_bytes"] += chunk_size(op); r["new_bytes"] += chunk_size(np_)
            r["two_byte"] += sum(1 for i, _ in np_ if i >= 240)
        rows.append(r)
    return rows

def group_of(f):
    g = f.split("/")
    return "patches" if "patches" in g else "released/v2" if "v2" in g else "released" if "released" in g else "music"

def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else "map"
    if cmd == "map":
        for i, (n, law, d) in enumerate(NEW):
            print(f"{i}\t{n}\t{law}\t{d:g}")
        print(f"COUNT {COUNT}", file=sys.stderr)
    elif cmd == "pairs":
        rows = pair_table()
        if "--json" in sys.argv:
            json.dump(rows, open(sys.argv[sys.argv.index("--json") + 1], "w"), indent=1)
        groups = {}
        for r in rows:
            s = groups.setdefault(group_of(r["file"]), dict(songs=0, grew=0, instruments=0, old_pairs=0, new_pairs=0,
                                                            old_bytes=0, new_bytes=0, two_byte=0))
            s["songs"] += 1; s["grew"] += r["new_pairs"] > r["old_pairs"]
            for k in ["instruments", "old_pairs", "new_pairs", "old_bytes", "new_bytes", "two_byte"]:
                s[k] += r[k]
        for k, s in groups.items():
            print(k, s)
    elif cmd == "rust":
        src = open(sys.argv[2]).read(); b = open(sys.argv[3], "rb").read()
        for i, (_, sec) in enumerate(sections(b)):
            src, n = re.subn(rf"static S{i}: \[u8; \d+\] = \[[^\]]*\];", f"static S{i}: [u8; {len(sec)}] = {list(sec)};", src)
            assert n == 1
        open(sys.argv[4], "w").write(src)
    elif cmd == "blob":
        open(sys.argv[3], "wb").write(migrate_blob(open(sys.argv[2], "rb").read()))
    else:
        sys.exit(__doc__)

if __name__ == "__main__":
    main()
