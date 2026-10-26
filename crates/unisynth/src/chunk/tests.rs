use super::*;
use crate::params::{self as pm, o, osc};
use synth_core::mix::LoadError;

const VEC_GSM: &[u8] = include_bytes!("../../../gsm/testdata/gsm_vectors.gsm");

/// A sample-section entry: GSM chunk header + WAVEFORMATEX + blocks, nothing after.
fn entry(blocks: &[u8]) -> Vec<u8> {
    let mut c = vec![];
    c.extend((blocks.len() as i32).to_le_bytes());
    c.extend(5000i32.to_le_bytes());
    c.extend(0x31u16.to_le_bytes());
    c.extend(1u16.to_le_bytes());
    c.extend(44100u32.to_le_bytes());
    c.extend(8957u32.to_le_bytes());
    c.extend(65u16.to_le_bytes());
    c.extend(0u16.to_le_bytes());
    c.extend(2u16.to_le_bytes());
    c.extend(320u16.to_le_bytes());
    c.extend(blocks);
    c
}

fn defaults() -> [f32; pm::COUNT] {
    pm::DEFAULTS
}

fn with(edits: &[(usize, f32)]) -> [f32; pm::COUNT] {
    let mut v = defaults();
    for &(i, x) in edits {
        v[i] = x;
    }
    v
}

/// Hand-built chunk: pairs and raw sample entries.
fn raw(version: u8, pairs: &[(u8, f32)], samples: &[Vec<u8>]) -> Vec<u8> {
    let mut c = vec![version, pairs.len() as u8];
    for &(i, v) in pairs {
        c.push(i);
        c.extend(v.to_le_bytes());
    }
    c.push(samples.len() as u8);
    for s in samples {
        c.extend((s.len() as u32).to_le_bytes());
        c.extend(s);
    }
    c
}

fn bad(c: &[u8]) {
    assert_eq!(Chunk::parse(c).err(), Some(LoadError::BadChunk), "{c:?}");
}

#[test]
fn defaults_encode_to_three_bytes() {
    let c = encode(&defaults(), &[]);
    assert_eq!(c, [VERSION, 0, 0]);
    assert_eq!(Chunk::parse(&c).unwrap().values(), defaults());
}

#[test]
fn sparse_chunk_sets_only_what_differs() {
    let v = with(&[(osc(0, o::TYPE), 0.5), (osc(0, o::GAIN), 0.4)]);
    let c = encode(&v, &[]);
    assert_eq!(c.len(), 3 + 2 * 5);
    let ch = Chunk::parse(&c).unwrap();
    assert_eq!(ch.values(), v);
    let pairs: Vec<_> = ch.pairs().collect();
    assert_eq!(pairs, [(osc(0, o::TYPE), 0.5), (osc(0, o::GAIN), 0.4)]);
    assert_eq!(ch.sample_count(), 0);
}

fn lcg(s: &mut u32) -> f32 {
    *s = s.wrapping_mul(1664525).wrapping_add(1013904223);
    (*s >> 8) as f32 / (1u32 << 24) as f32
}

#[test]
fn round_trip_is_bit_exact_and_canonical() {
    let mut s = 7;
    for _ in 0..50 {
        let mut v = defaults();
        for x in v.iter_mut() {
            if lcg(&mut s) < 0.4 {
                *x = lcg(&mut s);
            }
        }
        for i in 0..3 {
            v[pm::smp_index(i)] = (lcg(&mut s) * 3.0).floor();
            if v[pm::smp_index(i)] != 0.0 {
                v[osc(i, o::TYPE)] = 0.0; // not a sampler: the index is not checked against the section
            }
        }
        let c = encode(&v, &[]);
        let ch = Chunk::parse(&c).unwrap();
        let back = ch.values();
        assert!(v.iter().zip(&back).all(|(a, b)| a.to_bits() == b.to_bits()));
        assert_eq!(encode(&back, &[]), c, "same patch, same bytes");
        let idx: Vec<_> = ch.pairs().map(|p| p.0).collect();
        assert!(idx.windows(2).all(|w| w[0] < w[1]));
    }
}

#[test]
fn worst_case_size_is_923_bytes() {
    let mut v = defaults();
    v.iter_mut().for_each(|x| *x = 0.123); // every value differs from its default
    assert!(v.iter().zip(defaults()).all(|(a, b)| *a != b));
    let c = encode(&v, &[]);
    assert_eq!(c.len(), 3 + 5 * pm::COUNT);
    assert_eq!(Chunk::parse(&c).unwrap().values(), v);
}

#[test]
#[cfg(feature = "us-sample-gsm")]
fn truncation_at_every_byte_is_an_error() {
    let v = with(&[(osc(0, o::TYPE), 1.0), (osc(1, o::TYPE), 1.0), (pm::smp_index(1), 1.0)]);
    let e = entry(VEC_GSM);
    let c = encode(&v, &[&e]);
    assert!(Chunk::parse(&c).is_ok());
    for n in 0..c.len() {
        assert!(Chunk::parse(&c[..n]).is_err(), "cut at {n} of {}", c.len());
    }
}

