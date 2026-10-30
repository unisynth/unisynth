//! Synthetic only: the song-level refusals (kinds, chunks, settings, rate, automation boxes) and the device fault latch
//! of the `GlorpDevice` registry. Every refusal is checked through the owned `GlorpPlayer` (full diagnostic) **and**
//! through `Player::<GlorpDevice, _>::new` directly (the coarse `LoadError`: no bypass).

use crate::kinds::GlorpDevice;
use crate::player::{GlorpPlayer, NoRng};
use crate::preflight::{Reason, Reject, MARGIN, MAX_AUTOMATED, MAX_DEVICES};
use crate::testsong::{chunk, track, Song as B};
use synth_core::device::{Ctx, Device, Io, LaneLerp};
use synth_core::mix::{Either, Env, Kinds, LoadError, Player, Song};
use synth_core::pool::Pool;

/// A supported, musical parameter set per kind (made up here, not taken from any song).
pub fn good(kind: u8) -> Vec<f32> {
    match kind {
        3 => vec![0.3, 0.9, 0.2, 1.0, 0.5, 0.0, 0.5, 0.5, 0.7, 0.5, 0.1],
        7 => vec![0.0, 0.5, 0.8, 0.3, 0.1, 0.2, 0.5],
        20 => vec![0.3, 0.4, 0.5, 0.6, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.3, 0.7, 0.3, 0.2, 0.5, 0.3],
        21 => vec![0.01, 0.02, 0.5, 0.05, 0.05, 0.4, 0.03, 0.31, 0.2, 0.6, 0.3, 0.8, 0.5],
        22 => vec![0.5, 0.2, 0.3, 0.3, 0.2, 0.01, 0.05, 0.7, 0.05, 0.5, 0.3, 0.3, 0.2, 0.5, 0.2],
        25 => vec![0.5, 0.0, 0.4, 0.3, 0.0, 0.3, 0.4, 0.2, 0.0, 0.6, 0.3, 0.2, 0.3, 0.3, 0.1],
        14 => vec![0.0, 0.1, 0.3, 0.2, 0.2, 0.0, 0.2, 0.4, 0.25, 0.3, 0.5, 1.0, 0.5, 0.8, 0.5, 0.3, 0.5, 0.3, 0.8, 0.2, 0.05, 0.5, 1.0, 1.0, 0.0, 0.0],
        28 => vec![0.75, 0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.002, 0.05, 0.7, 0.05, 0.8, 0.1, 0.2, 0.5, 0.0, 0.002, 0.05, 0.6, 0.05, 0.6, 0.2, 0.5, 0.0],
        30 => vec![0.1, 0.5, 0.5, 0.7, 0.5, 0.8, 0.05, 0.5, 0.4, 0.2, 0.2, 0.0, 0.6, 0.4, 0.5, 0.0, 0.3, 0.3, 0.7, 0.2, 0.0, 0.3, 0.4, 1.0],
        27 => vec![0.1, 0.86, 0.534, 0.034, 0.252, 0.336, 0.725, 0.5, 0.045, 0.286, 0.252, 0.336, 0.725],
        32 => vec![0.45, 0.3, 0.0, 0.1, 0.0, 0.3, 0.5, 0.0, 0.5, 0.4, 1.0],
        34 => vec![0.5, 0.5, 1.0, 0.3],
        35 => vec![0.3, 0.5, 0.7, 0.2, 0.5, 1.0],
        38 => vec![0.0, 0.6, 0.1, 0.5, 0.0, 0.0, 0.0, 0.2, 0.1, 0.1, 0.1, 0.1, 0.5, 1.0, 0.5],
        _ => panic!("no parameters for kind {kind}"),
    }
}

pub const KINDS: [u8; 14] = [3, 7, 14, 20, 21, 22, 25, 27, 28, 30, 32, 34, 35, 38];

/// One device on one master track.
pub fn single(kind: u8, params: &[f32]) -> B {
    let mut s = B::new();
    s.device(kind, params);
    s.lanes.push(vec![]);
    s.tracks.push(track(&[0], 0));
    s
}

/// Runs `f` on a thread with a stack big enough for the 200–300 KB devices in a debug build.
pub fn big<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    std::thread::Builder::new().stack_size(256 << 20).spawn(f).unwrap().join().unwrap()
}

/// What the owned API says about a song (`required_pool` runs parse + preflight).
fn verdict(s: &B) -> Result<(), Reject> {
    let b = s.bytes();
    GlorpPlayer::required_pool(&b).map(|_| ())
}

/// The coarse answer of the core API for the same bytes (preflight inside `load_devices`, `BLOCK` pinned).
fn core_verdict(s: &B, sub: usize) -> Result<(), LoadError> {
    big({
        let b = s.bytes();
        move || {
            let song = Song::parse(&b).unwrap();
            let env = Env::for_song(&song, None);
            let mut mem = vec![0u8; 8 << 20];
            let mut pool = Pool::new(&mut mem);
            Player::<GlorpDevice, NoRng>::new(song, &env, &mut pool, NoRng, sub).map(|_| ())
        }
    })
}

fn refused(s: &B) -> Reject {
    verdict(s).err().unwrap_or_else(|| panic!("song was accepted"))
}

#[test]
fn supported_songs_pass_both_doors() {
    for k in KINDS {
        let s = single(k, &good(k));
        assert_eq!(verdict(&s), Ok(()), "kind {k}");
        assert_eq!(core_verdict(&s, 128), Ok(()), "kind {k}");
    }
}

#[test]
fn unsupported_ids_are_refused_by_every_door() {
    // the unimplemented Glorp ids and every foreign / unknown id
    for id in [0u8, 1, 2, 4, 5, 6, 8, 12, 31, 33, 39, 64, 255] {
        let s = single(id, &[0.5; 4]);
        let r = refused(&s);
        assert_eq!((r.device, r.kind, r.reason), (Some(0), Some(id), Reason::UnsupportedKind), "id {id}");
        assert_eq!(core_verdict(&s, 128), Err(LoadError::UnsupportedKind(id)), "id {id}");
    }
}

#[test]
fn one_unsupported_device_refuses_the_whole_song_and_names_it() {
    let mut s = single(38, &good(38));
    s.device(3, &good(3));
    s.device(31, &[0.5; 26]);
    s.tracks[0].devs = vec![0, 1, 2];
    let r = refused(&s);
    assert_eq!((r.device, r.kind), (Some(2), Some(31)));
}

#[test]
fn the_chunk_layout_is_exact() {
    for k in KINDS {
        let g = good(k);
        let n = g.len();
        let c = chunk(&g);
        assert_eq!(c.len(), 4 + 4 * n);
        let mut cases: Vec<(&str, Vec<u8>, Reason)> = vec![];
        cases.push(("empty", vec![], Reason::ChunkLength));
        cases.push(("size field only", 4u32.to_le_bytes().to_vec(), Reason::ChunkLength));
        cases.push(("one parameter short", chunk(&g[..n - 1]), Reason::ChunkLength));
        let mut long = g.clone();
        long.push(0.5);
        cases.push(("one parameter long (a voice-mode extension would look like this)", chunk(&long), Reason::ChunkLength));
        let mut odd = c.clone();
        odd.push(0);
        cases.push(("one stray byte", odd, Reason::ChunkLength));
        for (name, bad_size) in [("size field 0", 0u32), ("size field len+4", c.len() as u32 + 4), ("size field n*4", 4 * n as u32)] {
            let mut x = c.clone();
            let at = x.len() - 4;
            x[at..].copy_from_slice(&bad_size.to_le_bytes());
            cases.push((name, x, Reason::ChunkSize));
        }
        for (name, bytes, reason) in cases {
            let mut s = B::new();
            s.devices.push((k, bytes));
            s.lanes.push(vec![]);
            s.tracks.push(track(&[0], 0));
            let r = refused(&s);
            assert_eq!((r.device, r.kind, r.reason), (Some(0), Some(k), reason), "kind {k}: {name}");
            let want = LoadError::BadChunk;
            assert_eq!(core_verdict(&s, 128), Err(want), "kind {k}: {name}");
        }
    }
}

#[test]
fn non_finite_or_out_of_range_parameters_are_refused_with_their_index() {
    for k in KINDS {
        let g = good(k);
        for i in 0..g.len() {
            for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 1.1, -1e-30, 1.0000001] {
                let mut p = g.clone();
                p[i] = bad;
                let r = refused(&single(k, &p));
                assert_eq!((r.kind, r.param, r.reason), (Some(k), Some(i as u8), Reason::Range), "kind {k} P{i} = {bad}");
            }
        }
    }
}

/// The ID 22 refusal text (weight-sum overflow of the 3D map).
const W22: &str = "3D map weight sum overflows (small P2 with large P4)";

