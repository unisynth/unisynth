//! GSM 06.10 full-rate encoder (WAV49 / Microsoft framing), `no_std`, no heap.
//!
//! Bit-exact Rust port of the libgsm 1.0.24 encoder (`gsm_encode.c` WAV49 branch, `code.c`, `preprocess.c`,
//! `lpc.c`, `short_term.c`, `long_term.c`, `rpe.c`, `add.c`, `table.c`, `private.h`). Build config of the
//! reference: `-DWAV49`, no `FAST`, no `USE_FLOAT_MUL`, no `LTP_CUT`. Same saturating `GSM_ADD`/`GSM_SUB`,
//! arithmetic right shifts of negatives, non-rounding `GSM_MULT`, `gsm_L_add`, `gsm_norm`, `gsm_div`.
//! The word helpers (`add`, `sub`, `mult_r`, `asr`, `asl`) and tables are shared with the decoder in the
//! crate root. See the crate root for the libgsm COPYRIGHT notice, which applies to this file too.
use super::{add, asl, asr, mult_r, sub, BLOCK_BYTES, BLOCK_SAMPLES};

/// Table 4.3a, decision levels of the LTP gain quantizer.
const DLB: [i16; 4] = [6554, 16384, 26214, 32767];
/// Table 4.3b, quantization levels of the LTP gain (shared with the decoder's `QLB`).
const QLB: [i16; 4] = super::QLB;
/// Table 4.4, coefficients of the weighting filter.
const H: [i16; 11] = [-134, -374, 0, 2054, 5741, 8192, 5741, 2054, 0, -374, -134];
/// Table 4.5, normalized inverse mantissa.
const NRFAC: [i16; 8] = [29128, 26215, 23832, 21846, 20165, 18725, 17476, 16384];
/// Table 4.6, normalized direct mantissa (shared with the decoder's `FAC`).
const FAC: [i16; 8] = super::FAC;

/// `gsm_mult` / `GSM_MULT`: `(a*b) >> 15` (arithmetic shift), `MIN*MIN -> MAX`.
#[inline]
fn mult(a: i16, b: i16) -> i16 {
    if a == i16::MIN && b == i16::MIN {
        i16::MAX
    } else {
        ((a as i32 * b as i32) >> 15) as i16
    }
}
/// `gsm_abs` / `GSM_ABS`: `|MIN| -> MAX`.
#[inline]
fn abs(a: i16) -> i16 {
    if a < 0 {
        if a == i16::MIN { i16::MAX } else { -a }
    } else {
        a
    }
}
/// `gsm_L_add` / `GSM_L_ADD`: saturating 32-bit add (the C branches are exactly `i32::saturating_add`).
#[inline]
fn l_add(a: i32, b: i32) -> i32 {
    a.saturating_add(b)
}
/// `gsm_norm`: left shifts needed to normalize `a` (`a != 0`); 0 for `a <= -2^30`.
fn norm(a: i32) -> i16 {
    let mut a = a;
    if a < 0 {
        if a <= -1_073_741_824 {
            return 0;
        }
        a = !a;
    }
    // number of leading zeros below the sign bit, `bitoff` table folded into `leading_zeros`
    (a.leading_zeros() as i16) - 1
}
/// `gsm_div`: 15-bit fractional division, `0 <= num <= denum`.
fn div(num: i16, denum: i16) -> i16 {
    if num == 0 {
        return 0;
    }
    let mut l_num = num as i32;
    let l_denum = denum as i32;
    let mut d: i16 = 0;
    for _ in 0..15 {
        d <<= 1;
        l_num <<= 1;
        if l_num >= l_denum {
            l_num -= l_denum;
            d += 1;
        }
    }
    d
}

