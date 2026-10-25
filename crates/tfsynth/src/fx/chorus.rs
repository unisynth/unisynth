//! Chorus (spec §4.4): three cascaded single-tap lines per channel (`y = x + g·x[n−d]`, g = 0.7p, no feedback),
//! each with its own sine LFO started at a random phase in [0, 1) rad. Once per 512-frame block every line's delay
//! jumps to `clamp(1 + 9·depth·(sin φ + 0.5), 1, 10)` ms — the reference's mis-scaled offset, which leaves a flat spot
//! at 1 ms — and its phase advances by the voice-LFO rate law. Fix: phases wrap at 2π (the reference never wraps).

use crate::lfo::rate;
use crate::params::{CHORUS_DEPTH, CHORUS_GAIN, CHORUS_RATE, COUNT};
use crate::rng::Xorshift;
use synth_core::dsp::Ring;
use synth_core::pool::{OutOfMemory, Pool};
use synth_core::rng::Rng;

const LINES: usize = 6;
/// > 10 ms + 1 at 44.1 kHz.
const CAP: usize = 512;
const TWO_PI: f32 = 2.0 * core::f32::consts::PI;

pub struct Chorus<'a> {
    line: [Ring<'a>; LINES],
    pub phase: [f32; LINES],
    delay: [usize; LINES],
    gain: f32,
}

impl<'a> Chorus<'a> {
    pub fn need() -> usize {
        LINES * Pool::need::<f32>(CAP)
    }

    pub fn new(pool: &mut Pool<'a>, rng: &mut Xorshift) -> Result<Self, OutOfMemory> {
        let mut bufs = [None, None, None, None, None, None];
        for b in bufs.iter_mut() {
            *b = Some(Ring::new(pool.slice(CAP)?));
        }
        Ok(Chorus {
            line: bufs.map(|b| b.unwrap()),
            phase: core::array::from_fn(|_| rng.next_f32()),
            delay: [1; LINES],
            gain: 0.0,
        })
    }

    /// Fresh-instance state in place (new random phases from `rng`, as at creation).
    pub fn clear(&mut self, rng: &mut Xorshift) {
        self.line.iter_mut().for_each(|l| l.clear());
        self.phase = core::array::from_fn(|_| rng.next_f32());
        self.delay = [1; LINES];
        self.gain = 0.0;
    }

    /// Block-rate update (once per 512 frames): delays from the current phases, then the phases advance.
    pub fn control(&mut self, p: &[f32; COUNT]) {
        self.gain = p[CHORUS_GAIN] * 0.7;
        let step = rate(p[CHORUS_RATE]);
        for i in 0..LINES {
            let s = libm::sinf(self.phase[i]) + 0.5;
            let ms = (s * p[CHORUS_DEPTH] * 9.0 + 1.0).clamp(1.0, 10.0);
            self.delay[i] = ((44100.0 * ms / 1000.0) as usize).max(1);
            self.phase[i] += step;
            if self.phase[i] >= TWO_PI {
                self.phase[i] -= TWO_PI;
            }
        }
    }

    /// Lines 0, 2, 4 in series on the left, 1, 3, 5 on the right.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        for i in 0..LINES {
            let buf: &mut [f32] = if i % 2 == 0 { &mut *l } else { &mut *r };
            let (line, d, g) = (&mut self.line[i], self.delay[i], self.gain);
            for v in buf.iter_mut() {
                line.write(*v * g);
                *v += line.at(d + 1);
            }
        }
    }
}
