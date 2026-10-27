//! The sampler slot: GM.DLS and GSM sources, pitch, loops, reverse, start, interpolation, one-shot.

use super::*;
use crate::chunk::encode;
use crate::params::shape as sp;
#[cfg(feature = "us-voice-oneshot")]
use synth_core::voice::VoiceState;

const N: usize = 2000;
const VEC_GSM: &[u8] = include_bytes!("../../../../gsm/testdata/gsm_vectors.gsm");

// ---- a tiny DLS bank with one 16-bit ramp wave: s[i] = 10·i ----
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

/// A bank with a ramp wave of `n` samples and an optional loop `(start, length)` in its `wsmp`.
pub(super) fn bank(n: usize, lp: Option<(u32, u32)>) -> Vec<u8> {
    let u32b = |v: u32| v.to_le_bytes();
    let mut fmt = vec![];
    fmt.extend(1u16.to_le_bytes());
    fmt.extend(1u16.to_le_bytes());
    fmt.extend(u32b(44100));
    fmt.extend(u32b(88200));
    fmt.extend(2u16.to_le_bytes());
    fmt.extend(16u16.to_le_bytes());
    let mut ws = vec![];
    ws.extend(u32b(20));
    ws.extend(60u16.to_le_bytes());
    ws.extend(0i16.to_le_bytes());
    ws.extend(0i32.to_le_bytes());
    ws.extend(u32b(0));
    ws.extend(u32b(lp.is_some() as u32));
    if let Some((s, l)) = lp {
        ws.extend([u32b(16), u32b(0), u32b(s), u32b(l)].concat());
    }
    let data: Vec<u8> = (0..n).flat_map(|i| ((i * 10) as i16).to_le_bytes()).collect();
    let wave = list(b"wave", &[riff(b"fmt ", &fmt), riff(b"wsmp", &ws), riff(b"data", &data)]);
    let ptbl = riff(b"ptbl", &[u32b(8), u32b(1), u32b(0)].concat());
    let body = [b"DLS ".to_vec(), riff(b"colh", &u32b(0)), list(b"lins", &[]), ptbl, list(b"wvpl", &[wave])].concat();
    riff(b"RIFF", &body)
}

fn dls_value(i: usize) -> f32 {
    (i * 10) as f32 / 32768.0
}

// ---- a GSM sample-section entry (the libgsm test vectors) ----
pub(super) fn gsm_entry() -> Vec<u8> {
    let mut c = vec![];
    c.extend((VEC_GSM.len() as i32).to_le_bytes());
    c.extend(5120i32.to_le_bytes());
    c.extend(0x31u16.to_le_bytes());
    c.extend(1u16.to_le_bytes());
    c.extend(44100u32.to_le_bytes());
    c.extend(8957u32.to_le_bytes());
    c.extend(65u16.to_le_bytes());
    c.extend(0u16.to_le_bytes());
    c.extend(2u16.to_le_bytes());
    c.extend(320u16.to_le_bytes());
    c.extend(VEC_GSM);
    c
}

fn gsm_pcm() -> Vec<f32> {
    let mut out = vec![0.0f32; 16 * 320];
    let n = gsm::decode(VEC_GSM, &mut out).unwrap();
    out.truncate(n);
    out
}

pub(super) fn sh(k: usize) -> usize {
    osc(0, o::SHAPE + k)
}

/// Slot 1 = a DLS sampler playing wave 1, no loop, linear interpolation, from the start.
pub(super) fn dls_slot(extra: &[(usize, f32)]) -> Vec<(usize, f32)> {
    let mut e = vec![
        (osc(0, o::TYPE), inv::inv_choice(osc_type::SAMPLER, osc_type::COUNT)),
        (sh(sp::SMP_SOURCE), 1.0),
        (pm::smp_index(0), 1.0),
        (sh(sp::SMP_LOOP_MODE), 0.0),
        (sh(sp::SMP_INTERP), 1.0),
    ];
    e.extend_from_slice(extra);
    e
}

pub(super) fn chunk_of(edits: &[(usize, f32)], samples: &[&[u8]]) -> Vec<u8> {
    let mut v = pm::DEFAULTS;
    for &(i, x) in edits {
        v[i] = x;
    }
    encode(&v, samples)
}