/// libgsm `struct gsm_state`, encoder fields (decoder-only `nrp`, `v`, `msr`, `verbose`, `fast`, `ltp_cut` omitted).
pub struct GsmEncoder {
    dp0: [i16; 280],
    e: [i16; 50],
    z1: i16,
    l_z2: i32,
    mp: i32,
    u: [i16; 8],
    larpp: [[i16; 8]; 2],
    j: usize,
    frame_index: u8,
    frame_chain: u8,
}

impl Default for GsmEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl GsmEncoder {
    /// Fresh encoder in WAV49 mode (`gsm_create` + `GSM_OPT_WAV49`): all zero.
    pub fn new() -> Self {
        GsmEncoder {
            dp0: [0; 280],
            e: [0; 50],
            z1: 0,
            l_z2: 0,
            mp: 0,
            u: [0; 8],
            larpp: [[0; 8]; 2],
            j: 0,
            frame_index: 0,
            frame_chain: 0,
        }
    }

    /// Encode 320 samples (two frames) into one 65-byte WAV49 block; equals two `gsm_encode` calls
    /// (first frame 32 bytes — its 33rd byte only carries the 4-bit chain —, second frame 33 bytes).
    pub fn encode_block(&mut self, pcm: &[i16; BLOCK_SAMPLES], out: &mut [u8; BLOCK_BYTES]) {
        let mut c = [0u8; 33];
        self.encode_frame(&pcm[..160], &mut c);
        out[..32].copy_from_slice(&c[..32]);
        self.encode_frame(&pcm[160..], &mut c);
        out[32..].copy_from_slice(&c);
    }

