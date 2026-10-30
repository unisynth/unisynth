//! `features` (task 6.4): the cargo features a unified song needs.

mod common;
#[path = "../../wavesabre/tests/songs.rs"]
#[allow(dead_code)]
mod songs;

use common::*;
use synth_convert::features::{explain, features, read_cost, render_json, render_text};
use synth_convert::model::{Dev, SongM};
use synth_convert::{convert, ConvError};
use unisynth::params::{self as pm, filter_type as ft, v, COUNT};
use wavesabre::kind as w;

const MELODY: [(i32, u8, Option<u8>); 2] = [(0, 60, Some(100)), (20000, 60, None)];

fn converted(kind: u8, chunk: Vec<u8>) -> SongM {
    let s = song(vec![Dev { kind, chunk }], vec![MELODY.to_vec()], vec![track(&[0], 0)]);
    SongM::read(&convert(&s.write()).unwrap().blob).unwrap()
}

fn of(s: &SongM) -> Vec<&'static str> {
    features(&s.write()).unwrap()
}

fn slaughter(f: impl Fn(&mut [f32])) -> SongM {
    let mut p = vec![0.0f32; 42];
    p[2] = 1.0; // oscillator 1 level
    p[17] = 1.0; // master
    f(&mut p);
    converted(w::SLAUGHTER, dev(0, &p).chunk)
}

/// A unified instrument song: the default patch with `f` applied, and automation lanes `(param, points)`.
fn unified(f: impl Fn(&mut [f32; COUNT]), lanes: &[(usize, &[(i32, u8)])]) -> SongM {
    let mut p = pm::DEFAULTS;
    f(&mut p);
    let mut t = track(&[0], 0);
    t.autos = lanes.iter().map(|&(param, pts)| auto(0, param, pts)).collect();
    let d = Dev { kind: unisynth::kind::INSTRUMENT, chunk: unisynth::chunk::encode(&p, &[]) };
    song(vec![d], vec![MELODY.to_vec()], vec![t])
}

#[test]
fn a_slaughter_without_effects_needs_the_saw_pulse_oscillator_the_state_variable_filter_and_the_wavesabre_envelope() {
    // no noise: the oscillators and the filter only
    let s = slaughter(|p| {
        p[2] = 1.0;
    });
    let f = of(&s);
    assert!(f.contains(&"us-osc-blit") && f.contains(&"us-flt-svf") && f.contains(&"us-env-seconds"), "{f:?}");
    assert!(!f.contains(&"us-noise"), "a silent noise source costs no code: {f:?}");
    assert!(f.iter().all(|x| !x.starts_with("us-fx") && !x.contains("osc-pm") && !x.contains("osc-wt")), "{f:?}");
    // with noise
    let n = of(&slaughter(|p| p[15] = 0.8)); // noise level
    assert!(n.contains(&"us-noise"), "{n:?}");
}

#[test]
fn each_instrument_kind_lists_its_oscillator_and_sample_source() {
    let mut falcon = vec![0.0f32; 32];
    falcon[17] = 0.8;
    let f = of(&converted(w::FALCON, dev(0, &falcon).chunk));
    assert!(f.contains(&"us-osc-pm") && !f.contains(&"us-osc-blit"), "{f:?}");
    let t = of(&converted(w::THUNDER, songs::gsm_chunk(&[])));
    assert!(t.contains(&"us-osc-sample") && t.contains(&"us-sample-gsm") && !t.contains(&"us-sample-dls"), "{t:?}");
    let mut ad = vec![0.0f32; 28];
    ad[27] = 3.0; // wave index
    let a = of(&converted(w::ADULTERY, dev(0, &ad).chunk));
    assert!(a.contains(&"us-osc-sample") && a.contains(&"us-sample-dls") && !a.contains(&"us-sample-gsm"), "{a:?}");
}

