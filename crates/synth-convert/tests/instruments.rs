//! WaveSabre instruments (task 6.2): mapping scenarios of the converter spec, capacity, automation, hostile chunks.

mod common;
#[path = "../../wavesabre/tests/songs.rs"]
#[allow(dead_code)]
mod songs;

use common::*;
use synth_convert::auto::{read_lane, value};
use synth_convert::model::{Auto, SongM};
use synth_convert::{convert, ConvError};
use unisynth::chunk::Chunk;
use unisynth::law;
use unisynth::modmx::{source, target};
use unisynth::params::{self as pm, filt, filter_type, modslot, v, COUNT};
use wavesabre::kind as w;

fn chunk_of(legacy_params: &[f32]) -> Vec<u8> {
    dev(0, legacy_params).chunk
}

fn slaughter(f: impl Fn(&mut [f32])) -> Vec<u8> {
    let mut p = vec![0.0f32; 42];
    p[2] = 1.0;
    p[17] = 1.0;
    f(&mut p);
    chunk_of(&p)
}

fn one_instrument(kind: u8, chunk: Vec<u8>, autos: Vec<Auto>, lane: Vec<(i32, u8, Option<u8>)>) -> SongM {
    let mut t = track(&[0], 0);
    t.autos = autos;
    song(vec![synth_convert::model::Dev { kind, chunk }], vec![lane], vec![t, track(&[], 0)])
}

/// The unified patch of device `d` of a converted song.
fn patch(s: &SongM, d: usize) -> [f32; COUNT] {
    assert_eq!(s.devices[d].kind, unisynth::kind::INSTRUMENT);
    Chunk::parse(&s.devices[d].chunk).expect("a valid unified chunk").values()
}

fn conv(s: &SongM) -> (SongM, Vec<String>) {
    let c = convert(&s.write().unwrap()).expect("converts");
    (SongM::read(&c.blob).unwrap(), c.warnings)
}

/// Absolute-frame points of a lane.
fn abs(points: &[(i32, u8)]) -> Vec<(i32, u8)> {
    let mut t = 0i32;
    points
        .iter()
        .map(|&(d, b)| {
            t = t.wrapping_add(d);
            (t, b)
        })
        .collect()
}

fn lane_of(s: &SongM, track: usize, param: usize) -> Vec<(i32, u8)> {
    let a = s.tracks[track].autos.iter().find(|a| a.param == param).unwrap_or_else(|| panic!("no lane for {param}"));
    abs(&a.points)
}

const MELODY: [(i32, u8, Option<u8>); 4] = [(0, 60, Some(100)), (20000, 60, None), (0, 64, Some(100)), (20000, 64, None)];

#[test]
fn a_wavesabre_song_converts_to_unified_kinds_with_the_same_structure() {
    let sl = slaughter(|p| p[15] = 0.2);
    let devices = vec![
        (w::SLAUGHTER, sl),
        (w::FALCON, chunk_of(&[0.0; 32])),
        (w::SPECIMEN, songs::gsm_chunk(&[0.0; 26])),
        (w::ECHO, chunk_of(&[0.4, 0.2, 0.5, 0.5, 0.2, 0.3, 0.6, 0.5])),
        (w::CATHEDRAL, chunk_of(&[0.0, 0.7, 0.4, 0.8, 0.5, 0.3, 0.5, 0.8, 0.5])),
    ]
    .into_iter()
    .map(|(kind, chunk)| synth_convert::model::Dev { kind, chunk })
    .collect();
    let mut master = track(&[3, 4], 0);
    master.receives = vec![(0, 0, 0.5), (1, 0, 0.6), (2, 0, 0.7)];
    let mut t2 = track(&[2], 1);
    t2.volume = 0.8;
    let src = song(devices, vec![MELODY.to_vec(), MELODY[..2].to_vec()], vec![track(&[0], 0), track(&[1], 0), t2, master]);
    let (out, warnings) = conv(&src);
    assert!(warnings.is_empty(), "{warnings:?}");
    let kinds: Vec<u8> = out.devices.iter().map(|d| d.kind).collect();
    assert_eq!(kinds, vec![64, 64, 64, 68, 71]);
    assert_eq!((out.tempo, out.rate, out.length), (src.tempo, src.rate, src.length));
    assert_eq!(out.lanes, src.lanes);
    assert_eq!(out.tracks, src.tracks, "no automation: every track is unchanged");
    loads_and_renders(&out.write().unwrap());
}