#[test]
fn unsupported_settings_are_named() {
    let mode = "filter mode other than low-pass / high-pass";
    let table: &[(u8, usize, f32, Option<u8>, &str)] = &[
        (38, 0, 0.5, Some(0), mode),
        (38, 0, 1.0, Some(0), mode),
        (38, 0, 0.1667, Some(0), mode),
        (38, 4, 0.5, Some(4), "more than one filter stage"),
        (38, 5, 0.5, Some(5), "pre distortion"),
        (38, 6, 0.9, Some(6), "post distortion type above 2"),
        (38, 12, 0.6, Some(12), "envelope amount other than neutral"),
        (38, 12, 0.4999, Some(12), "envelope amount other than neutral"),
        (35, 5, 0.0, Some(5), "all-pass topology other than second-order"),
        (35, 5, 0.5, Some(5), "all-pass topology other than second-order"),
        (35, 5, 0.6668, Some(5), "all-pass topology other than second-order"),
        (21, 6, 1.0, None, "more than 512 partials per voice"),
        (21, 4, 1.0, None, "more than 512 partials per voice"),
        (22, 4, 1.0, Some(2), W22),
        (32, 2, 0.9, None, "mode A delay exceeds the delay line"),
        (32, 4, 1.0, None, "mode A delay exceeds the delay line"),
    ];
    for &(k, i, v, param, what) in table {
        let mut p = good(k);
        if k == 32 && i == 4 {
            p[2] = 0.45; // 0.45 with depth 1 fits; the full depth does not
        }
        if k == 22 {
            p[2] = 0.0; // a = 0.002 (500^n weights): n = 16 overflows
        }
        if k == 21 && i == 4 {
            p[6] = 0.06; // 101 x 7 = 707 partials (P4 alone fits with the default P6)
        }
        p[i] = v;
        let r = refused(&single(k, &p));
        assert_eq!((r.device, r.kind, r.param, r.reason), (Some(0), Some(k), param, Reason::Setting(what)), "kind {k} P{i} = {v}");
        assert_eq!(core_verdict(&single(k, &p), 128), Err(LoadError::UnsupportedModule(k)));
    }
    // the edges of the supported side
    for (k, i, v) in [(38, 0, 0.1666), (38, 0, 0.0), (35, 5, 0.667), (35, 5, 1.0), (38, 6, 0.4999), (38, 5, 0.1666)] {
        let mut p = good(k);
        p[i] = v;
        assert_eq!(verdict(&single(k, &p)), Ok(()), "kind {k} P{i} = {v}");
    }
    // mode B has no delay-line limit
    let mut p = good(32);
    p[7] = 1.0;
    p[2] = 1.0;
    p[4] = 1.0;
    assert_eq!(verdict(&single(32, &p)), Ok(()));
}

#[test]
fn another_sample_rate_is_refused() {
    let mut s = single(3, &good(3));
    s.rate = 48_000;
    let r = refused(&s);
    assert_eq!((r.device, r.reason), (None, Reason::Rate));
    assert_eq!(core_verdict(&s, 128), Err(LoadError::UnsupportedModule(0)));
    // a render context at another rate: Env as well
    let b = single(3, &good(3)).bytes();
    let song = Song::parse(&b).unwrap();
    let env = Env { rate: 48_000.0, tempo: 174, bank: None };
    assert_eq!(<GlorpDevice as Kinds>::preflight(&song, &env), Err(LoadError::UnsupportedModule(0)));
    assert_eq!(<GlorpDevice as Kinds>::need(3, &chunk(&good(3)), &env), Err(LoadError::UnsupportedModule(3)));
}

#[test]
fn only_the_128_frame_block_is_accepted() {
    let s = single(34, &good(34));
    for sub in [1usize, 100, 127, 129, 256, 4096] {
        assert_eq!(core_verdict(&s, sub), Err(LoadError::Malformed), "sub {sub}");
    }
    assert_eq!(core_verdict(&s, 128), Ok(()));
}

// ---- the block pin follows the song's actual owner (`Either`) ----

/// A foreign engine that owns kind 99 only; `PIN` = 0 means any block size, else it needs `PIN`-frame blocks.
struct Foreign<const PIN: usize>;
impl<const PIN: usize> Device for Foreign<PIN> {
    fn param_count(&self) -> usize {
        1
    }
    fn set_param(&mut self, _: usize, _: f32) {}
    fn reset(&mut self) {}
    fn run(&mut self, _: &mut Io, _: usize, _: &mut Ctx) {}
}
impl<'a, const PIN: usize> Kinds<'a> for Foreign<PIN> {
    fn block(song: &Song<'a>, _: &Env<'a>) -> Result<Option<usize>, LoadError> {
        Ok((PIN != 0 && song.devices().any(|d| d.kind == 99)).then_some(PIN))
    }
    fn need(kind: u8, _: &'a [u8], _: &Env<'a>) -> Result<usize, LoadError> {
        if kind == 99 { Ok(0) } else { Err(LoadError::UnsupportedKind(kind)) }
    }
    fn load(kind: u8, chunk: &'a [u8], env: &Env<'a>, _: &mut Pool<'a>, _: &mut dyn synth_core::rng::Rng) -> Result<Self, LoadError> {
        Self::need(kind, chunk, env).map(|_| Foreign)
    }
}

fn foreign_only() -> B {
    single(99, &[0.5; 4])
}

fn mixed() -> B {
    let mut s = single(3, &good(3));
    s.device(99, &[0.5; 4]);
    s.tracks[0].devs = vec![0, 1];
    s
}

/// `Player::new` of any registry type (the core API, the coarse `LoadError`); a macro because `GlorpDevice<'a>` borrows
/// the song bytes, so the type cannot be a generic parameter for every lifetime.
macro_rules! verdict_of {
    ($k:ty, $s:expr, $sub:expr) => {
        big({
            let b = $s.bytes();
            let sub = $sub;
            move || {
                let song = Song::parse(&b).unwrap();
                let env = Env::for_song(&song, None);
                let mut mem = vec![0u8; 8 << 20];
                let mut pool = Pool::new(&mut mem);
                Player::<$k, NoRng>::new(song, &env, &mut pool, NoRng, sub).map(|_| ())
            }
        })
    };
}

#[test]
fn a_foreign_only_song_in_an_either_is_not_pinned_to_128() {
    for sub in [1usize, 100, 127, 128, 129, 4096] {
        assert_eq!(verdict_of!(Either<Foreign<0>, GlorpDevice>, foreign_only(), sub), Ok(()), "sub {sub}");
        assert_eq!(verdict_of!(Either<GlorpDevice, Foreign<0>>, foreign_only(), sub), Ok(()), "sub {sub}");
    }
}

#[test]
fn an_owned_device_in_an_either_pins_128_alone_or_mixed() {
    for s in [single(34, &good(34)), mixed()] {
        for sub in [1usize, 100, 127, 129, 4096] {
            assert_eq!(verdict_of!(Either<Foreign<0>, GlorpDevice>, s, sub), Err(LoadError::Malformed), "sub {sub}");
            assert_eq!(verdict_of!(Either<GlorpDevice, Foreign<0>>, s, sub), Err(LoadError::Malformed), "sub {sub}");
        }
    }
    assert_eq!(verdict_of!(Either<Foreign<0>, GlorpDevice>, single(34, &good(34)), 128), Ok(()));
    assert_eq!(verdict_of!(Either<GlorpDevice, Foreign<0>>, mixed(), 128), Ok(()));
}

#[test]
fn glorp_alone_cannot_bypass_the_128_pin_and_still_refuses_foreign_kinds() {
    for k in KINDS {
        let s = single(k, &good(k));
        for sub in [1usize, 64, 100, 127, 129, 512] {
            assert_eq!(core_verdict(&s, sub), Err(LoadError::Malformed), "kind {k} sub {sub}");
        }
        assert_eq!(core_verdict(&s, 128), Ok(()), "kind {k}");
    }
    // no owned device: the pin is not ours, but the kind is still refused (by `need`, at every sub-block)
    for sub in [100usize, 128] {
        assert_eq!(core_verdict(&foreign_only(), sub), Err(LoadError::UnsupportedKind(99)), "sub {sub}");
        assert_eq!(core_verdict(&mixed(), sub), if sub == 128 { Err(LoadError::UnsupportedKind(99)) } else { Err(LoadError::Malformed) });
    }
    // the owned door is strict about sub-block-independent refusals too
    let r = refused(&foreign_only());
    assert_eq!((r.device, r.kind, r.reason), (Some(0), Some(99), Reason::UnsupportedKind));
    assert_eq!(verdict(&single(34, &good(34))), Ok(()));
}

