//! Optional external-reference check of the player integration (`GlorpDevice` / `GlorpPlayer`) against the original
//! engine's recorded output (local only: the third-party binary, the song and the harness output never enter git).
//! `GLORP_REF_ROOT` = the `ws-intros/out` directory holding `389811-2.bin` and `glorp-win64/analysis/harness/out/`
//! (taps + event logs, as for the other `reference*.rs`). Unset => every test below skips explicitly (that proves
//! nothing); set but a fixture missing => it fails loudly naming the file.
//!
//! What it checks, beyond the standalone device suites:
//!
//! * **`GlorpPlayer` on the real chunks and lanes** (`instruments_through_the_player_match_the_original`): for the
//!   instrument devices 5 and 6 (ID 21; device 5's lane form is bit-exact, see `id21_device5_through_the_player_matches_the_standalone_replay`), 7 (ID 22: 469,752 recorded parameter writes, see `id22_device7_through_the_player_matches_the_standalone_replay`), 8 (ID 25), 9 and 10 (ID 27) a one-device song is built from the real song's own device chunk, its
//!   own note lane and its own automation lanes (device 8 has 3,493 recorded parameter writes), played at the full
//!   78,292 blocks through `GlorpPlayer` (preflight, `Player` sub-block 128, lane interpolation, event offsets, the
//!   adapter) and compared with the device's recorded whole-song output at the same fixture-level parity bound as the
//!   standalone suites. The lane interpolation and the event timing of the core player are therefore checked against the
//!   original here, for the lanes the song really has.
//! * **`GlorpDevice` on the recorded buffers** (`effects_through_the_adapter_match_the_original`): ID 3 (dev 0), ID 7
//!   (dev 1), ID 34 (dev 25), ID 38 (dev 28 / 29 / 31 / 30), ID 35 (dev 27), ID 32 (dev 23, whole song), ID 30 (dev 15 / 16 / 17 / 19 / 20 / 21, whole song): the adapter is built from the
//!   real chunk by `Kinds::load`, every recorded `SetParam` goes through `Device::set_param`, every recorded note event
//!   through `note_on` / `note_off`, every block through `Device::run(128)` on the recorded input, side-chain pair
//!   included; no fault may latch.
//! * **The real song is accepted and played whole** (`whole_song_is_accepted_by_the_preflight`,
//!   `whole_song_through_the_player_matches_the_original_render`): every id is implemented; the render is compared with the
//!   original's own 16-bit render (`fullA.wav`) at tolerance, not bit, parity (see the test).
//!
//! Negative controls (`must_mismatch`): device 5 in the core's default lane form, a flipped reference bit, device 8's lanes removed, its notes shifted by one frame,
//! and ID 35's recorded parameter writes dropped must each leave the bound.
use glorp::kinds::GlorpDevice;
use glorp::player::{GlorpPlayer, NoRng};
use glorp::preflight::{param_count, Reason};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use synth_core::device::{Ctx, Device, EventKind, Io};
use synth_core::mix::{Env, Kinds, LoadError, Player, Song};
use synth_core::pool::Pool;

#[allow(dead_code)]
#[path = "../src/testsong.rs"]
mod testsong;

const B: usize = 128;
const SONG_BLOCKS: usize = 78_292;
const BLOCK_BYTES: usize = 8 * B * 4;

fn big<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    std::thread::Builder::new().stack_size(256 << 20).spawn(f).unwrap().join().unwrap()
}

struct Fx {
    out: PathBuf,
    blob: Vec<u8>,
}

fn require(p: &Path) -> PathBuf {
    assert!(p.is_file(), "GLORP_REF_ROOT is set but the fixture {} is missing", p.display());
    p.to_path_buf()
}

fn fixtures() -> Option<Fx> {
    let root = PathBuf::from(std::env::var_os("GLORP_REF_ROOT")?);
    let blob = std::fs::read(require(&root.join("389811-2.bin"))).unwrap();
    let out = root.join("glorp-win64/analysis/harness/out");
    Some(Fx { out, blob })
}

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}

#[derive(Clone, Copy, Debug)]
enum Ev {
    On(u8, u8, u32),
    Off(u8, u32),
    Param(usize, f32),
}

/// Rows of device `dev` of `ev_full.csv` (`block,dev,id,kind,a,b,c`), by block, in log order.
fn events_of(path: &Path, dev: usize) -> HashMap<usize, Vec<Ev>> {
    let mut m: HashMap<usize, Vec<Ev>> = HashMap::new();
    for (n, line) in BufReader::new(std::fs::File::open(path).unwrap()).lines().enumerate().skip(1) {
        let line = line.unwrap();
        let f: Vec<&str> = line.split(',').collect();
        assert_eq!(f.len(), 7, "{}:{}: bad row {line:?}", path.display(), n + 1);
        if f[1].parse::<usize>().unwrap() != dev {
            continue;
        }
        let ev = match f[3] {
            "on" => Ev::On(f[4].parse().unwrap(), f[5].parse().unwrap(), f[6].parse().unwrap()),
            "off" => Ev::Off(f[4].parse().unwrap(), f[5].parse::<f64>().unwrap() as u32),
            "param" => Ev::Param(f[4].parse().unwrap(), f32::from_bits(f[6].parse().unwrap())),
            k => panic!("{}:{}: unknown event kind {k}", path.display(), n + 1),
        };
        m.entry(f[0].parse().unwrap()).or_default().push(ev);
    }
    m
}