#[test]
fn a_slaughter_filter_mod_becomes_an_offset_route_from_the_mod_envelope_to_the_cutoff() {
    let (out, _) = conv(&one_instrument(w::SLAUGHTER, slaughter(|p| p[19] = 0.75), vec![], MELODY.to_vec()));
    let p = patch(&out, 0);
    let slot = (0..pm::MOD_SLOTS)
        .map(|i| (p[modslot(i, pm::m::SOURCE)], p[modslot(i, pm::m::TARGET)], p[modslot(i, pm::m::AMOUNT)], p[modslot(i, pm::m::OPERATION)]))
        .find(|s| law::index(s.1) == target::filter_cutoff(0))
        .expect("a route to filter 1 cutoff");
    assert_eq!(law::choice(slot.0, source::COUNT), source::ENV1 + 1, "from envelope 2 (the mod envelope)");
    assert_eq!(law::choice(slot.3, 2), 1, "an Offset slot");
    let hz = law::offset_amount(slot.2, target::offset_range(target::filter_cutoff(0)));
    assert!((hz - 9990.0).abs() < 1.0, "adds up to {hz} Hz");
    // and the mod envelope's times are the legacy ones
    assert_eq!(p[pm::env(1, pm::e::SUSTAIN)], 0.0);
}

#[test]
fn a_neutral_route_that_never_moves_is_left_out() {
    let (out, _) = conv(&one_instrument(w::SLAUGHTER, slaughter(|p| { p[19] = 0.5; p[39] = 0.5 }), vec![], MELODY.to_vec()));
    let p = patch(&out, 0);
    for i in 0..pm::MOD_SLOTS {
        assert_eq!(law::choice(p[modslot(i, pm::m::SOURCE)], source::COUNT), source::NONE, "slot {i}");
    }
    // …but one that is automated stays, even from neutral
    let a = Auto { device: 0, param: 19, points: vec![(0, 128), (1000, 255)] };
    let (out, _) = conv(&one_instrument(w::SLAUGHTER, slaughter(|p| { p[19] = 0.5; p[39] = 0.5 }), vec![a], MELODY.to_vec()));
    let p = patch(&out, 0);
    assert!((0..pm::MOD_SLOTS).any(|i| law::index(p[modslot(i, pm::m::TARGET)]) == target::filter_cutoff(0)));
}

#[test]
fn a_cutoff_automation_stays_within_a_byte_step_of_the_legacy_cutoff() {
    let a = Auto { device: 0, param: 17, points: vec![(0, 0), (44100, 255)] };
    let (out, _) = conv(&one_instrument(w::SLAUGHTER, slaughter(|_| {}), vec![a], MELODY.to_vec()));
    let lane = lane_of(&out, 0, filt(0, pm::f::CUTOFF));
    let got = read_lane(&lane, 442);
    let legacy = read_lane(&[(0, 0), (44100, 255)], 442);
    for (k, (g, l)) in got.iter().zip(&legacy).enumerate() {
        let (hz_u, hz_l) = (law::cutoff(*g), 20.0 + 19980.0 * l * l);
        let step = 19980.0 * (((l + 1.0 / 255.0).min(1.0)).powi(2) - l * l) + 1.0;
        assert!((hz_u - hz_l).abs() <= step, "at {}: {hz_u} vs {hz_l}", k * 100);
    }
}

#[test]
fn a_filter_type_sweep_switches_exactly_where_the_legacy_decode_changes() {
    let a = Auto { device: 0, param: 16, points: vec![(0, 0), (44100, 255)] };
    let (out, _) = conv(&one_instrument(w::SLAUGHTER, slaughter(|_| {}), vec![a], MELODY.to_vec()));
    let lane = lane_of(&out, 0, filt(0, pm::f::TYPE));
    let got = read_lane(&lane, 450);
    let legacy = read_lane(&[(0, 0), (44100, 255)], 450);
    let want = |v: f32| match (v * 3.0) as i32 {
        1 => filter_type::SVF_HP,
        2 => filter_type::SVF_BP,
        3 => filter_type::SVF_NOTCH,
        _ => filter_type::SVF_LP,
    };
    let mut switches = vec![];
    for (k, (g, l)) in got.iter().zip(&legacy).enumerate() {
        assert_eq!(law::choice(*g, filter_type::COUNT), want(*l), "block {k}");
        if k > 0 && law::choice(*g, filter_type::COUNT) != law::choice(got[k - 1], filter_type::COUNT) {
            switches.push(k * 100);
        }
    }
    assert_eq!(switches.len(), 3, "LP → HP → BP → notch, nothing in between: {switches:?}");
    // two points at one frame at each switch
    let at_one_frame = lane.windows(2).filter(|w| w[0].0 == w[1].0).count();
    assert_eq!(at_one_frame, 3);
}

#[test]
fn structural_parameters_are_never_automated() {
    // Adultery's wave index
    let a = Auto { device: 0, param: 0, points: vec![(0, 10), (1000, 200)] };
    let mut p = vec![0.0f32; 28];
    p[0] = 5.0;
    let (out, _) = conv(&one_instrument(w::ADULTERY, chunk_of(&p), vec![a], MELODY.to_vec()));
    assert!(out.tracks[0].autos.is_empty(), "{:?}", out.tracks[0].autos);
}

