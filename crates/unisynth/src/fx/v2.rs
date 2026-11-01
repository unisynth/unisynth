//! V2's global effects as effect modes (change add-v2-import): reverb `v2` (kind 71 mode 3), delay `v2` (68 mode 2),
//! compressor `v2` (72 mode 1, V2's sum compressor) and EQ `cut` (67 mode 2, V2's master DC filter and post low/high
//! cut). Parameters are V2 raw / 128 in V2's order, then the source format version (`count(0, 6)`, default 6) for
//! the behaviours that changed: the reverb low cut (version 4), the DC filter and denormal bias (version 6).
//! Reverb and delay read their input from the left channel (V2's mono aux buses: the instruments' sends put the
//! same mono mix on both channels) and output only the wet signal. Port of V2 (public domain) after v2redux (CC0).

use super::{Effect, Params};
use crate::v2::{dcf_r, linfreq, Dcf};
use crate::v2fx::{Comp, CompP, ModDel, ModDelP};
use synth_core::device::{Ctx, Io};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

/// Format version from a parameter (`count(0, 6)`).
fn era(p: f32) -> u32 {
    crate::law::count(p, 0, 6)
}

fn raw(p: f32) -> f32 {
    p * 128.0
}

// ---- EQ cut: DC filter (version 6), low cut, high cut ----

pub struct Cut {
    lc: f32,
    hc: f32,
    era: u32,
    dcf: [Dcf; 2],
    lcbuf: [f32; 2],
    hcbuf: [f32; 2],
}

impl<'a> Effect<'a> for Cut {
    const PARAMS: usize = 3;
    fn need(_: &Env, _: &Params) -> usize {
        0
    }
    fn new(_: &Env, _: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(Cut { lc: 0.0, hc: 127.0 / 128.0, era: 6, dcf: [Dcf::default(); 2], lcbuf: [0.0; 2], hcbuf: [0.0; 2] })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.lc = v,
            1 => self.hc = v,
            2 => self.era = era(v),
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.dcf = [Dcf::default(); 2];
        self.lcbuf = [0.0; 2];
        self.hcbuf = [0.0; 2];
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        let sq = |x: f32| x * x;
        let (lcf, hcf) = (sq((raw(self.lc) + 1.0) / 128.0), sq((raw(self.hc) + 1.0) / 128.0));
        let r = dcf_r(ctx.rate as f32);
        for ch in 0..2 {
            for x in io.ch[ch][..n].iter_mut() {
                let mut v = *x;
                if self.era >= 6 {
                    v = self.dcf[ch].step(v, r);
                }
                let y = v - self.lcbuf[ch];
                self.lcbuf[ch] += lcf * y;
                *x = if hcf != 1.0 {
                    self.hcbuf[ch] += hcf * (y - self.hcbuf[ch]);
                    self.hcbuf[ch]
                } else {
                    y
                };
            }
        }
    }
}

// ---- compressor v2 (the sum compressor) ----

pub struct CompressorV2<'a> {
    p: [f32; 9],
    era: u32,
    comp: Comp<'a>,
}

impl<'a> Effect<'a> for CompressorV2<'a> {
    const PARAMS: usize = 10;
    fn need(_: &Env, _: &Params) -> usize {
        Comp::need()
    }
    fn new(_: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let d = [0.0, 0.0, 1.0, 2.0, 90.0, 32.0, 20.0, 64.0, 64.0].map(|x| x / 128.0);
        Ok(CompressorV2 { p: d, era: 6, comp: Comp::new(pool).map_err(|_| LoadError::OutOfMemory)? })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0..=8 => self.p[i] = v,
            9 => self.era = era(v),
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.comp.clear();
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        let q = self.p.map(raw);
        let p = CompP::new(q[0], q[1], q[2], q[3], q[4], q[5], q[6], q[7], q[8], ctx.rate as f32);
        self.comp.set(&p);
        let [l, r, ..] = &mut io.ch;
        self.comp.render(&p, &mut l[..n], &mut r[..n]);
    }
}

// ---- delay v2 (the modulated stereo delay) ----

/// V2's main delay buffer per side.
const DELAY_LEN: usize = 32_768;

pub struct DelayV2<'a> {
    p: [f32; 7],
    era: u32,
    del: ModDel<'a>,
}

