# Demo songs: attribution and licences

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

`released/` holds 26 WaveSabre songs taken from released 64K intros and executable music (2018-2026; Robo Outer Space Epos uses QM's fork device Screwdriver, played as the distortion's curve mode), extracted with
`tools/ws-extract` and written by `tools/ws-extract/make-demos.py`. **No licence is stated for any of them: the music
belongs to its authors.** They are committed and published as adapted reference copies while each production's
authors are sent the link and asked for permission; a song is removed at any author's request.
Each file's `meta` names the production, its authors and music credit (Demozoo), its release and party placing, a pouet
or Demozoo link, and says that it is an adapted version (converted to the unified synth, within 0.5 dB loudness and
~1 dB per octave band of the original engine). Track names are derived from instrument and notes, not the authors'.
The full list and the productions that could not be extracted are in `research/wavesabre-intros.md`.
Exception: Teo gave permission (2026-11-09) for his music, so `hydra`, `lint-c`, `ouefvoke` and `invot-teih-martx` are published with
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

It also holds two songs of Brain Control's 64K intros made with Tunefish 3 (Enigma Studio 3): "Transplant", music by
mad (2nd at The Ultimate Meeting 2011), and "Train Control", music by payne (4th at Revision 2012),
`transplant.song.json` and `train-control.song.json`. They were read from the released executables with
`tools/tf3-extract` (kkrunchy stub emulated, synth block found in the unpacked image) and converted by
`synth-convert e3prj` in player mode (within 0.07 dB loudness and 1.02 dB per octave band of the original Tunefish 3
player built from Enigma Studio 3's source release, and within 1.40 dB per band of captures of the released intros).
Transplant's track names are the authors' instrument names from the project in that source release. **No licence is
stated: the music belongs to Brain Control** (the source release states no licence either). Same rules: published
pending Brain Control's permission, removed at their request, adapted version.

A third such song is "Pandora", music by skyrunner (1st at Revision 2011), `pandora.song.json`. It was made with an
earlier Tunefish 3, read from the released executable with `tools/tf3-extract` (that engine's older layout), and
converted by `synth-convert e3prj --engine 2011` with that engine's laws. No source of that engine exists, so its only
reference is a capture of the released intro (within 3.89 dB per octave band). Its per-layer oscillator clipping and
oscillator offset are not reproduced, so the layered instruments sound quieter than in the intro. **No licence is
stated: the music belongs to Brain Control.** Same rules: published pending Brain Control's permission, removed at
their request, adapted version.

The DAW's library (published with the DAW as `site/daw/data/library.json`) also lists the 53 distinct presets of
Enigma Studio 3's Tunefish 3 bank (`binary/tf3programs`), converted to unified-synth devices. No licence is stated
for them either: they belong to Brain Control and are published on the same terms, pending permission.

`released/v2/` holds 442 Farbrausch V2 songs, every `.v2m` of the local corpus that parses, once per checksum, written
by `tools/v2-extract/make-demos.py`:
- 13 carved by v2redux from farbrausch / .theprodukkt release executables
- 14 from farbrausch's fr_public repository, including the two version-6 acceptance songs
- 320 from the V2M collection of jgilje/v2m-player, which points to modland
- 10 by Teo from modland's own V2 directory (`pub/modules/V2/Teo/`, `teo-*.song.json`), published with his permission
- 1 carved by v2redux from Teo's release executable Ivory (Revision 2015, after `upx -d`), `teo-ivory.song.json`, published with his permission
- 2 by Agoston (Teo and Vincenzo's joint alias) among modland's V2 songs below, `agoston-*.song.json`, published with their permission
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
(Hydra, 2020), `lint-c` (Lint:C by Digital Dynamite, 2019), `ouefvoke` (Ouefvoke by Rebels, 2023) and
`invot-teih-martx` (invot teih martx by Rebels, 2022), and the eleven V2
songs `v2/teo-*.song.json` (damptraktor, downdraft, f14a, himiko, k0rnmeister, spark, speedway, sunbeam,
tranceformator (bozon), unlzx from modland; Ivory, Revision 2015 executable music, carved from its release). The music belongs to Teo; each file's `meta` says so, and a song is removed at his
request.

Agoston is the joint alias of Teo and Vincenzo. They gave permission on 2026-11-09 for Agoston's music: the two V2
songs `v2/agoston-female.song.json` (female) and `v2/agoston-ag04-les-ters.song.json` (ag04 - les'ters, from modland's
`pub/modules/V2/Agoston/`). The music belongs to them; a song is removed at their request. Vincenzo's own Nightform
(`nightform`, Digital Dynamite and Lethargy, 2026) is not covered and stays pending.

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