#[derive(Default, Debug)]
struct Stats {
    samples: usize,
    exact: usize,
    max_abs: f64,
    max_ref: f64,
    nonzero: usize,
    /// First sample beyond its per-sample bound (`Bound::Sample`), if any.
    first_bad: Option<String>,
}

#[derive(Clone, Copy)]
enum Bound {
    /// `abs + rel · |ref|` per sample.
    Sample(f64, f64),
    /// Fixture level: the largest error may not exceed `rel · max|ref|`.
    Peak(f64),
}

impl Stats {
    fn add(&mut self, what: &str, at: &str, got: f32, want: f32, bound: Bound) {
        let e = if got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan()) { 0.0 } else { (got as f64 - want as f64).abs() };
        self.samples += 1;
        self.exact += (e == 0.0) as usize;
        self.nonzero += (want != 0.0) as usize;
        self.max_abs = self.max_abs.max(e);
        self.max_ref = self.max_ref.max((want as f64).abs());
        if let Bound::Sample(a, r) = bound {
            if (e > a + r * (want as f64).abs() || e.is_nan()) && self.first_bad.is_none() {
                self.first_bad = Some(format!("{what} {at}: got {got} want {want} (err {e:e})"));
            }
        }
    }

    /// Panics with `parity got/want mismatch` when outside the bound.
    fn check(&self, what: &str, bound: Bound) {
        if let Some(b) = &self.first_bad {
            panic!("parity got/want mismatch: {b}");
        }
        if let Bound::Peak(rel) = bound {
            assert!(
                self.max_abs <= rel * self.max_ref,
                "parity got/want mismatch: {what}: max abs err {:e} exceeds {rel} of the peak {}",
                self.max_abs,
                self.max_ref
            );
        }
    }

    fn report(&self, what: &str) {
        eprintln!(
            "{what}: {} samples, {} exact ({:.3} %), {} nonzero refs, max |ref| {:.4}, max abs err {:.3e}",
            self.samples,
            self.exact,
            100.0 * self.exact as f64 / self.samples as f64,
            self.nonzero,
            self.max_ref,
            self.max_abs
        );
    }
}

fn must_mismatch(what: &str, f: impl FnOnce() + std::panic::UnwindSafe) {
    let err = std::panic::catch_unwind(f).expect_err(&format!("{what}: the control was not caught"));
    let msg = err.downcast_ref::<String>().cloned().or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
    assert!(msg.contains("parity got/want mismatch"), "{what}: failed for another reason: {msg}");
}

// ---- the real song ----

/// The original engine's whole-song render (`fullA.wav`, 16-bit stereo 44.1 kHz, 10,021,376 frames), captured through the
/// isolated harness. Every Glorp id of the song is implemented, so the whole song is accepted and plays.
fn original_wav(out: &Path) -> Vec<i16> {
    let raw = std::fs::read(require(&out.join("fullA.wav"))).unwrap();
    assert_eq!(&raw[..4], b"RIFF");
    assert_eq!(&raw[36..40], b"data", "fullA.wav: expected the canonical 44-byte header");
    raw[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect()
}

#[test]
fn whole_song_is_accepted_by_the_preflight() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    let song = Song::parse(&fx.blob).unwrap();
    let mut unsupported: Vec<u8> = song.devices().map(|d| d.kind).filter(|&k| param_count(k).is_none()).collect();
    unsupported.sort();
    unsupported.dedup();
    assert_eq!(unsupported, Vec::<u8>::new(), "every id of the real song is implemented");
    let env = Env::for_song(&song, None);
    assert_eq!(glorp::preflight::preflight(&song, &env), Ok(()));
    // the same blob with one ID 30 chunk made unplayable (cascade on, first-order topology) is refused naming that device
    let first30 = song.devices().position(|d| d.kind == 30).unwrap();
    let mut bad = fx.blob.clone();
    let rec = song.devices().nth(first30).unwrap();
    let at = rec.chunk.as_ptr() as usize - fx.blob.as_ptr() as usize;
    bad[at + 4 * 16..at + 4 * 16 + 4].copy_from_slice(&0.5f32.to_le_bytes());
    bad[at + 4 * 20..at + 4 * 20 + 4].copy_from_slice(&0.5f32.to_le_bytes());
    let err = big(move || {
        let mut mem = vec![0u8; 1 << 20];
        let mut pool = Pool::new(&mut mem);
        GlorpPlayer::new(&bad, &mut pool).err()
    });
    let r = err.expect("a first-order cascade topology must be refused");
    assert_eq!((r.device, r.kind), (Some(first30), Some(30)));
    assert!(matches!(r.reason, Reason::Setting(_)), "{:?}", r.reason);
}

