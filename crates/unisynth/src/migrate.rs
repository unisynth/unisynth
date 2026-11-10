//! The migration from the old layout ([`crate::params::old`], chunk versions 1 and 2) to v3 (change
//! unify-v2-algorithms, task 6.5; design "v3 layout (frozen, task 6.4)", value rules). Host-only: the player reads
//! only v3. `research/unify-v3-layout/layout.py` is the reference implementation; both give the same pairs for every
//! tracked song (test `the_migration_matches_the_reference`).
//!
//! - [`values`]: a whole old patch → a v3 patch (the value rules).
//! - [`instrument_chunk`]: an old instrument chunk → a v3 chunk (version 1 → 3, 2 → 4; samples and speech text kept).
//! - [`lane`]: an automation lane on an old index → the v3 lanes it becomes (one per v3 field that moves).
//! - [`effect_chunk`], [`effect_param`]: an old effect chunk and an effect lane's index.

use crate::law;
use crate::params::{self as pm, old as po};
use std::format;
use std::string::{String, ToString};
use std::vec::Vec;

/// A migration that cannot keep the meaning: the old value has no v3 form. The message names the parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrateError(pub String);

impl core::fmt::Display for MigrateError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

fn raw(p: f32) -> i32 {
    po::v2_raw(p) as i32
}

fn kinds(p: f32, old: usize, later: usize, none: usize) -> usize {
    po::kinds::decode(p, old, later, none)
}

/// What the old format version decided (`params::old::era_settings`).
fn era(old: &[f32; po::COUNT]) -> po::EraSettings {
    po::era_settings(old)
}

/// The mod targets that are channel-strip parameters (`modmx::target::PARAMS` from the strip volume on).
fn is_strip_target(t: u8) -> bool {
    crate::modmx::target::param(t).is_some_and(pm::is_strip)
}

/// The v3 value of a naive generator's old mode (`s0`): `None` = off.
fn naive_type(mode: i32) -> Option<usize> {
    use pm::osc_type::*;
    Some(match mode {
        1 => NAIVE_SAW,
        2 => NAIVE_PULSE,
        3 => SINE,
        4 => NOISE,
        5 => FM,
        6 => AUX_A,
        7 => AUX_B,
        _ => return None,
    })
}

/// An old filter kind → (v3 type, form, oversample, law).
fn filter_rule(k: usize) -> (usize, usize, usize, usize) {
    use pm::filter_type::*;
    use po::filter_type as ft;
    match k {
        ft::SVF_LP => (SVF_LP, 0, 0, 0),
        ft::SVF_HP => (SVF_HP, 0, 0, 0),
        ft::SVF_BP => (SVF_BP, 0, 0, 0),
        ft::SVF_NOTCH => (SVF_NOTCH, 0, 0, 0),
        ft::LADDER_LP => (P4_LP, 0, 0, 0),
        ft::RBJ_HP => (RBJ_HP, 0, 0, 0),
        ft::RBJ_BP => (RBJ_BP, 0, 0, 0),
        ft::NOTCH => (NOTCH, 0, 0, 0),
        ft::V2_LP => (SVF_LP, 1, 1, 1),
        ft::V2_BP => (SVF_BP, 1, 1, 1),
        ft::V2_HP => (SVF_HP, 1, 1, 1),
        ft::V2_NOTCH => (SVF_NOTCH, 1, 1, 1),
        ft::V2_ALL => (SVF_ALL, 1, 1, 1),
        ft::V2_MOOG_LP => (P4_LP, 1, 1, 1),
        ft::V2_MOOG_HP => (P4_HP, 1, 1, 1),
        _ => (OFF, 0, 0, 0),
    }
}

fn cv(id: usize, n: usize) -> f32 {
    law::host::inv_choice(id, n)
}