#[test]
fn a_dls_instrument_source_needs_the_bank_support_and_no_gsm() {
    let slot = |source: usize| {
        many(
            1,
            |_, p| {
                p[pm::osc(0, pm::o::TYPE)] = unisynth::law::host::inv_choice(pm::osc_type::SAMPLER, pm::osc_type::COUNT);
                p[pm::osc(0, pm::o::SHAPE + pm::shape::SMP_SOURCE)] = unisynth::law::host::inv_choice(source, pm::smp_source::COUNT);
                p[pm::smp_index(0)] = 1.0;
            },
            &[],
        )
    };
    let ins = of(&slot(pm::smp_source::INSTRUMENT));
    assert!(ins.contains(&"us-osc-sample") && ins.contains(&"us-sample-dls") && !ins.contains(&"us-sample-gsm"), "{ins:?}");
    let wave = of(&slot(pm::smp_source::WAVE));
    assert!(wave.contains(&"us-sample-dls") && !wave.contains(&"us-sample-gsm"), "{wave:?}");
}

#[test]
fn every_effect_kind_lists_exactly_its_mode() {
    let want = [
        (w::SCISSOR, "us-fx-distortion-shaper"),
        (tfsynth::device::kind::DISTORTION, "us-fx-distortion-power"),
        (w::CRUSHER, "us-fx-crusher"),
        (w::LEVELLER, "us-fx-eq-parametric"),
        (tfsynth::device::kind::EQ, "us-fx-eq-3band"),
        (w::ECHO, "us-fx-delay-tempo"),
        (tfsynth::device::kind::DELAY, "us-fx-delay-ms"),
        (tfsynth::device::kind::CHORUS, "us-fx-chorus"),
        (w::TWISTER, "us-fx-flanger-phaser"),
        (tfsynth::device::kind::FLANGER, "us-fx-flanger-ring"),
        (w::CATHEDRAL, "us-fx-reverb-freeverb"),
        (tfsynth::device::kind::REVERB, "us-fx-reverb-compact"),
        (w::CHAMBER, "us-fx-reverb-rooms"),
        (w::SMASHER, "us-fx-compressor"),
        (tfsynth::device::kind::FORMANT, "us-fx-formant"),
    ];
    for (k, feature) in want {
        let s = song(vec![dev(k, &[0.5; 4])], vec![vec![]], vec![track(&[0], 0)]);
        let out = convert(&s.write()).unwrap().blob;
        assert_eq!(features(&out).unwrap(), vec![feature], "legacy kind {k}");
    }
}

#[test]
fn a_song_without_devices_needs_nothing_and_a_default_instrument_needs_only_the_envelope() {
    assert_eq!(features(&song(vec![], vec![vec![]], vec![track(&[], 0)]).write()).unwrap(), Vec::<&str>::new());
    // the default patch has no oscillator, no filter and no noise; envelope 1 always runs
    let f = of(&unified(|_| {}, &[]));
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(f[0].starts_with("us-env-"), "{f:?}");
}

#[test]
fn an_automation_lane_adds_the_filter_types_it_can_step_to_and_the_noise_it_can_raise() {
    let base = of(&unified(|p| p[pm::filt(0, pm::f::TYPE)] = unisynth::law::host::inv_choice(ft::SVF_LP, ft::COUNT), &[]));
    assert!(base.contains(&"us-flt-svf") && !base.contains(&"us-flt-ladder") && !base.contains(&"us-noise"), "{base:?}");
    // a lane that runs the type from the low end to the top reaches every filter
    let swept = of(&unified(
        |p| p[pm::filt(0, pm::f::TYPE)] = unisynth::law::host::inv_choice(ft::SVF_LP, ft::COUNT),
        &[(pm::filt(0, pm::f::TYPE), &[(0, 0), (100, 255)])],
    ));
    for want in ["us-flt-svf", "us-flt-ladder", "us-flt-rbj", "us-flt-notch"] {
        assert!(swept.contains(&want), "{want} missing from {swept:?}");
    }
    // a lane that stays on one svf type adds nothing (the V2 kinds sit between the old ones: 30..38 of 255 rounds to
    // stored position 2 of 17)
    let near = of(&unified(
        |p| p[pm::filt(0, pm::f::TYPE)] = unisynth::law::host::inv_choice(ft::SVF_LP, ft::COUNT),
        &[(pm::filt(0, pm::f::TYPE), &[(0, 30), (100, 38)])],
    ));
    // the lane itself needs the automation walk; the filter types stay those of the chunk
    assert_eq!(near.into_iter().filter(|f| *f != "us-automation").collect::<Vec<_>>(), base);
    // noise: silent in the chunk, raised by a lane
    let raised = of(&unified(|_| {}, &[(v::NOISE_LEVEL, &[(0, 0), (100, 200)])]));
    assert!(raised.contains(&"us-noise"), "{raised:?}");
    let flat = of(&unified(|_| {}, &[(v::NOISE_LEVEL, &[(0, 0), (100, 0)])]));
    assert!(!flat.contains(&"us-noise"), "{flat:?}");
}

