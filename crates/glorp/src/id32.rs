//! Glorp effect ID 32: a chorus / ensemble with an all-pass-feedback alternative mode (11 parameters).
//!
//! Independent behavioural implementation written from observed behaviour (research/glorp-devices.md §32); no
//! original binary, table or pseudocode is included. f32, scalar operation order of the reference (no FMA) because
//! the order matters for bit parity. Parameters are read once per [`Id32::process`] call (the reference's block
//! granularity); the base delay glides per sample. Fixed 44.1 kHz (the original never reads the sample rate).
//!
//! | param | meaning |
//! |---|---|
//! | 0 | mode A voices `min(2·trunc(12·p)+2, 16)` |
//! | 1 | wet mix: `out = k·wet + (1−p)·dry` |
//! | 2 | base delay: target `0.002 + 0.4·p³` s (mode A, glides at 0.001/sample from 0); mode B section delay scale `0.0001 + 0.005·p³` s |
//! | 3 | feedback `2p−1` (signed); mode B all-pass coefficient `min(p, 0.999)` |
//! | 4 | depth: mode A `±0.8·p³·base`; mode B `±p` of the section delay (≥ 1/22050 s) |
//! | 5 | LFO rate `15040·p³/44100` rad per sample (× per-line factor) |
//! | 6 | mode A stereo alternation: even lines weight `(1+p, 1−p)`, odd lines `(1−p, 1+p)` |
//! | 7 | mode: `p < 0.5` A (chorus), `p ≥ 0.5` B (all-pass chain) |
//! | 8 | mode B sections `min(trunc(10.999·p)+2, 16)` (2..=12) |
//! | 9 | mode A per-line rate variation `1 + sin(i)·p` |
//! | 10 | enable: `p < 0.5` leaves the block untouched (in place) and freezes all state |
//!
//! State (all kept; nothing is ever reset): 16 stereo delay lines of 2205 samples (each with its write index and the
//! smoothed delay `cur`), 16 LFO phases (advanced only for the lines in use), the smoothed base delay, and mode B's
//! one-sample feedback pair. Lines above the live voice count are frozen and resume from their old history.
//!
//! **Rejected, never approximated** ([`Error`]; the device is left unchanged):
//! * index ≥ 11, non-finite or outside `[0, 1]`;
//! * [`Error::Delay`]: in mode A the delay may reach `base·(1 + 0.8·p4³)·44100` samples, and the original reads
//!   before the start of its 2205-sample line once that exceeds the buffer (undefined; not reproduced). The worst case
//!   must stay ≤ [`MAX_DELAY_SAMPLES`] (5 samples of margin). `new`/`set_param` check the target; `process` also
//!   checks the *current* smoothed base delay (it can still be long from an earlier mode-B setting) and refuses the
//!   whole block without touching anything. Mode B delays never exceed 450 samples.
//!
//! Note events are not part of the API (the original's note slots do nothing). The sine is the reference's table
//! sine shared with [`crate::id34`]. Size: ≈ 282 KB inline (16 lines); put it in a `static`/`Box`, not on a small stack.
//!
//! Not a `synth_core::Device`: its `set_param` cannot report a rejection.

use crate::id34::{make_table, sin_with, TABLE_LEN};
use synth_core::denormal::Guard;

/// Parameter count of the device.
pub const PARAMS: usize = 11;
/// Delay lines of the device (the original allocates 16).
pub const LINES: usize = 16;
/// Samples per delay line in the original (`0x89d`).
pub const CAPACITY: usize = 2205;
/// Largest worst-case read delay accepted in mode A (the buffer is [`CAPACITY`] samples).
pub const MAX_DELAY_SAMPLES: f32 = 2200.0;

/// Why a setting is not supported. The device is left unchanged when one is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parameter index ≥ 11.
    Index,
    /// Not finite or outside `[0, 1]`.
    Range,
    /// Mode A delay would exceed the line buffer (see the module doc).
    Delay,
}

const PI: f32 = 3.141_592_7;
const TAU: f32 = 6.283_185_5;
/// `1/44100` as the reference's f32 constant.
const INV_RATE: f32 = 2.267_573_7e-5;
/// Smallest mode B section delay, `1/22050` s.
const MIN_SECTION: f32 = 4.535_147_3e-5;

