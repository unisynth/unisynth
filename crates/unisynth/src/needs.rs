//! Host side: the modules (and so the cargo features) a song needs, from its device chunks and the range each
//! automated parameter can reach. It mirrors the load-time checks (`check_modules`, `Kind::all`, `smp::pool_need`), so a
//! build with exactly these modules loads the song and plays every value its automation can set.

use crate::chunk::Chunk;
use crate::law;
use crate::module as m;
use crate::modmx::{source, target};
use crate::params::{self as pm, filter_type as ft, osc_type, shape, v};
use crate::params::name;
use std::collections::{BTreeMap, BTreeSet};
use std::{format, string::{String, ToString}, vec::Vec};
use synth_core::mix::LoadError;

/// The values an automated parameter takes, as `(lowest, highest)` in the normalized form the player reads.
pub type Reach<'a> = &'a dyn Fn(usize) -> Option<(f32, f32)>;

/// The modules an instrument needs, each with the reasons: the parameters (by name, as `osc2.type`) that call for it.
pub type Why = BTreeMap<u8, Vec<String>>;

fn add(out: &mut Why, id: u8, why: String) {
    let w = out.entry(id).or_default();
    if !w.contains(&why) {
        w.push(why);
    }
}

/// Modules of an instrument chunk (kind 64). `reach(i)` is the span of parameter `i` when a lane drives it.
pub fn instrument(chunk: &[u8], reach: Reach) -> Result<BTreeSet<u8>, LoadError> {
    Ok(instrument_why(chunk, reach)?.into_keys().collect())
}

