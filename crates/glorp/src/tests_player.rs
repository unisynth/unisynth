//! Synthetic only: whole songs through `GlorpPlayer` against the standalone devices driven by hand. The adapter must be
//! a pure router: bit-identical to the manual chain (same 128-frame calls, parameter writes at block starts, note
//! offsets), and a fresh player after `seek` must equal the straight render. Plus a randomised song generator that
//! checks the preflight is sound: whatever it accepts renders without a device fault, and every vector the lanes
//! can write is supported.

use crate::id21::Id21;
use crate::id25::Id25;
use crate::id27::Id27;
use crate::id28::Id28;
use crate::id3::Id3;
use crate::id32::Id32;
use crate::id34::Id34;
use crate::id35::Id35;
use crate::id38::Id38;
use crate::id7::Id7;
use crate::player::GlorpPlayer;
use crate::preflight::{check_params, Reason};
use crate::tests_kinds::{big, good, single, KINDS};
use crate::testsong::{track, Song as B};
use synth_core::math::lerp;
use synth_core::pool::Pool;

const BLOCK: usize = 128;

fn byte(b: u8) -> f32 {
    (b as f64 / 255.0) as f32
}

/// The player's lane value at frame `pos` for sorted points `(frame, byte)` (see `AutoState::step`).
fn lane_at(points: &[(i64, u8)], pos: i64) -> f32 {
    let mut prev = points[0];
    let mut idx = 0;
    let mut cur = points[0];
    while idx < points.len() && cur.0 <= pos {
        prev = cur;
        idx += 1;
        if idx < points.len() {
            cur = points[idx];
        }
    }
    if idx >= points.len() {
        byte(prev.1)
    } else if idx == 0 {
        byte(cur.1)
    } else {
        let span = (cur.0 - prev.0) as i32;
        let t = if span > 0 { (pos - prev.0) as f32 / span as f32 } else { 0.0 };
        lerp(byte(prev.1), byte(cur.1), t) // the original engine's form (`GlorpDevice::lane_lerp`)
    }
}

/// Absolute-frame points from `(delta, byte)`.
fn abs(points: &[(i32, u8)]) -> Vec<(i64, u8)> {
    let mut t = 0i64;
    points.iter().map(|&(d, b)| { t += d as i64; (t, b) }).collect()
}

fn render(s: &B, blocks: usize) -> Vec<f32> {
    let bytes = s.bytes();
    let need = GlorpPlayer::required_pool(&bytes).unwrap_or_else(|r| panic!("refused: {r:?}"));
    let mut mem = vec![0u8; need];
    let mut pool = Pool::new(&mut mem);
    let mut p = GlorpPlayer::new(&bytes, &mut pool).unwrap();
    let mut out = vec![0.0f32; blocks * BLOCK * 2];
    p.render_f32(&mut out).unwrap();
    assert!(p.fault().is_none());
    out
}

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

/// Notes of a lane `(delta, note, Some(velocity) = on | None = off)` as `(frame, note, on)`.
fn frames(lane: &[(i32, u8, Option<u8>)]) -> Vec<(i64, u8, bool)> {
    let mut t = 0i64;
    lane.iter().map(|&(d, n, v)| { t += d as i64; (t, n, v.is_some()) }).collect()
}

const LANE_A: [(i32, u8, Option<u8>); 6] =
    [(0, 40, Some(100)), (300, 40, None), (100, 52, Some(90)), (400, 52, None), (130, 44, Some(80)), (700, 44, None)];

/// One track `[25, 38, 34, 3]`: instrument, filter, frequency shifter, waveshaper; lanes on one parameter of each.
fn chain_song() -> B {
    let mut s = B::new();
    s.device(25, &good(25));
    s.device(38, &good(38));
    s.device(34, &good(34));
    s.device(3, &good(3));
    s.lanes.push(LANE_A.to_vec());
    let mut t = track(&[0, 1, 2, 3], 0);
    t.autos.push((0, 0, vec![(0, 51), (1280, 204)]));
    t.autos.push((1, 1, vec![(0, 60), (1280, 200)]));
    t.autos.push((2, 3, vec![(0, 100), (700, 30), (900, 220)]));
    t.autos.push((3, 0, vec![(0, 0), (2000, 255)]));
    s.tracks.push(t);
    s
}

