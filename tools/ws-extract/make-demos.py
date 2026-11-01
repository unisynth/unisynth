#!/usr/bin/env python3
"""Turn extracted WaveSabre blobs into DAW demo songs with meta (title, author, description, copyright, url,
adapted_from) and derived track names, written to project/music/released/ (third-party music, committed
while the authors are asked for permission; see project/music/NOTICE.md).

  python3 tools/ws-extract/make-demos.py [<ws-intros dir>]   # default ~/Downloads/64k-tools/ws-intros

Needs in <dir>: out/<id>-<n>.bin (blobs from dump.py), catalog.json + details.json (Demozoo facts), and
synth-lab/ws/gm.dls for wave names. Uses target/release/{synth-convert,song}.
"""
import json, os, struct, subprocess, sys, statistics, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
WS = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser("~/Downloads/64k-tools/ws-intros")
DLS = os.path.expanduser("~/Downloads/64k-tools/synth-lab/ws/gm.dls")
OUT = os.path.join(ROOT, "project/music/released")
BIN = os.path.join(ROOT, "target/release")
KINDS = ["Falcon", "Slaughter", "Thunder", "Scissor", "Leveller", "Crusher", "Echo", "Smasher", "Chamber", "Twister",
         "Cathedral", "Adultery", "Specimen", "Screwdriver"]  # 13: qm210 fork waveshaper
SYNTH = {"Falcon": "FM synth", "Slaughter": "subtractive synth", "Thunder": "GSM sample player",
         "Adultery": "GM.DLS sampler", "Specimen": "GSM sampler"}

# slug, blob, parity (loudness dB, worst band 63 Hz-8 kHz dB) measured against the bit-exact WaveSabre port,
# and a hand-written note on what the release is. Facts from Demozoo / pouet; nothing invented.
SONGS = [
    ("trashpanda", "194842-0", (-0.06, 0.55), "Logicoma's party intro, which won the PC intro competition at TRSAC 2018. Released in October 2018, a few months before WaveSabre was open-sourced; it already uses the published song format."),
    ("atlas", "202450-0", (-0.04, 0.15), "A 64K intro that placed 3rd in the PC 64K intro competition at Revision 2019."),
    ("brainworm", "202448-0", (0.01, 0.69), "A 64K intro that placed 4th in the PC 64K intro competition at Revision 2019."),
    ("dope-on-wax", "202451-0", (0.06, 0.55), "Logicoma's 64K intro that won the PC 64K intro competition at Revision 2019, released with WaveSabre's open-sourcing."),
    ("secret-service-agency", "202446-0", (0.11, 0.60), "A 64K intro that placed 9th in the PC 64K intro competition at Revision 2019."),
    ("haec-diva", "202640-0", (0.00, 0.09), "32K executable music that placed 2nd in the executable music competition at Revision 2019."),
    ("makeshift-reprise", "202638-0", (0.06, 1.08), "32K executable music that placed 4th in the executable music competition at Revision 2019."),
    ("untie-the-knot", "202623-0", (0.06, 0.12), "32K executable music that placed 8th in the executable music competition at Revision 2019."),
    ("lint-c", "267663-0", (0.18, 1.07), "A 64K intro that won the 64K intro competition at Function 2019."),
    ("hydra", "277081-0", (0.04, 0.70), "Executable music that placed 4th in the executable music competition at Revision Online 2020."),
    ("smoothie", "280566-0", (0.02, 0.48), "A 64K intro that placed 2nd in the combined 4k/64k intro competition at Solskogen 2020."),
    ("gone-to-sleep", "280553-0", (0.04, 0.84), "Executable music that placed 2nd in the 16k executable music competition at Solskogen 2020."),
    ("miami-night-cruise", "292348-0", (-0.03, 0.83), "32K executable music that placed 6th in the executable music competition at Revision Online 2021."),
    ("multiplicity", "292347-0", (0.05, 0.21), "32K executable music that placed 4th in the executable music competition at Revision Online 2021."),
    ("astrophage", "304945-0", (-0.08, 0.81), "Logicoma's 64K invitation intro (January 2022), released outside a competition."),
    ("try-stopping", "322403-1", (-0.03, 0.83), "32K executable music that placed 6th in the executable music competition at Revision 2023. (The WaveSabre test suite has an earlier project file of it; that test song is eladamri.song.json.)"),
    ("ouefvoke", "326949-1", (-0.47, 0.68), "A 64K intro that shared 1st place in the PC 64K intro competition at Evoke 2023."),
    ("skranglefant", "342236-0", (-0.15, 0.91), "32K executable music that placed 3rd in the executable music competition at Revision 2024."),
    ("electromagination", "361283-0", (-0.42, 0.98), "32K executable music that placed 4th in the combined music competition at Comparade 2024."),
    ("breaking-waves", "371154-1", (-0.01, 0.96), "32K executable music that placed 6th in the executable music competition at Revision 2025."),
    ("indigo-shift", "371151-0", (0.05, 0.13), "32K executable music that placed 5th in the executable music competition at Revision 2025."),
    ("4o4-instance", "389813-0", (0.05, 0.26), "Executable music that placed 2nd in the executable music competition at Revision 2026."),
    ("nightform", "390011-0", (0.03, 0.92), "A 64K intro that placed 4th in the PC 64K intro competition at Revision 2026."),
    ("robo-outer-space-epos", "307397-0", (0.02, 0.18), "32K executable music that placed 8th in the executable music competition at Revision 2022. Made with QM's own WaveSabre fork: its four Screwdriver waveshapers (fork device 13, a three-segment power curve) play here as the unified distortion's curve mode."),
    ("hold-da-bass-up", "392727-0", (0.19, 0.68), "32K executable music that won the 32K executable music competition at Nova 2026."),
]