impl<'a> Effect<'a> for DelayV2<'a> {
    const PARAMS: usize = 8;
    fn need(_: &Env, _: &Params) -> usize {
        ModDel::need(DELAY_LEN)
    }
    fn new(_: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let d = [100.0, 80.0, 64.0, 64.0, 0.0, 0.0, 64.0].map(|x| x / 128.0);
        Ok(DelayV2 { p: d, era: 6, del: ModDel::new(pool, DELAY_LEN).map_err(|_| LoadError::OutOfMemory)? })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0..=6 => self.p[i] = v,
            7 => self.era = era(v),
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.del.clear();
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        let q = self.p.map(raw);
        let p = ModDelP::new(q[0], q[1], q[2], q[3], q[4], q[5], q[6], DELAY_LEN, ctx.rate as f32);
        let mut x = [0.0f32; 128];
        let mut pos = 0;
        while pos < n {
            let len = (n - pos).min(128);
            x[..len].copy_from_slice(&io.ch[0][pos..pos + len]);
            let [l, r, ..] = &mut io.ch;
            l[pos..pos + len].fill(0.0);
            r[pos..pos + len].fill(0.0);
            self.del.render_add(&p, &x[..len], &mut l[pos..pos + len], &mut r[pos..pos + len]);
            pos += len;
        }
    }
}

// ---- reverb v2 ----

const COMB: [[usize; 4]; 2] = [[1309, 1635, 1811, 1926], [1327, 1631, 1833, 1901]];
const ALL: [[usize; 2]; 2] = [[220, 74], [205, 77]];
const GAINC: [f32; 4] = [0.966_384_6, 0.958_186_36, 0.953_783_9, 0.950_933_2];
const GAINA: [f32; 2] = [0.994_260_1, 0.998_044_7];

/// A fixed-length delay line (`V2Delay`).
struct Line<'a> {
    buf: &'a mut [f32],
    pos: usize,
}

impl Line<'_> {
    #[inline]
    fn fetch(&self) -> f32 {
        self.buf[self.pos]
    }
    #[inline]
    fn feed(&mut self, v: f32) {
        self.buf[self.pos] = v;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
    }
    fn clear(&mut self) {
        self.buf.fill(0.0);
        self.pos = 0;
    }
}

pub struct ReverbV2<'a> {
    p: [f32; 4],
    era: u32,
    comb: [[Line<'a>; 4]; 2],
    combl: [[f32; 4]; 2],
    all: [[Line<'a>; 2]; 2],
    hpf: [f32; 2],
}

impl<'a> Effect<'a> for ReverbV2<'a> {
    const PARAMS: usize = 5;
    fn need(_: &Env, _: &Params) -> usize {
        COMB.iter().flatten().chain(ALL.iter().flatten()).map(|&n| Pool::need::<f32>(n)).sum()
    }
    fn new(_: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let mut line = |n: usize| -> Result<Line<'a>, LoadError> { Ok(Line { buf: pool.slice(n).map_err(|_| LoadError::OutOfMemory)?, pos: 0 }) };
        let comb = [
            [line(COMB[0][0])?, line(COMB[0][1])?, line(COMB[0][2])?, line(COMB[0][3])?],
            [line(COMB[1][0])?, line(COMB[1][1])?, line(COMB[1][2])?, line(COMB[1][3])?],
        ];
        let all = [[line(ALL[0][0])?, line(ALL[0][1])?], [line(ALL[1][0])?, line(ALL[1][1])?]];
        let d = [64.0, 64.0, 32.0, 127.0].map(|x| x / 128.0);
        Ok(ReverbV2 { p: d, era: 6, comb, combl: [[0.0; 4]; 2], all, hpf: [0.0; 2] })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0..=3 => self.p[i] = v,
            4 => self.era = era(v),
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.comb.iter_mut().flatten().for_each(Line::clear);
        self.all.iter_mut().flatten().for_each(Line::clear);
        self.combl = [[0.0; 4]; 2];
        self.hpf = [0.0; 2];
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        let lf = linfreq(ctx.rate as f32);
        let [time, highcut, lowcut, vol] = self.p.map(raw);
        let sq = |x: f32| x * x;
        let e = lf * sq(64.0 / (time + 1.0));
        let gainc = GAINC.map(|g| crate::fm::powf(g, e));
        let gaina = GAINA.map(|g| crate::fm::powf(g, e));
        let damp = lf * (highcut / 128.0);
        let gainin = vol / 128.0;
        // no low cut stage before format version 4 (its parameter came then)
        let low = if self.era >= 4 { lf * sq(sq(lowcut / 128.0)) } else { 0.0 };
        for i in 0..n {
            let x = io.ch[0][i] * gainin;
            for ch in 0..2 {
                let mut cur = 0.0;
                for j in 0..4 {
                    let dv = gainc[j] * self.comb[ch][j].fetch();
                    let nv = if j & 1 == 1 { dv - x } else { dv + x };
                    let lp = self.combl[ch][j] + damp * (nv - self.combl[ch][j]);
                    self.combl[ch][j] = lp;
                    self.comb[ch][j].feed(lp);
                    cur += lp;
                }
                for j in 0..2 {
                    let dv = self.all[ch][j].fetch();
                    let dz = cur + gaina[j] * dv;
                    self.all[ch][j].feed(dz);
                    cur = dv - gaina[j] * dz;
                }
                self.hpf[ch] += low * (cur - self.hpf[ch]);
                io.ch[ch][i] = cur - self.hpf[ch];
            }
        }
    }
}