pub(super) fn with_bank<R>(chunk: &[u8], bank: Option<&[u8]>, rate: f64, f: impl FnOnce(&mut UInstrument) -> R) -> R {
    let env = Env { rate, tempo: 120, bank: unsafe { std::mem::transmute::<Option<&[u8]>, Option<&[u8]>>(bank) } };
    let mut mem = vec![0u8; UInstrument::need(chunk).unwrap()];
    let mut pool = Pool::new(&mut mem);
    let mut ins = UInstrument::new(chunk, &env, &mut pool, &mut Lcg(7)).unwrap();
    f(&mut ins)
}

fn play(edits: &[(usize, f32)], bank: &[u8], note: u8, n: usize) -> Vec<f32> {
    with_bank(&chunk_of(edits, &[]), Some(bank), 44100.0, |ins| render(ins, 44100.0, &[on(0, note)], n, 4096).0)
}

/// `out ≈ k·want` over `from..to`: returns the worst deviation relative to the largest expected value.
fn fit(out: &[f32], want: impl Fn(usize) -> f32, from: usize, to: usize) -> (f32, f32) {
    let (mut oe, mut ee) = (0.0f64, 0.0f64);
    for t in from..to {
        oe += out[t] as f64 * want(t) as f64;
        ee += (want(t) as f64).powi(2);
    }
    let k = if ee > 0.0 { (oe / ee) as f32 } else { 0.0 };
    let peak = (from..to).fold(0.0f32, |m, t| m.max(want(t).abs()));
    let worst = (from..to).fold(0.0f32, |m, t| m.max((out[t] - k * want(t)).abs()));
    (k, worst / peak.max(1e-9))
}

const FROM: usize = 100;

/// The gain of a default slot at note velocity 100: centre pan (1/√2) × velocity (100/128) × envelope 1 (1 once
/// the attack is over). Fixing it keeps a pitch error from hiding in a fitted gain.
const K: f32 = std::f32::consts::FRAC_1_SQRT_2 * 100.0 / 128.0;

/// Worst deviation of `out` from `K·want` over `from..to`, relative to the largest expected value.
fn fit_k(out: &[f32], want: impl Fn(usize) -> f32, from: usize, to: usize) -> f32 {
    let peak = (from..to).fold(0.0f32, |m, t| m.max((K * want(t)).abs()));
    (from..to).fold(0.0f32, |m, t| m.max((out[t] - K * want(t)).abs())) / peak.max(1e-9)
}

#[test]
fn a_wave_plays_at_unity_pitch_at_its_root_note() {
    let x = play(&dls_slot(&[]), &bank(N, None), 60, N + 50);
    let (k, err) = fit(&x, |t| dls_value(t), FROM, N - 1);
    assert!((k - K).abs() < 2e-3 && err < 1e-3, "k {k} (expected {K}), err {err}");
    assert!(x[N + 5..].iter().all(|v| *v == 0.0), "silent after the wave");
}

#[test]
fn the_pitch_steps_through_the_data_by_two_to_the_note_over_twelve() {
    let b = bank(N, None);
    for (note, ratio) in [(72u8, 2.0f32), (48, 0.5), (67, 1.498_307), (53, 0.667_419_9)] {
        // starting a quarter in makes the ramp affine in t (a ramp from 0 would hide the speed in the fitted gain):
        // linear interpolation of a ramp is exact, so the value at frame t is the ramp at 499.75 + t·ratio
        let x = play(&dls_slot(&[(sh(sp::SMP_START), 0.25)]), &b, note, 1200);
        let to = (((N as f32 - 500.0) / ratio) as usize - 5).min(1100);
        let err = fit_k(&x, |t| (499.75 + t as f32 * ratio) * 10.0 / 32768.0, FROM, to);
        assert!(err < 4e-4, "note {note}: err {err}");
    }
}

