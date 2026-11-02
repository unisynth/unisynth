//! LFOs (design D1, change unify-v2-algorithms 3.1): one LFO for every law. A phase in turns advanced by elapsed time
//! at the rate of its law (Hz, or per-frame steps), the shape union, polarity, one-shot and a start phase at key sync;
//! two views of the output for Scale and Offset slots and one for Add slots.

use crate::law;
use crate::params as pm;
use crate::tables::sin_turns;

/// Shapes (`Choice(ALL)` of `l::SHAPE` on every law). The first five are the plain LFO's; the others need the settings
/// (`us-lfo-frame`).
pub mod shape {
    pub const SINE: usize = 0;
    pub const RAMP_UP: usize = 1;
    pub const RAMP_DOWN: usize = 2;
    /// High for the first half-turn.
    pub const SQUARE: usize = 3;
    /// Stepped random: 256 steps per cycle from a fixed hash (the same for every voice).
    pub const RANDOM: usize = 4;
    /// Shapes of the old layout's Hz law (the migration decodes old values with it).
    pub const COUNT_V1: usize = 5;
    /// Rises from 0 for the first half-turn.
    pub const TRIANGLE: usize = 5;
    /// A new value at every cycle start, from the LFO's own seed (drawn at key sync).
    pub const SAMPLE_HOLD: usize = 6;
    /// Low for the first half-turn (a square starting low).
    pub const PULSE: usize = 7;
    pub const ALL: usize = 8;
}

/// The old frame law's waveforms in order (V2's saw, triangle, pulse, sine, sample-and-hold); other values were the saw.
/// The migration maps old values with it.
pub const FRAME_SHAPES: [usize; 5] = [shape::RAMP_UP, shape::TRIANGLE, shape::PULSE, shape::SINE, shape::SAMPLE_HOLD];

/// What a note-on does to an LFO.
pub mod sync {
    /// Restart at the start phase.
    pub const RESTART: u8 = 0;
    /// The phase of an LFO that has run since frame 0, so every voice shares it.
    pub const SONG_TIME: u8 = 1;
    /// Keep running from where this voice's LFO was.
    pub const CONTINUE: u8 = 2;
}

/// Steps of the stepped-random shape per cycle.
const RANDOM_STEPS: f64 = 256.0;

/// The settings beyond the Hz law's shapes (one-shot, polarity, start phase, the later shapes, the frame law) are
/// built with `us-lfo-frame`; without it, an LFO is the plain Hz-law LFO.
const SETTINGS: bool = cfg!(feature = "us-lfo-frame");

/// Uniform `[0, 1)` from a step number (a fixed hash, so the sequence is the same for every voice and target).
#[inline]
fn hash01(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B1) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x >> 8) as f32 * (1.0 / 16_777_216.0)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Lfo {
    /// Phase in turns, `[0, 1)`.
    pub phase: f64,
    /// One-shot cycle finished, holding at its end; the sample-and-hold seed and the value it holds (`v2::Opt`: absent
    /// without the settings).
    hold: crate::v2::Opt<(bool, u32)>,
}

/// An LFO's settings for a control segment, from its fields ([`LfoP::read`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LfoP {
    pub shape: usize,
    /// Turns per second.
    pub rate: f64,
    pub one_shot: bool,
    /// What an Add slot sees: `shape·gain + dc` (polarity positive `(d, 0)`, negative `(−d, 0)`, bipolar
    /// `(d, −d/2)`, d = depth).
    pub gain: f32,
    pub dc: f32,
    /// Start phase in turns at a restart.
    pub phase0: f64,
    /// [`sync`].
    pub sync: u8,
    /// A restart draws a new sample-and-hold seed (the frame law always does, as V2 does).
    pub reseed: bool,
}

impl LfoP {
    /// LFO `k`'s settings ([`Self::shape_of`], [`Self::rate_of`], …; the voice reads only the fields a step needs).
    pub fn read(p: &[f32; pm::COUNT], k: usize) -> LfoP {
        let (gain, dc) = Self::view_of(p, k);
        let (sync, phase0, reseed) = Self::sync_of(p, k);
        LfoP { shape: Self::shape_of(p, k), rate: Self::rate_of(p, k), one_shot: Self::one_shot_of(p, k), gain, dc, phase0, sync, reseed }
    }

