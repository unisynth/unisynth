//! Glorp instrument ID 22: a circle oscillator pushed through an iterated 3D trigonometric map, on the shared 16-voice
//! `Synth` scheduler, complete for the poly mode the song uses.
//!
//! Independent behavioural implementation written from observed behaviour (research/glorp-devices.md §ID 22); no
//! original binary, table or pseudocode is included. f32 throughout, operation order follows the reference because the
//! map is chaotic for a state gain above 1 (a different rounding is a different sound). Fixed 44.1 kHz.
//!
//! # Voice
//!
//! One voice per note (the scheduler is shared with ID 25, see [`crate::synth`]). Parameters P0..P4, P9..P14 are
//! re-read **every sample** (automation acts on sounding notes); the envelope P5 attack, P6 decay, P7 sustain, P8
//! release (seconds / level, the shared exponential `Adsr` of ID 21) is **latched at note-on**. Per sample, with `φ` the
//! voice phase (an f32 accumulator that starts at 0, advances by `ω = 0.0626893714 · 2^(note/12 − 5.75)` per sample and
//! is never wrapped, so its resolution decays on long notes):
//!
//! ```text
//! r  = 5·P0                          n = trunc(15·P4) + 1              b = 3·P1          a = 10·P2 + 0.002
//! z0 = 0.2·φ·P3                                   (P11 < 0.5: an unbounded ramp)
//! z0 = 40·P10·tsin(φ·100·P3 / (440·2^(note/12 − 5.75)))   (P11 ≥ 0.5: an LFO of 100·P3 Hz, independent of the note)
//! p  = r · (tsin φ, tsin(φ + π/2), z0)
//! o  = map(p, a, b, n)               (only o.x and o.y are used)
//! s  = (1 − P13)·(o.x − o.y)         L = P13·o.x + s        R = P13·o.y + s
//! g  = 10·P14·env      d = P12 + 0.001       x = ch·g·d       out = P9 · T(x) / T(d)        T(x) = x(x² + 27) / (9x² + 27)
//! ```
//!
//! `tsin` is the 512-node linearly interpolated table sine of the other Glorp devices (`id34::sin_with`). The voice is
//! freed on the sample its envelope reaches stage 0 and, unlike ID 25, **renders nothing after it**.
//!
//! # The map
//!
//! A constant orthonormal frame (`u ∝ (−1, −1.2, 0.2)`, `n ∝ u × (0, 1, 0)`, `v = n × u`; a proper rotation of
//! 169.74°) is applied to the point, then `x += sin z, y += sin x, z += sin y` simultaneously, then the three
//! `(sin, cos)` pairs of the new point feed `d = (sx·cy − sz·cz, sy·cz − sx·cx, cx·sz − sy·cy)`, accumulated with the
//! decay `1/a` per iteration (`out = Σ aᵏ·dₖ / Σ aᵏ`, an exponentially weighted mean; `|d| ≤ 2` so `|out| ≤ 2`), then the
//! point is multiplied by `b`. `n` iterations. The weights are `a^-k` accumulated as `W ← (W + 1)/a`, so a small `a`
//! with a large `n` **overflows**: `a = 0.002` (P2 = 0) gives `500^n`, infinite from `n = 15`, and the result is NaN (or
//! ±inf) in the original.
//!
//! # What this crate refuses
//!
//! * Any parameter not finite or outside `[0, 1]` ([`Error::Range`]).
//! * A vector whose weight sum could overflow ([`Error::Weights`], see [`check`]): the sum is computed exactly as the
//!   voice does and must stay below `f32::MAX / 4` (the numerators are bounded by `2W`). **The whole resulting vector
//!   is judged on every write**, so a multi-parameter move whose intermediate vector overflows is refused: lower the
//!   iteration count (P4) before the weight ratio (P2), or raise P2 first.
//! * mono/legato and unison are not representable (the song chunk has no field for them, the adapter refuses any chunk
//!   longer than 15 parameters + size), other sample rates, notes above 127, offsets above `i32::MAX`.
//!
//! **Standalone poly-only API.** As for ID 25, nothing here can detect a mono/unison description: a loader MUST reject
//! it before constructing this device.
//!
//! Exactness: everything but `2^x` is IEEE-exact (`sqrtf`, `+ − × /`, the table sine, `cvttss2si`). `2^x` is the f64
//! `libm` result rounded to f32; it fixes `ω`, so a last-bit difference from the original's C library would show as a
//! slow phase drift (checked against the whole song, research doc §ID 22). Not a `synth_core::Device`.

use crate::id21::Adsr;
use crate::id25::sine_table;
use crate::id34::{sin_with, TABLE_LEN};
use crate::synth::{ActiveNote, Synth, Voice as SynthVoice};
use synth_core::denormal::Guard;

