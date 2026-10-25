//! Upstream `Smasher`: peak compressor with 2 ms lookahead and optional sidechain key (channels 2/3).

use crate::helpers::{ms_to_len, db_to_scalar, param_to_boolean, param_to_db, scalar_to_env_value};
use synth_core::device::{Ctx, Device, Io, SIDE_L};
use synth_core::dsp::Ring;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;
use synth_core::rng::Rng;

const LOOKAHEAD_MS: f32 = 2.0;

pub struct Smasher<'a> {
    sidechain: bool,
    input_gain: f32,
    threshold: f32,
    ratio: f32,
    attack: f32,
    release: f32,
    output_gain: f32,
    buf: [Ring<'a>; 2],
    peak: f32,
}

impl<'a> Smasher<'a> {
    pub fn need(_: &[u8], env: &Env) -> Result<usize, LoadError> {
        Ok(2 * Pool::need::<f32>(ms_to_len(LOOKAHEAD_MS, env.rate)))
    }
    pub fn new(_: &[u8], env: &Env, pool: &mut Pool<'a>, _: &mut dyn Rng) -> Result<Self, LoadError> {
        let cap = ms_to_len(LOOKAHEAD_MS, env.rate);
        Ok(Smasher {
            sidechain: false,
            input_gain: 0.0,
            threshold: 0.0,
            ratio: 2.0,
            attack: 1.0,
            release: 200.0,
            output_gain: 0.0,
            buf: [Ring::new(pool.slice(cap)?), Ring::new(pool.slice(cap)?)],
            peak: 0.0,
        })
    }
}

impl Device for Smasher<'_> {
    fn param_count(&self) -> usize {
        7
    }
    fn set_param(&mut self, i: usize, v: f32) {
        match i {
            0 => self.sidechain = param_to_boolean(v),
            1 => self.input_gain = param_to_db(v, 12.0),
            2 => self.threshold = param_to_db(v / 2.0, 36.0),
            3 => self.ratio = v * v * 18.0 + 2.0,
            4 => self.attack = scalar_to_env_value(v) / 5.0,
            5 => self.release = scalar_to_env_value(v),
            6 => self.output_gain = param_to_db(v, 12.0),
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.buf.iter_mut().for_each(Ring::clear);
        self.peak = 0.0;
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        for b in self.buf.iter_mut() {
            b.set_len(ms_to_len(LOOKAHEAD_MS, ctx.rate));
        }
        let in_gain = db_to_scalar(self.input_gain);
        let out_gain = db_to_scalar(self.output_gain);
        let key = if self.sidechain { SIDE_L } else { 0 };
        let env = (1000.0 / ctx.rate) as f32;
        let (att, rel) = (env / self.attack, env / self.release);
        let thr = db_to_scalar(self.threshold);
        for i in 0..n {
            self.buf[0].write(io.ch[0][i] * in_gain);
            self.buf[1].write(io.ch[1][i] * in_gain);
            let kl = libm::fabsf(io.ch[key][i] * in_gain);
            let kr = libm::fabsf(io.ch[key + 1][i] * in_gain);
            let level = if kl >= kr { kl } else { kr };
            if level > self.peak {
                self.peak += att;
                if self.peak > level {
                    self.peak = level;
                }
            } else {
                self.peak -= rel;
                if self.peak < level {
                    self.peak = level;
                }
            }
            let mut gain = out_gain;
            if self.peak > thr {
                gain *= (thr + (self.peak - thr) / self.ratio) / self.peak;
            }
            io.ch[0][i] = self.buf[0].oldest() * gain;
            io.ch[1][i] = self.buf[1].oldest() * gain;
        }
    }
}