#[test]
fn two_owners_bound_to_different_blocks_are_refused_by_either() {
    for sub in [64usize, 100, 128] {
        assert_eq!(verdict_of!(Either<Foreign<64>, GlorpDevice>, mixed(), sub), Err(LoadError::Malformed), "sub {sub}");
        assert_eq!(verdict_of!(Either<GlorpDevice, Foreign<64>>, mixed(), sub), Err(LoadError::Malformed), "sub {sub}");
    }
    // each owner alone keeps its own requirement
    assert_eq!(verdict_of!(Either<Foreign<64>, GlorpDevice>, foreign_only(), 64), Ok(()));
    assert_eq!(verdict_of!(Either<Foreign<64>, GlorpDevice>, foreign_only(), 128), Err(LoadError::Malformed));
    assert_eq!(verdict_of!(Either<Foreign<64>, GlorpDevice>, single(34, &good(34)), 128), Ok(()));
    assert_eq!(verdict_of!(Either<Foreign<64>, GlorpDevice>, single(34, &good(34)), 64), Err(LoadError::Malformed));
    // agreeing pins are not a conflict
    assert_eq!(verdict_of!(Either<Foreign<128>, GlorpDevice>, mixed(), 128), Ok(()));
}

// ---- automation ----

/// `lanes`: (device index in the track, parameter, points as (delta, byte)).
fn with_lanes(kind: u8, params: &[f32], lanes: &[(i32, i32, &[(i32, u8)])]) -> B {
    let mut s = single(kind, params);
    for &(d, p, pts) in lanes {
        s.tracks[0].autos.push((d, p, pts.to_vec()));
    }
    s
}

#[test]
fn a_lane_that_interpolates_through_an_unsupported_setting_is_refused() {
    let mode = "filter mode other than low-pass / high-pass";
    // ID 38 P0: the mode is trunc(P0·11.999) ≤ 1 only below 2/11.999 = 0.16668 (byte 42 = 0.1647, 43 = 0.1686)
    let lane = |pts: &[(i32, u8)]| with_lanes(38, &good(38), &[(0, 0, pts)]);
    assert_eq!(verdict(&lane(&[(0, 0), (1000, 42)])), Ok(()));
    for pts in [&[(0, 0), (1000, 43)][..], &[(0, 0), (1000, 255)], &[(0, 0), (100, 255), (0, 0)], &[(0, 255)]] {
        let r = refused(&lane(pts));
        assert_eq!((r.device, r.kind, r.param, r.reason), (Some(0), Some(38), Some(0), Reason::Automation(mode)), "{pts:?}");
        // the same lane through the core API: a coarse refusal, no device was built
        assert_eq!(core_verdict(&lane(pts), 128), Err(LoadError::UnsupportedModule(38)));
    }
    // post distortion type: trunc(P6·5.999) ≤ 2 up to byte 127 (0.498), byte 128 = 0.502 → 3
    let post = |hi: u8| with_lanes(38, &good(38), &[(0, 6, &[(0, 0), (500, hi)])]);
    assert_eq!(verdict(&post(127)), Ok(()));
    assert_eq!(refused(&post(128)).reason, Reason::Automation("post distortion type above 2"));
    // stages and pre distortion start at 0 and any step up is refused
    for (p, what) in [(4, "more than one filter stage"), (5, "pre distortion")] {
        let r = refused(&with_lanes(38, &good(38), &[(0, p, &[(0, 0), (500, 255)])]));
        assert_eq!((r.param, r.reason), (Some(p as u8), Reason::Automation(what)));
    }
    // envelope amount must be exactly 0.5, which no byte/255 lane value is, and interpolation would not hold it
    for pts in [&[(0, 127), (500, 128)][..], &[(0, 128)], &[(0, 127)]] {
        let r = refused(&with_lanes(38, &good(38), &[(0, 12, pts)]));
        assert_eq!((r.param, r.reason), (Some(12), Reason::Automation("envelope amount other than neutral")), "{pts:?}");
    }
    // ID 35 topology: P5 ≥ 0.6669 (byte 171 = 0.6706; byte 170 = 0.6667 is first-order)
    let topo = |lo: u8| with_lanes(35, &good(35), &[(0, 5, &[(0, 255), (1000, lo)])]);
    assert_eq!(verdict(&topo(171)), Ok(()));
    assert_eq!(refused(&topo(170)).reason, Reason::Automation("all-pass topology other than second-order"));
}

#[test]
fn a_value_within_the_interpolation_margin_of_an_edge_is_not_automated() {
    // P0 = 0.166675 is still a low-pass / high-pass mode (edge 2/11.999 = 0.1666806), but the player's interpolation can
    // land a few ulp (and the margin is 1e-5) beyond a written value: automating that parameter is refused, leaving it
    // constant is fine.
    let mut p = good(38);
    p[0] = 0.166_675;
    assert_eq!(verdict(&single(38, &p)), Ok(()));
    assert_eq!(verdict(&with_lanes(38, &p, &[(0, 3, &[(0, 10), (100, 200)])])), Ok(()));
    let r = refused(&with_lanes(38, &p, &[(0, 0, &[(0, 0)])]));
    assert_eq!((r.param, r.reason), (Some(0), Reason::Automation("filter mode other than low-pass / high-pass")));
}

#[test]
fn id32_automation_is_judged_on_the_worst_corner_not_per_lane() {
    let delay = "mode A delay exceeds the delay line";
    // P2 = 0 and P4 = 0 in the chunk. P2 up to byte 115 (0.451) with P4 = 0: 1706 samples, fits.
    let p2 = (0, 2, &[(0, 0), (4000, 115)][..]);
    // P4 up to 1.0 with P2 = 0: 88 · 1.8 samples, fits.
    let p4 = (0, 4, &[(0, 0), (4000, 255)][..]);
    assert_eq!(verdict(&with_lanes(32, &good(32), &[p2])), Ok(()));
    assert_eq!(verdict(&with_lanes(32, &good(32), &[p4])), Ok(()));
    // together some interleaving of the two writes reaches 1706 · 1.8 > 2200: refused whatever the lane order
    for lanes in [[p2, p4], [p4, p2]] {
        let r = refused(&with_lanes(32, &good(32), &lanes));
        assert_eq!((r.device, r.kind, r.reason), (Some(0), Some(32), Reason::Automation(delay)));
    }
    // a lane beyond the limit on its own
    assert_eq!(refused(&with_lanes(32, &good(32), &[(0, 2, &[(0, 0), (4000, 140)])])).reason, Reason::Automation(delay));
    // the chunk's own value counts: P2 = 0.45 in the chunk, P4 lane up: refused
    let mut g = good(32);
    g[2] = 0.45;
    assert_eq!(refused(&with_lanes(32, &g, &[p4])).reason, Reason::Automation(delay));
}

#[test]
fn id32_lane_may_not_cross_the_mode_boundary() {
    let cross = "lane crosses the mode A / B boundary";
    // chunk in mode A: a lane to mode B crosses, one within A does not
    assert_eq!(refused(&with_lanes(32, &good(32), &[(0, 7, &[(0, 0), (900, 255)])])).reason, Reason::Automation(cross));
    assert_eq!(refused(&with_lanes(32, &good(32), &[(0, 7, &[(0, 130)])])).reason, Reason::Automation(cross));
    assert_eq!(verdict(&with_lanes(32, &good(32), &[(0, 7, &[(0, 0), (900, 120)])])), Ok(()));
    // chunk in mode B: a lane staying in B is fine, one into A crosses
    let mut g = good(32);
    g[7] = 1.0;
    assert_eq!(verdict(&with_lanes(32, &g, &[(0, 7, &[(0, 255), (900, 130)])])), Ok(()));
    assert_eq!(refused(&with_lanes(32, &g, &[(0, 7, &[(0, 255), (900, 100)])])).reason, Reason::Automation(cross));
    // mode B with long delays: no limit, even when P2 / P4 move
    g[2] = 1.0;
    assert_eq!(verdict(&with_lanes(32, &g, &[(0, 4, &[(0, 0), (900, 255)])])), Ok(()));
}

#[test]
fn kinds_without_unsupported_settings_take_any_lane() {
    for k in [3u8, 7, 20, 25, 27, 28, 34] {
        let n = good(k).len();
        let pts: &[(i32, u8)] = &[(0, 0), (700, 255), (0, 3), (200, 128), (100, 255), (50, 0)];
        let lanes: Vec<_> = (0..n as i32).map(|p| (0, p, pts)).collect();
        assert_eq!(verdict(&with_lanes(k, &good(k), &lanes)), Ok(()), "kind {k}");
    }
    // lanes with no points write nothing
    assert_eq!(verdict(&with_lanes(38, &good(38), &[(0, 12, &[]), (0, 0, &[])])), Ok(()));
}

#[test]
fn lane_targets_must_exist() {
    let r = refused(&with_lanes(38, &good(38), &[(0, 15, &[(0, 1)])]));
    assert_eq!((r.device, r.kind, r.param, r.reason), (Some(0), Some(38), Some(15), Reason::LaneParam));
    assert_eq!(core_verdict(&with_lanes(38, &good(38), &[(0, 15, &[(0, 1)])]), 128), Err(LoadError::BadIndex));
    assert_eq!(refused(&with_lanes(34, &good(34), &[(0, 4, &[(0, 1)])])).reason, Reason::LaneParam);
    // a lane on a device index the track does not have never gets as far as the preflight: the song does not parse
    for d in [1, -1] {
        let r = refused(&with_lanes(34, &good(34), &[(d, 0, &[(0, 1)])]));
        assert_eq!(r.reason, Reason::Load(LoadError::BadIndex), "device {d}");
    }
}

