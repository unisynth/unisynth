//! The Ronan speech stage (change add-v2-import, `us-speech`): V2's formant speech synthesizer, applied to the
//! instrument's voice sum (V2 runs it on channel 16). A phoneme text steers seven resonators (nasal, F1–F6), voicing,
//! aspiration, frication and bypass; `!` waits for a note-on, `_` for a note-off, digits stretch the next syllable.
//! Controller 4 selects a text (0–62) or the frame rate (63–127), controller 5 the pitch. Port of V2's `ronan.cpp` by
//! Tammo "kb" Hinrichs (public domain) after v2redux's portable `ronan.cpp` (CC0; its reset clears the whole
//! sequencer, as the original's `memset` does). Runs once per V2 frame ([`Ronan::tick`]) and per sample
//! ([`Ronan::process`]).

use crate::fm::{cosf, expf, exp2f, log2};

/// The decoded phoneme table (`phonemtab.h` of V2, public domain): per phoneme f1f f1b f2f f2b f3f f3b fnf a_voicing
/// a_aspiration a_frication a_bypass a_1 a_2 a_3 a_4 a_n a_56 duration rank.
pub const PHONEMES: [[f32; 19]; 69] = [
    [490.0, 60.0, 1480.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 64.0, 47.0, 40.0, -16.0, -1.0, 4.0, 2.0],
    [790.0, 130.0, 1780.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 61.0, 52.0, 45.0, -16.0, -1.0, 5.0, 2.0],
    [640.0, 60.0, 1600.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 59.0, 49.0, 43.0, -16.0, -1.0, 6.0, 2.0],
    [640.0, 60.0, 2020.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 56.0, 52.0, 45.0, -16.0, -1.0, 6.0, 2.0],
    [790.0, 60.0, 880.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 63.0, 43.0, 36.0, -16.0, -1.0, 15.0, 2.0],
    [490.0, 60.0, 820.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 59.0, 36.0, 31.0, -16.0, -1.0, 10.0, 2.0],
    [190.0, 60.0, 760.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 38.0, -16.0, -16.0, -16.0, -16.0, -1.0, 12.0, 26.0],
    [190.0, 60.0, 760.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 38.0, 63.0, 57.0, 52.0, -16.0, -1.0, 1.0, 29.0],
    [190.0, 60.0, 760.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, -16.0, -16.0, -16.0, -16.0, -16.0, -1.0, 0.0, 26.0],
    [190.0, 60.0, 1780.0, 90.0, 2680.0, 150.0, 270.0, 0.0, 60.0, 60.0, -19.0, -16.0, -16.0, -16.0, -16.0, -16.0, 45.0, 4.0, 23.0],
    [400.0, 60.0, 2020.0, 90.0, 2560.0, 150.0, 270.0, 0.0, 60.0, 60.0, -19.0, -16.0, 45.0, 56.0, 45.0, -16.0, 35.0, 8.0, 18.0],
    [190.0, 60.0, 1780.0, 90.0, 2680.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 45.0, -16.0, -16.0, -16.0, -16.0, 35.0, 8.0, 26.0],
    [280.0, 60.0, 1600.0, 90.0, 2560.0, 150.0, 270.0, 36.0, 0.0, 60.0, 51.0, 43.0, 45.0, 40.0, 42.0, -16.0, 45.0, 4.0, 20.0],
    [280.0, 60.0, 1600.0, 90.0, 2560.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 43.0, 45.0, 40.0, 42.0, -16.0, 25.0, 4.0, 20.0],
    [190.0, 60.0, 1780.0, 90.0, 2680.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 52.0, 52.0, 49.0, 59.0, -16.0, 15.0, 1.0, 29.0],
    [190.0, 60.0, 1780.0, 90.0, 2680.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 52.0, 42.0, 38.0, 49.0, -16.0, -1.0, 1.0, 26.0],
    [640.0, 60.0, 2020.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 56.0, 52.0, 45.0, -16.0, -1.0, 4.0, 2.0],
    [250.0, 60.0, 2320.0, 90.0, 3200.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 47.0, 50.0, 45.0, -16.0, -1.0, 7.0, 2.0],
    [490.0, 60.0, 1480.0, 90.0, 2500.0, 150.0, 270.0, 0.0, 0.0, 0.0, -19.0, -16.0, -16.0, -16.0, -16.0, -16.0, -1.0, 5.0, 31.0],
    [580.0, 60.0, 1420.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 59.0, 47.0, 40.0, -16.0, -1.0, 16.0, 2.0],
    [400.0, 60.0, 1420.0, 90.0, 2560.0, 150.0, 270.0, 0.0, 32.0, 54.0, 51.0, -16.0, 14.0, 14.0, -16.0, -16.0, 45.0, 12.0, 18.0],
    [190.0, 60.0, 1480.0, 90.0, 2620.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 49.0, -16.0, -16.0, -16.0, -16.0, -1.0, 12.0, 26.0],
    [190.0, 60.0, 1480.0, 90.0, 2620.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 49.0, 59.0, 54.0, 38.0, -16.0, -1.0, 1.0, 29.0],
    [190.0, 60.0, 1480.0, 90.0, 2620.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 49.0, 49.0, 43.0, 28.0, -16.0, -1.0, 2.0, 26.0],
    [490.0, 60.0, 1480.0, 90.0, 2500.0, 150.0, 270.0, 0.0, 60.0, 60.0, -19.0, 49.0, 50.0, 40.0, 36.0, -16.0, 25.0, 5.0, 9.0],
    [400.0, 60.0, 2080.0, 90.0, 2560.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 50.0, 49.0, 43.0, -16.0, -1.0, 6.0, 2.0],
    [310.0, 60.0, 2200.0, 90.0, 2920.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 49.0, 50.0, 45.0, -16.0, -1.0, 6.0, 2.0],
    [490.0, 60.0, 1480.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 64.0, 47.0, 40.0, -16.0, -1.0, 6.0, 2.0],
    [790.0, 60.0, 880.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 63.0, 43.0, 36.0, -16.0, -1.0, 6.0, 2.0],
    [190.0, 60.0, 1780.0, 90.0, 2680.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 45.0, -16.0, -16.0, -16.0, -16.0, 25.0, 4.0, 26.0],
    [280.0, 60.0, 2020.0, 90.0, 2560.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 43.0, 40.0, 50.0, 40.0, -16.0, 15.0, 3.0, 20.0],
    [190.0, 60.0, 1480.0, 90.0, 2620.0, 150.0, 270.0, 0.0, 60.0, 60.0, -19.0, -16.0, -16.0, -16.0, -16.0, -16.0, 25.0, 8.0, 23.0],
    [190.0, 60.0, 1480.0, 90.0, 2620.0, 150.0, 270.0, 0.0, 60.0, 60.0, -19.0, -16.0, 64.0, 64.0, 43.0, -16.0, 25.0, 1.0, 29.0],
    [190.0, 60.0, 1480.0, 90.0, 2620.0, 150.0, 270.0, 0.0, 60.0, 60.0, -19.0, -16.0, 54.0, 54.0, 33.0, -16.0, 15.0, 4.0, 23.0],
    [460.0, 60.0, 1480.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 50.0, 40.0, 40.0, 35.0, -16.0, -1.0, 8.0, 11.0],
    [460.0, 60.0, 940.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 50.0, 40.0, 40.0, 35.0, -16.0, -1.0, 8.0, 11.0],
    [480.0, 40.0, 1000.0, 170.0, 2200.0, 120.0, 360.0, 62.0, 0.0, 0.0, -19.0, 40.0, 44.0, 47.0, -16.0, 56.0, -1.0, 8.0, 11.0],
    [480.0, 40.0, 1780.0, 300.0, 2620.0, 260.0, 450.0, 62.0, 0.0, 0.0, -19.0, 49.0, 49.0, 49.0, 34.0, 56.0, -1.0, 8.0, 11.0],
    [480.0, 160.0, 820.0, 150.0, 2800.0, 100.0, 360.0, 52.0, 0.0, 0.0, -19.0, 34.0, 44.0, 49.0, 14.0, 56.0, -1.0, 8.0, 11.0],
    [610.0, 60.0, 880.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 61.0, 36.0, 29.0, -16.0, -1.0, 6.0, 2.0],
    [490.0, 60.0, 1480.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 64.0, 47.0, 40.0, -16.0, -1.0, 6.0, 2.0],
    [490.0, 60.0, 820.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 59.0, 36.0, 31.0, -16.0, -1.0, 6.0, 2.0],
    [370.0, 60.0, 1000.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 56.0, 42.0, 36.0, -16.0, -1.0, 4.0, 2.0],
    [370.0, 60.0, 1000.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 56.0, 42.0, 36.0, -16.0, -1.0, 6.0, 2.0],
    [490.0, 60.0, 820.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 59.0, 36.0, 31.0, -16.0, -1.0, 6.0, 2.0],
    [790.0, 60.0, 1300.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 61.0, 49.0, 42.0, -16.0, -1.0, 6.0, 2.0],
    [370.0, 60.0, 1000.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 56.0, 42.0, 36.0, -16.0, -1.0, 6.0, 2.0],
    [190.0, 60.0, 760.0, 90.0, 2500.0, 150.0, 270.0, 0.0, 60.0, 60.0, -19.0, -16.0, -16.0, -16.0, -16.0, -16.0, -1.0, 8.0, 23.0],
    [190.0, 60.0, 760.0, 90.0, 2500.0, 150.0, 270.0, 0.0, 60.0, 60.0, -19.0, 38.0, 63.0, 57.0, 52.0, -16.0, -1.0, 1.0, 29.0],
    [190.0, 60.0, 760.0, 90.0, 2500.0, 150.0, 270.0, 0.0, 60.0, 60.0, -19.0, 38.0, 52.0, 47.0, 42.0, -16.0, -1.0, 2.0, 23.0],
    [490.0, 60.0, 1480.0, 90.0, 2500.0, 150.0, 270.0, 0.0, 0.0, 0.0, -19.0, -16.0, -16.0, -16.0, -16.0, -16.0, -1.0, 6.0, 29.0],
    [280.0, 60.0, 1420.0, 90.0, 2560.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 43.0, 54.0, 50.0, 47.0, 40.0, -1.0, 0.0, 30.0],
    [490.0, 60.0, 1180.0, 90.0, 1600.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 56.0, 49.0, 49.0, -16.0, -16.0, -1.0, 11.0, 10.0],
    [490.0, 60.0, 1180.0, 90.0, 1600.0, 70.0, 270.0, 50.0, 0.0, 0.0, -19.0, 56.0, 49.0, 49.0, -16.0, -16.0, -1.0, 10.0, 10.0],
    [400.0, 200.0, 1720.0, 90.0, 2620.0, 220.0, 270.0, 0.0, 32.0, 60.0, -19.0, -16.0, 42.0, 42.0, 54.0, 50.0, 45.0, 12.0, 18.0],
    [400.0, 60.0, 2200.0, 90.0, 2560.0, 150.0, 270.0, 0.0, 60.0, 60.0, -19.0, -16.0, 45.0, 56.0, 45.0, 40.0, 25.0, 12.0, 18.0],
    [190.0, 60.0, 1780.0, 90.0, 2680.0, 150.0, 270.0, 0.0, 60.0, 60.0, -19.0, -16.0, -16.0, -16.0, -16.0, 50.0, 35.0, 6.0, 23.0],
    [400.0, 60.0, 1780.0, 90.0, 2680.0, 150.0, 270.0, 0.0, 60.0, 60.0, -19.0, -16.0, 40.0, 42.0, 36.0, 40.0, 25.0, 15.0, 18.0],
    [190.0, 60.0, 1780.0, 90.0, 2680.0, 150.0, 270.0, 0.0, 60.0, 60.0, -19.0, -16.0, -16.0, 52.0, 64.0, 40.0, -1.0, 1.0, 29.0],
    [190.0, 60.0, 1780.0, 90.0, 2680.0, 150.0, 270.0, 0.0, 60.0, 60.0, -19.0, -16.0, -16.0, 42.0, 54.0, 30.0, -1.0, 2.0, 23.0],
    [700.0, 60.0, 1360.0, 90.0, 2500.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 57.0, 45.0, 38.0, -16.0, -1.0, 6.0, 2.0],
    [250.0, 60.0, 880.0, 90.0, 2200.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 52.0, 31.0, 24.0, -16.0, -1.0, 9.0, 2.0],
    [280.0, 60.0, 1420.0, 90.0, 2560.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 43.0, 54.0, 50.0, 47.0, 40.0, -1.0, 4.0, 20.0],
    [190.0, 60.0, 760.0, 90.0, 2020.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 57.0, 42.0, 35.0, -16.0, -16.0, -1.0, 8.0, 10.0],
    [190.0, 60.0, 1480.0, 90.0, 2620.0, 150.0, 270.0, 0.0, 60.0, 60.0, -19.0, -16.0, 54.0, 54.0, 33.0, 30.0, -1.0, 12.0, 18.0],
    [250.0, 60.0, 2500.0, 90.0, 2980.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 64.0, 47.0, 52.0, 45.0, -16.0, -1.0, 7.0, 10.0],
    [280.0, 60.0, 1720.0, 90.0, 2560.0, 150.0, 270.0, 40.0, 20.0, 60.0, -19.0, 43.0, 38.0, 38.0, 50.0, 50.0, -1.0, 4.0, 20.0],
    [280.0, 60.0, 2020.0, 90.0, 2560.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 43.0, 40.0, 50.0, 40.0, 30.0, 25.0, 4.0, 20.0],
    [280.0, 60.0, 1720.0, 90.0, 2560.0, 150.0, 270.0, 62.0, 0.0, 0.0, -19.0, 43.0, 38.0, 38.0, 50.0, 30.0, 25.0, 4.0, 20.0],
];

