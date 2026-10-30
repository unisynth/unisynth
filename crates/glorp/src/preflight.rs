//! Song-level preflight for the Glorp device set (kinds 3, 7, 14, 20, 21, 22, 25, 27, 28, 30, 32, 34, 35, 38): decides, from the song alone,
//! whether every device and every automation lane can be played by the devices in this crate, and names the first
//! thing that cannot.
//!
//! Why it exists: `synth_core::Device::set_param` cannot report a refusal, while these devices refuse some settings
//! (ID 38 filter mode, ID 35 topology, ID 32 delay length, …) instead of guessing. A lane that interpolates through
//! such a setting would be applied by the player as a silent no-op. So the whole song is judged **before** anything is
//! built ([`synth_core::mix::Kinds::preflight`], called by `Player::new` through `load_devices`):
//!
//! - rate must be 44.1 kHz (every device is fixed-rate); the block size (128) is enforced by `Kinds::block` (per song: only when it holds an owned device);
//! - every kind must be one of the fourteen above (every other id, known or foreign, is refused, not approximated);
//! - every chunk must be exactly `n` parameters + a trailing byte-length field equal to the chunk length. The song
//!   format has **no** place for a mono/legato voice mode or a voices-per-note count (a longer chunk would be the only
//!   way to carry one), so such a description cannot be represented and is refused by the length check;
//! - every initial parameter must be a setting the device supports;
//! - every automation lane is judged as a **box**: per device and parameter the range of everything the player can
//!   ever write (the chunk value, every lane point, widened by [`MARGIN`] for the interpolation's rounding). Every
//!   vector inside the box must be supported, whatever the order of the writes or the interpolation between points.
//!   For devices whose rejected settings are a product of per-parameter intervals (ID 35 topology, ID 38 mode / stages
//!   / distortion / envelope) each automated parameter is probed at both ends of its range through the device's own
//!   `check`. ID 21 bounds the partial count `(trunc(100·P4)+1)·(trunc(100·P6)+1)` by its fixed voice memory, so its box
//!   is judged on the corner with P4 and P6 at their tops. ID 32 couples P2, P4 and P7 (delay length against the buffer, mode switch with its glide), so its box
//!   is judged on the corner with the longest delay and a lane may not cross the mode boundary. ID 22 bounds the 3D map's
//!   weight sum `Σ (10·P2 + 0.002)^-k` (it overflows to NaN / inf for a small P2 with a large P4), which grows as P2 falls
//!   and P4 rises, so its box is judged on the corner with P2 at its bottom and P4 at its top. IDs 3, 7, 14, 25, 27, 28 and 34
//!   support the whole `[0, 1]` range of every parameter (ID 14's mono switch (P23) is a legal lane: a change cuts the voices as the original does; ID 28's ladder gain stays below 4 and its oscillators are
//!   bounded for every vector, so every lane, written in any order and interpolation, is safe).
//! - note lanes need no judging: a lane event is a note-on (byte 0..=127 + velocity) or a note-off (bit 7 set, the
//!   note is the low 7 bits), so the song format cannot carry a note above 127 (the only note the devices refuse) and
//!   the velocity is ignored, **except by ID 28** (which scales its oscillator by `velocity/127`: every velocity of 0..=255
//!   is finite, nothing to refuse) **and ID 20**, whose noise envelope grows without bound from velocity 214 on (with
//!   P13 = 1): a track that carries an ID 20 is refused when its lane holds a note-on above [`id20::MAX_VELOCITY`]
//!   (the song format carries up to 255), whatever the parameters or lanes. The only other refusal a note can meet, a full 256-slot queue, is the original's silent
//!   drop, not a fault.
//!
//! Two doors: [`preflight`] (standalone, strict: every device must be a kind above) and [`preflight_owned`] (the
//! `Kinds::preflight` hook, which judges only the owned kinds so it can sit in an `Either`; the unowned ones are then
//! refused by `Kinds::need` when this registry is alone). `Either` dispatches by "first registry that does not report
//! `UnsupportedKind`", so a registry whose kind numbers overlap a registry before it never sees them: mixing
//! overlapping ids (Glorp 3/7 with WaveSabre 0..=12, Glorp 32/34/35/38 with Tunefish 32..=39) is **not supported**;
//! only disjoint kind sets mix.