    /// `gsm_encode` (WAV49 branch): `Gsm_Coder` then bit packing into 33 bytes. For the first frame of a
    /// pair only 32 are kept (byte 33 = the 4 carry bits, stored in `frame_chain`); the second frame
    /// consumes `frame_chain` and keeps all 33.
    fn encode_frame(&mut self, source: &[i16], c: &mut [u8; 33]) {
        let mut larc = [0i16; 8];
        let mut nc = [0i16; 4];
        let mut bc = [0i16; 4];
        let mut mc = [0i16; 4];
        let mut xmaxc = [0i16; 4];
        let mut xmc = [0i16; 52];
        self.coder(source, &mut larc, &mut nc, &mut bc, &mut mc, &mut xmaxc, &mut xmc);

        let mut sr: u16 = 0;
        let mut p = 0usize;
        // `sr = sr >> n | v << (16 - n)` (uword arithmetic: shl drops the overflow like the C truncation)
        macro_rules! p {
            ($n:literal, $v:expr) => {
                sr = (sr >> $n) | (($v) as u16) << (16 - $n);
            };
        }
        // `*c++ = sr >> k`
        macro_rules! o {
            ($k:literal) => {
                c[p] = (sr >> $k) as u8;
                p += 1;
            };
        }
        self.frame_index = (self.frame_index == 0) as u8;
        if self.frame_index != 0 {
            p!(6, larc[0]);
            p!(6, larc[1]);
            o!(4);
            p!(5, larc[2]);
            o!(7);
            p!(5, larc[3]);
            p!(4, larc[4]);
            o!(6);
            p!(4, larc[5]);
            p!(3, larc[6]);
            o!(7);
            p!(3, larc[7]);
            p!(7, nc[0]);
            o!(5);
            p!(2, bc[0]);
            p!(2, mc[0]);
            p!(6, xmaxc[0]);
            o!(3);
            p!(3, xmc[0]);
            o!(8);
            p!(3, xmc[1]);
            p!(3, xmc[2]);
            p!(3, xmc[3]);
            o!(7);
            p!(3, xmc[4]);
            p!(3, xmc[5]);
            p!(3, xmc[6]);
            o!(6);
            p!(3, xmc[7]);
            p!(3, xmc[8]);
            o!(8);
            p!(3, xmc[9]);
            p!(3, xmc[10]);
            p!(3, xmc[11]);
            o!(7);
            p!(3, xmc[12]);
            p!(7, nc[1]);
            o!(5);
            p!(2, bc[1]);
            p!(2, mc[1]);
            p!(6, xmaxc[1]);
            o!(3);
            p!(3, xmc[13]);
            o!(8);
            p!(3, xmc[14]);
            p!(3, xmc[15]);
            p!(3, xmc[16]);
            o!(7);
            p!(3, xmc[17]);
            p!(3, xmc[18]);
            p!(3, xmc[19]);
            o!(6);
            p!(3, xmc[20]);
            p!(3, xmc[21]);
            o!(8);
            p!(3, xmc[22]);
            p!(3, xmc[23]);
            p!(3, xmc[24]);
            o!(7);
            p!(3, xmc[25]);
            p!(7, nc[2]);
            o!(5);
            p!(2, bc[2]);
            p!(2, mc[2]);
            p!(6, xmaxc[2]);
            o!(3);
            p!(3, xmc[26]);
            o!(8);
            p!(3, xmc[27]);
            p!(3, xmc[28]);
            p!(3, xmc[29]);
            o!(7);
            p!(3, xmc[30]);
            p!(3, xmc[31]);
            p!(3, xmc[32]);
            o!(6);
            p!(3, xmc[33]);
            p!(3, xmc[34]);
            o!(8);
            p!(3, xmc[35]);
            p!(3, xmc[36]);
            p!(3, xmc[37]);
            o!(7);
            p!(3, xmc[38]);
            p!(7, nc[3]);
            o!(5);
            p!(2, bc[3]);
            p!(2, mc[3]);
            p!(6, xmaxc[3]);
            o!(3);
            p!(3, xmc[39]);
            o!(8);
            p!(3, xmc[40]);
            p!(3, xmc[41]);
            p!(3, xmc[42]);
            o!(7);
            p!(3, xmc[43]);
            p!(3, xmc[44]);
            p!(3, xmc[45]);
            o!(6);
            p!(3, xmc[46]);
            p!(3, xmc[47]);
            o!(8);
            p!(3, xmc[48]);
            p!(3, xmc[49]);
            p!(3, xmc[50]);
            o!(7);
            p!(3, xmc[51]);
            sr >>= 4;
            c[p] = (sr >> 8) as u8;
            self.frame_chain = c[p];
            p += 1;
        } else {
            p!(4, self.frame_chain as i16);
            p!(6, larc[0]);
            o!(6);
            p!(6, larc[1]);
            o!(8);
            p!(5, larc[2]);
            p!(5, larc[3]);
            o!(6);
            p!(4, larc[4]);
            p!(4, larc[5]);
            o!(6);
            p!(3, larc[6]);
            p!(3, larc[7]);
            o!(8);
            p!(7, nc[0]);
            p!(2, bc[0]);
            o!(7);
            p!(2, mc[0]);
            p!(6, xmaxc[0]);
            o!(7);
            p!(3, xmc[0]);
            p!(3, xmc[1]);
            p!(3, xmc[2]);
            o!(6);
            p!(3, xmc[3]);
            p!(3, xmc[4]);
            o!(8);
            p!(3, xmc[5]);
            p!(3, xmc[6]);
            p!(3, xmc[7]);
            o!(7);
            p!(3, xmc[8]);
            p!(3, xmc[9]);
            p!(3, xmc[10]);
            o!(6);
            p!(3, xmc[11]);
            p!(3, xmc[12]);
            o!(8);
            p!(7, nc[1]);
            p!(2, bc[1]);
            o!(7);
            p!(2, mc[1]);
            p!(6, xmaxc[1]);
            o!(7);
            p!(3, xmc[13]);
            p!(3, xmc[14]);
            p!(3, xmc[15]);
            o!(6);
            p!(3, xmc[16]);
            p!(3, xmc[17]);
            o!(8);
            p!(3, xmc[18]);
            p!(3, xmc[19]);
            p!(3, xmc[20]);
            o!(7);
            p!(3, xmc[21]);
            p!(3, xmc[22]);
            p!(3, xmc[23]);
            o!(6);
            p!(3, xmc[24]);
            p!(3, xmc[25]);
            o!(8);
            p!(7, nc[2]);
            p!(2, bc[2]);
            o!(7);
            p!(2, mc[2]);
            p!(6, xmaxc[2]);
            o!(7);
            p!(3, xmc[26]);
            p!(3, xmc[27]);
            p!(3, xmc[28]);
            o!(6);
            p!(3, xmc[29]);
            p!(3, xmc[30]);
            o!(8);
            p!(3, xmc[31]);
            p!(3, xmc[32]);
            p!(3, xmc[33]);
            o!(7);
            p!(3, xmc[34]);
            p!(3, xmc[35]);
            p!(3, xmc[36]);
            o!(6);
            p!(3, xmc[37]);
            p!(3, xmc[38]);
            o!(8);
            p!(7, nc[3]);
            p!(2, bc[3]);
            o!(7);
            p!(2, mc[3]);
            p!(6, xmaxc[3]);
            o!(7);
            p!(3, xmc[39]);
            p!(3, xmc[40]);
            p!(3, xmc[41]);
            o!(6);
            p!(3, xmc[42]);
            p!(3, xmc[43]);
            o!(8);
            p!(3, xmc[44]);
            p!(3, xmc[45]);
            p!(3, xmc[46]);
            o!(7);
            p!(3, xmc[47]);
            p!(3, xmc[48]);
            p!(3, xmc[49]);
            o!(6);
            p!(3, xmc[50]);
            p!(3, xmc[51]);
            o!(8);
        }
        debug_assert_eq!(p, 33);
    }

