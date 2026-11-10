//! Elevated's global effects (change add-elevated-import, design D6): Mentor's 4k synthesizer runs them per sample
//! over the whole song. Every mode is its own feature; the laws below map the normalized parameters (after the mode)
//! to the original's values, wide enough for any song of that synth.
//!
//! | mode | parameters | law |
//! |---|---|---|
//! | distortion `atan` | a, b | `y = b·atan(x/a)`; `a = 10^(6p − 3)`, `b = 16p` (the V2 overdrive arithmetic, exact arctangent) |
//! | distortion `sine` | a, b | `y = b·sin(a·x)`; `a = 10^(6p − 3)`, `b = 16p` |
//! | distortion `knee` | threshold t, ratio r, offset o | `y = sign(x)·(t + (|x| − t)·r + o)` for `|x| ≥ t`, else `y = x`; `t = 4p`, `r = 4p`, `o = 4p − 2` |
//! | EQ `svf-lfo` | cutoff, resonance q, LFO 1 rate, depth, LFO 2 rate, depth, dry, output | the shared state-variable step once per sample (`flt::run`, input form) at `g = (cutoff + l₁ + l₂)·SCALE`, output low/high/band + `dry·x`; cutoff and depths `65 536p` (the original's units), `q = 2p`, rates `0.01p`, dry `2p − 1`, output `Choice(3)` |
//! | delay `ping-pong` | length, feedback g | one stereo line of `max(1, round(65 536p))` frames: `y = x + g·w`, the line stores `(y_R, y_L)`; `g = 2p − 1` |
//! | delay `allpass` | length, feedback g | `v_L = x_L + g·w_R`, `v_R = x_R + g·w_L`, the line stores `v`, `y_L = w_R − g·v_L`, `y_R = w_L − g·v_R` |
//!
//! The filter's LFOs are the original's coupled-form recurrence `c ← c − r·s; s ← s + r·c` from `s = 0`, `c = depth`,
//! one step before each sample. That recurrence is a rotation (determinant 1), so it is computed in closed form from the
//! absolute song frame `f`: `s = A·sin(ω·(f + 1))`, `ω = 2·asin(r/2)`, `A = depth·r/sin ω`. A render that starts mid-song
//! (the DAW's seek) therefore matches a full one.

use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

/// `10^(6p − 3)`.
#[allow(dead_code)]
fn factor(p: f32) -> f32 {
    crate::fm::exp2f((6.0 * p.max(0.0).min(1.0) - 3.0) * core::f32::consts::LOG2_10)
}

// ---- distortion ----

#[cfg(feature = "us-fx-distortion-atan")]
pub struct Atan {
    a: f32,
    b: f32,
    p: crate::v2::DistP,
    d: crate::v2::Dist,
}

#[cfg(feature = "us-fx-distortion-atan")]
impl<'a> Effect<'a> for Atan {
    const PARAMS: usize = 2;
    fn need(_: &Env, _: &Params) -> usize {
        0
    }
    fn new(_: &Env, _: &mut Pool<'a>) -> Result<Self, LoadError> {
        let mut e = Atan { a: 1.0, b: 0.0, p: crate::v2::DistP::default(), d: crate::v2::Dist::default() };
        e.set(0, 0.5);
        Ok(e)
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.a = factor(v),
            1 => self.b = 16.0 * v.max(0.0).min(1.0),
            _ => return,
        }
        self.p = crate::v2::DistP::overdrive(1.0 / self.a, self.b, 0.0);
    }
    fn reset(&mut self) {}
    fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
        let [l, r, ..] = &mut io.ch;
        for (x, y) in l[..n].iter_mut().zip(r[..n].iter_mut()) {
            (*x, *y) = self.d.run(&self.p, *x, *y);
        }
    }
    fn quiet_after(&self) -> Option<u32> {
        Some(0)
    }
}

#[cfg(feature = "us-fx-distortion-sine")]
pub struct Sine {
    a: f32,
    b: f32,
}

