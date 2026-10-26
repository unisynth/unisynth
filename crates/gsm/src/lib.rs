//! GSM 06.10 full-rate speech codec (WAV49 / Microsoft framing): decoder here, encoder in `encode.rs`
//! (re-exported), `no_std`, no heap.
//!
//! This is a Rust port of the libgsm 1.0.24 codec; the decoder side ports (`gsm_decode.c` WAV49 branch, `decode.c`, `rpe.c`,
//! `long_term.c`, `short_term.c`, `add.c`, `table.c`, `private.h`) from <https://www.quut.com/gsm/gsm-1.0.24.tar.gz>.
//! The output is bit-exact with libgsm (same `i16` words, saturating `GSM_ADD`/`GSM_SUB`, rounding `GSM_MULT_R`,
//! arithmetic right shifts of negatives).
//!
//! libgsm COPYRIGHT (reproduced verbatim, as the notice requires):
//!
//! ```text
//! Copyright 1992, 1993, 1994 by Jutta Degener and Carsten Bormann,
//! Technische Universitaet Berlin
//!
//! Any use of this software is permitted provided that this notice is not
//! removed and that neither the authors nor the Technische Universitaet Berlin
//! are deemed to have made any representations as to the suitability of this
//! software for any purpose nor are held responsible for any defects of
//! this software.  THERE IS ABSOLUTELY NO WARRANTY FOR THIS SOFTWARE.
//!
//! As a matter of courtesy, the authors request to be informed about uses
//! this software has found, about bugs in this software, and about any
//! improvements that may be of general interest.
//!
//! Berlin, 28.11.1994
//! Jutta Degener
//! Carsten Bormann
//!
//!                                  oOo
//!
//! Since the original terms of 15 years ago maybe do not make our
//! intentions completely clear given today's refined usage of the legal
//! terms, we append this additional permission:
//!
//!       Permission to use, copy, modify, and distribute this software
//!       for any purpose with or without fee is hereby granted,
//!       provided that this notice is not removed and that neither
//!       the authors nor the Technische Universitaet Berlin are
//!       deemed to have made any representations as to the suitability
//!       of this software for any purpose nor are held responsible
//!       for any defects of this software.  THERE IS ABSOLUTELY NO
//!       WARRANTY FOR THIS SOFTWARE.
//!
//! Berkeley/Bremen, 05.04.2009
//! Jutta Degener
//! Carsten Bormann
//! ```
#![cfg_attr(not(test), no_std)]

mod encode;
pub use encode::{blocks_for, encode, GsmEncoder};

/// Bytes per WAV49 block (two GSM frames).
pub const BLOCK_BYTES: usize = 65;
/// PCM samples per WAV49 block (two frames of 160).
pub const BLOCK_SAMPLES: usize = 320;

/// Decode error. libgsm's WAV49 decoder rejects nothing (every bit pattern is a valid frame: the 4-bit
/// GSM_MAGIC check exists only in the non-WAV49 33-byte format), so [`decode`] never returns this; the
/// `Result` is kept for API stability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GsmError {
    Malformed,
}

const MIN_WORD: i32 = -32768;
const MAX_WORD: i32 = 32767;

/// `GSM_ADD` / `GSM_SUB` saturation to a 16-bit word.
#[inline]
fn sat(x: i32) -> i16 {
    x.clamp(MIN_WORD, MAX_WORD) as i16
}
#[inline]
fn add(a: i16, b: i16) -> i16 {
    sat(a as i32 + b as i32)
}
#[inline]
fn sub(a: i16, b: i16) -> i16 {
    sat(a as i32 - b as i32)
}
/// `gsm_mult_r`: `(a*b + 16384) >> 15` (arithmetic shift), `MIN*MIN -> MAX`. |a*b| ≤ 2^30 so no i32 overflow.
#[inline]
fn mult_r(a: i16, b: i16) -> i16 {
    if a == i16::MIN && b == i16::MIN {
        i16::MAX
    } else {
        ((a as i32 * b as i32 + 16384) >> 15) as i16
    }
}
/// `gsm_asr` (n may be negative → left shift, truncated to a word like the C return).
#[inline]
fn asr(a: i16, n: i32) -> i16 {
    if n >= 16 {
        -((a < 0) as i16)
    } else if n <= -16 {
        0
    } else if n < 0 {
        ((a as i32) << (-n)) as i16
    } else {
        a >> n
    }
}
/// `gsm_asl`.
#[inline]
fn asl(a: i16, n: i32) -> i16 {
    if n >= 16 {
        0
    } else if n <= -16 {
        -((a < 0) as i16)
    } else if n < 0 {
        asr(a, -n)
    } else {
        ((a as i32) << n) as i16
    }
}

/// Table 4.3b, quantization levels of the LTP gain.
const QLB: [i16; 4] = [3277, 11469, 21299, 32767];
/// Table 4.6, normalized direct mantissa.
const FAC: [i16; 8] = [18431, 20479, 22527, 24575, 26623, 28671, 30719, 32767];