use crate::{id14, id20, id21, id22, id25, id27, id28, id3, id30, id32, id34, id35, id38, id7};
use synth_core::mix::{Env, LoadError, Song};

/// Sample rate of the song and of the render context.
pub const RATE: i32 = 44_100;
/// Frames of every device call (the block the reference engine renders and applies automation at).
pub const BLOCK: usize = 128;
/// Largest parameter count of the supported kinds (ID 28: 26).
pub const MAX_PARAMS: usize = 26;
/// Devices one song may have: a device is up to ~300 KB of pool (ID 32 / ID 27), so an untrusted song's device count
/// would otherwise decide the allocation. The reference song has 39.
pub const MAX_DEVICES: usize = 256;
/// Distinct automated devices one song may have (the preflight keeps one range box per device on the stack).
pub const MAX_AUTOMATED: usize = 64;
/// Distance added to both ends of a lane's range before it is judged: the player's interpolation can land a few ulp
/// outside the points it joins; the lane values are multiples of 1/255, far from this.
pub const MARGIN: f32 = 1e-5;

/// Parameter count of a supported kind.
pub const fn param_count(kind: u8) -> Option<usize> {
    match kind {
        3 => Some(id3::PARAMS),
        7 => Some(id7::PARAMS),
        14 => Some(id14::PARAMS),
        20 => Some(id20::PARAMS),
        21 => Some(id21::PARAMS),
        22 => Some(id22::PARAMS),
        25 => Some(id25::PARAMS),
        27 => Some(id27::PARAMS),
        28 => Some(id28::PARAMS),
        30 => Some(id30::PARAMS),
        32 => Some(id32::PARAMS),
        34 => Some(id34::PARAMS),
        35 => Some(id35::PARAMS),
        38 => Some(id38::PARAMS),
        _ => None,
    }
}

/// Why a song (or one device / lane of it) is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Song or context rate is not 44.1 kHz.
    Rate,
    /// Not one of the supported device kinds (includes the known but unimplemented Glorp kinds).
    UnsupportedKind,
    /// Chunk is not `4 + 4n` bytes.
    ChunkLength,
    /// The chunk's trailing size field is not its length.
    ChunkSize,
    /// A parameter is not finite or outside `[0, 1]`.
    Range,
    /// A parameter combination the device does not model (what: see the text).
    Setting(&'static str),
    /// An automation lane targets a device index the track does not have.
    LaneDevice,
    /// An automation lane targets a parameter the device does not have.
    LaneParam,
    /// An automation lane can drive the device into a setting it does not model (what: see the text).
    Automation(&'static str),
    /// More than [`MAX_AUTOMATED`] distinct devices carry automation.
    TooManyAutomated,
    /// More than [`MAX_DEVICES`] devices.
    TooManyDevices,
    /// The song or the pool failed to load (parse error, out of memory).
    Load(LoadError),
}

/// The first thing that makes a song unplayable: song-level device index, kind and parameter when they apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reject {
    pub device: Option<usize>,
    pub kind: Option<u8>,
    pub param: Option<u8>,
    pub reason: Reason,
}

impl Reject {
    pub(crate) fn song(reason: Reason) -> Self {
        Reject { device: None, kind: None, param: None, reason }
    }

    pub(crate) fn at(device: usize, kind: u8, param: Option<u8>, reason: Reason) -> Self {
        Reject { device: Some(device), kind: Some(kind), param, reason }
    }

    /// The coarse `LoadError` the core `Kinds` hooks can carry.
    pub fn load_error(&self) -> LoadError {
        let kind = self.kind.unwrap_or(0);
        match self.reason {
            Reason::UnsupportedKind => LoadError::UnsupportedKind(kind),
            Reason::ChunkLength | Reason::ChunkSize | Reason::Range => LoadError::BadChunk,
            Reason::LaneDevice | Reason::LaneParam => LoadError::BadIndex,
            Reason::Load(e) => e,
            Reason::Rate
            | Reason::Setting(_)
            | Reason::Automation(_)
            | Reason::TooManyAutomated
            | Reason::TooManyDevices => {
                LoadError::UnsupportedModule(kind)
            }
        }
    }
}