#[cfg(feature = "us-fx-distortion-sine")]
impl<'a> Effect<'a> for Sine {
    const PARAMS: usize = 2;
    fn need(_: &Env, _: &Params) -> usize {
        0
    }
    fn new(_: &Env, _: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(Sine { a: 1.0, b: 0.0 })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.a = factor(v),
            1 => self.b = 16.0 * v.max(0.0).min(1.0),
            _ => {}
        }
    }
    fn reset(&mut self) {}
    fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
        let (a, b) = (self.a, self.b);
        for c in 0..2 {
            io.ch[c][..n].iter_mut().for_each(|v| *v = b * crate::fm::sinf(a * *v));
        }
    }
    fn quiet_after(&self) -> Option<u32> {
        Some(0)
    }
}

#[cfg(feature = "us-fx-distortion-knee")]
pub struct Knee {
    t: f32,
    r: f32,
    o: f32,
}

/// The knee curve: above the threshold `|x| − t` is scaled by `r` and `o` added, then `t` comes back and the sign of
/// `x` is restored; below it `x` passes.
#[cfg(feature = "us-fx-distortion-knee")]
#[inline]
pub fn knee(x: f32, t: f32, r: f32, o: f32) -> f32 {
    let m = libm::fabsf(x) - t;
    if m < 0.0 {
        x
    } else {
        (t + m * r + o).copysign(x)
    }
}

#[cfg(feature = "us-fx-distortion-knee")]
impl<'a> Effect<'a> for Knee {
    const PARAMS: usize = 3;
    fn need(_: &Env, _: &Params) -> usize {
        0
    }
    fn new(_: &Env, _: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(Knee { t: 1.0, r: 1.0, o: 0.0 })
    }
    fn set(&mut self, i: usize, v: f32) {
        let v = v.max(0.0).min(1.0);
        match i {
            0 => self.t = 4.0 * v,
            1 => self.r = 4.0 * v,
            2 => self.o = 4.0 * v - 2.0,
            _ => {}
        }
    }
    fn reset(&mut self) {}
    fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
        let (t, r, o) = (self.t, self.r, self.o);
        for c in 0..2 {
            io.ch[c][..n].iter_mut().for_each(|v| *v = knee(*v, t, r, o));
        }
    }
    /// With `t = 0` and an offset it plays DC on silence, so only then its track stays awake.
    fn quiet_after(&self) -> Option<u32> {
        (self.t > 0.0 || self.o == 0.0).then_some(0)
    }
}

// ---- EQ: the swept state-variable filter ----

/// The original's cutoff scale (the bits `0x3815_0000`, about 3.5524e-5).
#[cfg(feature = "us-fx-eq-svf-lfo")]
pub const SCALE: f32 = f32::from_bits(0x3815_0000);

/// One coupled-form LFO in closed form: `A·sin(ω·(f + 1))` at absolute frame `f`.
#[cfg(feature = "us-fx-eq-svf-lfo")]
#[derive(Clone, Copy, Default)]
pub struct Lfo {
    w: f64,
    amp: f64,
}

#[cfg(feature = "us-fx-eq-svf-lfo")]
impl Lfo {
    /// The LFO of rate `r` (per sample) and depth `c₀`.
    pub fn new(r: f32, depth: f32) -> Self {
        let r = r as f64;
        if r <= 0.0 || r > 0.01 {
            return Lfo::default(); // s stays 0 (r = 0); the rate law stops at 0.01
        }
        // ω = 2·asin(r/2): for r/2 ≤ 0.005 the series to x⁷ is exact to f64 (the next term is below 1e-19)
        let x = r / 2.0;
        let x2 = x * x;
        let w = 2.0 * x * (1.0 + x2 * (1.0 / 6.0 + x2 * (3.0 / 40.0 + x2 * (5.0 / 112.0))));
        Lfo { w, amp: depth as f64 * r / crate::fm::sin(w) }
    }
    #[inline]
    pub fn at(&self, f: u64) -> f32 {
        if self.amp == 0.0 {
            return 0.0;
        }
        // reduce the phase in turns first so a long song keeps its precision
        let turns = self.w / core::f64::consts::TAU * (f + 1) as f64;
        (self.amp * crate::fm::sin(core::f64::consts::TAU * (turns - libm::floor(turns)))) as f32
    }
}