    /// Whether LFO `k` of `p` uses a setting beyond the plain Hz-law LFO (`us-lfo-frame`): the frame law, a later
    /// shape, polarity, one-shot, a start phase, or the sync "continue".
    pub fn uses_settings(p: &[f32; pm::COUNT], k: usize) -> bool {
        let c = |f: usize, n: usize| law::choice(p[pm::lfo(k, f)], n);
        c(pm::l::LAW, 2) == 1
            || c(pm::l::SHAPE, shape::ALL) >= shape::COUNT_V1
            || c(pm::l::POLARITY, 3) != 0
            || c(pm::l::ONE_SHOT, 2) == 1
            || p[pm::lfo(k, pm::l::PHASE)] != 0.0
            || c(pm::l::SYNC, pm::lfo_sync::COUNT) == pm::lfo_sync::CONTINUE
    }

    /// The frame law (`l::LAW`): rate in per-frame steps, a restart always draws a new sample-and-hold seed.
    #[inline(always)]
    fn frame(p: &[f32; pm::COUNT], k: usize) -> bool {
        SETTINGS && law::choice(p[pm::lfo(k, pm::l::LAW)], 2) == 1
    }

    #[inline(always)]
    pub fn shape_of(p: &[f32; pm::COUNT], k: usize) -> usize {
        law::choice(p[pm::lfo(k, pm::l::SHAPE)], shape::ALL)
    }

    /// Turns per second: `law::lfo_rate`, or `¼·calcfreq(p)` turns per frame on the frame law.
    #[inline(always)]
    pub fn rate_of(p: &[f32; pm::COUNT], k: usize) -> f64 {
        let v = p[pm::lfo(k, pm::l::RATE)];
        if Self::frame(p, k) {
            0.25 * crate::v2::calcfreq(v) as f64 * crate::v2::FRAMES_PER_S as f64
        } else {
            law::lfo_rate(v) as f64
        }
    }

    #[inline(always)]
    pub fn one_shot_of(p: &[f32; pm::COUNT], k: usize) -> bool {
        SETTINGS && law::choice(p[pm::lfo(k, pm::l::ONE_SHOT)], 2) == 1
    }

    /// `(gain, dc)` of the Add view for the polarity.
    #[inline(always)]
    pub fn view_of(p: &[f32; pm::COUNT], k: usize) -> (f32, f32) {
        let depth = p[pm::lfo(k, pm::l::DEPTH)];
        if !SETTINGS {
            return (depth, 0.0);
        }
        match law::choice(p[pm::lfo(k, pm::l::POLARITY)], 3) {
            1 => (-depth, 0.0),
            2 => (depth, -0.5 * depth),
            _ => (depth, 0.0),
        }
    }

    /// `(sync, phase0, reseed)` from `l::SYNC` (`params::lfo_sync`) and the start phase.
    #[inline(always)]
    pub fn sync_of(p: &[f32; pm::COUNT], k: usize) -> (u8, f64, bool) {
        let sync = match law::choice(p[pm::lfo(k, pm::l::SYNC)], pm::lfo_sync::COUNT) {
            pm::lfo_sync::SONG_TIME => sync::SONG_TIME,
            pm::lfo_sync::CONTINUE if SETTINGS => sync::CONTINUE,
            _ => sync::RESTART,
        };
        if !SETTINGS {
            return (sync, 0.0, false);
        }
        let phase0 = crate::v2::ftou32(p[pm::lfo(k, pm::l::PHASE)]) as f64 / 4_294_967_296.0;
        (sync, phase0, Self::frame(p, k) || Self::shape_of(p, k) == shape::SAMPLE_HOLD)
    }
}

impl Lfo {
    pub fn new(phase: f64) -> Self {
        Lfo { phase: phase - libm::floor(phase), hold: crate::v2::Opt::new((false, 0)) }
    }

    /// A restart at `phase0`; `seed` (drawn by the voice when `reseed`) starts the sample-and-hold sequence, which
    /// draws its first value now.
    pub fn restart(phase0: f64, reseed: bool, seed: u32) -> Self {
        let mut l = Lfo::new(phase0);
        if SETTINGS && reseed {
            l.hold.set((false, crate::v2::urandom(&mut { seed })));
        }
        l
    }

