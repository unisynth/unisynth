#!/usr/bin/env python3
"""Turn Tunefish 4 songs extracted from Brain Control's intros into DAW demo songs with meta (title, author,
description, copyright, url, adapted_from) and derived track names, written to project/music/released/
(third-party music, committed while the authors are asked for permission; see project/music/NOTICE.md).

  python3 tools/tf-extract/make-demos.py [<bc-intros dir>]   # default ~/Downloads/64k-tools/bc-intros

Needs <dir>/out/<slug>-0.tfm (from dump.py). Uses target/release/{synth-convert,song}.
"""
import importlib.util, json, os, struct, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BC = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser("~/Downloads/64k-tools/bc-intros")
OUT = os.path.join(ROOT, "project/music/released")
BIN = os.path.join(ROOT, "target/release")
spec = importlib.util.spec_from_file_location("ws", os.path.join(ROOT, "tools/ws-extract/make-demos.py"))
ws = importlib.util.module_from_spec(spec); spec.loader.exec_module(ws)

# slug, title, release, pouet id, note, music credit source, parity (loudness dB, worst band 63 Hz-8 kHz dB) of the unified render
# against the upstream Tunefish 4 player (tools/tf-parity reference). Facts from pouet and the intros' nfo.
SONGS = [
    ("aten-hotep", "Aten Hotep", "April 2019", 80993,
     "Brain Control's 64K intro that placed 5th in the PC 64K intro competition at Revision 2019.",
     "the intro's readme and pouet", (0.03, 1.48)),
    ("naumachia-insianis", "Naumachia Insianis", "March 2024", 96594,
     "Brain Control's 64K intro that placed 2nd in the PC 64K intro competition at Revision 2024.",
     "pouet", (-0.09, 0.79)),
]


def tfm_notes(m):
    n = struct.unpack_from("<H", m, 0)[0]
    counts = struct.unpack_from(f"<{n}H", m, 4)
    p = 4 + 2 * n + 4 + 112 * n + 4
    on = 0
    for c in counts:
        vels = m[p + 3 * c:p + 4 * c]
        on += sum(1 for v in vels if v)
        p += 4 * c
    return n, sum(1 for c in counts if c), on


def main():
    os.makedirs(OUT, exist_ok=True)
    for slug, title, date, pid, note, credit, (dl, band) in SONGS:
        src = os.path.join(BC, "out", slug + "-0.tfm")
        m = open(src, "rb").read()
        ninst, used_inst, notes = tfm_notes(m)
        tmp = tempfile.mkdtemp()
        u = os.path.join(tmp, "u.bin")
        subprocess.run([f"{BIN}/synth-convert", "tfm", src, u], check=True, capture_output=True)
        plain = os.path.join(tmp, "s.json")
        subprocess.run([f"{BIN}/song", "import", u, "-o", plain], check=True, capture_output=True)
        song = json.load(open(plain))
        tracks = [t for t in song["tracks"] if t["id"] != "master"]
        names = [f"{ws.role(ws.track_notes(song, t)) or 'Silent'} (Tunefish)" for t in tracks]
        count = {b: names.count(b) for b in names}
        seen, final = {}, []
        for b in names:
            seen[b] = seen.get(b, 0) + 1
            final.append(f"{b} {seen[b]}" if count[b] > 1 else b)
        secs = song["length"] / 960 * 60 / song["tempo"]
        url = f"https://www.pouet.net/prod.php?which={pid}"
        desc = (
            f"The soundtrack of {title} by Brain Control ({date}). {note} Music by Skyrunner (credited in {credit}), made with Tunefish 4, Brain Control's own synthesizer. "
            f"{song['tempo']:g} BPM, {int(secs // 60)}:{int(secs % 60):02d}, {len(tracks)} tracks, {notes} notes. "
            "This is an adapted version, not the original recording: the Tunefish 4 song (.tfm) was read from the "
            "running executable's memory (tools/tf-extract, Wine), converted by `synth-convert tfm` to the unified "
            "synth (instrument kind 64 with its insert effects as track devices, kinds 65-73) and imported with "
            "`song import` (lanes cut into 2-, 4- or 8-bar patterns, notes rounded to 960 ticks per beat). Against "
            f"the upstream Tunefish 4 player it differs by {dl:+.2f} dB loudness and at most {band:.2f} dB in any "
            "octave band from 63 Hz to 8 kHz; the random per-voice detune GenSlop is not converted and the wavetable "
            "phases come from a different random sequence. Track names are derived here from each track's notes; the "
            "executable does not store the authors' names. No licence is stated: the music belongs to its authors "
            "(Tunefish 4 itself is GPL-3.0, which does not cover songs made with it). It is published here as an "
            "adapted reference copy while Brain Control is asked for permission, and is removed at the authors' request."
        )
        meta = {
            "title": title,
            "author": "Skyrunner (Brain Control)",
            "description": desc,
            "copyright": f"© {date[-4:]} Brain Control",
            "url": url,
            "adapted_from": f"{title} (64K intro, {date}), {url}",
            "tracks": [{"name": n} for n in final],
        }
        mp = os.path.join(tmp, "meta.json")
        json.dump(meta, open(mp, "w"))
        dst = os.path.join(OUT, slug + ".song.json")
        subprocess.run([f"{BIN}/song", "import", u, "-o", dst, "--meta", mp], check=True)
        r = subprocess.run([f"{BIN}/song", "check", dst], capture_output=True, text=True)
        print(f"{slug:20} {ninst} instruments ({used_inst} used) {r.stdout.strip().split(': ')[-1] or r.stderr.strip()}")


if __name__ == "__main__":
    main()
