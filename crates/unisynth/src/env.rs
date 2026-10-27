//! ADSR envelopes (design D5): two curve modes, both advanced by elapsed time at control rate.
//!
//! - timed mode (WaveSabre): times from `law::env_ms`, linear attack, quadratic decay and release (`(1 − t/T)²`), the release
//!   starting from the value at note-off. Closed form per stage.
//! - rate mode (Tunefish): the original per-512-block steps expressed per second (`STEP · 44100/512`): attack `−log₁₀(0.94·a³)`
//!   linear, decay `¼·log₁₀(0.94·(d·M)³)·((0.01 + v − S)/(1 − S))^slope`, release `¼·log₁₀(0.94·r³)·v^slope`, integrated
//!   with one Euler step per control segment. Quirks: the decay can rise when `d·M` is above ≈ 1.02 (here it then holds
//!   at the peak instead of growing past 1), and the sustain level is fixed at note-on (no chase).
//!
//! Parameters are snapshotted at note-on ([`EnvP`]), like the original envelopes.

use crate::law;
use crate::params::{self as pm, e};

/// Rate-mode rates are per 512-frame block at 44 100 Hz; this many blocks per second.
#[cfg(feature = "us-env-rate")]
const TF_BLOCKS_PER_S: f32 = 44_100.0 / 512.0;
/// Rate-mode per-block step scale `0.0005·512`.
#[cfg(feature = "us-env-rate")]
const TF_STEP: f32 = 0.0005 * 512.0;
/// Release ends below this value (rate mode).
#[cfg(feature = "us-env-rate")]
const TF_DONE: f32 = 1e-5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
    Finished,
}

/// An envelope's parameters as read from the patch at note-on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvP {
    pub by_rate: bool,
    pub a: f32,
    pub d: f32,
    pub s: f32,
    pub r: f32,
    pub slope: f32,
}

/// The mode this build runs for a requested one: a build with one mode runs it for both (a patch that asks for the
/// other fails at load; only automating the mode can get here).
const fn of_mode(by_rate: bool) -> bool {
    if cfg!(all(feature = "us-env-timed", feature = "us-env-rate")) {
        by_rate
    } else {
        cfg!(feature = "us-env-rate")
    }
}

/// Fails with `UnsupportedModule` when the build lacks the mode `by_rate` asks for.
pub fn check(by_rate: bool) -> Result<(), synth_core::mix::LoadError> {
    let (have, id) = if by_rate {
        (cfg!(feature = "us-env-rate"), crate::module::ENV_RATE)
    } else {
        (cfg!(feature = "us-env-timed"), crate::module::ENV_TIMED)
    };
    if have {
        Ok(())
    } else {
        Err(synth_core::mix::LoadError::UnsupportedModule(id))
    }
}

impl EnvP {
    pub const DEFAULT: EnvP = EnvP { by_rate: false, a: 0.0, d: 0.3, s: 1.0, r: 0.1, slope: 0.5 };

    pub fn read(p: &[f32; pm::COUNT], i: usize) -> Self {
        EnvP {
            by_rate: of_mode(law::choice(p[pm::env(i, e::MODE)], 2) == 1),
            a: p[pm::env(i, e::ATTACK)],
            d: p[pm::env(i, e::DECAY)],
            s: p[pm::env(i, e::SUSTAIN)],
            r: p[pm::env(i, e::RELEASE)],
            slope: p[pm::env(i, e::SLOPE)],
        }
    }
}

/// Seconds of a timed-mode time parameter.
#[inline]
#[cfg(feature = "us-env-timed")]
fn ws_s(p: f32) -> f32 {
    law::env_ms(p) * 0.001
}

#[derive(Clone, Copy, Debug)]
pub struct Env {
    stage: Stage,
    /// Seconds into the stage (timed mode).
    pos: f32,
    /// Current value; at note-off, the value the release starts from.
    v: f32,
    from: f32,
}

impl Env {
    pub const fn new() -> Self {
        Env { stage: Stage::Finished, pos: 0.0, v: 0.0, from: 0.0 }
    }

    pub fn trigger(&mut self) {
        *self = Env { stage: Stage::Attack, pos: 0.0, v: 0.0, from: 0.0 };
    }

    /// Starts the release from the current value (no effect before the note or once released).
    pub fn release(&mut self) {
        if !matches!(self.stage, Stage::Release | Stage::Finished) {
            self.stage = Stage::Release;
            self.pos = 0.0;
            self.from = self.v;
        }
    }

    #[inline]
    pub fn value(&self) -> f32 {
        self.v
    }