fn arr<const N: usize>(v: &[f32]) -> [f32; N] {
    v.try_into().unwrap()
}

#[test]
fn a_four_device_chain_is_bit_identical_to_the_devices_driven_by_hand() {
    big(|| {
        let s = chain_song();
        let blocks = 30;
        let out = render(&s, blocks);
        let (mut inst, mut flt, mut shift, mut shaper) = (
            Id25::new(&arr(&good(25))).unwrap(),
            Id38::new(&arr(&good(38))).unwrap(),
            Id34::new(&arr(&good(34))).unwrap(),
            Id3::new(&arr(&good(3))).unwrap(),
        );
        let notes = frames(&LANE_A);
        let lanes = [abs(&[(0, 51), (1280, 204)]), abs(&[(0, 60), (1280, 200)]), abs(&[(0, 100), (700, 30), (900, 220)]), abs(&[(0, 0), (2000, 255)])];
        let mut want = Vec::new();
        for j in 0..blocks {
            let pos = (j * BLOCK) as i64;
            for &(at, n, on) in &notes {
                if at >= pos && at < pos + BLOCK as i64 {
                    let off = (at - pos) as u32;
                    if on { inst.note_on(n, off).unwrap() } else { inst.note_off(n, off).unwrap() }
                }
            }
            inst.set_param(0, lane_at(&lanes[0], pos)).unwrap();
            flt.set_param(1, lane_at(&lanes[1], pos)).unwrap();
            shift.set_param(3, lane_at(&lanes[2], pos)).unwrap();
            shaper.set_param(0, lane_at(&lanes[3], pos)).unwrap();
            let (mut l, mut r) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
            inst.process(&mut l, &mut r);
            flt.process(&mut l, &mut r);
            shift.process(&mut l, &mut r);
            shaper.process(&mut l, &mut r);
            for i in 0..BLOCK {
                want.push(l[i]);
                want.push(r[i]);
            }
        }
        assert!(want.iter().fold(0.0f32, |m, x| m.max(x.abs())) > 1e-3, "the chain is silent: a vacuous comparison");
        assert_eq!(bits(&out), bits(&want));
    });
}

/// `[21, 38]` on one track with lanes on the ID 21 parameters that matter at note-on (P1 decay, P5 ratio spread, P7 seed)
/// and one on the filter: the player is bit-identical to the standalone devices driven by hand.
#[test]
fn an_id21_chain_is_bit_identical_to_the_devices_driven_by_hand() {
    big(|| {
        let mut s = B::new();
        s.device(21, &good(21));
        s.device(38, &good(38));
        s.lanes.push(LANE_A.to_vec());
        let mut t = track(&[0, 1], 0);
        t.autos.push((0, 7, vec![(0, 60), (900, 200), (700, 90)]));
        t.autos.push((0, 1, vec![(0, 5), (1000, 40)]));
        t.autos.push((0, 5, vec![(0, 100), (500, 130)]));
        t.autos.push((1, 1, vec![(0, 60), (1280, 200)]));
        s.tracks.push(t);
        let blocks = 30;
        let out = render(&s, blocks);
        let (mut inst, mut flt) = (Id21::new(&arr(&good(21))).unwrap(), Id38::new(&arr(&good(38))).unwrap());
        let notes = frames(&LANE_A);
        let lanes = [abs(&[(0, 60), (900, 200), (700, 90)]), abs(&[(0, 5), (1000, 40)]), abs(&[(0, 100), (500, 130)]), abs(&[(0, 60), (1280, 200)])];
        let mut want = Vec::new();
        for j in 0..blocks {
            let pos = (j * BLOCK) as i64;
            for &(at, n, on) in &notes {
                if at >= pos && at < pos + BLOCK as i64 {
                    let off = (at - pos) as u32;
                    if on { inst.note_on(n, off).unwrap() } else { inst.note_off(n, off).unwrap() }
                }
            }
            inst.set_param(7, lane_at(&lanes[0], pos)).unwrap();
            inst.set_param(1, lane_at(&lanes[1], pos)).unwrap();
            inst.set_param(5, lane_at(&lanes[2], pos)).unwrap();
            flt.set_param(1, lane_at(&lanes[3], pos)).unwrap();
            let (mut l, mut r) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
            inst.process(&mut l, &mut r);
            flt.process(&mut l, &mut r);
            for i in 0..BLOCK {
                want.push(l[i]);
                want.push(r[i]);
            }
        }
        assert!(want.iter().fold(0.0f32, |m, x| m.max(x.abs())) > 1e-3, "the chain is silent: a vacuous comparison");
        assert_eq!(bits(&out), bits(&want));
    });
}