#[test]
fn too_many_automated_devices_are_refused_not_skipped() {
    let build = |n: usize| {
        let mut s = B::new();
        let mut t = track(&[], 0);
        for i in 0..n {
            s.device(34, &good(34));
            t.devs.push(i as i32);
            t.autos.push((i as i32, 0, vec![(0, 10)]));
        }
        s.lanes.push(vec![]);
        s.tracks.push(t);
        s
    };
    assert_eq!(verdict(&build(MAX_AUTOMATED)), Ok(()));
    let r = refused(&build(MAX_AUTOMATED + 1));
    assert_eq!((r.device, r.kind, r.reason), (Some(MAX_AUTOMATED), Some(34), Reason::TooManyAutomated));
}

#[test]
fn an_untrusted_device_count_cannot_decide_the_allocation() {
    let build = |n: usize| {
        let mut s = B::new();
        let mut t = track(&[], 0);
        for i in 0..n {
            s.device(34, &good(34));
            t.devs.push(i as i32);
        }
        s.lanes.push(vec![]);
        s.tracks.push(t);
        s
    };
    assert_eq!(verdict(&build(MAX_DEVICES)), Ok(()));
    let r = refused(&build(MAX_DEVICES + 1));
    assert_eq!((r.device, r.reason), (None, Reason::TooManyDevices));
    assert_eq!(core_verdict(&build(MAX_DEVICES + 1), 128), Err(LoadError::UnsupportedModule(0)));
}

#[test]
fn lanes_of_one_device_on_two_tracks_are_one_box() {
    // the same ID 32 device listed on two tracks, each with one lane: P2 and P4 together reach the bad corner
    let mut s = B::new();
    s.device(32, &good(32));
    s.lanes.push(vec![]);
    let mut a = track(&[0], 0);
    a.autos.push((0, 2, vec![(0, 0), (4000, 115)]));
    let mut b = track(&[0], 0);
    b.autos.push((0, 4, vec![(0, 0), (4000, 255)]));
    s.tracks.push(a);
    s.tracks.push(b);
    assert_eq!(refused(&s).reason, Reason::Automation("mode A delay exceeds the delay line"));
}

/// ID 21: the memory bound (512 partials per voice) is judged on the corner of the automation box, not per lane.
#[test]
fn id21_lanes_are_judged_on_the_partial_count_corner() {
    let what = "more than 512 partials per voice";
    // P4 alone to 1.0: 101 x 4 partials fit; P6 alone to 1.0: 6 x 101 = 606 do not
    assert_eq!(verdict(&with_lanes(21, &good(21), &[(0, 4, &[(0, 0), (500, 255)])])), Ok(()));
    let r = refused(&with_lanes(21, &good(21), &[(0, 6, &[(0, 0), (500, 255)])]));
    assert_eq!((r.device, r.kind, r.param, r.reason), (Some(0), Some(21), None, Reason::Automation(what)));
    assert_eq!(core_verdict(&with_lanes(21, &good(21), &[(0, 6, &[(0, 0), (500, 255)])]), 128), Err(LoadError::UnsupportedModule(21)));
    // two lanes that fit alone but not together (101 x 16 = 1616): each vector the player can write is fine one lane at a time
    // only when the other stays low, so the box corner decides
    let both = with_lanes(21, &good(21), &[(0, 4, &[(0, 0), (500, 255)]), (0, 6, &[(0, 0), (500, 38)])]);
    assert_eq!(refused(&both).reason, Reason::Automation(what));
    // the bound itself: 32 x 16 = 512 partials (bytes 79 = 0.3098 -> 31, 38 = 0.149 -> 14 -> 15 x 32) fit, one more does not
    let fits = with_lanes(21, &good(21), &[(0, 4, &[(0, 0), (500, 79)]), (0, 6, &[(0, 0), (500, 38)])]);
    assert_eq!(verdict(&fits), Ok(()));
    let over = with_lanes(21, &good(21), &[(0, 4, &[(0, 0), (500, 80)]), (0, 6, &[(0, 0), (500, 42)])]);
    assert_eq!(refused(&over).reason, Reason::Automation(what));
    // every other parameter takes any lane (they only seed or scale a note)
    for p in [0, 1, 2, 3, 5, 7, 8, 9, 10, 11, 12] {
        assert_eq!(verdict(&with_lanes(21, &good(21), &[(0, p, &[(0, 0), (700, 255), (0, 3), (200, 128)])])), Ok(()), "P{p}");
    }
}

/// ID 22: the weight sum `Σ (10·P2 + 0.002)^-k` of the 3D map is judged on the corner (lowest P2, highest P4) of the box.
#[test]
fn id22_lanes_are_judged_on_the_weight_corner() {
    let at = |p2: f32, p4: f32| {
        let mut p = good(22);
        p[2] = p2;
        p[4] = p4;
        p
    };
    // the initial chunk: P2 = 0 with n = 14 (byte-ish 0.92) is the last finite one, n = 15 is not (500^15 overflows)
    assert_eq!(verdict(&single(22, &at(0.0, 0.92))), Ok(()));
    let r = refused(&single(22, &at(0.0, 0.94)));
    assert_eq!((r.device, r.kind, r.param, r.reason), (Some(0), Some(22), Some(2), Reason::Setting(W22)));
    // a ratio above 1 never overflows
    assert_eq!(verdict(&single(22, &at(1.0, 1.0))), Ok(()));
    // one lane alone, the other at its safe chunk value: P2 down to 0 with n = 4, P4 up to 1 with a = 3.002
    assert_eq!(verdict(&with_lanes(22, &at(0.3, 0.2), &[(0, 2, &[(0, 255), (500, 0)])])), Ok(()));
    assert_eq!(verdict(&with_lanes(22, &at(0.3, 0.2), &[(0, 4, &[(0, 0), (500, 255)])])), Ok(()));
    // P2 lane down to 0 against a chunk P4 that already needs headroom, and the mirror image
    let r = refused(&with_lanes(22, &at(0.3, 0.94), &[(0, 2, &[(0, 255), (500, 0)])]));
    assert_eq!((r.device, r.kind, r.param, r.reason), (Some(0), Some(22), Some(2), Reason::Automation(W22)));
    let r = refused(&with_lanes(22, &at(0.0, 0.2), &[(0, 4, &[(0, 0), (500, 255)])]));
    assert_eq!((r.kind, r.reason), (Some(22), Reason::Automation(W22)));
    assert_eq!(core_verdict(&with_lanes(22, &at(0.0, 0.2), &[(0, 4, &[(0, 0), (500, 255)])]), 128), Err(LoadError::UnsupportedModule(22)));
    // two lanes that are fine alone but not together: the corner decides (each write alone could pass, the pair may not)
    let both = with_lanes(22, &at(0.3, 0.2), &[(0, 2, &[(0, 255), (500, 0)]), (0, 4, &[(0, 0), (500, 255)])]);
    assert_eq!(refused(&both).reason, Reason::Automation(W22));
    // the order of the points and of the lanes does not matter, a lane that touches the corner only in the middle too
    let mid = with_lanes(22, &at(0.3, 0.2), &[(0, 4, &[(0, 255), (250, 0), (500, 100)]), (0, 2, &[(0, 255), (100, 0), (500, 255)])]);
    assert_eq!(refused(&mid).reason, Reason::Automation(W22));
    // the bound itself on the lanes: bytes 235 = 0.9216 -> n = 14 fits at P2 = 0, 240 = 0.941 -> n = 15 does not
    let edge = |hi: u8| with_lanes(22, &at(0.0, 0.2), &[(0, 4, &[(0, 0), (500, hi)])]);
    assert_eq!(verdict(&edge(235)), Ok(()));
    assert_eq!(refused(&edge(240)).reason, Reason::Automation(W22));
    // a ratio lane that stays above the overflow for the chunk's iteration count passes: n = 16 needs a >= ~0.0043 (P2 >= 4e-4)
    let ratio = |lo: u8| with_lanes(22, &at(0.5, 1.0), &[(0, 2, &[(0, 255), (500, lo)])]);
    assert_eq!(verdict(&ratio(1)), Ok(())); // P2 = 0.0039: a = 0.041
    assert_eq!(refused(&ratio(0)).reason, Reason::Automation(W22));
    // every other parameter takes any lane
    for p in [0, 1, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14] {
        assert_eq!(verdict(&with_lanes(22, &good(22), &[(0, p, &[(0, 0), (700, 255), (0, 3), (200, 128)])])), Ok(()), "P{p}");
    }
}