/// libgsm `struct gsm_state`, decoder fields only (encoder-only `e`, `verbose`, `fast` omitted).
/// `z1`, `L_z2`, `mp`, `u`, `ltp_cut`, `wav_fmt` are kept for layout parity but unused when decoding.
#[allow(dead_code)]
pub struct GsmDecoder {
    dp0: [i16; 280],
    z1: i16,
    l_z2: i32,
    mp: i32,
    u: [i16; 8],
    larpp: [[i16; 8]; 2],
    j: usize,
    ltp_cut: i16,
    nrp: i16,
    v: [i16; 9],
    msr: i16,
    frame_index: u8,
    frame_chain: u8,
    wav_fmt: bool,
}

impl Default for GsmDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl GsmDecoder {
    /// Fresh decoder in WAV49 mode (`gsm_create` + `GSM_OPT_WAV49`): all zero, `nrp = 40`.
    pub fn new() -> Self {
        GsmDecoder {
            dp0: [0; 280],
            z1: 0,
            l_z2: 0,
            mp: 0,
            u: [0; 8],
            larpp: [[0; 8]; 2],
            j: 0,
            ltp_cut: 0,
            nrp: 40,
            v: [0; 9],
            msr: 0,
            frame_index: 0,
            frame_chain: 0,
            wav_fmt: true,
        }
    }

    /// Decode one 65-byte WAV49 block (two GSM frames) into 320 samples; equals two `gsm_decode` calls at
    /// `block` and `block + 33`. Every bit pattern decodes.
    pub fn decode_block(&mut self, block: &[u8; BLOCK_BYTES], out: &mut [i16; BLOCK_SAMPLES]) {
        let (o0, o1) = out.split_at_mut(160);
        self.decode_frame(&block[..33], o0);
        self.decode_frame(&block[33..], o1);
    }

