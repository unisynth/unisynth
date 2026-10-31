//! Memory: `need` is a safe bound, loading consumes no more than it, a pool one byte short of what loading consumes
//! fails cleanly, hostile sizes error without wrapping, and structural parameters cannot be automated into memory
//! the patch was not sized for.

use super::osc_sample::{bank, chunk_of, dls_slot, gsm_entry, sh, with_bank};
use super::*;
use crate::params::{shape as sp, v};

const RATE: f64 = 44100.0;

fn wt(edits: &[(usize, f32)]) -> Vec<u8> {
    patch(&merged(&[vec![(osc(0, o::TYPE), inv::inv_choice(osc_type::WAVETABLE, osc_type::COUNT))], edits.to_vec()]))
}

fn load<'m>(chunk: &'m [u8], mem: &'m mut [u8], bank: Option<&'m [u8]>) -> Result<(UInstrument<'m>, usize), LoadError> {
    let env = Env { rate: RATE, tempo: 120, bank };
    let total = mem.len();
    let mut pool = Pool::new(mem);
    let ins = UInstrument::new(chunk, &env, &mut pool, &mut Lcg(7))?;
    let used = total - pool.remaining();
    Ok((ins, used))
}

/// A slice of `len` bytes starting `skew` bytes past a 16-byte boundary.
fn skewed(buf: &mut [u8], skew: usize, len: usize) -> &mut [u8] {
    let start = (buf.as_ptr() as usize).wrapping_neg() & 15;
    &mut buf[start + skew..start + skew + len]
}

fn peak_of(ins: &mut UInstrument) -> f32 {
    peak(&render(ins, RATE, &[on(0, 60)], 3000, 512).0)
}

/// `(name, chunk, bank)` for a spread of memory shapes.
fn patches() -> Vec<(&'static str, Vec<u8>, Option<Vec<u8>>)> {
    let b = bank(3000, Some((100, 1000)));
    let cap = |n| (v::CAPACITY, super::cap(n));
    let gsm = |edits: &[(usize, f32)]| chunk_of(&merged(&[vec![(osc(0, o::TYPE), inv::inv_choice(osc_type::SAMPLER, osc_type::COUNT)), (sh(sp::SMP_SOURCE), 0.0), (pm::smp_index(0), 1.0)], edits.to_vec()]), &[&gsm_entry()]);
    vec![
        ("pm", patch(&[]), None),
        ("pm 64 voices", patch(&[cap(64)]), None),
        ("wavetable", wt(&[]), None),
        ("wavetable 64 voices", wt(&[cap(64)]), None),
        ("three wavetables", patch(&[osc_type_edit(0, osc_type::WAVETABLE), osc_type_edit(1, osc_type::WAVETABLE), osc_type_edit(2, osc_type::WAVETABLE)]), None),
        ("gsm sampler", gsm(&[]), None),
        ("gsm sampler 8 voices", gsm(&[cap(8)]), None),
        ("dls sampler", chunk_of(&dls_slot(&[]), &[]), Some(b)),
    ]
}

fn osc_type_edit(i: usize, ty: usize) -> (usize, f32) {
    (osc(i, o::TYPE), inv::inv_choice(ty, osc_type::COUNT))
}

#[test]
fn need_is_a_safe_bound_at_every_alignment_and_loading_uses_no_more() {
    for (name, chunk, bank) in patches() {
        let need = UInstrument::need(&chunk).unwrap();
        let mut buf = vec![0u8; need + 64];
        for skew in 0..16 {
            let (mut ins, used) = load(&chunk, skewed(&mut buf, skew, need), bank.as_deref())
                .unwrap_or_else(|e| panic!("{name}: skew {skew}: {e:?} with exactly the reported {need} bytes"));
            assert!(used <= need, "{name}: used {used} of {need}");
            assert!(peak_of(&mut ins) > 0.01, "{name}: renders at skew {skew}");
        }
    }
}

#[test]
fn one_byte_below_what_loading_consumes_is_out_of_memory() {
    for (name, chunk, bank) in patches() {
        let need = UInstrument::need(&chunk).unwrap();
        let mut buf = vec![0u8; need + 64];
        let (_, used) = load(&chunk, skewed(&mut buf, 0, need), bank.as_deref()).unwrap();
        // the same alignment, one byte short: a clean error, no panic, no read past the slice
        let short = load(&chunk, skewed(&mut buf, 0, used - 1), bank.as_deref()).err();
        assert_eq!(short, Some(LoadError::OutOfMemory), "{name}: used {used} of {need}");
        assert!(load(&chunk, skewed(&mut buf, 0, used), bank.as_deref()).is_ok(), "{name}");
    }
}