/// [`instrument`], with what calls for each module.
pub fn instrument_why(chunk: &[u8], reach: Reach) -> Result<Why, LoadError> {
    let c = Chunk::parse(chunk)?;
    let val = c.values();
    let span = |i: usize| match reach(i) {
        Some((lo, hi)) => (lo.min(val[i]), hi.max(val[i])),
        None => (val[i], val[i]),
    };
    // the choices a parameter with `n` values can take, from the lowest to the highest
    let choices = |i: usize, n: usize| {
        let (lo, hi) = span(i);
        law::choice(lo, n)..=law::choice(hi, n)
    };
    let n = name;
    let lane = |i: usize| if reach(i).is_some() { " (automated)" } else { "" };
    let mut out = Why::new();
    for i in 0..pm::OSC_SLOTS {
        let t = pm::osc(i, pm::o::TYPE);
        let ty = osc_type::decode(val[t]);
        let kind = |what: &str| format!("{} is {what}", n(t));
        match ty {
            osc_type::PM => add(&mut out, m::OSC_PM, kind("phase-mod")),
            osc_type::BLIT => add(&mut out, m::OSC_BLIT, kind("saw/pulse")),
            t if osc_type::is_naive(t) => add(&mut out, m::OSC_NAIVE, kind("a naive oscillator")),
            osc_type::CLOCKED => add(&mut out, m::OSC_CLOCKED, kind("a clocked oscillator")),
            osc_type::WAVETABLE => add(&mut out, m::OSC_WAVETABLE, kind("wavetable")),
            osc_type::SAMPLER => {
                add(&mut out, m::OSC_SAMPLER, kind("sampler"));
                let src = pm::osc(i, pm::o::SHAPE + shape::SMP_SOURCE);
                if law::choice(val[src], pm::smp_source::COUNT) != pm::smp_source::GSM {
                    let what = if law::choice(val[src], pm::smp_source::COUNT) == pm::smp_source::INSTRUMENT { "a DLS instrument" } else { "a DLS wave" };
                    add(&mut out, m::SAMPLE_DLS, format!("{} is {what}", n(src)));
                }
            }
            _ => {}
        }
    }
    for i in 0..pm::OSC_SLOTS {
        let c = pm::osc(i, pm::o::COMBINE);
        if osc_type::decode(val[pm::osc(i, pm::o::TYPE)]) != osc_type::OFF && choices(c, pm::combine::COUNT).any(|k| k != pm::combine::ADD) {
            add(&mut out, m::OSC_COMBINE, format!("{} multiplies or replaces{}", n(c), lane(c)));
        }
    }
    // a GSM index must name an entry of the chunk's sample section, so the section is the only sign of a GSM source
    if c.sample_count() > 0 {
        add(&mut out, m::SAMPLE_GSM, format!("the chunk holds {} GSM sample(s)", c.sample_count()));
    }
    for i in 0..pm::FILT_SLOTS {
        let t = pm::filt(i, pm::f::TYPE);
        // the forms (feedback, held / 2 steps, 7-bit law) a slot's fields can take: plain, formed, or both under lanes
        let fields = [pm::filt(i, pm::f::FORM), pm::filt(i, pm::f::OVERSAMPLE), pm::filt(i, pm::f::LAW)];
        let can_plain = fields.iter().all(|&f| choices(f, 2).any(|c| c == 0));
        let formed = fields.iter().find(|&&f| choices(f, 2).any(|c| c == 1)).copied();
        for ty in choices(t, ft::COUNT) {
            let (plain, what) = match ty {
                ft::SVF_LP | ft::SVF_HP | ft::SVF_BP | ft::SVF_NOTCH => (m::FILTER_SVF, "a state-variable filter"),
                ft::P4_LP => (m::FILTER_LADDER, "a 4-pole filter"),
                ft::SVF_ALL | ft::P4_HP => {
                    add(&mut out, m::FILTER_FEEDBACK, format!("{} can be an all-pass or 4-pole high-pass filter{}", n(t), lane(t)));
                    continue;
                }
                ft::RBJ_HP | ft::RBJ_BP => {
                    add(&mut out, m::FILTER_RBJ, format!("{} can be an RBJ filter{}", n(t), lane(t)));
                    continue;
                }
                ft::NOTCH => {
                    add(&mut out, m::FILTER_NOTCH, format!("{} can be the notch filter{}", n(t), lane(t)));
                    continue;
                }
                _ => continue,
            };
            if can_plain {
                add(&mut out, plain, format!("{} can be {what}{}", n(t), lane(t)));
            }
            if let Some(f) = formed {
                add(&mut out, m::FILTER_FEEDBACK, format!("{} can be {what} and {} is set{}", n(t), n(f), lane(f)));
            }
        }
    }
    // a lane on a routing parameter can create any route, so it is taken to read every envelope and reach the noise
    let rerouting = (0..pm::MOD_SLOTS).find(|&s| {
        reach(pm::modslot(s, pm::m::SOURCE)).is_some() || reach(pm::modslot(s, pm::m::TARGET)).is_some()
    });
    // a lane on an operation that reaches Add (review of the format switch)
    if let Some(s) = (0..pm::MOD_SLOTS).find(|&s| {
        let o = pm::modslot(s, pm::m::OPERATION);
        reach(o).is_some() && choices(o, crate::modmx::op::COUNT).any(|c| c == crate::modmx::op::ADD)
    }) {
        add(&mut out, m::MATRIX_ADD, format!("a lane on mod{} can make it an Add slot", s + 1));
    }
    let reads = |env: usize| {
        let direct = (0..pm::MOD_SLOTS).find(|&s| source::decode(val[pm::modslot(s, pm::m::SOURCE)]) == source::ENV1 + env);
        match (direct, rerouting) {
            (Some(s), _) => Some(format!("{} reads envelope {}", n(pm::modslot(s, pm::m::SOURCE)), env + 1)),
            (None, Some(s)) => Some(format!("a lane on mod{} can route envelope {}", s + 1, env + 1)),
            _ => None,
        }
    };
    for i in 0..pm::ENV_SLOTS {
        let why = if i == 0 { Some("envelope 1 ends the voice".to_string()) } else { reads(i) };
        if let Some(why) = why {
            let mt = pm::env(i, pm::e::LAW);
            let laws: Vec<usize> = choices(mt, pm::env_law::COUNT).collect();
            for &lw in &laws {
                let (id, what) = match lw {
                    pm::env_law::RATE => (m::ENV_RATE, "rate"),
                    pm::env_law::FRAME => (m::ENV_FRAME, "frame"),
                    _ => (m::ENV_SECONDS, "seconds"),
                };
                add(&mut out, id, format!("{} is the {what} law, {why}{}", n(mt), lane(mt)));
            }
            let ct = pm::env(i, pm::e::CURVE);
            for c in choices(ct, pm::env_curve::COUNT).filter(|&c| c != pm::env_curve::BY_LAW) {
                if c == pm::env_curve::POWER {
                    add(&mut out, m::ENV_RATE, format!("{} is the power curve, {why}{}", n(ct), lane(ct)));
                }
                if c == pm::env_curve::EXPONENTIAL {
                    add(&mut out, m::ENV_FRAME, format!("{} is the exponential curve, {why}{}", n(ct), lane(ct)));
                }
            }
            for (f, d) in [(pm::e::SUSTAIN_SLOPE, 0.5), (pm::e::AMPLIFY, 1.0)] {
                let fi = pm::env(i, f);
                if span(fi) != (d, d) {
                    add(&mut out, m::ENV_FRAME, format!("{} is set, {why}{}", n(fi), lane(fi)));
                }
            }
        }
    }
    let routed = (0..pm::MOD_SLOTS).find(|&s| {
        source::decode(val[pm::modslot(s, pm::m::SOURCE)]) != source::NONE
            && law::index(val[pm::modslot(s, pm::m::TARGET)]) == target::NOISE_LEVEL
    });
    if span(v::NOISE_LEVEL).1 > 0.0 {
        add(&mut out, m::NOISE, format!("{} is above 0{}", n(v::NOISE_LEVEL), lane(v::NOISE_LEVEL)));
    }
    if let Some(s) = routed {
        add(&mut out, m::NOISE, format!("mod{} routes to the noise level", s + 1));
    }
    if let Some(s) = rerouting {
        add(&mut out, m::NOISE, format!("a lane on mod{} can route to the noise level", s + 1));
    }
    // the optional voice paths (`instrument::check_voice`)
    for s in 0..pm::MOD_SLOTS {
        let t = pm::modslot(s, pm::m::SOURCE);
        let lfo = source::LFO1..source::LFO1 + pm::LFO_SLOTS;
        if let Some(k) = choices(t, source::COUNT).find(|c| lfo.contains(c)) {
            add(&mut out, m::VOICE_LFO, format!("{} reads LFO {}{}", n(t), k - source::LFO1 + 1, lane(t)));
        }
    }
    if let Some(s) = rerouting {
        add(&mut out, m::VOICE_LFO, format!("a lane on mod{} can route an LFO", s + 1));
    }
    let mono = choices(v::MODE, 2).any(|c| c == 1);
    if mono {
        add(&mut out, m::VOICE_MONO, format!("{} is mono-legato{}", n(v::MODE), lane(v::MODE)));
    }
    if span(v::GLIDE).1 > 0.0 {
        add(&mut out, m::VOICE_GLIDE, format!("{} is above 0{}", n(v::GLIDE), lane(v::GLIDE)));
    }
    if mono && span(v::SLIDE).1 > 0.0 {
        add(&mut out, m::VOICE_GLIDE, format!("{} is above 0 in mono mode{}", n(v::SLIDE), lane(v::SLIDE)));
    }
    if law::count(span(v::UNISON).1, 1, 16) > 1 {
        add(&mut out, m::VOICE_UNISON, format!("{} is above one voice{}", n(v::UNISON), lane(v::UNISON)));
    }
    if let Some(s) = (0..pm::MOD_SLOTS).find(|&s| crate::modmx::routes(&val, s)) {
        add(&mut out, m::MATRIX, format!("mod{} routes {} to {}", s + 1, n(pm::modslot(s, pm::m::SOURCE)), target::name(law::index(val[pm::modslot(s, pm::m::TARGET)]))));
    }
    if let Some(s) = rerouting {
        add(&mut out, m::MATRIX, format!("a lane on mod{} can create a route", s + 1));
        add(&mut out, m::MATRIX_ADD, format!("a lane on mod{} can create an Add route", s + 1));
    }
    if let Some(s) = (0..pm::MOD_SLOTS).find(|&s| crate::modmx::needs_add(&val, s)) {
        add(&mut out, m::MATRIX_ADD, format!("mod{} is a slot 11–16, an Add route or reads a controller", s + 1));
    }
    if c.wide() {
        add(&mut out, m::CHUNK_WIDE, "the chunk has an index of 240 or more, or 255 or more pairs".into());
    }
    if !c.text().is_empty() {
        add(&mut out, m::SPEECH, format!("the chunk holds {} bytes of speech text", c.text().len()));
    }
    // the mono-sum and voice-stage modules (`instrument::check_v2`)
    let raw_hi = |i: usize| (pm::v2_raw(span(i).1) as i32, pm::v2_raw(span(i).0) as i32);
    for k in 0..pm::LFO_SLOTS {
        let read = (0..pm::MOD_SLOTS).any(|s| source::decode(val[pm::modslot(s, pm::m::SOURCE)]) == source::LFO1 + k) || rerouting.is_some();
        if !read {
            continue;
        }
        let f = |fld: usize| pm::lfo(k, fld);
        let set = [
            (f(pm::l::LAW), choices(f(pm::l::LAW), 2).any(|c| c == 1)),
            (f(pm::l::SHAPE), choices(f(pm::l::SHAPE), crate::lfo::shape::ALL).any(|c| c >= crate::lfo::shape::COUNT_V1)),
            (f(pm::l::POLARITY), choices(f(pm::l::POLARITY), 3).any(|c| c != 0)),
            (f(pm::l::ONE_SHOT), choices(f(pm::l::ONE_SHOT), 2).any(|c| c == 1)),
            (f(pm::l::PHASE), span(f(pm::l::PHASE)).1 > 0.0),
            (f(pm::l::SYNC), choices(f(pm::l::SYNC), pm::lfo_sync::COUNT).any(|c| c == pm::lfo_sync::CONTINUE)),
        ];
        for (t, on) in set {
            if on {
                add(&mut out, m::LFO_FRAME, format!("{} is set and the LFO read{}", n(t), lane(t)));
            }
        }
    }
    let vd = pm::DRIVE + pm::dist::MODE;
    if raw_hi(vd).0 != 0 || raw_hi(vd).1 != 0 {
        add(&mut out, m::DRIVE, format!("{} is on{}", n(vd), lane(vd)));
    }
    let on_fields = [
        (pm::vs::ROUTING, m::FILTER_PARALLEL, "parallel filter routing"),
        (pm::vs::AMP_SOURCE, m::VOICE_OUT, "the voice output settings"),
        (pm::vs::PAN_STAGE, m::VOICE_OUT, "the voice output settings"),
        (pm::vs::DC_FILTER, m::VOICE_OUT, "the voice output settings"),
        (pm::vs::LAW, m::VOICE_OUT, "the voice output settings"),
        (pm::ch::ON, m::STRIP, "the channel strip"),
    ];
    for (i, id, what) in on_fields {
        if choices(i, 2).any(|c| c == 1) {
            add(&mut out, id, format!("{} is on ({what}){}", n(i), lane(i)));
        }
    }
    for q in pm::QUIRK..pm::QUIRK + pm::quirk::LEN {
        if pm::quirk_on(&val, q) {
            add(&mut out, m::quirk_module(q), format!("{} is on", n(q)));
        }
    }
    if let Some(i) = (0..pm::OSC_SLOTS).find(|&i| crate::osc::v2_aux(&val, i)) {
        add(&mut out, m::STRIP, format!("osc{} reads an aux bus (the extra input pairs)", i + 1));
    }
    if choices(v::ONE_SHOT, 2).any(|c| c == 1) {
        add(&mut out, m::VOICE_ONESHOT, format!("{} is on{}", n(v::ONE_SHOT), lane(v::ONE_SHOT)));
    }
    Ok(out)
}

/// The module of an effect chunk (kinds 65–73), or `None` for a kind that is not an effect.
pub fn effect(kind: u8, chunk: &[u8]) -> Option<u8> {
    crate::fx::module_of(kind, crate::fx::mode_of(kind, chunk))
}

/// The cargo feature that enables module `id` (the table lives in `module`).
pub fn feature(id: u8) -> &'static str {
    m::feature(id)
}
