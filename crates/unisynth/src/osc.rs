//! Oscillator slots (design D4): the types a slot can be, their per-voice state, the control values they ramp, and the
//! per-sample generators. The phase-mod operator is Falcon's: a sine blended with `Square35` (`sin 3φ/3 + sin 5φ/5`),
//! self-feedback, and a send into the next slot's phase. The band-limited saw/pulse is Slaughter's integrated BLIT:
//! a sinc impulse train (one impulse per period for the saw, a second one a pulse width later for the pulse)
//! integrated by a leaky integrator, with the DC of the impulse train removed.

use crate::module;
use crate::params::{self as pm, o, osc, osc_type, COUNT};
use crate::ramp::Ramp;
use crate::smp;
use crate::tables::sin_turns;
#[cfg(feature = "us-osc-wt")]
pub use crate::wt::Wt;
/// Stands in for the wavetable state when the module is left out of the build (never instantiated as a slot kind).
#[cfg(not(feature = "us-osc-wt"))]
pub struct Wt;
#[cfg(not(feature = "us-osc-wt"))]
impl Wt {
    pub const fn new() -> Self {
        Wt
    }
}
use synth_core::mix::LoadError;

/// What a slot is, fixed at load (`o::TYPE` is a structural parameter).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Off,
    Pm,
    Blit,
    /// Sample player: a GSM sample or a GM.DLS wave.
    Sampler,
    /// Spectral wavetable: needs a [`crate::wt::Wt`] from the pool.
    #[cfg(feature = "us-osc-wt")]
    Wt,
    /// V2's oscillator (change add-v2-import): mono, works on the voice's running sum ([`v2_sample`]).
    V2,
}

impl Kind {
    /// The kind for an `osc_type`, or the module that is missing from this build or not written yet.
    pub fn from_type(ty: usize) -> Result<Kind, LoadError> {
        let missing = |m| Err(LoadError::UnsupportedModule(m));
        match ty {
            osc_type::OFF => Ok(Kind::Off),
            osc_type::PM if cfg!(feature = "us-osc-pm") => Ok(Kind::Pm),
            osc_type::PM => missing(module::OSC_PM),
            osc_type::SAMPLER if cfg!(feature = "us-osc-sample") => Ok(Kind::Sampler),
            osc_type::BLIT if cfg!(feature = "us-osc-blit") => Ok(Kind::Blit),
            osc_type::BLIT => missing(module::OSC_BLIT),
            osc_type::V2 if cfg!(feature = "us-osc-v2") => Ok(Kind::V2),
            osc_type::V2 => missing(module::OSC_V2),
            #[cfg(feature = "us-osc-wt")]
            osc_type::WAVETABLE => Ok(Kind::Wt),
            #[cfg(not(feature = "us-osc-wt"))]
            osc_type::WAVETABLE => missing(module::OSC_WAVETABLE),
            _ => missing(module::OSC_SAMPLER),
        }
    }

    /// The three slot kinds of a patch.
    pub fn all(values: &[f32; COUNT]) -> Result<[Kind; pm::OSC_SLOTS], LoadError> {
        let mut k = [Kind::Off; pm::OSC_SLOTS];
        for (i, slot) in k.iter_mut().enumerate() {
            *slot = Kind::from_type(osc_type::decode(values[osc(i, o::TYPE)]))?;
        }
        Ok(k)
    }
}

/// Per-voice state of one slot.
#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub kind: Kind,
    /// Phase in turns.
    pub phase: f64,
    /// The slot's previous output (feedback).
    pub prev: f32,
    /// Saw/pulse: the leaky integrator.
    pub integral: f64,
    /// Sampler: playback position.
    pub smp: smp::State,
    /// Saw/pulse: no note has started it yet, so `phase` is still the unscaled draw.
    pub fresh: bool,
    /// V2: wave counter, noise filter and seed (`v2::Opt`: absent without the V2 voice features).
    pub v2s: crate::v2::Opt<V2Slot>,
}

/// A V2 slot's running state: wave counter (one turn = 2³²), noise filter and noise seed; `init` once the seed was
/// set.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V2Slot {
    pub cnt: u32,
    pub nf: crate::v2::Lrc,
    pub nseed: u32,
    pub init: bool,
}