/// A whole old patch → the v3 patch (design "v3 layout", value rules). Fails on a route the v3 layout cannot express
/// (a mod slot on a naive generator's mode or ring, which became structural).
pub fn values(old: &[f32; po::COUNT]) -> Result<[f32; pm::COUNT], MigrateError> {
    use pm::{ch, dist, e, f, l, m, n, o, vs};
    let era = era(old);
    let mut p = pm::DEFAULTS;
    p[..pm::v::LEN].copy_from_slice(&old[..po::v::LEN]);
    // voice settings
    if law::choice(old[po::v2::VOICE], 2) == 1 {
        p[vs::AMP_SOURCE] = 1.0;
        p[vs::PAN_STAGE] = 1.0;
        p[vs::LAW] = 1.0;
        if era.dc_filters {
            p[vs::DC_FILTER] = 1.0;
        }
    }
    let mut ks = raw(old[po::v2::KEYSYNC]).clamp(0, 2) as usize;
    if ks == 2 && !era.full_keysync {
        ks = 1;
    }
    p[vs::KEYSYNC] = cv(ks, vs::keysync::COUNT);
    p[vs::ROUTING] = if raw(old[po::v2::ROUTING]) == 2 { 1.0 } else { 0.0 };
    p[vs::BALANCE] = old[po::v2::BALANCE];
    // oscillators
    let (mut sine, mut fm, mut noise) = (false, false, false);
    for s in 0..pm::OSC_SLOTS {
        let (oi, ni) = (|fld| po::osc(s, fld), |fld| pm::osc(s, fld));
        for fld in o::TRACKING..o::SHAPE + o::SHAPES {
            p[ni(fld)] = old[oi(fld)];
        }
        p[ni(o::SAMPLE)] = old[po::smp_index(s)];
        let t = po::osc_type::decode(old[oi(po::o::TYPE)]);
        p[ni(o::TYPE)] = if t == po::osc_type::V2 {
            let mut mode = raw(old[oi(po::o::SHAPE + po::shape::V2_MODE)]) & 7;
            if (mode >= 6 && !era.aux_osc) || (mode == 5 && !era.fm_osc) {
                mode = 0;
            }
            let ring = raw(old[oi(po::o::SHAPE + po::shape::V2_RING)]) & 1 != 0;
            p[ni(o::SHAPE + po::shape::V2_MODE)] = 0.0;
            p[ni(o::SHAPE + po::shape::V2_RING)] = 0.0;
            match naive_type(mode) {
                None => pm::osc_type::encode(pm::osc_type::OFF),
                Some(t) => {
                    let c = if ring {
                        pm::combine::MUL
                    } else if mode >= 5 {
                        pm::combine::SET
                    } else {
                        pm::combine::ADD
                    };
                    p[ni(o::COMBINE)] = cv(c, pm::combine::COUNT);
                    p[ni(o::LAW)] = 1.0;
                    sine |= mode == 3;
                    fm |= mode == 5;
                    noise |= mode == 4;
                    pm::osc_type::encode(t)
                }
            }
        } else {
            use pm::osc_type as nt;
            use po::osc_type as ot;
            pm::osc_type::encode(match t {
                ot::PM => nt::PM,
                ot::BLIT => nt::BLIT,
                ot::WAVETABLE => nt::WAVETABLE,
                ot::SAMPLER => nt::SAMPLER,
                ot::CLOCKED => nt::CLOCKED,
                _ => nt::OFF,
            })
        };
    }
    for (k, i) in [po::NOISE + po::n::BAND_ON, po::NOISE + po::n::HP, po::NOISE + po::n::LP, po::ext::NOISE_WIDTH].into_iter().enumerate() {
        p[pm::NOISE + k] = old[i];
    }
    let _ = n::LEN;
    // filters
    for s in 0..pm::FILT_SLOTS {
        let mut k = po::filter_type::decode(old[po::filt(s, po::f::TYPE)]);
        if (k == po::filter_type::V2_MOOG_LP || k == po::filter_type::V2_MOOG_HP) && !era.four_pole {
            k = po::filter_type::OFF;
        }
        let (t, form, ovs, lw) = filter_rule(k);
        p[pm::filt(s, f::TYPE)] = pm::filter_type::encode(t);
        p[pm::filt(s, f::CUTOFF)] = old[po::filt(s, po::f::CUTOFF)];
        p[pm::filt(s, f::RESONANCE)] = old[po::filt(s, po::f::RESONANCE)];
        p[pm::filt(s, f::FORM)] = form as f32;
        p[pm::filt(s, f::OVERSAMPLE)] = ovs as f32;
        p[pm::filt(s, f::LAW)] = lw as f32;
    }
    // envelopes
    let mut frame_env = false;
    for s in 0..pm::ENV_SLOTS {
        let lw = po::env_mode::decode(old[po::env(s, po::e::MODE)]);
        frame_env |= lw == po::env_mode::V2;
        p[pm::env(s, e::LAW)] = pm::env_law::encode(lw);
        for (nf, of) in [(e::ATTACK, po::e::ATTACK), (e::DECAY, po::e::DECAY), (e::SUSTAIN, po::e::SUSTAIN), (e::RELEASE, po::e::RELEASE)] {
            p[pm::env(s, nf)] = old[po::env(s, of)];
        }
        let slope = old[po::env(s, po::e::SLOPE)];
        p[pm::env(s, if lw == po::env_mode::V2 { e::SUSTAIN_SLOPE } else { e::CURVE_SLOPE })] = slope;
        p[pm::env(s, e::AMPLIFY)] = old[po::env_amp(s)];
    }
    // LFOs
    for s in 0..pm::LFO_SLOTS {
        let frame = law::choice(old[po::lfo_ext(s, po::lx::MODE)], 2) == 1;
        let (shape, sync) = if frame {
            let st = raw(old[po::lfo(s, po::l::SHAPE)]).clamp(0, 7) as usize;
            let shape = crate::lfo::FRAME_SHAPES.get(st).copied().unwrap_or(crate::lfo::shape::RAMP_UP);
            (shape, if raw(old[po::lfo(s, po::l::SYNC)]) != 0 { pm::lfo_sync::RESTART } else { pm::lfo_sync::CONTINUE })
        } else {
            let shape = law::choice(old[po::lfo(s, po::l::SHAPE)], crate::lfo::shape::COUNT_V1);
            (shape, if law::choice(old[po::lfo(s, po::l::SYNC)], 2) == 1 { pm::lfo_sync::RESTART } else { pm::lfo_sync::SONG_TIME })
        };
        p[pm::lfo(s, l::SHAPE)] = cv(shape, crate::lfo::shape::ALL);
        p[pm::lfo(s, l::SYNC)] = cv(sync, pm::lfo_sync::COUNT);
        p[pm::lfo(s, l::LAW)] = if frame { 1.0 } else { 0.0 };
        p[pm::lfo(s, l::RATE)] = old[po::lfo(s, po::l::RATE)];
        p[pm::lfo(s, l::DEPTH)] = old[po::lfo(s, po::l::DEPTH)];
        p[pm::lfo(s, l::PHASE)] = old[po::lfo_ext(s, po::lx::PHASE)];
        p[pm::lfo(s, l::POLARITY)] = cv(raw(old[po::lfo_ext(s, po::lx::POLARITY)]).clamp(0, 2) as usize, 3);
        p[pm::lfo(s, l::ONE_SHOT)] = if raw(old[po::lfo_ext(s, po::lx::EG)]) != 0 { 1.0 } else { 0.0 };
    }
    // mod matrix
    for s in 0..pm::MOD_SLOTS {
        let src = po::kinds::decode(old[po::modslot(s, po::m::SOURCE)], crate::modmx::source::COUNT_V1, crate::modmx::source::COUNT - crate::modmx::source::COUNT_V1, 0);
        let tgt = old[po::modslot(s, po::m::TARGET)];
        let tid = law::index(tgt);
        if src != 0 {
            for k in 0..pm::OSC_SLOTS {
                let naive = po::osc_type::decode(old[po::osc(k, po::o::TYPE)]) == po::osc_type::V2;
                let mode_or_ring = [po::shape::V2_MODE, po::shape::V2_RING]
                    .iter()
                    .any(|&sh| tid == crate::modmx::target::osc(k, crate::modmx::target::OSC_SHAPE + sh as u8));
                if naive && mode_or_ring {
                    return Err(MigrateError(format!("mod{} targets osc{}'s naive mode or ring, which v3 cannot modulate", s + 1, k + 1)));
                }
            }
        }
        // before format version 5 a strip route on a voice source did nothing: the slot stays empty
        if !era.chan_voice_sources && is_strip_target(tid) && (1..=6).contains(&src) {
            continue;
        }
        p[pm::modslot(s, m::SOURCE)] = cv(src, crate::modmx::source::COUNT);
        p[pm::modslot(s, m::TARGET)] = tgt;
        p[pm::modslot(s, m::AMOUNT)] = old[po::modslot(s, po::m::AMOUNT)];
        let op = po::kinds::decode(old[po::modslot(s, po::m::OPERATION)], crate::modmx::op::COUNT_V1, crate::modmx::op::COUNT - crate::modmx::op::COUNT_V1, 0);
        p[pm::modslot(s, m::OPERATION)] = cv(op, crate::modmx::op::COUNT);
    }
    // controllers, drive, strip, speech: by name
    p[pm::ctl::WHEEL] = old[po::ctl::WHEEL];
    p[pm::ctl::BEND] = old[po::ctl::BEND];
    p[pm::ctl::BEND_RANGE] = old[po::ctl::BEND_RANGE];
    p[pm::DRIVE..pm::DRIVE + dist::LEN].copy_from_slice(&old[po::VDIST..po::VDIST + dist::LEN]);
    for c in 2..=7 {
        p[pm::cc(c)] = old[po::cc(c)];
    }
    let strip = [
        (ch::ON, po::ch::ON),
        (ch::VOL, po::ch::VOL),
        (ch::DIST, po::ch::DIST),
        (ch::DIST + 1, po::ch::DIST + 1),
        (ch::DIST + 2, po::ch::DIST + 2),
        (ch::DIST + 3, po::ch::DIST + 3),
        (ch::SEND_RVB, po::ch::SEND_RVB),
        (ch::SEND_DLY, po::ch::SEND_DLY),
        (ch::BOOST, po::ch::BOOST),
        (ch::FXROUTE, po::ch::FXROUTE),
        (ch::SEND_AUXA, po::ch::SEND_AUXA),
        (ch::SEND_AUXB, po::ch::SEND_AUXB),
        (ch::RECV_AUXA, po::ch::RECV_AUXA),
        (ch::RECV_AUXB, po::ch::RECV_AUXB),
    ];
    for (ni, oi) in strip {
        p[ni] = old[oi];
    }
    p[ch::CHORUS..ch::CHORUS + ch::CHORUS_LEN].copy_from_slice(&old[po::ch::CHORUS..po::ch::CHORUS + ch::CHORUS_LEN]);
    p[ch::COMP..ch::COMP + ch::COMP_LEN].copy_from_slice(&old[po::ch::COMP..po::ch::COMP + ch::COMP_LEN]);
    let strip_on = law::choice(old[po::ch::ON], 2) == 1;
    if strip_on && era.dc_filters {
        p[ch::DC_FILTER] = 1.0;
    }
    p[pm::speech::ON] = old[po::ch::SPEECH];
    p[pm::speech::SELECT] = old[po::ch::SPEECH_CTL4];
    p[pm::speech::PITCH] = old[po::ch::SPEECH_CTL5];
    // quirks: the format version's, each only where its code is reached
    let overdrive = raw(old[po::VDIST + dist::MODE]) == 1 || (strip_on && raw(old[po::ch::DIST + dist::MODE]) == 1);
    let on = [
        (pm::quirk::EXACT_MATH, era.exact_math && (sine || fm || overdrive)),
        (pm::quirk::INTEGER_FM, era.integer_fm && fm),
        (pm::quirk::FIXED_SEED, era.fixed_seed && noise),
        (pm::quirk::LCG_NOISE, era.lcg_noise && noise),
        (pm::quirk::COARSE_ENV, era.coarse_env && frame_env),
    ];
    for (q, set) in on {
        if set {
            p[q] = 1.0;
        }
    }
    Ok(p)
}