#[cfg(feature = "us-fx-eq-svf-lfo")]
pub struct SvfLfo {
    cutoff: f32,
    q: f32,
    lfo_p: [(f32, f32); 2],
    lfo: [Lfo; 2],
    dry: f32,
    kind: crate::flt::Kind,
    s: [[f32; crate::flt::NS]; 2],
}

#[cfg(feature = "us-fx-eq-svf-lfo")]
impl<'a> Effect<'a> for SvfLfo {
    const PARAMS: usize = 8;
    fn need(_: &Env, _: &Params) -> usize {
        0
    }
    fn new(_: &Env, _: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(SvfLfo {
            cutoff: 0.0,
            q: 0.0,
            lfo_p: [(0.0, 0.0); 2],
            lfo: [Lfo::default(); 2],
            dry: -1.0,
            kind: crate::flt::Kind::SvfLp,
            s: [[0.0; crate::flt::NS]; 2],
        })
    }
    fn set(&mut self, i: usize, v: f32) {
        let v = v.max(0.0).min(1.0);
        match i {
            0 => self.cutoff = 65_536.0 * v,
            1 => self.q = 2.0 * v,
            2 | 4 => self.lfo_p[(i - 2) / 2].0 = 0.01 * v,
            3 | 5 => self.lfo_p[(i - 3) / 2].1 = 65_536.0 * v,
            6 => self.dry = 2.0 * v - 1.0,
            7 => {
                use crate::flt::Kind::*;
                self.kind = [SvfLp, SvfHp, SvfBp][crate::law::choice(v, 3)];
            }
            _ => {}
        }
        if (2..=5).contains(&i) {
            let k = if i < 4 { 0 } else { 1 };
            self.lfo[k] = Lfo::new(self.lfo_p[k].0, self.lfo_p[k].1);
        }
    }
    fn reset(&mut self) {
        self.s = [[0.0; crate::flt::NS]; 2];
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        let [l, r, ..] = &mut io.ch;
        for (j, (x, y)) in l[..n].iter_mut().zip(r[..n].iter_mut()).enumerate() {
            let f = ctx.frame + j as u64;
            let g = (self.cutoff + self.lfo[0].at(f) + self.lfo[1].at(f)) * SCALE;
            let c = [g, self.q, 1.0, 0.0, 1.0];
            let (dl, dr) = (self.dry * *x, self.dry * *y);
            *x = crate::flt::run(self.kind, &c, &mut self.s[0], *x) + dl;
            *y = crate::flt::run(self.kind, &c, &mut self.s[1], *y) + dr;
        }
    }
}

// ---- delay: the crossed lines ----

/// The longest line, in frames (the original's `MAX_DELAY_SAMPLES`).
#[cfg(any(feature = "us-fx-delay-pingpong", feature = "us-fx-delay-allpass"))]
pub const MAX_LEN: usize = 65_536;

/// One stereo line with the original's position rule: before each frame the position steps back one, wrapping to
/// `len − 1`; the frame reads and then writes at that position.
#[cfg(any(feature = "us-fx-delay-pingpong", feature = "us-fx-delay-allpass"))]
pub struct Line<'a> {
    /// Interleaved left/right frames.
    buf: &'a mut [f32],
    len: usize,
    pos: usize,
    g: f32,
    allpass: bool,
}