    /// `Gsm_Coder` (code.c, 4.2): one 160-sample frame → parameters.
    #[allow(clippy::too_many_arguments)]
    fn coder(
        &mut self,
        s: &[i16],
        larc: &mut [i16; 8],
        nc: &mut [i16; 4],
        bc: &mut [i16; 4],
        mc: &mut [i16; 4],
        xmaxc: &mut [i16; 4],
        xmc: &mut [i16; 52],
    ) {
        let mut so = [0i16; 160];
        self.preprocess(s, &mut so);
        lpc_analysis(&mut so, larc);
        self.short_term_analysis_filter(larc, &mut so);

        for k in 0..4 {
            // dp = dp0 + 120 + 40k ; dpp aliases dp (the estimate is written into the slot being filled)
            let dp = 120 + 40 * k;
            let (b, n) = long_term_params(&so[k * 40..k * 40 + 40], &self.dp0, dp);
            bc[k] = b;
            nc[k] = n;
            // Long_term_analysis_filtering: dpp = dp[0..40], e = self.e[5..45]
            let bp = QLB[b as usize];
            for i in 0..40 {
                let dpp = mult_r(bp, self.dp0[dp + i - n as usize]);
                self.dp0[dp + i] = dpp;
                self.e[5 + i] = sub(so[k * 40 + i], dpp);
            }
            let (xm, mcv, xmx) = rpe_encoding(&mut self.e);
            mc[k] = mcv;
            xmaxc[k] = xmx;
            xmc[k * 13..k * 13 + 13].copy_from_slice(&xm);
            // Update of the reconstructed short time residual signal: dp[i] = e[5+i] + dpp[i]
            for i in 0..40 {
                self.dp0[dp + i] = add(self.e[5 + i], self.dp0[dp + i]);
            }
        }
        self.dp0.copy_within(160..280, 0);
    }

