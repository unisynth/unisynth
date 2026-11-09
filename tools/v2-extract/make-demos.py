#!/usr/bin/env python3
"""Turn the V2 (Farbrausch) corpus into DAW songs with meta (title, author, description, copyright, url,
adapted_from), written to project/music/released/v2/ (third-party music, committed while the authors are asked for
permission; see project/music/NOTICE.md).

  python3 tools/v2-extract/make-demos.py [<v2songs dir>]   # default $V2_SONGS or ~/Downloads/64k-tools/synth-lab/v2songs

Reads survey.json (from `bun tools/v2-extract/v2.ts survey`) and converts every song that parses, once per sha256
(acceptance before fr_public before modland before modland_ftp). Credits come only from where each file was found: the v2redux carve
table (acceptance), the fr_public path, or the modland `Author--title` file name (v2m-player collection, or modland_ftp/:
files from modland's own V2 directory). Authors in PERMITTED gave permission; the others get LICENCE.
Composers that the source does not name are not guessed. Uses target/release/{synth-convert,song}.
"""
import json, os, re, subprocess, sys, tempfile
from urllib.parse import quote

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SONGS = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("V2_SONGS", os.path.expanduser("~/Downloads/64k-tools/synth-lab/v2songs"))
PLAYER = os.path.join(os.path.dirname(SONGS), "v2m-player/v2m")
OUT = os.path.join(ROOT, "project/music/released/v2")
BIN = os.path.join(ROOT, "target/release")
ORDER = {"acceptance": 0, "fr_public": 1, "modland": 2, "modland_ftp": 3, "release": 4}
# release/<Author>--<title>.v2m: carved by v2redux (`era carve`) from a release executable after `upx -d`
RELEASES = {
    "Teo--ivory": ("Ivory", "Teo", "https://demozoo.org/music/136370/",
                   "the release executable of Ivory (executable music, 4th at Revision 2015, scene.org `teo-ivory.zip`), "
                   "unpacked with `upx -d` and carved by v2redux"),
}
# modland_ftp/<Author>--<title>.v2m: files the author pointed us to in modland's own V2 directory (infania mirror)
MODLAND_FTP = "https://ftpmirror.infania.net/sites/modland/pub/modules/V2"

# acceptance songs: demo as in research/v2-intros.md (v2redux corpus/embedded/PROVENANCE.md); the two version-6
# files are fr_public `v2/v2m/`
DEMOS = {
    "fr08": ("fr-08 .the .product", "farbrausch"),
    "flybye": ("fr-013 flybye", "farbrausch"),
    "fr014": ("fr-014 mark & sweep", "farbrausch"),
    "farbomat": ("fr-minus-03 farbomat", "farbrausch"),
    "fr022_party": ("fr-022 ein.schlag (party version)", "farbrausch"),
    "fr019": ("fr-019 poem to a horse", "farbrausch"),
    "brullwurfel": ("fr-028 brüllwürfel", "farbrausch"),
    "candytron": ("fr-030 candytron", "farbrausch"),
    "fr022_final": ("fr-022 ein.schlag (final)", "farbrausch"),
    "fr024": ("fr-024", "farbrausch"),
    "fr027": ("fr-027 out of the blue", "farbrausch"),
    "fr029": ("fr-029", "farbrausch"),
    "kkrieger": (".kkrieger (beta)", ".theprodukkt"),
    "pzero_new": ("pzero (fr_public v2/v2m)", "farbrausch"),
    "v2_zeitmaschine_new": ("zeitmaschine (fr_public v2/v2m)", "farbrausch"),
}
# parity against v2redux (tools/unisynth-parity/v2-report.md, 2026-10-28): loudness dB, worst band dB @ Hz
PARITY = {
    "fr08": (-0.13, "-0.45 @ 63"), "flybye": (-0.12, "-0.55 @ 63"), "fr014": (0.05, "0.79 @ 63"),
    "fr019": (0.12, "-0.54 @ 1000"), "fr022_final": (-0.82, "-1.39 @ 1000"), "fr022_party": (0.08, "0.43 @ 2000"),
    "fr024": (-0.24, "-1.34 @ 63"), "fr027": (0.74, "1.72 @ 16000"), "fr029": (0.15, "0.41 @ 2000"),
    "candytron": (0.21, "0.81 @ 1000"), "brullwurfel": (0.03, "0.14 @ 4000"), "farbomat": (-0.90, "-1.42 @ 250"),
    "kkrieger": (-0.20, "1.07 @ 4000"), "pzero_new": (0.11, "0.30 @ 1000"), "v2_zeitmaschine_new": (0.02, "-0.49 @ 125"),
    "fr_public/werkkzeug3__data__debris__debris_ost.v2m": (-0.10, "-0.72 @ 500"),
    "modland/KB--fr-052_platinum.v2m": (-0.07, "-1.81 @ 500"),
    "modland/Jandor--stars.v2m": (0.42, "1.26 @ 250"),
}
LICENCE = ("No licence is stated: the music belongs to its authors. It is published here as an adapted reference copy "
           "while its authors are asked for permission, and is removed at any author's request.")