#[test]
fn an_osc_that_is_silent_but_rises_is_kept_and_a_silent_one_is_not() {
    let a = Auto { device: 0, param: 7, points: vec![(0, 0), (1000, 255)] }; // osc 2 volume
    let (out, _) = conv(&one_instrument(w::SLAUGHTER, slaughter(|_| {}), vec![a], MELODY.to_vec()));
    let p = patch(&out, 0);
    let ty = |s| law::choice(p[pm::osc(s, pm::o::TYPE)], pm::osc_type::COUNT);
    assert_eq!((ty(0), ty(1), ty(2)), (pm::osc_type::BLIT, pm::osc_type::BLIT, pm::osc_type::OFF));
    assert!(out.tracks[0].autos.iter().any(|a| a.param == pm::osc(1, pm::o::GAIN)));
}

#[test]
fn the_voice_capacity_follows_the_notes_and_a_cap_warns() {
    let cap = |p: &[f32; COUNT]| law::count(p[v::CAPACITY], 1, 64);
    // two overlapping notes, no unison: 2 voices (plus nothing for the release: it ends before the next chord)
    let chord = vec![(0, 60, Some(100)), (0, 64, Some(100)), (1000, 60, None), (0, 64, None)];
    let (out, w) = conv(&one_instrument(w::SLAUGHTER, slaughter(|_| {}), vec![], chord.clone()));
    assert_eq!(cap(&patch(&out, 0)), 2);
    assert!(w.is_empty());
    // 3 unison voices per note
    let (out, _) = conv(&one_instrument(w::SLAUGHTER, slaughter(|p| p[29] = 2.0 / 15.0), vec![], chord.clone()));
    assert_eq!(cap(&patch(&out, 0)), 6);
    // automated unison counts at its highest
    let a = Auto { device: 0, param: 29, points: vec![(0, 0), (500, 255)] };
    let (out, _) = conv(&one_instrument(w::SLAUGHTER, slaughter(|_| {}), vec![a], chord));
    assert_eq!(cap(&patch(&out, 0)), 32);
    // a 12-note chord with 15 unison voices needs 180: capped, with a warning
    let big: Vec<_> = (0..12).map(|i| (0, 40 + i as u8, Some(100))).collect();
    let (out, w) = conv(&one_instrument(w::SLAUGHTER, slaughter(|p| p[29] = 14.0 / 15.0), vec![], big));
    assert_eq!(cap(&patch(&out, 0)), 64);
    assert_eq!(w.len(), 1);
    assert!(w[0].contains("180") && w[0].contains("Slaughter"), "{}", w[0]);
}

#[test]
fn legato_is_one_voice_per_unison_voice() {
    let chord = vec![(0, 60, Some(100)), (0, 64, Some(100)), (0, 67, Some(100))];
    let (out, _) = conv(&one_instrument(w::SLAUGHTER, slaughter(|p| { p[40] = 1.0; p[29] = 2.0 / 15.0 }), vec![], chord));
    let p = patch(&out, 0);
    assert_eq!(law::count(p[v::CAPACITY], 1, 64), 3);
    assert_eq!(law::choice(p[v::MODE], 2), 1);
}

#[test]
fn thunder_is_a_gated_one_shot_sampler_with_the_sqrt2_level_compensation() {
    let (out, _) = conv(&one_instrument(w::THUNDER, songs::gsm_chunk(&[]), vec![], MELODY.to_vec()));
    let p = patch(&out, 0);
    assert_eq!(law::choice(p[v::ONE_SHOT], 2), 1);
    assert!((law::gain(p[v::MASTER]) - std::f32::consts::SQRT_2).abs() < 1e-4);
    assert_eq!(law::choice(p[pm::osc(0, pm::o::TYPE)], pm::osc_type::COUNT), pm::osc_type::SAMPLER);
    assert_eq!(law::choice(p[pm::osc(0, pm::o::TRACKING)], 2), 1, "fixed pitch: the note does not change it");
    assert!((law::fixed_hz(p[pm::osc(0, pm::o::FIXED_HZ)]) - 261.62556).abs() < 1e-2);
    let c = Chunk::parse(&out.devices[0].chunk).unwrap();
    assert_eq!(c.sample_count(), 1);
}

#[test]
fn a_falcon_becomes_two_phase_mod_operators_with_the_carrier_envelope_on_envelope_one() {
    let mut f = vec![0.0f32; 32];
    f[3] = 0.6; // op1 feedback
    f[4] = 0.5; // feed-forward
    f[12] = 0.4; // op2 feedback
    f[17] = 0.8;
    let (out, _) = conv(&one_instrument(w::FALCON, chunk_of(&f), vec![], MELODY.to_vec()));
    let p = patch(&out, 0);
    let o = |s, fld| p[pm::osc(s, fld)];
    assert_eq!(law::choice(o(0, pm::o::TYPE), pm::osc_type::COUNT), pm::osc_type::PM);
    assert_eq!(law::choice(o(1, pm::o::TYPE), pm::osc_type::COUNT), pm::osc_type::PM);
    assert_eq!(law::choice(o(2, pm::o::TYPE), pm::osc_type::COUNT), pm::osc_type::OFF);
    assert!((law::gain(o(0, pm::o::GAIN)) - 13.25).abs() < 1e-3);
    assert!((law::gain(o(1, pm::o::GAIN)) - 1.0).abs() < 1e-3);
    assert!((law::feedback(o(0, pm::o::FEEDBACK)) - 0.6 * 0.6 / 2.0).abs() < 1e-4);
    assert!((law::feedback(o(1, pm::o::FEEDBACK)) - 13.25 * 0.4 * 0.4 / 2.0).abs() < 1e-3);
    assert!((law::pm_out(o(0, pm::o::PM_OUT)) - 0.25).abs() < 1e-4, "send = feed-forward²");
    assert_eq!(o(0, pm::o::MIX), 0.0, "operator 1 only modulates");
    assert_eq!(law::choice(p[v::ENV1_AT_OUTPUT], 2), 0);
    assert!((law::gain(p[v::MASTER]) - (0.4f32 * 0.8).powi(2)).abs() < 1e-4);
    loads_and_renders(&out.write().unwrap());
}