/// Silence (the "end" phoneme).
const SIL: usize = 18;

/// Syllables: name and up to four phonemes (-1 = none).
const SYLS: [(&[u8], [i8; 4]); 52] = [
    (b"sil", [50, -1, -1, -1]),
    (b"ng", [38, -1, -1, -1]),
    (b"th", [57, -1, -1, -1]),
    (b"sh", [55, -1, -1, -1]),
    (b"dh", [12, 51, 13, -1]),
    (b"zh", [67, 51, 67, -1]),
    (b"ch", [9, 10, -1, -1]),
    (b"ih", [25, -1, -1, -1]),
    (b"eh", [16, -1, -1, -1]),
    (b"ae", [1, -1, -1, -1]),
    (b"ah", [60, -1, -1, -1]),
    (b"oh", [39, -1, -1, -1]),
    (b"uh", [42, -1, -1, -1]),
    (b"ax", [0, -1, -1, -1]),
    (b"iy", [17, -1, -1, -1]),
    (b"er", [19, -1, -1, -1]),
    (b"aa", [4, -1, -1, -1]),
    (b"ao", [5, -1, -1, -1]),
    (b"uw", [61, -1, -1, -1]),
    (b"ey", [2, 25, -1, -1]),
    (b"ay", [28, 25, -1, -1]),
    (b"oy", [41, 25, -1, -1]),
    (b"aw", [45, 46, -1, -1]),
    (b"ow", [40, 46, -1, -1]),
    (b"ia", [26, 27, -1, -1]),
    (b"ea", [3, 27, -1, -1]),
    (b"ua", [43, 27, -1, -1]),
    (b"ll", [35, -1, -1, -1]),
    (b"wh", [63, -1, -1, -1]),
    (b"ix", [0, -1, -1, -1]),
    (b"el", [34, -1, -1, -1]),
    (b"rx", [53, -1, -1, -1]),
    (b"h", [24, -1, -1, -1]),
    (b"p", [47, 48, 49, -1]),
    (b"t", [56, 58, 59, -1]),
    (b"k", [31, 32, 33, -1]),
    (b"b", [6, 7, 8, -1]),
    (b"d", [11, 14, 15, -1]),
    (b"g", [21, 22, 23, -1]),
    (b"m", [36, -1, -1, -1]),
    (b"n", [37, -1, -1, -1]),
    (b"f", [20, -1, -1, -1]),
    (b"s", [54, -1, -1, -1]),
    (b"v", [62, 51, 62, -1]),
    (b"z", [66, 51, 68, -1]),
    (b"l", [34, -1, -1, -1]),
    (b"r", [52, -1, -1, -1]),
    (b"w", [63, -1, -1, -1]),
    (b"q", [51, -1, -1, -1]),
    (b"y", [65, -1, -1, -1]),
    (b"j", [29, 30, 51, 30]),
    (b" ", [18, -1, -1, -1]),
];

