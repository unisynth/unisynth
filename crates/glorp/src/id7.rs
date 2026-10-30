//! Glorp effect ID 7: a stereo look-ahead compressor / limiter (peak envelope follower with linear attack and release,
//! hard-knee gain computer, make-up gain, optional external side-chain). Independent behavioural implementation written
//! from observed behaviour (research/glorp-devices.md §ID 7); no original binary or pseudocode is included. Settings
//! outside what is supported are **rejected** with an [`Error`], never approximated:
//!
//! | param | meaning | law (u = value) |
//! |---|---|---|
//! | 0 | detector source | `u ≥ 0.5`: the side-chain pair (the aux input, see [`Id7::process_sidechain`]); else the signal itself |
//! | 1 | input gain | `2^(g/6)`, `g = (2u − 1)·12` (≈ ±12 dB, 6 per octave of amplitude) |
//! | 2 | threshold | `2^(t/6)`, `t = (2·(u·0.5) − 1)·36 = (u − 1)·36` (linear, on the gained detector value) |
//! | 3 | ratio | `2 + 18·u²` (2..20) |
//! | 4 | attack time | `T = (5000·u² + 1)·0.2` ms-like units; the envelope rises by `(1000/44100)/T` per sample |
//! | 5 | release time | `T = 5000·u² + 1`; the envelope falls by `(1000/44100)/T` per sample |
//! | 6 | make-up gain | `2^(g/6)`, `g = (2u − 1)·12` |
//!
//! Every value must be finite and within `[0, 1]` (song bytes/255 are); index ≥ 7 is rejected (the original ignores it
//! silently). Fixed 44.1 kHz. All seven parameters are modelled; nothing in range is unsupported. The only refusals are
//! the call-form mismatch of [`Error::Sidechain`]: [`Id7::process`] while P0 selects the side-chain, or
//! [`Id7::process_sidechain`] while it does not (the buffers and state are left untouched).
//!
//! Per sample (f32, scalar order of the reference, no FMA): both inputs times the input gain are written to two
//! 88-sample rings (the output is the input **87 samples later**: write, advance, read the slot about to be overwritten);
//! the detector `d = max(|L·g|, |R·g|)` of the selected pair (`R` wins only when strictly larger); the envelope `e`
//! moves linearly toward `d`: `d > e` (strict) rises by the attack rate (clamped to `d` when it would pass it), else falls
//! by the release rate (clamped to `d` when it would pass it **or when either is NaN**: a NaN detector value becomes the
//! envelope for one sample, then the next finite detector value replaces it; an infinite one is a plain clamp); the gain
//! is the make-up `m`, or when `e > thr` (strict; NaN gives `m`) `(((e − thr)/ratio + thr)·m)/e`; the output is the delayed, input-gained sample times that gain. The envelope
//! and the rings are never reset (parameter changes, P0 toggles included, keep them). Parameters are re-read at the start
//! of each `process*` call (the gain terms are recomputed per call, exactly as the reference), so a change takes effect
//! on the next call and block-split processing is bit-identical. In place; the side-chain buffers are only read.
//!
//! Numerics: the original links the C library's `powf`, whose last-ulp results are neither correctly rounded nor
//! `libm`'s. Here `2^e` is the f64 result rounded to f32; output is therefore tolerance-matched, not guaranteed
//! bit-exact (`tests/reference7.rs`; measured bounds in the research doc). Not a `synth_core::Device`: its `set_param`
//! cannot report a rejection.

use synth_core::denormal::Guard;

/// Parameter count of the device.
pub const PARAMS: usize = 7;
/// Look-ahead ring length: the signal is delayed by `RING - 1` = 87 samples.
const RING: usize = 88;

/// Why a setting or call is not supported. The device is left unchanged when one is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parameter index ≥ 7.
    Index,
    /// Not finite or outside `[0, 1]`.
    Range,
    /// Wrong `process` form for P0: `process` while the side-chain is selected, `process_sidechain` while it is not.
    Sidechain,
}

/// `(1000 / 44100)` rounded to f32 (the reference divides in f64 and narrows once).
const MS_PER_SAMPLE: f32 = (1000.0f64 / 44100.0f64) as f32;
/// The reference's f32 `1/6` constant (bits matter).
const SIXTH: f32 = 0.166_666_67;

#[derive(Clone, Debug)]
pub struct Id7 {
    side: bool,
    /// `(2P1−1)·12`
    in_db: f32,
    /// `(2·(P2·0.5)−1)·36`
    thr_db: f32,
    ratio: f32,
    attack: f32,
    release: f32,
    makeup_db: f32,
    env: f32,
    pos: usize,
    ring: [[f32; RING]; 2],
}

fn validate(v: f32) -> Result<(), Error> {
    if (0.0..=1.0).contains(&v) { Ok(()) } else { Err(Error::Range) }
}