/// The whole real song through `GlorpPlayer` (every device, every lane, every note) against the original's own render.
/// Our output is float: `· 32768`, rounded, clipped to the 16-bit range the original writes. Reports the exact share
/// and the worst difference in 16-bit steps, and saves nothing in the repository.
#[test]
fn whole_song_through_the_player_matches_the_original_render() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    let (blob, out) = (fx.blob.clone(), fx.out.clone());
    big(move || {
        let want = original_wav(&out);
        let need = GlorpPlayer::required_pool(&blob).unwrap();
        let mut mem = vec![0u8; need + 64];
        let mut pool = Pool::new(&mut mem);
        let mut p = GlorpPlayer::new(&blob, &mut pool).unwrap();
        let frames = p.length_frames() as usize;
        // the original renders whole 128-frame blocks; the player stops at the song length
        assert!(want.len() >= 2 * frames && want.len() - 2 * frames < 2 * B, "the original's render ({} samples) and the song ({frames} frames) disagree on the length", want.len());
        let mut buf = vec![0.0f32; 2 * 4096];
        let (mut exact, mut off_by_one, mut worst, mut worst_at, mut clipped) = (0usize, 0usize, 0i32, 0usize, 0usize);
        let mut done = 0;
        let mut dump = std::env::var_os("GLORP_SONG_WAV").map(|p| (std::fs::File::create(p).unwrap(), Vec::<u8>::new()));
        if let Some((_, v)) = dump.as_mut() {
            v.extend_from_slice(&[0u8; 44]);
        }
        while done < frames {
            let n = 4096.min(frames - done);
            p.render_f32(&mut buf[..2 * n]).expect("no device may fault");
            for (i, &x) in buf[..2 * n].iter().enumerate() {
                let q = (x as f64 * 32768.0).round().clamp(-32768.0, 32767.0) as i32;
                clipped += (x.abs() >= 1.0) as usize;
                let d = (q - want[2 * done + i] as i32).abs();
                exact += (d == 0) as usize;
                off_by_one += (d == 1) as usize;
                if d > worst {
                    (worst, worst_at) = (d, 2 * done + i);
                }
                if let Some((_, v)) = dump.as_mut() {
                    v.extend_from_slice(&(q as i16).to_le_bytes());
                }
            }
            done += n;
        }
        if let Some((mut f, v)) = dump {
            use std::io::Write;
            let mut v = v;
            let data = (v.len() - 44) as u32;
            v[..4].copy_from_slice(b"RIFF");
            v[4..8].copy_from_slice(&(36 + data).to_le_bytes());
            v[8..16].copy_from_slice(b"WAVEfmt ");
            v[16..20].copy_from_slice(&16u32.to_le_bytes());
            v[20..24].copy_from_slice(&[1, 0, 2, 0]);
            v[24..28].copy_from_slice(&44100u32.to_le_bytes());
            v[28..32].copy_from_slice(&(44100u32 * 4).to_le_bytes());
            v[32..36].copy_from_slice(&[4, 0, 16, 0]);
            v[36..40].copy_from_slice(b"data");
            v[40..44].copy_from_slice(&data.to_le_bytes());
            f.write_all(&v).unwrap();
        }
        let total = 2 * frames;
        eprintln!(
            "whole song: {total} samples, {exact} exact ({:.3} %), {off_by_one} off by one 16-bit step ({:.3} %), worst {worst} steps at sample {worst_at} ({} s), {clipped} float samples at or beyond full scale",
            100.0 * exact as f64 / total as f64,
            100.0 * off_by_one as f64 / total as f64,
            worst_at as f64 / 2.0 / 44100.0,
        );
        assert!(worst <= WHOLE_SONG_WORST_STEPS, "worst difference {worst} steps exceeds {WHOLE_SONG_WORST_STEPS}");
    });
}

/// Largest per-sample difference (in 16-bit steps) tolerated between the whole song through the player and the
/// original's render. Measured over the whole song (20,042,688 samples): 23.8 % exact, 49.5 % one step off, worst 137 steps
/// (0.42 % of full scale) at 183.2 s. The residual is carried by the instruments and effects that are tolerance-parity only
/// (ID 25 / 27 / 21 / 20: 1e-4..1e-3 of the peak, see their suites); the ID 30 devices are bit-exact (devices 15 / 16 / 17 /
/// 18 / 21) or within 1.4e-4 (device 20) given the recorded input, so they only pass those input errors on.
const WHOLE_SONG_WORST_STEPS: i32 = 160;

// ---- GlorpPlayer on the real chunks and lanes ----

struct Sub {
    /// One-device song bytes.
    bytes: Vec<u8>,
}

/// A one-device song from the real song: device `dev`'s chunk, the note lane of its track and the automation lanes
/// that target it. `lanes = false` drops the automation, `shift` delays the first note event by that many frames.
fn subsong(blob: &[u8], dev: usize, lanes: bool, shift: i32) -> Sub {
    let song = Song::parse(blob).unwrap();
    let rec = song.devices().nth(dev).unwrap();
    let t = song.tracks().find(|t| t.devices().any(|d| d == dev)).expect("device on no track");
    let local = t.devices().position(|d| d == dev).unwrap();
    let mut lane: Vec<(i32, u8, Option<u8>)> = song
        .lane(t.lane)
        .unwrap()
        .events()
        .map(|(d, k)| match k {
            EventKind::NoteOn { note, velocity } => (d as i32, note, Some(velocity)),
            EventKind::NoteOff { note } => (d as i32, note, None),
        })
        .collect();
    if let Some(first) = lane.first_mut() {
        first.0 += shift;
    }
    let mut autos = vec![];
    for a in t.automations().filter(|a| a.device == local && lanes) {
        let mut prev = 0i32;
        let pts: Vec<(i32, u8)> = a
            .points()
            .map(|(time, v)| {
                let d = time.wrapping_sub(prev);
                prev = time;
                (d, (v * 255.0).round() as u8)
            })
            .collect();
        autos.push((0, a.param as i32, pts));
    }
    let mut s = testsong::Song::new();
    s.devices.push((rec.kind, rec.chunk.to_vec()));
    s.lanes.push(lane);
    s.tracks.push(testsong::Track { vol: 1.0, recv: vec![], devs: vec![0], lane: 0, autos });
    Sub { bytes: s.bytes() }
}