#[test]
fn hostile_instrument_chunks_never_panic_and_always_load() {
    let mut x = 0x1234_5678u32;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x
    };
    for kind in [w::FALCON, w::SLAUGHTER, w::ADULTERY] {
        for round in 0..40 {
            let n = (next() % 60) as usize;
            let params: Vec<f32> = (0..n)
                .map(|_| match next() % 8 {
                    0 => f32::NAN,
                    1 => f32::INFINITY,
                    2 => f32::NEG_INFINITY,
                    3 => 1e30,
                    4 => -3.0,
                    5 => 255.0,
                    _ => (next() % 1000) as f32 / 500.0 - 0.2,
                })
                .collect();
            let src = one_instrument(kind, chunk_of(&params), vec![], MELODY.to_vec());
            let out = convert(&src.write().unwrap()).unwrap_or_else(|e| panic!("kind {kind} round {round}: {e}"));
            loads_and_renders(&out.blob);
        }
    }
    // a Specimen/Thunder chunk that is not a GSM sample is an error, not a panic
    for kind in [w::SPECIMEN, w::THUNDER] {
        let junk: Vec<u8> = (0..200).map(|_| next() as u8).collect();
        let src = one_instrument(kind, junk, vec![], MELODY.to_vec());
        assert!(matches!(convert(&src.write().unwrap()), Err(ConvError::Device(k, _)) if k == kind));
    }
}

#[test]
fn legacy_values_decode_like_the_player_reads_automation() {
    assert_eq!(value(255), 1.0);
    assert_eq!(value(0), 0.0);
}

// ---- physical values: the unified patch decodes, through its own laws, to what the legacy parameters mean ----

fn near(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() <= 1e-3 * b.abs().max(1.0), "{what}: {a} vs {b}");
}

/// `(source, op)` and the physical amount of the mod slot aimed at `tgt`, if any.
fn route(p: &[f32; COUNT], tgt: u8) -> Option<(usize, usize, f32)> {
    (0..pm::MOD_SLOTS).find(|&i| law::index(p[modslot(i, pm::m::TARGET)]) == tgt).map(|i| {
        let op = law::choice(p[modslot(i, pm::m::OPERATION)], 2);
        let a = p[modslot(i, pm::m::AMOUNT)];
        let amount = if op == 1 { law::offset_amount(a, target::offset_range(tgt)) } else { law::scale_amount(a) };
        (law::choice(p[modslot(i, pm::m::SOURCE)], source::COUNT), op, amount)
    })
}