/// `(2u + −1)·scale` in the reference's f32 order.
fn centred(u: f32, scale: f32) -> f32 {
    (u + u + -1.0) * scale
}

/// `2^(x/6)` as the f64 `pow` rounded to f32 (`x · (1/6)` is the f32 product the reference forms first).
fn pow2_sixth(x: f32) -> f32 {
    libm::pow(2.0, (x * SIXTH) as f64) as f32
}

fn abs(x: f32) -> f32 {
    f32::from_bits(x.to_bits() & 0x7fff_ffff)
}

/// Whether `new` accepts these parameters (cheap: builds nothing). `Err` leaves nothing to undo.
pub fn check(p: &[f32; PARAMS]) -> Result<(), Error> {
    p.iter().try_for_each(|&v| validate(v))
}

impl Id7 {
    /// Builds the device from the 7 normalized parameters (a song chunk's first 7 floats).
    pub fn new(p: &[f32; PARAMS]) -> Result<Self, Error> {
        check(p)?;
        let mut d = Id7 {
            side: false,
            in_db: 0.0,
            thr_db: 0.0,
            ratio: 2.0,
            attack: 1.0,
            release: 1.0,
            makeup_db: 0.0,
            env: 0.0,
            pos: 0,
            ring: [[0.0; RING]; 2],
        };
        for (i, &v) in p.iter().enumerate() {
            d.apply(i, v);
        }
        Ok(d)
    }

    /// Sets one parameter between `process` calls (envelope and delay state are kept). On `Err` nothing changed.
    pub fn set_param(&mut self, index: usize, value: f32) -> Result<(), Error> {
        if index >= PARAMS {
            return Err(Error::Index);
        }
        validate(value)?;
        self.apply(index, value);
        Ok(())
    }

    fn apply(&mut self, i: usize, v: f32) {
        match i {
            0 => self.side = v >= 0.5,
            1 => self.in_db = centred(v, 12.0),
            2 => self.thr_db = centred(v * 0.5, 36.0),
            3 => self.ratio = v * v * 18.0 + 2.0,
            4 => self.attack = (v * v * 5000.0 + 1.0) * 0.2,
            5 => self.release = v * v * 5000.0 + 1.0,
            _ => self.makeup_db = centred(v, 12.0),
        }
    }

    /// `true` while P0 selects the side-chain (aux) pair as detector input.
    pub fn sidechain(&self) -> bool {
        self.side
    }

    /// Processes `l`/`r` in place with the signal itself as detector input. `Err(Sidechain)` (nothing touched) when P0
    /// selects the side-chain: use [`Id7::process_sidechain`]. Panics when the lengths differ.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) -> Result<(), Error> {
        assert_eq!(l.len(), r.len(), "equal length required");
        if self.side {
            return Err(Error::Sidechain);
        }
        self.run(l, r, None);
        Ok(())
    }

    /// Like [`Id7::process`] but the detector listens to `sl`/`sr` (the aux pair, read only). `Err(Sidechain)` (nothing
    /// touched) when P0 does not select the side-chain. Panics when any of the four lengths differ.
    pub fn process_sidechain(&mut self, l: &mut [f32], r: &mut [f32], sl: &[f32], sr: &[f32]) -> Result<(), Error> {
        assert!(l.len() == r.len() && l.len() == sl.len() && l.len() == sr.len(), "equal length required");
        if !self.side {
            return Err(Error::Sidechain);
        }
        self.run(l, r, Some((sl, sr)));
        Ok(())
    }

    fn run(&mut self, l: &mut [f32], r: &mut [f32], side: Option<(&[f32], &[f32])>) {
        let _ftz = Guard::new();
        let gin = pow2_sixth(self.in_db);
        let mk = pow2_sixth(self.makeup_db);
        let att = MS_PER_SAMPLE / self.attack;
        let rel = MS_PER_SAMPLE / self.release;
        let thr = pow2_sixth(self.thr_db);
        for i in 0..l.len() {
            let (xl, xr) = (l[i], r[i]);
            self.ring[0][self.pos] = xl * gin;
            self.ring[1][self.pos] = xr * gin;
            self.pos = (self.pos + 1) % RING;
            let (dl, dr) = match side {
                Some((sl, sr)) => (sl[i], sr[i]),
                None => (xl, xr),
            };
            let (a, b) = (abs(dl * gin), abs(dr * gin));
            let det = if b > a { b } else { a };
            let mut e = self.env;
            if det > e {
                e += att;
                if e > det {
                    e = det;
                }
            } else {
                e -= rel;
                // `jb` after `ucomiss`: taken on e < det **and** when unordered (a NaN detector value becomes the envelope)
                if !(e >= det) {
                    e = det;
                }
            }
            self.env = e;
            let g = if e > thr { (((e - thr) / self.ratio + thr) * mk) / e } else { mk };
            l[i] = self.ring[0][self.pos] * g;
            r[i] = self.ring[1][self.pos] * g;
        }
    }
}