/// `GlorpDevice` answering the core's default lane form (`a·(1−t) + b·t`): what the core played before the per-device
/// hook. Only the negative control for the ID 21 lane parity below.
struct CoreDefaultForm<'a>(GlorpDevice<'a>);
impl Device for CoreDefaultForm<'_> {
    fn param_count(&self) -> usize {
        self.0.param_count()
    }
    fn set_param(&mut self, i: usize, v: f32) {
        self.0.set_param(i, v)
    }
    fn note_on(&mut self, n: u8, v: u8, o: u32) {
        self.0.note_on(n, v, o)
    }
    fn note_off(&mut self, n: u8, o: u32) {
        self.0.note_off(n, o)
    }
    fn all_notes_off(&mut self) {
        self.0.all_notes_off()
    }
    fn reset(&mut self) {
        self.0.reset()
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        self.0.run(io, n, ctx)
    }
}
impl<'a> Kinds<'a> for CoreDefaultForm<'a> {
    fn block(song: &Song<'a>, env: &Env<'a>) -> Result<Option<usize>, LoadError> {
        GlorpDevice::block(song, env)
    }
    fn preflight(song: &Song<'a>, env: &Env<'a>) -> Result<(), LoadError> {
        GlorpDevice::preflight(song, env)
    }
    fn need(k: u8, c: &'a [u8], env: &Env<'a>) -> Result<usize, LoadError> {
        GlorpDevice::need(k, c, env)
    }
    fn load(k: u8, c: &'a [u8], env: &Env<'a>, pool: &mut Pool<'a>, rng: &mut dyn synth_core::rng::Rng) -> Result<Self, LoadError> {
        GlorpDevice::load(k, c, env, pool, rng).map(CoreDefaultForm)
    }
}

/// Plays `sub` for `blocks` blocks through `GlorpPlayer` and compares with the device's recorded output.
fn player_vs_tap(what: &str, sub: &Sub, tap: &Path, blocks: usize, bound: Bound, flip: Option<usize>) -> Stats {
    player_vs_tap_form(what, sub, tap, blocks, bound, flip, false)
}

/// As [`player_vs_tap`]; `core_default` plays the same song through the core `Player` with [`CoreDefaultForm`] instead
/// (the lane form the core used before the per-device hook).
fn player_vs_tap_form(what: &str, sub: &Sub, tap: &Path, blocks: usize, bound: Bound, flip: Option<usize>, core_default: bool) -> Stats {
    let bytes = sub.bytes.clone();
    let tap = tap.to_path_buf();
    let what = what.to_string();
    big(move || {
        let need = GlorpPlayer::required_pool(&bytes).unwrap_or_else(|r| panic!("{what}: refused {r:?}"));
        let mut mem = vec![0u8; need];
        let mut pool = Pool::new(&mut mem);
        let mut render: Box<dyn FnMut(&mut [f32]) -> Result<(), &'static str> + '_> = if core_default {
            let song = Song::parse(&bytes).unwrap();
            let env = Env::for_song(&song, None);
            let mut p = Player::<CoreDefaultForm, NoRng>::new(song, &env, &mut pool, NoRng, B).unwrap_or_else(|e| panic!("{what}: refused {e:?}"));
            Box::new(move |out| {
                p.render_f32(out);
                Ok(())
            })
        } else {
            let mut p = GlorpPlayer::new(&bytes, &mut pool).unwrap_or_else(|r| panic!("{what}: refused {r:?}"));
            Box::new(move |out| p.render_f32(out))
        };
        let mut f = BufReader::with_capacity(1 << 20, std::fs::File::open(&tap).unwrap());
        let len = std::fs::metadata(&tap).unwrap().len() as usize;
        assert_eq!(len, SONG_BLOCKS * BLOCK_BYTES, "{}: not the whole-song tap", tap.display());
        let mut s = Stats::default();
        let (mut raw, mut out) = (vec![0u8; BLOCK_BYTES], vec![0.0f32; 2 * B]);
        for b in 0..blocks {
            f.read_exact(&mut raw).unwrap();
            let blk = f32s(&raw);
            render(&mut out).unwrap_or_else(|e| panic!("{what} block {b}: device fault: {e}"));
            for c in 0..2 {
                for i in 0..B {
                    let mut want = blk[(4 + c) * B + i];
                    if flip == Some(b * B + i) && c == 0 {
                        want = f32::from_bits(want.to_bits() ^ 1);
                    }
                    s.add(&what, &format!("block {b} ch {c} i {i}"), out[2 * i + c], want, bound);
                }
            }
        }
        s
    })
}

fn instrument_paths(fx: &Fx, dev: usize) -> PathBuf {
    let dir = match dev {
        5 | 6 => "id21impl",
        4 => "id20impl",
        7 => "id22impl",
        11..=14 => "id28impl",
        2 | 3 => "id14impl",
        8 => "id25impl",
        _ => "id27impl",
    };
    require(&fx.out.join(format!("{dir}/taps/dev{dev:02}.tap")))
}