    /// `Gsm_Preprocess` (4.2.1 .. 4.2.3): downscale, offset compensation, pre-emphasis.
    fn preprocess(&mut self, s: &[i16], so: &mut [i16; 160]) {
        let mut z1 = self.z1;
        let mut l_z2 = self.l_z2;
        let mut mp = self.mp;
        for k in 0..160 {
            // 4.2.1 downscaling
            let sov: i16 = (s[k] >> 3) << 2;
            // 4.2.2 offset compensation: non-recursive part
            let s1: i16 = sov.wrapping_sub(z1);
            z1 = sov;
            // recursive part, 31x16 bit multiplication
            let mut l_s2: i32 = (s1 as i32) << 15;
            let msp: i16 = (l_z2 >> 15) as i16;
            let lsp: i16 = (l_z2 - ((msp as i32) << 15)) as i16;
            l_s2 += mult_r(lsp, 32735) as i32;
            let l_temp = (msp as i32) * 32735;
            l_z2 = l_add(l_temp, l_s2);
            // sof[k] with rounding
            let l_temp = l_add(l_z2, 16384);
            // 4.2.3 pre-emphasis (`mp` is an int in C: not truncated to a word)
            let msp = ((mp * -28180 + 16384) >> 15) as i16; // GSM_MULT_R(mp, -28180)
            mp = l_temp >> 15;
            so[k] = (mp + msp as i32).clamp(-32768, 32767) as i16; // GSM_ADD
        }
        self.z1 = z1;
        self.l_z2 = l_z2;
        self.mp = mp;
    }

    /// `Gsm_Short_Term_Analysis_Filter` (4.2.8 .. 4.2.10): in-place residual `d` of `s`.
    fn short_term_analysis_filter(&mut self, larc: &[i16; 8], s: &mut [i16; 160]) {
        let cur = self.j;
        let prev = cur ^ 1;
        self.j = prev;
        // 4.2.8 decoding of the coded log area ratios
        const MIC: [i16; 8] = [-32, -32, -16, -16, -8, -8, -4, -4];
        const B2: [i16; 8] = [0, 0, 4096, -5120, 188, -3584, -682, -2288];
        const INVA: [i16; 8] = [13107, 13107, 13107, 13107, 19223, 17476, 31454, 29708];
        for i in 0..8 {
            let t = (add(larc[i], MIC[i]) as i32) << 10;
            let t = sub(t as i16, B2[i]);
            let t = mult_r(INVA[i], t);
            self.larpp[cur][i] = add(t, t);
        }
        let lj = self.larpp[cur];
        let lj1 = self.larpp[prev];
        let mut larp = [0i16; 8];

        for i in 0..8 {
            larp[i] = add(lj1[i] >> 2, lj[i] >> 2);
            larp[i] = add(larp[i], lj1[i] >> 1);
        }
        larp_to_rp(&mut larp);
        self.analysis_filtering(&larp, &mut s[0..13]);

        for i in 0..8 {
            larp[i] = add(lj1[i] >> 1, lj[i] >> 1);
        }
        larp_to_rp(&mut larp);
        self.analysis_filtering(&larp, &mut s[13..27]);

        for i in 0..8 {
            larp[i] = add(lj1[i] >> 2, lj[i] >> 2);
            larp[i] = add(larp[i], lj[i] >> 1);
        }
        larp_to_rp(&mut larp);
        self.analysis_filtering(&larp, &mut s[27..40]);

        larp = lj;
        larp_to_rp(&mut larp);
        self.analysis_filtering(&larp, &mut s[40..160]);
    }

    /// `Short_term_analysis_filtering` (4.2.10) over `s.len()` samples, in place.
    fn analysis_filtering(&mut self, rp: &[i16; 8], s: &mut [i16]) {
        let u = &mut self.u;
        for x in s.iter_mut() {
            let mut di = *x;
            let mut sav = di;
            for i in 0..8 {
                let ui = u[i];
                let rpi = rp[i];
                u[i] = sav;
                sav = add(ui, mult_r(rpi, di));
                di = add(di, mult_r(rpi, ui));
            }
            *x = di;
        }
    }
}