#[test]
fn slaughter_parameters_decode_to_the_same_physical_values() {
    let mut l = vec![0.0f32; 42];
    // osc 2: pulse blend .7, legacy width param .3, volume .6, coarse .98 (24 semitones), fine .4
    l[5..10].copy_from_slice(&[0.7, 0.3, 0.6, 0.98, 0.4]);
    l[2] = 0.5;
    l[15] = 0.8; // noise
    l[16] = 0.67; // band-pass
    l[17] = 0.5; // cutoff
    l[18] = 0.3; // resonance
    l[19] = 0.75;
    l[20..24].copy_from_slice(&[0.1, 0.2, 0.3, 0.4]);
    l[24..28].copy_from_slice(&[0.5, 0.6, 0.7, 0.8]);
    l[28] = 0.9; // master
    l[29] = 4.0 / 15.0;
    l[30] = 0.35;
    l[31] = 0.8;
    l[32] = 0.5; // vibrato rate
    l[33] = 0.5; // amount
    l[34] = 0.25; // rise
    l[35..39].copy_from_slice(&[0.15, 0.25, 0.35, 0.45]);
    l[39] = 0.9; // pitch amount
    l[40] = 1.0;
    l[41] = 0.6;
    let (out, _) = conv(&one_instrument(w::SLAUGHTER, chunk_of(&l), vec![], MELODY.to_vec()));
    let p = patch(&out, 0);
    let o = |s, f| p[pm::osc(s, f)];
    near(law::gain(o(1, pm::o::GAIN)), 0.36, "osc 2 level is its volume squared");
    near(law::gain(o(0, pm::o::GAIN)), 0.25, "osc 1 level");
    assert_eq!(law::choice(o(2, pm::o::TYPE), pm::osc_type::COUNT), pm::osc_type::OFF, "a silent osc is off");
    near(law::coarse(o(1, pm::o::COARSE)), 24.0, "coarse semitones");
    near(law::fine(o(1, pm::o::FINE)), 0.4, "fine semitones (0..1 upstream, not centred)");
    near(p[pm::osc(1, pm::o::SHAPE + pm::shape::BLIT_WAVEFORM)], 0.7, "pulse blend");
    near(p[pm::osc(1, pm::o::SHAPE + pm::shape::BLIT_PULSE_WIDTH)], 0.7, "pulse width is 1 − the parameter");
    near(law::gain(p[v::NOISE_LEVEL]), 0.64, "noise level is its parameter squared");
    assert_eq!(p[pm::NOISE + pm::n::WIDTH], 0.0, "WaveSabre's noise is one signal on both channels");
    assert_eq!(filter_type::decode(p[filt(0, pm::f::TYPE)]), filter_type::SVF_BP);
    near(law::cutoff(p[filt(0, pm::f::CUTOFF)]), 20.0 + 19980.0 * 0.25, "cutoff");
    near(p[filt(0, pm::f::RESONANCE)], 0.3, "resonance");
    for (k, want) in [0.1, 0.2, 0.3, 0.4].into_iter().enumerate() {
        let e = [pm::e::ATTACK, pm::e::DECAY, pm::e::SUSTAIN, pm::e::RELEASE][k];
        near(p[pm::env(0, e)], want, "amp envelope");
        near(p[pm::env(1, e)], [0.5, 0.6, 0.7, 0.8][k], "mod envelope");
        near(p[pm::env(2, e)], [0.15, 0.25, 0.35, 0.45][k], "pitch envelope");
    }
    near(law::gain(p[v::MASTER]), 16.0 * (0.4f32 * 0.9).powi(2), "master: the −16 of Slaughter's output gain included");
    assert_eq!(law::count(p[v::UNISON], 1, 16), 5);
    near(p[v::UNI_DETUNE], 0.35, "unison detune");
    near(p[v::UNI_SPREAD], 0.8, "unison pan spread");
    near(law::transpose(p[v::TRANSPOSE]), 6.0, "rise is 24 semitones at 1");
    near(law::time(p[v::SLIDE]), 10.0 * 0.6f32.powi(4), "slide time");
    assert_eq!(law::choice(p[v::MODE], 2), 1);
    // routes: pitch envelope, vibrato, filter mod envelope
    let pitch: Vec<_> = (0..pm::MOD_SLOTS).filter(|&i| law::index(p[modslot(i, pm::m::TARGET)]) == target::PITCH).collect();
    assert_eq!(pitch.len(), 2, "pitch envelope and vibrato");
    let amount_of = |src: usize| {
        let i = pitch.iter().copied().find(|&i| law::choice(p[modslot(i, pm::m::SOURCE)], source::COUNT) == src).expect("a route");
        assert_eq!(law::choice(p[modslot(i, pm::m::OPERATION)], 2), 1, "Offset");
        law::offset_amount(p[modslot(i, pm::m::AMOUNT)], 36.0)
    };
    near(amount_of(source::ENV1 + 2), 0.4 * 2.0 * 36.0, "pitch envelope (envelope 3) depth in semitones");
    near(amount_of(source::LFO1), 0.5, "vibrato depth in semitones");
    // upstream advances its vibrato phase by `freq` radians per second
    near(law::lfo_rate(p[pm::lfo(0, pm::l::RATE)]), (0.25 + 0.1) * 70.0 / std::f32::consts::TAU, "vibrato rate in Hz");
    assert_eq!(law::choice(p[pm::lfo(0, pm::l::SHAPE)], unisynth::lfo::shape::ALL), unisynth::lfo::shape::SINE, "sine");
    assert_eq!(law::choice(p[pm::lfo(0, pm::l::SYNC)], pm::lfo_sync::COUNT), pm::lfo_sync::RESTART);
    let (src, op, amount) = route(&p, target::filter_cutoff(0)).expect("a cutoff route");
    assert!(src == source::ENV1 + 1 && op == 1);
    near(amount, 9990.0, "mod envelope depth in Hz");
    assert_eq!(law::choice(p[v::ENV1_AT_OUTPUT], 2), 1);
    near(p[v::VELOCITY], 0.0, "Slaughter ignores velocity");
    assert!(law::start_spread(p[v::START_SPREAD]) > 12.0 && law::start_spread(p[v::START_SPREAD]) < 13.0);
}

