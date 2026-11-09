# Notices

## Songs in the DAW build (`site/daw/data/files/project/music/`)

The seven converted songs in this folder are **adapted versions** of works that ship with open-source synthesizers. They
are not the original recordings: each was converted to the unified synth by `synth-convert` (see `tools/demo-songs`)
and sounds close to, not identical with, its original. The same facts are in each file's `meta` block (author,
description, copyright, license, url, adapted_from). The songs follow the licence of the code they came from.

| File | Original | Author as stated upstream | Copyright | Licence |
|---|---|---|---|---|
| `global-groove.song.json` | `Global Groove.xrns` | not named (WaveSabre Team's test suite) | Copyright 2012-2021 WaveSabre Team | MIT |
| `elysian.song.json` | `Elysian.als` | logicoma (composer not named) | Copyright 2012-2021 WaveSabre Team | MIT |
| `engage.song.json` | `Engage.als` | logicoma (composer not named) | Copyright 2012-2021 WaveSabre Team | MIT |
| `eladamri.song.json` | `eladamri-Try_Stopping_abl11_triplet_issue.als` | eladamri (from the file name) | Copyright 2012-2021 WaveSabre Team | MIT |
| `no-remorse.song.json` | `no_remorse.tfm` | not named (Tunefish authors) | Brain Control | GPL-3.0-or-later |
| `tf4testsong.song.json` | `tf4testsong.tfm` | not named (Tunefish authors) | Brain Control | GPL-3.0-or-later |
| `turtles.song.json` | `zoom.tf4m` (Turtles all the way down, Revision 2013) | not named (Brain Control; the intro credits Hunta, Pap, Payne, Skyrunner, Strepto) | Brain Control | CC0-1.0 |

WaveSabre files come from <https://github.com/logicomacorp/WaveSabre> (commit 9245fb8, `Tests/WaveSabreConvertTests/Tests`),
Tunefish files from <https://github.com/paynebc/tunefish> (commit 7e48ce8, `media/tf4modules`), the Turtles song from
<https://github.com/enigmastudio/Enigma-Studio-4> (commit 5b82e1e, `demos/zoom`), released under CC0 1.0 (no rights
reserved; credit given anyway). It was made with the 2013 Tunefish 4 and is played by the final engine's equivalent, so
it sounds close to, not identical with, the intro. Composers the
repositories do not name are not guessed. `elysian` and `engage` are test projects named after logicoma's intros; the WaveSabre README lists those intros as made
with WaveSabre.

## Released songs (`released/`, pending the authors' permission)

`released/` holds 25 WaveSabre songs taken from released 64K intros and executable music (2018-2026; Robo Outer Space Epos uses QM's fork device Screwdriver, played as the distortion's curve mode), extracted with
`tools/ws-extract` and written by `tools/ws-extract/make-demos.py`. **No licence is stated for any of them: the music
belongs to its authors.** They are committed and published as adapted reference copies while each production's
authors are sent the link and asked for permission; a song is removed at any author's request.
Each file's `meta` names the production, its authors and music credit (Demozoo), its release and party placing, a pouet
or Demozoo link, and says that it is an adapted version (converted to the unified synth, within 0.5 dB loudness and
~1 dB per octave band of the original engine). Track names are derived from instrument and notes, not the authors'.
The full list and the productions that could not be extracted are in `research/wavesabre-intros.md`.
Exception: Teo gave permission (2026-11-09) for his music, so `hydra`, `lint-c` and `ouefvoke` are published with
his permission (see below).

It also holds three songs of The Black Lotus's 1997 64K intros, music by Probe: "Smokeout" (Jizz, 1st at Wired 1997)
and "over2" and "bluish" (Stash, 1st at The Party 1997), titles from Demozoo, `tbl-*.song.json`. They are converted
from the intros' XMs with their synthesized samples played by the unified synth's cauldron oscillator
(`research/tbl-mini-cauldron/tools/make_demos.py`); same rules: no licence, published pending permission, adapted version.

It also holds two songs of Brain Control's 64K intros, music by Skyrunner, made with Tunefish 4: "Aten Hotep"
(5th at Revision 2019) and "Naumachia Insianis" (2nd at Revision 2024), `aten-hotep.song.json` and
`naumachia-insianis.song.json`. They were read from the running intros' memory with `tools/tf-extract` and converted by
`synth-convert tfm` (within 0.1 dB loudness and 1.5 dB per octave band of the upstream Tunefish 4 player). **No licence
is stated: the music belongs to Brain Control.** Tunefish 4's GPL-3.0 covers the synth, not songs made with it. Same
rules: published pending Brain Control's permission, removed at their request, adapted version.