/// A unified instrument song with `n` instruments built from `f(device index, patch)`.
fn many(n: usize, f: impl Fn(usize, &mut [f32; COUNT]), lanes: &[(usize, usize, &[(i32, u8)])]) -> SongM {
    let devices = (0..n)
        .map(|d| {
            let mut p = pm::DEFAULTS;
            f(d, &mut p);
            Dev { kind: unisynth::kind::INSTRUMENT, chunk: unisynth::chunk::encode(&p, &[]) }
        })
        .collect();
    let mut t = track(&(0..n).collect::<Vec<_>>(), 0);
    t.autos = lanes.iter().map(|&(d, param, pts)| auto(d, param, pts)).collect();
    song(devices, vec![MELODY.to_vec()], vec![t])
}

fn ty(x: usize) -> f32 {
    unisynth::law::host::inv_choice(x, ft::COUNT)
}

#[test]
fn the_chunk_value_counts_with_the_lane_and_a_lane_belongs_to_its_own_device() {
    let slot = pm::filt(0, pm::f::TYPE);
    // the chunk says ladder, the lane only visits svf types: ladder is still needed
    let s = many(1, |_, p| p[slot] = ty(ft::LADDER_LP), &[(0, slot, &[(0, 30), (100, 40)])]);
    assert!(of(&s).contains(&"us-flt-ladder"), "{:?}", of(&s));
    // device 0 is svf, device 1 is the notch with a lane that stays on the notch: no rbj or ladder for either
    let s = many(2, |d, p| p[slot] = ty(if d == 0 { ft::SVF_LP } else { ft::NOTCH }), &[(1, slot, &[(0, 255), (100, 255)])]);
    let f = of(&s);
    assert!(f.contains(&"us-flt-svf") && f.contains(&"us-flt-notch") && !f.contains(&"us-flt-ladder") && !f.contains(&"us-flt-rbj"), "{f:?}");
}

#[test]
fn envelopes_other_than_the_first_count_when_read_or_when_a_lane_can_route_them() {
    let mode = |e| pm::env(e, pm::e::MODE);
    let tf = unisynth::law::host::inv_choice(1, 2);
    // envelope 2 is Tunefish-mode but nothing reads it
    let idle = many(1, |_, p| p[mode(1)] = tf, &[]);
    assert!(!of(&idle).contains(&"us-env-rate"), "{:?}", of(&idle));
    // read by a mod slot
    let read = many(
        1,
        |_, p| {
            p[mode(1)] = tf;
            p[pm::modslot(0, pm::m::SOURCE)] = unisynth::law::host::inv_choice(unisynth::modmx::source::ENV1 + 1, unisynth::modmx::source::COUNT);
        },
        &[],
    );
    assert!(of(&read).contains(&"us-env-rate"), "{:?}", of(&read));
    // a lane on a routing parameter can read it later
    let lane = many(1, |_, p| p[mode(1)] = tf, &[(0, pm::modslot(3, pm::m::SOURCE), &[(0, 0), (100, 255)])]);
    assert!(of(&lane).contains(&"us-env-rate") && of(&lane).contains(&"us-noise"), "{:?}", of(&lane));
    // envelope 1 in rate mode
    let one = many(1, |_, p| p[mode(0)] = tf, &[]);
    assert!(of(&one).contains(&"us-env-rate") && !of(&one).contains(&"us-env-seconds"), "{:?}", of(&one));
}