impl Slot {
    /// `phase` is the slot's start phase (turns), drawn once when the voice is built.
    pub const fn new(kind: Kind, phase: f64) -> Self {
        Slot {
            kind,
            phase,
            prev: 0.0,
            integral: 0.0,
            smp: smp::State::new(),
            fresh: true,
            v2s: crate::v2::Opt::new(V2Slot { cnt: 0, nf: crate::v2::Lrc { l: 0.0, b: 0.0 }, nseed: 0, init: false }),
        }
    }

    /// Voice start. The phase-mod operator restarts from `phase`; the saw/pulse keeps its phase and integrator, which
    /// run on across notes as Slaughter's do.
    ///
    /// `spread` is the fraction of a turn the random start phase reaches (`law::start_phase`): `phase` is the voice's
    /// draw for the operators, and the saw/pulse scales its own draw at its first note, when its pitch is known.
    pub fn start(&mut self, phase: f64, p: f32, inc: f32) {
        match self.kind {
            Kind::Pm if cfg!(feature = "us-osc-pm") => {
                self.phase = crate::law::start_phase(phase, p, inc);
                self.prev = 0.0;
            }
            Kind::Blit if cfg!(feature = "us-osc-blit") && self.fresh => {
                self.phase = crate::law::start_phase(self.phase, p, inc);
                self.fresh = false;
            }
            _ => {}
        }
    }
}

/// Control values of one slot at a control state.
#[derive(Clone, Copy, Debug, Default)]
pub struct SlotT {
    /// Phase increment per sample, in turns.
    pub inc: f32,
    pub gain: f32,
    pub mix: f32,
    pub pan_l: f32,
    pub pan_r: f32,
    pub pm_out: f32,
    pub feedback: f32,
    /// Phase-mod: weight of `Square35`. Saw/pulse: 0 saw .. 1 pulse.
    pub wf: f32,
    /// Saw/pulse: pulse width (fraction of the period).
    pub pw: f32,
    /// Saw/pulse: integrator leak per sample.
    pub leak: f32,
    /// Wavetable: left/right phase offset per layer, and extra offset per layer, in phase per sample.
    pub det: f32,
    pub spread: f32,
    /// Wavetable: drive before the hard clip.
    pub drive: f32,
    /// Wavetable: unison layers.
    pub layers: usize,
    /// Sampler: loop, interpolation and loop bounds.
    pub smp: smp::Cfg,
    /// V2: waveform, ring, break point, noise filter and the format-version switches.
    pub v2: crate::v2::Opt<V2T>,
}

/// A V2 slot's settings for a control segment (V2 keeps them for a frame; they are not ramped).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V2T {
    /// V2 mode 0..7 (`V2Osc::Mode`).
    pub mode: u8,
    pub ring: bool,
    /// Break point (colour) as 0.32 fixed point.
    pub brpt: u32,
    pub nffrq: f32,
    pub nfres: f32,
    /// Format version below 6: native sine for the sine and FM modes.
    pub native_sin: bool,
    /// Format version below 3: saw/triangle and pulse are the year-2000 4× oversampled box filter (v2redux
    /// `DELTA_OSC_BOXFILTER`).
    pub box4: bool,
    /// Format version below 5: the older noise (MSVC LCG, its own filter recurrence; `DELTA_NOISE_LCG_MSVC`).
    pub old_noise: bool,
}

/// V2 oscillator modes (`V2Osc::Mode`).
pub mod v2mode {
    pub const OFF: u8 = 0;
    pub const TRI_SAW: u8 = 1;
    pub const PULSE: u8 = 2;
    pub const SIN: u8 = 3;
    pub const NOISE: u8 = 4;
    pub const FM: u8 = 5;
    pub const AUX_A: u8 = 6;
    pub const AUX_B: u8 = 7;
}

/// Whether oscillator slot `i` of patch `p` is a V2 aux oscillator (mode aux A or aux B, format version 6): it reads
/// the aux buses through the instrument's extra input pairs, which only the channel stage's build has (`us-chan`).
pub fn v2_aux(p: &[f32; crate::params::COUNT], i: usize) -> bool {
    use crate::params as pm;
    pm::osc_type::decode(p[pm::osc(i, pm::o::TYPE)]) == pm::osc_type::V2
        && (pm::v2_raw(p[pm::osc(i, pm::o::SHAPE + pm::shape::V2_MODE)]) as i32 & 7) as u8 >= v2mode::AUX_A
        && pm::era(p).0 >= 6
}