#[test]
fn id22_hand_writes_beyond_the_weight_guard_latch_a_fault() {
    big(|| {
        let mut m = vec![];
        let mut p = good(22);
        p[2] = 0.0;
        p[4] = 0.9; // n = 14: finite
        let mut d = device(22, &p, &mut m);
        d.set_param(4, 0.95); // n = 15 at a = 0.002: NaN in the original, never forwarded
        assert_eq!(d.fault(), Some(W22));
        d.note_on(60, 100, 0);
        let (l, r) = run(&mut d, 0.0, 128, 44100.0);
        assert!(l.iter().chain(&r).all(|&x| x == 0.0), "a faulted instrument is silent, not NaN");
    });
}

// ---- the device fault latch (a Device driven by hand cannot render a setting it does not model) ----

fn device(kind: u8, params: &[f32], mem: &mut Vec<u8>) -> GlorpDevice<'static> {
    let env = Env { rate: 44100.0, tempo: 174, bank: None };
    let c: &'static [u8] = Box::leak(chunk(params).into_boxed_slice());
    *mem = vec![0u8; <GlorpDevice as Kinds>::need(kind, c, &env).unwrap() + 64];
    let m: &'static mut [u8] = Box::leak(std::mem::take(mem).into_boxed_slice());
    let mut pool = Pool::new(m);
    <GlorpDevice as Kinds>::load(kind, c, &env, &mut pool, &mut NoRng).unwrap()
}

fn run(d: &mut GlorpDevice, input: f32, n: usize, rate: f64) -> (Vec<f32>, Vec<f32>) {
    let (mut l, mut r) = (vec![input; n.max(128)], vec![input; n.max(128)]);
    let (mut sl, mut sr) = (vec![0.0f32; n.max(128)], vec![0.0f32; n.max(128)]);
    let mut rng = NoRng;
    let mut ctx = Ctx { rate, tempo: 174, frame: 0, rng: &mut rng };
    d.run(&mut Io { ch: [&mut l, &mut r, &mut sl, &mut sr] }, n, &mut ctx);
    (l, r)
}

#[test]
fn a_refused_write_latches_a_fault_and_mutes_for_good() {
    big(|| {
        let mut m = vec![];
        let mut d = device(38, &good(38), &mut m);
        let (l, _) = run(&mut d, 0.5, 128, 44100.0);
        assert!(l.iter().any(|&x| x != 0.0) && d.fault().is_none());
        // in range, but a filter mode the device does not model: not forwarded, latched
        d.set_param(0, 0.5);
        assert_eq!(d.fault(), Some("filter mode other than low-pass / high-pass"));
        let (l, r) = run(&mut d, 0.5, 128, 44100.0);
        assert!(l.iter().chain(&r).all(|&x| x == 0.0), "a faulted device must not pass audio");
        // the first fault stays; reset and a later valid write do not revive it
        d.set_param(0, 0.0);
        d.reset();
        assert_eq!(d.fault(), Some("filter mode other than low-pass / high-pass"));
        let (l, _) = run(&mut d, 0.5, 128, 44100.0);
        assert!(l.iter().all(|&x| x == 0.0));
    });
}

#[test]
fn bad_writes_all_latch() {
    big(|| {
        for (i, v) in [(0usize, f32::NAN), (0, f32::INFINITY), (0, 1.5), (0, -0.5), (99, 0.5), (11, 0.5)] {
            let mut m = vec![];
            let mut d = device(3, &good(3), &mut m);
            d.set_param(i, v);
            assert!(d.fault().is_some(), "write P{i} = {v}");
        }
        // an interpolation that lands an ulp outside the range is clamped, not a fault, and acts as the edge value
        let mut m = vec![];
        let mut a = device(3, &good(3), &mut m);
        let mut b = device(3, &good(3), &mut m);
        a.set_param(0, 1.0 + 1e-7);
        b.set_param(0, 1.0);
        assert!(a.fault().is_none());
        let (ya, yb) = (run(&mut a, 0.3, 128, 44100.0), run(&mut b, 0.3, 128, 44100.0));
        assert_eq!(ya.0.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), yb.0.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
        let mut c = device(3, &good(3), &mut m);
        c.set_param(1, -1e-7);
        assert!(c.fault().is_none());
    });
}

#[test]
fn another_block_size_or_rate_latches_instead_of_rendering() {
    big(|| {
        for n in [1usize, 100, 127, 129, 256] {
            let mut m = vec![];
            let mut d = device(34, &good(34), &mut m);
            let (l, _) = run(&mut d, 0.5, n, 44100.0);
            assert_eq!(d.fault(), Some("render call is not 128 frames"), "n = {n}");
            assert!(l[..n].iter().all(|&x| x == 0.0));
        }
        let mut m = vec![];
        let mut d = device(34, &good(34), &mut m);
        let (l, _) = run(&mut d, 0.5, 128, 48000.0);
        assert_eq!(d.fault(), Some("render rate is not 44100"));
        assert!(l.iter().all(|&x| x == 0.0));
    });
}

#[test]
fn effects_ignore_notes_and_instruments_take_them() {
    big(|| {
        let mut m = vec![];
        let mut fx = device(38, &good(38), &mut m);
        let mut reference = device(38, &good(38), &mut m);
        fx.note_on(60, 100, 5);
        fx.note_off(60, 9);
        assert!(fx.fault().is_none());
        let (a, b) = (run(&mut fx, 0.4, 128, 44100.0), run(&mut reference, 0.4, 128, 44100.0));
        assert_eq!(a.0, b.0);
        let mut inst = device(25, &good(25), &mut m);
        assert!(run(&mut inst, 0.0, 128, 44100.0).0.iter().all(|&x| x == 0.0));
        inst.note_on(40, 90, 0);
        let (l, _) = run(&mut inst, 0.0, 128, 44100.0);
        assert!(l.iter().any(|&x| x != 0.0) && inst.fault().is_none());
        // a full queue loses the event as the original does: no fault
        for _ in 0..300 {
            inst.note_on(41, 90, 100_000);
        }
        assert!(inst.fault().is_none());
    });
}

#[test]
fn reset_equals_a_fresh_device_and_keeps_the_parameters() {
    big(|| {
        for k in KINDS {
            let mut m = vec![];
            let mut fresh = device(k, &good(k), &mut m);
            let mut used = device(k, &good(k), &mut m);
            // drive `used` with notes, audio and a parameter change, then reset it
            let n = used.param_count();
            used.set_param(n - 1, 0.7);
            fresh.set_param(n - 1, 0.7);
            for _ in 0..6 {
                used.note_on(37, 100, 3);
                run(&mut used, 0.6, 128, 44100.0);
            }
            used.reset();
            fresh.note_on(37, 100, 3);
            used.note_on(37, 100, 3);
            for blk in 0..4 {
                let (a, b) = (run(&mut fresh, 0.45, 128, 44100.0), run(&mut used, 0.45, 128, 44100.0));
                let bits = |v: &Vec<f32>| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
                assert_eq!((bits(&a.0), bits(&a.1)), (bits(&b.0), bits(&b.1)), "kind {k} block {blk}");
            }
            assert!(used.fault().is_none() && fresh.fault().is_none());
        }
    });
}

/// The instruments: the kinds that take note events (the others ignore them).
const INSTRUMENTS: [u8; 7] = [14, 20, 21, 22, 25, 27, 28];

/// `Device::all_notes_off` (trait: release the voices, drop the queued events) is a rebuild from the current
/// parameters for every instrument, ID 14 included: no voice keeps sounding and no queued note fires afterwards.
#[test]
fn all_notes_off_silences_every_instrument_and_drops_its_queue() {
    big(|| {
        for k in INSTRUMENTS {
            let mut m = vec![];
            let mut d = device(k, &good(k), &mut m);
            d.note_on(60, 100, 0);
            let (l, r) = run(&mut d, 0.0, 128, 44100.0);
            assert!(l.iter().chain(&r).any(|&x| x != 0.0), "kind {k}: the note must sound before the cut");
            // a note due in the next call, and one due two calls ahead: both still queued at the cut
            d.note_on(64, 100, 100);
            d.note_on(67, 100, 300);
            d.all_notes_off();
            for blk in 0..4 {
                let (l, r) = run(&mut d, 0.0, 128, 44100.0);
                assert!(l.iter().chain(&r).all(|&x| x == 0.0), "kind {k} block {blk}: silence after all_notes_off");
            }
            assert!(d.fault().is_none(), "kind {k}");
            // the device still plays afterwards
            d.note_on(60, 100, 0);
            let (l, r) = run(&mut d, 0.0, 128, 44100.0);
            assert!(l.iter().chain(&r).any(|&x| x != 0.0), "kind {k}: playable again");
        }
    });
}