#[test]
fn noise_routed_by_the_matrix_and_gsm_samples_without_a_sampler_are_needed() {
    let routed = many(
        1,
        |_, p| {
            p[pm::modslot(0, pm::m::SOURCE)] = unisynth::law::host::inv_choice(unisynth::modmx::source::VELOCITY, unisynth::modmx::source::COUNT);
            p[pm::modslot(0, pm::m::TARGET)] = unisynth::law::host::inv_index(unisynth::modmx::target::NOISE_LEVEL);
        },
        &[],
    );
    assert!(of(&routed).contains(&"us-noise"), "{:?}", of(&routed));
    // a patch whose sampler was switched off but that still carries its GSM sample: the build must be able to read it
    let th = converted(w::THUNDER, songs::gsm_chunk(&[]));
    let c = unisynth::chunk::Chunk::parse(&th.devices[0].chunk).unwrap();
    let mut p = c.values();
    p[pm::osc(0, pm::o::TYPE)] = 0.0;
    let mut s = many(1, |_, _| {}, &[]);
    s.devices[0].chunk = unisynth::chunk::encode(&p, &[c.sample_bytes(0).unwrap()]);
    assert!(of(&s).contains(&"us-sample-gsm"), "{:?}", of(&s));
}

#[test]
fn the_optional_voice_paths_are_needed_only_when_the_patch_or_a_lane_uses_them() {
    use unisynth::law::host::{inv_choice, inv_count};
    use unisynth::modmx::source;
    let voice = ["us-voice-lfo", "us-voice-glide", "us-voice-mono", "us-voice-unison", "us-voice-oneshot"];
    let base = of(&unified(|_| {}, &[]));
    assert!(voice.iter().all(|f| !base.contains(f)), "the default patch uses none: {base:?}");
    let has = |s: SongM, f: &str| of(&s).contains(&f);
    // LFO: read by a mod slot, or reachable through a routing lane; an LFO nothing reads costs nothing
    let lfo_src = pm::modslot(2, pm::m::SOURCE);
    assert!(has(unified(|p| p[lfo_src] = inv_choice(source::LFO1 + 1, source::COUNT), &[]), "us-voice-lfo"));
    assert!(!has(unified(|p| p[pm::lfo(0, pm::l::RATE)] = 0.7, &[]), "us-voice-lfo"));
    assert!(has(unified(|_| {}, &[(lfo_src, &[(0, 0), (100, 255)])]), "us-voice-lfo"));
    // glide: a glide time, or a lane that raises it; slide only in mono mode
    assert!(has(unified(|p| p[v::GLIDE] = 0.3, &[]), "us-voice-glide"));
    assert!(has(unified(|_| {}, &[(v::GLIDE, &[(0, 0), (100, 90)])]), "us-voice-glide"));
    assert!(!has(unified(|p| p[v::SLIDE] = 0.3, &[]), "us-voice-glide"), "poly: the slide time is never read");
    let mono = inv_choice(1, 2);
    let m = of(&unified(|p| {
        p[v::MODE] = mono;
        p[v::SLIDE] = 0.3;
    }, &[]));
    assert!(m.contains(&"us-voice-mono") && m.contains(&"us-voice-glide"), "{m:?}");
    let snap = of(&unified(|p| p[v::MODE] = mono, &[]));
    assert!(snap.contains(&"us-voice-mono") && !snap.contains(&"us-voice-glide"), "{snap:?}");
    assert!(has(unified(|_| {}, &[(v::MODE, &[(0, 0), (100, 255)])]), "us-voice-mono"));
    // unison above one voice, one-shot on
    assert!(has(unified(|p| p[v::UNISON] = inv_count(3, 1, 16), &[]), "us-voice-unison"));
    assert!(!has(unified(|p| p[v::UNI_DETUNE] = 0.5, &[]), "us-voice-unison"));
    assert!(has(unified(|p| p[v::ONE_SHOT] = inv_choice(1, 2), &[]), "us-voice-oneshot"));
}