/// What a V2 slot does to the voice's running sum this sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum V2Out {
    Add(f32),
    /// Ring mode: multiplies the running sum.
    Mul(f32),
    /// FM and the aux input replace it.
    Set(f32),
}

/// The ramps that carry a slot's control values across a segment.
#[derive(Clone, Copy, Debug, Default)]
pub struct SlotRamps {
    inc: Ramp,
    gain: Ramp,
    mix: Ramp,
    pan_l: Ramp,
    pan_r: Ramp,
    pm_out: Ramp,
    feedback: Ramp,
    wf: Ramp,
    pw: Ramp,
    det: Ramp,
    spread: Ramp,
    drive: Ramp,
    leak: f32,
    layers: usize,
    smp: smp::Cfg,
    v2: crate::v2::Opt<V2T>,
}

impl SlotRamps {
    pub fn set(&mut self, t: &SlotT) {
        self.inc.set(t.inc);
        self.gain.set(t.gain);
        self.mix.set(t.mix);
        self.pan_l.set(t.pan_l);
        self.pan_r.set(t.pan_r);
        self.pm_out.set(t.pm_out);
        self.feedback.set(t.feedback);
        self.wf.set(t.wf);
        self.pw.set(t.pw);
        self.det.set(t.det);
        self.spread.set(t.spread);
        self.drive.set(t.drive);
        self.leak = t.leak;
        self.layers = t.layers;
        self.smp = t.smp;
        if cfg!(feature = "us-osc-v2") {
            self.v2 = t.v2;
        }
    }

    pub fn to(&mut self, t: &SlotT, frames: u32) {
        self.inc.to(t.inc, frames);
        self.gain.to(t.gain, frames);
        self.mix.to(t.mix, frames);
        self.pan_l.to(t.pan_l, frames);
        self.pan_r.to(t.pan_r, frames);
        self.pm_out.to(t.pm_out, frames);
        self.feedback.to(t.feedback, frames);
        self.wf.to(t.wf, frames);
        self.pw.to(t.pw, frames);
        self.det.to(t.det, frames);
        self.spread.to(t.spread, frames);
        self.drive.to(t.drive, frames);
        self.leak = t.leak;
        self.layers = t.layers;
        self.smp = t.smp;
        if cfg!(feature = "us-osc-v2") {
            self.v2 = t.v2;
        }
    }

    pub fn retarget(&mut self, t: &SlotT, frames: u32) {
        self.inc.retarget(t.inc, frames);
        self.gain.retarget(t.gain, frames);
        self.mix.retarget(t.mix, frames);
        self.pan_l.retarget(t.pan_l, frames);
        self.pan_r.retarget(t.pan_r, frames);
        self.pm_out.retarget(t.pm_out, frames);
        self.feedback.retarget(t.feedback, frames);
        self.wf.retarget(t.wf, frames);
        self.pw.retarget(t.pw, frames);
        self.det.retarget(t.det, frames);
        self.spread.retarget(t.spread, frames);
        self.drive.retarget(t.drive, frames);
        self.leak = t.leak;
        self.layers = t.layers;
        self.smp = t.smp;
        if cfg!(feature = "us-osc-v2") {
            self.v2 = t.v2;
        }
    }
}

const INV_TAU: f32 = 1.0 / (2.0 * core::f32::consts::PI);