/// Effects hold no notes: `all_notes_off` leaves their state alone (only `reset` clears it).
#[test]
fn all_notes_off_leaves_an_effect_untouched() {
    big(|| {
        for k in KINDS.into_iter().filter(|k| !INSTRUMENTS.contains(k)) {
            let mut m = vec![];
            let (mut a, mut b) = (device(k, &good(k), &mut m), device(k, &good(k), &mut m));
            for blk in 0..3 {
                if blk == 1 {
                    a.all_notes_off();
                }
                let (x, y) = (run(&mut a, 0.5, 128, 44100.0), run(&mut b, 0.5, 128, 44100.0));
                assert_eq!((x.0.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), x.1.iter().map(|v| v.to_bits()).collect::<Vec<_>>()),
                    (y.0.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), y.1.iter().map(|v| v.to_bits()).collect::<Vec<_>>()), "kind {k} block {blk}");
            }
        }
    });
}

/// ID 14 parameters with the hold flag on (a held note sounds on) and the given voice mode (P23).
fn id14_hold(mode: f32) -> Vec<f32> {
    let mut p = good(14);
    p[0] = 1.0;
    p[23] = mode;
    p
}

/// Writing the voice mode (P23, `trunc(v) = 1` = mono) so that the mode changes cuts every voice and empties the queue;
/// a write that leaves the mode alone (inside `[0, 1)`, or 1.0 again) changes nothing.
#[test]
fn id14_voice_mode_crossing_cuts_the_voices_and_clears_the_queue() {
    big(|| {
        let blocks = |d: &mut GlorpDevice, n: usize| (0..n).map(|_| run(d, 0.0, 128, 44100.0).0).collect::<Vec<_>>();
        let loud = |b: &[Vec<f32>]| b.iter().flatten().any(|&x| x != 0.0);
        for (start, same, crossing) in [(1.0f32, 1.0f32, 0.9999f32), (0.0, 0.9999, 1.0)] {
            let mut m = vec![];
            // sub-threshold control: the write keeps the mode, the voice and the queued note survive
            let mut keep = device(14, &id14_hold(start), &mut m);
            keep.note_on(60, 100, 0);
            keep.note_on(72, 100, 300);
            blocks(&mut keep, 1);
            keep.set_param(23, same);
            let kept = blocks(&mut keep, 4);
            assert!(loud(&kept[..1]), "start {start}: a write that keeps the mode does not cut the voice");
            assert_eq!(keep.fault(), None);
            // crossing: voice cut, queued note dropped (it would fire in the third block after the write)
            let mut cut = device(14, &id14_hold(start), &mut m);
            cut.note_on(60, 100, 0);
            cut.note_on(72, 100, 300);
            blocks(&mut cut, 1);
            cut.set_param(23, crossing);
            let after = blocks(&mut cut, 4);
            assert!(!loud(&after), "start {start}: crossing to {crossing} cuts the voice and drops the queued note");
            assert_eq!(cut.fault(), None);
            // and it plays again afterwards, in the new mode
            cut.note_on(60, 100, 0);
            assert!(loud(&blocks(&mut cut, 1)), "start {start}: playable after the mode change");
        }
    });
}

// ---- Either: the registry judges only what it owns ----

mod either {
    use super::*;
    use synth_core::device::{MAIN_L, MAIN_R};
    use synth_core::mix::{load_devices, Either};

    /// A foreign registry owning kind 99 only (outside every Glorp id); its device writes a constant.
    struct Foreign;
    impl Device for Foreign {
        fn param_count(&self) -> usize {
            1
        }
        fn set_param(&mut self, _: usize, _: f32) {}
        fn reset(&mut self) {}
        fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
            io.ch[MAIN_L][..n].fill(0.25);
            io.ch[MAIN_R][..n].fill(0.25);
        }
    }
    impl<'a> Kinds<'a> for Foreign {
        fn need(kind: u8, _: &'a [u8], _: &Env<'a>) -> Result<usize, LoadError> {
            if kind == 99 { Ok(0) } else { Err(LoadError::UnsupportedKind(kind)) }
        }
        fn load(kind: u8, c: &'a [u8], e: &Env<'a>, _: &mut Pool<'a>, _: &mut dyn synth_core::rng::Rng) -> Result<Self, LoadError> {
            Self::need(kind, c, e).map(|_| Foreign)
        }
    }

    type GF = Either<GlorpDevice<'static>, Foreign>;
    type FG = Either<Foreign, GlorpDevice<'static>>;

    /// Glorp kind 34 (device 0) and foreign kind 99 (device 1) on one track.
    fn mixed() -> B {
        let mut s = single(34, &good(34));
        s.device(99, &[0.5]);
        s.tracks[0].devs = vec![0, 1];
        s
    }

    /// Preflight + load of `s` through registry `K` (bytes and pool leaked: the registries borrow for `'static`).
    fn try_load<K: Kinds<'static> + 'static>(s: &B) -> Result<(), LoadError> {
        let b: &'static [u8] = Box::leak(s.bytes().into_boxed_slice());
        big(move || {
            let song = Song::parse(b).unwrap();
            let env = Env::for_song(&song, None);
            let mem: &'static mut [u8] = Box::leak(vec![0u8; 8 << 20].into_boxed_slice());
            let mut pool = Pool::new(mem);
            K::preflight(&song, &env)?;
            load_devices::<K>(&song, &env, &mut pool, &mut NoRng).map(|_| ())
        })
    }

    #[test]
    fn the_hook_skips_kinds_it_does_not_own() {
        let b = mixed().bytes();
        let song = Song::parse(&b).unwrap();
        let env = Env::for_song(&song, None);
        assert_eq!(<GlorpDevice as Kinds>::preflight(&song, &env), Ok(()));
        // a song without any owned device is no business of this registry (so is its rate)
        let mut only = single(99, &[0.5]);
        only.rate = 22_050;
        let b = only.bytes();
        let song = Song::parse(&b).unwrap();
        assert_eq!(<GlorpDevice as Kinds>::preflight(&song, &Env::for_song(&song, None)), Ok(()));
    }

    #[test]
    fn a_mixed_song_loads_through_either_in_both_orders() {
        assert_eq!(try_load::<GF>(&mixed()), Ok(()));
        assert_eq!(try_load::<FG>(&mixed()), Ok(()));
    }

    #[test]
    fn the_owned_part_of_a_mixed_song_is_still_judged() {
        // a bad chunk, an unsupported setting, a rate, a bad lane on the owned device, in both orders
        let mut bad_chunk = mixed();
        bad_chunk.devices[0].1.pop();
        let mut setting = single(38, &good(38));
        setting.device(99, &[0.5]);
        setting.tracks[0].devs = vec![0, 1];
        setting.tracks[0].autos.push((0, 0, vec![(0, 255)]));
        let mut rate = mixed();
        rate.rate = 22_050;
        for (what, s, e) in [
            ("chunk", &bad_chunk, LoadError::BadChunk),
            ("lane", &setting, LoadError::UnsupportedModule(38)),
            ("rate", &rate, LoadError::UnsupportedModule(0)),
        ] {
            assert_eq!(try_load::<GF>(s), Err(e), "{what} GF");
            assert_eq!(try_load::<FG>(s), Err(e), "{what} FG");
        }
    }

    #[test]
    fn a_lane_on_a_foreign_device_is_not_judged_here() {
        let mut s = mixed();
        // param 7 does not exist on kind 99's single parameter, and no Glorp range applies to it
        s.tracks[0].autos.push((1, 7, vec![(0, 0), (100, 255)]));
        assert_eq!(try_load::<GF>(&s), Ok(()));
        // the owned device's own lane on the same track is still judged
        s.tracks[0].autos.push((0, 4, vec![(0, 0)])); // kind 34 has 4 parameters: index 4 does not exist
        assert_eq!(try_load::<GF>(&s), Err(LoadError::BadIndex));
    }

    #[test]
    fn a_mixed_song_plays_both_engines() {
        let b: &'static [u8] = Box::leak(mixed().bytes().into_boxed_slice());
        big(move || {
            let song = Song::parse(b).unwrap();
            let env = Env::for_song(&song, None);
            let mem: &'static mut [u8] = Box::leak(vec![0u8; 8 << 20].into_boxed_slice());
            let mut pool = Pool::new(mem);
            let mut p = Player::<GF, NoRng>::new(song, &env, &mut pool, NoRng, 128).unwrap();
            let mut out = vec![0.0f32; 512];
            p.render_f32(&mut out);
            // device 0 (ID 34) processes silence, then the foreign device overwrites main with 0.25
            assert!(out.iter().all(|v| *v == 0.25), "{:?}", &out[..4]);
        });
    }

    #[test]
    fn alone_an_unowned_kind_is_still_refused_by_both_doors() {
        let s = mixed();
        // the owned API: strict, with the device index
        let r = refused(&s);
        assert_eq!((r.device, r.kind, r.reason), (Some(1), Some(99), Reason::UnsupportedKind));
        // the core API: `need` refuses it, with the kind
        assert_eq!(core_verdict(&s, 128), Err(LoadError::UnsupportedKind(99)));
        assert_eq!(try_load::<GlorpDevice<'static>>(&s), Err(LoadError::UnsupportedKind(99)));
        // and a lane on it does not hide it
        let mut s = mixed();
        s.tracks[0].autos.push((1, 0, vec![(0, 1)]));
        assert_eq!(core_verdict(&s, 128), Err(LoadError::UnsupportedKind(99)));
    }
}

