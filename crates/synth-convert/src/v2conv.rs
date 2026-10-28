//! `.v2m` → unified song (change add-v2-import, design D7). One track per (MIDI channel, program) that plays notes,
//! each with one kind 64 instrument converted from its V2 patch ([`patch`]). Event times come from V2's own sequencer
//! ([`crate::v2m::V2m::timeline`]); controllers 1–7 become instrument parameter automation; pitch bend is dropped
//! (V2's synth ignores it). The master track sums the instruments.

use crate::model::{Auto, Dev, SongM, Trk};
use crate::v2m::{Msg, Patch, V2m, PATCH_PARAMS};
use crate::{ConvError, Converted};
use std::collections::BTreeMap;
use unisynth::law::host as inv;
use unisynth::modmx::{op, source, target};
use unisynth::params::{self as pm, ch, dist, e, env_mode, f, filter_type as ft, l, lx, o, osc_type, shape, v, v2, COUNT};

/// The song rate of converted songs.
pub const RATE: u32 = 44_100;

/// V2's raw value as a unified V2 field (`raw / 128`).
fn r(x: u8) -> f32 {
    x as f32 / 128.0
}

/// V2 modulation destinations (`sounddef.h` `isdest`): the index in `target::PARAMS` of V2 parameter `d`, if any.
pub fn dest_target(d: u8) -> Option<u8> {
    // the isdest flags of v2parms in table order
    const ISDEST: [bool; PATCH_PARAMS] = {
        let mut a = [true; PATCH_PARAMS];
        let off = [2, 3, 8, 9, 14, 15, 20, 23, 26, 28, 44, 45, 46, 49, 51, 52, 53, 56, 58, 66, 68, 79, 80, 81, 88];
        let mut i = 0;
        while i < off.len() {
            a[off[i]] = false;
            i += 1;
        }
        a
    };
    if d as usize >= PATCH_PARAMS || !ISDEST[d as usize] {
        return None;
    }
    let pos = ISDEST[..d as usize].iter().filter(|&&x| x).count();
    Some(target::PARAM + pos as u8)
}

/// V2 modulation source → unified source.
fn source_of(s: u8) -> usize {
    match s {
        0 => source::VELOCITY,
        1 => source::WHEEL,
        2..=7 => source::CC2 + (s as usize - 2),
        8 => source::ENV1,
        9 => source::ENV1 + 1,
        10 => source::LFO1,
        11 => source::LFO1 + 1,
        _ => source::V2_NOTE,
    }
}

/// Which V2 engine build to model when the format version cannot tell (v2redux's `Era` profiles).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Profile {
    /// The detected format version.
    #[default]
    Auto,
    /// A version 5 file written by the 2004 engine (v2redux `eras::kkrieger2004`: version 6 sine, FM, arctangent).
    V5_2004,
}

impl Profile {
    pub fn parse(s: &str) -> Option<Profile> {
        match s {
            "auto" => Some(Profile::Auto),
            "kkrieger2004" | "v5-2004" => Some(Profile::V5_2004),
            _ => None,
        }
    }
}

/// Conversion options.
#[derive(Clone, Copy, Debug)]
pub struct Opts {
    /// Seconds after the last event.
    pub tail: f64,
    /// Channels to keep (bit 0 = MIDI channel 1).
    pub mask: u16,
    pub profile: Profile,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { tail: 4.0, mask: 0xffff, profile: Profile::Auto }
    }
}

/// The stored `v2::ERA` value for a format version and a profile.
fn era_value(version: u8, profile: Profile) -> f32 {
    match profile {
        Profile::V5_2004 if version == 5 => pm::kinds::encode(v2::ERA_V5_2004, 7),
        _ => inv::inv_count(version as u32, 0, 6),
    }
}

/// The unified patch of a V2 patch from format version `version`. Warnings name what could not be kept.
pub fn patch(p: &Patch, version: u8, warnings: &mut Vec<String>) -> [f32; COUNT] {
    patch_with(p, version, Profile::Auto, warnings)
}