/// `[28, 38]` on one track with note velocities and lanes on the live ID 28 parameters (P15 detune, P17 spread mode, P22
/// cutoff, P24 envelope amount), a latched one (P14 voice count, read at the next note-on) and one on the filter: the
/// player is bit-identical to the standalone devices driven by hand (the lane form is the engine's `a + (b − a)·t`).
#[test]
fn an_id28_chain_is_bit_identical_to_the_devices_driven_by_hand() {
    big(|| {
        let lane = [(0, 40, Some(127)), (300, 40, None), (100, 52, Some(90)), (400, 52, None), (130, 44, Some(20)), (700, 44, None)];
        let mut s = B::new();
        s.device(28, &good(28));
        s.device(38, &good(38));
        s.lanes.push(lane.to_vec());
        let mut t = track(&[0, 1], 0);
        t.autos.push((0, 15, vec![(0, 20), (1500, 200)]));
        t.autos.push((0, 17, vec![(0, 0), (2000, 255)]));
        t.autos.push((0, 22, vec![(0, 200), (1800, 60), (500, 150)]));
        t.autos.push((0, 24, vec![(0, 100), (1700, 170)]));
        t.autos.push((0, 14, vec![(0, 20), (700, 200)]));
        t.autos.push((1, 1, vec![(0, 60), (1280, 200)]));
        s.tracks.push(t);
        let blocks = 40;
        let out = render(&s, blocks);
        let (mut inst, mut flt) = (Id28::new(&arr(&good(28))).unwrap(), Id38::new(&arr(&good(38))).unwrap());
        let mut at = 0i64;
        let notes: Vec<(i64, u8, Option<u8>)> = lane.iter().map(|&(d, n, v)| { at += d as i64; (at, n, v) }).collect();
        let lanes = [
            (15, abs(&[(0, 20), (1500, 200)])),
            (17, abs(&[(0, 0), (2000, 255)])),
            (22, abs(&[(0, 200), (1800, 60), (500, 150)])),
            (24, abs(&[(0, 100), (1700, 170)])),
            (14, abs(&[(0, 20), (700, 200)])),
        ];
        let fl = abs(&[(0, 60), (1280, 200)]);
        let mut want = Vec::new();
        for j in 0..blocks {
            let pos = (j * BLOCK) as i64;
            for &(t, n, v) in &notes {
                if t >= pos && t < pos + BLOCK as i64 {
                    let off = (t - pos) as u32;
                    match v {
                        Some(v) => inst.note_on(n, v, off).unwrap(),
                        None => inst.note_off(n, off).unwrap(),
                    }
                }
            }
            for (i, pts) in &lanes {
                inst.set_param(*i, lane_at(pts, pos)).unwrap();
            }
            flt.set_param(1, lane_at(&fl, pos)).unwrap();
            let (mut l, mut r) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
            inst.process(&mut l, &mut r);
            flt.process(&mut l, &mut r);
            for i in 0..BLOCK {
                want.push(l[i]);
                want.push(r[i]);
            }
        }
        assert!(want.iter().fold(0.0f32, |m, x| m.max(x.abs())) > 1e-3, "the chain is silent: a vacuous comparison");
        assert_eq!(bits(&out), bits(&want));
    });
}

