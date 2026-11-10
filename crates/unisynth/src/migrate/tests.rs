use super::*;
use std::vec;

/// `old pairs|v3 pairs` lines, each pair `index:f32 bits (hex)` (`research/unify-v3-layout/crosscheck.py`).
fn check_lines(text: &str) -> usize {
    let parse = |s: &str| -> Vec<(usize, f32)> {
        s.split_whitespace()
            .map(|p| {
                let (i, b) = p.split_once(':').unwrap();
                (i.parse().unwrap(), f32::from_bits(u32::from_str_radix(b, 16).unwrap()))
            })
            .collect()
    };
    let mut n = 0;
    for (k, line) in text.lines().enumerate() {
        let (old, new) = line.split_once('|').unwrap();
        let got = values_of_pairs(&parse(old)).unwrap_or_else(|e| panic!("line {}: {e}", k + 1));
        let got: Vec<(usize, u32)> = (0..pm::COUNT).filter(|&i| got[i].to_bits() != pm::DEFAULTS[i].to_bits()).map(|i| (i, got[i].to_bits())).collect();
        let want: Vec<(usize, u32)> = parse(new).into_iter().map(|(i, v)| (i, v.to_bits())).collect();
        assert_eq!(got, want, "line {}: v3 pairs differ from layout.py ({})", k + 1, old.split_whitespace().count());
        n += 1;
    }
    n
}

#[test]
fn the_migration_matches_the_reference() {
    // one instrument per (format version, feature) of the tracked songs; `layout.py` wrote the expected pairs
    assert!(check_lines(include_str!("crosscheck.txt")) > 100);
}

/// Every instrument of every tracked song (opt-in: `crosscheck.py full <file>`, then `MIGRATE_CROSSCHECK=<file>`).
#[test]
fn every_tracked_instrument_matches_the_reference() {
    let Ok(path) = std::env::var("MIGRATE_CROSSCHECK") else { return };
    let n = check_lines(&std::fs::read_to_string(path).unwrap());
    std::eprintln!("{n} instruments match");
}

#[test]
fn the_v3_layout_is_the_frozen_one() {
    // names and defaults of `research/unify-v3-layout/v3-layout.tsv` (task 6.4). Change add-tf3-import appended the
    // point fields after it and expanded the oscillator-type choice 13 → 25 (every v3 type k at position 2k, the same
    // stored value), which is the one law the file no longer states.
    let tsv = include_str!("../../../../research/unify-v3-layout/v3-layout.tsv");
    let rows: Vec<&str> = tsv.lines().collect();
    assert_eq!(rows.len(), pm::PTS);
    for (i, row) in rows.iter().enumerate() {
        let c: Vec<&str> = row.split('\t').collect();
        assert_eq!(c[0].parse::<usize>().unwrap(), i);
        assert_eq!(pm::name(i), c[1], "index {i}");
        assert!((pm::DEFAULTS[i] - c[3].parse::<f32>().unwrap()).abs() < 1e-5, "default of {}", c[1]);
        if let Some(n) = c[2].strip_prefix("choice:") {
            let n: usize = n.parse().unwrap();
            let expanded = c[1].ends_with(".type") && c[1].starts_with("osc") && n == 13;
            let want = if expanded { pm::osc_type::COUNT } else { n };
            assert_eq!(pm::law_of(i), pm::Law::Choice(want), "law of {}", c[1]);
        }
    }
}

#[test]
fn every_old_name_has_a_v3_index_or_a_value_rule() {
    // `rename::OLD` gives the new name of every old index; a non-empty one is a v3 name
    for (i, (old, new)) in pm::rename::OLD.iter().enumerate() {
        if new.is_empty() {
            continue;
        }
        assert!((0..pm::COUNT).any(|j| pm::name(j) == *new), "old {i} {old} → {new}: no v3 index");
    }
}

#[test]
#[cfg(feature = "us-speech")]
fn an_old_chunk_becomes_v3_with_its_samples_and_text() {
    let mut old = po::DEFAULTS;
    old[po::v::MASTER] = 0.5;
    old[po::osc(0, po::o::TYPE)] = po::osc_type::encode(po::osc_type::BLIT);
    // version 2: no samples, a speech text
    let mut b = vec![2u8, 2, po::v::MASTER as u8];
    b.extend(0.5f32.to_le_bytes());
    b.push(po::osc(0, po::o::TYPE) as u8);
    b.extend(old[po::osc(0, po::o::TYPE)].to_le_bytes());
    b.push(0);
    b.extend(3u16.to_le_bytes());
    b.extend(b"abc");
    let v3 = instrument_chunk(&b).unwrap();
    assert_eq!(v3[0], crate::chunk::VERSION_TEXT);
    let c = crate::chunk::Chunk::parse(&v3).unwrap();
    assert_eq!(c.text(), b"abc");
    let v = c.values();
    assert_eq!(v[pm::v::MASTER], 0.5);
    assert_eq!(pm::osc_type::decode(v[pm::osc(0, pm::o::TYPE)]), pm::osc_type::BLIT);
    // the player rejects the old chunk
    assert_eq!(crate::chunk::Chunk::parse(&b).err(), Some(synth_core::mix::LoadError::UnknownVersion));
}

