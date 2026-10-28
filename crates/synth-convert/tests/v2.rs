//! `synth-convert v2m` (change add-v2-import): song mapping scenarios of the `synth-convert` spec on synthetic `.v2m`
//! files built with the test writer.

use synth_convert::model::SongM;
use synth_convert::v2conv;
use synth_convert::v2m::write::{write, ChanW, SongW};
use synth_convert::v2m::{Patch, PATCH_DEFAULTS};
use unisynth::chunk::Chunk;
use unisynth::modmx::{op, source};
use unisynth::params as pm;

fn patch(mods: &[(u8, u8, u8)]) -> Patch {
    let mut params = PATCH_DEFAULTS;
    params[2] = 3; // osc 1 sine
    Patch { params, mods: mods.to_vec() }
}

fn convert(s: &SongW) -> (SongM, Vec<String>) {
    let c = v2conv::convert(&write(s), v2conv::Opts::default()).unwrap();
    (SongM::read(&c.blob).unwrap(), c.warnings)
}

#[test]
fn each_program_of_a_channel_gets_its_track() {
    // channel 1 plays program 3, then program 5 from tick 960 (bar 3 at 96 ticks per quarter)
    let mut s = SongW::new((0..6).map(|_| patch(&[])).collect());
    s.chans[0] = ChanW {
        notes: vec![(0, 60, 100), (96, 60, 0), (960, 62, 100), (1056, 62, 0)],
        programs: vec![(0, 3), (960, 5)],
        ..Default::default()
    };
    let (m, _) = convert(&s);
    // two instrument tracks, the reverb and delay buses, the master
    assert_eq!(m.tracks.len(), 5);
    let keys = |t: usize| m.lanes[m.tracks[t].lane].iter().filter(|e| e.2.is_some()).map(|e| e.1).collect::<Vec<_>>();
    assert_eq!((keys(0), keys(1)), (vec![60], vec![62]));
    // the instruments differ only in what their patches say: both are V2 voices
    for t in 0..2 {
        let v = Chunk::parse(&m.devices[m.tracks[t].devices[0]].chunk).unwrap().values();
        assert_eq!(unisynth::law::choice(v[pm::v2::VOICE], 2), 1);
        assert_eq!(pm::osc_type::decode(v[pm::osc(0, pm::o::TYPE)]), pm::osc_type::V2);
    }
    // the master sums the instruments and the two buses, then V2's post filters and sum compressor
    let master = m.tracks.last().unwrap();
    assert_eq!(master.receives.len(), 4);
    let kinds: Vec<u8> = master.devices.iter().map(|&d| m.devices[d].kind).collect();
    assert_eq!(kinds, [unisynth::kind::EQ, unisynth::kind::COMPRESSOR]);
}

#[test]
fn more_modulation_entries_than_slots_keep_the_largest() {
    // 17 velocity routes to the pan (V2 destination 0) with growing amounts: the smallest (65) is dropped
    let mods: Vec<(u8, u8, u8)> = (0..17).map(|k| (0u8, 65 + k as u8, 0u8)).collect();
    let mut s = SongW::new(vec![patch(&mods)]);
    s.chans[0] = ChanW { notes: vec![(0, 60, 100), (96, 60, 0)], ..Default::default() };
    let (m, warnings) = convert(&s);
    assert!(warnings.iter().any(|w| w.contains("17 modulation entries")), "{warnings:?}");
    let v = Chunk::parse(&m.devices[m.tracks[0].devices[0]].chunk).unwrap().values();
    let amounts: Vec<f32> = (0..pm::MOD_SLOTS).map(|i| v[pm::modslot(i, pm::m::AMOUNT)]).collect();
    assert_eq!(amounts[0], 66.0 / 128.0, "the first kept entry, in V2's order");
    assert!((0..pm::MOD_SLOTS).all(|i| op::decode(v[pm::modslot(i, pm::m::OPERATION)]) == op::ADD));
    assert!((0..pm::MOD_SLOTS).all(|i| source::decode(v[pm::modslot(i, pm::m::SOURCE)]) == source::VELOCITY));
}

#[test]
fn events_land_on_v2_frames_and_old_versions_use_their_frame() {
    // two patches in use: with one, the patch size cannot tell versions 1–3 apart (V2's own detection has the same
    // limit)
    let mut s = SongW::new(vec![patch(&[]), patch(&[])]);
    s.chans[0] = ChanW { notes: vec![(1, 60, 100), (2, 60, 0)], ..Default::default() };
    s.chans[1] = ChanW { notes: vec![(100, 60, 100), (101, 60, 0)], programs: vec![(0, 1)], ..Default::default() };
    let first = |m: &SongM| m.lanes[m.tracks[0].lane][0].0 as u64;
    let (m6, _) = convert(&s);
    s.version = 1;
    let (m1, _) = convert(&s);
    // tick 1 = 229.6875 frames: the note starts at the next 128-frame (256 before version 2) synth frame
    assert_eq!(first(&m6), 256);
    assert_eq!(first(&m1), 256);
    s.chans[0].notes = vec![(2, 60, 100), (3, 60, 0)];
    let (m1, _) = convert(&s);
    s.version = 6;
    let (m6, _) = convert(&s);
    assert_eq!((first(&m6), first(&m1)), (512, 512));
    s.chans[0].notes = vec![(3, 60, 100), (4, 60, 0)];
    let (m6, _) = convert(&s);
    s.version = 1;
    let (m1, _) = convert(&s);
    assert_eq!((first(&m6), first(&m1)), (768, 768)); // 689 frames: 768 both (6 × 128, 3 × 256)
}
