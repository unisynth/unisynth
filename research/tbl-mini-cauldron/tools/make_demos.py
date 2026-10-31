#!/usr/bin/env python3
"""Turn the three converted TBL soundtracks (Jizz, Stash song 1, Stash BLUISH) into DAW demo songs with meta (title,
author, description, copyright, url, adapted_from) and track names, written to project/music/released/ (git-ignored:
third-party music, no licence stated).

  python3 research/tbl-mini-cauldron/tools/make_demos.py

Needs out/listen/<name>.xm (the intros' rebuilt XMs, `rebuild_xm.py`) and ref/patches_*.json. Uses
target/release/{synth-convert,song} (`cargo build --release -p synth-convert -p song`).
"""
import json, os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))  # research/tbl-mini-cauldron
ROOT = os.path.dirname(os.path.dirname(HERE))
OUT = os.path.join(ROOT, "project/music/released")
BIN = os.path.join(ROOT, "target/release")
THIN = 2

# slug, XM, patches, production facts (Demozoo productions 12 and 13, pouet 2 and 3); nothing invented
SONGS = [
    ("tbl-jizz-smokeout", "jizz", "patches_jizz.json", {
        "title": "Smokeout", "production": "Jizz", "date": "19 July 1997",
        "url": "https://www.pouet.net/prod.php?which=2", "demozoo": "https://demozoo.org/music/125061/",
        "note": "The soundtrack of Jizz, The Black Lotus's 64K intro that won the PC 64K intro competition at Wired 1997.",
    }),
    ("tbl-stash-over2", "stash_song1", "patches_stash.json", {
        "title": "over2", "production": "Stash", "date": "29 December 1997",
        "url": "https://www.pouet.net/prod.php?which=3", "demozoo": "https://demozoo.org/music/125063/",
        "note": "The first of the two songs of Stash, The Black Lotus's 64K intro that won the PC 64K intro competition at "
                "The Party 1997 (the XM the intro plays first, untitled in the file; Demozoo lists it as over2).",
    }),
    ("tbl-stash-bluish", "stash_bluish", "patches_stash.json", {
        "title": "bluish", "production": "Stash", "date": "29 December 1997",
        "url": "https://www.pouet.net/prod.php?which=3", "demozoo": "https://demozoo.org/music/125064/",
        "note": "The second song of Stash, The Black Lotus's 64K intro that won the PC 64K intro competition at The Party "
                "1997 (the XM titled BLUISH).",
    }),
]


def run(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode:
        sys.exit(f"{' '.join(cmd)}\n{r.stderr}")
    return r


def main():
    os.makedirs(OUT, exist_ok=True)
    for slug, name, patches, f in SONGS:
        xm = os.path.join(HERE, "out/listen", name + ".xm")
        if not os.path.exists(xm):
            print(f"skip  {slug}: {xm} missing (rebuild_xm.py)")
            continue
        tmp = tempfile.mkdtemp()
        blob, names = os.path.join(tmp, "u.song"), os.path.join(tmp, "names.json")
        # --thin 2: volume, pan and pitch-bend lanes within 2/255 of the per-tick steps (slides and fades as ramps)
        log = run([f"{BIN}/synth-convert", "xm", xm, os.path.join(HERE, "ref", patches), blob, "--names", names, "--thin", str(THIN)]).stderr
        tracks = next(l for l in log.splitlines() if l.startswith("tracks "))  # "tracks N phrase, M stored"
        plain = os.path.join(tmp, "s.json")
        run([f"{BIN}/song", "import", blob, "-o", plain])
        song = json.load(open(plain))
        secs = song["length"] / song["ticks_per_beat"] * 60 / song["tempo"]
        desc = (
            f"The soundtrack \"{f['title']}\" by Probe of The Black Lotus ({f['date']}). {f['note']} "
            f"{int(secs // 60)}:{int(secs % 60):02d}, {tracks.split(' ', 1)[1]} tracks. "
            "This is an adapted version, not the original recording. The intro synthesizes its instruments at start-up "
            "with TBL's mini-cauldron sample synth and plays them from an XM module; here every synthesized sample is a "
            "live unified-synth instrument (the cauldron oscillator, kind 64, with its comb, echo and doubler as track "
            "effects) and the XM's samples the intro keeps as PCM play on a sampler (GSM-encoded). `synth-convert xm` "
            "replays the XM (FT2 rules) into notes and per-tick automation (volume, pan, pitch slides), one track per "
            "record, XM channel and playback-speed class; `song import` cuts the lanes into patterns. Against the "
            "OpenMPT render of the original it is within 0.4 dB loudness and about 2 dB per octave band to 8 kHz (A/B listening "
            "approved 2026-10-30); known differences: reversed ping-pong loops attack sharper, extreme slow-downs lose "
            "the echo the original bakes in (a quiet gap near 21.5 s in over2). Track names are derived here from the XM "
            "sample, the record and the speed class, not the author's. No licence is stated: the music belongs to its "
            "authors and is kept as a local reference copy, never committed."
        )
        meta = {
            "title": f["title"],
            "author": "Probe (The Black Lotus)",
            "description": desc,
            "copyright": "© 1997 The Black Lotus",
            "url": f["url"],
            "adapted_from": f"{f['title']}, the soundtrack of {f['production']} (64K intro, {f['date']}), {f['demozoo']}",
            "tracks": [{"name": n} for n in json.load(open(names))],
        }
        mp = os.path.join(tmp, "meta.json")
        json.dump(meta, open(mp, "w"))
        dst = os.path.join(OUT, slug + ".song.json")
        run([f"{BIN}/song", "import", blob, "-o", dst, "--meta", mp])
        r = subprocess.run([f"{BIN}/song", "check", dst], capture_output=True, text=True)
        print(f"{slug:20} {(r.stdout.strip() or r.stderr.strip()).split(': ')[-1]} ({os.path.getsize(dst) // 1024} KB)")


if __name__ == "__main__":
    main()