#[test]
fn a_naive_slot_takes_its_mode_combine_and_law() {
    let mut old = po::DEFAULTS;
    old[po::v2::ERA] = po::kinds::encode(6, 7);
    let naive = |old: &mut [f32; po::COUNT], s: usize, mode: f32, ring: f32| {
        old[po::osc(s, po::o::TYPE)] = po::osc_type::encode(po::osc_type::V2);
        old[po::osc(s, po::o::SHAPE + po::shape::V2_MODE)] = mode / 128.0;
        old[po::osc(s, po::o::SHAPE + po::shape::V2_RING)] = ring / 128.0;
    };
    naive(&mut old, 0, 3.0, 0.0); // sine
    naive(&mut old, 1, 5.0, 0.0); // FM: replace
    naive(&mut old, 2, 1.0, 1.0); // saw, ring: multiply
    let p = values(&old).unwrap();
    let ty = |s: usize| pm::osc_type::decode(p[pm::osc(s, pm::o::TYPE)]);
    let comb = |s: usize| law::choice(p[pm::osc(s, pm::o::COMBINE)], pm::combine::COUNT);
    assert_eq!([ty(0), ty(1), ty(2)], [pm::osc_type::SINE, pm::osc_type::FM, pm::osc_type::NAIVE_SAW]);
    assert_eq!([comb(0), comb(1), comb(2)], [pm::combine::ADD, pm::combine::SET, pm::combine::MUL]);
    assert!((0..3).all(|s| p[pm::osc(s, pm::o::LAW)] == 1.0 && p[pm::osc(s, pm::o::SHAPE)] == 0.0));
    // version 3: no FM generator yet, the slot is off; the quirks of version 3 that the patch reaches
    old[po::v2::ERA] = po::kinds::encode(3, 7);
    let p = values(&old).unwrap();
    assert_eq!(ty(1).min(pm::osc_type::decode(p[pm::osc(1, pm::o::TYPE)])), pm::osc_type::OFF);
    assert_eq!(p[pm::quirk::EXACT_MATH], 1.0, "a sine generator reaches exact_math");
    assert_eq!(p[pm::quirk::INTEGER_FM], 0.0, "no FM generator left");
    assert_eq!(p[pm::quirk::LCG_NOISE], 0.0, "no noise generator");
}

#[test]
fn a_route_on_a_naive_mode_is_an_error() {
    let mut old = po::DEFAULTS;
    old[po::osc(0, po::o::TYPE)] = po::osc_type::encode(po::osc_type::V2);
    old[po::modslot(0, po::m::SOURCE)] = po::kinds::encode(crate::modmx::source::LFO1, crate::modmx::source::COUNT_V1);
    old[po::modslot(0, po::m::TARGET)] = law::host::inv_index(crate::modmx::target::osc(0, crate::modmx::target::OSC_SHAPE));
    let e = values(&old).unwrap_err();
    assert!(e.0.contains("mod1") && e.0.contains("osc1"), "{e}");
}

#[test]
fn a_lane_on_a_split_choice_becomes_one_lane_per_moving_field() {
    // a lane on filter 1's type between the SVF low-pass and the old feedback low-pass: the type stays, form,
    // oversample and law move
    let base = po::DEFAULTS;
    let svf = (po::filter_type::encode(po::filter_type::SVF_LP) * 255.0).round() as u8;
    let v2lp = (po::filter_type::encode(po::filter_type::V2_LP) * 255.0).round() as u8;
    let lanes = lane(&base, po::filt(0, po::f::TYPE), &[(0, svf), (100, v2lp)]).unwrap();
    let names: Vec<std::string::String> = lanes.iter().map(|(i, _)| pm::name(*i)).collect();
    assert_eq!(names, ["filt1.type", "filt1.form", "filt1.oversample", "filt1.law"]);
    for (i, pts) in &lanes {
        if *i != pm::filt(0, pm::f::TYPE) {
            assert_eq!(pts, &vec![(0, 0), (100, 255)], "{}", pm::name(*i));
        }
    }
    // a lane on a plain field keeps its points at the new index
    let l = lane(&base, po::filt(0, po::f::CUTOFF), &[(0, 10), (50, 200)]).unwrap();
    assert_eq!(l, vec![(pm::filt(0, pm::f::CUTOFF), vec![(0, 10), (50, 200)])]);
    // structural ones are errors
    assert!(lane(&base, po::v2::ERA, &[(0, 0)]).is_err());
}