#[cfg(any(feature = "us-fx-delay-pingpong", feature = "us-fx-delay-allpass"))]
impl<'a> Line<'a> {
    fn new(pool: &mut Pool<'a>, allpass: bool) -> Result<Self, LoadError> {
        Ok(Line { buf: pool.slice(2 * MAX_LEN)?, len: 1, pos: 0, g: 0.0, allpass })
    }
    fn set(&mut self, i: usize, v: f32) {
        let v = v.max(0.0).min(1.0);
        match i {
            0 => {
                self.len = (libm::roundf(v * MAX_LEN as f32) as usize).clamp(1, MAX_LEN);
                self.pos %= self.len;
            }
            1 => self.g = 2.0 * v - 1.0,
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|f| *f = 0.0);
        self.pos = 0;
    }
    fn run(&mut self, io: &mut Io, n: usize) {
        let [l, r, ..] = &mut io.ch;
        let g = self.g;
        for (x, y) in l[..n].iter_mut().zip(r[..n].iter_mut()) {
            self.pos = if self.pos == 0 { self.len - 1 } else { self.pos - 1 };
            let at = 2 * self.pos;
            let w = [self.buf[at], self.buf[at + 1]];
            let (a, b) = if self.allpass {
                let v = [*x + g * w[1], *y + g * w[0]];
                (*x, *y) = (w[1] - g * v[0], w[0] - g * v[1]);
                (v[0], v[1])
            } else {
                let (yl, yr) = (*x + g * w[0], *y + g * w[1]);
                (*x, *y) = (yl, yr);
                (yr, yl)
            };
            (self.buf[at], self.buf[at + 1]) = (a, b);
        }
    }
}

#[cfg(feature = "us-fx-delay-pingpong")]
pub struct PingPong<'a>(Line<'a>);

#[cfg(feature = "us-fx-delay-pingpong")]
impl<'a> Effect<'a> for PingPong<'a> {
    const PARAMS: usize = 2;
    fn need(_: &Env, _: &Params) -> usize {
        Pool::need::<f32>(2 * MAX_LEN)
    }
    fn new(_: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(PingPong(Line::new(pool, false)?))
    }
    fn set(&mut self, i: usize, v: f32) {
        self.0.set(i, v);
    }
    fn reset(&mut self) {
        self.0.reset();
    }
    fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
        self.0.run(io, n);
    }
}

#[cfg(feature = "us-fx-delay-allpass")]
pub struct Allpass<'a>(Line<'a>);

#[cfg(feature = "us-fx-delay-allpass")]
impl<'a> Effect<'a> for Allpass<'a> {
    const PARAMS: usize = 2;
    fn need(_: &Env, _: &Params) -> usize {
        Pool::need::<f32>(2 * MAX_LEN)
    }
    fn new(_: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(Allpass(Line::new(pool, true)?))
    }
    fn set(&mut self, i: usize, v: f32) {
        self.0.set(i, v);
    }
    fn reset(&mut self) {
        self.0.reset();
    }
    fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
        self.0.run(io, n);
    }
}

#[cfg(test)]
#[allow(unused_imports, dead_code)] // each test needs its own mode's feature
mod tests {
    use super::super::tests::{noise, old_mode_value, run, run_side, sine, with_fx};
    use crate::kind;

    /// Effect values `[mode, params…]` for Elevated mode `id` of kind `k` (v3 chunk; see `fx::tests::chunk`).
    fn vals(k: u8, id: usize, p: &[f32]) -> Vec<f32> {
        let mut v = vec![old_mode_value(k, id)];
        v.extend_from_slice(p);
        v
    }

    fn power(x: &[f32]) -> f64 {
        x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64
    }

    // ---- 3.1 svf-lfo ----

    #[cfg(feature = "us-fx-eq-svf-lfo")]
    #[test]
    fn one_step_is_a_hand_run_chamberlin_step() {
        // low += g·band; high = q·(x − band) − low; band += g·high, once per sample
        let (g, q) = (0.2f32, 0.4f32);
        let c = [g, q, 1.0, 0.0, 1.0];
        let mut s = [0.0f32; crate::flt::NS];
        let (mut low, mut band) = (0.0f32, 0.0f32);
        for (n, x) in sine(440.0, 44100.0, 400, 0.8).into_iter().enumerate() {
            low += g * band;
            let high = q * (x - band) - low;
            band += g * high;
            let y = crate::flt::run(crate::flt::Kind::SvfLp, &c, &mut s, x);
            assert_eq!(y, low, "sample {n}");
        }
    }