    /// `gsm_decode` (WAV49 branch): unpack one frame (33 bytes for odd frames, 32 for even), then `Gsm_Decoder`.
    /// `c` bytes past the end read as 0 (never happens with the sizes above).
    #[allow(unused_assignments)]
    fn decode_frame(&mut self, c: &[u8], target: &mut [i16]) {
        let mut larc = [0i16; 8];
        let mut nc = [0i16; 4];
        let mut bc = [0i16; 4];
        let mut mc = [0i16; 4];
        let mut xmaxc = [0i16; 4];
        let mut xmc = [0i16; 52];
        let mut p = 0usize;
        macro_rules! nx {
            () => {{
                let b = c.get(p).copied().unwrap_or(0);
                p += 1;
                b
            }};
        }
        let mut sr: u16;
        self.frame_index = (self.frame_index == 0) as u8;
        if self.frame_index != 0 {
            sr = nx!() as u16;
            larc[0] = (sr & 0x3f) as i16; sr >>= 6;
            sr |= (nx!() as u16) << 2;
            larc[1] = (sr & 0x3f) as i16; sr >>= 6;
            sr |= (nx!() as u16) << 4;
            larc[2] = (sr & 0x1f) as i16; sr >>= 5;
            larc[3] = (sr & 0x1f) as i16; sr >>= 5;
            sr |= (nx!() as u16) << 2;
            larc[4] = (sr & 0xf) as i16; sr >>= 4;
            larc[5] = (sr & 0xf) as i16; sr >>= 4;
            sr |= (nx!() as u16) << 2;
            larc[6] = (sr & 0x7) as i16; sr >>= 3;
            larc[7] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 4;
            nc[0] = (sr & 0x7f) as i16; sr >>= 7;
            bc[0] = (sr & 0x3) as i16; sr >>= 2;
            mc[0] = (sr & 0x3) as i16; sr >>= 2;
            sr |= (nx!() as u16) << 1;
            xmaxc[0] = (sr & 0x3f) as i16; sr >>= 6;
            xmc[0] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            xmc[1] = (sr & 0x7) as i16; sr >>= 3;
            xmc[2] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 2;
            xmc[3] = (sr & 0x7) as i16; sr >>= 3;
            xmc[4] = (sr & 0x7) as i16; sr >>= 3;
            xmc[5] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 1;
            xmc[6] = (sr & 0x7) as i16; sr >>= 3;
            xmc[7] = (sr & 0x7) as i16; sr >>= 3;
            xmc[8] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            xmc[9] = (sr & 0x7) as i16; sr >>= 3;
            xmc[10] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 2;
            xmc[11] = (sr & 0x7) as i16; sr >>= 3;
            xmc[12] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 4;
            nc[1] = (sr & 0x7f) as i16; sr >>= 7;
            bc[1] = (sr & 0x3) as i16; sr >>= 2;
            mc[1] = (sr & 0x3) as i16; sr >>= 2;
            sr |= (nx!() as u16) << 1;
            xmaxc[1] = (sr & 0x3f) as i16; sr >>= 6;
            xmc[13] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            xmc[14] = (sr & 0x7) as i16; sr >>= 3;
            xmc[15] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 2;
            xmc[16] = (sr & 0x7) as i16; sr >>= 3;
            xmc[17] = (sr & 0x7) as i16; sr >>= 3;
            xmc[18] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 1;
            xmc[19] = (sr & 0x7) as i16; sr >>= 3;
            xmc[20] = (sr & 0x7) as i16; sr >>= 3;
            xmc[21] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            xmc[22] = (sr & 0x7) as i16; sr >>= 3;
            xmc[23] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 2;
            xmc[24] = (sr & 0x7) as i16; sr >>= 3;
            xmc[25] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 4;
            nc[2] = (sr & 0x7f) as i16; sr >>= 7;
            bc[2] = (sr & 0x3) as i16; sr >>= 2;
            mc[2] = (sr & 0x3) as i16; sr >>= 2;
            sr |= (nx!() as u16) << 1;
            xmaxc[2] = (sr & 0x3f) as i16; sr >>= 6;
            xmc[26] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            xmc[27] = (sr & 0x7) as i16; sr >>= 3;
            xmc[28] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 2;
            xmc[29] = (sr & 0x7) as i16; sr >>= 3;
            xmc[30] = (sr & 0x7) as i16; sr >>= 3;
            xmc[31] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 1;
            xmc[32] = (sr & 0x7) as i16; sr >>= 3;
            xmc[33] = (sr & 0x7) as i16; sr >>= 3;
            xmc[34] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            xmc[35] = (sr & 0x7) as i16; sr >>= 3;
            xmc[36] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 2;
            xmc[37] = (sr & 0x7) as i16; sr >>= 3;
            xmc[38] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 4;
            nc[3] = (sr & 0x7f) as i16; sr >>= 7;
            bc[3] = (sr & 0x3) as i16; sr >>= 2;
            mc[3] = (sr & 0x3) as i16; sr >>= 2;
            sr |= (nx!() as u16) << 1;
            xmaxc[3] = (sr & 0x3f) as i16; sr >>= 6;
            xmc[39] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            xmc[40] = (sr & 0x7) as i16; sr >>= 3;
            xmc[41] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 2;
            xmc[42] = (sr & 0x7) as i16; sr >>= 3;
            xmc[43] = (sr & 0x7) as i16; sr >>= 3;
            xmc[44] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 1;
            xmc[45] = (sr & 0x7) as i16; sr >>= 3;
            xmc[46] = (sr & 0x7) as i16; sr >>= 3;
            xmc[47] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            xmc[48] = (sr & 0x7) as i16; sr >>= 3;
            xmc[49] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 2;
            xmc[50] = (sr & 0x7) as i16; sr >>= 3;
            xmc[51] = (sr & 0x7) as i16; sr >>= 3;
            self.frame_chain = (sr & 0xf) as u8;
        } else {
            sr = self.frame_chain as u16;
            sr |= (nx!() as u16) << 4;
            larc[0] = (sr & 0x3f) as i16; sr >>= 6;
            larc[1] = (sr & 0x3f) as i16; sr >>= 6;
            sr = nx!() as u16;
            larc[2] = (sr & 0x1f) as i16; sr >>= 5;
            sr |= (nx!() as u16) << 3;
            larc[3] = (sr & 0x1f) as i16; sr >>= 5;
            larc[4] = (sr & 0xf) as i16; sr >>= 4;
            sr |= (nx!() as u16) << 2;
            larc[5] = (sr & 0xf) as i16; sr >>= 4;
            larc[6] = (sr & 0x7) as i16; sr >>= 3;
            larc[7] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            nc[0] = (sr & 0x7f) as i16; sr >>= 7;
            sr |= (nx!() as u16) << 1;
            bc[0] = (sr & 0x3) as i16; sr >>= 2;
            mc[0] = (sr & 0x3) as i16; sr >>= 2;
            sr |= (nx!() as u16) << 5;
            xmaxc[0] = (sr & 0x3f) as i16; sr >>= 6;
            xmc[0] = (sr & 0x7) as i16; sr >>= 3;
            xmc[1] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 1;
            xmc[2] = (sr & 0x7) as i16; sr >>= 3;
            xmc[3] = (sr & 0x7) as i16; sr >>= 3;
            xmc[4] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            xmc[5] = (sr & 0x7) as i16; sr >>= 3;
            xmc[6] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 2;
            xmc[7] = (sr & 0x7) as i16; sr >>= 3;
            xmc[8] = (sr & 0x7) as i16; sr >>= 3;
            xmc[9] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 1;
            xmc[10] = (sr & 0x7) as i16; sr >>= 3;
            xmc[11] = (sr & 0x7) as i16; sr >>= 3;
            xmc[12] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            nc[1] = (sr & 0x7f) as i16; sr >>= 7;
            sr |= (nx!() as u16) << 1;
            bc[1] = (sr & 0x3) as i16; sr >>= 2;
            mc[1] = (sr & 0x3) as i16; sr >>= 2;
            sr |= (nx!() as u16) << 5;
            xmaxc[1] = (sr & 0x3f) as i16; sr >>= 6;
            xmc[13] = (sr & 0x7) as i16; sr >>= 3;
            xmc[14] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 1;
            xmc[15] = (sr & 0x7) as i16; sr >>= 3;
            xmc[16] = (sr & 0x7) as i16; sr >>= 3;
            xmc[17] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            xmc[18] = (sr & 0x7) as i16; sr >>= 3;
            xmc[19] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 2;
            xmc[20] = (sr & 0x7) as i16; sr >>= 3;
            xmc[21] = (sr & 0x7) as i16; sr >>= 3;
            xmc[22] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 1;
            xmc[23] = (sr & 0x7) as i16; sr >>= 3;
            xmc[24] = (sr & 0x7) as i16; sr >>= 3;
            xmc[25] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            nc[2] = (sr & 0x7f) as i16; sr >>= 7;
            sr |= (nx!() as u16) << 1;
            bc[2] = (sr & 0x3) as i16; sr >>= 2;
            mc[2] = (sr & 0x3) as i16; sr >>= 2;
            sr |= (nx!() as u16) << 5;
            xmaxc[2] = (sr & 0x3f) as i16; sr >>= 6;
            xmc[26] = (sr & 0x7) as i16; sr >>= 3;
            xmc[27] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 1;
            xmc[28] = (sr & 0x7) as i16; sr >>= 3;
            xmc[29] = (sr & 0x7) as i16; sr >>= 3;
            xmc[30] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            xmc[31] = (sr & 0x7) as i16; sr >>= 3;
            xmc[32] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 2;
            xmc[33] = (sr & 0x7) as i16; sr >>= 3;
            xmc[34] = (sr & 0x7) as i16; sr >>= 3;
            xmc[35] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 1;
            xmc[36] = (sr & 0x7) as i16; sr >>= 3;
            xmc[37] = (sr & 0x7) as i16; sr >>= 3;
            xmc[38] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            nc[3] = (sr & 0x7f) as i16; sr >>= 7;
            sr |= (nx!() as u16) << 1;
            bc[3] = (sr & 0x3) as i16; sr >>= 2;
            mc[3] = (sr & 0x3) as i16; sr >>= 2;
            sr |= (nx!() as u16) << 5;
            xmaxc[3] = (sr & 0x3f) as i16; sr >>= 6;
            xmc[39] = (sr & 0x7) as i16; sr >>= 3;
            xmc[40] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 1;
            xmc[41] = (sr & 0x7) as i16; sr >>= 3;
            xmc[42] = (sr & 0x7) as i16; sr >>= 3;
            xmc[43] = (sr & 0x7) as i16; sr >>= 3;
            sr = nx!() as u16;
            xmc[44] = (sr & 0x7) as i16; sr >>= 3;
            xmc[45] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 2;
            xmc[46] = (sr & 0x7) as i16; sr >>= 3;
            xmc[47] = (sr & 0x7) as i16; sr >>= 3;
            xmc[48] = (sr & 0x7) as i16; sr >>= 3;
            sr |= (nx!() as u16) << 1;
            xmc[49] = (sr & 0x7) as i16; sr >>= 3;
            xmc[50] = (sr & 0x7) as i16; sr >>= 3;
            xmc[51] = (sr & 0x7) as i16; sr >>= 3;
        }
        self.gsm_decoder(&larc, &nc, &bc, &mc, &xmaxc, &xmc, target);
    }