/// `LARp_to_rp` (4.2.9.2).
fn larp_to_rp(larp: &mut [i16; 8]) {
    for x in larp.iter_mut() {
        let neg = *x < 0;
        let temp: i16 = if neg {
            if *x == i16::MIN { i16::MAX } else { -*x }
        } else {
            *x
        };
        let r: i16 = if temp < 11059 {
            temp << 1
        } else if temp < 20070 {
            temp + 11059
        } else {
            add(temp >> 2, 26112)
        };
        *x = if neg { -r } else { r };
    }
}

/// `Gsm_LPC_Analysis` (4.2.4 .. 4.2.7). `s` is scaled in place and rescaled (lossy, as in C: the
/// `mult_r` down-scale / `<<` up-scale round trip changes the low bits that the filter later sees).
fn lpc_analysis(s: &mut [i16; 160], larc: &mut [i16; 8]) {
    let l_acf = autocorrelation(s);
    reflection_coefficients(&l_acf, larc);
    // 4.2.6 transformation to log area ratios
    for x in larc.iter_mut() {
        let temp = abs(*x);
        let temp: i16 = if temp < 22118 {
            temp >> 1
        } else if temp < 31130 {
            temp - 11059
        } else {
            (temp - 26112) << 2
        };
        *x = if *x < 0 { -temp } else { temp };
    }
    // 4.2.7 quantization and coding
    const Q: [(i16, i16, i16, i16); 8] = [
        (20480, 0, 31, -32),
        (20480, 0, 31, -32),
        (20480, 2048, 15, -16),
        (20480, -2560, 15, -16),
        (13964, 94, 7, -8),
        (15360, -1792, 7, -8),
        (8534, -341, 3, -4),
        (9036, -1144, 3, -4),
    ];
    for (x, &(a, b, mac, mic)) in larc.iter_mut().zip(Q.iter()) {
        let mut temp = mult(a, *x);
        temp = add(temp, b);
        temp = add(temp, 256);
        temp >>= 9;
        *x = if temp > mac {
            mac - mic
        } else if temp < mic {
            0
        } else {
            temp - mic
        };
    }
}

/// 4.2.4 `Autocorrelation`.
fn autocorrelation(s: &mut [i16; 160]) -> [i32; 9] {
    let mut smax: i16 = 0;
    for &x in s.iter() {
        let t = abs(x);
        if t > smax {
            smax = t;
        }
    }
    let scalauto: i16 = if smax == 0 { 0 } else { 4 - norm((smax as i32) << 16) };
    if scalauto > 0 {
        // `SCALE(n)`: s = GSM_MULT_R(s, 16384 >> (n-1)), n = 1..=4
        let f = 16384i16 >> (scalauto - 1);
        for x in s.iter_mut() {
            *x = mult_r(*x, f);
        }
    }
    let mut l_acf = [0i32; 9];
    for i in 0..160 {
        let sl = s[i] as i32;
        for (k, acc) in l_acf.iter_mut().enumerate() {
            if k > i {
                break;
            }
            *acc += sl * s[i - k] as i32;
        }
    }
    for a in l_acf.iter_mut() {
        *a <<= 1;
    }
    if scalauto > 0 {
        for x in s.iter_mut() {
            *x = ((*x as i32) << scalauto) as i16;
        }
    }
    l_acf
}

/// 4.2.5 `Reflection_coefficients`: Schur recursion with 16-bit arithmetic.
fn reflection_coefficients(l_acf: &[i32; 9], r: &mut [i16; 8]) {
    if l_acf[0] == 0 {
        *r = [0; 8];
        return;
    }
    let temp = norm(l_acf[0]);
    let mut acf = [0i16; 9];
    for i in 0..9 {
        acf[i] = ((l_acf[i] << temp) >> 16) as i16;
    }
    let mut k = [0i16; 9];
    let mut p = acf;
    k[1..8].copy_from_slice(&acf[1..8]);

    for n in 1..=8usize {
        let temp = abs(p[1]);
        if p[0] < temp {
            for x in r[n - 1..].iter_mut() {
                *x = 0;
            }
            return;
        }
        let mut rv = div(temp, p[0]);
        if p[1] > 0 {
            rv = -rv;
        }
        r[n - 1] = rv;
        if n == 8 {
            return;
        }
        // Schur recursion
        let t = mult_r(p[1], rv);
        p[0] = add(p[0], t);
        for m in 1..=(8 - n) {
            let t = mult_r(k[m], rv);
            p[m] = add(p[m + 1], t);
            let t = mult_r(p[m + 1], rv);
            k[m] = add(k[m], t);
        }
    }
}