    #[inline]
    pub fn stage(&self) -> Stage {
        self.stage
    }

    #[inline]
    pub fn finished(&self) -> bool {
        self.stage == Stage::Finished
    }

    /// Advances by `dt` seconds. `decay_mod` scales the rate-mode decay (mod matrix `M(decay)`; 1 = none).
    pub fn advance(&mut self, dt: f32, ep: &EnvP, decay_mod: f32) {
        #[cfg(feature = "us-env-rate")]
        if ep.by_rate {
            return self.advance_tf(dt, ep, decay_mod);
        }
        let _ = decay_mod;
        #[cfg(feature = "us-env-timed")]
        self.advance_ws(dt, ep);
        #[cfg(not(feature = "us-env-timed"))]
        let _ = (dt, ep);
    }

    #[cfg(feature = "us-env-timed")]
    fn advance_ws(&mut self, mut t: f32, ep: &EnvP) {
        loop {
            match self.stage {
                Stage::Attack => {
                    let a = ws_s(ep.a);
                    if t >= a - self.pos {
                        t -= a - self.pos;
                        self.stage = Stage::Decay;
                        self.pos = 0.0;
                        self.v = 1.0;
                    } else {
                        self.pos += t;
                        self.v = self.pos / a;
                        return;
                    }
                }
                Stage::Decay => {
                    let d = ws_s(ep.d);
                    if t >= d - self.pos {
                        self.stage = Stage::Sustain;
                        self.v = ep.s;
                    } else {
                        self.pos += t;
                        let f = 1.0 - self.pos / d;
                        let f = f * f;
                        self.v = f + ep.s * (1.0 - f);
                        return;
                    }
                }
                Stage::Sustain => {
                    self.v = ep.s;
                    return;
                }
                Stage::Release => {
                    let r = ws_s(ep.r);
                    if t >= r - self.pos {
                        *self = Env::new();
                    } else {
                        self.pos += t;
                        let f = 1.0 - self.pos / r;
                        self.v = self.from * f * f;
                    }
                    return;
                }
                Stage::Finished => {
                    self.v = 0.0;
                    return;
                }
            }
        }
    }

    #[cfg(feature = "us-env-rate")]
    fn advance_tf(&mut self, dt: f32, ep: &EnvP, decay_mod: f32) {
        let cube = |x: f32| x * x * x;
        let blocks = dt * TF_BLOCKS_PER_S;
        let sustain = ep.s.min(0.99);
        match self.stage {
            Stage::Attack => {
                let rate = -crate::fm::log10f((cube(ep.a).max(1e-9)) * 0.94) * TF_STEP;
                self.v += rate * blocks;
                if self.v >= 1.0 {
                    self.v = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                if self.v < sustain {
                    self.v = sustain;
                    self.stage = Stage::Sustain;
                } else {
                    let rate = crate::fm::log10f(cube(ep.d * decay_mod).max(1e-9) * 0.94) * 0.25 * TF_STEP;
                    let pos = (0.01 + (self.v - sustain)) / (1.0 - sustain);
                    self.v += rate * crate::fm::powf(pos, ep.slope) * blocks;
                    if self.v <= sustain {
                        self.v = sustain;
                        self.stage = Stage::Sustain;
                    }
                    self.v = self.v.min(1.0); // the original decay can rise past 1; here it holds at the peak
                }
            }
            Stage::Sustain => self.v = sustain,
            Stage::Release => {
                let rate = crate::fm::log10f(cube(ep.r).max(1e-9) * 0.94) * 0.25 * TF_STEP;
                self.v += rate * crate::fm::powf(self.v.max(0.0), ep.slope) * blocks;
                if self.v <= TF_DONE {
                    *self = Env::new();
                }
            }
            Stage::Finished => self.v = 0.0,
        }
    }
}

impl Default for Env {
    fn default() -> Self {
        Env::new()
    }
}

#[cfg(all(test, feature = "us-env-timed", feature = "us-env-rate"))]
mod tests {
    use super::*;

    const WS: EnvP = EnvP { by_rate: false, a: 0.0, d: 0.0, s: 0.5, r: 0.0, slope: 0.5 };

    /// Advances `t` seconds in steps of `step`, returns the value.
    fn run(e: &mut Env, ep: &EnvP, t: f32, step: f32) -> f32 {
        let n = (t / step).round() as usize;
        for _ in 0..n {
            e.advance(step, ep, 1.0);
        }
        e.value()
    }