    /// `Gsm_Decoder` (decode.c, 4.3).
    #[allow(clippy::too_many_arguments)]
    fn gsm_decoder(
        &mut self,
        larcr: &[i16; 8],
        ncr: &[i16; 4],
        bcr: &[i16; 4],
        mcr: &[i16; 4],
        xmaxcr: &[i16; 4],
        xmcr: &[i16; 52],
        s: &mut [i16],
    ) {
        let mut wt = [0i16; 160];
        for j in 0..4 {
            let mut erp = [0i16; 40];
            self.rpe_decoding(xmaxcr[j], mcr[j], &xmcr[j * 13..j * 13 + 13], &mut erp);
            self.long_term_synthesis(ncr[j], bcr[j], &erp);
            // drp = dp0 + 120
            wt[j * 40..j * 40 + 40].copy_from_slice(&self.dp0[120..160]);
        }
        self.short_term_synthesis_filter(larcr, &wt, s);
        self.postprocessing(s);
    }

    /// `Postprocessing`: de-emphasis, upscale ×2, truncate to `& 0xFFF8`.
    fn postprocessing(&mut self, s: &mut [i16]) {
        let mut msr = self.msr;
        for x in s.iter_mut().take(160) {
            let tmp = mult_r(msr, 28180);
            msr = add(*x, tmp);
            *x = add(msr, msr) & !7;
        }
        self.msr = msr;
    }