/// The v3 values of a whole old patch given as sparse pairs.
pub fn values_of_pairs(pairs: &[(usize, f32)]) -> Result<[f32; pm::COUNT], MigrateError> {
    let mut old = po::DEFAULTS;
    for &(i, v) in pairs {
        if i >= po::COUNT {
            return Err(MigrateError(format!("old index {i} out of range")));
        }
        old[i] = v;
    }
    values(&old)
}

/// An old instrument chunk (versions 1 and 2), decoded on the host: version, the full old values, the bytes after the
/// pairs (samples, speech text).
pub fn decode_old_chunk(b: &[u8]) -> Result<(u8, [f32; po::COUNT], &[u8]), MigrateError> {
    let bad = || MigrateError("malformed old instrument chunk".to_string());
    let ver = *b.first().ok_or_else(bad)?;
    if ver != 1 && ver != 2 {
        return Err(MigrateError(format!("instrument chunk version {ver} is not an old one")));
    }
    let mut pos = 1usize;
    let mut byte = || -> Result<u8, MigrateError> {
        let x = *b.get(pos).ok_or_else(bad)?;
        pos += 1;
        Ok(x)
    };
    let mut n = byte()? as usize;
    if n == 0xFF {
        n = byte()? as usize | (byte()? as usize) << 8;
    }
    let mut old = po::DEFAULTS;
    for _ in 0..n {
        let mut i = byte()? as usize;
        if i >= 0xF0 {
            i = 240 + 256 * (i - 0xF0) + byte()? as usize;
        }
        let v = f32::from_le_bytes([byte()?, byte()?, byte()?, byte()?]);
        *old.get_mut(i).ok_or_else(bad)? = v;
    }
    Ok((ver, old, &b[pos..]))
}