    fn ep_ms(a_ms: f32, d_ms: f32, s: f32, r_ms: f32) -> EnvP {
        let inv = |ms: f32| crate::law::host::inv_env_ms(ms);
        EnvP { by_rate: false, a: inv(a_ms), d: inv(d_ms), s, r: inv(r_ms), slope: 0.5 }
    }

    #[test]
    fn ws_curves_match_the_original_formulas() {
        let ep = ep_ms(100.0, 200.0, 0.4, 300.0);
        let mut e = Env::new();
        assert_eq!(e.value(), 0.0);
        e.trigger();
        assert_eq!(e.value(), 0.0);
        assert!((run(&mut e, &ep, 0.05, 0.001) - 0.5).abs() < 1e-4, "attack is linear");
        assert!((run(&mut e, &ep, 0.05, 0.001) - 1.0).abs() < 1e-3);
        assert_eq!(e.stage(), Stage::Decay);
        // 100 ms into a 200 ms decay: f = (1 − 0.5)² = 0.25 → 1·0.25 + 0.4·0.75
        assert!((run(&mut e, &ep, 0.1, 0.001) - (0.25 + 0.4 * 0.75)).abs() < 1e-3);
        run(&mut e, &ep, 0.2, 0.001);
        assert_eq!((e.stage(), e.value()), (Stage::Sustain, 0.4));
        // release 300 ms from 0.4: halfway f = 0.25
        e.release();
        assert!((run(&mut e, &ep, 0.15, 0.001) - 0.4 * 0.25).abs() < 1e-3);
        run(&mut e, &ep, 0.16, 0.001);
        assert!(e.finished() && e.value() == 0.0);
    }

    #[test]
    fn ws_release_starts_from_the_current_value_in_any_stage() {
        let ep = ep_ms(100.0, 200.0, 0.4, 100.0);
        let mut e = Env::new();
        e.trigger();
        run(&mut e, &ep, 0.05, 0.001); // mid attack, 0.5
        e.release();
        assert_eq!(e.stage(), Stage::Release);
        assert!((run(&mut e, &ep, 0.05, 0.001) - 0.5 * 0.25).abs() < 1e-3);
        e.release(); // a second release changes nothing
        assert!((e.value() - 0.125).abs() < 1e-3);
    }

    #[test]
    fn step_size_does_not_change_the_ws_result() {
        let ep = ep_ms(30.0, 80.0, 0.3, 120.0);
        let at = |step: f32| {
            let mut e = Env::new();
            e.trigger();
            let a = run(&mut e, &ep, 0.055, step);
            e.release();
            (a, run(&mut e, &ep, 0.06, step))
        };
        let (a, b) = at(0.0001);
        for step in [0.0005, 0.001, 0.005] {
            let (x, y) = at(step);
            assert!((x - a).abs() < 2e-3 && (y - b).abs() < 2e-3, "step {step}: {x} {y} vs {a} {b}");
        }
    }

    fn tf(a: f32, d: f32, s: f32, r: f32, slope: f32) -> EnvP {
        EnvP { by_rate: true, a, d, s, r, slope }
    }

    /// The original per-block recurrence, step 1 block, as an independent model.
    fn tf_model(ep: &EnvP, blocks: usize, release_at: usize) -> Vec<f32> {
        let step = 0.0005 * 512.0;
        let cube = |x: f32| x * x * x;
        let (mut v, mut stage) = (0.0f32, 0u8); // 0 attack, 1 decay, 2 sustain, 3 release, 4 done
        let s = ep.s.min(0.99);
        let mut out = vec![];
        for b in 0..blocks {
            if b == release_at && stage < 3 {
                stage = 3;
            }
            match stage {
                0 => {
                    v += -(cube(ep.a).max(1e-9) * 0.94).log10() * step;
                    if v >= 1.0 {
                        v = 1.0;
                        stage = 1;
                    }
                }
                1 => {
                    if v < s {
                        v = s;
                        stage = 2;
                    } else {
                        let pos = (0.01 + (v - s)) / (1.0 - s);
                        v += (cube(ep.d).max(1e-9) * 0.94).log10() * 0.25 * step * pos.powf(ep.slope);
                        if v <= s {
                            v = s;
                            stage = 2;
                        }
                    }
                }
                2 => v = s,
                3 => {
                    v += (cube(ep.r).max(1e-9) * 0.94).log10() * 0.25 * step * v.powf(ep.slope);
                    if v <= 1e-5 {
                        v = 0.0;
                        stage = 4;
                    }
                }
                _ => {}
            }
            out.push(v);
        }
        out
    }

