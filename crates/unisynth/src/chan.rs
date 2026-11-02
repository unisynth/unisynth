//! The channel stage of the instrument (change add-v2-import, design D5, `us-strip`): V2's per-channel strip after the
//! voice sum. Aux receive, DC filter (format version 6), compressor, bass boost, then distortion → DC filter → chorus
//! or chorus → distortion → DC filter (`ch::FXROUTE`), then the sends (reverb and delay as mono mixes, aux A/B
//! stereo) and the channel volume (`0.6 · vol/128`). Settings are V2 raw values (`params::v2_raw`) read every
//! control-grid step from the parameters with the channel Add routes of the newest voice applied (V2's
//! `storeChanValues`).

use crate::params::{self as pm, ch, dist, v2_raw};
use crate::v2::{dcf_r, Dcf, Dist, DistP};
use crate::v2fx::{Boost, BoostP, Comp, CompP, ModDel, ModDelP};
use synth_core::mix::LoadError;
use synth_core::pool::Pool;

/// The chorus/flanger buffer length per side (V2's `chandelbuf`).
pub const CHORUS_LEN: usize = 2048;
/// V2's `fcgain`, `fcgainh`.
const GAIN: f32 = 0.6;
const GAIN_H: f32 = 0.6;

/// Whether the patch runs the channel stage.
pub fn on(p: &[f32; pm::COUNT]) -> bool {
    cfg!(feature = "us-strip") && crate::law::choice(p[ch::ON], 2) == 1
}

/// The settings of one control step.
#[derive(Clone, Copy, Debug, Default)]
pub struct ChanP {
    pub chgain: f32,
    /// Reverb and delay sends (mono mixes), aux A/B sends (stereo), aux A/B receive.
    pub send: [f32; 4],
    pub recv: [f32; 2],
    pub chorus_first: bool,
    pub dist: DistP,
    pub chorus: ModDelP,
    pub comp: CompP,
    pub boost: BoostP,
    /// The strip's DC filters (`ch::DC_FILTER`).
    pub v6: bool,
}

impl ChanP {
    /// From the parameters (channel Adds already applied).
    pub fn new(p: &[f32; pm::COUNT], rate: f32) -> Self {
        let r = |i: usize| v2_raw(p[i]);
        let chgain = GAIN * (r(ch::VOL) / 128.0);
        let v6 = crate::law::choice(p[ch::DC_FILTER], 2) == 1;
        let exact = cfg!(feature = "us-quirk-exact-math") && pm::quirk_on(p, pm::quirk::EXACT_MATH);
        let d = ch::DIST;
        let c = ch::CHORUS;
        let k = ch::COMP;
        ChanP {
            chgain,
            send: [
                (r(ch::SEND_RVB) / 128.0) * GAIN_H * chgain,
                (r(ch::SEND_DLY) / 128.0) * GAIN_H * chgain,
                GAIN * (r(ch::SEND_AUXA) / 128.0),
                GAIN * (r(ch::SEND_AUXB) / 128.0),
            ],
            recv: [r(ch::RECV_AUXA) / 128.0, r(ch::RECV_AUXB) / 128.0],
            chorus_first: r(ch::FXROUTE) as i32 != 0,
            dist: DistP::new(r(d), r(d + dist::GAIN), r(d + dist::P1), r(d + dist::P2), rate).native_atan(exact),
            chorus: ModDelP::new(r(c), r(c + 1), r(c + 2), r(c + 3), r(c + 4), r(c + 5), r(c + 6), CHORUS_LEN, rate),
            comp: CompP::new(r(k), r(k + 1), r(k + 2), r(k + 3), r(k + 4), r(k + 5), r(k + 6), r(k + 7), r(k + 8), rate),
            boost: BoostP::new(r(ch::BOOST), rate),
            v6,
        }
    }
}

/// The channel stage state.
pub struct Chan<'a> {
    dcf1: [Dcf; 2],
    dcf2: [Dcf; 2],
    comp: Comp<'a>,
    boost: Boost,
    dist: Dist,
    chorus: ModDel<'a>,
}