/// Parameter count of the device.
pub const PARAMS: usize = 15;
pub use crate::synth::{MAX_NOTE, QUEUE, VOICES};

/// Why a call is not supported. The device is left unchanged when one is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parameter index ≥ 15.
    Index,
    /// Not finite or outside `[0, 1]`.
    Range,
    /// The map's weight sum would overflow f32 (small P2 with large P4): the original returns NaN / inf.
    Weights,
    /// Note above 127.
    Note,
    /// Offset above `i32::MAX`.
    Offset,
    /// All 256 queue slots are taken (the original drops the event silently).
    QueueFull,
}

impl From<crate::synth::Error> for Error {
    fn from(e: crate::synth::Error) -> Self {
        match e {
            crate::synth::Error::Note => Error::Note,
            crate::synth::Error::Offset => Error::Offset,
            crate::synth::Error::QueueFull => Error::QueueFull,
        }
    }
}

// f32 literals of the reference (bit patterns matter).
const HALF_PI: f32 = f32::from_bits(0x3fc9_0fdb); // 1.5707964
const TWELFTH: f32 = f32::from_bits(0x3daa_aaab); // 1/12
const OCT_OFFSET: f32 = f32::from_bits(0xc0b8_0000); // −5.75
const OMEGA_A4: f32 = f32::from_bits(0x3d80_6349); // 2π·440/44100
const INV_440: f32 = f32::from_bits(0x3b14_f209); // 1/440
const K_TENTH: f32 = f32::from_bits(0x3e4c_cccd); // 0.2
const K_WEIGHT_FLOOR: f32 = f32::from_bits(0x3b03_126f); // 0.002
const K_DRIVE_FLOOR: f32 = f32::from_bits(0x3a83_126f); // 0.001
const K_SQ: f32 = f32::from_bits(0x401e_b852); // 2.48
const K_UY: f32 = f32::from_bits(0xbf99_999a); // −1.2

fn pow2(x: f32) -> f32 {
    libm::exp2(x as f64) as f32
}

/// Iteration count `n = trunc(15·P4) + 1` (1..=16 for P4 in `[0, 1]`).
pub(crate) fn iterations(p4: f32) -> usize {
    ((p4 * 15.0) as i32 + 1) as usize
}

/// The weight ratio `a = 10·P2 + 0.002`.
pub(crate) fn weight_ratio(p2: f32) -> f32 {
    p2 * 10.0 + K_WEIGHT_FLOOR
}

/// The map's weight sum `W`, accumulated exactly as the voice does (`W ← (W + 1)/a`, `n` times).
fn weight_sum(a: f32, n: i32) -> f32 {
    let ia = 1.0 / a;
    let mut w = 0.0f32;
    for _ in 0..n {
        w = (w + 1.0) * ia;
    }
    w
}

/// Whether the weight sum of this `(P2, P4)` leaves headroom: `W ≤ f32::MAX / 4`, so that the numerators (`|A| ≤ 2W`
/// plus rounding) and the reciprocal stay finite. Monotone: larger for smaller P2 and larger P4.
fn weights_ok(p2: f32, p4: f32) -> bool {
    weight_sum(weight_ratio(p2), iterations(p4) as i32) <= f32::MAX / 4.0
}

/// Whether `new` accepts these parameters (cheap: builds nothing). `Err` leaves nothing to undo.
pub fn check(p: &[f32; PARAMS]) -> Result<(), Error> {
    if !p.iter().all(|v| (0.0..=1.0).contains(v)) {
        return Err(Error::Range);
    }
    if !weights_ok(p[2], p[4]) {
        return Err(Error::Weights);
    }
    Ok(())
}

/// The constant rotation `p ← M·p` the map applies at the start of every iteration.
#[derive(Clone, Copy, Debug)]
struct Frame {
    n: [f32; 3],
    v: [f32; 3],
    u: [f32; 3],
}

impl Frame {
    fn new() -> Self {
        let s = libm::sqrtf(K_SQ);
        let inv = 1.0 / s;
        let (ux, uy, uz) = (-1.0 * inv, K_UY * inv, K_TENTH * inv);
        // w = u × (0, 1, 0) = (−uz, 0, ux)
        let (wx, wy, wz) = (-uz, 0.0f32, ux);
        let l = libm::sqrtf(wz * wz + (wy * wy + wx * wx));
        let il = 1.0 / l;
        let (nx, ny, nz) = (wx * il, il * wy, il * wz);
        let vx = ny * uz - nz * uy;
        let vz = uy * nx - ny * ux;
        let vy = nz * ux - nx * uz;
        Frame { n: [nx, ny, nz], v: [vx, vy, vz], u: [ux, uy, uz] }
    }