/// One sample of a slot: `(left, right, send)` = its output after mix and pan, and the phase-mod send for the next
/// slot in radians. `pm_in` is the previous slot's send.
#[inline]
pub fn sample(slot: &mut Slot, wt: Option<&mut Wt>, src: &smp::Source, r: &mut SlotRamps, pm_in: f32) -> (f32, f32, f32) {
    let inc = r.inc.next();
    let gain = r.gain.next();
    let mix = r.mix.next();
    let (pl, pr) = (r.pan_l.next(), r.pan_r.next());
    let send = r.pm_out.next();
    let fb = r.feedback.next();
    let wf = r.wf.next();
    let pw = r.pw.next();
    let (det, spread, drive) = (r.det.next(), r.spread.next(), r.drive.next());
    #[cfg(not(feature = "us-osc-wt"))]
    let _ = (wt, det, spread, drive);
    match slot.kind {
        Kind::Pm if cfg!(feature = "us-osc-pm") => {
            let t = slot.phase + ((fb * slot.prev + pm_in) * INV_TAU) as f64;
            let y = (sin_turns(t) + (sin_turns(3.0 * t) * (1.0 / 3.0) + sin_turns(5.0 * t) * 0.2) * wf) * gain;
            slot.prev = y;
            slot.phase += inc as f64;
            slot.phase -= libm::floor(slot.phase);
            (y * mix * pl, y * mix * pr, y * send)
        }
        Kind::Sampler if cfg!(feature = "us-osc-sample") => {
            let y = slot.smp.next(src, inc, &r.smp) * gain;
            (y * mix * pl, y * mix * pr, y * send)
        }
        Kind::Blit if cfg!(feature = "us-osc-blit") => {
            let y = blit(slot, inc as f64, wf as f64, pw as f64, r.leak as f64) * gain;
            (y * mix * pl, y * mix * pr, y * send)
        }
        #[cfg(feature = "us-osc-wt")]
        Kind::Wt => {
            let (a, b) = match wt {
                Some(wt) => wavetable(wt, inc, det, spread, drive, r.layers),
                None => (0.0, 0.0),
            };
            (a * gain * mix * pl, b * gain * mix * pr, (a + b) * 0.5 * gain * send)
        }
        // a type of a module this build lacks is rejected at load; the guards let the compiler drop its code
        _ => (0.0, 0.0, 0.0),
    }
}

const TWO_32: f32 = 4_294_967_296.0;

/// One sample of a V2 slot (`V2Osc::render*`, v2redux's assembly-faithful versions). `mono` is the voice's running sum
/// (the FM modulator), `aux` the aux A and aux B input pairs at this sample (the bus mixes so far in the frame). The counter advances by the ramped increment
/// as V2's integer `freq`.
#[inline]
pub fn v2_sample(slot: &mut Slot, r: &mut SlotRamps, mono: f32, aux: [(f32, f32); 2]) -> V2Out {
    let mut st = slot.v2s.get();
    let y = v2_step(&mut st, r, mono, aux);
    slot.v2s.set(st);
    y
}