/// Encodes an old-layout patch as an old chunk (version 1, or 2 with a speech text): what the converters build, before
/// the migration of the song they write.
pub fn encode_old(values: &[f32; po::COUNT], samples: &[&[u8]], text: &[u8]) -> Vec<u8> {
    let mut c = std::vec![if text.is_empty() { 1u8 } else { 2 }];
    let pairs: Vec<(usize, f32)> = (0..po::COUNT).filter(|&i| values[i].to_bits() != po::DEFAULTS[i].to_bits()).map(|i| (i, values[i])).collect();
    crate::chunk::push_count(&mut c, pairs.len());
    for (i, v) in pairs {
        crate::chunk::push_index(&mut c, i);
        c.extend(v.to_le_bytes());
    }
    c.push(samples.len() as u8);
    for s in samples {
        c.extend((s.len() as u32).to_le_bytes());
        c.extend(*s);
    }
    if !text.is_empty() {
        c.extend((text.len() as u16).to_le_bytes());
        c.extend(text);
    }
    c
}

/// Whether an instrument chunk is in the old layout (versions 1 and 2).
pub fn is_old_instrument(chunk: &[u8]) -> bool {
    matches!(chunk.first(), Some(1 | 2))
}

/// Whether an effect chunk is in the old layout (no v3 tag).
pub fn is_old_effect(chunk: &[u8]) -> bool {
    chunk.len() < 4 || chunk[chunk.len() - 4..] != FX_TAG
}

