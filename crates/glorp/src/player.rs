//! Owned entry point for playing a Glorp-device song: [`GlorpPlayer`] parses the blob, runs the [`crate::preflight`]
//! with full diagnostics, builds `synth_core::mix::Player<GlorpDevice>` at the only supported block (128) and 44.1 kHz,
//! and reports a device fault instead of handing out silence-for-wrong-audio unannounced.
//!
//! Going around it (`Player::<GlorpDevice, _>::new` directly) is not a bypass: the owned-kinds preflight runs inside
//! `load_devices` (a kind this crate does not own is then refused by `Kinds::need`) and `Kinds::block` pins the
//! sub-block, only with the coarser `LoadError` instead of a [`Reject`].

use crate::kinds::{Fault, GlorpDevice};
use crate::preflight::{preflight, Reason, Reject, BLOCK};
use synth_core::mix::{Env, LoadError, Player, Song};
use synth_core::pool::Pool;
use synth_core::rng::Rng;

/// Random source for a song whose devices never draw (Glorp devices own all their state).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoRng;

impl Rng for NoRng {
    fn next_f32(&mut self) -> f32 {
        0.0
    }
}

fn load(e: LoadError) -> Reject {
    Reject { device: None, kind: None, param: None, reason: Reason::Load(e) }
}

/// A song of Glorp devices, playable only when the whole song passed the preflight.
pub struct GlorpPlayer<'a> {
    inner: Player<'a, GlorpDevice<'a>, NoRng>,
}

impl<'a> GlorpPlayer<'a> {
    /// Pool bytes [`GlorpPlayer::new`] needs (the song is parsed and judged first). The caller must impose its own
    /// pool-byte limit on the returned value before allocating; this function sets no cap of its own.
    pub fn required_pool(bytes: &'a [u8]) -> Result<usize, Reject> {
        let song = Song::parse(bytes).map_err(load)?;
        let env = Env::for_song(&song, None);
        preflight(&song, &env)?;
        Player::<GlorpDevice<'a>, NoRng>::required_pool(&song, &env, BLOCK).map_err(load)
    }

    /// Parses and judges `bytes`, builds every device in `pool`. `Err` names the first device / parameter / lane that
    /// cannot be played; nothing is built then.
    pub fn new(bytes: &'a [u8], pool: &mut Pool<'a>) -> Result<Self, Reject> {
        let song = Song::parse(bytes).map_err(load)?;
        let env = Env::for_song(&song, None);
        preflight(&song, &env)?;
        let inner = Player::new(song, &env, pool, NoRng, BLOCK).map_err(load)?;
        Ok(GlorpPlayer { inner })
    }

    /// Renders interleaved stereo. `Err` = some device faulted (an unsupported call it could not refuse earlier): the
    /// buffer is zeroed and the player keeps returning `Err`.
    pub fn render_f32(&mut self, out: &mut [f32]) -> Result<(), Fault> {
        self.inner.render_f32(out);
        match self.fault() {
            None => Ok(()),
            Some(f) => {
                out.fill(0.0);
                Err(f)
            }
        }
    }

    /// Restores the state at `frame` (see `Player::seek`).
    pub fn seek(&mut self, frame: u64) {
        self.inner.seek(frame);
    }

    pub fn length_frames(&self) -> u64 {
        self.inner.length_frames()
    }

    /// First fault of any device.
    pub fn fault(&self) -> Option<Fault> {
        self.inner.devices().iter().find_map(GlorpDevice::fault)
    }

    /// The core player (read access: devices, song).
    pub fn player(&self) -> &Player<'a, GlorpDevice<'a>, NoRng> {
        &self.inner
    }
}