    /// Rows of the matrix (`x'`, `y'`, `z'` as linear forms of `x, y, z`).
    #[cfg(test)]
    fn rows(&self) -> [[f32; 3]; 3] {
        [[self.n[2], self.v[2], self.u[2]], [self.n[1], self.v[1], self.u[1]], [self.n[0], self.v[0], self.u[0]]]
    }
}

#[cfg(test)]
pub(crate) fn frame() -> [[f32; 3]; 3] {
    Frame::new().rows()
}

/// The 3D map: `n` iterations of rotate, coupled sine step, trig vector, exponential weighting `1/a`, gain `b`.
/// Returns `(o.x, o.y, o.z)` (`o.z` is unused by the voice). `n ≤ 0` returns NaN (as the original: 0 · ∞).
fn map_with(f: &Frame, table: &[f32; TABLE_LEN], p: [f32; 3], a: f32, b: f32, n: i32) -> [f32; 3] {
    let s = |x: f32| sin_with(table, x);
    let c = |x: f32| sin_with(table, x + HALF_PI);
    let ia = 1.0 / a;
    let [mut x, mut y, mut z] = p;
    let (mut a0, mut a1, mut cc, mut w) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let (nv, vv, uv) = (f.n, f.v, f.u);
    for _ in 0..n.max(0) {
        let pz = (x * nv[0] + y * vv[0]) + z * uv[0];
        let px = (y * vv[2] + x * nv[2]) + z * uv[2];
        let py = (y * vv[1] + x * nv[1]) + z * uv[1];
        // simultaneous: x += sin z, y += sin x, z += sin y (of the rotated point)
        let (sz0, sx0, sy0) = (s(pz), s(px), s(py));
        x = px + sz0;
        y = py + sx0;
        z = sy0 + pz;
        let (sx, sy, sz) = (s(x), s(y), s(z));
        let (cz, cy, cx) = (c(z), c(y), c(x));
        a0 = ((a0 - sz * cz) + sx * cy) * ia;
        a1 = ((a1 + sy * cz) - sx * cx) * ia;
        cc = ((cc - sy * cy) + cx * sz) * ia;
        w = (w + 1.0) * ia;
        x *= b;
        y *= b;
        z *= b;
    }
    let rw = 1.0 / w;
    [rw * a0, rw * a1, cc * rw]
}

#[cfg(test)]
pub(crate) fn map(p: [f32; 3], a: f32, b: f32, n: i32) -> [f32; 3] {
    map_with(&Frame::new(), &sine_table(), p, a, b, n)
}

/// `T(d) = d(d² + 27) / (9d² + 27)`: the normaliser of the output shaper for the drive `d`.
pub(crate) fn shaper_norm(d: f32) -> f32 {
    let d2 = d * d;
    ((d2 + 27.0) * d) / ((d2 * 9.0) + 27.0)
}

/// One output channel `p9 · T(x) / T(d)` in the reference's operation order (`td` = [`shaper_norm`]`(d)`).
pub(crate) fn shaped(x: f32, p9: f32, td: f32) -> f32 {
    let x2 = x * x;
    ((x * p9) * (x2 + 27.0)) / (((x2 * 9.0) + 27.0) * td)
}

#[derive(Clone, Copy, Debug)]
struct Voice {
    active: bool,
    note: u8,
    /// Voice phase (f32 accumulator, never wrapped).
    phi: f32,
    /// `ω` and `1 / 2^(note/12 − 5.75)` of the note.
    omega: f32,
    inv_pow: f32,
    adsr: Adsr,
}

impl Voice {
    fn idle() -> Self {
        Voice { active: false, note: 0, phi: 0.0, omega: 0.0, inv_pow: 0.0, adsr: Adsr::idle() }
    }

    /// Note-on: phase 0, envelope times latched (the stored level is kept), note-derived rates.
    fn note_start(&mut self, note: u8, p: &[f32; PARAMS]) {
        let pw = pow2((note as f32) * TWELFTH + OCT_OFFSET);
        self.omega = OMEGA_A4 * pw;
        self.inv_pow = 1.0 / pw;
        self.phi = 0.0;
        self.adsr.restart(p[5], p[6], p[7], p[8]);
        self.active = true;
        self.note = note;
    }