def wave_names():
    d = open(DLS, "rb").read()
    names = []

    def walk(o, end):
        while o + 8 <= end:
            cid, sz = d[o:o + 4], struct.unpack_from("<I", d, o + 4)[0]
            if cid in (b"LIST", b"RIFF"):
                if d[o + 8:o + 12] == b"wave":
                    names.append(None)
                walk(o + 12, o + 8 + sz)
            elif cid == b"INAM" and names and names[-1] is None:
                names[-1] = d[o + 8:o + 8 + sz].split(b"\0")[0].decode("latin1")
            o += 8 + sz + (sz & 1)

    walk(0, len(d))
    return names


def blob_tracks(m):
    """Original WaveSabre blob → (device kinds, per track: (device indices, midi lane))."""
    p = 16
    nd = struct.unpack_from("<i", m, p)[0]; p += 4
    kinds = []
    for _ in range(nd):
        kinds.append(m[p]); p += 5 + struct.unpack_from("<i", m, p + 1)[0]
    nl = struct.unpack_from("<i", m, p)[0]; p += 4
    for _ in range(nl):
        ne = struct.unpack_from("<i", m, p)[0]; p += 4
        for _ in range(ne):
            p += 5 + (0 if m[p + 4] & 0x80 else 1)
    nt = struct.unpack_from("<i", m, p)[0]; p += 4
    tracks = []
    for _ in range(nt):
        p += 4
        nr = struct.unpack_from("<i", m, p)[0]; p += 4 + 12 * nr
        ndv = struct.unpack_from("<i", m, p)[0]; p += 4
        devs = list(struct.unpack_from(f"<{ndv}i", m, p)); p += 4 * ndv
        lane, na = struct.unpack_from("<ii", m, p); p += 8
        for _ in range(na):
            p += 12 + 5 * struct.unpack_from("<iii", m, p)[2]
        tracks.append(devs)
    return kinds, tracks