#[cfg(feature = "us-matrix")]
#[test]
fn coarse_fine_octave_and_the_ratio_shift_the_pitch() {
    let b = bank(N, None);
    let at = |extra: &[(usize, f32)], ratio: f32| {
        let mut e = vec![(sh(sp::SMP_START), 0.25)];
        e.extend_from_slice(extra);
        let x = play(&dls_slot(&e), &b, 60, 1000);
        fit_k(&x, |t| (499.75 + t as f32 * ratio) * 10.0 / 32768.0, FROM, (((N as f32 - 500.0) / ratio) as usize - 5).min(900))
    };
    assert!(at(&[(osc(0, o::COARSE), inv::inv_coarse(12.0))], 2.0) < 4e-4);
    assert!(at(&[(osc(0, o::OCTAVE), inv::inv_octave(-1.0))], 0.5) < 4e-4);
    assert!(at(&[(osc(0, o::RATIO), inv::inv_ratio(2.0))], 2.0) < 4e-4);
    assert!(at(&[(osc(0, o::FINE), inv::inv_fine(1.0))], 1.059_463) < 4e-4);
    // a pitch route scales the step like a frequency: velocity (100/128) × 2
    let e = [(sh(sp::SMP_START), 0.25)];
    let x = play(&dls_slot(&[&e[..], &route(0, source::VELOCITY, target::PITCH, true, 2.0)[..]].concat()), &b, 60, 1000);
    assert!(fit_k(&x, |t| (499.75 + t as f32 * 1.5625) * 10.0 / 32768.0, FROM, 600) < 4e-4);
}

#[test]
fn the_fixed_frequency_mode_is_relative_to_note_60() {
    let b = bank(N, None);
    let x = play(&dls_slot(&[(sh(sp::SMP_START), 0.25), (osc(0, o::TRACKING), 1.0), (osc(0, o::FIXED_HZ), inv::inv_fixed_hz(523.2511))]), &b, 20, 900);
    assert!(fit_k(&x, |t| (499.75 + t as f32 * 2.0) * 10.0 / 32768.0, FROM, 740) < 4e-4, "an octave above middle C steps by 2 whatever the note");
}

#[test]
fn sample_duration_scales_with_the_song_rate() {
    // the position advances per output frame, not per second: the wave lasts N frames at any rate
    for rate in [22050.0, 48000.0] {
        let x = with_bank(&chunk_of(&dls_slot(&[]), &[]), Some(&bank(N, None)), rate, |ins| render(ins, rate, &[on(0, 60)], N + 100, 512).0);
        assert!(x[N - 50] != 0.0 && x[N + 20..].iter().all(|v| *v == 0.0), "{rate}");
    }
}

#[test]
fn nearest_and_linear_interpolation() {
    let b = bank(N, None);
    let lin = play(&dls_slot(&[]), &b, 48, 800);
    let near = play(&dls_slot(&[(sh(sp::SMP_INTERP), 0.0)]), &b, 48, 800);
    // half speed: linear gives the midpoints, nearest repeats each sample twice
    assert!(fit(&lin, |t| t as f32 * 0.5 * 10.0 / 32768.0, FROM, 700).1 < 2e-3);
    let (k, _) = fit(&near, |t| ((t as f32 * 0.5).floor()) * 10.0 / 32768.0, FROM, 700);
    assert!(fit(&near, |t| ((t as f32 * 0.5).floor()) * 10.0 / 32768.0, FROM, 700).1 < 2e-3, "k {k}");
    assert!(lin.iter().zip(&near).any(|(a, b)| (a - b).abs() > 1e-5));
}

/// The indices the original player reads, by an independent walk of its rules (integer steps): `pos` moves by `step`
/// after each read; a repeat loop subtracts the loop length at the end; a ping-pong reflects and repeats the edge.
fn walk(len: i32, start: f64, rev: bool, mode: u8, ls: i32, ll: i32, frames: usize) -> Vec<Option<usize>> {
    let le = (ls + ll).min(len);
    let l = le - ls;
    let (mut pos, mut back) = (start, rev);
    let mut out = vec![];
    for _ in 0..frames {
        if pos < 0.0 || pos >= len as f64 {
            out.push(None);
            continue;
        }
        out.push(Some(pos.floor() as usize));
        pos += if back { -1.0 } else { 1.0 };
        match mode {
            1 => {
                if !back {
                    while pos >= le as f64 {
                        pos -= l as f64;
                    }
                } else {
                    while pos < ls as f64 {
                        pos += l as f64;
                    }
                }
            }
            2 => {
                if !back && pos >= le as f64 {
                    pos = (le - 1) as f64;
                    back = true;
                } else if back && pos < ls as f64 {
                    pos = ls as f64;
                    back = false;
                }
            }
            _ => {}
        }
    }
    out
}

fn expect(w: &[Option<usize>]) -> impl Fn(usize) -> f32 + '_ {
    move |t| w[t].map_or(0.0, dls_value)
}

const NEAREST: (usize, f32) = (sp::SMP_INTERP, 0.0);