/// Length of the syllable name `syl` matched case-insensitively at the start of `text`, or 0.
fn prefix(syl: &[u8], text: &[u8]) -> usize {
    if text.len() < syl.len() {
        return 0;
    }
    for (a, b) in syl.iter().zip(text) {
        if a | 0x20 != b | 0x20 {
            return 0;
        }
    }
    syl.len()
}

/// A 2-pole resonator's coefficients.
#[derive(Clone, Copy, Debug, Default)]
struct ResDef {
    a: f32,
    b: f32,
    c: f32,
}

impl ResDef {
    fn set(&mut self, f: f32, bw: f32, gain: f32, minus_pi_sr: f32, two_pi_sr: f32) {
        let r = expf(minus_pi_sr * bw);
        self.c = -(r * r);
        self.b = r * cosf(two_pi_sr * f) * 2.0;
        self.a = gain * (1.0 - self.b - self.c);
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Res {
    p1: f32,
    p2: f32,
}

impl Res {
    #[inline]
    fn tick(&mut self, d: &ResDef, x: f32) -> f32 {
        let y = d.a * x + d.b * self.p1 + d.c * self.p2;
        self.p2 = self.p1;
        self.p1 = y;
        y
    }
}

/// The interpolated part of the state (`syVRonan`): seven resonators and four amplitudes, as one flat array so a
/// frame interpolates every value the same way.
#[derive(Clone, Copy, Debug, Default)]
struct Frame([f32; 25]);

impl Frame {
    fn res(&self, i: usize) -> ResDef {
        ResDef { a: self.0[3 * i], b: self.0[3 * i + 1], c: self.0[3 * i + 2] }
    }
    fn set_res(&mut self, i: usize, d: ResDef) {
        self.0[3 * i..3 * i + 3].copy_from_slice(&[d.a, d.b, d.c]);
    }
}

const F4: f32 = 3200.0;
const F5: f32 = 4000.0;
const F6: f32 = 6000.0;
const BN: f32 = 100.0;
const B4: f32 = 200.0;
const B5: f32 = 500.0;
const B6: f32 = 800.0;
const NOISE_GAIN: f32 = 0.25;

fn lerp(a: f32, b: f32, x: f32) -> f32 {
    a + x * (b - a)
}

/// The speech synthesizer of one instrument.
#[derive(Clone, Debug)]
pub struct Ronan<'t> {
    cur: Frame,
    next: Frame,
    res: [Res; 7],
    lastin2: f32,
    /// The texts (`\0`-separated in the chunk), the one playing and the read position in it.
    texts: &'t [u8],
    base: usize,
    end: usize,
    pos: usize,
    pitch: f32,
    framerate: i32,
    nseed: u32,
    nout: f32,
    framecount: i32,
    spos: i32,
    scounter: i32,
    cursyl: usize,
    durfactor: i32,
    invdur: f32,
    curp1: usize,
    curp2: usize,
    wait4on: bool,
    wait4off: bool,
    hpb1: f32,
    hpb2: f32,
    /// Per-sample step of the frame interpolation, set by [`Ronan::tick`].
    delta: Frame,
    peq: ResDef,
    peq_res: Res,
    minus_pi_sr: f32,
    two_pi_sr: f32,
}

impl<'t> Ronan<'t> {
    /// `texts`: the speech texts, `\0`-separated (text 0 first).
    pub fn new(texts: &'t [u8], rate: f32) -> Self {
        let two_pi_sr = 2.0 * core::f32::consts::PI / rate;
        let mut peq = ResDef::default();
        peq.set(12_000.0, 4000.0, 2.0, -two_pi_sr * 0.5, two_pi_sr);
        let mut r = Ronan {
            cur: Frame::default(),
            next: Frame::default(),
            res: [Res::default(); 7],
            lastin2: 0.0,
            texts,
            base: 0,
            end: 0,
            pos: 0,
            pitch: 1.0,
            framerate: 3,
            nseed: 0,
            nout: 0.0,
            framecount: 0,
            spos: 4,
            scounter: 0,
            cursyl: 0,
            durfactor: 0,
            invdur: 0.0,
            curp1: SIL,
            curp2: SIL,
            wait4on: false,
            wait4off: false,
            hpb1: 0.0,
            hpb2: 0.0,
            delta: Frame::default(),
            peq,
            peq_res: Res::default(),
            minus_pi_sr: -two_pi_sr * 0.5,
            two_pi_sr,
        };
        r.reset();
        r.select(0);
        r
    }

