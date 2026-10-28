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
    // the kinds an expanded choice (`params::kinds`) can take over its span
    let kinds = |i: usize, old: usize, all: usize, none: usize| {
        let (lo, hi) = span(i);
        let n = 2 * old - 1;
        (law::choice(lo, n)..=law::choice(hi, n)).map(move |c| pm::kinds::at(c, old, all - old, none))
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
            osc_type::V2 => add(&mut out, m::OSC_V2, kind("a V2 oscillator")),
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
    // a GSM index must name an entry of the chunk's sample section, so the section is the only sign of a GSM source
    if c.sample_count() > 0 {
        add(&mut out, m::SAMPLE_GSM, format!("the chunk holds {} GSM sample(s)", c.sample_count()));
    }
    for i in 0..pm::FILT_SLOTS {
        let t = pm::filt(i, pm::f::TYPE);
        for ty in kinds(t, ft::COUNT, ft::ALL, ft::OFF) {
            let (id, what) = match ty {
                ft::SVF_LP | ft::SVF_HP | ft::SVF_BP | ft::SVF_NOTCH => (m::FILTER_SVF, "a state-variable filter"),
                ft::LADDER_LP => (m::FILTER_LADDER, "the ladder filter"),
                ft::RBJ_HP | ft::RBJ_BP => (m::FILTER_RBJ, "an RBJ filter"),
                ft::NOTCH => (m::FILTER_NOTCH, "the notch filter"),
                ft::V2_LP..=ft::V2_MOOG_HP => (m::FILTER_V2, "a V2 filter"),
                _ => continue,
            };
            add(&mut out, id, format!("{} can be {what}{}", n(t), lane(t)));
        }
    }
    // a lane on a routing parameter can create any route, so it is taken to read every envelope and reach the noise
    let rerouting = (0..pm::MOD_SLOTS).find(|&s| {
        reach(pm::modslot(s, pm::m::SOURCE)).is_some() || reach(pm::modslot(s, pm::m::TARGET)).is_some()
    });
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
            let mt = pm::env(i, pm::e::MODE);
            for mode in kinds(mt, pm::env_mode::COUNT, pm::env_mode::ALL, pm::env_mode::TIMED) {
                let (id, what) = match mode {
                    pm::env_mode::RATE => (m::ENV_RATE, "rate"),
                    pm::env_mode::V2 => (m::ENV_V2, "V2"),
                    _ => (m::ENV_TIMED, "timed"),
                };
                add(&mut out, id, format!("{} is {what} mode, {why}{}", n(mt), lane(mt)));
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
        if let Some(k) = kinds(t, source::COUNT, source::ALL, source::NONE).find(|c| lfo.contains(c)) {
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
        add(&mut out, m::MATRIX_ADD, format!("mod{} is a slot 13–16, an Add route or reads a controller", s + 1));
    }
    if c.wide() {
        add(&mut out, m::CHUNK_WIDE, "the chunk has an index of 240 or more, or 255 or more pairs".into());
    }
    if !c.text().is_empty() {
        add(&mut out, m::SPEECH, format!("the chunk holds {} bytes of speech text", c.text().len()));
    }
    // the V2 modules (`instrument::check_v2`)
    let raw_hi = |i: usize| (pm::v2_raw(span(i).1) as i32, pm::v2_raw(span(i).0) as i32);
    for k in 0..pm::LFO_SLOTS {
        let t = pm::lfo_ext(k, pm::lx::MODE);
        let read = (0..pm::MOD_SLOTS).any(|s| source::decode(val[pm::modslot(s, pm::m::SOURCE)]) == source::LFO1 + k) || rerouting.is_some();
        if read && choices(t, 2).any(|c| c == 1) {
            add(&mut out, m::LFO_V2, format!("{} is V2 mode and read{}", n(t), lane(t)));
        }
    }
    let vd = pm::VDIST + pm::dist::MODE;
    if raw_hi(vd).0 != 0 || raw_hi(vd).1 != 0 {
        add(&mut out, m::VDIST, format!("{} is on{}", n(vd), lane(vd)));
    }
    let (rhi, rlo) = raw_hi(pm::v2::ROUTING);
    if rhi >= 2 && rlo <= 2 {
        add(&mut out, m::FILTER_ROUTE, format!("{} is parallel{}", n(pm::v2::ROUTING), lane(pm::v2::ROUTING)));
    }
    for (i, id, what) in [(pm::v2::VOICE, m::V2_VOICE, "the V2 voice"), (pm::ch::ON, m::CHAN, "the channel stage")] {
        if choices(i, 2).any(|c| c == 1) {
            add(&mut out, id, format!("{} is on ({what}){}", n(i), lane(i)));
        }
    }
    if let Some(i) = (0..pm::OSC_SLOTS).find(|&i| crate::osc::v2_aux(&val, i)) {
        add(&mut out, m::CHAN, format!("osc{} reads an aux bus (the extra input pairs)", i + 1));
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