#[test]
fn the_mod_matrix_is_needed_when_a_slot_routes_or_a_lane_can_create_a_route() {
    use unisynth::law::host::{inv_choice, inv_index};
    use unisynth::modmx::{source, target};
    assert!(!of(&unified(|_| {}, &[])).contains(&"us-matrix"));
    let route = |p: &mut [f32; COUNT]| {
        p[pm::modslot(5, pm::m::SOURCE)] = inv_choice(source::VELOCITY, source::COUNT);
        p[pm::modslot(5, pm::m::TARGET)] = inv_index(target::PITCH);
    };
    assert!(of(&unified(route, &[])).contains(&"us-matrix"));
    // a source without a target (or a target without a source) routes nothing
    let half = of(&unified(|p| p[pm::modslot(5, pm::m::SOURCE)] = inv_choice(source::VELOCITY, source::COUNT), &[]));
    assert!(!half.contains(&"us-matrix"), "{half:?}");
    let lane = of(&unified(|_| {}, &[(pm::modslot(1, pm::m::TARGET), &[(0, 0), (100, 255)])]));
    assert!(lane.contains(&"us-matrix"), "{lane:?}");
}

#[test]
fn automation_is_needed_when_a_track_has_a_lane() {
    assert!(!of(&unified(|_| {}, &[])).contains(&"us-automation"));
    let s = unified(|_| {}, &[(pm::filt(0, pm::f::CUTOFF), &[(0, 10), (100, 200)])]);
    assert!(of(&s).contains(&"us-automation"), "{:?}", of(&s));
    let w = why(&s, "us-automation");
    assert!(w.iter().any(|w| w.contains("track 0") && w.contains("filt1.cutoff")), "{w:?}");
}

#[test]
fn legacy_and_malformed_songs_are_errors_naming_the_problem() {
    let legacy = song(vec![dev(w::SCISSOR, &[0.5; 4])], vec![vec![]], vec![track(&[0], 0)]).write();
    match features(&legacy) {
        Err(ConvError::Device(k, m)) => assert!(k == w::SCISSOR && m.contains("convert"), "{m}"),
        r => panic!("{:?}", r.err()),
    }
    assert!(matches!(features(&[1, 2, 3]), Err(ConvError::Input(_))));
    let mut bad = unified(|_| {}, &[]);
    bad.devices[0].chunk = vec![9, 9, 9];
    assert!(matches!(features(&bad.write()), Err(ConvError::Device(..)) | Err(ConvError::Input(_))));
}

/// Opt-in: `FEATURES_DUMP=<dir>` writes the WaveSabre test songs (`legacy/<name>.song`) and their conversions
/// (`<name>.song`) there, for `tools/check-song-features.sh` and the converter walkthrough in `docs/synth/unisynth.md`.
#[test]
fn dump_songs_for_the_compo_build_check() {
    let Some(dir) = std::env::var_os("FEATURES_DUMP") else { return };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(dir.join("legacy")).unwrap();
    for (name, blob) in songs::all().into_iter().chain([("showcase", songs::showcase())]) {
        std::fs::write(dir.join("legacy").join(format!("{name}.song")), &blob).unwrap();
        std::fs::write(dir.join(format!("{name}.song")), convert(&blob).unwrap().blob).unwrap();
    }
}

fn why(s: &SongM, feature: &str) -> Vec<String> {
    explain(&s.write(), None).unwrap().into_iter().find(|n| n.feature == feature).map(|n| n.why).unwrap_or_default()
}

#[test]
fn the_explained_map_lists_the_same_features_and_names_each_cause() {
    let s = slaughter(|p| p[15] = 0.8);
    let e = explain(&s.write(), None).unwrap();
    assert_eq!(e.iter().map(|n| n.feature).collect::<Vec<_>>(), of(&s), "same list, same order");
    assert!(e.iter().all(|n| !n.why.is_empty() && n.cost.is_none()));
    assert_eq!(why(&s, "us-noise"), ["instrument device 0: noise_level is above 0"]);
    assert_eq!(why(&s, "us-osc-blit"), ["instrument device 0: osc1.type is saw/pulse"]);
    let ws = why(&s, "us-env-seconds");
    assert_eq!(ws[0], "instrument device 0: env1.law is the seconds law, envelope 1 ends the voice");
    assert!(ws[1].starts_with("instrument device 0: env2.law is the seconds law, mod"), "{ws:?}");
}