/// `Calculation_of_the_LTP_parameters` (4.2.11, integer path): returns `(bc, Nc)`.
/// `d` = 40 residual samples; `dp[k]` for `k` in `-120..0` is `dp0[base + k]`.
fn long_term_params(d: &[i16], dp0: &[i16; 280], base: usize) -> (i16, i16) {
    let mut dmax: i16 = 0;
    for &x in d {
        let t = abs(x);
        if t > dmax {
            dmax = t;
        }
    }
    let mut temp: i16 = 0;
    if dmax != 0 {
        temp = norm((dmax as i32) << 16);
    }
    let scal: i16 = if temp > 6 { 0 } else { 6 - temp };

    let mut wt = [0i16; 40];
    for k in 0..40 {
        wt[k] = d[k] >> scal;
    }

    let mut l_max: i32 = 0;
    let mut nc: i16 = 40;
    for lambda in 40..=120usize {
        let mut l_result: i32 = 0;
        for k in 0..40 {
            l_result += wt[k] as i32 * dp0[base + k - lambda] as i32;
        }
        if l_result > l_max {
            nc = lambda as i16;
            l_max = l_result;
        }
    }

    l_max <<= 1;
    // rescaling of L_max
    l_max >>= 6 - scal;

    // power of the reconstructed short term residual signal dp[..]
    let mut l_power: i32 = 0;
    for k in 0..40 {
        let t = (dp0[base + k - nc as usize] >> 3) as i32;
        l_power += t * t;
    }
    l_power <<= 1;

    if l_max <= 0 {
        return (0, nc);
    }
    if l_max >= l_power {
        return (3, nc);
    }
    let temp = norm(l_power);
    let r = ((l_max << temp) >> 16) as i16;
    let s = ((l_power << temp) >> 16) as i16;
    let mut bc = 0usize;
    while bc <= 2 {
        if r <= mult(s, DLB[bc]) {
            break;
        }
        bc += 1;
    }
    (bc as i16, nc)
}