    /// V2's `reset`: the whole sequencer and DSP state back to silence (texts, pitch, frame rate stay).
    fn reset(&mut self) {
        let keep = (self.texts, self.pitch, self.framerate, self.peq, self.minus_pi_sr, self.two_pi_sr);
        *self = Ronan {
            cur: Frame::default(),
            next: Frame::default(),
            res: [Res::default(); 7],
            lastin2: 0.0,
            texts: keep.0,
            base: 0,
            end: 0,
            pos: 0,
            pitch: keep.1,
            framerate: keep.2,
            nseed: 0,
            nout: 0.0,
            framecount: 0,
            spos: 4,
            scounter: 0,
            cursyl: 0,
            durfactor: 0,
            invdur: 0.0,
            curp1: SIL,
            curp2: SIL,
            wait4on: false,
            wait4off: false,
            hpb1: 0.0,
            hpb2: 0.0,
            delta: Frame::default(),
            peq: keep.3,
            peq_res: Res::default(),
            minus_pi_sr: keep.4,
            two_pi_sr: keep.5,
        };
        let (mut a, mut b) = (Frame::default(), Frame::default());
        self.set_frame(SIL, SIL, 0.0, &mut a);
        self.set_frame(SIL, SIL, 0.0, &mut b);
        (self.cur, self.next) = (a, b);
    }

