#!/usr/bin/env python3
"""Turn the converted 1998 softsynth XM soundtracks into DAW demo songs with meta (title, author, description,
copyright, url, adapted_from) and track names, written to project/music/released/ (third-party music, no licence
stated; committed while the authors are asked for permission, see project/music/NOTICE.md). Change
add-fudge-softsynth: Alien Sex Clone; the Quad songs join with add-poespack-softsynth.

  python3 tools/xm-gen/make-demos.py

Needs the received XMs in $ZENEKK_DIR (default ~/Downloads/zenekk; never committed) and maps/<intro>.json. Uses
target/release/{synth-convert,song} (`cargo build --release -p synth-convert -p song`). Facts from
research/xm-softsynth-intros.md (sections 1 and 6); nothing invented.
"""
import json, os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))  # tools/xm-gen
ROOT = os.path.dirname(os.path.dirname(HERE))
OUT = os.path.join(ROOT, "project/music/released")
BIN = os.path.join(ROOT, "target/release")
ZENEKK = os.environ.get("ZENEKK_DIR", os.path.expanduser("~/Downloads/zenekk"))
THIN = 2

# slug, intro (map), XM, facts
SONGS = [
    ("fudge-alien-sex-clone", "fudge-asc", "Fudge - Alien Sex Clone.xm", {
        "title": "Alien Sex Clone",
        "author": "Simon (Fudge)",
        "copyright": "© 1998 Fudge",
        "url": "https://www.pouet.net/prod.php?which=444",
        "adapted_from": "the soundtrack of Alien Sex Clone (64K intro by Fudge, The Party 1998), https://demozoo.org/productions/21494/",
        "note": "The soundtrack of Alien Sex Clone, Fudge's 64K intro that won the PC 64K intro competition at The Party 1998 "
                "(pouet 444, Demozoo 21494). Music by Simon (Demozoo 21494); Demozoo music 202313, \"Alien Seckse Clone (final "
                "version)\", credits Echo and Simn, and its XM shares the order list and 86 % of the note cells with this one.",
        "synth": "The intro synthesizes 22 of its 25 samples at start-up with two generators of its own, a sine tone with a "
                 "two-stage shaper and a sine drum with an accelerating sweep; here each is a live unified-synth "
                 "oscillator (Shaper tone, Sweep drum: the generators' own arguments, read from the running original by "
                 "tools/xm-gen), and the three samples the intro keeps as PCM play on a sampler (GSM-encoded).",
    }),
]


def run(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode:
        sys.exit(f"{' '.join(cmd)}\n{r.stderr}")
    return r


def main():
    os.makedirs(OUT, exist_ok=True)
    for slug, intro, xm_name, f in SONGS:
        xm = os.path.join(ZENEKK, xm_name)
        if not os.path.exists(xm):
            print(f"skip  {slug}: {xm} missing")
            continue
        tmp = tempfile.mkdtemp()
        blob, names = os.path.join(tmp, "u.song"), os.path.join(tmp, "names.json")
        # --thin 2: volume, pan and pitch-bend lanes within 2/255 of the per-tick steps (slides and fades as ramps)
        log = run([f"{BIN}/synth-convert", "xm", xm, os.path.join(HERE, "maps", intro + ".json"), blob, "--names", names, "--thin", str(THIN)]).stderr
        tracks = next((l for l in log.splitlines() if l.startswith("tracks ")), None)  # "tracks N phrase, M stored"
        if tracks is None:
            sys.exit(f"{slug}: synth-convert printed no 'tracks' line\n{log}")
        plain = os.path.join(tmp, "s.json")
        run([f"{BIN}/song", "import", blob, "-o", plain])
        song = json.load(open(plain))
        secs = song["length"] / song["ticks_per_beat"] * 60 / song["tempo"]
        desc = (
            f"{f['note']} {int(secs // 60)}:{int(secs % 60):02d}, {tracks.split(' ', 1)[1]} tracks. "
            f"This is an adapted version, not the original recording. {f['synth']} `synth-convert xm` replays the XM into "
            "notes and per-tick automation (volume, pan, pitch slides), mixed as OpenMPT mixes this file (it takes it for "
            "ModPlug Tracker 1.0-1.16: a linear pan law); `song import` cuts the lanes into patterns. Every generated sample "
            "matches the original's within one step of 16 bits; against the OpenMPT render of the original XM it is within "
            "0.2 dB loudness and 0.7 dB per octave band (tools/unisynth-parity/plain-xm.ts). Track names are derived here "
            "from the XM instrument and the speed class, not the author's. No licence is stated: the music belongs to its "
            "authors. It is published here as an adapted reference copy while its authors are asked for permission, and is "
            "removed at any author's request."
        )
        meta = {k: f[k] for k in ("title", "author", "copyright", "url", "adapted_from")}
        meta["description"] = desc
        meta["tracks"] = [{"name": n} for n in json.load(open(names))]
        mp = os.path.join(tmp, "meta.json")
        json.dump(meta, open(mp, "w"))
        dst = os.path.join(OUT, slug + ".song.json")
        run([f"{BIN}/song", "import", blob, "-o", dst, "--meta", mp])
        r = subprocess.run([f"{BIN}/song", "check", dst], capture_output=True, text=True)
        if r.returncode:
            os.remove(dst)
            sys.exit(f"{slug}: song check failed, {dst} removed\n{r.stdout}{r.stderr}")
        print(f"{slug:24} {r.stdout.strip().split(': ')[-1]} ({os.path.getsize(dst) // 1024} KB)")


if __name__ == "__main__":
    main()