/// Tracks: 0 = ID 27 on lane 0, 1 = ID 25 on lane 1, master 2 = `[7, 35, 32]` receiving 0 into main (×0.8) and 1 into
/// the side-chain pair (×0.5), lanes on ID 7 threshold, ID 35 base frequency, ID 32 mix.
fn mix_song() -> B {
    let mut s = B::new();
    s.device(27, &good(27));
    s.device(25, &good(25));
    let mut comp = good(7);
    comp[0] = 1.0; // side-chain detector
    comp[2] = 0.5;
    s.device(7, &comp);
    s.device(35, &good(35));
    s.device(32, &good(32));
    s.lanes.push(vec![(0, 16, Some(100)), (900, 16, None), (200, 18, Some(100)), (900, 18, None)]);
    s.lanes.push(vec![(0, 30, Some(100)), (500, 30, None), (800, 31, Some(100)), (300, 31, None)]);
    s.lanes.push(vec![]);
    s.tracks.push(track(&[0], 0));
    s.tracks.push(track(&[1], 1));
    let mut m = track(&[2, 3, 4], 2);
    m.recv = vec![(0, 0, 0.8), (1, 2, 0.5)];
    m.autos.push((0, 2, vec![(0, 255), (1500, 40)]));
    m.autos.push((1, 0, vec![(0, 30), (2000, 120)]));
    m.autos.push((2, 1, vec![(0, 40), (1000, 200)]));
    s.tracks.push(m);
    s
}

#[test]
fn tracks_receives_and_the_sidechain_pair_route_like_the_hand_built_graph() {
    big(|| {
        let s = mix_song();
        let blocks = 24;
        let out = render(&s, blocks);
        let (mut drum, mut inst) = (Id27::new(&arr(&good(27))).unwrap(), Id25::new(&arr(&good(25))).unwrap());
        let mut comp_p = good(7);
        comp_p[0] = 1.0;
        comp_p[2] = 0.5;
        let (mut comp, mut ap, mut chorus) =
            (Id7::new(&arr(&comp_p)).unwrap(), Id35::new(&arr(&good(35))).unwrap(), Id32::new(&arr(&good(32))).unwrap());
        assert!(comp.sidechain());
        let la = frames(&[(0, 16, Some(100)), (900, 16, None), (200, 18, Some(100)), (900, 18, None)]);
        let lb = frames(&[(0, 30, Some(100)), (500, 30, None), (800, 31, Some(100)), (300, 31, None)]);
        let lanes = [abs(&[(0, 255), (1500, 40)]), abs(&[(0, 30), (2000, 120)]), abs(&[(0, 40), (1000, 200)])];
        let mut want = Vec::new();
        for j in 0..blocks {
            let pos = (j * BLOCK) as i64;
            for &(at, n, on) in &la {
                if at >= pos && at < pos + BLOCK as i64 {
                    if on { drum.note_on(n, (at - pos) as u32).unwrap() } else { drum.note_off(n, (at - pos) as u32).unwrap() }
                }
            }
            for &(at, n, on) in &lb {
                if at >= pos && at < pos + BLOCK as i64 {
                    if on { inst.note_on(n, (at - pos) as u32).unwrap() } else { inst.note_off(n, (at - pos) as u32).unwrap() }
                }
            }
            comp.set_param(2, lane_at(&lanes[0], pos)).unwrap();
            ap.set_param(0, lane_at(&lanes[1], pos)).unwrap();
            chorus.set_param(1, lane_at(&lanes[2], pos)).unwrap();
            let (mut dl, mut dr) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
            drum.process(&mut dl, &mut dr);
            let (mut il, mut ir) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
            inst.process(&mut il, &mut ir);
            // receives accumulate into zeroed buffers: 0 + x · volume
            let m = |v: &[f32], g: f32| v.iter().map(|x| 0.0f32 + x * g).collect::<Vec<_>>();
            let (mut l, mut r, sl, sr) = (m(&dl, 0.8), m(&dr, 0.8), m(&il, 0.5), m(&ir, 0.5));
            comp.process_sidechain(&mut l, &mut r, &sl, &sr).unwrap();
            ap.process(&mut l, &mut r);
            chorus.process(&mut l, &mut r).unwrap();
            for i in 0..BLOCK {
                want.push(l[i]);
                want.push(r[i]);
            }
        }
        assert!(want.iter().fold(0.0f32, |m, x| m.max(x.abs())) > 1e-3, "silent: a vacuous comparison");
        assert_eq!(bits(&out), bits(&want));
    });
}