    #[test]
    fn tf_curves_follow_the_original_recurrence_in_time() {
        let ep = tf(0.6, 0.35, 0.45, 0.4, 0.5);
        let model = tf_model(&ep, 400, 200);
        let mut e = Env::new();
        e.trigger();
        let block = 512.0 / 44_100.0;
        let mut worst = 0.0f32;
        for b in 0..400 {
            if b == 200 {
                e.release();
            }
            // the model's value after block b+1 is the state at time (b+1)·block
            let step = 32.0 / 44_100.0;
            for _ in 0..16 {
                e.advance(step, &ep, 1.0);
            }
            worst = worst.max((e.value() - model[b]).abs());
            let _ = block;
        }
        assert!(worst < 0.05, "{worst}: Euler at 32 frames vs the original 512-frame recurrence");
        assert!(model[199] > 0.4 && model[199] < 0.6, "model sits near sustain before the release");
    }

    #[test]
    fn tf_attack_decay_release_reach_their_targets() {
        let ep = tf(0.5, 0.5, 0.3, 0.5, 0.5);
        let mut e = Env::new();
        e.trigger();
        run(&mut e, &ep, 0.3, 32.0 / 44_100.0);
        assert!(matches!(e.stage(), Stage::Decay | Stage::Sustain));
        run(&mut e, &ep, 3.0, 32.0 / 44_100.0);
        assert_eq!((e.stage(), e.value()), (Stage::Sustain, 0.3));
        e.release();
        run(&mut e, &ep, 5.0, 32.0 / 44_100.0);
        assert!(e.finished());
    }

    #[test]
    fn tf_decay_that_would_rise_holds_at_the_peak() {
        let ep = tf(0.0, 1.0, 0.5, 0.5, 0.5);
        let mut e = Env::new();
        e.trigger();
        for _ in 0..2000 {
            e.advance(32.0 / 44_100.0, &ep, 1.1); // d·M = 1.1 → log10(0.94·1.33) > 0
        }
        assert!(e.value() <= 1.0 && e.value() > 0.99, "{}", e.value());
        assert_eq!(e.stage(), Stage::Decay);
        // and with M = 1 it decays normally
        let mut e = Env::new();
        e.trigger();
        for _ in 0..2000 {
            e.advance(32.0 / 44_100.0, &ep, 1.0);
        }
        assert!(e.value() < 1.0);
    }

    #[test]
    fn unused_env_is_finished_and_silent() {
        let mut e = Env::new();
        e.advance(1.0, &WS, 1.0);
        assert!(e.finished() && e.value() == 0.0);
        e.release();
        assert!(e.finished());
    }
}

/// A patch that asks for a mode the build lacks fails at load; modes of envelopes nobody reads do not count.
#[cfg(all(test, feature = "us-env-timed"))]
mod gating {
    use super::*;
    use crate::chunk::encode;
    use crate::instrument::UInstrument;
    #[cfg(feature = "us-matrix")]
    use crate::law::host as inv;
    use crate::module;
    #[cfg(feature = "us-matrix")]
    use crate::modmx::{source, target};
    use synth_core::mix::LoadError;

    fn need(edits: &[(usize, f32)]) -> Result<usize, LoadError> {
        let mut v = pm::DEFAULTS;
        edits.iter().for_each(|&(i, x)| v[i] = x);
        UInstrument::need(&encode(&v, &[]))
    }

    fn expect_rate(r: Result<usize, LoadError>) {
        if cfg!(feature = "us-env-rate") {
            assert!(r.is_ok());
        } else {
            assert_eq!(r, Err(LoadError::UnsupportedModule(module::ENV_RATE)));
        }
    }

    #[test]
    fn the_amplitude_envelope_always_counts() {
        expect_rate(need(&[(pm::env(0, e::MODE), 1.0)]));
    }

    #[cfg(feature = "us-matrix")]
    #[test]
    fn another_envelope_counts_only_when_the_matrix_reads_it() {
        assert!(need(&[(pm::env(2, e::MODE), 1.0)]).is_ok(), "not read: its mode is irrelevant");
        let read = [
            (pm::env(2, e::MODE), 1.0),
            (pm::modslot(5, pm::m::SOURCE), inv::inv_choice(source::ENV1 + 2, source::COUNT)),
            (pm::modslot(5, pm::m::TARGET), inv::inv_index(target::GAIN)),
        ];
        expect_rate(need(&read));
    }

    #[test]
    fn an_automated_mode_the_build_lacks_runs_the_mode_it_has() {
        // with the timed mode built in, the rate mode is only reachable when it is built in too
        assert!(!of_mode(false));
        assert_eq!(of_mode(true), cfg!(feature = "us-env-rate"));
    }
}