fn nearest(extra: &[(usize, f32)]) -> Vec<(usize, f32)> {
    let mut e = vec![(sh(NEAREST.0), NEAREST.1)];
    e.extend_from_slice(extra);
    e
}

#[test]
fn repeat_loops_between_manual_bounds_and_from_the_sample() {
    let b = bank(N, None);
    // manual: [N/4, N/2)
    let e = nearest(&[(sh(sp::SMP_LOOP_MODE), 0.5), (sh(sp::SMP_BOUNDARY), 1.0), (sh(sp::SMP_LOOP_START), 0.25), (sh(sp::SMP_LOOP_LENGTH), 0.25)]);
    let x = play(&dls_slot(&e), &b, 60, 5000);
    let w = walk(N as i32, 0.0, false, 1, 500, 500, 5000);
    assert!(fit(&x, expect(&w), FROM, 5000).1 < 1e-3);
    assert!(x[4000] != 0.0, "still looping");
    // from the sample: the whole wave (no wsmp loop)
    let e = nearest(&[(sh(sp::SMP_LOOP_MODE), 0.5)]);
    let x = play(&dls_slot(&e), &b, 60, 5000);
    assert!(fit(&x, expect(&walk(N as i32, 0.0, false, 1, 0, N as i32, 5000)), FROM, 5000).1 < 1e-3);
    // from the sample: the wave's own loop (its wsmp)
    let looped = bank(N, Some((600, 300)));
    let x = play(&dls_slot(&e), &looped, 60, 5000);
    assert!(fit(&x, expect(&walk(N as i32, 0.0, false, 1, 600, 300, 5000)), FROM, 5000).1 < 1e-3);
}

#[test]
fn ping_pong_reflects_and_repeats_the_edges() {
    let b = bank(N, None);
    let e = nearest(&[(sh(sp::SMP_LOOP_MODE), 1.0), (sh(sp::SMP_BOUNDARY), 1.0), (sh(sp::SMP_LOOP_START), 0.25), (sh(sp::SMP_LOOP_LENGTH), 0.25)]);
    let x = play(&dls_slot(&e), &b, 60, 4000);
    let w = walk(N as i32, 0.0, false, 2, 500, 500, 4000);
    assert!(fit(&x, expect(&w), FROM, 4000).1 < 1e-3);
    assert_eq!((w[999], w[1000], w[1001]), (Some(999), Some(999), Some(998)), "the walk itself reflects");
}

#[test]
fn a_reversed_repeat_loop_wraps_at_its_start() {
    let b = bank(N, None);
    let e = nearest(&[(sh(sp::SMP_LOOP_MODE), 0.5), (sh(sp::SMP_REVERSE), 1.0), (sh(sp::SMP_BOUNDARY), 1.0), (sh(sp::SMP_LOOP_START), 0.25), (sh(sp::SMP_LOOP_LENGTH), 0.25)]);
    let x = play(&dls_slot(&e), &b, 60, 4000);
    let w = walk(N as i32, (N - 1) as f64, true, 1, 500, 500, 4000);
    assert!(fit(&x, expect(&w), FROM, 4000).1 < 1e-3);
    assert!(w.iter().skip(1500).all(|i| i.is_some_and(|i| (500..1000).contains(&i))), "it ends up looping inside the bounds");
}

#[test]
fn reverse_and_start_position() {
    let b = bank(N, None);
    let rev = nearest(&[(sh(sp::SMP_REVERSE), 1.0)]);
    let x = play(&dls_slot(&rev), &b, 60, N + 50);
    assert!(fit(&x, expect(&walk(N as i32, (N - 1) as f64, true, 0, 0, N as i32, N + 50)), FROM, N + 50).1 < 1e-3);
    assert!(x[N + 5..].iter().all(|v| *v == 0.0));
    let half = nearest(&[(sh(sp::SMP_START), 0.5)]);
    let x = play(&dls_slot(&half), &b, 60, 1200);
    assert!(fit(&x, expect(&walk(N as i32, 0.5 * (N - 1) as f64, false, 0, 0, N as i32, 1200)), FROM, 1200).1 < 1e-3);
    let both = nearest(&[(sh(sp::SMP_START), 0.25), (sh(sp::SMP_REVERSE), 1.0)]);
    let x = play(&dls_slot(&both), &b, 60, 1700);
    assert!(fit(&x, expect(&walk(N as i32, 0.75 * (N - 1) as f64, true, 0, 0, N as i32, 1700)), FROM, 1700).1 < 1e-3);
}