`released/v2/` holds 442 Farbrausch V2 songs, every `.v2m` of the local corpus that parses, once per checksum, written
by `tools/v2-extract/make-demos.py`:
- 13 carved by v2redux from farbrausch / .theprodukkt release executables
- 14 from farbrausch's fr_public repository, including the two version-6 acceptance songs
- 320 from the V2M collection of jgilje/v2m-player, which points to modland
- 10 by Teo from modland's own V2 directory (`pub/modules/V2/Teo/`, `teo-*.song.json`), published with his permission
- 1 carved by v2redux from Teo's release executable Ivory (Revision 2015, after `upx -d`), `teo-ivory.song.json`, published with his permission
- 84 more from modland's own V2 directory that the v2m-player collection lacks (by checksum; 6 other versions of
  songs already here and a second copy of one file are left out); a `coop-Other` folder credits both authors

Credits come only from where each file was found: the demo for carved songs, the repository path, or modland's
`Author--title` file name. Composers the source does not name are not guessed (7 songs say "not named"). 18 songs are
parity-checked against the v2redux reference; the others say they were not. Songs whose V2 file runs far past the
music end 4 s after the last note, as their description says. Same rules: no licence, published pending permission,
adapted version. `modland/KB--fr-013_flybye-loader.v2m` does not parse and is not included.

### Published with permission

Teo gave permission on 2026-11-09 to publish his music here as adapted versions and pointed to his V2 songs on modland
(<https://ftpmirror.infania.net/sites/modland/pub/modules/V2/Teo/>). This covers the WaveSabre songs `hydra`
(Hydra, 2020), `lint-c` (Lint:C by Digital Dynamite, 2019) and `ouefvoke` (Ouefvoke by Rebels, 2023), and the eleven V2
songs `v2/teo-*.song.json` (damptraktor, downdraft, f14a, himiko, k0rnmeister, spark, speedway, sunbeam,
tranceformator (bozon), unlzx from modland; Ivory, Revision 2015 executable music, carved from its release). The music belongs to Teo; each file's `meta` says so, and a song is removed at his
request.

## MIT (WaveSabre)

Copyright 2012-2021 WaveSabre Team

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the Software without restriction, including without limitation the
rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit
persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the
Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

## GPL-3.0-or-later (Tunefish)

`no-remorse.song.json` and `tf4testsong.song.json` are derived from Tunefish modules, which are distributed with the
Tunefish source under the GNU General Public License, version 3 or (at your option) any later version
(<https://www.gnu.org/licenses/gpl-3.0.html>; the full text is the `COPYING` file of the Tunefish repository). These
two files are separate data files: they are not linked into the 64k player or the MIT crates.

## Team logos (`site/logos/`)

Used to thank the teams whose synths unisynth learned from. The marks belong to their teams.

| File | Team | Source | How |
|---|---|---|---|
| `logicoma.svg` | Logicoma | the mark shown at the start of their 64k *Trashpanda* (TRSAC 2018), YouTube capture `gVHktHeYsfs`, frames 6–10.5 s | Rebuilt from measured geometry: ring r=100; three small rings r=7.5 at (±50.7, −39) and (0, 64.3), i.e. two above, one below. Measured spacing is ~103°/129°/129°, not an exact 120°. `currentColor`. (logicoma.com's own inline SVG is a different, scribbled wordmark; not used.) |
| `braincontrol.svg` | Brain Control | `braincontrol.org/images/bclogo.jpg` (500×500, via web.archive.org) | No vector published. Rebuilt as one square spiral (counter-clockwise outward, segment lengths 1,1,2,2,…,6 pitches) whose outer end runs into an outlined triangle arrow, rotated 45°; generated by a script, compared against the artwork. `currentColor`. |
| `tbl.svg` | The Black Lotus | GitHub org avatar `github.com/theblacklotus` (460×460) | No vector found. Lotus + orange eye traced with potrace; the 3×9 dot matrix rebuilt as exact circles from measured centres. White parts `currentColor`. |

Not found: Farbrausch (farbrausch.de shows a different joke logo on every visit, no fixed mark), Team210 (no group logo published; NR4's "210" box-glyph logo from *Cybertopia* is one logo-compo entry, not a group mark), Exceed. The site shows typographic `fr` / `210` placeholders.
Logos are the teams' marks, used here for credit only.

## GM sound bank (`site/daw/data/bank.dls`)

The Windows General MIDI bank `gm.dls` (Roland GS sound set, licensed to Microsoft), copied unchanged from a Windows
installation so instruments that use the GM.DLS sampler play as released. It is not covered by this repository's
licence; all rights stay with its owners. Published by the repository owner's decision; it will be removed on request.