    #[cfg(feature = "us-fx-eq-svf-lfo")]
    #[test]
    fn a_static_svf_is_a_low_pass_whose_peak_rises_as_q_falls() {
        let gain = |hz: f32, q: f32| {
            // cutoff 4096 units: g = 0.1455, a corner near 1 kHz
            let p = [4096.0 / 65536.0, q / 2.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0];
            let x = sine(hz, 44100.0, 22050, 0.1);
            let (l, _) = with_fx(kind::EQ, &vals(kind::EQ, 3, &p), 44100.0, 120, |fx| run(fx, 44100.0, 120, &x, &x, 512));
            (power(&l[11025..]) / power(&x[11025..])).sqrt()
        };
        assert!(gain(100.0, 1.0) > 0.9 && gain(8000.0, 1.0) < 0.05, "low-pass: {} {}", gain(100.0, 1.0), gain(8000.0, 1.0));
        // the input form scales the input by q as well: the passband is q, the peak above it rises as q falls
        let peak = |q: f32| [600.0, 800.0, 1000.0, 1200.0, 1400.0].map(|f| gain(f, q)).into_iter().fold(0.0, f64::max) / gain(50.0, q);
        assert!(peak(0.3) > 2.0 * peak(1.0), "resonance: {} vs {}", peak(0.3), peak(1.0));
    }

    #[cfg(feature = "us-fx-eq-svf-lfo")]
    #[test]
    fn the_lfo_closed_form_is_the_coupled_recurrence() {
        // Atrium's sweeps: r 7e-6 depth 7168, r 7.2e-5 depth 1536; 10⁷ samples against an f64 recurrence
        for (r, depth) in [(7e-6f32, 7168.0f32), (7.2e-5, 1536.0), (2.9e-5, 3072.0)] {
            let lfo = super::Lfo::new(r, depth);
            let (rr, mut c, mut s) = (r as f64, depth as f64, 0.0f64);
            let mut worst = 0.0f64;
            for f in 0..10_000_000u64 {
                c -= rr * s;
                s += rr * c;
                if f % 997 == 0 {
                    worst = worst.max((lfo.at(f) as f64 - s).abs());
                }
            }
            assert!(worst < 1e-4 * depth as f64, "r {r}: {worst}");
        }
    }

    #[cfg(feature = "us-fx-eq-svf-lfo")]
    #[test]
    fn a_render_started_mid_song_matches_the_full_one() {
        // silence until frame 441 000, then a tone: the filter state is the same, the LFOs are at the same song time
        let p = [6016.0 / 65536.0, 0.3125 / 2.0, 0.0029, 3072.0 / 65536.0, 0.0006, 2048.0 / 65536.0, 0.5, 0.0];
        let v = vals(kind::EQ, 3, &p);
        let at = 441_000usize;
        let tone = sine(220.0, 44100.0, 20_000, 0.5);
        let mut x = vec![0.0f32; at];
        x.extend(&tone);
        let (full, _) = with_fx(kind::EQ, &v, 44100.0, 120, |fx| run(fx, 44100.0, 120, &x, &x, 4096));
        let (part, _) = with_fx(kind::EQ, &v, 44100.0, 120, |fx| run_side(fx, 44100.0, 120, &tone, &tone, None, 777, at as u64));
        for k in 0..tone.len() {
            assert!((full[at + k] - part[k]).abs() < 1e-4, "frame {}: {} vs {}", at + k, full[at + k], part[k]);
        }
    }

    // ---- 3.2 distortion ----