#[test]
fn no_bank_or_no_wave_is_silent_and_the_rest_of_the_voice_sounds() {
    let mut e = dls_slot(&[]);
    e.extend([(osc(1, o::TYPE), inv::inv_choice(osc_type::PM, osc_type::COUNT))]);
    let run = |bank: Option<&[u8]>, idx: f32| {
        let mut e = e.clone();
        e.push((pm::smp_index(0), idx));
        with_bank(&chunk_of(&e, &[]), bank, 44100.0, |ins| render(ins, 44100.0, &[on(0, 69)], 4000, 1000).0)
    };
    let b = bank(N, None);
    let sampler_only = |bank: Option<&[u8]>, idx: f32| {
        let mut e = dls_slot(&[]);
        e.push((pm::smp_index(0), idx));
        with_bank(&chunk_of(&e, &[]), bank, 44100.0, |ins| render(ins, 44100.0, &[on(0, 69)], 4000, 1000).0)
    };
    assert!(peak(&sampler_only(None, 1.0)) == 0.0, "no bank");
    assert!(peak(&sampler_only(Some(&b), 0.0)) == 0.0, "index 0 is none");
    assert!(peak(&sampler_only(Some(&b), 2.0)) == 0.0, "an index past the bank");
    assert!(peak(&sampler_only(Some(b"RIFF....nope"), 1.0)) == 0.0, "an unreadable bank");
    assert!(peak(&sampler_only(Some(&b), 1.0)) > 0.01);
    for (bank, idx) in [(None, 1.0), (Some(&b[..]), 0.0)] {
        assert!(peak(&run(bank, idx)) > 0.1, "the phase-mod slot still plays");
    }
}

#[test]
fn a_gsm_sample_plays_as_decoded_and_the_index_picks_the_entry() {
    let pcm = gsm_pcm();
    let e = [
        (osc(0, o::TYPE), inv::inv_choice(osc_type::SAMPLER, osc_type::COUNT)),
        (sh(sp::SMP_SOURCE), 0.0),
        (pm::smp_index(0), 1.0),
        (sh(sp::SMP_LOOP_MODE), 0.0),
        (sh(sp::SMP_INTERP), 1.0),
    ];
    let entry = gsm_entry();
    let x = with_bank(&chunk_of(&e, &[&entry]), None, 44100.0, |ins| render(ins, 44100.0, &[on(0, 60)], pcm.len() + 100, 4096).0);
    let (k, err) = fit(&x, |t| pcm.get(t).copied().unwrap_or(0.0), FROM, pcm.len() - 1);
    assert!(k > 0.3 && err < 1e-3, "k {k}, err {err}");
    assert!(x[pcm.len() + 5..].iter().all(|v| *v == 0.0));
    // index 2 selects the second entry, a different (here: reversed-start) sample: swap the two entries' order
    let mut quiet = entry.clone();
    quiet.truncate(quiet.len() - VEC_GSM.len() + 65 * 2); // two blocks only
    quiet[0..4].copy_from_slice(&(130i32).to_le_bytes());
    let two = with_bank(&chunk_of(&[e[0], e[1], (pm::smp_index(0), 2.0), e[3], e[4]], &[&entry, &quiet]), None, 44100.0, |ins| render(ins, 44100.0, &[on(0, 60)], 1500, 500).0);
    assert!(two[700..].iter().all(|v| *v == 0.0) && peak(&two[..600]) > 0.0, "the second entry is only 640 samples long");
}

#[test]
fn a_decoded_sample_is_shared_by_slots_that_use_it() {
    let entry = gsm_entry();
    let one = [
        (osc(0, o::TYPE), inv::inv_choice(osc_type::SAMPLER, osc_type::COUNT)),
        (sh(sp::SMP_SOURCE), 0.0),
        (pm::smp_index(0), 1.0),
    ];
    let mut two = one.to_vec();
    two.extend([
        (osc(1, o::TYPE), inv::inv_choice(osc_type::SAMPLER, osc_type::COUNT)),
        (osc(1, o::SHAPE + sp::SMP_SOURCE), 0.0),
        (pm::smp_index(1), 1.0),
    ]);
    let base = UInstrument::need(&chunk_of(&[], &[&entry])).unwrap();
    let a = UInstrument::need(&chunk_of(&one, &[&entry])).unwrap();
    let b = UInstrument::need(&chunk_of(&two, &[&entry])).unwrap();
    assert!(a >= base + 4 * 5120, "the decoded samples are in the pool: {a} vs {base}");
    assert_eq!(a, b, "a sample used by two slots is decoded once");
    // the pool `need` reports is exactly enough for it (a second decode would not fit)
    with_bank(&chunk_of(&two, &[&entry]), None, 44100.0, |ins| {
        assert!(peak(&render(ins, 44100.0, &[on(0, 60)], 2000, 500).0) > 0.0);
    });
}

