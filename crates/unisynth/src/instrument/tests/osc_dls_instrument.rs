//! The DLS-instrument sampler source: each note-on picks the region for its key and velocity and plays its wave at the
//! region's root key, fine tune and rate, with its loop; a missing bank, instrument or region is silence.

use super::osc_sample::{chunk_of, sh, with_bank};
use super::*;
use crate::params::{shape as sp, smp_source};

const LEN: usize = 1200;
/// Gain of a default slot at velocity 100 (as in `osc_sample`).
const K: f32 = std::f32::consts::FRAC_1_SQRT_2 * 100.0 / 128.0;

fn riff(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut v = id.to_vec();
    v.extend((data.len() as u32).to_le_bytes());
    v.extend_from_slice(data);
    if data.len() & 1 == 1 {
        v.push(0);
    }
    v
}

fn list(kind: &[u8; 4], parts: &[Vec<u8>]) -> Vec<u8> {
    let mut body = kind.to_vec();
    parts.iter().for_each(|p| body.extend_from_slice(p));
    riff(b"LIST", &body)
}

/// A ramp wave `s[i] = slope · i` with its `wsmp`.
#[derive(Clone, Copy)]
struct W {
    slope: i16,
    rate: u32,
    unity: u16,
    fine: i16,
    gain: i32,
    lp: Option<(u32, u32)>,
}

impl W {
    fn new(slope: i16, unity: u16) -> W {
        W { slope, rate: 44100, unity, fine: 0, gain: 0, lp: None }
    }
}

fn wsmp(w: &W) -> Vec<u8> {
    let u32b = |v: u32| v.to_le_bytes();
    let mut ws = vec![];
    ws.extend(u32b(20));
    ws.extend(w.unity.to_le_bytes());
    ws.extend(w.fine.to_le_bytes());
    ws.extend(w.gain.to_le_bytes());
    ws.extend(u32b(0));
    ws.extend(u32b(w.lp.is_some() as u32));
    if let Some((s, l)) = w.lp {
        ws.extend([u32b(16), u32b(0), u32b(s), u32b(l)].concat());
    }
    riff(b"wsmp", &ws)
}

/// `(key lo, key hi, vel lo, vel hi, wave, wsmp override)`
type R = (u16, u16, u16, u16, usize, Option<W>);
/// `(drums, bank msb, program, regions)`
type I = (bool, u8, u8, Vec<R>);

fn bank(waves: &[W], instruments: &[I]) -> Vec<u8> {
    let u32b = |v: u32| v.to_le_bytes();
    let mut pool = vec![];
    let mut offsets = vec![];
    for w in waves {
        let mut fmt = vec![];
        fmt.extend(1u16.to_le_bytes());
        fmt.extend(1u16.to_le_bytes());
        fmt.extend(u32b(w.rate));
        fmt.extend(u32b(w.rate * 2));
        fmt.extend(2u16.to_le_bytes());
        fmt.extend(16u16.to_le_bytes());
        let data: Vec<u8> = (0..LEN).flat_map(|i| (w.slope.wrapping_mul(i as i16)).to_le_bytes()).collect();
        let l = list(b"wave", &[riff(b"fmt ", &fmt), wsmp(w), riff(b"data", &data)]);
        offsets.push(pool.len() as u32);
        pool.extend(l);
    }
    let mut ptbl = [u32b(8), u32b(waves.len() as u32)].concat();
    offsets.iter().for_each(|o| ptbl.extend(u32b(*o)));
    let ins: Vec<Vec<u8>> = instruments
        .iter()
        .map(|(drums, bank, program, regions)| {
            let rgns: Vec<Vec<u8>> = regions
                .iter()
                .map(|&(klo, khi, vlo, vhi, wave, ov)| {
                    let mut h = vec![];
                    for x in [klo, khi, vlo, vhi, 0, 0] {
                        h.extend(x.to_le_bytes());
                    }
                    let wlnk = [0u16.to_le_bytes().to_vec(), 0u16.to_le_bytes().to_vec(), u32b(1).to_vec(), u32b(wave as u32).to_vec()].concat();
                    let mut parts = vec![riff(b"rgnh", &h), riff(b"wlnk", &wlnk)];
                    if let Some(w) = ov {
                        parts.push(wsmp(&w));
                    }
                    list(b"rgn ", &parts)
                })
                .collect();
            let bankword = (*bank as u32) << 8 | if *drums { 0x8000_0000 } else { 0 };
            let insh = [u32b(regions.len() as u32), u32b(bankword), u32b(*program as u32)].concat();
            list(b"ins ", &[riff(b"insh", &insh), list(b"lrgn", &rgns)])
        })
        .collect();
    let body = [b"DLS ".to_vec(), riff(b"colh", &u32b(instruments.len() as u32)), list(b"lins", &ins), riff(b"ptbl", &ptbl), list(b"wvpl", &[pool])].concat();
    riff(b"RIFF", &body)
}