    /// Plays text `i` from its start (an empty text when there is none).
    fn select(&mut self, i: usize) {
        let mut start = 0;
        for _ in 0..i {
            match self.texts[start..].iter().position(|&c| c == 0) {
                Some(p) => start += p + 1,
                None => {
                    start = self.texts.len();
                    break;
                }
            }
        }
        let start = start.min(self.texts.len());
        let end = self.texts[start..].iter().position(|&c| c == 0).map_or(self.texts.len(), |p| start + p);
        (self.base, self.pos, self.end) = (start, start, end);
    }

    fn at(&self, k: usize) -> u8 {
        if self.pos + k < self.end { self.texts[self.pos + k] } else { 0 }
    }

    fn db2lin(a: f32, b: f32, x: f32) -> f32 {
        exp2f((lerp(a, b, x) - 70.0) / 6.0)
    }

    fn set_frame(&self, p1: usize, p2: usize, x: f32, dest: &mut Frame) {
        let (a, b) = (&PHONEMES[p1], &PHONEMES[p2]);
        // per resonator: frequency, bandwidth, amplitude (field indices of the table)
        let f = |p: &[f32; 19], i: usize| [p[6], p[0], p[2], p[4], F4, F5, F6][i];
        let bw = |p: &[f32; 19], i: usize| [BN, p[1], p[3], p[5], B4, B5, B6][i];
        let amp = |p: &[f32; 19], i: usize| [p[15], p[11], p[12], p[13], p[14], p[16], p[16]][i];
        for i in 0..7 {
            let mut d = ResDef::default();
            d.set(lerp(f(a, i), f(b, i), x) * self.pitch, lerp(bw(a, i), bw(b, i), x), Self::db2lin(amp(a, i), amp(b, i), x), self.minus_pi_sr, self.two_pi_sr);
            dest.set_res(i, d);
        }
        dest.0[21] = Self::db2lin(a[7], b[7], x);
        dest.0[22] = Self::db2lin(a[8], b[8], x);
        dest.0[23] = Self::db2lin(a[9], b[9], x);
        dest.0[24] = Self::db2lin(a[10], b[10], x);
    }