#[test]
fn sampler_parameters_decode_to_the_same_physical_values() {
    let mut l = vec![0.0f32; 26];
    l[0..4].copy_from_slice(&[0.1, 0.2, 0.3, 0.4]); // amp
    l[4] = 0.35; // start
    l[5] = 1.0; // ping-pong
    l[6] = 0.25;
    l[7] = 0.5;
    l[8] = 1.0; // reverse
    l[9] = 0.0; // nearest
    l[10] = 0.7; // coarse: floor((1.4 − 1)·12) = 4
    l[11] = 0.3; // fine: 2f − 1
    l[12] = 0.34; // high-pass
    l[13] = 0.4;
    l[14] = 0.2;
    l[15] = 0.9;
    l[16..20].copy_from_slice(&[0.5, 0.6, 0.7, 0.8]);
    l[23] = 0.8;
    let (out, _) = conv(&one_instrument(w::SPECIMEN, songs::gsm_chunk(&l), vec![], MELODY.to_vec()));
    let p = patch(&out, 0);
    let sh = |k| p[pm::osc(0, pm::o::SHAPE + k)];
    near(sh(pm::shape::SMP_START), 0.35, "start");
    assert_eq!(law::choice(sh(pm::shape::SMP_LOOP_MODE), 3), 2, "ping-pong");
    assert_eq!(law::choice(sh(pm::shape::SMP_BOUNDARY), 2), 1, "Specimen's bounds are manual");
    near(sh(pm::shape::SMP_LOOP_START), 0.25, "loop start");
    near(sh(pm::shape::SMP_LOOP_LENGTH), 0.5, "loop length");
    assert_eq!(law::choice(sh(pm::shape::SMP_REVERSE), 2), 1);
    assert_eq!(law::choice(sh(pm::shape::SMP_INTERP), 2), 0, "nearest");
    near(law::coarse(p[pm::osc(0, pm::o::COARSE)]), 4.0, "coarse rounds down like upstream");
    near(law::fine(p[pm::osc(0, pm::o::FINE)]), 2.0 * 0.3 - 1.0, "fine semitones");
    assert_eq!(filter_type::decode(p[filt(0, pm::f::TYPE)]), filter_type::SVF_HP);
    near(p[pm::env(0, pm::e::SUSTAIN)], 0.3, "sustain");
    near(p[pm::env(1, pm::e::RELEASE)], 0.8, "mod envelope release");
    near(law::gain(p[v::MASTER]), (0.4f32 * 0.8).powi(2), "master");
    near(p[v::VELOCITY], 1.0, "velocity scales the output");
    let (src, op, amount) = route(&p, target::filter_cutoff(0)).unwrap();
    assert!(src == source::ENV1 + 1 && op == 1);
    near(amount, 19980.0 * (2.0 * 0.9 - 1.0), "mod envelope depth in Hz");
    // the loop-mode decode `(int)(2v)`: 0.5 repeats, 0.49 does not, 0.25 does not
    for (v2, want) in [(0.5, 1), (0.49, 0), (0.25, 0), (0.99, 1)] {
        l[5] = v2;
        let (out, _) = conv(&one_instrument(w::SPECIMEN, songs::gsm_chunk(&l), vec![], MELODY.to_vec()));
        assert_eq!(law::choice(patch(&out, 0)[pm::osc(0, pm::o::SHAPE + pm::shape::SMP_LOOP_MODE)], 3), want, "loop {v2}");
    }
    // reverse is a boolean parameter: ≥ 0.5
    for (v2, want) in [(0.5, 1), (0.49, 0)] {
        l[8] = v2;
        let (out, _) = conv(&one_instrument(w::SPECIMEN, songs::gsm_chunk(&l), vec![], MELODY.to_vec()));
        assert_eq!(law::choice(patch(&out, 0)[pm::osc(0, pm::o::SHAPE + pm::shape::SMP_REVERSE)], 2), want, "reverse {v2}");
    }
}

#[test]
fn falcon_parameters_decode_to_the_same_physical_values() {
    let mut l = vec![0.0f32; 32];
    l[0] = 0.4; // op 1 blend
    l[1] = 0.1; // op 1 coarse
    l[2] = 0.6; // op 1 fine
    l[9] = 0.9;
    l[10] = 0.05;
    l[11] = 0.2;
    l[5..9].copy_from_slice(&[0.1, 0.2, 0.3, 0.4]);
    l[13..17].copy_from_slice(&[0.5, 0.6, 0.7, 0.8]);
    l[24..28].copy_from_slice(&[0.15, 0.25, 0.35, 0.45]);
    l[28] = 0.75;
    l[29] = 0.25;
    l[21] = 0.5;
    l[22] = 0.3;
    l[23] = 0.5;
    let (out, _) = conv(&one_instrument(w::FALCON, chunk_of(&l), vec![], MELODY.to_vec()));
    let p = patch(&out, 0);
    let o = |s, f| p[pm::osc(s, f)];
    near(law::ratio(o(0, pm::o::RATIO)) + law::ratio_fine(o(0, pm::o::RATIO_FINE)), 1.0 + (0.1f32 * 32.99).floor() + (0.2f32).powi(3), "op 1 ratio");
    near(law::ratio(o(1, pm::o::RATIO)) + law::ratio_fine(o(1, pm::o::RATIO_FINE)), 1.0 + (0.05f32 * 32.99).floor() + (-0.6f32).powi(3), "op 2 ratio");
    near(p[pm::osc(0, pm::o::SHAPE)], 0.4, "op 1 blend");
    near(p[pm::osc(1, pm::o::SHAPE)], 0.9, "op 2 blend");
    // envelope 1 is operator 2's, envelope 2 operator 1's
    near(p[pm::env(0, pm::e::DECAY)], 0.6, "envelope 1 = operator 2");
    near(p[pm::env(1, pm::e::DECAY)], 0.2, "envelope 2 = operator 1");
    near(p[pm::env(2, pm::e::SUSTAIN)], 0.35, "pitch envelope");
    let (src, op, a) = route(&p, target::osc(0, target::OSC_PITCH)).unwrap();
    assert!(src == source::ENV1 + 2 && op == 1);
    near(a, 0.25 * 2.0 * 36.0, "operator 1 pitch envelope depth");
    let (_, _, a) = route(&p, target::osc(1, target::OSC_PITCH)).unwrap();
    near(a, -0.25 * 2.0 * 36.0, "operator 2 pitch envelope depth");
    let (src, op, a) = route(&p, target::osc(0, target::OSC_GAIN)).unwrap();
    assert!(src == source::ENV1 + 1 && op == 0);
    near(a, 1.0, "operator 1 level follows its envelope");
    let (src, op, _) = route(&p, target::osc(1, target::OSC_GAIN)).unwrap();
    assert!(src == source::ENV1 && op == 0, "operator 2 level follows envelope 1");
    near(law::start_spread(p[v::START_SPREAD]), 0.01, "Falcon's operators start in phase");
    near(law::transpose(p[v::TRANSPOSE]), 12.0, "rise 0.5 is 24·0.5 semitones");
}

