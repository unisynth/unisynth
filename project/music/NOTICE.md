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

## Released songs (local only, `released/`)

`released/` holds 25 WaveSabre songs taken from released 64K intros and executable music (2018-2026; Robo Outer Space Epos uses QM's fork device Screwdriver, played as the distortion's curve mode), extracted with
`tools/ws-extract` and written by `tools/ws-extract/make-demos.py`. **No licence is stated for any of them: the music
belongs to its authors.** The folder is git-ignored and is rebuilt locally; it is never committed or redistributed.
Each file's `meta` names the production, its authors and music credit (Demozoo), its release and party placing, a pouet
or Demozoo link, and says that it is an adapted version (converted to the unified synth, within 0.5 dB loudness and
~1 dB per octave band of the original engine). Track names are derived from instrument and notes, not the authors'.
The full list and the productions that could not be extracted are in `research/wavesabre-intros.md`.

It also holds three songs of The Black Lotus's 1997 64K intros, music by Probe: "Smokeout" (Jizz, 1st at Wired 1997)
and "over2" and "bluish" (Stash, 1st at The Party 1997), titles from Demozoo, `tbl-*.song.json`. They are converted
from the intros' XMs with their synthesized samples played by the unified synth's cauldron oscillator
(`research/tbl-mini-cauldron/tools/make_demos.py`); same rules: no licence, local only, adapted version.

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