def blob_notes(m):
    p = 16
    nd = struct.unpack_from("<i", m, p)[0]; p += 4
    for _ in range(nd):
        p += 5 + struct.unpack_from("<i", m, p + 1)[0]
    nl = struct.unpack_from("<i", m, p)[0]; p += 4
    n = 0
    for _ in range(nl):
        ne = struct.unpack_from("<i", m, p)[0]; p += 4
        for _ in range(ne):
            on = not m[p + 4] & 0x80
            n += on; p += 5 + (1 if on else 0)
    return n


def note_name(p):
    return ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"][p % 12] + str(p // 12 - 1)


def track_notes(song, t):
    clips = {c["id"]: c for c in song["clips"]}
    out = []
    for pl in t.get("placements", []):
        c = clips.get(pl["clip"])
        if not c:
            continue
        off, ln, cl = pl.get("offset", 0), pl["length"], c["length"]
        reps = range(0, ln + off, cl) if pl.get("loop") else [0]
        for r in reps:
            for n in c["notes"]:
                at = r + n[0] - off
                if 0 <= at < ln:
                    out.append([pl["start"] + at] + list(n[1:]))
    return out


DRUM_WAVES = ("KICK", "SNR", "SNARE", "CRASH", "RDCYM", "CYM", "OH", "CHH", "HH", "HAT", "TOM", "CLAP", "RIM", "COWB",
              "TAMB", "SHAK", "METRO", "MARCS", "CONGA", "BONGO", "TIMB", "GATSD", "STICK", "CABA", "SPLASH", "CHINA",
              "AGOGO", "WHIS", "GUIRO", "WBLK", "CUICA", "BD", "SD")

DRUM_PARTS = ("SNR", "SD60", "808CW", "HI_Q_", "CLAP", "CNGA", "BNGO", "GUIR", "CAST", "VBSLP", "808MC", "B_TAP", "SEA__", "CLAVE", "TRIAN")


def role(notes, wave=None):
    if wave:
        w = wave.upper().lstrip("0123456789")
        if w.startswith(DRUM_WAVES) or any(k in wave.upper() for k in DRUM_PARTS):
            return "Drum"
    if not notes:
        return None
    pitches = [n[2] for n in notes]
    distinct = len(set(pitches))
    med = statistics.median(pitches)
    short = statistics.median(n[1] for n in notes) <= 480
    starts = {}
    for n in notes:
        starts[n[0]] = starts.get(n[0], 0) + 1
    chordy = sum(1 for v in starts.values() if v >= 3) > len(starts) * 0.3
    if distinct <= 3 and short and not wave:
        return "Drum"
    if chordy:
        return "Chords"
    if med < 48:
        return "Bass"
    if med >= 72:
        return "High"
    return "Lead" if not chordy else "Chords"