// ---- notes: nothing the song format can carry makes an instrument fault ----

#[test]
fn no_note_event_of_the_song_format_faults_an_instrument() {
    // every lane byte: 0..=127 = note-on (+ velocity), 128..=255 = note-off of the low 7 bits; velocity 0 and 255 too
    for kind in [20u8, 21, 22, 25, 27] {
        let mut s = single(kind, &good(kind));
        let mut lane = vec![];
        for n in 0u8..=255 {
            // ID 20's velocity ceiling is the preflight's business (below), every other instrument takes the whole byte
            for v in [0u8, if kind == 20 { 213 } else { 255 }] {
                match n {
                    0..=127 => lane.push((50, n, Some(v))),
                    _ => lane.push((50, n & 0x7f, None)),
                }
            }
        }
        s.lanes[0] = lane;
        let bytes = s.bytes();
        {
            let song = Song::parse(&bytes).unwrap();
            let lane = song.lanes().next().unwrap();
            assert_eq!(lane.events().count(), 512);
            for (_, e) in lane.events() {
                use synth_core::device::EventKind::{NoteOff, NoteOn};
                let (NoteOn { note, .. } | NoteOff { note }) = e;
                assert!(note <= 127, "kind {kind}: note {note}");
            }
        }
        big(move || {
            let mut mem = vec![0u8; GlorpPlayer::required_pool(&bytes).unwrap()];
            let mut pool = Pool::new(&mut mem);
            let mut p = GlorpPlayer::new(&bytes, &mut pool).unwrap();
            let mut out = vec![0.0f32; 2 * 44_100];
            p.render_f32(&mut out).unwrap_or_else(|f| panic!("kind {kind} faulted: {f}"));
            assert_eq!(p.fault(), None);
            assert!(out.iter().any(|v| *v != 0.0), "kind {kind} rendered nothing");
        });
    }
}

#[test]
fn a_note_above_127_set_by_hand_latches_instead_of_playing() {
    // the direct Device door can carry one: layer 3 (the latch) is what covers it
    for kind in [20u8, 21, 22, 25, 27] {
        big(move || {
            let mut mem = vec![];
            let mut d = device(kind, &good(kind), &mut mem);
            d.note_on(128, 100, 0);
            assert_eq!(d.fault(), Some("note event refused"), "kind {kind}");
        });
    }
}

#[test]
fn id21_hand_writes_beyond_the_partial_bound_latch_a_fault() {
    big(|| {
        let mut m = vec![];
        let mut d = device(21, &good(21), &mut m);
        d.set_param(4, 0.3105); // 32 x 4 = 128: fine
        assert!(d.fault().is_none());
        d.set_param(6, 0.2); // 32 x 21 = 672 partials: not forwarded
        assert_eq!(d.fault(), Some("more than 512 partials per voice"));
        d.note_on(60, 100, 0);
        let (l, r) = run(&mut d, 0.0, 128, 44100.0);
        assert!(l.iter().chain(&r).all(|&x| x == 0.0), "a faulted instrument is silent");
    });
}

// ---- lane interpolation: the original Glorp engine's `a + (b-a)·t`, per device, also inside an `Either` ----

#[test]
fn every_glorp_kind_asks_for_the_original_lane_interpolation() {
    big(|| {
        for k in KINDS {
            let mut m = vec![];
            assert_eq!(device(k, &good(k), &mut m).lane_lerp(), LaneLerp::Delta, "kind {k}");
        }
    });
}

/// `GlorpDevice` with the core's default lane form: the pre-hook behaviour, kept as the sensitivity control.
struct MixForm<'a>(GlorpDevice<'a>);
impl Device for MixForm<'_> {
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
impl<'a> Kinds<'a> for MixForm<'a> {
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
        GlorpDevice::load(k, c, env, pool, rng).map(MixForm)
    }
}

/// An ID 21 song whose P7 lane (the noise seed, bit-sensitive) ramps while notes keep arriving: every note latches the
/// lane's value of its block, so the two lane forms play other partials wherever their floats differ.
fn seeded_song() -> B {
    let mut s = single(21, &good(21));
    s.lanes[0] = (0..24).flat_map(|i| [(if i == 0 { 0 } else { 1500 }, 60 + (i % 7) as u8, Some(100)), (700, 60 + (i % 7) as u8, None)]).collect();
    s.tracks[0].autos.push((0, 7, vec![(0, 11), (36_000, 243)]));
    s
}

macro_rules! render_of {
    ($k:ty, $s:expr, $blocks:expr) => {
        big({
            let b = $s.bytes();
            move || {
                let song = Song::parse(&b).unwrap();
                let env = Env::for_song(&song, None);
                let mut mem = vec![0u8; 8 << 20];
                let mut pool = Pool::new(&mut mem);
                let mut p = Player::<$k, NoRng>::new(song, &env, &mut pool, NoRng, 128).unwrap();
                let mut out = vec![0.0f32; 2 * 128 * $blocks];
                p.render_f32(&mut out);
                out.iter().map(|x| x.to_bits()).collect::<Vec<u32>>()
            }
        })
    };
}

#[test]
fn glorp_lanes_use_the_original_form_alone_and_inside_an_either() {
    let s = seeded_song();
    let alone = render_of!(GlorpDevice, s, 300);
    assert!(alone.iter().any(|&b| f32::from_bits(b).abs() > 1e-3), "the song must sound");
    // an `Either` must forward the device's form: the same bits, whichever side Glorp sits on
    assert_eq!(render_of!(Either<Foreign<0>, GlorpDevice>, s, 300), alone, "Either<Foreign, Glorp>");
    assert_eq!(render_of!(Either<GlorpDevice, Foreign<0>>, s, 300), alone, "Either<Glorp, Foreign>");
    // sensitivity: the default form (what the core played before the hook) is another sound for this song
    let old = render_of!(MixForm, s, 300);
    assert_ne!(old, alone, "the two lane forms must be told apart by this song");
}

/// The preflight judges the box `[lo − MARGIN, hi + MARGIN]` of every lane (and `Shell::set_param` tolerates that much
/// outside [0, 1]). The original's form `a + (b−a)·t` must stay inside it for every pair of lane values (byte/255) and
/// every `t` the player forms (`pos / span` of two i32-range frame counts), with room to spare, so no lane the preflight
/// accepted can write a vector it did not judge.
#[test]
fn the_preflight_margin_covers_the_original_lane_form() {
    let byte = |b: u32| (b as f64 / 255.0) as f32;
    let mut worst = 0.0f32;
    for span in [1i64, 3, 128, 1000, 44_100, 1 << 24, (1 << 31) - 1] {
        for k in 0..=200i64 {
            let pos = (span - 1) * k / 200;
            let t = pos as f32 / span as f32;
            for a in (0..256u32).step_by(3) {
                for b in (0..256u32).step_by(5).chain([255]) {
                    let (a, b) = (byte(a), byte(b));
                    let v = synth_core::math::lerp(a, b, t);
                    worst = worst.max(a.min(b) - v).max(v - a.max(b));
                    assert!(v >= a.min(b) - MARGIN && v <= a.max(b) + MARGIN, "{a} -> {b} at t {t}: {v}");
                }
            }
        }
    }
    assert!(worst < MARGIN / 100.0, "the margin must not be tight: worst overshoot {worst} vs {MARGIN}");
}

// ---- ID 20: velocity reaches the device, the ceiling is judged per track ----

/// ID 20 on lane 0 with the given note-on velocities (one note each, 10 frames apart).
fn id20_song(velocities: &[u8]) -> B {
    let mut s = single(20, &good(20));
    s.lanes[0] = velocities.iter().map(|&v| (10, 40, Some(v))).collect();
    s
}

#[test]
fn id20_velocity_ceiling_is_refused_by_the_preflight_with_the_device_named() {
    for ok in [0u8, 1, 127, 213] {
        let s = id20_song(&[ok]);
        assert_eq!(verdict(&s), Ok(()), "velocity {ok}");
        assert_eq!(core_verdict(&s, 128), Ok(()), "velocity {ok}");
    }
    for bad in [214u8, 215, 255] {
        let s = id20_song(&[100, bad, 100]);
        let r = refused(&s);
        assert_eq!((r.device, r.kind), (Some(0), Some(20)), "velocity {bad}");
        assert!(matches!(r.reason, Reason::Setting(w) if w.contains("velocity")), "velocity {bad}: {:?}", r.reason);
        assert_eq!(core_verdict(&s, 128), Err(LoadError::UnsupportedModule(20)), "velocity {bad}");
    }
}