/// Text for a refusal (also the device fault text).
pub fn reason_name(r: Reason) -> &'static str {
    match r {
        Reason::Rate => "sample rate is not 44100",
        Reason::UnsupportedKind => "unsupported device kind",
        Reason::ChunkLength => "chunk length is not 4 + 4 * parameters",
        Reason::ChunkSize => "chunk size field is not the chunk length",
        Reason::Range => "parameter not finite or outside [0, 1]",
        Reason::Setting(s) | Reason::Automation(s) => s,
        Reason::LaneDevice => "automation device index outside the track",
        Reason::LaneParam => "automation parameter index outside the device",
        Reason::TooManyAutomated => "too many automated devices",
        Reason::TooManyDevices => "too many devices",
        Reason::Load(_) => "song did not load",
    }
}

pub type Params = [f32; MAX_PARAMS];

fn head<const N: usize>(p: &Params) -> &[f32; N] {
    p[..N].first_chunk::<N>().unwrap_or_else(|| unreachable!())
}

/// Judges one whole parameter vector (first `param_count(kind)` entries): range first, then the device's own `check`.
/// `Err((param, reason))`.
pub fn check_params(kind: u8, p: &Params) -> Result<(), (Option<u8>, Reason)> {
    let n = param_count(kind).ok_or((None, Reason::UnsupportedKind))?;
    if let Some(i) = p[..n].iter().position(|v| !(0.0..=1.0).contains(v)) {
        return Err((Some(i as u8), Reason::Range));
    }
    match kind {
        3 => id3::check(head(p)).map_err(|_| (None, Reason::Range)),
        7 => id7::check(head(p)).map_err(|_| (None, Reason::Range)),
        14 => id14::check(head(p)).map_err(|_| (None, Reason::Range)),
        20 => id20::check(head(p)).map_err(|_| (None, Reason::Range)),
        21 => id21::check(head(p)).map_err(|e| match e {
            id21::Error::Partials => (None, Reason::Setting("more than 512 partials per voice")),
            _ => (None, Reason::Range),
        }),
        22 => id22::check(head(p)).map_err(|e| match e {
            id22::Error::Weights => (Some(2), Reason::Setting("3D map weight sum overflows (small P2 with large P4)")),
            _ => (None, Reason::Range),
        }),
        25 => id25::check(head(p)).map_err(|_| (None, Reason::Range)),
        27 => id27::check(head(p)).map_err(|_| (None, Reason::Range)),
        28 => id28::check(head(p)).map_err(|_| (None, Reason::Range)),
        30 => id30::check(head(p)).map_err(|e| match e {
            id30::Error::Topology => (Some(20), Reason::Setting("all-pass cascade on with a topology other than delay-line")),
            _ => (None, Reason::Range),
        }),
        34 => id34::check(head(p)).map_err(|_| (None, Reason::Range)),
        32 => id32::check(head(p)).map_err(|e| match e {
            id32::Error::Delay => (None, Reason::Setting("mode A delay exceeds the delay line")),
            _ => (None, Reason::Range),
        }),
        35 => id35::check(head(p)).map_err(|e| match e {
            id35::Error::Topology => (Some(5), Reason::Setting("all-pass topology other than second-order")),
            _ => (None, Reason::Range),
        }),
        _ => id38::check(head(p)).map_err(|e| match e {
            id38::Error::Mode => (Some(0), Reason::Setting("filter mode other than low-pass / high-pass")),
            id38::Error::Stages => (Some(4), Reason::Setting("more than one filter stage")),
            id38::Error::PreDistortion => (Some(5), Reason::Setting("pre distortion")),
            id38::Error::PostDistortion => (Some(6), Reason::Setting("post distortion type above 2")),
            id38::Error::Envelope => (Some(12), Reason::Setting("envelope amount other than neutral")),
            _ => (None, Reason::Range),
        }),
    }
}