# authors who gave permission to publish their songs here (keyed by the song's author credit)
PERMITTED = {
    "Teo": ("Teo gave permission (2026-11-09) to publish his music here as adapted versions; the music belongs to "
            "him and is removed at his request."),
}


def slug(s):
    return re.sub(r"-+", "-", re.sub(r"[^a-z0-9]+", "-", s.lower())).strip("-")


def credits(file):
    """(title, author, url, where) from where the file was found; author None when the source names nobody."""
    kind, name = file.split("/", 1)
    stem = name[:-4]
    if kind == "acceptance":
        demo, by = DEMOS[stem]
        if stem in ("pzero_new", "v2_zeitmaschine_new"):
            url = f"https://github.com/farbrausch/fr_public/blob/master/v2/v2m/{name}"
            where = f"farbrausch's fr_public repository (`v2/v2m/{name}`)"
        else:
            url = "https://github.com/spheenik/v2redux/tree/master/corpus/embedded"
            where = f"the release executable of {demo}, carved by v2redux at its own format version (`corpus/embedded/{name}`)"
        return demo, by, url, where
    if kind == "fr_public":
        path = name.replace("__", "/")
        title = os.path.basename(path)[:-4].replace("_", " ")
        return title, "farbrausch", f"https://github.com/farbrausch/fr_public/blob/master/{path}", f"farbrausch's fr_public repository (`{path}`)"
    if kind == "release":
        return RELEASES[stem]
    if kind == "modland_ftp":
        # `Author--title` or `Author--coop-Other--title` = modland's `Author/[coop-Other/]title.v2m`, spaces as `_`
        parts = [p if p == "_unknown" else p.replace("_", " ") for p in stem.split("--")]
        path = "/".join(parts) + ".v2m"
        url = f"{MODLAND_FTP}/{quote(path, safe="/()'!,")}"
        author = None if parts[0] == "_unknown" else parts[0]
        if len(parts) == 3 and parts[1].startswith("coop-"):
            author = f"{author} and {parts[1][5:]}"
        return parts[-1], author, url, f"modland's V2 collection (`pub/modules/V2/{path}`)"
    # modland: the v2m-player collection; modland names files `Author--title`
    orig = next((f for f in (stem + ".v2mz", name) if os.path.exists(os.path.join(PLAYER, f))), name)
    url = f"https://github.com/jgilje/v2m-player/blob/master/v2m/{orig}"
    where = f"the V2M collection of jgilje/v2m-player (`v2m/{orig}`; its README points to modland)"
    if "--" in stem:
        a, t = stem.split("--", 1)
        return t.replace("_", " "), a.replace("_", " "), url, where
    return stem.replace("_", " "), None, url, where


TAIL_S = 4.0  # after the last note's end
HOLD_S = 16.0  # a note still held this long after the last note-on is a hanging note (no note-off before V2's end)


def last_note(song, hold):
    """End tick of the music: each note's end, cut at `hold` ticks after the last note-on."""
    clips = {c["id"]: c for c in song["clips"]}
    notes = []
    for t in song["tracks"]:
        for pl in t["placements"]:
            for n in clips[pl["clip"]].get("notes", []):
                if n[0] < pl["length"]:
                    notes.append((pl["start"] + n[0], pl["start"] + min(n[0] + n[1], pl["length"])))
    if not notes:
        return 0
    cap = max(s for s, _ in notes) + hold
    return max(min(e, cap) for _, e in notes)