#[test]
fn id20_velocity_is_judged_on_the_tracks_that_carry_the_instrument_only() {
    // lane 1 (velocity 255) is played by a track without ID 20: not this device's business
    let mut s = single(20, &good(20));
    s.device(25, &good(25));
    s.lanes[0] = vec![(10, 40, Some(127))];
    s.lanes.push(vec![(10, 40, Some(255))]);
    s.tracks.push(track(&[1], 1));
    assert_eq!(verdict(&s), Ok(()));
    // ...but the same lane on ID 20's own track is refused, even when another track plays it harmlessly too
    s.tracks[0].lane = 1;
    assert_eq!(refused(&s).kind, Some(20));
    // and a lane shared by two tracks is judged for the one that carries ID 20
    s.tracks[0].lane = 0;
    s.tracks[1].lane = 0;
    assert_eq!(verdict(&s), Ok(()));
}

#[test]
fn id20_hand_written_velocity_above_the_ceiling_latches() {
    big(|| {
        let mut m = vec![];
        let mut d = device(20, &good(20), &mut m);
        d.note_on(40, 213, 0);
        assert_eq!(d.fault(), None);
        d.note_on(40, 214, 0);
        assert_eq!(d.fault(), Some("note event refused"));
        let (l, r) = run(&mut d, 0.0, 128, 44100.0);
        assert!(l.iter().chain(&r).all(|&x| x == 0.0), "a faulted device is silent");
    });
}

#[test]
fn id20_velocity_reaches_the_device() {
    big(|| {
        let peak = |v: u8| {
            let mut m = vec![];
            let mut d = device(20, &good(20), &mut m);
            d.note_on(40, v, 0);
            let (l, r) = run(&mut d, 0.0, 128, 44100.0);
            assert_eq!(d.fault(), None);
            l.iter().chain(&r).fold(0.0f32, |m, x| m.max(x.abs()))
        };
        let (quiet, loud) = (peak(30), peak(127));
        assert!(quiet > 0.0 && loud > 3.0 * quiet, "velocity 30 peak {quiet}, velocity 127 peak {loud}");
        assert_eq!(peak(0), 0.0, "velocity 0 is silent");
    });
}

/// ID 28 reads the velocity (`velocity/127`); every value the song format carries (0..=255) is finite and accepted.
#[test]
fn id28_velocity_reaches_the_device_and_every_song_velocity_is_finite() {
    big(|| {
        let peak = |v: u8| {
            let mut m = vec![];
            let mut d = device(28, &good(28), &mut m);
            d.note_on(40, v, 0);
            let mut best = 0.0f32;
            for k in 0..10 {
                let (l, r) = run(&mut d, 0.0, 128, 44100.0);
                assert_eq!(d.fault(), None, "velocity {v} block {k}");
                assert!(l.iter().chain(&r).all(|x| x.is_finite()), "velocity {v}");
                best = l.iter().chain(&r).fold(best, |m, x| m.max(x.abs()));
            }
            best
        };
        let (quiet, loud) = (peak(30), peak(127));
        assert!(quiet > 0.0 && loud > 2.0 * quiet, "velocity 30 peak {quiet}, velocity 127 peak {loud}");
        assert!(peak(255) > loud, "velocity above 127 is louder, not refused");
        assert!(peak(0) < 1e-3 * loud, "velocity 0 is (nearly) silent");
        // a song whose lane carries the extreme velocities is accepted by both doors
        let mut s = single(28, &good(28));
        s.lanes[0] = vec![(0, 40, Some(255)), (200, 40, None), (10, 41, Some(0)), (200, 41, None), (10, 127, Some(1))];
        assert_eq!(verdict(&s), Ok(()));
        assert_eq!(core_verdict(&s, 128), Ok(()));
    });
}

/// The preflight needs no judge arm for ID 28: every vector of `[0, 1]^26` is accepted (tested vectors render finite), so lanes on P14 / P17 / P15
/// (voice count, spread mode, detune) and the filter are all accepted, in either door, and render without a fault.
#[test]
fn id28_lanes_on_every_parameter_are_accepted_and_render_finite() {
    big(|| {
        let n = good(28).len();
        assert_eq!(n, 26);
        let pts: &[(i32, u8)] = &[(0, 0), (700, 255), (0, 3), (200, 128), (100, 255), (50, 0)];
        let lanes: Vec<_> = (0..n as i32).map(|p| (0, p, pts)).collect();
        let mut s = with_lanes(28, &good(28), &lanes);
        s.lanes[0] = vec![(0, 40, Some(127)), (3000, 40, None), (0, 52, Some(90)), (6000, 52, None)];
        assert_eq!(verdict(&s), Ok(()));
        assert_eq!(core_verdict(&s, 128), Ok(()));
        let bytes = s.bytes();
        let need = GlorpPlayer::required_pool(&bytes).unwrap();
        let mut mem = vec![0u8; need];
        let mut pool = Pool::new(&mut mem);
        let mut p = GlorpPlayer::new(&bytes, &mut pool).unwrap();
        let mut out = vec![0.0f32; 128 * 2 * 120];
        p.render_f32(&mut out).unwrap();
        assert!(p.fault().is_none());
        assert!(out.iter().all(|x| x.is_finite() && x.abs() < 1e3));
        // the same lanes on the live parameters only (the chunk keeps level and envelopes): audible
        let live: Vec<_> = [14i32, 15, 16, 17, 22, 23, 24, 25].iter().map(|&p| (0, p, pts)).collect();
        let mut s = with_lanes(28, &good(28), &live);
        s.lanes[0] = vec![(0, 40, Some(127)), (3000, 40, None)];
        let bytes = s.bytes();
        let mut mem = vec![0u8; GlorpPlayer::required_pool(&bytes).unwrap()];
        let mut pool = Pool::new(&mut mem);
        let mut p = GlorpPlayer::new(&bytes, &mut pool).unwrap();
        let mut out = vec![0.0f32; 128 * 2 * 60];
        p.render_f32(&mut out).unwrap();
        assert!(out.iter().all(|x| x.is_finite()) && out.iter().any(|&x| x.abs() > 1e-3));
        // a lane on a 27th parameter does not exist
        let r = refused(&with_lanes(28, &good(28), &[(0, 26, &[(0, 1)])]));
        assert_eq!((r.kind, r.param, r.reason), (Some(28), Some(26), Reason::LaneParam));
    });
}

/// A hand-driven `GlorpDevice` of kind 28 latches a note above 127 and a write outside `[0, 1]`, like the other instruments.
#[test]
fn id28_hand_driven_misuse_latches() {
    big(|| {
        let mut m = vec![];
        let mut d = device(28, &good(28), &mut m);
        d.note_on(128, 100, 0);
        assert_eq!(d.fault(), Some("note event refused"));
        let (l, r) = run(&mut d, 0.0, 128, 44100.0);
        assert!(l.iter().chain(&r).all(|&x| x == 0.0));
        let mut m = vec![];
        let mut d = device(28, &good(28), &mut m);
        d.set_param(26, 0.5);
        assert_eq!(d.fault(), Some("parameter index outside the device"));
        let mut m = vec![];
        let mut d = device(28, &good(28), &mut m);
        d.set_param(14, 1.5);
        assert!(d.fault().is_some());
        // an overflowing queue is the original's silent drop, not a fault
        let mut m = vec![];
        let mut d = device(28, &good(28), &mut m);
        for _ in 0..300 {
            d.note_on(60, 100, 0);
        }
        assert_eq!(d.fault(), None);
    });
}

/// The same through the player: a P23 lane that leaves mono (1.0 → below 1.0) cuts the held voice; a lane that stays on
/// one side of the threshold does not.
#[test]
fn id14_voice_mode_lane_crossing_cuts_the_voice_through_the_player() {
    let song = |chunk_mode: f32, lane: &[(i32, u8)]| {
        let mut s = single(14, &id14_hold(chunk_mode));
        s.lanes[0] = vec![(0, 60, Some(100))];
        s.tracks[0].autos.push((0, 23, lane.to_vec()));
        s
    };
    // 16 blocks of 128 frames, the voice lives well under its 0.5 s silence check and 2.5 s limit
    let tail = |s: &B| {
        let bits = render_of!(GlorpDevice, s, 16);
        let v: Vec<f32> = bits.into_iter().map(f32::from_bits).collect();
        assert!(v.iter().all(|x| x.is_finite()));
        (v[..2 * 128 * 2].iter().any(|&x| x != 0.0), v[2 * 128 * 8..].iter().any(|&x| x != 0.0))
    };
    // mono → poly mid-song: sounds at the start, silent from the crossing on
    assert_eq!(tail(&song(1.0, &[(0, 255), (600, 0)])), (true, false), "mono lane leaves 1.0: cut");
    // a lane that stays mono for good (1.0 throughout), and a poly lane that never reaches 1.0, do not cut
    assert_eq!(tail(&song(1.0, &[(0, 255), (600, 255)])), (true, true), "mono stays mono");
    assert_eq!(tail(&song(0.0, &[(0, 0), (600, 254)])), (true, true), "poly stays poly below 1.0");
}