#[test]
fn seek_reproduces_the_straight_render_for_every_kind() {
    big(|| {
        for s in [chain_song(), mix_song()] {
            let bytes = s.bytes();
            let need = GlorpPlayer::required_pool(&bytes).unwrap();
            let mut mem = vec![0u8; need];
            let mut pool = Pool::new(&mut mem);
            let mut p = GlorpPlayer::new(&bytes, &mut pool).unwrap();
            let n = 40 * BLOCK;
            let mut full = vec![0.0f32; 2 * n];
            p.render_f32(&mut full).unwrap();
            // back to the start, then into the middle of a block
            p.seek(0);
            let mut again = vec![0.0f32; 2 * n];
            p.render_f32(&mut again).unwrap();
            assert_eq!(bits(&full), bits(&again));
            let at = 17 * BLOCK + 50;
            p.seek(at as u64);
            let mut tail = vec![0.0f32; 2 * (n - at)];
            p.render_f32(&mut tail).unwrap();
            assert_eq!(bits(&full[2 * at..]), bits(&tail));
        }
    });
}

#[test]
fn every_kind_plays_alone_and_makes_sound() {
    big(|| {
        for k in KINDS {
            let mut s = single(k, &good(k));
            // instruments need notes; effects see silence, which they pass as silence
            s.lanes[0] = vec![(0, 36, Some(100)), (400, 36, None)];
            let out = render(&s, 10);
            assert!(out.iter().all(|x| x.is_finite()), "kind {k}");
            let loud = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert_eq!(loud > 1e-4, k == 14 || k == 20 || k == 21 || k == 22 || k == 25 || k == 27 || k == 28, "kind {k}: peak {loud}");
        }
    });
}

// ---- soundness of the preflight on random songs ----

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
}

/// Values that sit on the edges of the supported / unsupported settings, plus plain ones.
const EDGE: [f32; 14] = [0.0, 0.1, 0.1666, 0.1667, 0.4999, 0.5, 0.5001, 0.667, 0.6668, 0.8, 0.9, 0.99, 1.0, 0.25];

fn random_song(kind: u8, g: &mut Lcg) -> B {
    let mut p = good(kind);
    for _ in 0..g.below(3) {
        let i = g.below(p.len() as u32) as usize;
        p[i] = EDGE[g.below(EDGE.len() as u32) as usize];
    }
    let mut s = single(kind, &p);
    if kind == 21 || kind == 22 {
        s.lanes[0] = vec![(0, 50, Some(100)), (300, 50, None), (200, 62, Some(90)), (300, 62, None), (100, 71, Some(80))];
    }
    for _ in 0..g.below(4) {
        let param = if kind == 32 && g.below(2) == 0 {
            [2, 4, 7][g.below(3) as usize]
        } else if kind == 22 && g.below(2) == 0 {
            [2, 4][g.below(2) as usize] // the weight guard's two parameters
        } else {
            g.below(p.len() as u32) as i32
        };
        let n = 1 + g.below(4);
        // lane values biased low so that a good share of the songs is accepted
        let cap = [255u32, 255, 120, 60][g.below(4) as usize] + 1;
        let mut pts: Vec<(i32, u8)> = (0..n).map(|_| (g.below(900) as i32, g.below(cap) as u8)).collect();
        if kind == 22 {
            // the guard's edges: P2 near 0 (a = 0.002..0.03), P4 near 1 (n = 14..16)
            let pick = |g: &mut Lcg, v: &[u8]| v[g.below(v.len() as u32) as usize];
            for pt in pts.iter_mut() {
                match param {
                    2 if g.below(2) == 0 => pt.1 = pick(g, &[0, 0, 1, 2, 3, 5]),
                    4 if g.below(2) == 0 => pt.1 = pick(g, &[255, 250, 240, 235, 230, 200]),
                    _ => {}
                }
            }
        }
        s.tracks[0].autos.push((0, param, pts));
    }
    if kind == 22 && g.below(2) == 0 {
        // both guard parameters automated at once: the box corner (lowest P2, highest P4) is what has to be judged
        let lo: Vec<(i32, u8)> = (0..2).map(|_| (g.below(900) as i32, [0u8, 1, 2, 3, 5, 20][g.below(6) as usize])).collect();
        let hi: Vec<(i32, u8)> = (0..2).map(|_| (g.below(900) as i32, [255u8, 245, 240, 235, 230, 100][g.below(6) as usize])).collect();
        s.tracks[0].autos.push((0, 2, lo));
        s.tracks[0].autos.push((0, 4, hi));
    }
    s
}