#[test]
fn adultery_decodes_its_wave_index_loop_boundary_and_defaults() {
    let mut l = vec![0.0f32; 28];
    l[0] = 25.0; // wave 24 (the index is 1-based in the parameter)
    let sh = |p: &[f32; COUNT], k| p[pm::osc(0, pm::o::SHAPE + k)];
    let at = |l: &[f32]| {
        let (out, _) = conv(&one_instrument(w::ADULTERY, chunk_of(l), vec![], MELODY.to_vec()));
        patch(&out, 0)
    };
    let p = at(&l);
    assert_eq!(law::choice(sh(&p, pm::shape::SMP_SOURCE), 2), 1, "a DLS wave");
    assert_eq!(p[pm::smp_index(0)], 25.0, "the raw index is the legacy parameter");
    assert_eq!(law::choice(sh(&p, pm::shape::SMP_BOUNDARY), 2), 0, "boundary 0: from the wave's loop");
    l[7] = 1.0;
    assert_eq!(law::choice(sh(&at(&l), pm::shape::SMP_BOUNDARY), 2), 1, "boundary 1: manual");
    // a short chunk keeps upstream's defaults: repeat loop, bounds from the sample, whole-sample loop length
    let short = at(&[]);
    assert_eq!(law::choice(sh(&short, pm::shape::SMP_LOOP_MODE), 3), 1);
    assert_eq!(law::choice(sh(&short, pm::shape::SMP_BOUNDARY), 2), 0);
    assert_eq!(sh(&short, pm::shape::SMP_LOOP_LENGTH), 1.0);
    assert_eq!(law::choice(sh(&short, pm::shape::SMP_INTERP), 2), 1, "linear");
    // Specimen's: no loop, manual bounds
    let (out, _) = conv(&one_instrument(w::SPECIMEN, songs::gsm_chunk(&[]), vec![], MELODY.to_vec()));
    let p = patch(&out, 0);
    assert_eq!(law::choice(sh(&p, pm::shape::SMP_LOOP_MODE), 3), 0);
    assert_eq!(law::choice(sh(&p, pm::shape::SMP_BOUNDARY), 2), 1);
    assert_eq!(sh(&p, pm::shape::SMP_LOOP_LENGTH), 1.0);
}

// ---- findings of the adversarial review ----