#[test]
fn an_old_effect_chunk_becomes_tagged() {
    use crate::kind;
    // cut EQ at format version 5: the DC filter off in place of `era`
    let old = [old_mode_value(kind::EQ, 2), 0.1, 0.9, law::host::inv_count(5, 0, 6)];
    let mut b: Vec<u8> = old.iter().flat_map(|x| x.to_le_bytes()).collect();
    b.extend(20i32.to_le_bytes());
    let v3 = effect_chunk(kind::EQ, &b).unwrap();
    assert_eq!(&v3[v3.len() - 4..], &FX_TAG);
    assert_eq!(crate::fx::mode_of(kind::EQ, &v3), 2);
    let f = |i: usize| f32::from_le_bytes([v3[4 * i], v3[4 * i + 1], v3[4 * i + 2], v3[4 * i + 3]]);
    assert_eq!((f(1), f(2), f(3)), (0.1, 0.9, 0.0));
    // the comb reverb below version 4: the low cut off, `era` gone
    let old = [old_mode_value(kind::REVERB, 3), 0.5, 0.5, 0.7, 0.4, law::host::inv_count(3, 0, 6)];
    let mut b: Vec<u8> = old.iter().flat_map(|x| x.to_le_bytes()).collect();
    b.extend(28i32.to_le_bytes());
    let v3 = effect_chunk(kind::REVERB, &b).unwrap();
    assert_eq!(v3.len(), 4 * 5 + 4);
    assert_eq!(f32::from_le_bytes(v3[12..16].try_into().unwrap()), 0.0);
    // the player rejects the old chunk
    assert_eq!(crate::fx::UFx::need(kind::EQ, &b, &synth_core::mix::Env { rate: 44100.0, tempo: 120, bank: None }).err(), Some(synth_core::mix::LoadError::UnknownVersion));
}

/// The old layout's stored value of effect mode `m` (the expanded choice).
fn old_mode_value(k: u8, m: usize) -> f32 {
    let (old, later) = (crate::fx::mode_count(k), crate::fx::migrated_total(k) - crate::fx::mode_count(k));
    if later == 0 {
        if old > 1 { m as f32 / (old - 1) as f32 } else { 0.0 }
    } else if old == 1 {
        m as f32 / later as f32
    } else {
        po::kinds::encode(m, old)
    }
}

#[test]
fn a_quirk_the_build_lacks_fails_naming_it() {
    // spec "Quirk disabled": each quirk is its own module; a build without it refuses a patch that sets it
    for q in pm::QUIRK..pm::QUIRK + pm::quirk::LEN {
        let mut p = pm::DEFAULTS;
        p[q] = 1.0;
        let r = crate::instrument::UInstrument::need(&crate::chunk::encode(&p, &[]));
        let id = crate::module::quirk_module(q);
        if crate::module::compiled(id) {
            // built in: the quirk is not what fails (the build may lack the default patch's envelope)
            assert_ne!(r.err(), Some(synth_core::mix::LoadError::UnsupportedModule(id)), "{}", pm::name(q));
        } else {
            assert_eq!(r.err(), Some(synth_core::mix::LoadError::UnsupportedModule(id)), "{}", pm::name(q));
        }
    }
}

#[test]
#[cfg(feature = "us-fx-crusher")]
fn a_truncated_effect_chunk_is_rejected_at_every_byte() {
    // spec "Effect chunk version": cut anywhere, a v3 chunk loses its tag or its whole values; no panic
    let c = crate::fx::encode(&[0.0, 0.5, 0.5, 1.0]);
    let env = synth_core::mix::Env { rate: 44100.0, tempo: 120, bank: None };
    assert!(crate::fx::UFx::need(crate::kind::CRUSHER, &c, &env).is_ok());
    for n in 0..c.len() {
        let r = crate::fx::UFx::need(crate::kind::CRUSHER, &c[..n], &env);
        assert!(matches!(r, Err(synth_core::mix::LoadError::UnknownVersion | synth_core::mix::LoadError::BadChunk)), "{n} bytes: {r:?}");
    }
}

#[test]
fn a_lane_that_reaches_a_quirk_sets_it_on_the_patch() {
    // format version 1: an envelope-law lane from seconds to the old frame mode reaches `coarse_env` (review)
    let mut base = po::DEFAULTS;
    base[po::v2::ERA] = po::kinds::encode(1, 7);
    let frame = (po::env_mode::encode(po::env_mode::V2) * 255.0).round() as u8;
    let lanes = lane(&base, po::env(0, po::e::MODE), &[(0, 0), (100, frame)]).unwrap();
    assert!(lanes.iter().all(|(j, _)| !(pm::QUIRK..pm::QUIRK + pm::quirk::LEN).contains(j)), "no lane on a quirk");
    assert!(lanes.iter().any(|(j, _)| *j == pm::env(0, pm::e::LAW)));
    let q = lane_quirks(&base, po::env(0, po::e::MODE), [0.0, frame as f32 / 255.0].into_iter());
    assert_eq!(q, [pm::quirk::COARSE_ENV]);
}