#[test]
fn sizes_are_what_the_slot_types_in_use_require() {
    let pm = UInstrument::need(&patch(&[])).unwrap();
    let table = UInstrument::need(&wt(&[])).unwrap();
    // the tables are the whole difference: one per voice for the wavetable slot, nothing for phase-mod
    let tables = 16 * std::mem::size_of::<crate::wt::Wt>();
    assert!(table - pm >= tables && table - pm < tables + 64, "{} extra bytes for {tables} of tables", table - pm);
    // (×4 since change add-v2-import: a voice also carries the V2 slot, distortion and envelope state; ×3 since
    // add-cauldron-xm-import: a slot also carries the clocked resampler's reconstruction filter)
    assert!(pm * 3 < table, "a phase-mod patch needs {pm} bytes, a wavetable one {table}");
    // a wavetable table per voice and slot: three slots cost about three times one
    let three = UInstrument::need(&patch(&[osc_type_edit(0, osc_type::WAVETABLE), osc_type_edit(1, osc_type::WAVETABLE), osc_type_edit(2, osc_type::WAVETABLE)])).unwrap();
    assert!(three > 2 * table && three < 4 * table, "{three} vs {table}");
    // more voices cost more, linearly enough
    let big = UInstrument::need(&patch(&[(v::CAPACITY, cap(64))])).unwrap();
    assert!(big > 3 * pm && big < 5 * pm * 4, "{big} vs {pm}");
    // a sample is decoded once however many slots play it
    let one = UInstrument::need(&chunk_of(&dls_slot(&[]), &[])).unwrap();
    let gsm_one = chunk_of(&[osc_type_edit(0, osc_type::SAMPLER), (sh(sp::SMP_SOURCE), 0.0), (pm::smp_index(0), 1.0)], &[&gsm_entry()]);
    let gsm_two = chunk_of(
        &[
            osc_type_edit(0, osc_type::SAMPLER),
            (sh(sp::SMP_SOURCE), 0.0),
            (pm::smp_index(0), 1.0),
            osc_type_edit(1, osc_type::SAMPLER),
            (osc(1, o::SHAPE + sp::SMP_SOURCE), 0.0),
            (pm::smp_index(1), 1.0),
        ],
        &[&gsm_entry()],
    );
    let (a, b) = (UInstrument::need(&gsm_one).unwrap(), UInstrument::need(&gsm_two).unwrap());
    assert_eq!(a, b, "two slots, one decoded copy");
    assert!(a > one, "a GSM sample is decoded into the pool, a DLS wave is read in place: {a} vs {one}");
}

#[test]
fn hostile_sample_sizes_are_errors_not_wraparound() {
    let on_values = |extra: &[(usize, f32)]| {
        let mut vals = pm::DEFAULTS;
        vals[osc(0, o::TYPE)] = inv::inv_choice(osc_type::SAMPLER, osc_type::COUNT);
        vals[sh(sp::SMP_SOURCE)] = 0.0;
        vals[pm::smp_index(0)] = 1.0;
        extra.iter().for_each(|&(i, x)| vals[i] = x);
        vals
    };
    let head = encode(&on_values(&[]), &[]);
    // the sample section follows the values: 255 samples of 0xFFFFFFFF bytes each, with no data
    let mut c = head[..head.len() - 1].to_vec();
    c.push(255);
    for _ in 0..255 {
        c.extend(0xFFFF_FFFFu32.to_le_bytes());
    }
    assert_eq!(UInstrument::need(&c).err(), Some(LoadError::BadChunk));
    // one entry that claims nearly 4 GiB
    let mut c = head[..head.len() - 1].to_vec();
    c.push(1);
    c.extend((u32::MAX - 2).to_le_bytes());
    c.extend([0u8; 16]);
    assert_eq!(UInstrument::need(&c).err(), Some(LoadError::BadChunk));
    // a GSM entry whose header declares 0x7FFFFFFF decoded samples but carries one block: sized by the data
    let mut e = gsm_entry();
    e[4..8].copy_from_slice(&0x7FFF_FFFFi32.to_le_bytes());
    let c = chunk_of(&[osc_type_edit(0, osc_type::SAMPLER), (sh(sp::SMP_SOURCE), 0.0), (pm::smp_index(0), 1.0)], &[&e]);
    match UInstrument::need(&c) {
        Ok(n) => assert!(n < 1 << 20, "{n}"),
        Err(e) => assert!(matches!(e, LoadError::BadChunk | LoadError::OutOfMemory), "{e:?}"),
    }
    // the device registry reports the same
    let env = Env { rate: RATE, tempo: 120, bank: None };
    assert!(crate::device::UDevice::need(64, &head[..head.len() - 1], &env).is_err());
}

#[test]
fn automating_a_structural_parameter_cannot_reach_memory_that_was_not_sized() {
    // a phase-mod patch in exactly its own memory; every structural parameter is automated to something larger
    let chunk = patch(&[(v::CAPACITY, cap(2))]);
    let need = UInstrument::need(&chunk).unwrap();
    let mut mem = vec![0u8; need];
    let (mut ins, _) = load(&chunk, &mut mem, None).unwrap();
    let base = render(&mut ins, RATE, &[on(0, 60)], 3000, 512).0;
    ins.reset();
    for ty in [osc_type::WAVETABLE, osc_type::SAMPLER, osc_type::BLIT, osc_type::OFF] {
        ins.set_param(osc(0, o::TYPE), inv::inv_choice(ty, osc_type::COUNT));
    }
    ins.set_param(v::CAPACITY, super::cap(64));
    ins.set_param(sh(sp::SMP_SOURCE), 1.0);
    ins.set_param(pm::smp_index(0), 7.0);
    let after = render(&mut ins, RATE, &[on(0, 60), on(10, 62), on(20, 64)], 3000, 512).0;
    assert!(after.iter().all(|x| x.is_finite()));
    assert_eq!(ins.voices().len(), 2, "the capacity stays what was sized");
    ins.reset();
    assert_eq!(render(&mut ins, RATE, &[on(0, 60)], 3000, 512).0, base, "and the sound is the loaded patch's");
    // a sampler patch whose bank is absent still renders silence rather than reading anything
    let smp = chunk_of(&dls_slot(&[]), &[]);
    with_bank(&smp, None, RATE, |ins| assert_eq!(peak_of(ins), 0.0));
}