#[test]
fn instruments_through_the_player_match_the_original() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    // fixture-level bounds of the standalone suites (id21 device 5: 4e-4, measured 2.6e-4; id25: 2.5e-3, measured 1.3e-4;
    // id27: 1e-3, measured 6.7e-4; id21 device 6: 1e-6, measured 2.8e-8; id22: 1e-6, measured 6.2e-8)
    for (dev, rel, blocks) in [(5usize, 4e-4, SONG_BLOCKS), (6, 1e-6, SONG_BLOCKS), (7, 1e-6, SONG_BLOCKS), (8, 2.5e-3, SONG_BLOCKS), (9, 1e-3, SONG_BLOCKS), (10, 1e-3, SONG_BLOCKS)] {
        let sub = subsong(&fx.blob, dev, true, 0);
        let bound = Bound::Peak(rel);
        let s = player_vs_tap(&format!("device {dev} via GlorpPlayer"), &sub, &instrument_paths(&fx, dev), blocks, bound, None);
        s.report(&format!("device {dev} via GlorpPlayer"));
        assert_eq!(s.samples, blocks * B * 2);
        assert!(s.nonzero > 100_000, "device {dev}: tap too quiet to mean anything");
        s.check(&format!("device {dev}"), bound);
    }
}

/// ID 21 device 5 (167 notes, lanes on P1 / P5 / P7; ID 21 seeds on the bits of P7) through the player is the standalone
/// replay of the original's recorded writes: the core player writes the lane in the original's own form
/// (`GlorpDevice::lane_lerp` = `Delta`, `a + (b−a)·t`), so P7 is bit-identical and the same partials play. The measured
/// standalone figures (99.4462 % exact, max 9.389e-4 at peak 3.64) are pinned, not just the 4e-4 bound.
#[test]
fn id21_device5_through_the_player_matches_the_standalone_replay() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    let bound = Bound::Peak(4e-4);
    let sub = subsong(&fx.blob, 5, true, 0);
    let s = player_vs_tap("device 5", &sub, &instrument_paths(&fx, 5), SONG_BLOCKS, bound, None);
    s.report("device 5 via GlorpPlayer (whole song)");
    s.check("device 5", bound);
    assert_eq!(s.samples, SONG_BLOCKS * B * 2);
    let exact = 100.0 * s.exact as f64 / s.samples as f64;
    assert!(exact > 99.44, "exact fraction {exact} %: lanes no longer bit-identical");
    assert!(s.max_abs < 9.5e-4, "max abs error {}", s.max_abs);
}

/// Sensitivity of the test above, and the record of the gap it closes: the same song through the core's **default**
/// lane form (`a·(1−t) + b·t`) leaves the bound at the first note after P7's first one-ulp difference (block 62,728).
#[test]
fn id21_device5_with_the_core_default_lane_form_still_mismatches() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    let tap = instrument_paths(&fx, 5);
    let bound = Bound::Peak(4e-4);
    let sub = subsong(&fx.blob, 5, true, 0);
    must_mismatch("device 5 lanes in the core's default form", move || {
        player_vs_tap_form("device 5", &sub, &tap, 64_500, bound, None, true).check("device 5", bound);
    });
}

/// ID 22 device 7 (30 notes, lanes on P0 / P1 / P2 / P3 / P6 / P8) through the player is the standalone replay of the
/// original's recorded writes: the same lane form, event timing and weight guard (the song's own lanes stay inside it,
/// the preflight accepts them). Pinned, not just bounded: whole song, 99.99 % of the samples bit-identical.
#[test]
fn id22_device7_through_the_player_matches_the_standalone_replay() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    let bound = Bound::Peak(1e-6);
    let sub = subsong(&fx.blob, 7, true, 0);
    let s = player_vs_tap("device 7", &sub, &instrument_paths(&fx, 7), SONG_BLOCKS, bound, None);
    s.report("device 7 via GlorpPlayer (whole song)");
    s.check("device 7", bound);
    assert_eq!(s.samples, SONG_BLOCKS * B * 2);
    let exact = 100.0 * s.exact as f64 / s.samples as f64;
    assert!(exact > 99.99, "exact fraction {exact} %");
    assert!(s.max_abs < 2e-7, "max abs error {}", s.max_abs);
    // sensitivity: without its lanes, or with its notes a frame late, the same song leaves the bound
    let tap = instrument_paths(&fx, 7);
    for (what, sub) in [("id22 lanes removed", subsong(&fx.blob, 7, false, 0)), ("id22 notes one frame late", subsong(&fx.blob, 7, true, 1))] {
        let tap = tap.clone();
        must_mismatch(what, move || {
            player_vs_tap(what, &sub, &tap, 20_000, bound, None).check(what, bound);
        });
    }
}