#[test]
fn whatever_the_preflight_accepts_never_leaves_the_supported_settings() {
    big(|| {
        let mut g = Lcg(0x5eed);
        let (mut accepted, mut refused) = ([0u32; 5], [0u32; 5]);
        for (slot, (kind, cases)) in [(38u8, 220u32), (35, 220), (32, 40), (21, 300), (22, 300)].into_iter().enumerate() {
            for _ in 0..cases {
                let s = random_song(kind, &mut g);
                let bytes = s.bytes();
                let Ok(need) = GlorpPlayer::required_pool(&bytes) else {
                    refused[slot] += 1;
                    continue;
                };
                accepted[slot] += 1;
                // every vector the lanes write at every frame of the first 2048 is supported (the lane values at one
                // frame are written together, in any order: the set of such vectors is inside the judged box)
                let init = crate::preflight::chunk_params(kind, &s.devices[0].1).unwrap();
                let lanes: Vec<(usize, Vec<(i64, u8)>)> =
                    s.tracks[0].autos.iter().filter(|a| !a.2.is_empty()).map(|a| (a.1 as usize, abs(&a.2))).collect();
                for pos in 0..2048i64 {
                    let mut v = init;
                    for (param, pts) in &lanes {
                        v[*param] = lane_at(pts, pos).clamp(0.0, 1.0);
                        // and each partial application on the way
                        assert_eq!(check_params(kind, &v), Ok(()), "kind {kind} frame {pos}: {autos:?}", autos = s.tracks[0].autos);
                    }
                }
                // end to end: no device fault, finite sound
                let mut mem = vec![0u8; need];
                let mut pool = Pool::new(&mut mem);
                let mut p = GlorpPlayer::new(&bytes, &mut pool).unwrap();
                let mut out = vec![0.0f32; 16 * BLOCK * 2];
                p.render_f32(&mut out).unwrap_or_else(|f| panic!("kind {kind}: fault {f}: {:?}", s.tracks[0].autos));
                assert!(out.iter().all(|x| x.is_finite()));
            }
        }
        for slot in 0..5 {
            assert!(accepted[slot] >= 10 && refused[slot] >= 10, "vacuous generator: accepted {accepted:?} refused {refused:?}");
        }
    });
}

#[test]
fn the_refusal_reasons_of_the_random_songs_are_all_automation_or_settings() {
    // a song is refused for something real, never for a plumbing reason
    let mut g = Lcg(7);
    for kind in [38u8, 35, 32, 21, 22] {
        for _ in 0..100 {
            let s = random_song(kind, &mut g);
            if let Err(r) = GlorpPlayer::required_pool(&s.bytes()) {
                assert!(matches!(r.reason, Reason::Setting(_) | Reason::Automation(_) | Reason::Range), "{r:?}");
            }
        }
    }
}