/// An old instrument chunk → the v3 chunk: version 1 → 3, 2 → 4, the pairs migrated, samples and speech text kept.
pub fn instrument_chunk(b: &[u8]) -> Result<Vec<u8>, MigrateError> {
    let (ver, old, rest) = decode_old_chunk(b)?;
    let p = values(&old)?;
    let mut out = crate::chunk::encode_pairs(ver + 2, &p);
    out.extend_from_slice(rest);
    Ok(out)
}

/// Whether v3 index `j` is a quirk: fixed per patch, so a lane that would move one sets it on the patch instead
/// ([`lane_quirks`]).
fn is_quirk(j: usize) -> bool {
    (pm::QUIRK..pm::QUIRK + pm::quirk::LEN).contains(&j)
}

/// The quirks a lane's points reach that the base patch lacks (an envelope-law lane to the frame law on a format 0–1
/// song reaches `coarse_env`, …): the patch sets them, as the old player applied them wherever their code ran.
pub fn lane_quirks(base: &[f32; po::COUNT], index: usize, points: impl Iterator<Item = f32>) -> Vec<usize> {
    let Ok(base_new) = values(base) else { return Vec::new() };
    let mut on = Vec::new();
    for x in points {
        let mut old = *base;
        old[index] = x;
        if let Ok(v) = values(&old) {
            for q in pm::QUIRK..pm::QUIRK + pm::quirk::LEN {
                if v[q] != base_new[q] && v[q] >= 0.5 && !on.contains(&q) {
                    on.push(q);
                }
            }
        }
    }
    on
}