    fn noise(&mut self) -> f32 {
        self.nseed = self.nseed.wrapping_mul(196_314_165).wrapping_add(907_633_515);
        let v = f32::from_bits((self.nseed >> 9) | 0x4000_0000);
        self.nout = (v - 3.0) + 0.75 * self.nout;
        self.nout * NOISE_GAIN
    }

    pub fn note_on(&mut self) {
        self.wait4on = false;
    }

    pub fn note_off(&mut self) {
        self.wait4off = false;
    }

    /// Controller 4 (text 0–62, or frame rate `v − 63`) and 5 (pitch).
    pub fn set_ctl(&mut self, ctl: u32, val: u32) {
        match ctl {
            4 if val < 63 => {
                self.reset();
                self.select(val as usize);
            }
            4 => self.framerate = val as i32 - 63,
            5 => self.pitch = exp2f((val as f32 - 64.0) / 128.0),
            _ => {}
        }
    }

    /// One V2 frame of the phoneme sequencer (`ronanCBTick`) for a frame of `len` samples: the next frame's values
    /// and the per-sample step towards them.
    pub fn tick(&mut self, len: usize) {
        self.step();
        let mul = 1.0 / len.max(1) as f32;
        for k in 0..25 {
            self.delta.0[k] = (self.next.0[k] - self.cur.0[k]) * mul;
        }
    }