/// [`patch`] for an engine profile.
pub fn patch_with(p: &Patch, version: u8, profile: Profile, warnings: &mut Vec<String>) -> [f32; COUNT] {
    let q = &p.params;
    let mut u = pm::DEFAULTS;
    // voice: the V2 voice stage, V2's envelope 1 as the amplitude, no velocity law (V2 routes velocity itself)
    u[v2::VOICE] = 1.0;
    u[v2::ERA] = era_value(version, profile);
    u[v::VELOCITY] = 0.0;
    u[v::ENV1_AT_OUTPUT] = 1.0;
    u[v::CAPACITY] = inv::inv_count(q[88].clamp(1, 64) as u32, 1, 64);
    u[v::STEAL] = 1.0;
    u[v::PAN] = r(q[0]);
    u[v::TRANSPOSE] = r(q[1]);
    u[v2::KEYSYNC] = r(q[58]);
    for i in 0..3 {
        let b = 2 + 6 * i;
        let os = |fld: usize| pm::osc(i, fld);
        u[os(o::TYPE)] = osc_type::encode(if q[b] & 7 == 0 { osc_type::OFF } else { osc_type::V2 });
        u[os(o::SHAPE + shape::V2_MODE)] = r(q[b]);
        u[os(o::SHAPE + shape::V2_RING)] = r(q[b + 1]);
        u[os(o::COARSE)] = r(q[b + 2]);
        u[os(o::FINE)] = r(q[b + 3]);
        u[os(o::SHAPE + shape::V2_COLOR)] = r(q[b + 4]);
        u[os(o::GAIN)] = r(q[b + 5]);
        u[os(o::MIX)] = 1.0;
        u[os(o::PAN)] = 0.5;
    }
    for i in 0..2 {
        let b = 20 + 3 * i;
        let mode = q[b] & 7;
        // V2 "single" routing runs filter 1 only
        let off = mode == 0 || (i == 1 && q[26] == 0);
        u[pm::filt(i, f::TYPE)] = ft::encode(if off { ft::OFF } else { ft::V2_LP + mode as usize - 1 });
        u[pm::filt(i, f::CUTOFF)] = r(q[b + 1]);
        u[pm::filt(i, f::RESONANCE)] = r(q[b + 2]);
    }
    u[v2::ROUTING] = r(q[26]);
    u[v2::BALANCE] = r(q[27]);
    for k in 0..4 {
        u[pm::VDIST + k] = r(q[28 + k]);
    }
    let _ = dist::LEN;
    for i in 0..2 {
        let b = 32 + 6 * i;
        u[pm::env(i, e::MODE)] = env_mode::encode(env_mode::V2);
        u[pm::env(i, e::ATTACK)] = r(q[b]);
        u[pm::env(i, e::DECAY)] = r(q[b + 1]);
        u[pm::env(i, e::SUSTAIN)] = r(q[b + 2]);
        u[pm::env(i, e::SLOPE)] = r(q[b + 3]);
        u[pm::env(i, e::RELEASE)] = r(q[b + 4]);
        u[pm::env_amp(i)] = r(q[b + 5]);
    }
    for k in 0..2 {
        let b = 44 + 7 * k;
        u[pm::lfo_ext(k, lx::MODE)] = 1.0;
        u[pm::lfo(k, l::SHAPE)] = r(q[b]);
        u[pm::lfo(k, l::SYNC)] = r(q[b + 1]);
        u[pm::lfo_ext(k, lx::EG)] = r(q[b + 2]);
        u[pm::lfo(k, l::RATE)] = r(q[b + 3]);
        u[pm::lfo_ext(k, lx::PHASE)] = r(q[b + 4]);
        u[pm::lfo_ext(k, lx::POLARITY)] = r(q[b + 5]);
        u[pm::lfo(k, l::DEPTH)] = r(q[b + 6]);
    }
    // channel stage
    u[ch::ON] = 1.0;
    u[ch::VOL] = r(q[59]);
    u[ch::RECV_AUXA] = r(q[60]);
    u[ch::RECV_AUXB] = r(q[61]);
    u[ch::SEND_AUXA] = r(q[62]);
    u[ch::SEND_AUXB] = r(q[63]);
    u[ch::SEND_RVB] = r(q[64]);
    u[ch::SEND_DLY] = r(q[65]);
    u[ch::FXROUTE] = r(q[66]);
    u[ch::BOOST] = r(q[67]);
    for k in 0..4 {
        u[ch::DIST + k] = r(q[68 + k]);
    }
    for k in 0..ch::CHORUS_LEN {
        u[ch::CHORUS + k] = r(q[72 + k]);
    }
    for k in 0..ch::COMP_LEN {
        u[ch::COMP + k] = r(q[79 + k]);
    }
    // modulation: Add slots in V2's order (amount (val − 64)/64 = 2·(val/128) − 1)
    let mut mods: Vec<(usize, &(u8, u8, u8))> = p.mods.iter().enumerate().filter(|(_, m)| m.1 != 64).collect();
    mods.retain(|(_, m)| {
        let ok = dest_target(m.2).is_some();
        if !ok {
            warnings.push(format!("mod {:?}: V2 parameter {} is not a modulation destination; dropped", m, m.2));
        }
        ok
    });
    if mods.len() > pm::MOD_SLOTS {
        let mut by = mods.clone();
        by.sort_by_key(|(_, m)| std::cmp::Reverse((m.1 as i32 - 64).abs()));
        let drop: Vec<_> = by[pm::MOD_SLOTS..].iter().map(|(i, _)| *i).collect();
        warnings.push(format!("{} modulation entries, the matrix has {}: dropped {:?}", mods.len(), pm::MOD_SLOTS, drop.iter().map(|i| p.mods[*i]).collect::<Vec<_>>()));
        mods.retain(|(i, _)| !drop.contains(i));
    }
    for (slot, (_, m)) in mods.iter().enumerate() {
        u[pm::modslot(slot, pm::m::SOURCE)] = source::encode(source_of(m.0));
        u[pm::modslot(slot, pm::m::TARGET)] = inv::inv_index(dest_target(m.2).unwrap());
        u[pm::modslot(slot, pm::m::OPERATION)] = op::encode(op::ADD);
        u[pm::modslot(slot, pm::m::AMOUNT)] = r(m.1);
    }
    u
}