/// `Gsm_RPE_Encoding` (4.2.13 .. 4.2.17): `e` is the 50-word buffer (`e[5..45]` in/out: the long term
/// residual in, the reconstructed `ep` out). Returns `(xMc[0..13], Mc, xmaxc)`.
fn rpe_encoding(e: &mut [i16; 50]) -> ([i16; 13], i16, i16) {
    // 4.2.13 weighting filter
    let mut x = [0i16; 40];
    for k in 0..40 {
        let mut l_result: i32 = 8192 >> 1;
        for i in 0..11 {
            l_result += e[k + i] as i32 * H[i] as i32;
        }
        x[k] = (l_result >> 13).clamp(-32768, 32767) as i16;
    }

    // 4.2.14 RPE grid selection
    let mut em: i32 = 0;
    let mut mc: usize = 0;
    for m in 0..4 {
        let mut l_result: i32 = 0;
        for i in 0..13 {
            let t = (x[m + 3 * i] >> 2) as i32;
            l_result += t * t;
        }
        l_result <<= 1; // implicit in L_MULT
        // m = 0 seeds EM (the C sets EM = L(0)); later grids win only on strictly greater energy
        if m == 0 || l_result > em {
            mc = m;
            em = l_result;
        }
    }
    let mut xm = [0i16; 13];
    for i in 0..13 {
        xm[i] = x[mc + 3 * i];
    }

    // 4.2.15 APCM quantization
    let mut xmax: i16 = 0;
    for &v in xm.iter() {
        let t = abs(v);
        if t > xmax {
            xmax = t;
        }
    }
    let mut exp: i16 = 0;
    let mut temp: i16 = xmax >> 9;
    let mut itest = false;
    for _ in 0..=5 {
        itest |= temp <= 0;
        temp >>= 1;
        if !itest {
            exp += 1;
        }
    }
    let temp = exp + 5;
    let xmaxc = add(xmax >> temp, exp << 3);

    let (exp, mant) = xmaxc_to_exp_mant(xmaxc);
    let temp1 = 6 - exp;
    let temp2 = NRFAC[mant as usize];
    let mut xmc = [0i16; 13];
    for i in 0..13 {
        let t = ((xm[i] as i32) << temp1) as i16; // SASL into a word
        let t = mult(t, temp2) >> 12;
        xmc[i] = t + 4;
    }

    // 4.2.16 APCM inverse quantization
    let temp1 = FAC[mant as usize];
    let temp2 = sub(6, exp);
    let temp3 = asl(1, sub(temp2, 1) as i32);
    let mut xmp = [0i16; 13];
    for i in 0..13 {
        let mut t: i16 = (xmc[i] << 1) - 7;
        t <<= 12;
        t = mult_r(temp1, t);
        t = add(t, temp3);
        xmp[i] = asr(t, temp2 as i32);
    }

    // 4.2.17 RPE grid positioning (writes all of e[5..45])
    for v in e[5..45].iter_mut() {
        *v = 0;
    }
    for i in 0..13 {
        e[5 + mc + 3 * i] = xmp[i];
    }
    (xmc, mc as i16, xmaxc)
}

/// `APCM_quantization_xmaxc_to_exp_mant` → `(exp, mant)`.
fn xmaxc_to_exp_mant(xmaxc: i16) -> (i16, i16) {
    let mut exp: i16 = 0;
    if xmaxc > 15 {
        exp = (xmaxc >> 3) - 1;
    }
    let mut mant: i16 = xmaxc - (exp << 3);
    if mant == 0 {
        exp = -4;
        mant = 7;
    } else {
        while mant <= 7 {
            mant = mant << 1 | 1;
            exp -= 1;
        }
        mant -= 8;
    }
    (exp, mant)
}

/// Samples → blocks: `ceil(samples / 320)`.
pub fn blocks_for(samples: usize) -> usize {
    samples.div_ceil(BLOCK_SAMPLES)
}

/// Encode f32 PCM (`-1..1`) into consecutive 65-byte WAV49 blocks with one fresh [`GsmEncoder`]. Each
/// sample is `(x * 32768.0) as i16` (truncation toward zero, saturating, NaN → 0), the C toast conversion.
/// The last partial block is zero-padded. Returns the bytes written (`blocks_for(pcm.len()) * 65`).
///
/// # Panics
/// If `out` is shorter than `blocks_for(pcm.len()) * 65`.
pub fn encode(pcm: &[f32], out: &mut [u8]) -> usize {
    let need = blocks_for(pcm.len()) * BLOCK_BYTES;
    assert!(out.len() >= need, "gsm::encode: out too small");
    let mut enc = GsmEncoder::new();
    for (chunk, o) in pcm.chunks(BLOCK_SAMPLES).zip(out.chunks_exact_mut(BLOCK_BYTES)) {
        let mut blk = [0i16; BLOCK_SAMPLES];
        for (b, &x) in blk.iter_mut().zip(chunk) {
            *b = (x * 32768.0) as i16;
        }
        let mut ob = [0u8; BLOCK_BYTES];
        enc.encode_block(&blk, &mut ob);
        o.copy_from_slice(&ob);
    }
    need
}