/// ID 20 device 4 (notes at velocity 127 / 81 / 49 / 38 …, lanes on P5 / P8 / P9 / P15, the velocity ceiling judged by the
/// preflight, which accepts the song's own lane) through the player at the fixture bound of the standalone suite
/// (1e-4 of the peak; measured 98.796 % of the samples bit-identical, worst 1.12e-4 = 2.4e-5 of the peak, the same as the standalone replay: the original's `exp` rounding, see `reference20.rs`). Without its lanes, or with its
/// notes a frame late, the same song leaves the bound.
#[test]
fn id20_device4_through_the_player_matches_the_original() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    let bound = Bound::Peak(1e-4);
    let sub = subsong(&fx.blob, 4, true, 0);
    let tap = instrument_paths(&fx, 4);
    let s = player_vs_tap("device 4 via GlorpPlayer", &sub, &tap, SONG_BLOCKS, bound, None);
    s.report("device 4 via GlorpPlayer (whole song)");
    assert_eq!(s.samples, SONG_BLOCKS * B * 2);
    assert!(s.nonzero > 1_000_000, "device 4: tap too quiet to mean anything");
    s.check("device 4", bound);
    let exact = 100.0 * s.exact as f64 / s.samples as f64;
    assert!(exact > 98.7 && s.max_abs < 1.2e-4, "exact fraction {exact} %, max abs error {}: the replay of the original is no longer reproduced", s.max_abs);
    for (what, sub) in [("id20 lanes removed", subsong(&fx.blob, 4, false, 0)), ("id20 notes one frame late", subsong(&fx.blob, 4, true, 1))] {
        let tap = tap.clone();
        must_mismatch(what, move || {
            player_vs_tap(what, &sub, &tap, 30_000, bound, None).check(what, bound);
        });
    }
}

/// The four ID 28 devices (11..14: 55 / 172 / 352 / 65 notes with velocities 1..127, lanes on P5 / P17 / P22 / P24, P3 / P10 /
/// P17 / P22, P17 / P18 / P22, up to 156,584 recorded parameter writes) through the player: the same fixture-level bound as
/// the standalone replay (`reference28.rs`: 99.29–99.93 % bit-exact, worst 2.4e-7 absolute = 4.6e-7 of the peak). Without its
/// lanes, or with its notes a frame late, the same song leaves the bound.
#[test]
fn id28_devices_through_the_player_match_the_original() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    for dev in 11..=14usize {
        let bound = Bound::Peak(2.0e-6);
        let sub = subsong(&fx.blob, dev, true, 0);
        let tap = instrument_paths(&fx, dev);
        let s = player_vs_tap(&format!("device {dev} via GlorpPlayer"), &sub, &tap, SONG_BLOCKS, bound, None);
        s.report(&format!("device {dev} via GlorpPlayer (whole song)"));
        assert_eq!(s.samples, SONG_BLOCKS * B * 2);
        assert!(s.nonzero > 4_000_000, "device {dev}: tap too quiet to mean anything");
        s.check(&format!("device {dev}"), bound);
        let exact = 100.0 * s.exact as f64 / s.samples as f64;
        assert!(exact > 99.2 && s.max_abs < 3.0e-7, "device {dev}: exact fraction {exact} %, max abs error {}: the standalone replay is no longer reproduced", s.max_abs);
        for (what, sub) in [("id28 lanes removed", subsong(&fx.blob, dev, false, 0)), ("id28 notes one frame late", subsong(&fx.blob, dev, true, 1))] {
            let tap = tap.clone();
            must_mismatch(what, move || {
                player_vs_tap(what, &sub, &tap, 60_000, bound, None).check(what, bound);
            });
        }
    }
}

/// The two ID 14 devices (2 never plays, 3 holds the song's 16 mono notes with P2 / P3 / P17 lanes written every block) through
/// the player: the same fixture-level bound as the standalone replay (`reference14.rs`: 1e-4 of the peak, measured 2.9e-5; the
/// original's `expf` rounding differs from `libm`'s by an ulp that the Q = 1000 resonators ring up). Without its lanes, or with
/// its notes a frame late, the same song leaves the bound.
#[test]
fn id14_devices_through_the_player_match_the_original() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    for dev in 2..=3usize {
        let bound = Bound::Peak(1.0e-4);
        let sub = subsong(&fx.blob, dev, true, 0);
        let tap = instrument_paths(&fx, dev);
        let s = player_vs_tap(&format!("device {dev} via GlorpPlayer"), &sub, &tap, SONG_BLOCKS, bound, None);
        s.report(&format!("device {dev} via GlorpPlayer (whole song)"));
        assert_eq!(s.samples, SONG_BLOCKS * B * 2);
        s.check(&format!("device {dev}"), bound);
        if dev == 2 {
            assert_eq!(s.nonzero, 0, "device 2 never plays");
            continue;
        }
        assert!(s.nonzero > 1_000_000, "device {dev}: tap too quiet to mean anything");
        for (what, sub) in [("id14 lanes removed", subsong(&fx.blob, dev, false, 0)), ("id14 notes one frame late", subsong(&fx.blob, dev, true, 1))] {
            let tap = tap.clone();
            must_mismatch(what, move || {
                player_vs_tap(what, &sub, &tap, 30_000, bound, None).check(what, bound);
            });
        }
    }
}