/// Reads a device chunk: `4 + 4n` bytes, `n` little-endian f32 then the chunk's own length as u32. Returns the
/// parameters (entries past `n` are 0) after the settings check.
pub fn chunk_params(kind: u8, chunk: &[u8]) -> Result<Params, (Option<u8>, Reason)> {
    let n = param_count(kind).ok_or((None, Reason::UnsupportedKind))?;
    if chunk.len() != 4 + 4 * n {
        return Err((None, Reason::ChunkLength));
    }
    let word = |i: usize| -> [u8; 4] { [chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]] };
    if u32::from_le_bytes(word(n)) as usize != chunk.len() {
        return Err((None, Reason::ChunkSize));
    }
    let mut p = [0.0; MAX_PARAMS];
    for (i, v) in p[..n].iter_mut().enumerate() {
        *v = f32::from_le_bytes(word(i));
    }
    check_params(kind, &p)?;
    Ok(p)
}

/// Range box of what the player can write to one device.
struct Slot {
    device: usize,
    kind: u8,
    init: Params,
    lo: Params,
    hi: Params,
    /// Bit `i`: some lane writes parameter `i`.
    mask: u32,
}

fn widen(lo: f32, hi: f32) -> (f32, f32) {
    ((lo - MARGIN).max(0.0), (hi + MARGIN).min(1.0))
}

/// Judges the box of one device (see the module doc). `Err((param, what))`.
fn judge(s: &Slot) -> Result<(), (Option<u8>, &'static str)> {
    let what = |r: (Option<u8>, Reason)| (r.0, reason_name(r.1));
    match s.kind {
        35 | 38 => {
            for i in (0..MAX_PARAMS).filter(|i| s.mask >> i & 1 == 1) {
                let (lo, hi) = widen(s.lo[i], s.hi[i]);
                for v in [lo, hi] {
                    let mut p = s.init;
                    p[i] = v;
                    check_params(s.kind, &p).map_err(|r| (Some(i as u8), reason_name(r.1)))?;
                }
            }
            Ok(())
        }
        32 => {
            let (m7, h7) = (s.lo[7], s.hi[7]);
            let a = h7 + MARGIN < 0.5;
            let b = m7 - MARGIN >= 0.5;
            if !a && !b {
                return Err((Some(7), "lane crosses the mode A / B boundary"));
            }
            if a {
                // mode A: the delay grows with P2 and P4, so the corner with both at their top is the worst vector
                let mut p = s.init;
                p[2] = widen(s.lo[2], s.hi[2]).1;
                p[4] = widen(s.lo[4], s.hi[4]).1;
                p[7] = s.hi[7];
                check_params(32, &p).map_err(what)?;
            }
            Ok(())
        }
        21 => {
            // the partial count grows with P4 and with P6, so the corner with both at their top is the worst vector
            let mut p = s.init;
            p[4] = widen(s.lo[4], s.hi[4]).1;
            p[6] = widen(s.lo[6], s.hi[6]).1;
            check_params(21, &p).map_err(what)
        }
        30 => {
            // the cascade is audible when P16 > 0.001 and only the delay-line topology (P20 < 1/2.999) is modelled; both
            // conditions are monotone (P16 up, P20 up), so the corner with both at their top is the worst vector
            let mut p = s.init;
            for i in [16, 20] {
                if s.mask >> i & 1 == 1 {
                    p[i] = widen(s.lo[i], s.hi[i]).1;
                }
            }
            check_params(30, &p).map_err(what)
        }
        22 => {
            // the weight sum grows as P2 falls and as P4 rises, so the corner with P2 at its bottom and P4 at its top is
            // the worst vector (monotone in both, so every vector inside the box is then safe)
            let mut p = s.init;
            p[2] = widen(s.lo[2], s.hi[2]).0;
            p[4] = widen(s.lo[4], s.hi[4]).1;
            check_params(22, &p).map_err(what)
        }
        _ => Ok(()),
    }
}

/// Judges a whole song: see the module doc. `Ok` means every device builds from its chunk and no automation lane can
/// drive a device into a setting it does not model. A device of a kind this crate does not implement is refused
/// ([`Reason::UnsupportedKind`], with its song index): this is the standalone door ([`crate::player::GlorpPlayer`]).
pub fn preflight(song: &Song<'_>, env: &Env<'_>) -> Result<(), Reject> {
    judge_song(song, env, true)
}