    fn step(&mut self) {
        if self.wait4off || self.wait4on {
            return;
        }
        if self.framecount <= 0 {
            self.framecount = self.framerate;
            if self.scounter <= 0 {
                self.spos += 1;
                if self.spos >= 4 || SYLS[self.cursyl].1[self.spos as usize] == -1 {
                    if self.at(0) == 0 {
                        // empty text (or its end): silence, then from the start again
                        self.durfactor = 1;
                        self.framecount = 1;
                        self.cursyl = SYLS.len() - 1;
                        self.spos = 0;
                        self.pos = self.base;
                    } else if self.at(0) == b'!' {
                        self.framecount = 0;
                        self.scounter = 0;
                        self.wait4on = true;
                        self.pos += 1;
                        return;
                    } else if self.at(0) == b'_' {
                        self.framecount = 0;
                        self.scounter = 0;
                        self.wait4off = true;
                        self.pos += 1;
                        return;
                    }
                    if self.at(0) != 0 && self.at(0) != b'!' && self.at(0) != b'_' {
                        self.durfactor = 0;
                        while self.at(0).is_ascii_digit() {
                            self.durfactor = 10 * self.durfactor + (self.at(0) - b'0') as i32;
                            self.pos += 1;
                        }
                        if self.durfactor == 0 {
                            self.durfactor = 1;
                        }
                        let rest = &self.texts[self.pos.min(self.end)..self.end];
                        let (mut fs, mut len) = (SYLS.len() - 1, 1);
                        for (k, s) in SYLS[..SYLS.len() - 1].iter().enumerate() {
                            let l = prefix(s.0, rest);
                            if l > 0 {
                                (fs, len) = (k, l);
                                break;
                            }
                        }
                        self.cursyl = fs;
                        self.spos = 0;
                        self.pos += len;
                    }
                }
                self.curp1 = self.curp2;
                self.curp2 = SYLS[self.cursyl].1[self.spos as usize].max(0) as usize;
                self.scounter = libm::rintf(PHONEMES[self.curp2][17] * self.durfactor as f32) as i32;
                if self.scounter == 0 {
                    self.scounter = 1;
                }
                self.invdur = 1.0 / (self.scounter * self.framerate) as f32;
            }
            self.scounter -= 1;
        }
        self.framecount -= 1;
        let mut x = (self.scounter * self.framerate + self.framecount) as f32 * self.invdur;
        let (r1, r2) = (PHONEMES[self.curp1][18], PHONEMES[self.curp2][18]);
        x = if x > 0.0 { exp2f((log2(x as f64) as f32) * (r1 / r2)) } else { 0.0 };
        let other = if libm::fabsf(r2 - r1) > 8.0 { self.curp2 } else { self.curp1 };
        let mut nf = Frame::default();
        self.set_frame(self.curp2, other, x, &mut nf);
        self.next = nf;
    }

