//! Automation re-mapping (design D10). A legacy parameter's automation drives the unified parameters that parameter
//! changes (found by evaluating the mapping at every byte value). Each such unified parameter gets its own lane:
//!
//! * **continuous** laws: every legacy point moves to the unified value of its byte (same frame), and points are added
//!   where linear interpolation between neighbours would differ from the unified value of the legacy interpolation by
//!   more than one byte step at a 100-frame sub-block start, which is where the player reads automation;
//! * **discrete** decodes (types, counts, whole-semitone steps): the legacy value is read at every sub-block start and
//!   a step (two points at one frame) is written wherever the decoded value changes, so the song never passes
//!   through the values in between.
//!
//! Lanes of structural parameters (capacity, oscillator type, sampler source and index) are never written.

use synth_core::math::mix;
use synth_core::mix::DEFAULT_SUB;
use unisynth::params::old::{self as pm, law_of, Law, COUNT};

const SUB: i64 = DEFAULT_SUB as i64;

/// One converted lane: unified parameter and `(absolute frame, byte)` points.
#[derive(Clone, Debug, PartialEq)]
pub struct Lane {
    pub param: usize,
    pub points: Vec<(i32, u8)>,
}

/// Port of the player's `AutoState::step`: the value of a sub-block starting at `pos`.
struct Sim<'a> {
    pts: &'a [(i32, f32)],
    idx: usize,
    prev: (i32, f32),
    cur: (i32, f32),
    pos: i64,
}