    /// `Gsm_RPE_Decoding`.
    fn rpe_decoding(&self, xmaxcr: i16, mcr: i16, xmcr: &[i16], erp: &mut [i16; 40]) {
        // APCM_quantization_xmaxc_to_exp_mant
        let mut exp: i16 = 0;
        if xmaxcr > 15 {
            exp = (xmaxcr >> 3) - 1;
        }
        let mut mant: i16 = xmaxcr - (exp << 3);
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
        // APCM_inverse_quantization
        let temp1 = FAC[(mant & 7) as usize];
        let temp2 = sub(6, exp) as i32;
        let temp3 = asl(1, temp2 - 1);
        let mut xmp = [0i16; 13];
        for i in 0..13 {
            let mut temp: i16 = (xmcr[i] << 1) - 7;
            temp <<= 12;
            temp = mult_r(temp1, temp);
            temp = add(temp, temp3);
            xmp[i] = asr(temp, temp2);
        }
        // RPE_grid_positioning
        let m = (mcr & 3) as usize;
        for (i, x) in xmp.iter().enumerate() {
            erp[m + 3 * i] = *x;
        }
    }

    /// `Gsm_Long_Term_Synthesis_Filtering`; `drp` = `dp0[120..]`.
    fn long_term_synthesis(&mut self, ncr: i16, bcr: i16, erp: &[i16; 40]) {
        let nr = if !(40..=120).contains(&ncr) { self.nrp } else { ncr };
        self.nrp = nr;
        let nr = nr as usize;
        let brp = QLB[(bcr & 3) as usize];
        for k in 0..40 {
            let drpp = mult_r(brp, self.dp0[120 + k - nr]);
            self.dp0[120 + k] = add(erp[k], drpp);
        }
        self.dp0.copy_within(40..160, 0);
    }

    /// `Gsm_Short_Term_Synthesis_Filter`.
    fn short_term_synthesis_filter(&mut self, larcr: &[i16; 8], wt: &[i16; 160], s: &mut [i16]) {
        let cur = self.j;
        let prev = cur ^ 1;
        self.j = prev;
        // Decoding_of_the_coded_Log_Area_Ratios
        const MIC: [i16; 8] = [-32, -32, -16, -16, -8, -8, -4, -4];
        const B2: [i16; 8] = [0, 0, 4096, -5120, 188, -3584, -682, -2288];
        const INVA: [i16; 8] = [13107, 13107, 13107, 13107, 19223, 17476, 31454, 29708];
        for i in 0..8 {
            let mut t = (add(larcr[i], MIC[i]) as i32) << 10;
            t = sub(t as i16, B2[i]) as i32;
            let t = mult_r(INVA[i], t as i16);
            self.larpp[cur][i] = add(t, t);
        }
        let lj = self.larpp[cur];
        let lj1 = self.larpp[prev];
        let mut larp = [0i16; 8];

        for i in 0..8 {
            larp[i] = add(lj1[i] >> 2, lj[i] >> 2);
            larp[i] = add(larp[i], lj1[i] >> 1);
        }
        self.larp_to_rp(&mut larp);
        self.synth_filter(&larp, 13, &wt[0..], s, 0);

        for i in 0..8 {
            larp[i] = add(lj1[i] >> 1, lj[i] >> 1);
        }
        self.larp_to_rp(&mut larp);
        self.synth_filter(&larp, 14, &wt[13..], s, 13);

        for i in 0..8 {
            larp[i] = add(lj1[i] >> 2, lj[i] >> 2);
            larp[i] = add(larp[i], lj[i] >> 1);
        }
        self.larp_to_rp(&mut larp);
        self.synth_filter(&larp, 13, &wt[27..], s, 27);

        larp = lj;
        self.larp_to_rp(&mut larp);
        self.synth_filter(&larp, 120, &wt[40..], s, 40);
    }