#[test]
fn player_negative_controls_are_caught() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    // device 8 is the one with automation and a dense note lane; 6000 blocks are enough to leave the bound
    let tap = instrument_paths(&fx, 8);
    let blocks = 6000;
    let bound = Bound::Peak(2.5e-3);
    let good = subsong(&fx.blob, 8, true, 0);
    player_vs_tap("control baseline", &good, &tap, blocks, bound, None).check("baseline", bound);
    for (what, sub, flip) in [
        ("flipped reference bit", subsong(&fx.blob, 8, true, 0), Some(5000)),
        ("lanes removed", subsong(&fx.blob, 8, false, 0), None),
        ("notes one frame late", subsong(&fx.blob, 8, true, 1), None),
    ] {
        let tap = tap.clone();
        // a flipped bit is a 1-ulp change: only the per-sample bound sees it
        let bound = if flip.is_some() { Bound::Sample(0.0, 0.0) } else { bound };
        must_mismatch(what, move || {
            player_vs_tap(what, &sub, &tap, blocks, bound, flip).check(what, bound);
        });
    }
    // ID 21 (device 6, 4000 blocks: its five notes): the song's lanes (P5 / P6 / P7, the last one the noise seed) and the
    // note timing are heard at the tight bound of an almost bit-exact device
    let tap = instrument_paths(&fx, 6);
    let (blocks, bound) = (4000, Bound::Peak(1e-6));
    player_vs_tap("id21 baseline", &subsong(&fx.blob, 6, true, 0), &tap, blocks, bound, None).check("id21 baseline", bound);
    for (what, sub) in [("id21 lanes removed", subsong(&fx.blob, 6, false, 0)), ("id21 notes one frame late", subsong(&fx.blob, 6, true, 1))] {
        let tap = tap.clone();
        must_mismatch(what, move || {
            player_vs_tap(what, &sub, &tap, blocks, bound, None).check(what, bound);
        });
    }
}

// ---- GlorpDevice on the recorded buffers ----

#[derive(Clone, Copy, PartialEq)]
enum Src {
    /// `taps/devNN.tap` window of `tapspec.txt`, events of `ev_full.csv`.
    Window,
    /// `taps32/devNN.tap`: the whole song from block 0, events of `ev_full.csv`.
    Taps32,
    /// `id30impl/taps/devNN.tap` (whole song) with `id30impl/ev.csv`.
    Id30,
}

struct Spec {
    dev: usize,
    kind: u8,
    /// Run silent blocks from 0 before the window (state that integrates through silence).
    silent_prefix: bool,
    bound: Bound,
    /// Where the recorded buffers live.
    src: Src,
}

const SPECS: [Spec; 15] = [
    Spec { dev: 0, kind: 3, silent_prefix: false, bound: Bound::Sample(1e-7, 3e-7), src: Src::Window },
    Spec { dev: 1, kind: 7, silent_prefix: false, bound: Bound::Sample(1e-7, 3e-7), src: Src::Window },
    Spec { dev: 25, kind: 34, silent_prefix: true, bound: Bound::Sample(3e-7, 3e-7), src: Src::Window },
    Spec { dev: 26, kind: 34, silent_prefix: true, bound: Bound::Sample(3e-7, 3e-7), src: Src::Window },
    Spec { dev: 28, kind: 38, silent_prefix: false, bound: Bound::Sample(1e-6, 4e-7), src: Src::Window },
    Spec { dev: 29, kind: 38, silent_prefix: false, bound: Bound::Sample(1e-6, 4e-7), src: Src::Window },
    Spec { dev: 30, kind: 38, silent_prefix: false, bound: Bound::Sample(1e-6, 4e-7), src: Src::Window },
    Spec { dev: 27, kind: 35, silent_prefix: false, bound: Bound::Sample(4e-3, 0.0), src: Src::Window },
    Spec { dev: 23, kind: 32, silent_prefix: false, bound: Bound::Sample(1e-4, 0.0), src: Src::Taps32 },
    // ID 30, whole song from the recorded input: devices 15 / 16 / 17 / 21 bit-exact, 19 within 2.5e-13, 20 within 1.5e-4
    // (a 1-ulp filter coefficient difference, see `reference30.rs`); device 18 is inactive in the song (no evidence here)
    Spec { dev: 15, kind: 30, silent_prefix: false, bound: Bound::Sample(0.0, 0.0), src: Src::Id30 },
    Spec { dev: 16, kind: 30, silent_prefix: false, bound: Bound::Sample(0.0, 0.0), src: Src::Id30 },
    Spec { dev: 17, kind: 30, silent_prefix: false, bound: Bound::Sample(0.0, 0.0), src: Src::Id30 },
    Spec { dev: 19, kind: 30, silent_prefix: false, bound: Bound::Sample(1e-9, 0.0), src: Src::Id30 },
    Spec { dev: 20, kind: 30, silent_prefix: false, bound: Bound::Peak(3e-4), src: Src::Id30 },
    Spec { dev: 21, kind: 30, silent_prefix: false, bound: Bound::Sample(0.0, 0.0), src: Src::Id30 },
];

fn tapspec(out: &Path) -> HashMap<usize, (usize, usize)> {
    std::fs::read_to_string(require(&out.join("tapspec.txt")))
        .unwrap()
        .lines()
        .filter_map(|l| {
            let a: Vec<usize> = l.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            (a.len() == 3).then(|| a[2].checked_sub(a[1]).map(|n| (a[0], (a[1], n)))).flatten()
        })
        .collect()
}