/// One stereo delay line as the reference keeps it: ring buffer, write index, current and target delay (seconds).
#[derive(Clone, Debug)]
struct Line {
    cur: f32,
    target: f32,
    widx: usize,
    buf: [[f32; 2]; CAPACITY],
}

impl Line {
    const fn new() -> Self {
        Line { cur: 0.0, target: 0.0, widx: 0, buf: [[0.0; 2]; CAPACITY] }
    }

    /// Linear-interpolated read `cur` seconds behind the write index (before this sample's push).
    fn read(&self) -> [f32; 2] {
        let cap = CAPACITY as f32;
        let mut pos = (0.0 + self.cur) * -44100.0 + self.widx as f32;
        let t = pos + cap;
        if t < cap {
            pos = if t >= 0.0 { t } else { t + cap };
        }
        let i0 = pos as usize;
        let i1 = if i0 + 1 >= CAPACITY { i0 + 1 - CAPACITY } else { i0 + 1 };
        let frac = pos - i0 as f32;
        let (a, b) = (self.buf[i0], self.buf[i1]);
        [frac * b[0] + (1.0 - frac) * a[0], frac * b[1] + (1.0 - frac) * a[1]]
    }

    /// Glides `cur` to `target` (full step: the original's smoothing factor is `min(1.001, 1)`), then writes `v`.
    fn push(&mut self, v: [f32; 2]) {
        self.cur = (self.target - self.cur) * 1.0 + self.cur;
        self.widx = if self.widx + 1 < CAPACITY { self.widx + 1 } else { 0 };
        self.buf[self.widx] = v;
    }
}

#[derive(Clone, Debug)]
pub struct Id32 {
    p: [f32; PARAMS],
    lines: [Line; LINES],
    phase: [f32; LINES],
    /// Smoothed mode A base delay in seconds.
    dsm: f32,
    /// Mode B feedback pair (previous chain output).
    fbst: [f32; 2],
    table: [f32; TABLE_LEN],
}

fn validate(v: f32) -> Result<(), Error> {
    if (0.0..=1.0).contains(&v) { Ok(()) } else { Err(Error::Range) }
}

/// Mode A base-delay target in seconds, `(0.4·p2³) + 0.002`.
fn base_target(p2: f32) -> f32 {
    let c = (p2 * p2) * p2;
    0.4 * c + 0.002
}

/// Mode A needs the worst-case delay inside the line buffer. `base` is the (current or target) base delay.
fn delay_fits(base: f32, p4: f32) -> Result<(), Error> {
    let depth = 1.0 + 0.8 * ((p4 * p4) * p4);
    if (base as f64) * (depth as f64) * 44100.0 <= MAX_DELAY_SAMPLES as f64 { Ok(()) } else { Err(Error::Delay) }
}

/// Whether `new` / `set_param` accept this whole parameter vector (cheap: builds nothing).
pub fn check(p: &[f32; PARAMS]) -> Result<(), Error> {
    p.iter().try_for_each(|&v| validate(v))?;
    if p[7] < 0.5 {
        delay_fits(base_target(p[2]), p[4])?;
    }
    Ok(())
}

impl Id32 {
    /// Builds the device from the 11 normalized parameters (a song chunk's first 11 floats).
    pub fn new(p: &[f32; PARAMS]) -> Result<Self, Error> {
        check(p)?;
        Ok(Id32 {
            p: *p,
            lines: [const { Line::new() }; LINES],
            phase: [0.0; LINES],
            dsm: 0.0,
            fbst: [0.0; 2],
            table: make_table(),
        })
    }

    /// Sets one parameter between `process` calls (all state is kept). The whole resulting parameter vector must be
    /// supported. On `Err` nothing changed.
    ///
    /// **Multi-parameter transitions are order-dependent.** Each call is validated on its own, so every intermediate
    /// vector must be supported, not only the final one. In mode A (`P7 < 0.5`) the worst-case delay of
    /// `P2` and `P4` must fit the line: moving B → A with a long `P2`/`P4`, or raising `P2` and `P4` together, fails with
    /// `Err(Delay)` if the shrinking setting is applied last. Apply the settings that shorten the delay first (lower
    /// `P2`/`P4` while still in mode B, then `P7`; or `P7` to B first, then raise `P2`/`P4`). A failed step leaves the
    /// device unchanged, so the caller can retry in another order; applying an automation batch in recorded order
    /// is only correct if the original's order was valid.
    pub fn set_param(&mut self, index: usize, value: f32) -> Result<(), Error> {
        if index >= PARAMS {
            return Err(Error::Index);
        }
        let mut p = self.p;
        p[index] = value;
        check(&p)?;
        self.p = p;
        Ok(())
    }