#[test]
fn rejects_version_index_order_values_and_trailing_bytes() {
    for ver in [0u8, 2, 255] {
        bad(&raw(ver, &[], &[]));
    }
    bad(&raw(VERSION, &[(pm::COUNT as u8, 0.5)], &[]));
    assert!(Chunk::parse(&raw(VERSION, &[(pm::COUNT as u8 - 1, 0.5)], &[])).is_ok(), "the last controller decodes");
    bad(&raw(VERSION, &[(pm::ctl::BEND_RANGE as u8, 1.5)], &[]));
    bad(&raw(VERSION, &[(255, 0.5)], &[]));
    bad(&raw(VERSION, &[(5, 0.1), (5, 0.9)], &[]));
    bad(&raw(VERSION, &[(9, 0.1), (5, 0.9)], &[]));
    for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0001, -0.0001, 1e30] {
        bad(&raw(VERSION, &[(5, x)], &[]));
    }
    // raw (sample index) range: 0..=65535 on the three sample index values only; shape[8] is normalized
    let idx = pm::smp_index(1) as u8;
    assert!(Chunk::parse(&raw(VERSION, &[(idx, 65535.0)], &[])).is_ok());
    bad(&raw(VERSION, &[(idx, 65536.0)], &[]));
    bad(&raw(VERSION, &[(idx, -1.0)], &[]));
    bad(&raw(VERSION, &[(idx, f32::NAN)], &[]));
    bad(&raw(VERSION, &[(osc(1, o::SHAPE + 8) as u8, 5.0)], &[]));
    bad(&raw(VERSION, &[(pm::smp_index(0) as u8 - 1, 5.0)], &[])); // the last mod slot value
    assert!(Chunk::parse(&raw(VERSION, &[(pm::smp_index(0) as u8, 5.0)], &[])).is_ok());
    let mut ok = raw(VERSION, &[(5, 0.5)], &[]);
    ok.push(0);
    bad(&ok);
    assert!(Chunk::parse(&[]).is_err());
}

#[test]
#[cfg(feature = "us-sample-gsm")]
fn sample_section_entries_are_checked() {
    let e = entry(VEC_GSM);
    let bytes = raw(VERSION, &[], &[e.clone(), e.clone()]);
    let ch = Chunk::parse(&bytes).unwrap();
    assert_eq!(ch.sample_count(), 2);
    for i in 0..2 {
        let g = ch.sample(i).unwrap();
        assert_eq!(g.blocks, VEC_GSM);
        assert!(g.rest.is_empty());
    }
    assert!(ch.sample(2).is_none());
    // size 0, past the end, bad format tag, bytes after the compressed data
    bad(&raw(VERSION, &[], &[vec![]]));
    let mut c = raw(VERSION, &[], &[e.clone()]);
    let size_at = 3; // version, n, s, then the first size
    c[size_at..size_at + 4].copy_from_slice(&(e.len() as u32 + 1).to_le_bytes());
    bad(&c);
    let mut wrong_tag = e.clone();
    wrong_tag[8] = 0x32;
    bad(&raw(VERSION, &[], &[wrong_tag]));
    let mut tail = e.clone();
    tail.extend([1, 2, 3]);
    bad(&raw(VERSION, &[], &[tail]));
    // compressed size larger than the entry
    let mut big = e.clone();
    big[0..4].copy_from_slice(&1_000_000i32.to_le_bytes());
    bad(&raw(VERSION, &[], &[big]));
}

#[test]
fn hostile_sizes_do_not_overflow() {
    let mut c = vec![VERSION, 0, 255];
    for _ in 0..255 {
        c.extend(0xFFFF_FFFFu32.to_le_bytes());
    }
    assert!(Chunk::parse(&c).is_err());
    let mut c = vec![VERSION, 0, 1];
    c.extend((u32::MAX - 2).to_le_bytes());
    c.extend([0u8; 16]);
    assert!(Chunk::parse(&c).is_err());
    // a count that cannot fit
    bad(&[VERSION, 255, 0]);
}

#[test]
#[cfg(feature = "us-sample-gsm")]
fn gsm_sampler_index_must_exist() {
    let e = entry(VEC_GSM);
    let slot = |ty: f32, source: f32, index: f32| {
        with(&[
            (osc(0, o::TYPE), ty),
            (osc(0, o::SHAPE + crate::params::shape::SMP_SOURCE), source),
            (pm::smp_index(0), index),
        ])
    };
    let sampler = 1.0; // choice(5) index 4 = 1.0
    // GSM source (0), index 1 with one sample: ok; index 2: error; index 0 = none: ok
    assert!(Chunk::parse(&encode(&slot(sampler, 0.0, 1.0), &[&e])).is_ok());
    assert!(Chunk::parse(&encode(&slot(sampler, 0.0, 0.0), &[&e])).is_ok());
    assert_eq!(Chunk::parse(&encode(&slot(sampler, 0.0, 2.0), &[&e])).err(), Some(LoadError::BadChunk));
    assert_eq!(Chunk::parse(&encode(&slot(sampler, 0.0, 3.0), &[])).err(), Some(LoadError::BadChunk));
    // fractional index truncates like a C `(int)v`: 1.9 is sample 1
    assert!(Chunk::parse(&encode(&slot(sampler, 0.0, 1.9), &[&e])).is_ok());
    // DLS source (1): the index refers to the bank, not the section
    assert!(Chunk::parse(&encode(&slot(sampler, 1.0, 400.0), &[])).is_ok());
    // a slot that is not a sampler ignores the index
    assert!(Chunk::parse(&encode(&slot(0.5, 0.0, 9.0), &[])).is_ok());
}

#[test]
fn sample_indices_read_like_int_cast() {
    assert_eq!(sample_index(0.0), 0);
    assert_eq!(sample_index(0.99), 0);
    assert_eq!(sample_index(1.0), 1);
    assert_eq!(sample_index(2.7), 2);
    assert_eq!(sample_index(65535.0), 65535);
    assert_eq!(sample_index(-3.0), 0);
    assert_eq!(sample_index(f32::NAN), 0);
}

#[test]
#[cfg(not(feature = "us-sample-gsm"))]
fn samples_need_the_gsm_module() {
    let c = raw(VERSION, &[], &[entry(VEC_GSM)]);
    assert_eq!(Chunk::parse(&c).err(), Some(LoadError::UnsupportedModule(crate::module::SAMPLE_GSM)));
}
