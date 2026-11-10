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

/// Hand-built chunk: pairs (indices in their escaped form) and raw sample entries.
fn raw(version: u8, pairs: &[(usize, f32)], samples: &[Vec<u8>]) -> Vec<u8> {
    let mut c = vec![version];
    push_count(&mut c, pairs.len());
    for &(i, v) in pairs {
        push_index(&mut c, i);
        c.extend(v.to_le_bytes());
    }
    c.push(samples.len() as u8);
    for s in samples {
        c.extend((s.len() as u32).to_le_bytes());
        c.extend(s);
    }
    c
}

/// The values a build can read: all of them, or (no `us-chunk-wide`, no `us-matrix-add`) the one-byte block, as
/// `Chunk::values` stores the rest as zero there.
fn readable(v: [f32; pm::COUNT]) -> Vec<f32> {
    let n = if cfg!(any(feature = "us-chunk-wide", feature = "us-matrix-add")) { pm::COUNT } else { SHORT };
    v[..n].to_vec()
}

fn bad(c: &[u8]) {
    assert_eq!(Chunk::parse(c).err(), Some(LoadError::BadChunk), "{c:?}");
}

/// [`bad`] for a chunk that uses an escape: without `us-chunk-wide` the escape itself is refused first.
fn bad_wide(c: &[u8]) {
    let want = if cfg!(feature = "us-chunk-wide") { LoadError::BadChunk } else { LoadError::UnsupportedModule(crate::module::CHUNK_WIDE) };
    assert_eq!(Chunk::parse(c).err(), Some(want), "{c:?}");
}

#[test]
#[cfg(not(feature = "us-chunk-wide"))]
fn escapes_need_the_wide_module() {
    let hi = with(&[(pm::COUNT - 10, 0.75)]);
    assert_eq!(Chunk::parse(&encode(&hi, &[])).err(), Some(LoadError::UnsupportedModule(crate::module::CHUNK_WIDE)));
    let mut many = defaults();
    many.iter_mut().take(255).for_each(|x| *x = 0.123);
    assert_eq!(Chunk::parse(&encode(&many, &[])).err(), Some(LoadError::UnsupportedModule(crate::module::CHUNK_WIDE)));
    // a chunk without escapes (one-byte indices, fewer than 255 pairs) loads
    assert!(Chunk::parse(&encode(&with(&[(pm::ctl::WHEEL, 0.75)]), &[])).is_ok());
}

#[test]
fn defaults_encode_to_three_bytes() {
    let c = encode(&defaults(), &[]);
    assert_eq!(c, [VERSION, 0, 0]);
    assert_eq!(readable(Chunk::parse(&c).unwrap().values()), readable(defaults()));
}

#[test]
fn sparse_chunk_sets_only_what_differs() {
    let v = with(&[(osc(0, o::TYPE), 0.5), (osc(0, o::GAIN), 0.4)]);
    let c = encode(&v, &[]);
    assert_eq!(c.len(), 3 + 2 * 5);
    let ch = Chunk::parse(&c).unwrap();
    assert_eq!(readable(ch.values()), readable(v));
    let pairs: Vec<_> = ch.pairs().collect();
    assert_eq!(pairs, [(osc(0, o::TYPE), 0.5), (osc(0, o::GAIN), 0.4)]);
    assert_eq!(ch.sample_count(), 0);
}

#[cfg(feature = "us-chunk-wide")]
fn lcg(s: &mut u32) -> f32 {
    *s = s.wrapping_mul(1664525).wrapping_add(1013904223);
    (*s >> 8) as f32 / (1u32 << 24) as f32
}

#[test]
#[cfg(feature = "us-chunk-wide")]
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
#[cfg(feature = "us-chunk-wide")]
fn worst_case_size_is_1751_bytes() {
    let mut v = defaults();
    v.iter_mut().for_each(|x| *x = 0.123); // every value differs from its default
    assert!(v.iter().zip(defaults()).all(|(a, b)| *a != b));
    let c = encode(&v, &[]);
    // version, escaped count (3), 240 one-byte pairs, 91 two-byte pairs, sample count
    assert_eq!(c.len(), 1 + 3 + 5 * SHORT + 6 * (pm::COUNT - SHORT) + 1);
    assert_eq!(c.len(), 1751);
    assert_eq!(readable(Chunk::parse(&c).unwrap().values()), readable(v));
}

#[test]
#[cfg(feature = "us-chunk-wide")]
fn high_indices_take_two_bytes_and_round_trip() {
    let hi = pm::COUNT - 10; // 250
    let v = with(&[(190, 0.3), (hi, 0.75)]);
    let c = encode(&v, &[]);
    assert_eq!(c[..3], [VERSION, 2, 190]);
    assert_eq!(c[7..9], [ESC, (hi - SHORT) as u8], "250 = bank 0, byte 10");
    assert_eq!(c.len(), 1 + 1 + 5 + 6 + 1);
    let ch = Chunk::parse(&c).unwrap();
    assert_eq!(readable(ch.values()), readable(v));
    assert_eq!(ch.pairs().collect::<Vec<_>>(), [(190, 0.3), (hi, 0.75)]);
    // a chunk without escapes (indices below 240, fewer than 255 pairs): one byte per index
    let old = with(&[(osc(0, o::TYPE), 0.5), ((pm::NOISE + pm::n::WIDTH), 0.0)]);
    let mut want = vec![VERSION, 2, osc(0, o::TYPE) as u8];
    want.extend(0.5f32.to_le_bytes());
    want.push((pm::NOISE + pm::n::WIDTH) as u8);
    want.extend(0.0f32.to_le_bytes());
    want.push(0);
    assert_eq!(encode(&old, &[]), want);
}