    /// Renders `l.len()` samples, adding into `l` / `r`. Stops at the sample the envelope finishes on.
    fn render_samples(&mut self, sh: &Shared<'_>, l: &mut [f32], r: &mut [f32]) {
        let (p, table, frame) = (sh.p, sh.table, sh.frame);
        for i in 0..l.len() {
            let env = self.adsr.tick();
            let n = (p[4] * 15.0) as i32 + 1;
            let b = p[1] * 3.0;
            let a = p[2] * 10.0 + K_WEIGHT_FLOOR;
            let phi = self.phi;
            let z0 = if 0.5 > p[11] {
                (phi * K_TENTH) * p[3]
            } else {
                let arg = (((phi * 100.0) * p[3]) * INV_440) * self.inv_pow;
                (sin_with(table, arg) * 40.0) * p[10]
            };
            let cosv = sin_with(table, phi + HALF_PI);
            let sinv = sin_with(table, phi);
            let rad = p[0] * 5.0;
            let z = z0 * rad;
            let o = map_with(frame, table, [sinv * rad, cosv * rad, z], a, b, n);
            let w = p[13];
            let sd = (1.0 - w) * (o[0] - o[1]);
            let ch_r = (o[1] * w) + sd;
            let ch_l = (w * o[0]) + sd;
            let g = (p[14] * 10.0) * env;
            let d = p[12] + K_DRIVE_FLOOR;
            let td = shaper_norm(d);
            let x_r = (ch_r * g) * d;
            let x_l = (g * ch_l) * d;
            let out_l = shaped(x_l, p[9], td);
            let out_r = shaped(x_r, p[9], td);
            self.phi = phi + self.omega;
            l[i] = out_l + l[i];
            r[i] = out_r + r[i];
            if self.adsr.stage == 0 {
                self.active = false;
                break;
            }
        }
    }
}

/// Read-only state the voices share.
struct Shared<'a> {
    p: &'a [f32; PARAMS],
    table: &'a [f32; TABLE_LEN],
    frame: &'a Frame,
}

impl SynthVoice<Shared<'_>> for Voice {
    fn active(&self) -> bool {
        self.active
    }
    fn note(&self) -> u8 {
        self.note
    }
    fn start(&mut self, note: u8, c: &Shared<'_>) {
        self.note_start(note, c.p);
    }
    fn release(&mut self) {
        self.adsr.release();
    }
    fn render(&mut self, c: &Shared<'_>, l: &mut [f32], r: &mut [f32]) {
        self.render_samples(c, l, r);
    }
}

impl ActiveNote for Voice {
    fn is_active(&self) -> bool {
        self.active
    }
    fn held_note(&self) -> u8 {
        self.note
    }
}

/// The ID 22 instrument: 16 voices on the shared scheduler, 15 parameters.
#[derive(Clone, Debug)]
pub struct Id22 {
    p: [f32; PARAMS],
    table: [f32; TABLE_LEN],
    frame: Frame,
    synth: Synth<Voice>,
}

impl Id22 {
    /// Builds the device from the 15 normalized parameters (a song chunk's first 15 floats). No voice is playing.
    pub fn new(p: &[f32; PARAMS]) -> Result<Self, Error> {
        check(p)?;
        Ok(Id22 { p: *p, table: sine_table(), frame: Frame::new(), synth: Synth::new([Voice::idle(); VOICES]) })
    }

    /// Sets one parameter between `process` calls (sounding voices use the new value from the next sample, except the
    /// envelope P5..P8, which a note latches at its note-on). The **whole resulting vector** is judged ([`check`]): on
    /// `Err` nothing changed.
    pub fn set_param(&mut self, index: usize, value: f32) -> Result<(), Error> {
        if index >= PARAMS {
            return Err(Error::Index);
        }
        let mut next = self.p;
        next[index] = value;
        check(&next)?;
        self.p = next;
        Ok(())
    }

    /// Queues a note-on `offset` samples after the start of the next `process` call (velocity does not exist for
    /// this instrument). With every voice busy at the time it fires, it is lost.
    pub fn note_on(&mut self, note: u8, offset: u32) -> Result<(), Error> {
        self.synth.note_on(note, offset).map_err(Error::from)
    }

    /// Queues a note-off (releases every voice currently playing `note`).
    pub fn note_off(&mut self, note: u8, offset: u32) -> Result<(), Error> {
        self.synth.note_off(note, offset).map_err(Error::from)
    }

    /// Voices currently holding a slot (playing or in release).
    pub fn active_voices(&self) -> usize {
        self.synth.active_voices()
    }

    /// Note held by each voice slot (`None` = free).
    pub fn voice_notes(&self) -> [Option<u8>; VOICES] {
        self.synth.voice_notes()
    }

    /// Renders a stereo block: the buffers are **overwritten** (an instrument, not an effect). `l` and `r` must have
    /// the same length (panics otherwise). Events whose offset lies inside the block fire at that sample, later ones
    /// stay queued for the following calls.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let _ftz = Guard::new();
        let sh = Shared { p: &self.p, table: &self.table, frame: &self.frame };
        self.synth.process(&sh, l, r);
    }
}