def main():
    cat = {str(o["id"]): o for o in json.load(open(os.path.join(WS, "catalog.json")))}
    det = json.load(open(os.path.join(WS, "details.json")))
    waves = wave_names()
    os.makedirs(OUT, exist_ok=True)
    for slug, blob, (dl, band), note in SONGS:
        pid = blob.split("-")[0]
        c, d = cat[pid], det[pid]
        m = open(os.path.join(WS, "out", blob + ".bin"), "rb").read()
        kinds, btracks = blob_tracks(m)
        tmp = tempfile.mkdtemp()
        u = os.path.join(tmp, "u.bin")
        subprocess.run([f"{BIN}/synth-convert", "ws", os.path.join(WS, "out", blob + ".bin"), u], check=True, capture_output=True)
        plain = os.path.join(tmp, "s.json")
        subprocess.run([f"{BIN}/song", "import", u, "-o", plain], check=True, capture_output=True)
        song = json.load(open(plain))
        tracks = [t for t in song["tracks"] if t["id"] != "master"]

        # track names: role from the notes + the WaveSabre instrument (GM wave name for Adultery)
        names, used = [], {}
        for i, t in enumerate(tracks):
            insts = [KINDS[kinds[k]] for k in (btracks[i] if i < len(btracks) else []) if kinds[k] < len(KINDS) and KINDS[kinds[k]] in SYNTH]
            inst = insts[0] if insts else None
            if inst is None:
                base = "Bus"
            else:
                extra, wave = inst, None
                if inst == "Adultery":
                    dev = next(dv for dv in t["devices"] if dv["kind"] == 64)
                    s = int(dev["values"].get("osc1.sample", 0))
                    if dev["values"].get("osc1.s7") == 1.0 and 1 <= s <= len(waves):
                        wave = waves[s - 1]
                        extra = f"GM {wave}"
                r = role(track_notes(song, t), wave) or "Silent"
                base = f"{r} ({extra})"
            used[base] = used.get(base, 0) + 1
            names.append(base)
        seen = {}
        final = []
        for b in names:
            if used[b] > 1:
                seen[b] = seen.get(b, 0) + 1
                final.append(f"{b} {seen[b]}")
            else:
                final.append(b)

        notes = blob_notes(m)
        secs = song["length"] / 960 * 60 / song["tempo"]
        inst_count = {}
        for k in kinds:
            if k < len(KINDS):
                inst_count[KINDS[k]] = inst_count.get(KINDS[k], 0) + 1
        synths = ", ".join(f"{inst_count[k]} {k} ({SYNTH[k]})" for k in KINDS if k in SYNTH and k in inst_count)
        fx = ", ".join(f"{inst_count[k]} {k}" for k in KINDS if k not in SYNTH and k in inst_count)
        music = [x.split(":")[0] for x in d["credits"] if "Music" in x]
        by = c["by"]
        author = ", ".join(music) + f" ({by})" if music else by
        year = c["date"][:4]
        url = d.get("pouet") or f"https://demozoo.org/productions/{pid}/"
        gm = " Its Adultery instruments play waves from the Windows GM.DLS bank: they sound as intended only with that bank (GM_DLS=<path to gm.dls>); macOS's own GS bank has other waves." if "Adultery" in inst_count else ""
        desc = (
            f"The soundtrack of {d['title']} by {by} ({c['date']}). {note} "
            + (f"Music credited to {', '.join(music)} on Demozoo. " if music else "")
            + f"{song['tempo']:g} BPM, {int(secs // 60)}:{int(secs % 60):02d}, {len(tracks)} tracks, {notes} notes. "
            f"Instruments: {synths}. Effects: {fx}.{gm} "
            "This is an adapted version, not the original recording: the WaveSabre song was read from the running "
            "executable's memory (tools/ws-extract, Wine), converted by `synth-convert ws` to the unified synth "
            "(instrument kind 64, effects 65-73) and imported with `song import` (lanes cut into 2-, 4- or 8-bar "
            "patterns, notes rounded to 960 ticks per beat). Against the bit-exact WaveSabre port it differs by "
            f"{dl:+.2f} dB loudness and at most {band:.2f} dB in any octave band from 63 Hz to 8 kHz (16 kHz up to 3 dB "
            "brighter, a known difference of the unified engine). Track names are derived here from each track's "
            "instrument and notes; the executable does not store the authors' names. No licence is stated: the music "
            "belongs to its authors. It is published here as an adapted reference copy while its authors are asked for "
            "permission, and is removed at any author's request."
        )
        meta = {
            "title": d["title"],
            "author": author,
            "description": desc,
            "copyright": f"© {year} {by}",
            "url": url,
            "adapted_from": f"{d['title']} ({', '.join(c['types'])}, {c['date']}), {url}",
            "tracks": [{"name": n} for n in final],
        }
        mp = os.path.join(tmp, "meta.json")
        json.dump(meta, open(mp, "w"))
        dst = os.path.join(OUT, slug + ".song.json")
        subprocess.run([f"{BIN}/song", "import", u, "-o", dst, "--meta", mp], check=True)
        r = subprocess.run([f"{BIN}/song", "check", dst], capture_output=True, text=True)
        print(f"{slug:24} {r.stdout.strip().split(': ')[-1] or r.stderr.strip()}")


if __name__ == "__main__":
    main()
