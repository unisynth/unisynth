//! Host-side parameter metadata: names and the law of every index (tests, converter, editor, fitter). Compiled out
//! of the player.

use super::*;
use crate::law;
use crate::law::host as inv;
use std::format;
use std::string::String;

/// How a normalized value maps to its physical value. `Shape`, `ModAmount` and `EnvTime` depend on a sibling value
/// (oscillator type, mod operation, envelope mode); [`Law::map`] returns `p` for them and the module resolves them.
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
    /// Envelope time: ms in timed mode, rate-based in rate mode.
    EnvTime,
    /// Meaning depends on the oscillator type ([`shape`]).
    Shape,
    /// Meaning depends on the slot operation (Scale or Offset).
    ModAmount,
    /// Stored as is (sample index).
    Raw,
    /// An expanded choice (`params::kinds`): `old` kinds of chunk v1, `all` kinds now; the value is the kind id.
    Kinds { old: usize, all: usize },
    /// A V2 field: `raw = 128·p` (design D4a).
    V2Raw,
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
            Law::Kinds { old, all } => kinds::decode(p, old, all - old, 0) as f32,
            Law::V2Raw => v2_raw(p),
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
            Law::Kinds { old, all } => {
                if v as usize >= all {
                    return None;
                }
                kinds::encode(v as usize, old)
            }
            Law::V2Raw => v / 128.0,
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
    if let Some(fld) = group(OSC, o::LEN, OSC_SLOTS) {
        return match fld {
            o::TYPE => Kinds { old: osc_type::COUNT, all: osc_type::ALL },
            o::TRACKING => Choice(2),
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
            o::LEN.. => Unit,
            _ => Shape,
        };
    }
    if let Some(fld) = group(NOISE, n::LEN, 1) {
        return match fld {
            n::BAND_ON => Choice(2),
            _ => Cutoff,
        };
    }
    if let Some(fld) = group(FILT, f::LEN, FILT_SLOTS) {
        return match fld {
            f::TYPE => Kinds { old: filter_type::COUNT, all: filter_type::ALL },
            f::CUTOFF => Cutoff,
            _ => Unit,
        };
    }
    if let Some(fld) = group(ENV, e::LEN, ENV_SLOTS) {
        return match fld {
            e::MODE => Kinds { old: env_mode::COUNT, all: env_mode::ALL },
            e::ATTACK | e::DECAY | e::RELEASE => EnvTime,
            _ => Unit,
        };
    }
    if let Some(fld) = group(LFO, l::LEN, LFO_SLOTS) {
        return match fld {
            l::SHAPE => Choice(5),
            l::RATE => LfoRate,
            l::SYNC => Choice(2),
            _ => Unit,
        };
    }
    let modfld = if let Some(f) = group(MOD, m::LEN, MOD_SLOTS1) { Some(f) } else { group(MOD2, m::LEN, MOD_SLOTS - MOD_SLOTS1) };
    if let Some(fld) = modfld {
        use crate::modmx::{op, source};
        return match fld {
            m::SOURCE => Kinds { old: source::COUNT_V1, all: source::COUNT },
            m::TARGET => Index,
            m::AMOUNT => ModAmount,
            _ => Kinds { old: op::COUNT_V1, all: op::COUNT },
        };
    }
    if i == v2::VOICE || i == ch::ON || i == ch::SPEECH || i == lfo_ext(0, lx::MODE) || i == lfo_ext(1, lx::MODE) {
        return Choice(2);
    }
    if i == v2::ERA {
        return Kinds { old: 7, all: 8 };
    }
    if i >= V2G && i < COUNT {
        return V2Raw;
    }
    if is_raw(i) {
        return Raw;
    }
    if i == ctl::BEND_RANGE {
        return BendRange;
    }
    Unit
}

/// Parameter name, e.g. `osc2.gain`, `filt1.cutoff`, `mod7.amount` (1-based slots).
pub fn name(i: usize) -> String {
    const V: [&str; v::LEN] = [
        "master", "pan", "capacity", "steal", "unison", "uni_detune", "uni_spread", "mode", "slide", "glide",
        "transpose", "velocity", "start_spread", "one_shot", "noise_level", "env1_at_output",
    ];
    const O: [&str; o::LEN] = [
        "type", "tracking", "coarse", "fine", "octave", "ratio", "ratio_fine", "fixed_hz", "gain", "mix", "pan",
        "pm_out", "feedback", "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8",
    ];
    const N: [&str; n::LEN] = ["band_on", "hp", "lp"];
    const F: [&str; f::LEN] = ["type", "cutoff", "resonance"];
    const E: [&str; e::LEN] = ["law", "attack", "decay", "sustain", "release", "curve_slope"];
    const L: [&str; l::LEN] = ["shape", "rate", "depth", "sync"];
    const M: [&str; m::LEN] = ["source", "target", "amount", "operation"];
    let slot = |base: usize, stride: usize, names: &[&str], group: &str| {
        let r = i - base;
        format!("{group}{}.{}", r / stride + 1, names[r % stride])
    };
    if i < v::LEN {
        String::from(V[i])
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
    } else if i < SMP {
        slot(MOD, m::LEN, &M, "mod")
    } else if i < CTL {
        format!("osc{}.sample", i - SMP + 1)
    } else if i < EXT {
        String::from(["ctl.wheel", "ctl.bend", "ctl.bend_range"][i - CTL])
    } else if i == ext::NOISE_WIDTH {
        String::from("noise.width")
    } else if i < VDIST {
        // `voice.stage` and `voice.era` leave the layout at the format switch (change unify-v2-algorithms D4)
        String::from(["voice.stage", "voice.keysync", "voice.era", "voice.routing", "voice.balance"][i - V2G])
    } else if i < ENV_AMP {
        format!("drive.{}", ["mode", "gain", "p1", "p2"][i - VDIST])
    } else if i < LFO_EXT {
        format!("env{}.amplify", i - ENV_AMP + 1)
    } else if i < MOD2 {
        let r = i - LFO_EXT;
        format!("lfo{}.{}", r / lx::LEN + 1, ["law", "phase", "polarity", "one_shot"][r % lx::LEN])
    } else if i < CC {
        slot(MOD2 - MOD_SLOTS1 * m::LEN, m::LEN, &M, "mod")
    } else if i < CH {
        format!("ctl.cc{}", i - CC + 2)
    } else if i < CH + ch::LEN {
        String::from(
            [
                "strip.on", "strip.volume", "strip.boost", "strip.order", "strip.send_reverb", "strip.send_delay", "strip.send_aux_a", "strip.send_aux_b",
                "strip.recv_aux_a", "strip.recv_aux_b", "speech.on", "speech.select", "speech.pitch",
            ][i - CH],
        )
    } else if i >= ch::DIST && i < ch::CHORUS {
        format!("strip.drive_{}", ["mode", "gain", "p1", "p2"][i - ch::DIST])
    } else if i >= ch::CHORUS && i < ch::COMP {
        format!("strip.chorus_{}", ["amount", "feedback", "delay_l", "delay_r", "rate", "depth", "phase"][i - ch::CHORUS])
    } else if i >= ch::COMP && i < COUNT {
        format!(
            "strip.comp_{}",
            ["mode", "couple", "auto_gain", "lookahead", "threshold", "ratio", "attack", "release", "out_gain"][i - ch::COMP]
        )
    } else {
        format!("param{i}")
    }
}