/// Converts a `.v2m` (any format version) into a unified song.
pub fn convert(bytes: &[u8], opts: Opts) -> Result<Converted, ConvError> {
    let s = crate::v2m::parse(bytes)?;
    let mut warnings = vec![];
    let song = song_opts(&s, opts, &mut warnings)?;
    Ok(Converted { blob: song.write(), warnings })
}

/// The program each channel plays at each frame, and its notes and controllers, per (channel, program).
#[derive(Default)]
struct Part {
    /// `(frame, note, Some(velocity) | None for off)`.
    notes: Vec<(u64, u8, Option<u8>)>,
}

/// V2's synth frame at 44.1 kHz: events reach the voices at the start of the next frame (the synth renders whole
/// frames ahead and reads the MIDI it was given at its next frame tick).
pub const FRAME: u64 = 128;

pub fn song(s: &V2m, tail: f64, warnings: &mut Vec<String>) -> Result<SongM, ConvError> {
    song_opts(s, Opts { tail, ..Opts::default() }, warnings)
}

/// [`song`] with options (channel mask: a parity aid; engine profile).
pub fn song_opts(s: &V2m, opts: Opts, warnings: &mut Vec<String>) -> Result<SongM, ConvError> {
    let (tail, mask) = (opts.tail, opts.mask);
    let mut t = s.timeline(RATE);
    // the synth's control frame: 256 samples before format version 2 (v2redux `DELTA_FRAME256`)
    let frame = if s.version < 2 { 2 * FRAME } else { FRAME };
    for e in t.events.iter_mut() {
        e.frame = e.frame.div_ceil(frame) * frame;
    }
    let mut prog = [0u8; 16];
    // notes held per channel: key → program at its note-on
    let mut held: [BTreeMap<u8, u8>; 16] = Default::default();
    let mut parts: BTreeMap<(u8, u8), Part> = BTreeMap::new();
    // controller values per channel over time: (frame, controller 1..7, value)
    let mut ctls: [Vec<(u64, u8, u8)>; 16] = Default::default();
    // controller messages 4 and 5 as V2 sends them to the speech synthesizer (not the program-change reset)
    let mut speech_ctl: [Vec<(u64, u8, u8)>; 16] = Default::default();
    for e in &t.events {
        let c = e.chan as usize;
        match e.msg {
            Msg::Program(p) => {
                let p = p & 0x7f;
                // before format version 4 every program change cuts the channel and sets the volume to 127
                let old = s.version < 4;
                if p != prog[c] || old {
                    // V2 cuts the channel's voices at a real program change: release the held notes there
                    for (key, pr) in std::mem::take(&mut held[c]) {
                        parts.entry((c as u8, pr)).or_default().notes.push((e.frame, key, None));
                    }
                    prog[c] = p;
                }
                // either way V2 resets controllers 1–6
                for k in 1..=6 {
                    ctls[c].push((e.frame, k, 0));
                }
                if old {
                    ctls[c].push((e.frame, 7, 127));
                }
            }
            Msg::Ctl { ctl, val } => {
                ctls[c].push((e.frame, ctl, val));
                if ctl == 4 || ctl == 5 {
                    speech_ctl[c].push((e.frame, ctl, val));
                }
            }
            Msg::Note { key, vel } if vel > 0 => {
                let pr = prog[c];
                if let Some(old) = held[c].insert(key, pr) {
                    parts.entry((c as u8, old)).or_default().notes.push((e.frame, key, None));
                }
                parts.entry((c as u8, pr)).or_default().notes.push((e.frame, key, Some(vel)));
            }
            Msg::Note { key, .. } => {
                if let Some(pr) = held[c].remove(&key) {
                    parts.entry((c as u8, pr)).or_default().notes.push((e.frame, key, None));
                }
            }
            Msg::Bend { .. } => {}
        }
    }
    let mut m = SongM { tempo: 120, rate: RATE as i32, length: t.end_frame as f64 / RATE as f64 + tail, devices: vec![], lanes: vec![], tracks: vec![] };
    // the MIDI channel of each instrument track (V2 mixes the aux buses in channel order)
    let mut tch: Vec<u8> = vec![];
    for (&(c, pr), part) in parts.iter().filter(|(k, p)| mask & (1 << k.0) != 0 && p.notes.iter().any(|n| n.2.is_some())) {
        let Some(patch_v2) = s.patches.get(pr as usize) else {
            warnings.push(format!("channel {} plays program {pr}, which the song does not have; skipped", c + 1));
            continue;
        };
        let mut w = vec![];
        let values = patch_with(patch_v2, s.version, opts.profile, &mut w);
        warnings.extend(w.into_iter().map(|x| format!("channel {} program {pr}: {x}", c + 1)));
        // V2 routes channel 16 through the speech synthesizer (format version 5 on)
        let text: Vec<u8> = if c == 15 && s.version >= 5 { speech_text(s) } else { vec![] };
        let mut values = values;
        if !text.is_empty() {
            values[ch::SPEECH] = 1.0;
        }
        m.devices.push(Dev { kind: unisynth::kind::INSTRUMENT, chunk: unisynth::chunk::encode_text(&values, &[], &text) });
        let dev = m.devices.len() - 1;
        let mut prev = 0u64;
        let lane = part
            .notes
            .iter()
            .map(|&(fr, key, vel)| {
                let d = (fr - prev) as i32;
                prev = fr;
                (d, key & 0x7f, vel.map(|v| v & 0x7f))
            })
            .collect();
        m.lanes.push(lane);
        let mut autos = controller_autos(&ctls[c as usize]);
        if !text.is_empty() {
            autos.extend(speech_autos(&speech_ctl[c as usize]));
        }
        m.tracks.push(Trk { volume: 1.0, receives: vec![], devices: vec![dev], lane: m.lanes.len() - 1, autos });
        tch.push(c);
    }
    let n = m.tracks.len();
    let g = &s.globals;
    let era = era_value(s.version, opts.profile);
    let fx = |kind: u8, mode: usize, vals: &[u8]| {
        let mut v = vec![unisynth::fx::mode_value(kind, mode)];
        v.extend(vals.iter().map(|&x| r(x)));
        v.push(era);
        Dev { kind, chunk: unisynth::fx::encode(&v) }
    };
    let bus = |m: &mut SongM, dev: Option<Dev>, recv: Vec<(usize, usize, f32)>| {
        let devices = dev.map(|d| {
            m.devices.push(d);
            vec![m.devices.len() - 1]
        });
        m.lanes.push(vec![]);
        m.tracks.push(Trk { volume: 1.0, receives: recv, devices: devices.unwrap_or_default(), lane: m.lanes.len() - 1, autos: vec![] });
        m.tracks.len() - 1
    };
    use synth_core::device::recv;
    // a send or receive level is in use when it is above 0 or an Add route moves it (a controller often opens it)
    let used = |v: &[f32; COUNT], i: usize| v[i] > 0.0 || (0..pm::MOD_SLOTS).any(|k| unisynth::modmx::adds_to(v, k, i));
    // aux A/B. V2 clears both buses every frame and renders the channels in order, each one's voices (aux
    // oscillators) and receive before its sends: a reader hears exactly the sends of the lower channels in the same
    // frame. So each reader receives straight from those senders' aux output pair (no bus track, no cycle).
    let vals: Vec<[f32; COUNT]> = (0..n).map(|t| unisynth::chunk::Chunk::parse(&m.devices[m.tracks[t].devices[0]].chunk).unwrap().values()).collect();
    for (k, (send, rcv)) in [(ch::SEND_AUXA, ch::RECV_AUXA), (ch::SEND_AUXB, ch::RECV_AUXB)].into_iter().enumerate() {
        let aux_mode = unisynth::osc::v2mode::AUX_A + k as u8;
        for t in 0..n {
            let v = &vals[t];
            let osc = (0..3).any(|i| unisynth::osc::v2_aux(v, i) && (pm::v2_raw(v[pm::osc(i, o::SHAPE + shape::V2_MODE)]) as i32 & 7) as u8 == aux_mode);
            if !used(v, rcv) && !osc {
                continue;
            }
            let from: Vec<usize> = (0..n).filter(|&u| tch[u] < tch[t] && used(&vals[u], send)).collect();
            m.tracks[t].receives.extend(from.into_iter().map(|u| (u, recv::encode(3 + k, 4 + 2 * k), 1.0)));
        }
    }
    let rvb = bus(&mut m, Some(fx(unisynth::kind::REVERB, 3, &g[0..4])), (0..n).map(|t| (t, recv::encode(1, 0), 1.0)).collect());
    let dly = bus(&mut m, Some(fx(unisynth::kind::DELAY, 2, &g[4..11])), (0..n).map(|t| (t, recv::encode(2, 0), 1.0)).collect());
    // master: the instruments, the reverb and the delay; then V2's DC filter + post filters and the sum compressor
    let mut recv_all: Vec<(usize, usize, f32)> = (0..n).map(|i| (i, 0, 1.0)).collect();
    recv_all.extend([(rvb, 0, 1.0), (dly, 0, 1.0)]);
    m.devices.push(fx(unisynth::kind::EQ, 2, &g[11..13]));
    m.devices.push(fx(unisynth::kind::COMPRESSOR, 1, &g[13..22]));
    let d = m.devices.len();
    m.lanes.push(vec![]);
    m.tracks.push(Trk { volume: 1.0, receives: recv_all, devices: vec![d - 2, d - 1], lane: m.lanes.len() - 1, autos: vec![] });
    Ok(m)
}