/// The same judgement restricted to what this crate owns, for [`synth_core::mix::Kinds::preflight`] (whose contract is
/// to judge only the registry's own kinds, so a facade such as `Either` can hand the rest to another registry): devices
/// of unowned kinds and lanes on them are skipped, the rate and the [`MAX_DEVICES`] cap apply to owned devices only
/// (a song without any owned device is accepted here). An unowned kind is still refused, by `Kinds::need`, with
/// `LoadError::UnsupportedKind` when this registry is used alone.
pub fn preflight_owned(song: &Song<'_>, env: &Env<'_>) -> Result<(), Reject> {
    judge_song(song, env, false)
}

fn judge_song(song: &Song<'_>, env: &Env<'_>, strict: bool) -> Result<(), Reject> {
    let owned = |kind: u8| param_count(kind).is_some();
    let count = if strict { song.device_count() } else { song.devices().filter(|d| owned(d.kind)).count() };
    if (strict || count > 0) && (song.rate != RATE || env.rate != RATE as f64) {
        return Err(Reject::song(Reason::Rate));
    }
    if count > MAX_DEVICES {
        return Err(Reject::song(Reason::TooManyDevices));
    }
    for (i, d) in song.devices().enumerate() {
        if !strict && !owned(d.kind) {
            continue;
        }
        if let Err((param, reason)) = chunk_params(d.kind, d.chunk) {
            return Err(Reject::at(i, d.kind, param, reason));
        }
    }
    for t in song.tracks() {
        let Some(dev) = t.devices().find(|&d| song.devices().nth(d).is_some_and(|r| r.kind == 20)) else {
            continue;
        };
        let loud = song.lane(t.lane).is_some_and(|l| {
            l.events().any(|(_, e)| matches!(e, synth_core::device::EventKind::NoteOn { velocity, .. } if velocity > id20::MAX_VELOCITY))
        });
        if loud {
            return Err(Reject::at(dev, 20, None, Reason::Setting("note velocity above 213 (the noise envelope would grow without bound)")));
        }
    }
    let mut slots: [Option<Slot>; MAX_AUTOMATED] = [const { None }; MAX_AUTOMATED];
    // Automation boxes. ID 20 (like 3, 7, 25, 27, 34) has no arm in `judge` and needs none: `id20::check` accepts
    // exactly the vectors in [0, 1]^16, the box is widened by `MARGIN` and clamped to [0, 1], and the lane points are
    // u8 / 255 values, so every vector a lane can write is valid whatever the order or interpolation. Only the
    // parameter index (`LaneParam`) is judged. If ID 20 ever gains a non-range restriction, add a `judge` arm.
    for t in song.tracks() {
        for a in t.automations() {
            let dev = t.devices().nth(a.device).ok_or(Reject::song(Reason::LaneDevice))?;
            let at = match slots.iter().position(|s| s.as_ref().is_some_and(|s| s.device == dev)) {
                Some(at) => at,
                None => {
                    let free = slots.iter().position(Option::is_none);
                    let rec = song.devices().nth(dev).ok_or(Reject::song(Reason::LaneDevice))?;
                    if !strict && !owned(rec.kind) {
                        continue;
                    }
                    let init = chunk_params(rec.kind, rec.chunk).map_err(|(p, r)| Reject::at(dev, rec.kind, p, r))?;
                    let free = free.ok_or(Reject::at(dev, rec.kind, None, Reason::TooManyAutomated))?;
                    slots[free] = Some(Slot { device: dev, kind: rec.kind, init, lo: init, hi: init, mask: 0 });
                    free
                }
            };
            let s = slots[at].as_mut().unwrap_or_else(|| unreachable!());
            if a.param >= param_count(s.kind).unwrap_or(0) {
                return Err(Reject::at(dev, s.kind, u8::try_from(a.param).ok(), Reason::LaneParam));
            }
            for (_, v) in a.points() {
                s.lo[a.param] = s.lo[a.param].min(v);
                s.hi[a.param] = s.hi[a.param].max(v);
                s.mask |= 1 << a.param;
            }
        }
    }
    for s in slots.iter().flatten() {
        judge(s).map_err(|(p, what)| Reject::at(s.device, s.kind, p, Reason::Automation(what)))?;
    }
    Ok(())
}