#[cfg(feature = "us-voice-oneshot")]
#[test]
fn a_one_shot_ignores_note_off_and_ends_with_its_sample() {
    let b = bank(N, None);
    let e = dls_slot(&[(pm::v::ONE_SHOT, 1.0)]);
    with_bank(&chunk_of(&e, &[]), Some(&b), 44100.0, |ins| {
        render(ins, 44100.0, &[on(0, 60), off(100, 60)], 1000, 1000);
        assert!(ins.voices().iter().any(|v| v.is_on()), "note-off ignored");
        let x = render_more_out(ins, 1000, 1200);
        assert!(x[..900].iter().any(|v| *v != 0.0), "keeps playing");
        assert!(!ins.voices().iter().any(|v| v.is_on()), "the voice ends when the wave has run out");
    });
    // without one-shot the note-off releases the voice
    with_bank(&chunk_of(&dls_slot(&[]), &[]), Some(&b), 44100.0, |ins| {
        render(ins, 44100.0, &[on(0, 60), off(100, 60)], 100_000, 4096);
        assert!(!ins.voices().iter().any(|v| v.is_on()));
    });
}

#[cfg(feature = "us-voice-oneshot")]
fn render_more_out(ins: &mut UInstrument, from: usize, n: usize) -> Vec<f32> {
    let mut rng = Lcg(1);
    let (mut a, mut b, mut c, mut d) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    let mut ctx = Ctx { rate: 44100.0, tempo: 120, frame: from as u64, rng: &mut rng };
    ins.run(&mut Io { ch: [&mut a, &mut b, &mut c, &mut d] }, n, &mut ctx);
    a
}

#[cfg(feature = "us-voice-oneshot")]
#[test]
fn a_looped_one_shot_keeps_its_voice_until_all_notes_off() {
    let b = bank(N, None);
    let e = dls_slot(&[(pm::v::ONE_SHOT, 1.0), (sh(sp::SMP_LOOP_MODE), 0.5)]);
    with_bank(&chunk_of(&e, &[]), Some(&b), 44100.0, |ins| {
        render(ins, 44100.0, &[on(0, 60), off(100, 60)], 20_000, 1000);
        assert!(ins.voices().iter().any(|v| v.is_on()));
        ins.all_notes_off();
        render_more_out(ins, 20_000, 100_000);
        assert!(!ins.voices().iter().any(|v| v.is_on()));
    });
}

#[test]
fn playback_is_independent_of_the_call_split_and_reset_repeats_it() {
    let b = bank(N, Some((600, 300)));
    let e = dls_slot(&[(sh(sp::SMP_LOOP_MODE), 1.0), (sh(sp::SMP_INTERP), 1.0)]);
    let run = |call: usize| with_bank(&chunk_of(&e, &[]), Some(&b), 44100.0, |ins| render(ins, 44100.0, &[on(0, 64), on(777, 55), off(5000, 64)], 9000, call));
    let whole = run(9000);
    assert!(peak(&whole.0) > 0.01);
    for call in [1, 37, 512] {
        assert_eq!(run(call), whole, "call {call}");
    }
    with_bank(&chunk_of(&e, &[]), Some(&b), 44100.0, |ins| {
        let a = render(ins, 44100.0, &[on(0, 60)], 3000, 500);
        ins.reset();
        assert_eq!(render(ins, 44100.0, &[on(0, 60)], 3000, 500), a);
    });
}

#[cfg(feature = "us-matrix")]
#[test]
fn a_modulated_start_position_is_read_at_note_on() {
    // velocity (100/128) as an Offset on the start value: the wave starts a fraction in
    let t = target::osc(0, target::OSC_SHAPE + sp::SMP_START as u8);
    let e = dls_slot(&nearest(&route(0, source::VELOCITY, t, false, 1.0)));
    let x = play(&e, &bank(N, None), 60, 600);
    let start = 100.0 / 128.0 * (N - 1) as f64;
    let w = walk(N as i32, start, false, 0, 0, N as i32, 600);
    assert!(fit(&x, expect(&w), FROM, 600).1 < 1e-3, "{:?}", &w[..3]);
}