    /// `LARp_to_rp`.
    fn larp_to_rp(&self, larp: &mut [i16; 8]) {
        for x in larp.iter_mut() {
            let neg = *x < 0;
            let temp: i16 = if neg {
                if *x == i16::MIN {
                    i16::MAX
                } else {
                    -*x
                }
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

    /// `Short_term_synthesis_filtering` over `k` samples; `wt` input, results at `s[off..off+k]`.
    fn synth_filter(&mut self, rrp: &[i16; 8], k: usize, wt: &[i16], s: &mut [i16], off: usize) {
        let v = &mut self.v;
        for n in 0..k {
            let mut sri = wt[n];
            for i in (0..8).rev() {
                sri = sub(sri, mult_r(rrp[i], v[i]));
                v[i + 1] = add(v[i], mult_r(rrp[i], sri));
            }
            v[0] = sri;
            s[off + n] = sri;
        }
    }
}

/// Decode consecutive 65-byte WAV49 blocks of `data` with one fresh [`GsmDecoder`] into `out` as
/// `pcm / 32768` (`(pcm as f64 / 32768.0) as f32`, as the C `(float)((double)s / 32768.0)`).
///
/// Writes at most `out.len()` samples (the last block may be cut short) and stops when `out` is full or
/// `data` runs out; a trailing partial block (`data.len() % 65` bytes) is ignored. Empty `data` → `Ok(0)`.
/// Returns the number of samples written. All bit patterns decode, so no error is ever returned today.
pub fn decode(data: &[u8], out: &mut [f32]) -> Result<usize, GsmError> {
    let mut dec = GsmDecoder::new();
    let mut written = 0usize;
    let mut pcm = [0i16; BLOCK_SAMPLES];
    for chunk in data.chunks_exact(BLOCK_BYTES) {
        if written >= out.len() {
            break;
        }
        let mut block = [0u8; BLOCK_BYTES];
        block.copy_from_slice(chunk);
        dec.decode_block(&block, &mut pcm);
        for &s in pcm.iter() {
            if written >= out.len() {
                break;
            }
            out[written] = (s as f64 / 32768.0) as f32;
            written += 1;
        }
    }
    Ok(written)
}

// ---- WaveSabre sample chunk (Specimen, Thunder) ----

use synth_core::mix::LoadError;
use synth_core::pool::Pool;

/// Parsed GSM sample chunk (shared with Thunder).
pub struct GsmChunk<'a> {
    pub blocks: &'a [u8],
    /// Decoded length: upstream ACM output is capped at the stored uncompressed size.
    pub samples: usize,
    /// Bytes after the compressed data (Specimen params + trailing size).
    pub rest: &'a [u8],
}

impl<'a> GsmChunk<'a> {
    /// `None` for an empty chunk (upstream: no sample loaded).
    pub fn parse(c: &'a [u8]) -> Result<Option<Self>, LoadError> {
        if c.is_empty() {
            return Ok(None);
        }
        let i32_at = |o: usize| c.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let u16_at = |o: usize| c.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let (comp, uncomp) = (i32_at(0).ok_or(LoadError::BadChunk)?, i32_at(4).ok_or(LoadError::BadChunk)?);
        let (tag, channels) = (u16_at(8).ok_or(LoadError::BadChunk)?, u16_at(10).ok_or(LoadError::BadChunk)?);
        let cb = u16_at(24).ok_or(LoadError::BadChunk)? as usize;
        if tag != 0x31 || channels != 1 || comp < 0 {
            return Err(LoadError::BadChunk);
        }
        let start = 8 + 18 + cb;
        let end = start.checked_add(comp as usize).ok_or(LoadError::BadChunk)?;
        let blocks = c.get(start..end).ok_or(LoadError::BadChunk)?;
        let decoded = blocks.len() / 65 * 320;
        let samples = if uncomp > 0 && (uncomp as usize) < decoded { uncomp as usize } else { decoded };
        Ok(Some(GsmChunk { blocks, samples, rest: &c[end..] }))
    }

    pub fn pool_need(c: &[u8]) -> Result<usize, LoadError> {
        Ok(GsmChunk::parse(c)?.map_or(0, |g| Pool::need::<f32>(g.samples)))
    }

    /// Decodes into the pool.
    pub fn decode(&self, pool: &mut Pool<'a>) -> Result<&'a [f32], LoadError> {
        let buf = pool.slice::<f32>(self.samples)?;
        let n = decode(self.blocks, buf).map_err(|_| LoadError::BadChunk)?;
        Ok(&buf[..n])
    }
}

/// Bytes of a GSM sample-chunk header (see [`write_chunk_header`]).
pub const CHUNK_HEADER: usize = 28;

/// Write the header of a GSM sample chunk, the inverse of [`GsmChunk::parse`] with `cbSize = 2`:
/// `i32` compressed size, `i32` uncompressed size, then `WAVEFORMATEX { tag 0x31, channels 1, rate 44100,
/// avg bytes/s 8957, block align 65, bits 0, cbSize 2 }` + `u16 320` (samples per block). The compressed
/// blocks (`compressed` bytes, a multiple of 65) follow directly; append them after the header.
///
/// The uncompressed field is set to `samples` (the **sample count**), because [`GsmChunk::parse`] reads it as
/// a cap on the decoded length (`samples = uncomp` when `0 < uncomp < blocks*320`), so this is the value that
/// makes `parse` return exactly `samples`. (Upstream WaveSabre/ACM files store the uncompressed *byte* size,
/// about `samples * 2`; `parse` ignores such a value since it exceeds the decoded length and falls back to
/// `blocks * 320`, i.e. a padded tail.)
pub fn write_chunk_header(compressed: usize, samples: usize, out: &mut [u8; CHUNK_HEADER]) {
    out[0..4].copy_from_slice(&(compressed as i32).to_le_bytes());
    out[4..8].copy_from_slice(&(samples as i32).to_le_bytes());
    out[8..10].copy_from_slice(&0x31u16.to_le_bytes());
    out[10..12].copy_from_slice(&1u16.to_le_bytes());
    out[12..16].copy_from_slice(&44100u32.to_le_bytes());
    out[16..20].copy_from_slice(&8957u32.to_le_bytes());
    out[20..22].copy_from_slice(&(BLOCK_BYTES as u16).to_le_bytes());
    out[22..24].copy_from_slice(&0u16.to_le_bytes());
    out[24..26].copy_from_slice(&2u16.to_le_bytes());
    out[26..28].copy_from_slice(&(BLOCK_SAMPLES as u16).to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENC: &[u8] = include_bytes!("../testdata/gsm_vectors.gsm");
    const PCM: &[u8] = include_bytes!("../testdata/gsm_vectors.pcm");

    fn ref_pcm() -> std::vec::Vec<i16> {
        PCM.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
    }

    #[test]
    fn vector_sizes() {
        assert_eq!(ENC.len(), 16 * BLOCK_BYTES);
        assert_eq!(PCM.len(), 16 * BLOCK_SAMPLES * 2);
    }

    #[test]
    fn decode_block_matches_libgsm_exactly() {
        let want = ref_pcm();
        let mut dec = GsmDecoder::new();
        let mut out = [0i16; BLOCK_SAMPLES];
        for (b, blk) in ENC.chunks_exact(BLOCK_BYTES).enumerate() {
            dec.decode_block(blk.try_into().unwrap(), &mut out);
            assert_eq!(&out[..], &want[b * BLOCK_SAMPLES..(b + 1) * BLOCK_SAMPLES], "block {b}");
        }
        assert!(want.iter().any(|&s| s != 0));
    }

    #[test]
    fn decode_f32_matches_and_respects_len() {
        let want = ref_pcm();
        let mut out = std::vec![0f32; want.len()];
        assert_eq!(decode(ENC, &mut out), Ok(want.len()));
        for (o, w) in out.iter().zip(&want) {
            assert_eq!(*o, (*w as f64 / 32768.0) as f32);
        }
        // shorter out: partial block, nothing past out.len() written
        let mut part = std::vec![7.0f32; 500];
        assert_eq!(decode(ENC, &mut part), Ok(500));
        assert_eq!(part[499], out[499]);
        let mut big = std::vec![7.0f32; want.len() + 10];
        assert_eq!(decode(ENC, &mut big), Ok(want.len()));
        assert_eq!(big[want.len()], 7.0);
        // trailing partial block ignored, empty data
        assert_eq!(decode(&ENC[..BLOCK_BYTES + 64], &mut out), Ok(BLOCK_SAMPLES));
        assert_eq!(decode(&[], &mut out), Ok(0));
        assert_eq!(decode(ENC, &mut []), Ok(0));
    }

    #[test]
    fn garbage_blocks_do_not_panic() {
        let mut dec = GsmDecoder::new();
        let mut out = [0i16; BLOCK_SAMPLES];
        dec.decode_block(&[0xFF; BLOCK_BYTES], &mut out);
        dec.decode_block(&[0x00; BLOCK_BYTES], &mut out);
        let mut s = 12345u32;
        let mut data = std::vec![0u8; 100 * BLOCK_BYTES];
        for b in data.iter_mut() {
            s = s.wrapping_mul(1103515245).wrapping_add(12345);
            *b = (s >> 16) as u8;
        }
        let mut f = std::vec![0f32; 100 * BLOCK_SAMPLES];
        assert_eq!(decode(&data, &mut f), Ok(f.len()));
        // every single-byte value pattern and alternating patterns, on a continuing decoder
        for v in 0..=255u8 {
            dec.decode_block(&[v; BLOCK_BYTES], &mut out);
            let mut blk = [0u8; BLOCK_BYTES];
            for (i, b) in blk.iter_mut().enumerate() {
                *b = if i % 2 == 0 { v } else { !v };
            }
            dec.decode_block(&blk, &mut out);
        }
    }

    // ---- encoder: bit-exact against libgsm 1.0.24 (-DWAV49, no FAST/USE_FLOAT_MUL/LTP_CUT) ----
    //
    // Vectors in testdata/: `enc_in.pcm` (s16le mono, 80423 samples: silence, sine sweep, full-scale noise,
    // quiet noise, plucked tone, full-scale squares incl. -32768/32767, Nyquist alternation, DC sine, odd tail)
    // and `enc_ref.gsm` (libgsm output, 252 x 65-byte blocks). Regenerate with:
    //   python3 testdata/gen_enc_vectors.py testdata/enc_in.pcm
    //   /tmp/libgsm/enc49 < testdata/enc_in.pcm > testdata/enc_ref.gsm
    // (enc49.c: gsm_create + GSM_OPT_WAV49, gsm_encode per 160 samples, 32 + 33 bytes per pair, zero-padded tail).
    const ENC_IN: &[u8] = include_bytes!("../testdata/enc_in.pcm");
    const ENC_REF: &[u8] = include_bytes!("../testdata/enc_ref.gsm");

    fn enc_in() -> std::vec::Vec<i16> {
        ENC_IN.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
    }

    #[test]
    fn encoder_matches_libgsm() {
        let pcm = enc_in();
        assert_eq!(pcm.len() % BLOCK_SAMPLES, 103, "odd tail present");
        assert_eq!(ENC_REF.len(), blocks_for(pcm.len()) * BLOCK_BYTES);
        let mut padded = pcm.clone();
        padded.resize(blocks_for(pcm.len()) * BLOCK_SAMPLES, 0);
        let mut enc = GsmEncoder::new();
        let mut out = [0u8; BLOCK_BYTES];
        for (b, (src, want)) in padded.chunks_exact(BLOCK_SAMPLES).zip(ENC_REF.chunks_exact(BLOCK_BYTES)).enumerate() {
            enc.encode_block(src.try_into().unwrap(), &mut out);
            if let Some(i) = (0..BLOCK_BYTES).find(|&i| out[i] != want[i]) {
                panic!("block {b} byte {i}: got {:#04x} want {:#04x}", out[i], want[i]);
            }
        }
    }

    #[test]
    fn encode_f32_matches_libgsm() {
        let pcm = enc_in();
        let f: std::vec::Vec<f32> = pcm.iter().map(|&s| s as f32 / 32768.0).collect();
        let mut out = std::vec![0u8; blocks_for(f.len()) * BLOCK_BYTES];
        assert_eq!(encode(&f, &mut out), ENC_REF.len());
        assert!(out == ENC_REF, "f32 path differs from libgsm output");
        assert_eq!(blocks_for(0), 0);
        assert_eq!(blocks_for(1), 1);
        assert_eq!(blocks_for(320), 1);
        assert_eq!(blocks_for(321), 2);
        assert_eq!(encode(&[], &mut []), 0);
        // out-of-range / NaN input converts like the saturating cast
        let mut one = [0u8; BLOCK_BYTES];
        assert_eq!(encode(&[2.0, -2.0, f32::NAN], &mut one), BLOCK_BYTES);
    }

    #[test]
    fn encoder_round_trip_decodes_like_reference() {
        let pcm = enc_in();
        let f: std::vec::Vec<f32> = pcm.iter().map(|&s| s as f32 / 32768.0).collect();
        let mut ours = std::vec![0u8; blocks_for(f.len()) * BLOCK_BYTES];
        encode(&f, &mut ours);
        let n = blocks_for(f.len()) * BLOCK_SAMPLES;
        let mut a = std::vec![0f32; n];
        let mut b = std::vec![0f32; n];
        assert_eq!(decode(&ours, &mut a), Ok(n));
        assert_eq!(decode(ENC_REF, &mut b), Ok(n));
        assert!(a == b);
        // sine sweep segment (samples 4000..24000): lossy but close, SNR > 5 dB
        let (mut sig, mut err) = (0f64, 0f64);
        for i in 4000..24000 {
            let x = pcm[i] as f64 / 32768.0;
            sig += x * x;
            err += (x - a[i] as f64) * (x - a[i] as f64);
        }
        let snr = 10.0 * (sig / err).log10();
        assert!(snr > 5.0, "SNR {snr} dB");
    }

    #[test]
    fn chunk_header_round_trip() {
        let pcm = enc_in();
        let f: std::vec::Vec<f32> = pcm.iter().map(|&s| s as f32 / 32768.0).collect();
        let nb = blocks_for(f.len());
        let mut chunk = std::vec![0u8; CHUNK_HEADER + nb * BLOCK_BYTES];
        let (h, body) = chunk.split_at_mut(CHUNK_HEADER);
        encode(&f, body);
        write_chunk_header(nb * BLOCK_BYTES, f.len(), h.try_into().unwrap());
        let g = GsmChunk::parse(&chunk).unwrap().unwrap();
        assert_eq!(g.samples, f.len());
        assert!(g.rest.is_empty());
        assert_eq!(g.blocks, ENC_REF);
        // exact block multiple and the empty chunk
        let mut h2 = [0u8; CHUNK_HEADER];
        write_chunk_header(BLOCK_BYTES, BLOCK_SAMPLES, &mut h2);
        let mut c2 = h2.to_vec();
        c2.extend_from_slice(&ENC_REF[..BLOCK_BYTES]);
        let g = GsmChunk::parse(&c2).unwrap().unwrap();
        assert_eq!(g.samples, BLOCK_SAMPLES);
        assert!(g.rest.is_empty());
    }

    #[test]
    fn chunk_header_layout_matches_real_chunk() {
        // header of a real song chunk (base64 "XBcAACLkAAAxAAEARKwAAP0iAABBAAAAAgBAAQ==")
        let real: [u8; 28] = [
            0x5C, 0x17, 0, 0, 0x22, 0xE4, 0, 0, 0x31, 0, 1, 0, 0x44, 0xAC, 0, 0, 0xFD, 0x22, 0, 0, 0x41, 0, 0, 0, 2, 0,
            0x40, 0x01,
        ];
        let mut h = [0u8; CHUNK_HEADER];
        write_chunk_header(0x175C, 0xE422, &mut h);
        assert_eq!(h, real);
    }
}