/// The packed index of an instrument: `1 + drums·16384 + bank·128 + program`.
fn packed(drums: bool, bank: u8, program: u8) -> f32 {
    (1 + if drums { 16384 } else { 0 } + bank as u32 * 128 + program as u32) as f32
}

/// Slot 1 plays instrument `idx` of the bank; starting a quarter in makes the ramp affine in the frame.
fn slot(idx: f32, extra: &[(usize, f32)]) -> Vec<(usize, f32)> {
    let mut e = vec![
        (osc(0, o::TYPE), inv::inv_choice(osc_type::SAMPLER, osc_type::COUNT)),
        (sh(sp::SMP_SOURCE), inv::inv_choice(smp_source::INSTRUMENT, smp_source::COUNT)),
        (pm::smp_index(0), idx),
        (sh(sp::SMP_LOOP_MODE), 0.0),
        (sh(sp::SMP_INTERP), 1.0),
        (sh(sp::SMP_START), 0.25),
    ];
    e.extend_from_slice(extra);
    e
}

fn play_at(edits: &[(usize, f32)], b: Option<&[u8]>, evs: &[Ev], n: usize, rate: f64) -> Vec<f32> {
    with_bank(&chunk_of(edits, &[]), b, rate, |ins| render(ins, rate, evs, n, 4096).0)
}

fn play(edits: &[(usize, f32)], b: &[u8], note: u8, n: usize) -> Vec<f32> {
    play_at(edits, Some(b), &[on(0, note)], n, 44100.0)
}