#[test]
#[cfg(feature = "us-chunk-wide")]
fn escaped_counts_are_canonical_and_escapes_at_the_end_are_errors() {
    // 255 or more pairs need the escaped count; fewer must not use it
    let mut v = defaults();
    v.iter_mut().take(255).for_each(|x| *x = 0.123);
    let c = encode(&v, &[]);
    assert_eq!(c[..4], [VERSION, COUNT_ESC, 255, 0]);
    assert_eq!(readable(Chunk::parse(&c).unwrap().values()), readable(v));
    let mut short = vec![VERSION, COUNT_ESC, 1, 0, 5];
    short.extend(0.5f32.to_le_bytes());
    short.push(0);
    bad(&short);
    // the chunk ends right after a bank prefix, or inside an escaped count
    bad(&[VERSION, 1, ESC]);
    bad(&[VERSION, COUNT_ESC]);
    bad(&[VERSION, COUNT_ESC, 255]);
    // a bank prefix past the parameters
    let mut past = vec![VERSION, 1, ESC + 15, 0];
    past.extend(0.5f32.to_le_bytes());
    past.push(0);
    bad(&past);
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
    // the old layout's versions 1 and 2, and unknown ones: rejected, not misread
    for ver in [0u8, 1, 2, 5, 255] {
        assert_eq!(Chunk::parse(&raw(ver, &[], &[])).err(), Some(LoadError::UnknownVersion), "version {ver}");
    }
    bad(&raw(VERSION_TEXT, &[], &[])); // version 4 needs its text section
    bad_wide(&raw(VERSION, &[(pm::COUNT, 0.5)], &[]));
    assert_eq!(Chunk::parse(&raw(VERSION, &[(pm::COUNT - 1, 0.5)], &[])).is_ok(), cfg!(feature = "us-chunk-wide"), "the last parameter decodes");
    bad(&raw(VERSION, &[(pm::ctl::BEND_RANGE, 1.5)], &[]));
    bad(&raw(VERSION, &[(5, 0.1), (5, 0.9)], &[]));
    bad(&raw(VERSION, &[(9, 0.1), (5, 0.9)], &[]));
    for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0001, -0.0001, 1e30] {
        bad(&raw(VERSION, &[(5, x)], &[]));
    }
    // raw (sample index) range: 0..=65535 on the three sample index values only; shape[8] is normalized
    let idx = pm::smp_index(1);
    assert!(Chunk::parse(&raw(VERSION, &[(idx, 65535.0)], &[])).is_ok());
    bad(&raw(VERSION, &[(idx, 65536.0)], &[]));
    bad(&raw(VERSION, &[(idx, -1.0)], &[]));
    bad(&raw(VERSION, &[(idx, f32::NAN)], &[]));
    bad(&raw(VERSION, &[(osc(1, o::SHAPE + 8), 5.0)], &[]));
    bad(&raw(VERSION, &[(pm::smp_index(0) - 1, 5.0)], &[])); // the last mod slot value
    assert!(Chunk::parse(&raw(VERSION, &[(pm::smp_index(0), 5.0)], &[])).is_ok());
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
    bad_wide(&[VERSION, 255, 0]);
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
    let sampler = pm::osc_type::encode(pm::osc_type::SAMPLER);
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

#[test]
fn speech_text_makes_a_version_2_chunk() {
    let v = with(&[(osc(0, o::TYPE), 0.5)]);
    let text = b"!hEl@U wOrld";
    let c = encode_text(&v, &[], text);
    assert_eq!(c[0], VERSION_TEXT);
    assert_eq!(encode_text(&v, &[], &[]), encode(&v, &[]), "no text: version 1");
    assert_eq!(encode(&v, &[])[0], VERSION);
    if cfg!(feature = "us-speech") {
        let ch = Chunk::parse(&c).unwrap();
        assert_eq!(ch.text(), text);
        assert_eq!(readable(ch.values()), readable(v));
        assert_eq!(Chunk::parse(&encode(&v, &[])).unwrap().text(), b"");
        for n in 0..c.len() {
            assert!(Chunk::parse(&c[..n]).is_err(), "cut at {n}");
        }
        let mut extra = c.clone();
        extra.push(0);
        bad(&extra);
        // an empty text section is not canonical (that chunk is version 1)
        let mut empty = encode(&v, &[]);
        empty[0] = VERSION_TEXT;
        empty.extend([0, 0]);
        bad(&empty);
    } else {
        assert_eq!(Chunk::parse(&c).err(), Some(LoadError::UnsupportedModule(crate::module::SPEECH)));
    }
}

#[test]
#[cfg(all(feature = "us-sample-gsm", feature = "us-speech"))]
fn speech_text_follows_the_samples() {
    let e = entry(VEC_GSM);
    let v = with(&[(osc(0, o::TYPE), 1.0)]);
    let c = encode_text(&v, &[&e, &e], b"abc");
    let ch = Chunk::parse(&c).unwrap();
    assert_eq!((ch.sample_count(), ch.text()), (2, &b"abc"[..]));
    assert_eq!(ch.sample(1).unwrap().blocks, VEC_GSM);
}
