//! `rust`: a unified song as Rust source for compo builds (shrink-unified-synth 4.4).

mod common;
#[path = "../../wavesabre/tests/songs.rs"]
#[allow(dead_code)]
mod songs;

use common::*;
use synth_convert::features::features;
use synth_convert::model::SongM;
use synth_convert::rust::{bake, generate};
use synth_convert::{convert, ConvError};
use synth_core::mix::{Env, Player, Song};
use synth_core::pool::Pool;
use unisynth::device::UDevice;
use unisynth::params as pm;
use unisynth::rng::Xorshift;

const MELODY: [(i32, u8, Option<u8>); 4] = [(0, 60, Some(100)), (5000, 64, Some(0)), (5000, 60, None), (9000, 64, None)];

/// A unified phase-mod instrument through a low-pass whose cutoff a lane moves (one point before the song start).
fn sample() -> Vec<u8> {
    let mut p = pm::DEFAULTS;
    p[pm::osc(0, pm::o::TYPE)] = unisynth::law::host::inv_choice(unisynth::params::osc_type::PM, unisynth::params::osc_type::COUNT);
    p[pm::filt(0, pm::f::TYPE)] = unisynth::law::host::inv_choice(unisynth::params::filter_type::SVF_LP, unisynth::params::filter_type::COUNT);
    let ins = synth_convert::model::Dev { kind: unisynth::kind::INSTRUMENT, chunk: unisynth::chunk::encode(&p, &[]) };
    let mut t = track(&[0], 0);
    t.autos = vec![auto(0, pm::filt(0, pm::f::CUTOFF), &[(-100, 10), (20000, 250)])];
    song(vec![ins], vec![MELODY.to_vec()], vec![t]).write()
}

/// Renders `song` with every module from frame 0, then again after seeking into the middle of a sub-block.
fn render(song: Song, sub: usize) -> (Vec<f32>, Vec<i16>) {
    let env = Env::for_song(&song, None);
    let need = Player::<UDevice, Xorshift>::required_pool(&song, &env, sub).unwrap();
    let mut mem = vec![0u8; need];
    let mut pool = Pool::new(&mut mem);
    let mut p = Player::<UDevice, Xorshift>::new(song, &env, &mut pool, Xorshift::new(1), sub).unwrap();
    let mut f = vec![0.0f32; 2 * 30_000];
    p.render_f32(&mut f);
    p.seek(12_345);
    let mut i = vec![0i16; 2 * 20_000];
    p.render_i16(&mut i);
    (f, i)
}

#[test]
fn the_baked_sections_play_exactly_what_the_blob_plays() {
    for blob in [sample(), convert(&songs::showcase()).unwrap().blob] {
        let b = bake(&blob, None).unwrap();
        let baked = Song::from_sections(b.tempo, b.rate, f64::from_bits(b.length_bits), b.sections.each_ref().map(|(n, s)| (*n, &s[..])));
        for sub in [100, 77] {
            assert!(render(Song::parse(&blob).unwrap(), sub) == render(baked, sub), "sub {sub}");
        }
    }
}

#[test]
fn the_module_list_is_the_feature_list_and_the_source_asserts_it() {
    let blob = sample();
    let b = bake(&blob, None).unwrap();
    let names: Vec<_> = b.modules.iter().map(|&id| unisynth::module::feature(id)).collect();
    assert_eq!(names, features(&blob).unwrap());
    let src = generate(&blob, None, "sample.song").unwrap();
    let ids = b.modules.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(", ");
    assert!(src.contains(&format!("assert!(unisynth::module::compiled_all(&[{ids}])")), "{src}");
    assert!(src.contains("Song::from_sections(") && src.contains("f64::from_bits(") && src.contains("Do not edit"), "{src}");
    // every section's bytes are in the source, in order
    for (i, (_, s)) in b.sections.iter().enumerate() {
        assert!(src.contains(&format!("static S{i}: [u8; {}] = {:?};", s.len(), s)), "section {i}");
    }
}

#[test]
fn a_song_the_player_would_refuse_is_refused() {
    // garbage, a legacy device, an automation of a parameter the device lacks, a receive cycle
    assert!(matches!(bake(&[1, 2, 3], None), Err(ConvError::Input(_))));
    let legacy = song(vec![dev(wavesabre::kind::SCISSOR, &[0.5; 4])], vec![vec![]], vec![track(&[0], 0)]).write();
    assert!(bake(&legacy, None).is_err());
    let mut s = SongM::read(&sample()).unwrap();
    s.tracks[0].autos[0].param = 9999;
    assert!(bake(&s.write(), None).is_err());
    let mut s = SongM::read(&sample()).unwrap();
    let mut t2 = track(&[], 0);
    t2.receives = vec![(1, 0, 1.0)];
    s.tracks[0].receives = vec![(0, 0, 1.0)];
    s.tracks.push(t2);
    assert!(bake(&s.write(), None).is_err());
}