/// The v3 lanes an old lane becomes: for each point the whole patch (`base`, old) with the lane's value is migrated,
/// and every v3 field whose value moves gets a lane at the old lane's times. Bytes are `round(255·v)`, which a
/// choice decodes back to its option. A quirk the lane reaches is not a lane ([`lane_quirks`] sets it on the patch).
/// Returns `(v3 index, points)`.
pub fn lane(base: &[f32; po::COUNT], index: usize, points: &[(i32, u8)]) -> Result<Vec<(usize, Vec<(i32, u8)>)>, MigrateError> {
    if index >= po::COUNT {
        return Err(MigrateError(format!("lane on old index {index} out of range")));
    }
    let name = po::name(index);
    if index == po::v2::VOICE || index == po::v2::ERA {
        return Err(MigrateError(format!("a lane on {name} has no v3 form (it is structural)")));
    }
    let mut per_point = Vec::with_capacity(points.len());
    for &(_, b) in points {
        let mut old = *base;
        old[index] = b as f32 / 255.0;
        per_point.push(values(&old).map_err(|e| MigrateError(format!("lane on {name}: {e}")))?);
    }
    let base_new = values(base)?;
    let mut out = Vec::new();
    for j in 0..pm::COUNT {
        if per_point.iter().all(|v| v[j] == base_new[j]) || is_quirk(j) {
            continue;
        }
        if pm::is_structural(j) {
            return Err(MigrateError(format!("a lane on {name} moves {}, which is structural in v3", pm::name(j))));
        }
        let pts = points.iter().zip(&per_point).map(|(&(t, _), v)| (t, (v[j].clamp(0.0, 1.0) * 255.0 + 0.5) as u8)).collect();
        out.push((j, pts));
    }
    // a lane that moves nothing (a value its v3 form ignores) still keeps its index when the name survives
    if out.is_empty() {
        let new = pm::rename::OLD.get(index).map(|e| e.1).unwrap_or("");
        if let Some(j) = (0..pm::COUNT).find(|&j| pm::name(j) == new) {
            out.push((j, points.to_vec()));
        }
    }
    Ok(out)
}

use crate::fx::TAG as FX_TAG;

/// The mode of an old effect chunk's mode value (the expanded choice of chunk v1/v2).
pub fn old_fx_mode(k: u8, p: f32) -> usize {
    let (old, later) = (crate::fx::mode_count(k), crate::fx::migrated_total(k) - crate::fx::mode_count(k));
    if later == 0 || old == 0 {
        law::choice(p, old.max(1))
    } else if old == 1 {
        law::choice(p, 1 + later)
    } else {
        kinds(p, old, later, 0)
    }
}