#[test]
fn linear_interpolation_wraps_to_the_loop_start_at_the_seam() {
    // repeat loop [500, 1000) at half speed: the value between samples 999 and the loop's end reads the loop start
    let e = dls_slot(&[(sh(sp::SMP_LOOP_MODE), 0.5), (sh(sp::SMP_BOUNDARY), 1.0), (sh(sp::SMP_LOOP_START), 0.25), (sh(sp::SMP_LOOP_LENGTH), 0.25)]);
    let x = play(&e, &bank(N, None), 48, 2400);
    let k = x[1500] / dls_value(750);
    assert!(k > 0.3);
    // frame 1999 reads position 999.5: half of sample 999, half of sample 500
    let want = 0.5 * (dls_value(999) + dls_value(500));
    assert!((x[1999] / k - want).abs() < 2e-4, "{} vs {want}", x[1999] / k);
    // and one frame later position 500.0 again
    assert!((x[2001] / k - dls_value(500)).abs() < 2e-4, "{} vs {}", x[2001] / k, dls_value(500));
}

#[test]
fn loop_bounds_are_clamped_like_the_original_player() {
    let b = bank(N, None);
    // start 0.75, length 0.5: the end is cut to the sample's end
    let e = nearest(&[(sh(sp::SMP_LOOP_MODE), 0.5), (sh(sp::SMP_BOUNDARY), 1.0), (sh(sp::SMP_LOOP_START), 0.75), (sh(sp::SMP_LOOP_LENGTH), 0.5)]);
    let x = play(&dls_slot(&e), &b, 60, 4000);
    assert!(fit(&x, expect(&walk(N as i32, 0.0, false, 1, 1500, 500, 4000)), FROM, 4000).1 < 1e-3);
    // a loop length of zero is one sample long; a start at the very end is moved inside the sample
    let e = nearest(&[(sh(sp::SMP_LOOP_MODE), 0.5), (sh(sp::SMP_BOUNDARY), 1.0), (sh(sp::SMP_LOOP_START), 0.5), (sh(sp::SMP_LOOP_LENGTH), 0.0)]);
    let x = play(&dls_slot(&e), &b, 60, 3000);
    assert!(fit(&x, expect(&walk(N as i32, 0.0, false, 1, 1000, 1, 3000)), FROM, 3000).1 < 1e-3);
    let e = nearest(&[(sh(sp::SMP_LOOP_MODE), 0.5), (sh(sp::SMP_BOUNDARY), 1.0), (sh(sp::SMP_LOOP_START), 1.0), (sh(sp::SMP_LOOP_LENGTH), 0.5)]);
    let x = play(&dls_slot(&e), &b, 60, 3000);
    assert!(fit(&x, expect(&walk(N as i32, 0.0, false, 1, N as i32 - 1, 1, 3000)), FROM, 3000).1 < 1e-3);
}

#[test]
fn a_sampler_can_phase_modulate_the_next_slot() {
    let level = |send: f32| {
        let mut e = dls_slot(&[(sh(sp::SMP_LOOP_MODE), 0.5), (osc(0, o::MIX), 0.0), (osc(0, o::PM_OUT), inv::inv_pm_out(send))]);
        e.extend([(osc(1, o::TYPE), inv::inv_choice(osc_type::PM, osc_type::COUNT)), (osc(1, o::FINE), inv::inv_fine(12.0 * (441.0f32 / 440.0).log2()))]);
        let x = play(&e, &bank(N, None), 69, 44100);
        // the share of the energy that is not the carrier's 441 Hz sine
        let w = &x[4000..40000];
        let (mut re, mut im, mut energy) = (0.0f64, 0.0f64, 0.0f64);
        for (i, v) in w.iter().enumerate() {
            let a = 2.0 * std::f64::consts::PI * 441.0 * (4000 + i) as f64 / 44100.0;
            re += *v as f64 * a.cos();
            im += *v as f64 * a.sin();
            energy += (*v as f64).powi(2);
        }
        let carrier = 2.0 * (re * re + im * im) / w.len() as f64;
        1.0 - carrier / energy
    };
    assert!(level(0.0) < 0.01);
    assert!(level(3.0) > 0.05, "{}", level(3.0));
}