#[test]
fn a_sampler_loop_mode_sweep_steps_where_the_legacy_decode_changes() {
    // `(int)(2v)`: off below 0.5, repeat from 0.5, ping-pong only at exactly 1
    let a = Auto { device: 0, param: 5, points: vec![(0, 0), (44100, 255)] };
    let (out, _) = conv(&one_instrument(w::SPECIMEN, songs::gsm_chunk(&[0.0; 26]), vec![a], MELODY.to_vec()));
    let idx = pm::osc(0, pm::o::SHAPE + pm::shape::SMP_LOOP_MODE);
    let lane = lane_of(&out, 0, idx);
    let got = read_lane(&lane, 450);
    let legacy = read_lane(&[(0, 0), (44100, 255)], 450);
    let want = |v: f32| ((v * 2.0) as i32).clamp(0, 2) as usize;
    let mut seen = std::collections::BTreeSet::new();
    for (k, (g, l)) in got.iter().zip(&legacy).enumerate() {
        let (gu, lw) = (law::choice(*g, 3), want(*l));
        // quantising to a byte may move a switch by one block, never more
        assert!(gu == lw || (k > 0 && (gu == want(legacy[k - 1]) || gu == want(legacy[(k + 1).min(449)]))), "block {k}: {gu} vs {lw}");
        seen.insert(gu);
    }
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), vec![0, 1, 2]);
    assert!(lane.windows(2).filter(|w| w[0].0 == w[1].0).count() >= 2, "steps, not a ramp: {lane:?}");
    // the same for the boolean and the interpolation choice: a ramp 0 → 1 switches once, at the legacy frame
    for (param, fld, n, at) in [(8, pm::shape::SMP_REVERSE, 2, 22050), (9, pm::shape::SMP_INTERP, 2, 44100)] {
        let a = Auto { device: 0, param, points: vec![(0, 0), (44100, 255)] };
        let (out, _) = conv(&one_instrument(w::SPECIMEN, songs::gsm_chunk(&[0.0; 26]), vec![a], MELODY.to_vec()));
        let lane = lane_of(&out, 0, pm::osc(0, pm::o::SHAPE + fld));
        let v = read_lane(&lane, 450);
        let first = v.iter().position(|x| law::choice(*x, n) == 1).unwrap() * 100;
        assert!((first as i64 - at as i64).abs() <= 200, "param {param} switches at {first}, legacy at {at}");
        assert!(v[..first / 100].iter().all(|x| law::choice(*x, n) == 0), "no value in between");
    }
}

#[test]
fn a_device_shared_by_two_tracks_needs_the_voices_of_both() {
    let chord = vec![(0, 60, Some(100)), (0, 64, Some(100)), (2000, 60, None), (0, 64, None)];
    let mut t0 = track(&[0], 0);
    t0.volume = 1.0;
    let t1 = track(&[0], 1);
    let src = song(
        vec![synth_convert::model::Dev { kind: w::SLAUGHTER, chunk: slaughter(|_| {}) }],
        vec![chord.clone(), chord],
        vec![t0, t1, track(&[], 0)],
    );
    let (out, _) = conv(&src);
    assert_eq!(law::count(patch(&out, 0)[v::CAPACITY], 1, 64), 4);
}

#[test]
fn out_of_range_legacy_values_follow_upstream_decodes() {
    // interpolation 2 plays silence upstream: the slot is muted
    let mut l = vec![0.0f32; 26];
    l[9] = 2.0;
    let (out, _) = conv(&one_instrument(w::SPECIMEN, songs::gsm_chunk(&l), vec![], MELODY.to_vec()));
    assert_eq!(law::gain(patch(&out, 0)[pm::osc(0, pm::o::GAIN)]), 0.0);
    l[9] = 1.0;
    let (out, _) = conv(&one_instrument(w::SPECIMEN, songs::gsm_chunk(&l), vec![], MELODY.to_vec()));
    assert!(law::gain(patch(&out, 0)[pm::osc(0, pm::o::GAIN)]) > 0.9);
    // NaN reads as 0 (upstream's `(int)` decodes), not as the default: the loop mode default of Adultery is repeat
    let mut a = vec![0.0f32; 28];
    a[6] = f32::NAN;
    let (out, _) = conv(&one_instrument(w::ADULTERY, chunk_of(&a), vec![], MELODY.to_vec()));
    assert_eq!(law::choice(patch(&out, 0)[pm::osc(0, pm::o::SHAPE + pm::shape::SMP_LOOP_MODE)], 3), 0);
    // more than 16 unison voices cannot be kept: warned, and 16 are
    let mut s = vec![0.0f32; 42];
    s[2] = 1.0;
    s[29] = 2.0;
    let (out, w) = conv(&one_instrument(w::SLAUGHTER, chunk_of(&s), vec![], MELODY.to_vec()));
    assert_eq!(law::count(patch(&out, 0)[v::UNISON], 1, 16), 16);
    assert!(w.iter().any(|m| m.contains("31") && m.contains("16")), "{w:?}");
}

#[test]
fn a_whole_number_ratio_sweep_steps_at_the_legacy_thresholds() {
    let mut f = vec![0.0f32; 32];
    f[7] = 1.0;
    f[17] = 0.7;
    let a = Auto { device: 0, param: 10, points: vec![(0, 0), (44100, 255)] };
    let (out, _) = conv(&one_instrument(w::FALCON, chunk_of(&f), vec![a], MELODY.to_vec()));
    let lane = lane_of(&out, 0, pm::osc(1, pm::o::RATIO));
    let got = read_lane(&lane, 450);
    let legacy = read_lane(&[(0, 0), (44100, 255)], 450);
    let mut switches = 0;
    for (k, (g, l)) in got.iter().zip(&legacy).enumerate() {
        let (gu, lw) = (law::ratio(*g) as i32, 1 + (l * 32.99).floor() as i32);
        assert!((gu - lw).abs() <= 1, "block {k}: ratio {gu} vs {lw}");
        if k > 0 && law::ratio(got[k - 1]) != law::ratio(*g) {
            switches += 1;
        }
    }
    assert_eq!(switches, 32, "one step per whole ratio, never a ramp through fractions");
}