#[inline(always)]
fn v2_step(slot: &mut V2Slot, r: &mut SlotRamps, mono: f32, aux: [(f32, f32); 2]) -> V2Out {
    use crate::v2::{fastsin, fastsinrc, utof23};
    let inc = r.inc.next();
    let gain = r.gain.next() * r.mix.next();
    let _ = (r.pan_l.next(), r.pan_r.next(), r.pm_out.next(), r.feedback.next(), r.wf.next(), r.pw.next());
    let _ = (r.det.next(), r.spread.next(), r.drive.next());
    let t = &r.v2.get();
    let freq = (inc * TWO_32) as i64 as u32;
    let out = |y: f32| if t.ring { V2Out::Mul(y) } else { V2Out::Add(y) };
    match t.mode {
        v2mode::TRI_SAW | v2mode::PULSE if t.box4 => {
            // four sub-steps of a quarter increment, averaged
            let step = freq / 4;
            let col = utof23(t.brpt);
            let bg = gain * 0.25;
            let mut acc = 0.0f32;
            let mut c = slot.cnt;
            for _ in 0..4 {
                if t.mode == v2mode::TRI_SAW {
                    let p = f32::from_bits((c >> 9) | 0x3f80_0000);
                    acc += if c < t.brpt {
                        p * (2.0 / col.max(1e-9)) + (-1.0 - 2.0 / col.max(1e-9))
                    } else {
                        p * (-2.0 / (1.0 - col).max(1e-9)) + (4.0 / (1.0 - col).max(1e-9) - 1.0)
                    };
                } else if c >= t.brpt {
                    acc += 2.0;
                }
                c = c.wrapping_add(step);
            }
            slot.cnt = c;
            out(if t.mode == v2mode::TRI_SAW { acc * bg } else { (acc - 4.0) * bg })
        }
        v2mode::TRI_SAW | v2mode::PULSE => {
            let cnt = slot.cnt;
            let brpt = t.brpt;
            let old_up = cnt.wrapping_sub(freq) < brpt;
            let new_up = cnt < brpt;
            let carry = cnt < freq;
            slot.cnt = cnt.wrapping_add(freq);
            let f = utof23(freq).max(1e-9);
            let col = utof23(brpt);
            let p = utof23(cnt);
            let code = (carry as u8) << 2 | (old_up as u8) << 1 | new_up as u8;
            if t.mode == v2mode::TRI_SAW {
                // the transition cases cancel catastrophically and then divide by f: V2 ran them on the x87 (15-bit
                // exponent); in f32 a low note picks up audible noise, so they run in f64 (as v2redux's portable build)
                let (f, g) = (f as f64, gain as f64);
                let omf = 1.0 - f;
                let rcpf = 1.0 / f;
                let col = col as f64;
                let (c1, c2) = (g / col.max(1e-9), -g / (1.0 - col).max(1e-9));
                let p = p as f64 - col;
                let y = match code {
                    3 => c1 * (p + p - f),
                    0 => c2 * (p + p - f),
                    2 => rcpf * (c2 * (p * p) - c1 * ((p - f) * (p - f))),
                    5 => {
                        let t = p + 1.0 - f;
                        -((c2 * (t * t) - c1 * (p * p)) + g) * rcpf
                    }
                    7 => -((c1 * (omf * (p + p + omf))) + g) * rcpf,
                    4 => -((c2 * (omf * (p + p + omf))) + g) * rcpf,
                    _ => 0.0,
                };
                out((y + g) as f32)
            } else {
                let gdf = gain / f;
                out(match code {
                    3 => gain,
                    0 => -gain,
                    2 => gdf * 2.0 * (col - p) + gain,
                    5 => gdf * 2.0 * p - gain,
                    7 => gdf * 2.0 * (col - 1.0) + gain,
                    4 => gdf * 2.0 * col - gain,
                    _ => 0.0,
                })
            }
        }
        v2mode::SIN => {
            let cnt = slot.cnt;
            slot.cnt = cnt.wrapping_add(freq);
            if t.native_sin {
                return out(gain * crate::fm::sinf(utof23(cnt) * core::f32::consts::TAU));
            }
            let mut phase = cnt.wrapping_add(0x4000_0000);
            if phase & 0x8000_0000 != 0 {
                phase = !phase;
            }
            let x = f32::from_bits((phase >> 8) | 0x3f80_0000) * core::f32::consts::PI - 4.712_389;
            out(gain * fastsin(x))
        }
        v2mode::NOISE if t.old_noise => {
            slot.nseed = slot.nseed.wrapping_mul(214_013).wrapping_add(2_531_011);
            let n = f32::from_bits(((slot.nseed & 0xffff) << 7) | 0x4000_0000) - 3.0;
            let (sl, sb, f, r) = (slot.nf.l, slot.nf.b, t.nffrq, t.nfres);
            let bb = sb + sl * f;
            let h = (n - sl) * r - bb;
            let ll = sl + h * f;
            (slot.nf.l, slot.nf.b) = (ll, bb);
            out(gain * (h + bb + ll))
        }
        v2mode::NOISE => {
            let n = crate::v2::frandom(&mut slot.nseed);
            let h = slot.nf.step(n, t.nffrq, t.nfres);
            out(gain * (t.nfres * (slot.nf.l + h) + slot.nf.b))
        }
        v2mode::FM => {
            let cnt = slot.cnt;
            slot.cnt = cnt.wrapping_add(freq);
            let y = if t.native_sin {
                // the x87 `fistp`: round to nearest, and the "integer indefinite" 0x80000000 when out of range (a
                // loud modulator collapses the carrier, as in V2 before version 6)
                let m = libm::rint((mono as f64) * 4.0 * 2_147_483_648.0);
                let modi = if (-2_147_483_648.0..2_147_483_648.0).contains(&m) { m as i32 as u32 } else { 0x8000_0000 };
                gain * crate::fm::sinf(utof23(cnt.wrapping_add(modi)) * core::f32::consts::TAU)
            } else {
                gain * fastsinrc((mono * 2.0 + f32::from_bits((cnt >> 9) | 0x3f80_0000)) * core::f32::consts::TAU)
            };
            if t.ring { V2Out::Mul(y) } else { V2Out::Set(y) }
        }
        v2mode::AUX_A | v2mode::AUX_B => {
            // V2's `renderAux`: the bus's (l + r) · gain · 0.6 (fcgain), replacing the sum or ring-multiplying it
            let (l, r) = aux[(t.mode - v2mode::AUX_A) as usize];
            let a = (l + r) * gain * 0.6;
            V2Out::Set(if t.ring { a * mono } else { a })
        }
        _ => V2Out::Add(0.0),
    }
}