    #[cfg(feature = "us-fx-distortion-knee")]
    #[test]
    fn the_knee_transfer_matches_the_original() {
        use super::knee;
        assert_eq!([0.25, 0.9, -0.9].map(|x| knee(x, 0.5, 0.25, 0.0)), [0.25, 0.6, -0.6]);
        // Elevated's last machine (1, 0, 0) is a hard clip at ±1
        assert_eq!([0.5, 1.7, -3.0].map(|x| knee(x, 1.0, 0.0, 0.0)), [0.5, 1.0, -1.0]);
        // through the device: t = 4p, r = 4p, o = 4p − 2
        let x = [0.25f32, 0.9, -0.9, 0.0];
        let p = [0.5 / 4.0, 0.25 / 4.0, 0.5];
        let (l, _) = with_fx(kind::DISTORTION, &vals(kind::DISTORTION, 6, &p), 44100.0, 120, |fx| run(fx, 44100.0, 120, &x, &x, 4));
        assert_eq!(l, [0.25, 0.6, -0.6, 0.0]);
    }

    #[cfg(all(feature = "us-fx-distortion-atan", feature = "us-fx-distortion-sine"))]
    #[test]
    fn atan_and_sine_follow_their_laws() {
        let x = [-0.9f32, -0.2, 0.0, 0.3, 0.8];
        let factor = |a: f32| (libm::log10f(a) + 3.0) / 6.0;
        // Elevated's sine machine (10, 0.625), and an atan (a = 0.5, b = 0.75)
        let (s, _) = with_fx(kind::DISTORTION, &vals(kind::DISTORTION, 5, &[factor(10.0), 0.625 / 16.0]), 44100.0, 120, |fx| run(fx, 44100.0, 120, &x, &x, 5));
        let (a, _) = with_fx(kind::DISTORTION, &vals(kind::DISTORTION, 4, &[factor(0.5), 0.75 / 16.0]), 44100.0, 120, |fx| run(fx, 44100.0, 120, &x, &x, 5));
        for k in 0..5 {
            assert!((s[k] - 0.625 * libm::sinf(10.0 * x[k])).abs() < 1e-4, "sine {}: {}", x[k], s[k]);
            assert!((a[k] - 0.75 * libm::atanf(x[k] / 0.5)).abs() < 1e-4, "atan {}: {}", x[k], a[k]);
        }
    }

    // ---- 3.3 delay ----

    #[cfg(feature = "us-fx-delay-pingpong")]
    #[test]
    fn a_ping_pong_echo_alternates_channels() {
        let mut l = vec![0.0f32; 400];
        l[0] = 1.0;
        let r = vec![0.0f32; 400];
        // length 100, feedback 0.5 (= 2p − 1)
        let (ol, or) = with_fx(kind::DELAY, &vals(kind::DELAY, 3, &[100.0 / 65536.0, 0.75]), 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &r, 37));
        let hits: Vec<(usize, char, f32)> = (0..400)
            .flat_map(|k| [(k, 'L', ol[k]), (k, 'R', or[k])])
            .filter(|h| h.2 != 0.0)
            .collect();
        assert_eq!(hits[..3], [(0, 'L', 1.0), (100, 'R', 0.5), (200, 'L', 0.25)]);
    }

    #[cfg(feature = "us-fx-delay-allpass")]
    #[test]
    fn the_crossed_allpass_keeps_the_power() {
        let (l, r) = noise(9, 441_000);
        let (ol, or) = with_fx(kind::DELAY, &vals(kind::DELAY, 4, &[2116.0 / 65536.0, (0.7 + 1.0) / 2.0]), 44100.0, 120, |fx| run(fx, 44100.0, 120, &l, &r, 512));
        let db = 10.0 * libm::log10((power(&ol) + power(&or)) / (power(&l) + power(&r)));
        assert!(db.abs() < 0.1, "{db} dB");
    }

    #[cfg(all(feature = "us-fx-delay-pingpong", feature = "us-fx-delay-allpass"))]
    #[test]
    fn the_lines_need_one_stereo_maximum() {
        use super::super::{Params, Effect};
        let env = synth_core::mix::Env { rate: 44100.0, tempo: 120, bank: None };
        let need = <super::PingPong as Effect>::need(&env, &Params(&[]));
        assert_eq!(need, <super::Allpass as Effect>::need(&env, &Params(&[])));
        assert!(need >= 2 * super::MAX_LEN * 4 && need < 2 * super::MAX_LEN * 4 + 64, "{need}");
    }
}