    /// Speaks through `l` (the voice sum's left channel), writing the result to both channels (`ronanCBProcess`); the
    /// samples of one frame may come in several calls.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        for i in 0..n {
            for k in 0..25 {
                self.cur.0[k] += self.delta.0[k];
            }
            let c = self.cur;
            let mut x = l[i] * c.0[21] + self.noise() * c.0[22];
            let mut out = self.res[0].tick(&c.res(0), x) + self.res[1].tick(&c.res(1), x);
            let lin = x;
            x = self.noise() * c.0[23] + x - self.lastin2;
            self.lastin2 = lin;
            for k in 2..7 {
                out = self.res[k].tick(&c.res(k), x) - out;
            }
            out = x * c.0[24] - out;
            out -= self.hpb1;
            self.hpb1 += 0.012 * out;
            out -= self.hpb2;
            self.hpb2 += 0.012 * out;
            out = self.peq_res.tick(&self.peq, out) - out;
            l[i] = out;
            r[i] = out;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_speaks_and_waits_for_notes() {
        let mut ro = Ronan::new(b"!hEl@U wOrld\0two", 44_100.0);
        let saw: Vec<f32> = (0..128).map(|i| ((i % 100) as f32 / 50.0 - 1.0) * 0.5).collect();
        let energy = |ro: &mut Ronan, frames: usize| {
            let mut e = 0.0;
            for _ in 0..frames {
                ro.tick(128);
                let (mut l, mut r) = (saw.clone(), saw.clone());
                ro.process(&mut l, &mut r);
                e += l.iter().map(|x| x * x).sum::<f32>();
            }
            e
        };
        let quiet = energy(&mut ro, 40); // waits at '!' for a note-on
        ro.note_on();
        let loud = energy(&mut ro, 200);
        assert!(loud > 10.0 * quiet.max(1e-6), "{quiet} {loud}");
        // text 1 is selected by controller 4
        ro.set_ctl(4, 1);
        assert_eq!(&ro.texts[ro.base..ro.end], b"two");
        ro.set_ctl(5, 96);
        assert!((ro.pitch - exp2f(0.25)).abs() < 1e-6);
    }
}
