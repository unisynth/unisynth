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
        let ty = law::choice(val[t], osc_type::COUNT);
        let kind = |what: &str| format!("{} is {what}", n(t));
        match ty {
            osc_type::PM => add(&mut out, m::OSC_PM, kind("phase-mod")),
            osc_type::BLIT => add(&mut out, m::OSC_BLIT, kind("saw/pulse")),
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
        for ty in choices(t, ft::COUNT) {
            let (id, what) = match ty {
                ft::SVF_LP | ft::SVF_HP | ft::SVF_BP | ft::SVF_NOTCH => (m::FILTER_SVF, "a state-variable filter"),
                ft::LADDER_LP => (m::FILTER_LADDER, "the ladder filter"),
                ft::RBJ_HP | ft::RBJ_BP => (m::FILTER_RBJ, "an RBJ filter"),
                ft::NOTCH => (m::FILTER_NOTCH, "the notch filter"),
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
        let direct = (0..pm::MOD_SLOTS).find(|&s| law::choice(val[pm::modslot(s, pm::m::SOURCE)], source::COUNT) == source::ENV1 + env);
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
            for mode in choices(mt, 2) {
                let (id, what) = if mode == 1 { (m::ENV_RATE, "rate") } else { (m::ENV_TIMED, "timed") };
                add(&mut out, id, format!("{} is {what} mode, {why}{}", n(mt), lane(mt)));
            }
        }
    }
    let routed = (0..pm::MOD_SLOTS).find(|&s| {
        law::choice(val[pm::modslot(s, pm::m::SOURCE)], source::COUNT) != source::NONE
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
    Ok(out)
}

/// The module of an effect chunk (kinds 65–73), or `None` for a kind that is not an effect.
pub fn effect(kind: u8, chunk: &[u8]) -> Option<u8> {
    crate::fx::module_of(kind, crate::fx::mode_of(kind, chunk))
}

/// The cargo feature that enables module `id`.
pub fn feature(id: u8) -> &'static str {
    match id {
        m::OSC_PM => "us-osc-pm",
        m::OSC_BLIT => "us-osc-blit",
        m::OSC_WAVETABLE => "us-osc-wt",
        m::OSC_SAMPLER => "us-osc-sample",
        m::SAMPLE_GSM => "us-sample-gsm",
        m::SAMPLE_DLS => "us-sample-dls",
        m::NOISE => "us-noise",
        m::FILTER_SVF => "us-flt-svf",
        m::FILTER_LADDER => "us-flt-ladder",
        m::FILTER_RBJ => "us-flt-rbj",
        m::FILTER_NOTCH => "us-flt-notch",
        m::ENV_TIMED => "us-env-timed",
        m::ENV_RATE => "us-env-rate",
        m::FX_DISTORTION_SHAPER => "us-fx-distortion-shaper",
        m::FX_DISTORTION_POWER => "us-fx-distortion-power",
        m::FX_CRUSHER => "us-fx-crusher",
        m::FX_EQ_PARAMETRIC => "us-fx-eq-parametric",
        m::FX_EQ_3BAND => "us-fx-eq-3band",
        m::FX_DELAY_TEMPO => "us-fx-delay-tempo",
        m::FX_DELAY_MS => "us-fx-delay-ms",
        m::FX_CHORUS => "us-fx-chorus",
        m::FX_FLANGER_PHASER => "us-fx-flanger-phaser",
        m::FX_FLANGER_RING => "us-fx-flanger-ring",
        m::FX_REVERB_FREEVERB => "us-fx-reverb-freeverb",
        m::FX_REVERB_COMPACT => "us-fx-reverb-compact",
        m::FX_REVERB_ROOMS => "us-fx-reverb-rooms",
        m::FX_COMPRESSOR => "us-fx-compressor",
        m::FX_FORMANT => "us-fx-formant",
        _ => "",
    }
}