#[cfg(feature = "us-osc-wt")]
/// One sample of the unison wavetable: layer `k` reads the table at `|inc ± det| ± k·spread` on the left/right phase,
/// non-interpolated (as Tunefish does), drives and hard-clips each read, and sums the layers.
#[inline]
fn wavetable(wt: &mut Wt, inc: f32, det: f32, spread: f32, drive: f32, layers: usize) -> (f32, f32) {
    let wave = wt.table.wave();
    let (mut l, mut r) = (0.0f32, 0.0f32);
    let (f1, f2) = (inc + det, inc - det);
    for (k, ph) in wt.phase.iter_mut().take(layers).enumerate() {
        let (fl, fr) = (libm::fabsf(f1 + k as f32 * spread), libm::fabsf(f2 - k as f32 * spread));
        l += (wave[(ph[0] * 511.0) as usize & 511] * drive).max(-1.0).min(1.0);
        r += (wave[(ph[1] * 511.0) as usize & 511] * drive).max(-1.0).min(1.0);
        // a phase past 1 loses its whole turns (a phase of exactly 1 stays, as in Tunefish); the conversion to an
        // integer is the floor of a positive value, without the library call
        ph[0] += fl;
        ph[0] -= if ph[0] > 1.0 { ph[0] as u32 as f32 } else { 0.0 };
        ph[1] += fr;
        ph[1] -= if ph[1] > 1.0 { ph[1] as u32 as f32 } else { 0.0 };
    }
    (l, r)
}

/// `sinc(x) = sin(πx)/(πx)` for `x` in samples.
#[inline]
fn sinc(x: f64) -> f64 {
    if x > 1e-7 || x < -1e-7 {
        sin_turns(x * 0.5) as f64 / (core::f64::consts::PI * x)
    } else {
        1.0
    }
}

