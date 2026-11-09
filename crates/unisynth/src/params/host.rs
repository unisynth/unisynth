//! Host-side parameter metadata: names and the law of every index (tests, converter, editor, fitter). Compiled out
//! of the player.

use super::*;
use crate::law;
use crate::law::host as inv;
use std::format;
use std::string::String;

/// How a normalized value maps to its physical value. `Shape`, `ModAmount` and `EnvTime` depend on a sibling value
/// (oscillator type, mod operation, envelope law); [`Law::map`] returns `p` for them and the module resolves them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Law {
    Unit,
    Cutoff,
    Gain,
    LfoRate,
    Time,
    Transpose,
    /// Bend range in semitones, `0..24`.
    BendRange,
    Coarse,
    Fine,
    Octave,
    Ratio,
    RatioFine,
    FixedHz,
    Feedback,
    PmOut,
    Pan,
    Choice(usize),
    Count(u32, u32),
    Index,
    /// Envelope time: ms on the seconds law, rate-based on the rate law, 7-bit step on the frame law.
    EnvTime,
    /// Meaning depends on the oscillator type ([`shape`]).
    Shape,
    /// Meaning depends on the slot operation (Scale or Offset).
    ModAmount,
    /// Stored as is (sample index).
    Raw,
    /// A 7-bit step: `step = 128·p`.
    Steps,
}

impl Law {
    /// Physical value for `p` (module-dependent laws return `p`).
    pub fn map(self, p: f32) -> f32 {
        match self {
            Law::Cutoff => law::cutoff(p),
            Law::Gain => law::gain(p),
            Law::LfoRate => law::lfo_rate(p),
            Law::Time => law::time(p),
            Law::Transpose => law::transpose(p),
            Law::BendRange => law::bend_range(p),
            Law::Coarse => law::coarse(p),
            Law::Fine => law::fine(p),
            Law::Octave => law::octave(p),
            Law::Ratio => law::ratio(p),
            Law::RatioFine => law::ratio_fine(p),
            Law::FixedHz => law::fixed_hz(p),
            Law::Feedback => law::feedback(p),
            Law::PmOut => law::pm_out(p),
            Law::Choice(n) => law::choice(p, n) as f32,
            Law::Count(lo, hi) => law::count(p, lo, hi) as f32,
            Law::Index => law::index(p) as f32,
            Law::Steps => v2_raw(p),
            Law::Pan | Law::Unit | Law::EnvTime | Law::Shape | Law::ModAmount | Law::Raw => p,
        }
    }

    /// `p` for a physical value (`None` for module-dependent laws and `Pan`, which has two outputs).
    pub fn inverse(self, v: f32) -> Option<f32> {
        Some(match self {
            Law::Cutoff => inv::inv_cutoff(v),
            Law::Gain => inv::inv_gain(v),
            Law::LfoRate => inv::inv_lfo_rate(v),
            Law::Time => inv::inv_time(v),
            Law::Transpose => inv::inv_transpose(v),
            Law::BendRange => inv::inv_bend_range(v),
            Law::Coarse => inv::inv_coarse(v),
            Law::Fine => inv::inv_fine(v),
            Law::Octave => inv::inv_octave(v),
            Law::Ratio => inv::inv_ratio(v),
            Law::FixedHz => inv::inv_fixed_hz(v),
            Law::Feedback => inv::inv_feedback(v),
            Law::PmOut => inv::inv_pm_out(v),
            Law::Choice(n) => inv::inv_choice(v as usize, n),
            Law::Count(lo, hi) => inv::inv_count(v as u32, lo, hi),
            Law::Index => inv::inv_index(v as u8),
            Law::Steps => v / 128.0,
            Law::Unit => v,
            Law::RatioFine | Law::Pan | Law::EnvTime | Law::Shape | Law::ModAmount | Law::Raw => return None,
        })
    }
}