fn device_vs_tap(fx: &Fx, sp: &Spec, drop_params: bool, flip: Option<usize>, tight: bool) -> Stats {
    let (blob, out) = (fx.blob.clone(), fx.out.clone());
    let (dev, kind, silent, src) = (sp.dev, sp.kind, sp.silent_prefix, sp.src);
    // `tight`: exact equality (only for a device that is bit-exact on its window)
    let bound = if tight { Bound::Sample(0.0, 0.0) } else { sp.bound };
    let what = format!("device {dev} (ID {kind}) via GlorpDevice");
    big(move || {
        let (start, n) = if src == Src::Window { tapspec(&out)[&dev] } else { (0, SONG_BLOCKS) };
        let path = require(&out.join(match src {
            Src::Window => format!("taps/dev{dev:02}.tap"),
            Src::Taps32 => format!("taps32/dev{dev:02}.tap"),
            Src::Id30 => format!("id30impl/taps/dev{dev}.tap"),
        }));
        let ev_path = if src == Src::Id30 { "id30impl/ev.csv" } else { "ev_full.csv" };
        let events = events_of(&require(&out.join(ev_path)), dev);
        let song = Song::parse(&blob).unwrap();
        let rec = song.devices().nth(dev).unwrap();
        assert_eq!(rec.kind, kind);
        let env = Env::for_song(&song, None);
        let mut mem = vec![0u8; <GlorpDevice as Kinds>::need(kind, rec.chunk, &env).unwrap() + 64];
        let mut pool = Pool::new(&mut mem);
        let mut d = <GlorpDevice as Kinds>::load(kind, rec.chunk, &env, &mut pool, &mut NoRng).unwrap();
        let mut f = BufReader::with_capacity(1 << 20, std::fs::File::open(&path).unwrap());
        let mut s = Stats::default();
        let mut raw = vec![0u8; BLOCK_BYTES];
        for b in 0..start + n {
            for ev in events.get(&b).map(|v| v.as_slice()).unwrap_or(&[]) {
                match *ev {
                    Ev::Param(i, v) if !drop_params => d.set_param(i, v),
                    Ev::Param(..) => {}
                    Ev::On(note, vel, off) if b >= start => d.note_on(note, vel, off),
                    Ev::Off(note, off) if b >= start => d.note_off(note, off),
                    _ => {}
                }
            }
            let (mut l, mut r, mut sl, mut sr);
            let blk;
            if b < start {
                if !silent {
                    continue;
                }
                blk = vec![0.0f32; 8 * B];
            } else {
                f.read_exact(&mut raw).unwrap_or_else(|e| panic!("{what}: short tap at block {b}: {e}"));
                blk = f32s(&raw);
            }
            (l, r, sl, sr) = (blk[..B].to_vec(), blk[B..2 * B].to_vec(), blk[2 * B..3 * B].to_vec(), blk[3 * B..4 * B].to_vec());
            let mut rng = NoRng;
            let mut ctx = Ctx { rate: 44100.0, tempo: 174, frame: (b * B) as u64, rng: &mut rng };
            d.run(&mut Io { ch: [&mut l, &mut r, &mut sl, &mut sr] }, B, &mut ctx);
            assert!(d.fault().is_none(), "{what} block {b}: fault {:?}", d.fault());
            if b < start {
                continue;
            }
            for (c, got) in [&l, &r].into_iter().enumerate() {
                for i in 0..B {
                    let mut want = blk[(4 + c) * B + i];
                    if flip == Some((b - start) * B + i) && c == 0 {
                        want = f32::from_bits(want.to_bits() ^ 1);
                    }
                    s.add(&what, &format!("block {b} ch {c} i {i}"), got[i], want, bound);
                }
            }
        }
        s
    })
}

#[test]
fn effects_through_the_adapter_match_the_original() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    for sp in &SPECS {
        let s = device_vs_tap(&fx, sp, false, None, false);
        s.report(&format!("device {} (ID {})", sp.dev, sp.kind));
        assert!(s.nonzero > 1_000, "device {}: window too quiet to mean anything", sp.dev);
        s.check(&format!("device {}", sp.dev), sp.bound);
    }
}

#[test]
fn adapter_negative_controls_are_caught() {
    let Some(fx) = fixtures() else {
        eprintln!("GLORP_REF_ROOT not set: skipped (this proves nothing)");
        return;
    };
    let fx = std::sync::Arc::new(fx);
    // a flipped reference bit on a bit-exact device (ID 7, dev 1: 100 % exact on its window): the baseline passes the
    // exact bound, the flip does not
    let sp = SPECS.iter().find(|s| s.dev == 1).unwrap();
    device_vs_tap(&fx, sp, false, None, true).check("exact baseline", Bound::Sample(0.0, 0.0));
    let f = fx.clone();
    must_mismatch("flipped bit", std::panic::AssertUnwindSafe(move || {
        device_vs_tap(&f, sp, false, Some(40_000), true).check("flip", Bound::Sample(0.0, 0.0))
    }));
    // ID 35's song automation (P0..P4 move) dropped
    let sp = SPECS.iter().find(|s| s.dev == 27).unwrap();
    let f = fx.clone();
    must_mismatch("ID 35 recorded writes dropped", std::panic::AssertUnwindSafe(move || {
        device_vs_tap(&f, sp, true, None, false).check("drop", sp.bound)
    }));
}

#[test]
fn missing_fixtures_are_a_hard_error() {
    // the loud path, without needing the variable: a path that is not a file is refused with its name
    let r = std::panic::catch_unwind(|| require(Path::new("/nonexistent/glorp-ref/389811-2.bin")));
    let msg = r.err().and_then(|e| e.downcast_ref::<String>().cloned()).unwrap_or_default();
    assert!(msg.contains("GLORP_REF_ROOT is set but the fixture /nonexistent/glorp-ref/389811-2.bin is missing"), "{msg}");
}
