//! Read-only voice probe for the editor (feature `probe`, host only; change add-device-live-view design D5): a
//! snapshot of the sounding voices and, when enabled, each filter slot's input and output summed over voices. Never
//! in the player build; reading it changes no rendered sample.

use crate::env::Stage;
use crate::params as pm;
use std::boxed::Box;
use std::vec::Vec;

/// Frames of filter signal kept per slot (the newest ones).
pub const TAP_LEN: usize = 2048;
const _: () = assert!(TAP_LEN.is_power_of_two());

/// One sounding voice at the end of its control segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceSnap {
    /// Note-on order: unique per note-on, so a stolen voice gets a new id.
    pub id: u32,
    pub note: u8,
    /// Voice gain after the oscillators (master, velocity, envelope 1 when it shapes the output).
    pub amp: f32,
    /// Envelope 1 is releasing (or has finished).
    pub released: bool,
    /// Seconds since note-on.
    pub age_s: f32,
    /// Each envelope's level and stage.
    pub env: [(f32, Stage); pm::ENV_SLOTS],
    /// Each LFO's phase in turns.
    pub lfo: [f32; pm::LFO_SLOTS],
    /// Each sampler slot's play position as a fraction of its sample (`None` = not a sampler).
    pub smp: [Option<f32>; pm::OSC_SLOTS],
    /// Each filter slot's cutoff after the mod matrix as (normalized value after Scale routes, Offset routes in Hz);
    /// `cutoff_hz` turns it into Hz. `None` = off or a V2 filter.
    pub cutoff: [Option<(f32, f32)>; pm::FILT_SLOTS],
}

/// A probed cutoff in Hz, as the voice computes it.
pub fn cutoff_hz((norm, offset): (f32, f32)) -> f32 {
    (crate::law::cutoff(norm) + offset).max(0.0).min(19_980.0)
}

/// What the probe reports after a render call.
#[derive(Clone, Debug, Default)]
pub struct Probe {
    pub voices: Vec<VoiceSnap>,
    /// Per filter slot that is on, with taps enabled: `(input, output)`, `TAP_LEN` frames oldest first.
    pub taps: [Option<(Vec<f32>, Vec<f32>)>; pm::FILT_SLOTS],
}

/// The filter taps' ring: `[slot][input, output][frame]`, `w` = the next frame to write.
pub struct Ring {
    buf: Box<[[[f32; TAP_LEN]; 2]; pm::FILT_SLOTS]>,
    w: usize,
    /// Frames the current device call has been given (cleared by `begin`).
    n: usize,
}

impl Default for Ring {
    fn default() -> Self {
        Ring { buf: Box::new([[[0.0; TAP_LEN]; 2]; pm::FILT_SLOTS]), w: 0, n: 0 }
    }
}

impl Ring {
    /// Clears the `n` frames a device call is about to add into.
    pub fn begin(&mut self, n: usize) {
        debug_assert!(n <= TAP_LEN, "a device call longer than the tap ring");
        for k in 0..n.min(TAP_LEN) {
            let i = (self.w + k) % TAP_LEN;
            self.buf.iter_mut().for_each(|s| s.iter_mut().for_each(|c| c[i] = 0.0));
        }
        self.n = n;
    }

    /// Moves the write position past the call.
    pub fn end(&mut self) {
        self.w = (self.w + self.n) % TAP_LEN;
        self.n = 0;
    }

    /// Adds one voice's filter slot `slot` input and output at frame `frame` of the device call.
    #[inline]
    pub fn add(&mut self, frame: usize, slot: usize, inp: f32, out: f32) {
        // TAP_LEN is a power of two: the mask keeps the index in range without a division or a bounds check
        let i = (self.w + frame) & (TAP_LEN - 1);
        if let Some(c) = self.buf.get_mut(slot) {
            c[0][i] += inp;
            c[1][i] += out;
        }
    }

    /// Slot `s`'s input and output, oldest frame first.
    pub fn read(&self, s: usize) -> (Vec<f32>, Vec<f32>) {
        let lin = |c: &[f32; TAP_LEN]| (0..TAP_LEN).map(|k| c[(self.w + k) % TAP_LEN]).collect();
        (lin(&self.buf[s][0]), lin(&self.buf[s][1]))
    }
}