#[test]
fn a_cause_through_an_automation_lane_says_so() {
    // the filter type is off in the patch; a lane steps it to the ladder
    let low = |p: &mut [f32; COUNT]| p[pm::filt(0, pm::f::TYPE)] = unisynth::law::host::inv_choice(ft::SVF_LP, ft::COUNT);
    let s = unified(low, &[(pm::filt(0, pm::f::TYPE), &[(0, 0), (100, 255)])]);
    let w = why(&s, "us-flt-notch");
    assert_eq!(w, ["instrument device 0: filt1.type can be the notch filter (automated)"], "{w:?}");
    // a lane on a routing parameter can reach the noise and every envelope
    let r = unified(|_| {}, &[(pm::modslot(0, pm::m::TARGET), &[(0, 0), (100, 255)])]);
    assert_eq!(why(&r, "us-noise"), ["instrument device 0: a lane on mod1 can route to the noise level"]);
    assert!(why(&r, "us-env-seconds").iter().any(|w| w.contains("a lane on mod1 can route envelope 2")), "{:?}", why(&r, "us-env-seconds"));
}

#[test]
fn an_effect_names_its_device_kind_and_mode() {
    let mut p = vec![0.0f32; 7];
    p[0] = 0.5;
    let s = song(vec![dev(w::ECHO, &p)], vec![vec![]], vec![track(&[0], 0)]);
    let c = SongM::read(&convert(&s.write()).unwrap().blob).unwrap();
    assert_eq!(why(&c, "us-fx-delay-tempo"), ["device 0: delay (kind 68), mode 0"]);
}

#[test]
fn the_cost_table_gives_each_features_saving_and_the_text_adds_them_up() {
    let table = "feature,raw,xz\nall,100000,50000\nus-noise,99488,49816\nus-flt-svf,99000,49500\nodd,line\n";
    let cost = read_cost(table);
    assert_eq!(cost["us-noise"], (512, 184));
    assert_eq!(cost["us-flt-svf"], (1000, 500));
    assert!(!cost.contains_key("all") && !cost.contains_key("odd"));
    assert!(read_cost("feature,raw,xz\nus-noise,1,1\n").is_empty(), "no `all` row: nothing to subtract from");
    let s = slaughter(|p| p[15] = 0.8);
    let e = explain(&s.write(), Some(&cost)).unwrap();
    let text = render_text(&e);
    assert!(text.contains("us-noise   +0.5 K raw, +0.2 K xz"), "{text}");
    assert!(text.contains("marginal cost of the listed features") && text.contains("+1.5 K raw, +0.7 K xz"), "{text}");
    assert!(text.contains("us-osc-blit\n"), "a feature without a cost row has no figure: {text}");
}

#[test]
fn many_causes_are_cut_in_the_text_and_kept_in_the_json() {
    let mut p = pm::DEFAULTS;
    p[v::NOISE_LEVEL] = 1.0;
    let d = || Dev { kind: unisynth::kind::INSTRUMENT, chunk: unisynth::chunk::encode(&p, &[]) };
    let s = song(vec![d(), d(), d(), d(), d()], vec![MELODY.to_vec()], vec![track(&[0], 0)]);
    let e = explain(&s.write(), None).unwrap();
    let noise = e.iter().find(|n| n.feature == "us-noise").unwrap();
    assert_eq!(noise.why.len(), 5);
    let text = render_text(&e);
    assert!(text.contains("… and 2 more"), "{text}");
    let json = render_json(&e);
    assert!(json.starts_with('[') && json.contains("\"raw\":null") && json.matches("noise_level is above 0").count() == 5, "{json}");
    // quotes and control characters escape
    let q = render_json(&[synth_convert::features::Need { feature: "f", why: vec!["a\"b\\c\n".into()], cost: Some((1, 2)) }]);
    assert_eq!(q, "[{\"feature\":\"f\",\"why\":[\"a\\\"b\\\\c\\u000a\"],\"raw\":1,\"xz\":2}]\n");
}