impl<'a> Sim<'a> {
    fn new(pts: &'a [(i32, f32)]) -> Sim<'a> {
        let cur = pts.first().copied().unwrap_or((0, 0.0));
        Sim { pts, idx: 0, prev: cur, cur, pos: 0 }
    }

    fn step(&mut self, n: i64) -> f32 {
        let len = self.pts.len();
        while self.idx < len && self.cur.0 as i64 <= self.pos {
            self.prev = self.cur;
            self.idx += 1;
            if let Some(&p) = self.pts.get(self.idx) {
                self.cur = p;
            }
        }
        let v = if self.idx >= len {
            self.prev.1
        } else if self.idx == 0 {
            self.cur.1
        } else {
            let span = self.cur.0.wrapping_sub(self.prev.0);
            let t = if span > 0 { (self.pos - self.prev.0 as i64) as f32 / span as f32 } else { 0.0 };
            mix(self.prev.1, self.cur.1, t)
        };
        self.pos += n;
        v
    }
}

fn byte(p: f32) -> u8 {
    (p.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A legacy byte value as the player decodes it.
pub fn value(b: u8) -> f32 {
    (b as f64 / 255.0) as f32
}

/// The values a player reads from a lane (`(absolute frame, byte)` points) at the first `steps` sub-block starts.
pub fn read_lane(points: &[(i32, u8)], steps: usize) -> Vec<f32> {
    let pts: Vec<(i32, f32)> = points.iter().map(|&(t, b)| (t, value(b))).collect();
    let mut sim = Sim::new(&pts);
    (0..steps).map(|_| sim.step(SUB)).collect()
}

/// How a discrete unified parameter decodes: by its law, or as a choice among `n` (the type-dependent `Shape` values of
/// a sampler slot, which `law_of` cannot tell from continuous ones).
#[derive(Clone, Copy)]
enum Dec {
    Law(Law),
    Choice(usize),
}

impl Dec {
    fn of(j: usize, choices: &[(usize, usize)]) -> Option<Dec> {
        if let Some(&(_, n)) = choices.iter().find(|c| c.0 == j) {
            return Some(Dec::Choice(n));
        }
        matches!(law_of(j), Law::Choice(_) | Law::Kinds { .. } | Law::Count(..) | Law::Index | Law::Coarse | Law::Ratio).then(|| Dec::Law(law_of(j)))
    }

    /// The decoded value of byte-quantised `p`, as bits.
    fn bits(self, p: f32) -> u32 {
        let q = byte(p) as f32 / 255.0;
        match self {
            Dec::Law(l) => l.map(q).to_bits(),
            Dec::Choice(n) => unisynth::law::choice(q, n) as u32,
        }
    }
}

/// The unified parameters a legacy parameter moves, given the unified values for every byte (`probe(b)`).
pub fn affected(probe: &dyn Fn(u8) -> [f32; COUNT]) -> Vec<usize> {
    let first = probe(0);
    let mut hit = [false; COUNT];
    for b in 1..=255u8 {
        let p = probe(b);
        for j in 0..COUNT {
            if p[j] != first[j] {
                hit[j] = true;
            }
        }
    }
    (0..COUNT).filter(|&j| hit[j] && !pm::is_structural(j) && !pm::is_raw(j)).collect()
}

/// Converts one legacy automation (`points`: absolute frame, byte) into lanes for the unified parameters `params`.
/// `eval(v)` is the unified values for the legacy value `v`.
pub fn convert(
    points: &[(i32, u8)],
    params: &[usize],
    choices: &[(usize, usize)],
    eval: &dyn Fn(f32) -> [f32; COUNT],
) -> Vec<Lane> {
    if points.is_empty() {
        return vec![];
    }
    let legacy: Vec<(i32, f32)> = points.iter().map(|&(t, b)| (t, value(b))).collect();
    // the legacy value at every sub-block start up to the last point
    let last = legacy.iter().map(|p| p.0 as i64).max().unwrap_or(0).max(0);
    let steps = (last / SUB + 2) as usize;
    let mut sim = Sim::new(&legacy);
    let grid: Vec<f32> = (0..steps).map(|_| sim.step(SUB)).collect();
    // the unified values at those starts, once per distinct legacy value
    let mut cache: Option<(f32, [f32; COUNT])> = None;
    let mut at = |v: f32| -> [f32; COUNT] {
        match cache {
            Some((c, r)) if c == v => r,
            _ => {
                let r = eval(v);
                cache = Some((v, r));
                r
            }
        }
    };
    let unified: Vec<[f32; COUNT]> = grid.iter().map(|&v| at(v)).collect();
    let first_t = legacy[0].0;
    params
        .iter()
        .map(|&j| {
            let u: Vec<f32> = unified.iter().map(|r| r[j]).collect();
            let pts = if let Some(dec) = Dec::of(j, choices) {
                steps_lane(dec, first_t, &u)
            } else {
                smooth_lane(&legacy, &u, &mut |v| eval(v)[j])
            };
            Lane { param: j, points: pts }
        })
        .collect()
}

fn steps_lane(dec: Dec, first_t: i32, u: &[f32]) -> Vec<(i32, u8)> {
    let dec = |p: f32| dec.bits(p);
    let mut out = vec![(first_t, byte(u[0]))];
    for k in 1..u.len() {
        if dec(u[k]) != dec(u[k - 1]) {
            let t = (k as i64 * SUB) as i32;
            out.push((t, byte(u[k - 1])));
            out.push((t, byte(u[k])));
        }
    }
    out
}

fn smooth_lane(legacy: &[(i32, f32)], u: &[f32], f: &mut dyn FnMut(f32) -> f32) -> Vec<(i32, u8)> {
    let pts: Vec<(i32, u8)> = legacy.iter().map(|&(t, v)| (t, byte(f(v)))).collect();
    let mut out = vec![pts[0]];
    for w in 0..pts.len() - 1 {
        refine(pts[w], pts[w + 1], u, &mut out, 0);
        out.push(pts[w + 1]);
    }
    out
}

/// Adds the points between `a` and `b` that keep the interpolation within one byte step of `u` at every sub-block
/// start strictly between them.
fn refine(a: (i32, u8), b: (i32, u8), u: &[f32], out: &mut Vec<(i32, u8)>, depth: u32) {
    if b.0 <= a.0 || depth > 48 {
        return;
    }
    let (mut worst, mut at) = (0.0f32, None);
    let mut k = (a.0 as i64).div_euclid(SUB) + 1;
    k = k.max(0);
    while k * SUB < b.0 as i64 {
        let pos = k * SUB;
        if (k as usize) < u.len() {
            let t = (pos - a.0 as i64) as f32 / (b.0 as i64 - a.0 as i64) as f32;
            let line = mix(value(a.1), value(b.1), t);
            let err = (line - u[k as usize]).abs();
            if err > worst {
                (worst, at) = (err, Some(k as usize));
            }
        }
        k += 1;
    }
    if let (true, Some(k)) = (worst > 1.0 / 255.0 + 1e-6, at) {
        let m = ((k as i64 * SUB) as i32, byte(u[k]));
        refine(a, m, u, out, depth + 1);
        out.push(m);
        refine(m, b, u, out, depth + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mapping that changes parameter 0 (the master gain, continuous) by the square of the legacy value and parameter 85's
    /// filter type (discrete: type = floor(3v) + 1 of 9).
    fn eval(v: f32) -> [f32; COUNT] {
        let mut r = [0.0; COUNT];
        r[0] = v * v;
        r[pm::filt(0, pm::f::TYPE)] = unisynth::law::host::inv_choice(1 + ((v * 3.0) as usize).min(3), 9);
        r
    }

    #[test]
    fn the_player_port_reads_a_ramp_at_sub_block_starts() {
        let pts = [(0, 0.0), (1000, 1.0)];
        let mut s = Sim::new(&pts);
        let v: Vec<f32> = (0..12).map(|_| s.step(100)).collect();
        assert_eq!(v[0], 0.0);
        assert!((v[5] - 0.5).abs() < 1e-6);
        assert_eq!(v[10], 1.0);
        assert_eq!(v[11], 1.0);
    }

    #[test]
    fn a_square_law_ramp_gets_points_until_it_is_within_a_byte_step() {
        let lanes = convert(&[(0, 0), (44100, 255)], &[0], &[], &eval);
        let l = &lanes[0];
        assert!(l.points.len() > 3, "a curve needs points: {:?}", l.points.len());
        // read it back like the player
        let pts: Vec<(i32, f32)> = l.points.iter().map(|&(t, b)| (t, value(b))).collect();
        let mut sim = Sim::new(&pts);
        let mut leg = Sim::new(&[(0, 0.0), (44100, 1.0)]);
        for k in 0..441 {
            let got = sim.step(100);
            let want = eval(leg.step(100))[0];
            assert!((got - want).abs() <= 1.0 / 255.0 + 1e-6, "at {}: {got} vs {want}", k * 100);
        }
    }

    #[test]
    fn a_linear_law_needs_no_extra_points() {
        let lin = |v: f32| {
            let mut r = [0.0; COUNT];
            r[0] = v * 0.25;
            r
        };
        let l = &convert(&[(0, 0), (44100, 255), (88200, 0)], &[0], &[], &lin)[0];
        assert_eq!(l.points, vec![(0, 0), (44100, 64), (88200, 0)]);
    }

    #[test]
    fn a_discrete_parameter_steps_at_the_frames_where_the_decode_changes() {
        let t = pm::filt(0, pm::f::TYPE);
        let lanes = convert(&[(0, 0), (30000, 255)], &[t], &[], &eval);
        let l = &lanes[0];
        // v crosses 1/3 and 2/3 and 1 at t = 10000, 20000, 30000; the player reads whole sub-blocks
        let steps: Vec<i32> = l.points.iter().skip(1).step_by(2).map(|p| p.0).collect();
        assert_eq!(steps, vec![10000, 20000, 30000]);
        assert_eq!(l.points[0], (0, byte(unisynth::law::host::inv_choice(1, 9))));
        // two points at one frame, old value then new: no ramp through the intermediate types
        for pair in l.points[1..].chunks(2) {
            assert_eq!(pair[0].0, pair[1].0);
            assert_ne!(pair[0].1, pair[1].1);
        }
        // and the value the player reads at every sub-block start is the legacy decode
        let pts: Vec<(i32, f32)> = l.points.iter().map(|&(t, b)| (t, value(b))).collect();
        let mut sim = Sim::new(&pts);
        let mut leg = Sim::new(&[(0, 0.0), (30000, 1.0)]);
        for k in 0..320 {
            let got = law_of(t).map(sim.step(100));
            let want = law_of(t).map(eval(leg.step(100))[t]);
            assert_eq!(got, want, "block {k}");
        }
    }

    #[test]
    fn automation_before_the_song_start_and_a_single_point_work() {
        let l = &convert(&[(-500, 0), (500, 255)], &[0], &[], &eval)[0];
        assert_eq!(l.points[0].0, -500);
        let l = &convert(&[(300, 128)], &[0], &[], &eval)[0];
        assert_eq!(l.points, vec![(300, byte(eval(value(128))[0]))]);
    }

    #[test]
    fn affected_finds_the_parameters_a_mapping_moves() {
        let probe = |b: u8| eval(value(b));
        assert_eq!(affected(&probe), vec![0, pm::filt(0, pm::f::TYPE)]);
    }
}