/// The old layout's stored value of effect mode `mode` of kind `k` (what the converters write before the migration).
pub fn old_fx_mode_value(k: u8, mode: usize) -> f32 {
    let (old, later) = (crate::fx::mode_count(k), crate::fx::migrated_total(k) - crate::fx::mode_count(k));
    if later == 0 {
        if old > 1 { mode as f32 / (old - 1) as f32 } else { 0.0 }
    } else if old == 1 {
        mode as f32 / later as f32
    } else {
        po::kinds::encode(mode, old)
    }
}

/// An old effect chunk: the values (`[mode, p0, …]`, the mode as [`old_fx_mode_value`]) and WaveSabre's size trailer.
pub fn encode_old_fx(values: &[f32]) -> Vec<u8> {
    let mut v: Vec<u8> = values.iter().flat_map(|x| x.to_le_bytes()).collect();
    v.extend((v.len() as i32 + 4).to_le_bytes());
    v
}

/// The 7-bit modes' parameter counts in v3 and where their old `era` parameter was (`fx/v2.rs`).
fn era_param(k: u8, mode: usize) -> Option<usize> {
    use crate::kind;
    match (k, mode) {
        (kind::EQ, 2) => Some(2),
        (kind::COMPRESSOR, 1) => Some(9),
        (kind::DELAY, 2) => Some(7),
        (kind::REVERB, 3) => Some(4),
        _ => None,
    }
}

/// An old effect chunk (`[mode, p0, …]` + a 4-byte trailer) → v3: the mode as a plain choice of the kind's modes, the
/// 7-bit modes' `era` turned into what it decided (cut EQ: the DC filter; comb reverb below 4: low cut 0; delay and
/// compressor: dropped), and the version tag as the trailer.
pub fn effect_chunk(k: u8, b: &[u8]) -> Result<Vec<u8>, MigrateError> {
    if b.len() >= 4 && b[b.len() - 4..] == FX_TAG {
        return Err(MigrateError("effect chunk is already v3".to_string()));
    }
    let n = b.len().saturating_sub(4) / 4;
    let mut v: Vec<f32> = (0..n).map(|i| f32::from_le_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]])).collect();
    if v.is_empty() {
        v.push(0.0);
    }
    let mode = old_fx_mode(k, v[0]);
    v[0] = crate::fx::mode_value(k, mode);
    if let Some(e) = era_param(k, mode) {
        let version = v.get(1 + e).map_or(6, |&p| law::count(p, 0, 6));
        v.truncate(1 + e);
        match (k, e) {
            (crate::kind::EQ, _) => {
                while v.len() < 1 + e {
                    v.push(if v.len() == 1 { 0.0 } else { 127.0 / 128.0 });
                }
                v.push(if version >= 6 { 1.0 } else { 0.0 });
            }
            (crate::kind::REVERB, _) if version < 4 => {
                while v.len() < 1 + e {
                    v.push(0.0);
                }
                v[1 + 2] = 0.0; // low cut
            }
            _ => {}
        }
    }
    let mut out: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
    out.extend_from_slice(&FX_TAG);
    Ok(out)
}

/// An effect lane's device parameter index in v3 (the chunk value index: 0 is the mode): the same, except the 7-bit
/// modes' old `era`, which no lane may move.
pub fn effect_param(k: u8, mode_value_old: f32, index: usize) -> Result<usize, MigrateError> {
    let mode = old_fx_mode(k, mode_value_old);
    if era_param(k, mode).map(|e| e + 1) == Some(index) {
        return Err(MigrateError(format!("a lane on effect kind {k} mode {mode}'s format version has no v3 form")));
    }
    Ok(index)
}

#[cfg(test)]
mod tests;