/// The law of parameter `i` (`Unit` past the end).
pub fn law_of(i: usize) -> Law {
    use Law::*;
    let group = |base: usize, stride: usize, slots: usize| -> Option<usize> {
        (i >= base && i < base + stride * slots).then(|| (i - base) % stride)
    };
    if i < v::LEN {
        return match i {
            v::MASTER | v::NOISE_LEVEL => Gain,
            v::PAN => Pan,
            v::CAPACITY => Count(1, 64),
            v::STEAL | v::MODE | v::ONE_SHOT | v::ENV1_AT_OUTPUT => Choice(2),
            v::UNISON => Count(1, 16),
            v::SLIDE | v::GLIDE => Time,
            v::TRANSPOSE => Transpose,
            _ => Unit,
        };
    }
    if i >= VS && i < VS + vs::LEN {
        return match i {
            vs::KEYSYNC => Choice(vs::keysync::COUNT),
            vs::BALANCE => Steps,
            _ => Choice(2),
        };
    }
    if let Some(fld) = group(OSC, o::LEN, OSC_SLOTS) {
        return match fld {
            o::TYPE => Choice(osc_type::COUNT),
            o::TRACKING | o::LAW => Choice(2),
            o::COMBINE => Choice(combine::COUNT),
            o::SAMPLE => Raw,
            o::COARSE => Coarse,
            o::FINE => Fine,
            o::OCTAVE => Octave,
            o::RATIO => Ratio,
            o::RATIO_FINE => RatioFine,
            o::FIXED_HZ => FixedHz,
            o::GAIN => Gain,
            o::PAN => Pan,
            o::PM_OUT => PmOut,
            o::FEEDBACK => Feedback,
            o::MIX => Unit,
            _ => Shape,
        };
    }
    if let Some(fld) = group(NOISE, n::LEN, 1) {
        return match fld {
            n::BAND_ON => Choice(2),
            n::WIDTH => Unit,
            _ => Cutoff,
        };
    }
    if let Some(fld) = group(FILT, f::LEN, FILT_SLOTS) {
        return match fld {
            f::TYPE => Choice(filter_type::COUNT),
            f::CUTOFF => Cutoff,
            f::RESONANCE => Unit,
            _ => Choice(2),
        };
    }
    if let Some(fld) = group(ENV, e::LEN, ENV_SLOTS) {
        return match fld {
            e::LAW => Choice(env_law::COUNT),
            e::CURVE => Choice(env_curve::COUNT),
            e::ATTACK | e::DECAY | e::RELEASE => EnvTime,
            e::SUSTAIN_SLOPE | e::AMPLIFY => Steps,
            _ => Unit,
        };
    }
    if let Some(fld) = group(LFO, l::LEN, LFO_SLOTS) {
        return match fld {
            l::SHAPE => Choice(crate::lfo::shape::ALL),
            l::RATE => LfoRate,
            l::SYNC => Choice(lfo_sync::COUNT),
            l::LAW | l::ONE_SHOT => Choice(2),
            l::POLARITY => Choice(3),
            l::PHASE => Steps,
            _ => Unit,
        };
    }
    let modfld = if let Some(f) = group(MOD, m::LEN, MOD_SLOTS1) { Some(f) } else { group(MOD2, m::LEN, MOD_SLOTS - MOD_SLOTS1) };
    if let Some(fld) = modfld {
        use crate::modmx::{op, source};
        return match fld {
            m::SOURCE => Choice(source::COUNT),
            m::TARGET => Index,
            m::AMOUNT => ModAmount,
            _ => Choice(op::COUNT),
        };
    }
    if i == ctl::BEND_RANGE {
        return BendRange;
    }
    if i >= CTL && i < CTL + ctl::LEN {
        return Unit;
    }
    if (i >= QUIRK && i < QUIRK + quirk::LEN) || i == ch::ON || i == ch::DC_FILTER || i == speech::ON {
        return Choice(2);
    }
    if let Some(fld) = group(PTS, pt::LEN, OSC_SLOTS) {
        return match fld {
            pt::COUNT_ => Count(1, 4),
            pt::LAYERS => Count(1, 11),
            pt::DETUNE | pt::SPREAD => Shape,
            _ => Choice(3),
        };
    }
    if i < COUNT {
        return Steps;
    }
    Unit
}