impl<'a> Chan<'a> {
    pub const fn need() -> usize {
        Comp::need() + ModDel::need(CHORUS_LEN)
    }

    pub fn new(pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let oom = |_| LoadError::OutOfMemory;
        Ok(Chan {
            dcf1: [Dcf::default(); 2],
            dcf2: [Dcf::default(); 2],
            comp: Comp::new(pool).map_err(oom)?,
            boost: Boost::default(),
            dist: Dist::default(),
            chorus: ModDel::new(pool, CHORUS_LEN).map_err(oom)?,
        })
    }

    pub fn reset(&mut self) {
        self.dcf1 = [Dcf::default(); 2];
        self.dcf2 = [Dcf::default(); 2];
        self.comp.clear();
        self.boost = Boost::default();
        self.dist = Dist::default();
        self.chorus.clear();
    }

    /// Runs the strip on `l`/`r` (the voice sum) in place and leaves the channel output (× volume) there. `sends`
    /// receives the reverb and delay mono mixes (`[rvb, dly]`, added) and aux A/B (stereo, added) when given;
    /// `aux` is the aux input (A, B) added at the start.
    pub fn process(&mut self, p: &ChanP, l: &mut [f32], r: &mut [f32], rate: f32, mut sends: Option<Sends>, aux: Option<[(&[f32], &[f32]); 2]>) {
        self.comp.set(&p.comp);
        if let Some(aux) = aux {
            for (k, (al, ar)) in aux.iter().enumerate() {
                if p.recv[k] != 0.0 {
                    l.iter_mut().zip(al.iter()).for_each(|(x, a)| *x += p.recv[k] * a);
                    r.iter_mut().zip(ar.iter()).for_each(|(x, a)| *x += p.recv[k] * a);
                }
            }
        }
        let dr = dcf_r(rate);
        let dcf = |d: &mut [Dcf; 2], l: &mut [f32], r: &mut [f32]| {
            l.iter_mut().for_each(|x| *x = d[0].step(*x, dr));
            r.iter_mut().for_each(|x| *x = d[1].step(*x, dr));
        };
        if p.v6 {
            dcf(&mut self.dcf1, l, r);
        }
        self.comp.render(&p.comp, l, r);
        self.boost.render(&p.boost, l, r);
        let dist = |d: &mut Dist, l: &mut [f32], r: &mut [f32]| {
            if p.dist.mode != 0 {
                for (a, b) in l.iter_mut().zip(r.iter_mut()) {
                    (*a, *b) = d.run(&p.dist, *a, *b);
                }
            }
        };
        if p.chorus_first {
            self.chorus.render_chan(&p.chorus, l, r);
            dist(&mut self.dist, l, r);
            if p.v6 {
                dcf(&mut self.dcf2, l, r);
            }
        } else {
            dist(&mut self.dist, l, r);
            if p.v6 {
                dcf(&mut self.dcf2, l, r);
            }
            self.chorus.render_chan(&p.chorus, l, r);
        }
        if let Some(s) = sends.as_mut() {
            for i in 0..l.len() {
                let m = l[i] + r[i];
                s.rvb[i] += p.send[0] * m;
                s.dly[i] += p.send[1] * m;
                s.aux[0].0[i] += p.send[2] * l[i];
                s.aux[0].1[i] += p.send[2] * r[i];
                s.aux[1].0[i] += p.send[3] * l[i];
                s.aux[1].1[i] += p.send[3] * r[i];
            }
        }
        l.iter_mut().for_each(|x| *x *= p.chgain);
        r.iter_mut().for_each(|x| *x *= p.chgain);
    }
}

/// Where the channel's sends go: reverb and delay (mono mixes), aux A and B (stereo).
pub struct Sends<'s> {
    pub rvb: &'s mut [f32],
    pub dly: &'s mut [f32],
    pub aux: [(&'s mut [f32], &'s mut [f32]); 2],
}
