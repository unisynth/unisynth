//! Delay (spec §4.2): per channel a feedback line of `trunc(sr·trunc(1000p)/1000)` samples (1..44 100). The line
//! stores `b = k·(x + b_old)` and the output is `x + b`, so the dry also appears once at gain k (the reference's
//! zero-latency copy) and echoes come at k², k³ …. A time change keeps the contents and clamps the position.

use crate::params::{COUNT, DELAY_DECAY, DELAY_LEFT, DELAY_RIGHT};
use synth_core::dsp::Ring;
use synth_core::pool::{OutOfMemory, Pool};

/// 1000 ms at 44.1 kHz.
pub const MAX: usize = 44100;

pub struct Delay<'a> {
    line: [Ring<'a>; 2],
}

/// Samples for a delay parameter: whole milliseconds, then `trunc(44100·ms/1000)`, at least 1.
pub fn length(p: f32) -> usize {
    let ms = (p * 1000.0) as u32 as f32;
    ((44100.0 * ms / 1000.0) as usize).clamp(1, MAX)
}

impl<'a> Delay<'a> {
    pub fn need() -> usize {
        2 * Pool::need::<f32>(MAX)
    }

    pub fn new(pool: &mut Pool<'a>) -> Result<Self, OutOfMemory> {
        Ok(Delay { line: [Ring::new(pool.slice(MAX)?), Ring::new(pool.slice(MAX)?)] })
    }

    /// Fresh-instance state in place: zeroed lines, positions at 0.
    pub fn clear(&mut self) {
        for l in self.line.iter_mut() {
            l.set_len_keep(MAX);
            l.clear();
        }
    }

    pub fn run(&mut self, p: &[f32; COUNT], l: &mut [f32], r: &mut [f32]) {
        let k = p[DELAY_DECAY];
        for (c, (buf, t)) in [(l, DELAY_LEFT), (r, DELAY_RIGHT)].into_iter().enumerate() {
            let line = &mut self.line[c];
            line.set_len_keep(length(p[t]));
            for v in buf.iter_mut() {
                let b = (*v + line.oldest()) * k;
                line.write(b);
                *v += b;
            }
        }
    }
}