/// One sample of Slaughter's oscillator, in turns instead of samples: the impulse sits at phase ½ and the pulse's
/// second impulse `pw` later, both as a sinc with the period as its lobe length. The leaky integral is the saw
/// (`wf` 0) or the pulse (`wf` 1) and is inverted like Slaughter's output stage (its gain is `−16·…`), so a negative
/// sign here keeps the original polarity when the gain law is positive.
#[inline]
fn blit(slot: &mut Slot, inc: f64, wf: f64, pw: f64, leak: f64) -> f32 {
    let inc = inc.max(1e-9);
    let p2 = slot.phase + pw;
    let p2 = p2 - libm::floor(p2);
    slot.phase += inc;
    slot.phase -= libm::floor(slot.phase);
    let dc = -0.996 * inc; // -0.498 / half period in samples
    slot.integral = leak * slot.integral + dc * (1.0 - wf) + sinc((slot.phase - 0.5) / inc) - sinc((p2 - 0.5) / inc) * wf;
    -slot.integral as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::law::host as inv;

    #[cfg(feature = "us-osc-wt")]
    mod wavetable_unit {
        use super::super::*;

        /// A table that reads back its own index: `wave[i] = i/511`.
        fn ramp() -> Wt {
            let mut w = Wt::new();
            w.table.load(&core::array::from_fn(|i| i as f32 / 511.0));
            w
        }

        #[test]
        fn layers_sum_and_move_at_their_own_rates() {
            let mut w = ramp();
            w.phase = [[0.0; 2]; crate::wt::MAX_LAYERS];
            for (k, ph) in w.phase.iter_mut().enumerate() {
                *ph = [0.1 * k as f32, 0.05 + 0.1 * k as f32];
            }
            let want_l: f32 = (0..3).map(|k| ((0.1 * k as f32 * 511.0) as usize) as f32 / 511.0).sum();
            let want_r: f32 = (0..3).map(|k| (((0.05 + 0.1 * k as f32) * 511.0) as usize) as f32 / 511.0).sum();
            let (l, r) = wavetable(&mut w, 0.01, 0.002, 0.001, 1.0, 3);
            assert!((l - want_l).abs() < 1e-6 && (r - want_r).abs() < 1e-6, "{l} {r} vs {want_l} {want_r}");
            // layer k advances by inc ± det ± k·spread; the right phase gets the opposite signs
            for k in 0..3 {
                let kf = k as f32;
                assert!((w.phase[k][0] - (0.1 * kf + 0.012 + 0.001 * kf)).abs() < 1e-6, "{k}: {}", w.phase[k][0]);
                assert!((w.phase[k][1] - (0.05 + 0.1 * kf + 0.008 - 0.001 * kf)).abs() < 1e-6, "{k}: {}", w.phase[k][1]);
            }
            assert!(w.phase[3][0] == 0.3 && (w.phase[3][1] - 0.35).abs() < 1e-6, "layers past the count stay put");
        }

        #[test]
        fn drive_hard_clips_each_read_and_a_negative_rate_runs_forward() {
            let mut w = Wt::new();
            w.table.load(&core::array::from_fn(|i| if i < 256 { 0.4 } else { -0.4 }));
            w.phase[0] = [0.1, 0.9];
            let (l, r) = wavetable(&mut w, 0.0, 0.0, 0.0, 10.0, 1);
            assert_eq!((l, r), (1.0, -1.0), "0.4 × 10 clips to ±1");
            // inc − det < 0: the right layer runs at |inc − det|, not backwards
            let mut w = ramp();
            w.phase[0] = [0.5, 0.5];
            wavetable(&mut w, 0.01, 0.03, 0.0, 1.0, 1);
            assert!((w.phase[0][0] - 0.54).abs() < 1e-6 && (w.phase[0][1] - 0.52).abs() < 1e-6, "{:?}", w.phase[0]);
        }

        #[test]
        fn a_phase_past_one_wraps() {
            let mut w = ramp();
            w.phase[0] = [0.99, 0.5];
            wavetable(&mut w, 0.05, 0.0, 0.0, 1.0, 1);
            assert!((w.phase[0][0] - 0.04).abs() < 1e-5, "{}", w.phase[0][0]);
        }
    }

    #[test]
    fn sinc_is_the_normalised_sinc() {
        crate::tables::init();
        for x in [0.0, 1e-9, 0.25, 0.5, 1.0, 1.5, 2.0, 7.3, -3.7, 150.25, -199.9] {
            let want = if x == 0.0 { 1.0 } else { (core::f64::consts::PI * x).sin() / (core::f64::consts::PI * x) };
            assert!((sinc(x) - want).abs() < 2e-6, "{x}: {} vs {want}", sinc(x));
        }
    }

    #[test]
    fn kinds_follow_the_type_parameter_and_the_build() {
        let kind = |ty: usize| {
            let mut v = pm::DEFAULTS;
            v[osc(1, o::TYPE)] = inv::inv_choice(ty, osc_type::COUNT);
            Kind::all(&v)
        };
        assert_eq!(Kind::all(&pm::DEFAULTS).unwrap(), [Kind::Off; 3]);
        assert_eq!(kind(osc_type::OFF).unwrap(), [Kind::Off; 3]);
        for (ty, want, on, id) in [
            (osc_type::PM, Kind::Pm, cfg!(feature = "us-osc-pm"), module::OSC_PM),
            (osc_type::BLIT, Kind::Blit, cfg!(feature = "us-osc-blit"), module::OSC_BLIT),
            (osc_type::SAMPLER, Kind::Sampler, cfg!(feature = "us-osc-sample"), module::OSC_SAMPLER),
        ] {
            match kind(ty) {
                Ok(k) => assert!(on && k == [Kind::Off, want, Kind::Off]),
                Err(e) => assert!(!on && e == LoadError::UnsupportedModule(id)),
            }
        }
        #[cfg(feature = "us-osc-wt")]
        assert_eq!(kind(osc_type::WAVETABLE).unwrap(), [Kind::Off, Kind::Wt, Kind::Off]);
        #[cfg(not(feature = "us-osc-wt"))]
        assert_eq!(kind(osc_type::WAVETABLE).err(), Some(LoadError::UnsupportedModule(module::OSC_WAVETABLE)));
    }
}
