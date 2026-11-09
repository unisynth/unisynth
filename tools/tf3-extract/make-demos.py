#!/usr/bin/env python3
"""Turn Tunefish 3 songs extracted from Brain Control's intros into DAW demo songs with meta (title, author,
description, copyright, url, adapted_from) and derived track names, written to project/music/released/
(third-party music, committed while the authors are asked for permission; see project/music/NOTICE.md).

  python3 tools/tf3-extract/make-demos.py [<bc-intros dir>]   # default ~/Downloads/64k-tools/bc-intros

Needs <dir>/out/<slug>.e3prj (from unpack.py + extract.py; Transplant with --names from the source release's project). Uses target/release/{synth-convert,song}.
"""
import importlib.util, json, os, re, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BC = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser("~/Downloads/64k-tools/bc-intros")
OUT = os.path.join(ROOT, "project/music/released")
BIN = os.path.join(ROOT, "target/release")
spec = importlib.util.spec_from_file_location("ws", os.path.join(ROOT, "tools/ws-extract/make-demos.py"))
ws = importlib.util.module_from_spec(spec); spec.loader.exec_module(ws)

# slug, title, date, pouet id, note, composer, credit source, parameter storage, parity of the unified render against the
# original Tunefish 3 player (tools/tf3-parity reference: loudness dB, worst octave band 63 Hz-8 kHz dB) and against the
# YouTube capture of the released intro (worst octave band dB). Facts from pouet and the intros' readme.
SONGS = [
    ("transplant", "Transplant", "December 2011", 58257,
     "Brain Control's 64K intro that placed 2nd in the PC 64K intro competition at The Ultimate Meeting 2011.",
     "mad", "the intro's readme (\"mad/still\") and pouet", "ranged", (0.05, 1.02), 1.20),
    ("train-control", "Train Control", "April 2012", 59104,
     "Brain Control's 64K intro that placed 4th in the PC 64K intro competition at Revision 2012.",
     "payne", "the intro's readme and pouet", "percent", (0.07, 0.39), 1.40),
]


def played_instruments(prj):
    """Instruments with a note in an instanced pattern."""
    used_patterns = set(re.findall(r'<patterninstance [^>]*patternid="(\d+)"', prj))
    out = set()
    for pid, body in re.findall(r'<pattern [^>]*patternid="(\d+)"[^>]*>(.*?)</pattern>', prj, re.S):
        if pid in used_patterns:
            out |= {int(i) for i, n in re.findall(r'<ne i="(\d+)" n="(\d+)"', body) if 0 < int(n) < 128}
    return out


def main():
    os.makedirs(OUT, exist_ok=True)
    for slug, title, date, pid, note, composer, credit, storage, (dl, band), cap in SONGS:
        src = os.path.join(BC, "out", slug + ".e3prj")
        prj = open(src).read()
        notes = len(re.findall(r'<ne i="\d+" n="(?:[1-9]|[1-9]\d|1[01]\d|12[0-7])"', prj))
        tmp = tempfile.mkdtemp()
        u = os.path.join(tmp, "u.bin")
        subprocess.run([f"{BIN}/synth-convert", "e3prj", src, u, "4"], check=True, capture_output=True)
        plain = os.path.join(tmp, "s.json")
        subprocess.run([f"{BIN}/song", "import", u, "-o", plain], check=True, capture_output=True)
        song = json.load(open(plain))
        tracks = [t for t in song["tracks"] if t["id"] != "master"]
        # the authors' instrument names where a public project carries them (extract.py --names), else derived; the
        # converter makes one track per playing instrument in index order
        inames = dict((int(i), n) for i, n in re.findall(r'<instrument id="(\d+)" name="([^"]*)"', prj))
        played = sorted(played_instruments(prj) & set(inames))
        assert len(played) == len(tracks), (slug, played, len(tracks))
        names = [inames[i] if not re.fullmatch(r"Instrument \d+", inames[i]) else
                 f"{ws.role(ws.track_notes(song, t)) or 'Silent'} (Tunefish 3)" for i, t in zip(played, tracks)]
        count = {b: names.count(b) for b in names}
        seen, final = {}, []
        for b in names:
            seen[b] = seen.get(b, 0) + 1
            final.append(f"{b} {seen[b]}" if count[b] > 1 else b)
        secs = song["length"] / 960 * 60 / song["tempo"]
        names_note = ("Track names are the authors' instrument names from the project in Enigma Studio 3's source "
                      "release (the executable holds none)." if all(not n.endswith("(Tunefish 3)") for n in final) else
                      "Track names are derived here from each track's notes; the executable does not store the authors' names.")
        url = f"https://www.pouet.net/prod.php?which={pid}"
        desc = (
            f"The soundtrack of {title} by Brain Control ({date}). {note} Music by {composer} (credited in {credit}), "
            "made with Tunefish 3, the synthesizer of Brain Control's Enigma Studio 3. "
            f"{song['tempo']:g} BPM, {int(secs // 60)}:{int(secs % 60):02d}, {len(tracks)} tracks, {notes} note cells. "
            "This is an adapted version, not the original recording: the song and its instruments were read from the "
            "released executable (tools/tf3-extract: the kkrunchy stub emulated until the intro starts, then the "
            f"synth block found in the unpacked image; parameters stored {storage}), converted by `synth-convert e3prj` "
            "in player mode to the unified synth (points and PAD table oscillators, Tunefish 3 decimation stage, "
            "insert effects as track devices) and imported with `song import` (notes rounded to 960 ticks per beat). "
            f"Against the original Tunefish 3 player it differs by {dl:+.2f} dB loudness and at most {band:.2f} dB in "
            f"any octave band from 63 Hz to 8 kHz; against a capture of the released intro by at most {cap:.2f} dB per "
            f"octave band. The random per-note detune (slop) is not converted. {names_note} No licence is stated: the music belongs "
            "to its authors. It is published here as an adapted reference copy while Brain Control is asked for "
            "permission, and is removed at the authors' request."
        )
        meta = {
            "title": title,
            "author": f"{composer} (Brain Control)",
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
        print(f"{slug:15} {len(tracks)} tracks {r.stdout.strip().split(': ')[-1] or r.stderr.strip()}")


if __name__ == "__main__":
    main()