    fn sin(&self, x: f32) -> f32 {
        sin_with(&self.table, x)
    }

    /// Processes a stereo block in place. `l` and `r` must have the same length (panics otherwise). Disabled
    /// (`P10 < 0.5`) or empty blocks are left untouched. `Err(Delay)` refuses the block (buffers and state untouched):
    /// the current smoothed base delay is too long for mode A, see the module doc.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) -> Result<(), Error> {
        assert_eq!(l.len(), r.len(), "channel length mismatch");
        let p = self.p;
        if p[10] < 0.5 || l.is_empty() {
            return Ok(());
        }
        let mode_b = p[7] >= 0.5;
        let target = base_target(p[2]);
        if !mode_b {
            // the smoothed delay moves monotonically towards the target: its extreme in this block is one of the two
            delay_fits(self.dsm.max(target), p[4])?;
        }
        let _ftz = Guard::new();
        let c = (p[2] * p[2]) * p[2];
        let fb = (p[3] + p[3]) + -1.0;
        let g = if p[3] < 0.0 { 0.0 } else { 0.999f32.min(p[3]) };
        let depth_b = c * 0.005 + 0.0001; // mode B section delay scale
        let n_a = (2 * ((p[0] * 12.0) as i32) + 2).min(16) as usize;
        let n_b = (((p[8] * 10.999) as i32) + 2).min(16) as usize;
        let rate = ((p[5] * p[5]) * 15040.0) * p[5] * INV_RATE;
        let inv_na = 1.0 / n_a as f32;
        let inv_nb = 1.0 / n_b as f32;
        let k_a = 2.25 * inv_na;
        let k_rate_b = 0.3 * inv_nb;
        let pi_nb = inv_nb * PI;
        let depth_a = (p[4] * p[4]) * p[4];
        let (hi, lo) = (p[6] + 1.0, 1.0 - p[6]);
        for k in 0..l.len() {
            self.dsm = (target - self.dsm) * 0.001 + self.dsm;
            let x = [l[k], r[k]];
            let out = if !mode_b {
                let mut acc = [0.0f32; 2];
                for i in 0..n_a {
                    let fi = i as f32;
                    let m = self.sin(fi) * p[9] + 1.0;
                    let arg = (fi * PI) * inv_na + self.phase[i];
                    let s = self.sin(arg);
                    let delay = (depth_a * (self.dsm * 0.8)) * s + self.dsm;
                    let line = &mut self.lines[i];
                    line.target = delay;
                    let tap = line.read();
                    line.push([tap[0] * fb + x[0], tap[1] * fb + x[1]]);
                    let v = m * rate + self.phase[i];
                    self.phase[i] = if v < TAU { v } else { v + -TAU };
                    let w = if i & 1 == 0 { [hi, lo] } else { [lo, hi] };
                    acc[0] += tap[0] * w[0];
                    acc[1] += tap[1] * w[1];
                }
                let k1 = p[1] * k_a;
                let dry = 1.0 - p[1];
                [k1 * acc[0] + dry * x[0], k1 * acc[1] + dry * x[1]]
            } else {
                let mut y = [self.fbst[0] * fb + x[0], self.fbst[1] * fb + x[1]];
                for i in 0..n_b {
                    let fi = i as f32;
                    let scale = fi * k_rate_b + 1.0;
                    let arg = pi_nb * fi + self.phase[i];
                    let t = (self.sin(arg) * p[4] + 1.0) * depth_b;
                    let line = &mut self.lines[i];
                    line.target = if t > MIN_SECTION { t } else { MIN_SECTION };
                    let tap = line.read();
                    let w = [y[0] - tap[0] * g, y[1] - tap[1] * g];
                    line.push(w);
                    y = [g * w[0] + tap[0], g * w[1] + tap[1]];
                    let v = scale * rate + self.phase[i];
                    self.phase[i] = if v < TAU { v } else { v + -TAU };
                }
                self.fbst = y;
                let dry = 1.0 - p[1];
                [p[1] * y[0] + dry * x[0], p[1] * y[1] + dry * x[1]]
            };
            l[k] = out[0];
            r[k] = out[1];
        }
        Ok(())
    }
}