/// Parameter name, e.g. `osc2.gain`, `filt1.cutoff`, `mod7.amount` (1-based slots).
pub fn name(i: usize) -> String {
    const V: [&str; v::LEN] = [
        "master", "pan", "capacity", "steal", "unison", "uni_detune", "uni_spread", "mode", "slide", "glide",
        "transpose", "velocity", "start_spread", "one_shot", "noise_level", "env1_at_output",
    ];
    const VSN: [&str; vs::LEN] =
        ["voice.amp_source", "voice.pan_stage", "voice.dc_filter", "voice.law", "voice.keysync", "voice.routing", "voice.balance"];
    const O: [&str; o::LEN] = [
        "type", "tracking", "coarse", "fine", "octave", "ratio", "ratio_fine", "fixed_hz", "gain", "mix", "pan",
        "pm_out", "feedback", "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "sample", "combine", "law",
    ];
    const N: [&str; n::LEN] = ["band_on", "hp", "lp", "width"];
    const F: [&str; f::LEN] = ["type", "cutoff", "resonance", "form", "oversample", "law"];
    const E: [&str; e::LEN] = ["law", "attack", "decay", "sustain", "release", "curve", "curve_slope", "sustain_slope", "amplify"];
    const L: [&str; l::LEN] = ["shape", "rate", "depth", "sync", "law", "phase", "polarity", "one_shot"];
    const M: [&str; m::LEN] = ["source", "target", "amount", "operation"];
    const Q: [&str; quirk::LEN] = ["exact_math", "integer_fm", "fixed_seed", "lcg_noise", "coarse_env"];
    const S: [&str; 15] = [
        "on", "volume", "drive_mode", "drive_gain", "drive_p1", "drive_p2", "send_reverb", "send_delay", "boost",
        "order", "send_aux_a", "send_aux_b", "recv_aux_a", "recv_aux_b", "dc_filter",
    ];
    let slot = |base: usize, stride: usize, names: &[&str], group: &str| {
        let r = i - base;
        format!("{group}{}.{}", r / stride + 1, names[r % stride])
    };
    if i < v::LEN {
        String::from(V[i])
    } else if i < OSC {
        String::from(VSN[i - VS])
    } else if i < NOISE {
        slot(OSC, o::LEN, &O, "osc")
    } else if i < FILT {
        format!("noise.{}", N[i - NOISE])
    } else if i < ENV {
        slot(FILT, f::LEN, &F, "filt")
    } else if i < LFO {
        slot(ENV, e::LEN, &E, "env")
    } else if i < MOD {
        slot(LFO, l::LEN, &L, "lfo")
    } else if i < CTL {
        slot(MOD, m::LEN, &M, "mod")
    } else if i < DRIVE {
        String::from(["ctl.wheel", "ctl.bend", "ctl.bend_range"][i - CTL])
    } else if i < QUIRK {
        format!("drive.{}", ["mode", "gain", "p1", "p2"][i - DRIVE])
    } else if i < STRIP {
        format!("quirk.{}", Q[i - QUIRK])
    } else if i < ch::CHORUS {
        format!("strip.{}", S[i - STRIP])
    } else if i < ch::COMP {
        format!("strip.chorus_{}", ["amount", "feedback", "delay_l", "delay_r", "rate", "depth", "phase"][i - ch::CHORUS])
    } else if i < MOD2 {
        format!(
            "strip.comp_{}",
            ["mode", "couple", "auto_gain", "lookahead", "threshold", "ratio", "attack", "release", "out_gain"][i - ch::COMP]
        )
    } else if i < CC {
        slot(MOD2 - MOD_SLOTS1 * m::LEN, m::LEN, &M, "mod")
    } else if i < SPEECH {
        format!("ctl.cc{}", i - CC + 2)
    } else if i < PTS {
        format!("speech.{}", ["on", "select", "pitch"][i - SPEECH])
    } else if i < COUNT {
        const P: [&str; pt::LEN] =
            ["points", "mode1", "mode2", "mode3", "mode4", "mode_last", "layers", "layer_detune", "layer_spread"];
        slot(PTS, pt::LEN, &P, "osc")
    } else {
        format!("param{i}")
    }
}
