//! Formant (spec §4.6): a 10th-order all-pole vowel filter in f64, mixed linearly with the dry signal.
//! Coefficients: the public-domain "Formant filter" by Alex (alex@smartelectronix.com), musicdsp.org archive #110
//! (https://www.musicdsp.org/en/latest/Filters/110-formant-filter.html), designed for 44.1 kHz, soprano vowels
//! A, E, I, O, U — with the A input gain toned down to 3.11044e-06 as suggested in that page's comments (2003-10-04).

use crate::params::{COUNT, FORMANT_TYPE, FORMANT_WET};

/// Per vowel: input gain, then 10 feedback coefficients.
const VOWELS: [[f64; 11]; 5] = [
    [3.11044e-06, 8.943665402, -36.83889529, 92.01697887, -154.337906, 181.6233289, -151.8651235, 89.09614114, -35.10298511, 8.388101016, -0.923313471],
    [4.36215e-06, 8.90438318, -36.55179099, 91.05750846, -152.422234, 179.1170248, -149.6496211, 87.78352223, -34.60687431, 8.282228154, -0.914150747],
    [3.33819e-06, 8.893102966, -36.49532826, 90.96543286, -152.4545478, 179.4835618, -150.315433, 88.43409371, -34.98612086, 8.407803364, -0.932568035],
    [1.13572e-06, 8.994734087, -37.2084849, 93.22900521, -156.6929844, 184.596544, -154.3755513, 90.49663749, -35.58964535, 8.478996281, -0.929252233],
    [4.09431e-07, 8.997322763, -37.20218544, 93.11385476, -156.2530937, 183.7080141, -153.2631681, 89.59539726, -35.12454591, 8.338655623, -0.910251753],
];

#[derive(Clone, Copy, Debug, Default)]
pub struct Formant {
    mem: [[f64; 10]; 2],
}

impl Formant {
    pub fn run(&mut self, p: &[f32; COUNT], l: &mut [f32], r: &mut [f32]) {
        let c = &VOWELS[((p[FORMANT_TYPE] * 4.0) as usize).min(4)];
        let wet = p[FORMANT_WET];
        let dry = 1.0 - wet;
        for (ch, buf) in [l, r].into_iter().enumerate() {
            let m = &mut self.mem[ch];
            for v in buf.iter_mut() {
                let mut y = c[0] * *v as f64;
                for j in 0..10 {
                    y += c[j + 1] * m[j];
                }
                m.copy_within(0..9, 1);
                m[0] = y;
                *v = *v * dry + y as f32 * wet;
            }
        }
    }
}