    /// Advances by `dt` seconds. A one-shot LFO stops at the end of its first cycle; a sample-and-hold draws a new value
    /// at every wrap.
    pub fn advance(&mut self, dt: f32, rate: f64, one_shot: bool) {
        let ph = self.phase + rate * dt as f64;
        if !SETTINGS {
            self.phase = ph - libm::floor(ph);
            return;
        }
        let (held, mut seed) = self.hold.get();
        if held {
            return;
        }
        if ph >= 1.0 {
            if one_shot {
                self.phase = 1.0 - 1.0 / 4_294_967_296.0;
                self.hold.set((true, seed));
                return;
            }
            let wraps = libm::floor(ph);
            for _ in 0..(wraps as u32).min(4) {
                crate::v2::urandom(&mut seed);
            }
            self.hold.set((false, seed));
            self.phase = ph - wraps;
        } else {
            self.phase = ph;
        }
    }

    /// The unipolar shape value `0..1` at the current phase.
    pub fn value(&self, kind: usize) -> f32 {
        let ph = self.phase;
        match kind {
            shape::SINE => (sin_turns(ph) + 1.0) * 0.5,
            shape::RAMP_UP => ph as f32,
            shape::RAMP_DOWN => 1.0 - ph as f32,
            shape::SQUARE => {
                if ph < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            shape::TRIANGLE if SETTINGS => {
                let t = 2.0 * ph as f32;
                if t < 1.0 { t } else { 2.0 - t }
            }
            shape::PULSE if SETTINGS => {
                if ph < 0.5 {
                    0.0
                } else {
                    1.0
                }
            }
            shape::SAMPLE_HOLD if SETTINGS => crate::v2::utof23(self.hold.get().1),
            _ => hash01((ph * RANDOM_STEPS) as u32),
        }
    }
}

/// What a Scale slot sees: `1 − d + d·shape`, `d = (depth·M)²`.
#[inline]
pub fn scale_view(depth: f32, shape: f32) -> f32 {
    let d = depth * depth;
    1.0 - d + d * shape
}

/// What an Offset slot sees: bipolar `d·(2·shape − 1)`.
#[inline]
pub fn offset_view(depth: f32, shape: f32) -> f32 {
    let d = depth * depth;
    d * (2.0 * shape - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The LFOs as they were before the merge (change unify-v2-algorithms 3.1), kept as the reference: the Hz law's
    /// `advance`/`shape` and the frame law's `V2P`, `sync_v2`, `advance_v2`, `shape_v2`.
    #[cfg(feature = "us-lfo-frame")]
    mod old {
        pub struct V2Lfo {
            pub phase: f64,
            held: bool,
            seed: u32,
        }
        pub struct V2P {
            pub shape: u8,
            per_frame: f64,
            eg: bool,
        }
        impl V2P {
            pub fn new(shape: f32, rate: f32, eg: f32) -> Self {
                let raw = |x: f32| x * 128.0;
                V2P { shape: (raw(shape) as i32).clamp(0, 7) as u8, per_frame: 0.25 * crate::v2::calcfreq(rate) as f64, eg: raw(eg) as i32 != 0 }
            }
        }
        impl V2Lfo {
            pub fn sync(phase: f32, seed: u32) -> Self {
                V2Lfo { phase: crate::v2::ftou32(phase) as f64 / 4_294_967_296.0, held: false, seed: crate::v2::urandom(&mut { seed }) }
            }
            pub fn advance(&mut self, dt: f32, p: &V2P) {
                if self.held {
                    return;
                }
                let ph = self.phase + p.per_frame * (dt * crate::v2::FRAMES_PER_S) as f64;
                if ph >= 1.0 {
                    if p.eg {
                        self.phase = 1.0 - 1.0 / 4_294_967_296.0;
                        self.held = true;
                        return;
                    }
                    let wraps = libm::floor(ph);
                    for _ in 0..(wraps as u32).min(4) {
                        crate::v2::urandom(&mut self.seed);
                    }
                    self.phase = ph - wraps;
                } else {
                    self.phase = ph;
                }
            }
            pub fn shape(&self, shape: u8, native_sin: bool) -> f32 {
                let cntr = (self.phase * 4_294_967_296.0) as u64 as u32;
                match shape {
                    1 => crate::v2::utof23((cntr << 1) ^ ((cntr as i32 >> 31) as u32)),
                    2 => crate::v2::utof23((cntr as i32 >> 31) as u32),
                    3 => {
                        let x = crate::v2::utof23(cntr) * core::f32::consts::TAU;
                        (if native_sin { libm::sinf(x) } else { crate::v2::fastsinrc(x) }) * 0.5 + 0.5
                    }
                    4 => crate::v2::utof23(self.seed),
                    _ => crate::v2::utof23(cntr),
                }
            }
        }
    }

    #[cfg(feature = "us-lfo-frame")]
    const BLOCK: f32 = 32.0 / 44_100.0;

    #[cfg(feature = "us-lfo-frame")]
    fn frame_law(shape: usize, rate: f32, one_shot: bool, phase: f32) -> [f32; pm::COUNT] {
        let mut v = pm::DEFAULTS;
        // the old frame-law LFO with V2 waveform `shape`, as the migration writes it
        v[pm::lfo(0, pm::l::LAW)] = 1.0;
        v[pm::lfo(0, pm::l::SHAPE)] = pm::choice_value(FRAME_SHAPES[shape], shape::ALL);
        v[pm::lfo(0, pm::l::RATE)] = rate;
        v[pm::lfo(0, pm::l::DEPTH)] = 1.0;
        v[pm::lfo(0, pm::l::ONE_SHOT)] = if one_shot { 1.0 } else { 0.0 };
        v[pm::lfo(0, pm::l::PHASE)] = phase;
        v
    }

    #[test]
    #[cfg(feature = "us-lfo-frame")]
    fn the_earlier_lfo_modes_are_reproduced() {
        crate::tables::init();
        // the frame law: every waveform, three rates, free and one-shot, a start phase; 4 s in 32-frame blocks
        for raw_shape in 0..5usize {
            for rate in [0.3f32, 0.55, 0.8] {
                for one_shot in [false, true] {
                    let v = frame_law(raw_shape, rate, one_shot, 0.3);
                    let q = LfoP::read(&v, 0);
                    assert_eq!((q.sync, q.reseed), (sync::RESTART, true));
                    let o = old::V2P::new(raw_shape as f32 / 128.0, rate, if one_shot { 1.0 / 128.0 } else { 0.0 });
                    let (mut new, mut was) = (Lfo::restart(q.phase0, q.reseed, 0x1234_5678), old::V2Lfo::sync(0.3, 0x1234_5678));
                    let (mut prev_old, mut steps) = (was.shape(o.shape, false), 0);
                    for b in 0..(4.0 / BLOCK) as usize {
                        let (a, w) = (new.value(q.shape), was.shape(o.shape, false));
                        if (a - w).abs() > 2e-3 {
                            // a step of the old shape may land one block apart (phase rounding at the boundary)
                            assert!((w - prev_old).abs() > 0.1 || steps < 2, "shape {raw_shape} rate {rate} one-shot {one_shot} block {b}: {a} vs {w}");
                            steps += 1;
                        }
                        prev_old = w;
                        new.advance(BLOCK, q.rate, q.one_shot);
                        was.advance(BLOCK, &o);
                    }
                    assert!(steps <= 2, "shape {raw_shape} rate {rate}: {steps} blocks off");
                }
            }
        }
        // the Hz law is unchanged, sample for sample
        for k in 0..shape::COUNT_V1 {
            let q = LfoP { shape: k, rate: 3.7f32 as f64, ..Default::default() }; // `law::lfo_rate` is f32
            let mut l = Lfo::new(0.1);
            let mut ph = 0.1f64;
            for _ in 0..2000 {
                let want = Lfo::new(ph).value(k);
                assert_eq!(l.value(k), want);
                l.advance(BLOCK, q.rate, q.one_shot);
                ph += 3.7f32 as f64 * BLOCK as f64;
                ph -= libm::floor(ph);
                assert_eq!(l.phase, ph);
            }
        }
    }

    #[test]
    #[cfg(feature = "us-lfo-frame")]
    fn sample_and_hold_on_a_hz_law_lfo() {
        // 4 Hz: a new value at each cycle start, held for 0.25 s
        let q = LfoP { shape: shape::SAMPLE_HOLD, rate: 4.0, gain: 1.0, sync: sync::RESTART, reseed: true, ..Default::default() };
        let mut l = Lfo::restart(q.phase0, q.reseed, 99);
        let (mut last, mut t, mut changes) = (l.value(q.shape), 0.0f64, std::vec::Vec::new());
        while t + (BLOCK as f64) < 2.0 {
            l.advance(BLOCK, q.rate, q.one_shot);
            t += BLOCK as f64;
            let v = l.value(q.shape);
            if v != last {
                changes.push(t);
            }
            last = v;
        }
        assert_eq!(changes.len(), 7, "{changes:?}");
        for (i, c) in changes.iter().enumerate() {
            let at = 0.25 * (i + 1) as f64;
            assert!(*c >= at && *c < at + BLOCK as f64 + 1e-9, "change {i} at {c}, cycle start {at}");
        }
    }

    #[test]
    #[cfg(feature = "us-lfo-frame")]
    fn one_shot_and_polarity_on_a_hz_law_lfo() {
        // ramp up at 2 Hz, one-shot: one ramp in 0.5 s, then its end value
        let q = LfoP { shape: shape::RAMP_UP, rate: 2.0, one_shot: true, ..Default::default() };
        let mut l = Lfo::restart(q.phase0, q.reseed, 0);
        for _ in 0..(0.45 / BLOCK as f64) as usize {
            l.advance(BLOCK, q.rate, q.one_shot);
        }
        assert!((l.value(q.shape) - 0.9).abs() < 0.01, "{}", l.value(q.shape));
        for _ in 0..(1.5 / BLOCK as f64) as usize {
            l.advance(BLOCK, q.rate, q.one_shot);
        }
        assert!(l.value(q.shape) > 0.999, "holds the end");
    }

    #[test]
    fn shapes_at_their_phases() {
        crate::tables::init();
        let at = |ph: f64, k| Lfo::new(ph).value(k);
        assert!((at(0.0, shape::SINE) - 0.5).abs() < 1e-6);
        assert!((at(0.25, shape::SINE) - 1.0).abs() < 1e-6);
        assert!(at(0.75, shape::SINE).abs() < 1e-6);
        assert_eq!((at(0.0, shape::RAMP_UP), at(0.5, shape::RAMP_UP)), (0.0, 0.5));
        assert_eq!((at(0.0, shape::RAMP_DOWN), at(0.25, shape::RAMP_DOWN)), (1.0, 0.75));
        assert_eq!((at(0.0, shape::SQUARE), at(0.49, shape::SQUARE), at(0.5, shape::SQUARE)), (1.0, 1.0, 0.0));
        // stepped random: constant within a step, uniform-looking across steps
        assert_eq!(at(0.0, shape::RANDOM), at(0.001, shape::RANDOM));
        let v: Vec<f32> = (0..256).map(|i| at(i as f64 / 256.0 + 1e-4, shape::RANDOM)).collect();
        assert!(v.iter().all(|x| (0.0..1.0).contains(x)));
        let mean = v.iter().sum::<f32>() / 256.0;
        assert!((mean - 0.5).abs() < 0.08, "{mean}");
        assert!(v.windows(2).any(|w| w[0] != w[1]));
    }

    #[test]
    fn advance_runs_in_hz_and_wraps() {
        let mut l = Lfo::new(0.0);
        for _ in 0..1000 {
            l.advance(0.001, 2.5, false); // 1 s at 2.5 Hz = 2.5 turns
        }
        assert!((l.phase - 0.5).abs() < 1e-6, "{}", l.phase); // dt is f32
        l.advance(10.0, 0.0, false);
        assert!((l.phase - 0.5).abs() < 1e-6);
        assert!(Lfo::new(-0.25).phase > 0.74 && Lfo::new(3.5).phase == 0.5);
    }

    #[test]
    fn the_two_views() {
        // depth 1: full range; depth 0.5: d = 0.25
        assert_eq!((scale_view(1.0, 0.0), scale_view(1.0, 1.0)), (0.0, 1.0));
        assert_eq!((offset_view(1.0, 0.0), offset_view(1.0, 1.0), offset_view(1.0, 0.5)), (-1.0, 1.0, 0.0));
        assert!((scale_view(0.5, 0.0) - 0.75).abs() < 1e-7);
        assert!((offset_view(0.5, 1.0) - 0.25).abs() < 1e-7);
        assert_eq!(scale_view(0.0, 0.3), 1.0);
    }
}