def main():
    rows = [r for r in json.load(open(os.path.join(SONGS, "survey.json")))]
    rows.sort(key=lambda r: (ORDER[r["file"].split("/")[0]], r["file"]))
    os.makedirs(OUT, exist_ok=True)
    seen, slugs, skipped = {}, set(), []
    for r in rows:
        if r.get("error"):
            skipped.append(f"{r['file']}: does not parse (survey error {r['error']})")
            continue
        if r["sha256"] in seen:
            continue
        seen[r["sha256"]] = r["file"]
        dups = [x["file"] for x in rows if x["sha256"] == r["sha256"] and x["file"] != r["file"]]
        title, author, url, where = credits(r["file"])
        stem = r["file"].split("/", 1)[1][:-4]
        base = {"acceptance": "v2-" + stem, "fr_public": stem.split("__")[-1]}.get(r["file"].split("/")[0], stem)
        s = slug(base)
        while s in slugs:
            s += "-2"
        slugs.add(s)
        tmp = tempfile.mkdtemp()
        u = os.path.join(tmp, "u.bin")
        subprocess.run([f"{BIN}/synth-convert", "v2m", os.path.join(SONGS, r["file"]), u], check=True, capture_output=True)
        plain = os.path.join(tmp, "s.json")
        subprocess.run([f"{BIN}/song", "import", u, "-o", plain], check=True, capture_output=True)
        song = json.load(open(plain))
        tracks = [t for t in song["tracks"] if t["id"] != "master"]
        tpb = song["ticks_per_beat"]
        tps = tpb * song["tempo"] / 60
        # V2 plays to the file's last tick, which in looping or tracker-exported songs lies far past the music:
        # end the DAW song at the last note plus a release tail instead
        end = last_note(song, int(HOLD_S * tps)) + int(TAIL_S * tps)
        end = -(-end // tpb) * tpb
        trimmed = ""
        if end < song["length"]:
            v2len = song["length"] / tps
            if v2len - end / tps > 60:
                trimmed = (f"The V2 file's last tick is at {int(v2len // 60)}:{int(v2len % 60):02d}; the song here ends "
                           f"{TAIL_S:g} s after the last note (a note held more than {HOLD_S:g} s past the last note-on "
                           "is cut there). ")
            song["length"] = end
            for t in song["tracks"] + [song["master"]]:
                pls = [pl for pl in t.get("placements", []) if pl["start"] < end]
                for pl in pls:
                    pl["length"] = min(pl["length"], end - pl["start"])
                if "placements" in t:
                    t["placements"] = pls
                for ln in t.get("lanes", []):
                    pts = ln["points"]
                    keep = [p for p in pts if p[0] <= end]
                    after = [p for p in pts if p[0] > end]
                    if after and keep and keep[-1][0] < end:
                        (a, va), (b, vb) = keep[-1], after[0]
                        keep.append([end, va + (vb - va) * (end - a) / (b - a)])
                    ln["points"] = keep
        secs = song["length"] / tps
        key = stem if r["file"].startswith("acceptance/") else r["file"]
        if key in PARITY:
            dl, band = PARITY[key]
            par = (f"Against the v2redux reference render it differs by {dl:+.2f} dB loudness and at most {band} Hz "
                   "in any octave band (tools/unisynth-parity, A/B listening approved 2026-10-28).")
        else:
            par = ("This song was not parity-checked on its own; the V2 import passes on the 18 songs that were "
                   "(within 0.9 dB loudness and 1.8 dB per octave band of the v2redux reference).")
        desc = (
            f"{title}" + (f" by {author}" if author else "") + f", a Farbrausch V2 song (.v2m format version "
            f"{r['version']}) taken from {where}. "
            + (f"The same file is also at {', '.join(dups)} in the local corpus. " if dups else "")
            + ("" if author else "The source does not name the author. ")
            + f"{song['tempo']:g} BPM, {int(secs // 60)}:{int(secs % 60):02d}, {len(tracks)} tracks. " + trimmed
            + "This is an adapted version, not the original recording: converted by `synth-convert v2m` to the unified "
            "synth (instrument kind 64) and imported with `song import`. " + par + " " + PERMITTED.get(author, LICENCE)
        )
        meta = {
            "title": title,
            "author": author or "not named",
            "description": desc,
            "copyright": f"© {author or 'its authors'}",
            "url": url,
            "adapted_from": f"{title} (V2M, format version {r['version']}), {url}",
        }
        # the trimmed song back to binary, then imported with the meta (`song import` writes the canonical layout)
        json.dump(song, open(plain, "w"))
        u2 = os.path.join(tmp, "u2.bin")
        e = subprocess.run([f"{BIN}/song", "export", plain, "-o", u2], capture_output=True, text=True)
        if e.returncode:
            sys.exit(f"{s}: song export failed: {e.stderr.strip()}")
        mp = os.path.join(tmp, "meta.json")
        json.dump(meta, open(mp, "w"))
        dst = os.path.join(OUT, s + ".song.json")
        r2 = subprocess.run([f"{BIN}/song", "import", u2, "-o", dst, "--meta", mp], capture_output=True, text=True)
        if r2.returncode:
            sys.exit(f"{s}: song import failed: {r2.stderr.strip()}")
        c = subprocess.run([f"{BIN}/song", "check", dst], capture_output=True, text=True)
        if c.returncode:
            sys.exit(f"{s}: song check failed: {c.stderr.strip()}")
    print(f"{len(seen)} songs written to {OUT}")
    for x in skipped:
        print("skipped", x)


if __name__ == "__main__":
    main()