/// Worst deviation of `out` from `gain · K · slope · pos(t) / 32768`, `pos = start + t · step`, relative to the peak.
fn err(out: &[f32], slope: f32, step: f32, gain: f32, from: usize, to: usize) -> f32 {
    let start = 0.25 * (LEN - 1) as f32;
    let want = |t: usize| gain * K * slope * (start + t as f32 * step) / 32768.0;
    let peak = (from..to).fold(0.0f32, |m, t| m.max(want(t).abs()));
    (from..to).fold(0.0f32, |m, t| m.max((out[t] - want(t)).abs())) / peak.max(1e-9)
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

fn step(note: f32, root: f32) -> f32 {
    2f32.powf((note - root) / 12.0)
}

#[test]
fn a_key_split_plays_the_region_of_each_note_at_its_own_root() {
    // two regions: keys 0..59 → wave A (slope 10, root 48), keys 60..127 → wave B (slope 24, root 72)
    let b = bank(&[W::new(10, 48), W::new(24, 72)], &[(false, 0, 0, vec![(0, 59, 0, 127, 0, None), (60, 127, 0, 127, 1, None)])]);
    let idx = packed(false, 0, 0);
    for (note, slope, root) in [(36u8, 10.0f32, 48.0f32), (59, 10.0, 48.0), (60, 24.0, 72.0), (84, 24.0, 72.0)] {
        let x = play(&slot(idx, &[]), &b, note, 400);
        let e = err(&x, slope, step(note as f32, root), 1.0, 100, 380);
        assert!(e < 5e-4, "note {note}: wave of slope {slope}, root {root}: err {e}");
    }
}

#[test]
fn a_drum_kit_plays_each_drum_at_its_own_pitch_not_one_wave_transposed() {
    // the kit is the percussion bank: key 36 → wave A and key 38 → wave B, each with its own root
    let b = bank(&[W::new(10, 36), W::new(25, 38)], &[(true, 0, 0, vec![(36, 36, 0, 127, 0, None), (38, 38, 0, 127, 1, None)])]);
    let idx = packed(true, 0, 0);
    assert_eq!(idx, 16385.0);
    let (a, c) = (play(&slot(idx, &[]), &b, 36, 400), play(&slot(idx, &[]), &b, 38, 400));
    assert!(err(&a, 10.0, 1.0, 1.0, 100, 380) < 5e-4, "36 plays wave A at its root pitch");
    assert!(err(&c, 25.0, 1.0, 1.0, 100, 380) < 5e-4, "38 plays wave B at its root pitch");
    // a key the kit has no region for is silent; the melodic program 0 of bank 0 is not the kit
    assert!(play(&slot(idx, &[]), &b, 40, 400).iter().all(|v| *v == 0.0));
    assert!(play(&slot(packed(false, 0, 0), &[]), &b, 36, 400).iter().all(|v| *v == 0.0));
}

#[test]
fn velocity_layers_pick_by_the_note_velocity() {
    let b = bank(&[W::new(10, 60), W::new(20, 60)], &[(false, 0, 5, vec![(0, 127, 0, 63, 0, None), (0, 127, 64, 127, 1, None)])]);
    let idx = packed(false, 0, 5);
    let ev = |vel: u8| play_at(&slot(idx, &[(v::VELOCITY, 0.0)]), Some(&b), &[Ev { frame: 0, note: 60, vel }], 400, 44100.0);
    // velocity sensitivity 0: the level does not depend on the velocity, so the slope shows which layer sounded
    let (soft, loud) = (ev(30), ev(100));
    let kv = std::f32::consts::FRAC_1_SQRT_2; // no velocity scaling
    let e = |x: &[f32], slope: f32| {
        let start = 0.25 * (LEN - 1) as f32;
        (100..380).fold(0.0f32, |m, t| m.max((x[t] - kv * slope * (start + t as f32) / 32768.0).abs())) / (kv * slope * (start + 380.0) / 32768.0)
    };
    assert!(e(&soft, 10.0) < 5e-4, "velocity 30 plays the soft layer");
    assert!(e(&loud, 20.0) < 5e-4, "velocity 100 plays the loud layer");
    // the layer is chosen at note-on and kept for the life of the voice
    let both = play_at(&slot(idx, &[(v::VELOCITY, 0.0)]), Some(&b), &[Ev { frame: 0, note: 60, vel: 30 }, Ev { frame: 200, note: 64, vel: 100 }], 400, 44100.0);
    assert!(e(&both[..], 10.0) > 1e-3, "the second note adds a voice");
}

#[test]
fn root_fine_tune_rate_and_gain_come_from_the_wave_and_the_region_overrides_them() {
    // wave: root 60, +100 cents (a semitone up), −6.0206 dB in its wsmp (not applied: banks pair such gains with
    // articulation we do not read), at half the song rate
    let w = W { slope: 10, rate: 22050, unity: 60, fine: 100, gain: (-6.0206 * 65536.0) as i32, lp: None };
    let b = bank(&[w], &[(false, 0, 1, vec![(0, 127, 0, 127, 0, None)])]);
    let x = play(&slot(packed(false, 0, 1), &[]), &b, 60, 300);
    // `fine` is in cents as the wave's tuning offset: the root key is 60 − 1 = 59 semitones, so note 60 sounds a semitone up
    let s = step(60.0, 59.0) * 0.5;
    assert!(err(&x, 10.0, s, 1.0, 100, 280) < 5e-4, "step {s}, the wsmp gain is not applied");
    // a region wsmp replaces the wave's: root 48, no fine tune
    let over = W { unity: 48, fine: 0, gain: 0, ..w };
    let b2 = bank(&[w], &[(false, 0, 1, vec![(0, 127, 0, 127, 0, Some(over))])]);
    let y = play(&slot(packed(false, 0, 1), &[]), &b2, 60, 300);
    assert!(err(&y, 10.0, step(60.0, 48.0) * 0.5, 1.0, 100, 280) < 5e-4);
}

#[test]
fn the_loop_comes_from_the_region_or_the_wave() {
    let lp = Some((800, 300));
    let w = W { lp, ..W::new(10, 60) };
    let b = bank(&[w], &[(false, 0, 0, vec![(0, 127, 0, 127, 0, None)])]);
    let looped = [(sh(sp::SMP_LOOP_MODE), inv::inv_choice(1, 3)), (sh(sp::SMP_START), 0.0)];
    let long = LEN + 2000;
    let x = play(&slot(packed(false, 0, 0), &looped), &b, 60, long);
    assert!(x[LEN + 1500..].iter().any(|v| v.abs() > 0.001), "the wave's loop keeps it sounding past its end");
    let none = play(&slot(packed(false, 0, 0), &[(sh(sp::SMP_START), 0.0)]), &b, 60, long);
    assert!(none[LEN + 5..].iter().all(|v| *v == 0.0), "no loop mode: it ends");
    // a loop in the region's wsmp wins over the wave's: a one-sample-long loop holds one value
    let held = W { lp: Some((900, 1)), ..w };
    let b2 = bank(&[w], &[(false, 0, 0, vec![(0, 127, 0, 127, 0, Some(held))])]);
    let y = play(&slot(packed(false, 0, 0), &looped), &b2, 60, long);
    let tail = &y[LEN + 100..long - 10];
    assert!(tail.iter().all(|v| (*v - tail[0]).abs() < 1e-5) && tail[0] != 0.0, "the region's loop holds a single value");
}

#[test]
fn a_missing_bank_instrument_or_key_is_silence_and_the_rest_of_the_voice_still_sounds() {
    let b = bank(&[W::new(10, 60)], &[(false, 0, 0, vec![(0, 127, 0, 127, 0, None)])]);
    let silent = |x: &[f32]| x.iter().all(|v| *v == 0.0);
    assert!(silent(&play_at(&slot(packed(false, 0, 0), &[]), None, &[on(0, 60)], 300, 44100.0)), "no bank");
    assert!(silent(&play(&slot(packed(false, 16, 0), &[]), &b, 60, 300)), "bank 16 program 0 is not in the bank");
    assert!(silent(&play(&slot(0.0, &[]), &b, 60, 300)), "index 0 = none");
    assert!(silent(&play(&slot(packed(false, 0, 1), &[]), &b, 60, 300)), "program 1 is not in the bank");
    // a bad bank is no bank, and loading does not fail
    assert!(silent(&play(&slot(packed(false, 0, 0), &[]), &b"RIFF....nope"[..], 60, 300)));
    // slot 2 is a phase-mod operator: it sounds although slot 1 has nothing to play
    let mut e = slot(packed(false, 16, 0), &[]);
    e.push((osc(1, o::TYPE), inv::inv_choice(osc_type::PM, osc_type::COUNT)));
    let x = play(&e, &b, 60, 2000);
    assert!(rms(&x[500..]) > 0.01);
}

#[test]
fn banks_and_programs_pick_their_own_instrument() {
    let w = [W::new(8, 60), W::new(12, 60), W::new(16, 60), W::new(20, 60)];
    let one = |drums, bank, program, wave| (drums, bank, program, vec![(0u16, 127u16, 0u16, 127u16, wave, None)]);
    let b = bank(&w, &[one(false, 0, 99, 0), one(false, 3, 99, 1), one(false, 7, 99, 2), one(true, 3, 99, 3)]);
    for (idx, slope) in [(packed(false, 0, 99), 8.0f32), (packed(false, 3, 99), 12.0), (packed(false, 7, 99), 16.0), (packed(true, 3, 99), 20.0)] {
        let x = play(&slot(idx, &[]), &b, 60, 300);
        assert!(err(&x, slope, 1.0, 1.0, 100, 280) < 5e-4, "index {idx}: slope {slope}");
    }
}

#[test]
fn a_fixed_frequency_sampler_scales_by_the_wave_rate_too() {
    // fixed 2 × middle C: the sampler steps by hz/261.6 = 2 units per frame, halved by the wave's 22.05 kHz rate
    let w = W { rate: 22050, ..W::new(10, 60) };
    let b = bank(&[w], &[(false, 0, 0, vec![(0, 127, 0, 127, 0, None)])]);
    let e = [(osc(0, o::TRACKING), 1.0), (osc(0, o::FIXED_HZ), inv::inv_fixed_hz(523.2511))];
    let x = play(&slot(packed(false, 0, 0), &e), &b, 40, 300);
    assert!(err(&x, 10.0, 1.0, 1.0, 100, 280) < 5e-4);
}

#[test]
fn a_chord_gives_each_note_its_own_region_and_the_source_values_are_compatible() {
    let b = bank(&[W::new(10, 36), W::new(24, 84)], &[(false, 0, 0, vec![(0, 59, 0, 127, 0, None), (60, 127, 0, 127, 1, None)])]);
    let idx = packed(false, 0, 0);
    let solo = |note| play(&slot(idx, &[]), &b, note, 400);
    let (lo, hi) = (solo(40), solo(80));
    let both = play_at(&slot(idx, &[]), Some(&b), &[on(0, 40), on(0, 80)], 400, 44100.0);
    let e = (100..380).fold(0.0f32, |m, t| m.max((both[t] - lo[t] - hi[t]).abs()));
    assert!(e < 1e-5, "the chord is the sum of its notes ({e})");
    // 1.0 is still a DLS wave by index (files written before the instrument source existed), 0.0 a GSM sample
    assert_eq!(law::choice(1.0, smp_source::COUNT), smp_source::WAVE);
    assert_eq!(law::choice(0.0, smp_source::COUNT), smp_source::GSM);
    let wave_slot = [
        (osc(0, o::TYPE), inv::inv_choice(osc_type::SAMPLER, osc_type::COUNT)),
        (sh(sp::SMP_SOURCE), 1.0),
        (pm::smp_index(0), 1.0),
        (sh(sp::SMP_LOOP_MODE), 0.0),
        (sh(sp::SMP_INTERP), 1.0),
        (sh(sp::SMP_START), 0.25),
    ];
    let w = play(&wave_slot, &b, 60, 300);
    assert!(rms(&w[100..]) > 0.001, "wave 1 of the bank, not the instrument");
}

#[test]
fn the_instrument_is_a_pool_record_and_the_size_report_covers_it() {
    let b = bank(&[W::new(10, 60)], &[(false, 0, 0, vec![(0, 127, 0, 127, 0, None)])]);
    let with = chunk_of(&slot(packed(false, 0, 0), &[]), &[]);
    let wave = chunk_of(&[(osc(0, o::TYPE), inv::inv_choice(osc_type::SAMPLER, osc_type::COUNT)), (sh(sp::SMP_SOURCE), 1.0), (pm::smp_index(0), 1.0)], &[]);
    let need = UInstrument::need(&with).unwrap();
    assert!(need > UInstrument::need(&wave).unwrap(), "an instrument slot reserves its record");
    // loading consumes no more than reported, at every start alignment
    for skew in 0..16 {
        let env = Env { rate: 44100.0, tempo: 120, bank: Some(&b) };
        let mut buf = vec![0u8; need + 64];
        let mut pool = Pool::new(&mut buf[skew..skew + need]);
        assert!(UInstrument::new(&with, &env, &mut pool, &mut Lcg(7)).is_ok(), "skew {skew}");
    }
}

// ---- the real bank (skipped when this machine has none) ----

fn real_bank() -> Option<Vec<u8>> {
    let from_env = std::env::var("GM_DLS").ok().filter(|p| !p.is_empty());
    let path = from_env.unwrap_or_else(|| "/System/Library/Components/CoreAudio.component/Contents/Resources/gs_instruments.dls".into());
    std::fs::read(path).ok()
}

/// What the instrument must play for `note` at velocity 100 from the real bank, read straight from the bank: the wave's
/// linear interpolation at `start + t · step`.
fn expected(bank: &[u8], drums: bool, program: u8, note: u8, n: usize) -> Option<(Vec<f32>, usize)> {
    let d = dls::Dls::parse(bank).ok()?;
    let ins = d.find(drums, 0, program)?;
    let r = ins.region_for(note, 100)?;
    let (w, ws) = d.sampler(&r)?;
    let step = 2f32.powf((note as f32 - (ws.unity_note as f32 - ws.fine_tune as f32 / 100.0)) / 12.0) * w.rate as f32 / 44100.0;
    let at = |p: f32| {
        let i = p.floor() as usize;
        let f = p - i as f32;
        w.sample(i) * (1.0 - f) + w.sample(i + 1) * f
    };
    Some(((0..n).map(|t| K * at(t as f32 * step)).collect(), r.wave as usize))
}

fn real_check(drums: bool, program: u8, notes: &[u8]) {
    let Some(bank) = real_bank() else {
        eprintln!("no GM.DLS bank: skipped");
        return;
    };
    let mut waves = vec![];
    for &note in notes {
        let (want, wave) = expected(&bank, drums, program, note, 3000).unwrap_or_else(|| panic!("region for note {note}"));
        let edits = slot(packed(drums, 0, program), &[(sh(sp::SMP_START), 0.0)]);
        let got = play_at(&edits, Some(&bank), &[on(0, note)], 3000, 44100.0);
        let peak = want.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
        let e = (100..2900).fold(0.0f32, |m, t| m.max((got[t] - want[t]).abs())) / peak;
        assert!(peak > 1e-4 && e < 2e-3, "note {note} (wave {wave}): err {e}, peak {peak}");
        waves.push(wave);
    }
    waves.dedup();
    assert!(waves.len() > 1, "the notes play different waves: {waves:?}");
}

#[test]
fn piano_1_of_the_real_bank_splits_by_key() {
    real_check(false, 0, &[36, 72]);
}

#[test]
fn the_standard_kit_of_the_real_bank_plays_each_drum_its_own_wave() {
    real_check(true, 0, &[36, 38]);
}

#[test]
fn an_instrument_that_the_real_bank_lacks_is_silent() {
    let Some(bank) = real_bank() else { return };
    // bank 127 program 127 is not in the macOS bank
    assert!(dls::Dls::parse(&bank).unwrap().find(false, 127, 127).is_none());
    assert!(play(&slot(packed(false, 127, 127), &[]), &bank, 60, 300).iter().all(|v| *v == 0.0));
}