/// The speech text of a channel-16 instrument: V2's speech strings, joined (index `i` starts at the i-th string).
fn speech_text(s: &V2m) -> Vec<u8> {
    let mut t = vec![];
    for (i, x) in s.speech.iter().enumerate() {
        if i > 0 {
            t.push(0);
        }
        t.extend(x);
    }
    if t.iter().all(|&b| b == 0 || b == b' ') { vec![] } else { t }
}

/// Step automation of the speech controllers (`ch::SPEECH_CTL4/5`): every message as a step, so a repeated text
/// select still restarts the text (a value that does not change is moved by one step and back).
fn speech_autos(msgs: &[(u64, u8, u8)]) -> Vec<Auto> {
    let byte = |x: u8| ((x as f32 / 128.0) * 255.0).round() as u8;
    let mut out = vec![];
    for (k, param) in [(4u8, ch::SPEECH_CTL4), (5, ch::SPEECH_CTL5)] {
        let mut cur = 0u8;
        let mut pts: Vec<(u64, u8)> = vec![(0, 0)];
        for &(fr, _, val) in msgs.iter().filter(|m| m.1 == k) {
            if val == cur {
                // the same value again: a two-sample dip so the instrument sees a change (V2 applies every message)
                let other = if val == 0 { 1 } else { val - 1 };
                pts.extend([(fr.saturating_sub(2), byte(cur)), (fr.saturating_sub(2), byte(other)), (fr, byte(other))]);
            } else {
                pts.push((fr, byte(cur)));
            }
            pts.push((fr, byte(val)));
            cur = val;
        }
        if pts.len() == 1 {
            continue;
        }
        let mut prev = 0u64;
        let points = pts
            .into_iter()
            .map(|(fr, b)| {
                let d = (fr - prev) as i32;
                prev = fr;
                (d, b)
            })
            .collect();
        out.push(Auto { device: 0, param, points });
    }
    out
}

/// Step automation of the instrument's controller parameters from the channel's controller events.
fn controller_autos(ctls: &[(u64, u8, u8)]) -> Vec<Auto> {
    let mut out = vec![];
    for k in 1..=7u8 {
        let param = if k == 1 { pm::ctl::WHEEL } else { pm::cc(k as usize) };
        // V2 starts at 0, volume at 127
        let mut cur = if k == 7 { 127u8 } else { 0 };
        let mut pts: Vec<(u64, u8)> = vec![];
        for &(fr, c, val) in ctls.iter().filter(|x| x.1 == k) {
            if val == cur {
                continue;
            }
            let byte = |x: u8| ((x as f32 / 128.0) * 255.0).round() as u8;
            if pts.is_empty() {
                pts.push((0, byte(cur)));
            }
            pts.push((fr, byte(cur)));
            pts.push((fr, byte(val)));
            cur = val;
            let _ = c;
        }
        if pts.is_empty() {
            continue;
        }
        let mut prev = 0u64;
        let points = pts
            .into_iter()
            .map(|(fr, b)| {
                let d = (fr - prev) as i32;
                prev = fr;
                (d, b)
            })
            .collect();
        out.push(Auto { device: 0, param, points });
    }
    out
}
