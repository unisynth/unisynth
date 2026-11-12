#!/usr/bin/env python3
"""Turn the songs of Mentor's 4k synth (Atrium, Elevated, Receptor) into DAW demo songs with meta (title, author,
description, copyright, licence, url, adapted_from) and derived track names, written to project/music/released/
(third-party music, committed while the authors are asked for permission; see project/music/NOTICE.md).

  python3 tools/elevated-parity/make-demos.py

Sources as tools/elevated-parity (ELEVATED_SRC: Atrium's music.asm + music.h; ELEVATED_REF: the Elevated source
release, cloned by build-ref.sh; RECEPTOR_SRC: Receptor's music.asm + music.h carved by build-ref.sh receptor). Uses target/release/{synth-convert,song}.
"""
import importlib.util, json, os, subprocess, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = os.path.join(ROOT, "project/music/released")
BIN = os.path.join(ROOT, "target/release")
SRC = os.environ.get("ELEVATED_SRC", os.path.expanduser("~/Downloads/zenekk/TBC - Atrium"))
REF = os.environ.get("ELEVATED_REF", "/tmp/elevated-ref")
RECEPTOR = os.environ.get("RECEPTOR_SRC", os.path.expanduser("~/Downloads/zenekk/TBC-Receptor"))
spec = importlib.util.spec_from_file_location("ws", os.path.join(ROOT, "tools/ws-extract/make-demos.py"))
ws = importlib.util.module_from_spec(spec); spec.loader.exec_module(ws)

# slug, title, groups, date, pouet id, demozoo id, placing, composer, other credits, source, licence, parity against the
# original code (tools/elevated-parity/report.md: loudness dB, worst octave band dB). Facts from pouet, the intros'
# nfo files and Elevated's music.asm header.
SONGS = [
    ("atrium", "Atrium", "TBC & Loonies", "March 2008", 50063, 4846, "won the PC 4k intro competition at Breakpoint 2008",
     "Puryx", "synth, object generator and rendering by Mentor, object modelling tool by Blueberry, modelling, design "
     "and direction by Lemmus (the intro's nfo)", os.path.join(SRC, "music.asm"), None, (0.14, 1.08)),
    ("elevated", "Elevated", "Rgba & TBC", "April 2009", 52938, 102, "won the PC 4k intro competition at Breakpoint "
     "2009 and the Scene.org Award for best 4k intro", "Puryx (Christian Rønde)", "visuals by iq, synth and "
     "optimization by Mentor (Rune L. H. Stubbe) (the intro's nfo and its source release)",
     os.path.join(REF, "elevated-src/src/music.asm"), "CC-BY-NC-SA-3.0", (-0.03, -0.51)),
    ("receptor", "Receptor", "TBC", "August 2008", 51449, 31385, "placed 2nd in the PC 4k intro competition at "
     "NVScene 2008 and was nominated for the Scene.org Award for best 4k intro", "Puryx", "code and design by Mentor "
     "(the intro's nfo)", os.path.join(RECEPTOR, "music.asm"), None, (0.22, 0.86)),
]

# what a song needs said beyond the common description
EXTRA = {
    "receptor": "No source was released: the song file was carved from the intro's unpacked executable "
    "(tools/elevated-parity/carve.py). Its synth adds glides (a flagged note slides linearly in frequency to the next "
    "note of its pattern), played here as pitch-bend automation. ",
}


def main():
    os.makedirs(OUT, exist_ok=True)
    for slug, title, groups, date, pid, dz, placing, composer, others, asm, lic, (ld, band) in SONGS:
        tmp = tempfile.mkdtemp()
        u = os.path.join(tmp, "u.song")
        subprocess.run([f"{BIN}/synth-convert", "elevated", asm, u], check=True, capture_output=True)
        plain = os.path.join(tmp, "s.json")
        subprocess.run([f"{BIN}/song", "import", u, "-o", plain], check=True, capture_output=True)
        song = json.load(open(plain))
        # a synth machine's track plays notes; a mixer machine's track only receives two others
        names, n_mix = [], 0
        for t in song["tracks"]:
            if t.get("receives"):
                n_mix += 1
                names.append(f"Mix {n_mix}")
            else:  # the instrument tells noise-only synths (no oscillator) and kicks (a pitch drop); else the notes do
                v = next(d["values"] for d in t["devices"] if d["kind"] == 64)
                names.append("Noise" if not any(k.endswith(".type") for k in v) else "Kick" if "drop.rate" in v else
                             ws.role(ws.track_notes(song, t)) or "Synth")
        count = {b: names.count(b) for b in names}
        seen, final = {}, []
        for b in names:
            seen[b] = seen.get(b, 0) + 1
            final.append(f"{b} {seen[b]}" if count[b] > 1 else b)
        synths = sum(1 for t in song["tracks"] if not t.get("receives"))
        notes = sum(len(ws.track_notes(song, t)) for t in song["tracks"])
        secs = song["length"] / 960 * 60 / song["tempo"]
        url = f"https://www.pouet.net/prod.php?which={pid}"
        terms = ("Elevated's source release states the Creative Commons Attribution-NonCommercial-ShareAlike 3.0 "
                 "Unported licence for its code and music, so this adapted version is under the same licence. "
                 if lic else "No licence is stated for the music: it belongs to its authors. ")
        desc = (
            f"The soundtrack of {title} by {groups} ({date}), a 4k intro that {placing} (pouet {pid}, Demozoo {dz}). "
            f"Music by {composer}; {others}. It is played by Mentor's 4k synthesizer: per synth three oscillator pairs, "
            "an envelope, a pitch drop and noise, through a tree of filter, delay, distortion and mixer machines. "
            f"{song['tempo']:g} BPM, {int(secs // 60)}:{int(secs % 60):02d}, {synths} synths, {notes} notes. "
            "This is an adapted version, not the original recording: the song file (music.asm) was converted by "
            "`synth-convert elevated` to the unified synth (pair oscillators, linear timed envelope, pitch drop; the "
            "machine tree as tracks, effect devices and mix buses) and imported with `song import` (notes rounded to "
            f"960 ticks per beat). Against the original synth code it differs by {ld:+.2f} dB loudness and at most "
            f"{abs(band):.2f} dB in any octave band, and it passes the timing check against a capture of the released "
            f"intro (tools/elevated-parity). {EXTRA.get(slug, '')}Track names are derived here from each track's notes; the song file does "
            f"not name them. {terms}It is published here as an adapted reference copy while the authors are asked "
            "for permission, and is removed at their request."
        )
        meta = {
            "title": title,
            "author": f"{composer.split(' (')[0]} ({groups})",
            "description": desc,
            "copyright": f"© {date[-4:]} {composer.split(' (')[0]}, {groups}",
            **({"license": lic} if lic else {}),
            "url": url,
            "adapted_from": f"{title} (4k intro by {groups}, {date}), {url}",
            "tracks": [{"name": n} for n in final],
        }
        mp = os.path.join(tmp, "meta.json")
        json.dump(meta, open(mp, "w"), ensure_ascii=False)
        dst = os.path.join(OUT, slug + ".song.json")
        subprocess.run([f"{BIN}/song", "import", u, "-o", dst, "--meta", mp], check=True)
        r = subprocess.run([f"{BIN}/song", "check", dst], capture_output=True, text=True)
        print(f"{slug:10} {len(final)} tracks: {', '.join(final)}\n  {r.stdout.strip() or r.stderr.strip()}")


if __name__ == "__main__":
    main()
