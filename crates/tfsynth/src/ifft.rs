//! In-place 512-point radix-2 complex FFT with positive exponent (inverse direction), no 1/N scaling.
//! Data is interleaved `[re0, im0, re1, im1, …]`; twiddles come from the shared 4096-point sine table.

use crate::tables::{tables, BINS, TAB};

pub fn ifft512(x: &mut [f32; 2 * BINS]) {
    let t = tables();
    // bit-reversal permutation (9 bits)
    for i in 0..BINS {
        let j = (i as u32).reverse_bits() as usize >> (32 - 9);
        if i < j {
            x.swap(2 * i, 2 * j);
            x.swap(2 * i + 1, 2 * j + 1);
        }
    }
    let mut m = 2;
    while m <= BINS {
        let half = m / 2;
        let stride = TAB / m; // twiddle e^{+i·2π·k/m} = table index k·4096/m
        for k in 0..half {
            let idx = k * stride;
            let (wr, wi) = (t.sin[(idx + TAB / 4) % TAB], t.sin[idx]);
            let mut a = k;
            while a < BINS {
                let b = a + half;
                let (br, bi) = (x[2 * b], x[2 * b + 1]);
                let tr = br * wr - bi * wi;
                let ti = br * wi + bi * wr;
                let (ar, ai) = (x[2 * a], x[2 * a + 1]);
                x[2 * b] = ar - tr;
                x[2 * b + 1] = ai - ti;
                x[2 * a] = ar + tr;
                x[2 * a + 1] = ai + ti;
                a += m;
            }
        }
        m *= 2;
    }
}
