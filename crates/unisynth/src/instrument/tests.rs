use super::*;
use crate::chunk::encode;
use crate::law::host as inv;
use crate::params::{o, osc, osc_type};
use synth_core::device::{Ctx, Device, Io};
use synth_core::mix::Kinds;

struct Lcg(u32);
impl Rng for Lcg {
    fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// Patch with the test oscillator on (slot 1 = phase-mod stub) and `edits` applied.
fn patch(edits: &[(usize, f32)]) -> Vec<u8> {
    let mut vals = pm::DEFAULTS;
    vals[osc(0, o::TYPE)] = inv::inv_choice(osc_type::PM, osc_type::COUNT);
    for &(i, x) in edits {
        vals[i] = x;
    }
    encode(&vals, &[])
}

fn cap(n: u32) -> f32 {
    inv::inv_count(n, 1, 64)
}

fn with_ins<R>(chunk: &[u8], rate: f64, f: impl FnOnce(&mut UInstrument) -> R) -> R {
    let env = Env { rate, tempo: 120, bank: None };
    let mut mem = vec![0u8; UInstrument::need(chunk).unwrap()];
    let mut pool = Pool::new(&mut mem);
    let mut ins = UInstrument::new(chunk, &env, &mut pool, &mut Lcg(7)).unwrap();
    f(&mut ins)
}

/// A scripted note event at an absolute frame.
#[derive(Clone, Copy)]
struct Ev {
    frame: u64,
    note: u8,
    /// 0 = note-off, else the velocity of a note-on
    vel: u8,
}

/// Renders `total` frames in calls of `call` frames, delivering events like the song player (no lookahead).
fn render(ins: &mut UInstrument, rate: f64, evs: &[Ev], total: usize, call: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut l, mut r) = (vec![], vec![]);
    let mut frame = 0usize;
    let mut rng = Lcg(1);
    while frame < total {
        let n = call.min(total - frame);
        for e in evs.iter().filter(|e| (e.frame as usize) >= frame && (e.frame as usize) < frame + n) {
            let off = (e.frame as usize - frame) as u32;
            if e.vel == 0 {
                ins.note_off(e.note, off);
            } else {
                ins.note_on(e.note, e.vel, off);
            }
        }
        let (mut a, mut b, mut c, mut d) = (vec![9.0; n], vec![9.0; n], vec![0.0; n], vec![0.0; n]);
        let mut ctx = Ctx { rate, tempo: 120, frame: frame as u64, rng: &mut rng };
        ins.run(&mut Io { ch: [&mut a, &mut b, &mut c, &mut d] }, n, &mut ctx);
        l.extend(a);
        r.extend(b);
        frame += n;
    }
    (l, r)
}

fn on(frame: u64, note: u8) -> Ev {
    Ev { frame, note, vel: 100 }
}

fn off(frame: u64, note: u8) -> Ev {
    Ev { frame, note, vel: 0 }
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

/// Positive-going zero crossings.
fn crossings(x: &[f32]) -> usize {
    x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
}

// ---- 2.1: allocation, onset, parameters ----

#[test]
fn note_starts_on_its_exact_frame() {
    // the envelope starts from 0 at the note's frame: with the minimum attack the first non-zero sample is the next
    with_ins(&patch(&[]), 44100.0, |ins| {
        let (l, r) = render(ins, 44100.0, &[on(37, 69)], 200, 200);
        assert!(l[..=37].iter().chain(&r[..=37]).all(|&x| x == 0.0), "silent up to and including frame 37");
        assert!(l[38] != 0.0 && r[38] != 0.0, "first sample on frame 38");
    });
    // other offsets, calls and exactly on a control grid frame
    for frame in [64u64, 100, 131, 160, 163] {
        with_ins(&patch(&[]), 44100.0, |ins| {
            let (l, _) = render(ins, 44100.0, &[on(frame, 69)], 300, 100);
            let first = l.iter().position(|&x| x != 0.0).unwrap();
            assert_eq!(first as u64, frame + 1);
        });
    }
}

#[test]
fn capacity_and_stealing() {
    let evs: Vec<Ev> = (0..5).map(|i| on(i as u64 * 40, 60 + i as u8)).collect();
    // steal oldest (default): the 5th note takes the voice of the first
    with_ins(&patch(&[(v::CAPACITY, cap(4))]), 44100.0, |ins| {
        render(ins, 44100.0, &evs, 400, 400);
        let notes: Vec<u8> = ins.voices().iter().map(|v| v.note()).collect();
        assert_eq!(ins.voices().len(), 4);
        assert_eq!(notes, [64, 61, 62, 63]);
        assert!(ins.voices().iter().all(|v| v.is_on()));
    });
    // no stealing: the 5th note is dropped
    with_ins(&patch(&[(v::CAPACITY, cap(4)), (v::STEAL, 0.0)]), 44100.0, |ins| {
        render(ins, 44100.0, &evs, 400, 400);
        let notes: Vec<u8> = ins.voices().iter().map(|v| v.note()).collect();
        assert_eq!(notes, [60, 61, 62, 63]);
    });
    // capacity range
    with_ins(&patch(&[(v::CAPACITY, 1.0)]), 44100.0, |ins| assert_eq!(ins.voices().len(), 64));
    with_ins(&patch(&[(v::CAPACITY, 0.0)]), 44100.0, |ins| assert_eq!(ins.voices().len(), 1));
}

#[cfg(feature = "us-voice-unison")]
#[test]
fn unison_starts_several_voices_with_spread() {
    let edits = [(v::UNISON, inv::inv_count(3, 1, 16)), (v::UNI_DETUNE, 0.5), (v::UNI_SPREAD, 1.0)];
    with_ins(&patch(&edits), 44100.0, |ins| {
        render(ins, 44100.0, &[on(0, 60)], 100, 100);
        assert_eq!(ins.voices().iter().filter(|v| v.is_on()).count(), 3);
        let mut detunes: Vec<f32> = ins.voices().iter().filter(|v| v.is_on()).map(|v| v.detune()).collect();
        detunes.sort_by(f32::total_cmp);
        // detune = f · 0.5 semitones with f = 0, 0.5, 1
        assert_eq!(detunes, [0.0, 0.25, 0.5]);
    });
}

#[cfg(all(feature = "us-voice-glide", feature = "us-voice-mono"))]
#[test]
fn mono_legato_slides_without_retriggering() {
    let slide = inv::inv_time(0.5); // half a second
    let edits = [(v::MODE, 1.0), (v::SLIDE, slide)];
    with_ins(&patch(&edits), 44100.0, |ins| {
        let evs = [on(0, 60), on(1000, 64), off(23_000, 64)];
        render(ins, 44100.0, &evs, 1000, 1000);
        let v = &ins.voices()[0];
        assert_eq!((v.starts, v.note(), v.pitch()), (1, 60, 60.0));
        // second note held: the voice slides up over 0.5 s = 22 050 frames, never restarting
        render_more(ins, &evs, 1000, 11_000);
        let mid = ins.voices()[0].pitch();
        assert!(mid > 61.5 && mid < 62.5, "halfway through the slide: {mid}");
        render_more(ins, &evs, 12_000, 10_000); // frame 22 000: slide still running (ends at 23 050)
        assert_eq!(ins.voices()[0].starts, 1);
        render_more(ins, &evs, 22_000, 2_000); // 64 released at 23 000 → slide back to 60
        let v = &ins.voices()[0];
        assert_eq!((v.starts, v.note()), (1, 60), "back to the earlier held note, same voice");
        assert_eq!(ins.voices().iter().filter(|v| v.is_on()).count(), 1);
    });
}

#[cfg(all(feature = "us-voice-glide", feature = "us-voice-mono", feature = "us-osc-clocked"))]
#[test]
fn a_legato_note_keeps_the_clocked_phase() {
    // a clocked saw in mono-legato mode: a legato note slides on without resetting the phase to -32766
    let edits = [(osc(0, o::TYPE), osc_type::encode(osc_type::CLOCKED)), (v::MODE, 1.0), (v::SLIDE, inv::inv_time(0.5))];
    let run = |evs: &[Ev]| with_ins(&patch(&edits), 44100.0, |ins| render(ins, 44100.0, evs, 1100, 1100).0);
    let held = run(&[on(0, 60)]);
    let legato = run(&[on(0, 60), on(1000, 72)]);
    assert_eq!(held[..1000], legato[..1000]);
    for n in 1000..1008 {
        assert!((held[n] - legato[n]).abs() < 1e-3, "frame {n}: {} vs {}", held[n], legato[n]);
    }
    // the legato note slid: one voice, started once; a note after a release starts a voice again
    let starts = |evs: &[Ev]| with_ins(&patch(&edits), 44100.0, |ins| {
        render(ins, 44100.0, evs, 1100, 1100);
        ins.voices().iter().map(|v| v.starts).sum::<u32>()
    });
    assert_eq!(starts(&[on(0, 60), on(1000, 72)]), 1);
    assert_eq!(starts(&[on(0, 60), off(990, 60), on(1000, 72)]), 2);
}

#[cfg(feature = "us-voice-mono")]
#[test]
fn a_legato_note_keeps_the_clocked_filter_rising() {
    // a rising clocked filter (decay 1.0001 per phrase frame, one phrase frame per output frame): across a legato
    // note its envelope part keeps growing from the first note-on; a note after a release restarts it at the amount
    use crate::params::shape as sp;
    let sh = |i| osc(0, o::SHAPE + i);
    let edits = [
        (osc(0, o::TYPE), osc_type::encode(osc_type::CLOCKED)),
        (v::MODE, 1.0),
        (sh(sp::CLK_F_BASE), 0.001 / 8.0),
        (sh(sp::CLK_F_AMOUNT), 0.01 / 8.0),
        (sh(sp::CLK_F_DECAY), 0.49),
        (sh(sp::CLK_F_DAMP), 0.1),
    ];
    let v1 = |evs: &[Ev]| with_ins(&patch(&edits), 44100.0, |ins| {
        render(ins, 44100.0, evs, 1100, 1100);
        ins.voices().iter().filter(|v| v.starts > 0).map(|v| v.slots[0].clk.v1).collect::<Vec<f64>>()
    });
    let amount = (0.01f32 / 8.0) as f64 * 8.0;
    let d = 1.0 + (0.5 - 0.49f32 as f64) / 100.0;
    let rel = |x: f64, y: f64| (x - y).abs() < 1e-9 * y;
    // legato: one voice, 1100 frames of growth since the first note-on
    let legato = v1(&[on(0, 60), on(1000, 72)]);
    assert!(legato.len() == 1 && rel(legato[0], amount * d.powi(1100)), "{legato:?}");
    // after a release: the new voice restarted at the amount 100 frames ago
    let restarted = v1(&[on(0, 60), off(990, 60), on(1000, 72)]);
    assert!(restarted.iter().any(|&x| rel(x, amount * d.powi(100))), "{restarted:?}");
}

#[cfg(feature = "us-voice-mono")]
#[test]
fn a_clocked_restart_fades_the_cut_note_out() {
    // one mono voice, a note cut by the next one in the same frame (the converter's ping-pong joints): the restart
    // zeroes the filter's output, so the old note dropped to 0 in one frame (a click, multiplied by the chain's comb
    // and doubler). Now the step at the restart is carried and faded out over 1 ms
    use crate::params::shape as sp;
    let sh = |i| osc(0, o::SHAPE + i);
    let edits = [
        (osc(0, o::TYPE), osc_type::encode(osc_type::CLOCKED)),
        (v::MODE, 1.0),
        (v::CAPACITY, cap(1)),
        (sh(sp::CLK_F_BASE), 0.02 / 8.0),
        (sh(sp::CLK_F_DAMP), 0.5),
        (pm::env(0, pm::e::RELEASE), inv::inv_env_ms(200.0)),
    ];
    let (l, _) = with_ins(&patch(&edits), 44100.0, |ins| render(ins, 44100.0, &[on(0, 48), off(1000, 48), on(1000, 48)], 1200, 1200));
    let d = |i: usize| (l[i] - l[i - 1]).abs();
    let smooth = (500..1000).map(d).fold(0.0f32, f32::max);
    assert!(l[999].abs() > 4.0 * smooth, "the old note sounds at the cut: {} vs step {smooth}", l[999]);
    assert!(d(1000) <= 2.0 * smooth, "restart step {} vs the wave's {smooth}", d(1000));
    // a note from silence keeps its instant start: nothing to carry, so the first note is the same with or without
    // the cut note before it (its start, before the restart, is unchanged)
    let (fresh, _) = with_ins(&patch(&edits), 44100.0, |ins| render(ins, 44100.0, &[on(0, 48)], 200, 200));
    assert_eq!(&fresh[..200], &l[..200], "a first note fades nothing in");
}

#[cfg(feature = "us-osc-clocked")]
#[test]
fn transpose_and_bend_resample_the_clocked_phrase() {
    // an octave of transpose (or bend) is a playback twice as fast: the phrase clock doubles with the pitch, so the
    // rising filter envelope (per phrase frame) has grown twice as many frames after 1100 output frames
    use crate::params::{ctl, shape as sp};
    let sh = |i| osc(0, o::SHAPE + i);
    let base = [
        (osc(0, o::TYPE), osc_type::encode(osc_type::CLOCKED)),
        (sh(sp::CLK_F_BASE), 0.001 / 8.0),
        (sh(sp::CLK_F_AMOUNT), 0.01 / 8.0),
        (sh(sp::CLK_F_DECAY), 0.49),
        (sh(sp::CLK_F_DAMP), 0.1),
    ];
    let v1 = |extra: &[(usize, f32)]| {
        let edits: std::vec::Vec<(usize, f32)> = base.iter().chain(extra).copied().collect();
        with_ins(&patch(&edits), 44100.0, |ins| {
            render(ins, 44100.0, &[on(0, 60)], 1100, 1100);
            ins.voices()[0].slots[0].clk.v1
        })
    };
    let amount = (0.01f32 / 8.0) as f64 * 8.0;
    let d = 1.0 + (0.5 - 0.49f32 as f64) / 100.0;
    let rel = |x: f64, y: f64| (x - y).abs() < 1e-6 * y;
    assert!(rel(v1(&[]), amount * d.powi(1100)));
    assert!(rel(v1(&[(v::TRANSPOSE, inv::inv_transpose(12.0))]), amount * d.powi(2200)));
    // bend +12 of a 24 range: the same
    assert!(rel(v1(&[(ctl::BEND_RANGE, inv::inv_bend_range(24.0)), (ctl::BEND, 0.75)]), amount * d.powi(2200)));
    // the envelopes follow the rate too: a 200 ms release after a note-off at 1000 ends twice as soon an octave up
    let tail = |extra: &[(usize, f32)]| {
        let mut edits: std::vec::Vec<(usize, f32)> = base.iter().chain(extra).copied().collect();
        edits.push((crate::params::env(0, crate::params::e::RELEASE), inv::inv_env_ms(200.0)));
        let out = with_ins(&patch(&edits), 44100.0, |ins| render(ins, 44100.0, &[on(0, 60), off(1000, 60)], 20_000, 20_000).0);
        out.iter().rposition(|x| x.abs() > 1e-4).unwrap() as f64 - 1000.0
    };
    let (plain, up) = (tail(&[]), tail(&[(v::TRANSPOSE, inv::inv_transpose(12.0))]));
    assert!((up / plain - 0.5).abs() < 0.05, "release {plain} frames, an octave up {up}");
}

/// Renders `n` frames at absolute frame `from` (no scripted events), returns the output.
fn render_from(ins: &mut UInstrument, from: u64, n: usize) -> (Vec<f32>, Vec<f32>) {
    let mut rng = Lcg(1);
    let (mut a, mut b, mut c, mut d) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    let mut ctx = Ctx { rate: 44100.0, tempo: 120, frame: from, rng: &mut rng };
    ins.run(&mut Io { ch: [&mut a, &mut b, &mut c, &mut d] }, n, &mut ctx);
    (a, b)
}

/// Continues a render started with `render` at frame `from`, for `n` frames.
fn render_more(ins: &mut UInstrument, evs: &[Ev], from: usize, n: usize) {
    let mut rng = Lcg(1);
    for e in evs.iter().filter(|e| (e.frame as usize) >= from && (e.frame as usize) < from + n) {
        let off = (e.frame as usize - from) as u32;
        if e.vel == 0 { ins.note_off(e.note, off) } else { ins.note_on(e.note, e.vel, off) }
    }
    let (mut a, mut b, mut c, mut d) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    let mut ctx = Ctx { rate: 44100.0, tempo: 120, frame: from as u64, rng: &mut rng };
    ins.run(&mut Io { ch: [&mut a, &mut b, &mut c, &mut d] }, n, &mut ctx);
}

#[cfg(feature = "us-voice-glide")]
#[test]
fn glide_approaches_the_new_note_from_the_voice_s_last_pitch() {
    // capacity 1 so the same voice plays both notes; glide time constant 0.1 s
    let edits = [(v::CAPACITY, cap(1)), (v::GLIDE, inv::inv_time(0.1))];
    with_ins(&patch(&edits), 44100.0, |ins| {
        let evs = [on(0, 60), off(3000, 60), on(4000, 72)];
        render(ins, 44100.0, &evs, 4100, 4100);
        let p = ins.voices()[0].pitch();
        assert!(p > 60.0 && p < 72.0, "gliding up from 60: {p}");
        render_more(ins, &evs, 4100, 44_100); // one second later
        assert!((ins.voices()[0].pitch() - 72.0).abs() < 0.01);
    });
}

#[test]
fn pitch_follows_the_note_transpose_and_rate() {
    for (rate, note, transpose, hz) in [(44100.0, 69u8, 0.5f32, 440.0f32), (48000.0, 69, 0.5, 440.0), (44100.0, 69, 1.0, 1760.0)] {
        with_ins(&patch(&[(v::TRANSPOSE, transpose)]), rate, |ins| {
            let (l, _) = render(ins, rate, &[on(0, note)], rate as usize, 4096);
            let c = crossings(&l) as f32;
            assert!((c - hz).abs() <= 2.0, "{rate} Hz, transpose {transpose}: {c} crossings vs {hz}");
        });
    }
}

/// Zero crossings per second of `l[from..from + len]`, as Hz.
fn hz_of(l: &[f32], from: usize, len: usize, rate: f64) -> f64 {
    crossings(&l[from..from + len]) as f64 * rate / len as f64
}

#[test]
fn bend_shifts_every_voice_by_its_range() {
    let r = 44100.0;
    let range2 = inv::inv_bend_range(2.0);
    // a held note: note 69 at 440 Hz, bend to the top while it sounds, range 2 semitones → 493.9 Hz (note 71)
    with_ins(&patch(&[(pm::ctl::BEND_RANGE, range2)]), r, |ins| {
        let (a, _) = render(ins, r, &[on(0, 69)], 22050, 4096);
        assert!((hz_of(&a, 4000, 16000, r) - 440.0).abs() < 2.0, "{}", hz_of(&a, 4000, 16000, r));
        ins.set_param(pm::ctl::BEND, 1.0);
        let (b, _) = render(ins, r, &[], 22050, 4096);
        let want = 440.0 * 2f64.powf(2.0 / 12.0);
        assert!((hz_of(&b, 4000, 16000, r) - want).abs() < 3.0, "{} vs {want}", hz_of(&b, 4000, 16000, r));
        // down by the range, and back to the centre
        ins.set_param(pm::ctl::BEND, 0.0);
        let (c, _) = render(ins, r, &[], 22050, 4096);
        let want = 440.0 * 2f64.powf(-2.0 / 12.0);
        assert!((hz_of(&c, 4000, 16000, r) - want).abs() < 3.0, "{} vs {want}", hz_of(&c, 4000, 16000, r));
        ins.set_param(pm::ctl::BEND, 0.5);
        let (d, _) = render(ins, r, &[], 22050, 4096);
        assert!((hz_of(&d, 4000, 16000, r) - 440.0).abs() < 2.0);
    });
    // the range is a parameter: 24 semitones down from 440 Hz is 110 Hz; range 0 ignores the bend
    with_ins(&patch(&[(pm::ctl::BEND_RANGE, 1.0), (pm::ctl::BEND, 0.0)]), r, |ins| {
        let (a, _) = render(ins, r, &[on(0, 69)], 22050, 4096);
        assert!((hz_of(&a, 4000, 16000, r) - 110.0).abs() < 2.0, "{}", hz_of(&a, 4000, 16000, r));
    });
    with_ins(&patch(&[(pm::ctl::BEND_RANGE, 0.0), (pm::ctl::BEND, 1.0)]), r, |ins| {
        let (a, _) = render(ins, r, &[on(0, 69)], 22050, 4096);
        assert!((hz_of(&a, 4000, 16000, r) - 440.0).abs() < 2.0);
    });
}

#[test]
fn bend_moves_a_released_voice_too_and_the_default_changes_nothing() {
    let r = 44100.0;
    // long release: the voice is still sounding after the note-off
    let long = [(pm::env(0, pm::e::RELEASE), 0.9), (pm::ctl::BEND_RANGE, inv::inv_bend_range(12.0))];
    with_ins(&patch(&long), r, |ins| {
        let (a, _) = render(ins, r, &[on(0, 57), off(2000, 57)], 8000, 1000);
        assert!((hz_of(&a, 3000, 4000, r) - 220.0).abs() < 6.0);
        ins.set_param(pm::ctl::BEND, 1.0);
        let (b, _) = render(ins, r, &[], 12000, 1000);
        assert!((hz_of(&b, 4000, 4000, r) - 440.0).abs() < 12.0, "{}", hz_of(&b, 4000, 4000, r));
    });
    // a patch that never sets the controllers renders exactly as with the explicit rest values
    let plain = with_ins(&patch(&[]), r, |ins| render(ins, r, &[on(0, 60), off(9000, 60)], 12000, 512));
    let rest = with_ins(&patch(&[(pm::ctl::WHEEL, 0.0), (pm::ctl::BEND, 0.5), (pm::ctl::BEND_RANGE, inv::inv_bend_range(2.0))]), r, |ins| {
        render(ins, r, &[on(0, 60), off(9000, 60)], 12000, 512)
    });
    assert_eq!(plain, rest);
    assert_eq!(pm::DEFAULTS[pm::ctl::WHEEL], 0.0);
    assert_eq!(pm::DEFAULTS[pm::ctl::BEND], 0.5);
    assert_eq!(pm::DEFAULTS[pm::ctl::BEND_RANGE], inv::inv_bend_range(2.0));
}

#[cfg(feature = "us-matrix")]
#[test]
fn the_wheel_parameter_is_the_live_wheel_and_a_reset_restores_the_loaded_controllers() {
    let level = |setup: &dyn Fn(&mut UInstrument)| {
        let wheel = route(0, source::WHEEL, target::GAIN, true, 1.0);
        with_ins(&patch(&wheel), 44100.0, |ins| {
            setup(ins);
            let (l, _) = render(ins, 44100.0, &[on(0, 69)], 2000, 2000);
            peak(&l[300..])
        })
    };
    let live = level(&|ins| ins.mod_wheel(0.5));
    let param = level(&|ins| ins.set_param(pm::ctl::WHEEL, 0.5));
    assert_eq!(live, param, "mod_wheel writes the same parameter");
    assert!((param / level(&|ins| ins.set_param(pm::ctl::WHEEL, 1.0)) - 0.5).abs() < 0.02);
    // seek: a reset puts wheel, bend and range back to the loaded values (the chunk's, not the defaults')
    let chunk = patch(&[(pm::ctl::WHEEL, 0.25), (pm::ctl::BEND, 0.75), (pm::ctl::BEND_RANGE, 0.5)]);
    with_ins(&chunk, 44100.0, |ins| {
        ins.set_param(pm::ctl::WHEEL, 1.0);
        ins.set_param(pm::ctl::BEND, 0.0);
        ins.set_param(pm::ctl::BEND_RANGE, 0.0);
        ins.mod_wheel(0.9);
        ins.reset();
        assert_eq!((ins.p[pm::ctl::WHEEL], ins.p[pm::ctl::BEND], ins.p[pm::ctl::BEND_RANGE]), (0.25, 0.75, 0.5));
    });
}

#[test]
fn velocity_sensitivity_scales_the_level() {
    let amp = |sens: f32, vel: u8| {
        with_ins(&patch(&[(v::VELOCITY, sens)]), 44100.0, |ins| {
            let (l, r) = render(ins, 44100.0, &[Ev { frame: 0, note: 69, vel }], 2000, 2000);
            peak(&l).max(peak(&r))
        })
    };
    let (full, half, none) = (amp(1.0, 127), amp(1.0, 64), amp(0.0, 20));
    assert!((half / full - 64.0 / 127.0).abs() < 0.01, "{half} / {full}");
    let (a, b) = (amp(0.0, 20), amp(0.0, 127));
    assert!((a - b).abs() < 1e-6 && none > 0.0, "sensitivity 0 ignores velocity");
}

#[test]
fn master_and_pan() {
    let level = |edits: &[(usize, f32)]| {
        with_ins(&patch(edits), 44100.0, |ins| {
            let (l, r) = render(ins, 44100.0, &[on(0, 69)], 2000, 2000);
            (peak(&l), peak(&r))
        })
    };
    let (l, r) = level(&[]);
    assert!((l - r).abs() < 1e-6, "centred: equal power left = right");
    let expect = 0.5f32.sqrt() * 127.0 / 128.0 * 0.0 + 0.5f32.sqrt() * (1.0 - 1.0 + 100.0 / 128.0);
    assert!((l - expect).abs() < 0.01, "{l} vs {expect}");
    let (l0, r0) = level(&[(v::PAN, 0.0)]);
    assert!(l0 > 0.7 && r0 < 1e-6, "hard left: {l0} {r0}");
    let (lm, _) = level(&[(v::MASTER, inv::inv_gain(2.0))]);
    assert!((lm / l - 2.0).abs() < 0.01, "master gain 2 doubles the level");
}

#[test]
fn structural_and_bad_parameters_are_ignored() {
    with_ins(&patch(&[]), 44100.0, |ins| {
        let before = ins.p;
        ins.set_param(v::CAPACITY, 1.0);
        ins.set_param(osc(0, o::TYPE), 0.0);
        ins.set_param(pm::smp_index(1), 5.0);
        ins.set_param(osc(2, o::SHAPE + pm::shape::SMP_SOURCE), 1.0);
        ins.set_param(v::MASTER, f32::NAN);
        ins.set_param(v::MASTER, f32::INFINITY);
        ins.set_param(COUNT, 0.5);
        ins.set_param(10_000, 0.5);
        assert_eq!(ins.p, before);
        assert_eq!(ins.voices().len(), 16);
        ins.set_param(v::MASTER, 7.0);
        assert_eq!(ins.p[v::MASTER], 1.0, "out of range clamps");
        ins.set_param(v::MASTER, -3.0);
        assert_eq!(ins.p[v::MASTER], 0.0);
        assert_eq!(ins.param_count(), pm::COUNT);
    });
}

#[test]
fn every_oscillator_shape_value_is_automatable_and_the_sample_index_is_not() {
    // shape[8] is the wavetable's layer spread: a normal normalized value that set_param takes (it used to share its
    // index with the raw sampler index and was ignored as structural)
    with_ins(&patch(&[]), 44100.0, |ins| {
        for slot in 0..3 {
            for k in 0..9 {
                let i = osc(slot, o::SHAPE + k);
                if k == pm::shape::SMP_SOURCE {
                    continue; // structural
                }
                ins.set_param(i, 0.29);
                assert_eq!(ins.p[i], 0.29, "osc{} s{k}", slot + 1);
            }
            let i = pm::smp_index(slot);
            ins.set_param(i, 5.0);
            assert_eq!(ins.p[i], 0.0, "the sample index is read from the chunk only");
        }
    });
}

#[test]
fn automation_of_a_structural_parameter_changes_nothing() {
    with_ins(&patch(&[]), 44100.0, |ins| {
        let (a, _) = render(ins, 44100.0, &[on(0, 69)], 500, 500);
        ins.reset();
        ins.set_param(osc(0, o::TYPE), 1.0); // would switch the slot to a sampler
        let (b, _) = render(ins, 44100.0, &[on(0, 69)], 500, 500);
        assert_eq!(a, b);
    });
}

#[test]
fn registry_builds_the_instrument_and_rejects_other_kinds() {
    use crate::device::UDevice;
    let chunk = patch(&[]);
    let env = Env { rate: 44100.0, tempo: 120, bank: None };
    assert!(UDevice::need(64, &chunk, &env).unwrap() > 0);
    assert_eq!(UDevice::need(74, &chunk, &env).err(), Some(LoadError::UnsupportedKind(74)));
    assert_eq!(UDevice::need(3, &chunk, &env).err(), Some(LoadError::UnsupportedKind(3)));
    let mut mem = vec![0u8; 1 << 16];
    let mut pool = Pool::new(&mut mem);
    let d = UDevice::load(64, &chunk, &env, &mut pool, &mut Lcg(1)).unwrap();
    assert_eq!((d.param_count(), d.event_lookahead()), (pm::COUNT, 0));
    assert_eq!(UDevice::need(64, &[1, 0, 0, 7], &env).err(), Some(LoadError::BadChunk)); // a trailing byte
}

// ---- 2.2: the control grid ----

fn song_events() -> Vec<Ev> {
    vec![on(0, 60), on(5, 64), on(31, 67), on(32, 55), off(95, 60), on(131, 72), on(250, 48), off(311, 64), off(333, 55)]
}

#[cfg(feature = "us-voice-unison")]
#[test]
fn output_does_not_depend_on_how_the_calls_are_split() {
    let edits = [(v::UNISON, inv::inv_count(2, 1, 16)), (v::UNI_DETUNE, 0.3), (v::UNI_SPREAD, 1.0), (v::MODE, 0.0)];
    let evs = song_events();
    let whole = with_ins(&patch(&edits), 44100.0, |ins| render(ins, 44100.0, &evs, 1000, 1000));
    assert!(peak(&whole.0) > 0.0);
    for call in [1, 7, 31, 32, 33, 37, 64, 100, 511] {
        let split = with_ins(&patch(&edits), 44100.0, |ins| render(ins, 44100.0, &evs, 1000, call));
        assert!(split == whole, "call size {call}");
    }
}

#[cfg(all(feature = "us-voice-glide", feature = "us-voice-mono"))]
#[test]
fn mono_slides_and_glide_are_split_independent() {
    let edits = [(v::MODE, 1.0), (v::SLIDE, inv::inv_time(0.01))];
    let evs = [on(0, 60), on(100, 67), on(300, 50), off(600, 50), on(900, 48), off(1400, 48), off(1500, 60)];
    let whole = with_ins(&patch(&edits), 44100.0, |ins| render(ins, 44100.0, &evs, 2000, 2000));
    for call in [3, 32, 100, 333] {
        let split = with_ins(&patch(&edits), 44100.0, |ins| render(ins, 44100.0, &evs, 2000, call));
        assert!(split == whole, "slide, call size {call}");
    }
    let edits = [(v::CAPACITY, cap(1)), (v::GLIDE, inv::inv_time(0.02))];
    let evs = [on(0, 60), off(300, 60), on(400, 72), off(900, 72), on(1100, 55)];
    let whole = with_ins(&patch(&edits), 44100.0, |ins| render(ins, 44100.0, &evs, 2000, 2000));
    for call in [5, 32, 64, 401] {
        let split = with_ins(&patch(&edits), 44100.0, |ins| render(ins, 44100.0, &evs, 2000, call));
        assert!(split == whole, "glide, call size {call}");
    }
}

#[test]
fn repeat_render_is_bit_identical() {
    let evs = song_events();
    let a = with_ins(&patch(&[]), 44100.0, |ins| render(ins, 44100.0, &evs, 1000, 100));
    let b = with_ins(&patch(&[]), 44100.0, |ins| render(ins, 44100.0, &evs, 1000, 100));
    assert!(a == b);
}

#[cfg(all(feature = "us-voice-glide", feature = "us-voice-unison"))]
#[test]
fn reset_equals_a_fresh_load() {
    let evs = song_events();
    let edits = [(v::UNISON, inv::inv_count(3, 1, 16)), (v::UNI_DETUNE, 0.4), (v::GLIDE, inv::inv_time(0.05))];
    with_ins(&patch(&edits), 44100.0, |ins| {
        let first = render(ins, 44100.0, &evs, 1000, 100);
        // leave things in a different state: more notes, pending events, a parameter change
        ins.note_on(70, 100, 5);
        render(ins, 44100.0, &[on(0, 40)], 333, 77);
        ins.reset();
        let again = render(ins, 44100.0, &evs, 1000, 100);
        assert!(first == again, "reset must return to the freshly loaded state");
    });
}

#[test]
fn voice_ages_tick_on_the_grid() {
    // capacity 2, release 1 ms. Voice 0: 60 @0, released @20 (free by frame 64), then 62 @100 (young). Voice 1: 61
    // @10 (old). The steal at @200 takes the oldest voice, which is voice 1 although voice 0 has the lower index
    let edits = [(v::CAPACITY, cap(2)), (pm::env(0, pm::e::RELEASE), 0.0)];
    with_ins(&patch(&edits), 44100.0, |ins| {
        let evs = [on(0, 60), on(10, 61), off(20, 60), on(100, 62), on(200, 63)];
        render(ins, 44100.0, &evs, 300, 300);
        let notes: Vec<u8> = ins.voices().iter().map(|v| v.note()).collect();
        assert_eq!(notes, [62, 63]);
    });
}

#[cfg(all(feature = "us-voice-glide", feature = "us-voice-mono"))]
#[test]
fn an_event_inside_a_moving_segment_splits_it_exactly() {
    // mono slide over 441 frames: 60 → 72 from frame 100, then (frame 200, inside a control segment, pitch moving)
    // a third note: the new slide starts from the pitch at frame 200, not from the segment start
    let edits = [(v::MODE, 1.0), (v::SLIDE, inv::inv_time(0.01))];
    with_ins(&patch(&edits), 44100.0, |ins| {
        let evs = [on(0, 60), on(100, 72), on(200, 48)];
        render(ins, 44100.0, &evs, 210, 210); // the voice's fine segments are [200, 208) and [208, 216)
        let at_200 = 60.0 + 12.0 * 100.0 / 441.0;
        let at_216 = at_200 - (at_200 - 48.0) * 16.0 / 441.0;
        let got = ins.voices()[0].pitch();
        assert!((got - at_216).abs() < 0.01, "{got} vs {at_216}");
    });
}

#[cfg(feature = "us-voice-glide")]
#[test]
fn a_voice_started_inside_a_block_has_a_short_first_segment() {
    // capacity 1, glide τ = 882 frames; the second note starts at frame 405 (phase 21), so its first segment is 11
    // frames long and ends on the grid at 416
    let edits = [(v::CAPACITY, cap(1)), (v::GLIDE, inv::inv_time(0.02))];
    with_ins(&patch(&edits), 44100.0, |ins| {
        let evs = [on(0, 60), off(300, 60), on(405, 72)];
        render(ins, 44100.0, &evs, 410, 410);
        let want = 72.0 - 12.0 * (-11.0f32 / 882.0).exp();
        let got = ins.voices()[0].pitch();
        assert!((got - want).abs() < 0.01, "{got} vs {want}");
    });
}

#[test]
fn all_notes_off_and_events_past_the_call_are_kept() {
    with_ins(&patch(&[]), 44100.0, |ins| {
        render(ins, 44100.0, &[on(0, 60), on(0, 64)], 100, 100);
        assert_eq!(ins.voices().iter().filter(|v| v.is_on()).count(), 2);
        ins.all_notes_off(); // releases (default release ≈ 51 ms), then the voices free themselves
        render(ins, 44100.0, &[], 3000, 3000);
        assert_eq!(ins.voices().iter().filter(|v| v.is_on()).count(), 0);
        // an event beyond the call stays queued and fires in the next call at its offset
        ins.note_on(69, 100, 150);
        let (l, _) = render_from(ins, 3000, 100);
        assert!(l.iter().all(|&x| x == 0.0));
        let (l2, _) = render_from(ins, 3100, 100);
        let first = l2.iter().position(|&x| x != 0.0);
        assert_eq!(first, Some(51), "offset 150 → frame 50 of the second call, audible from the next frame");
    });
}

#[cfg(all(feature = "us-voice-glide", feature = "us-voice-mono"))]
#[test]
fn control_grid_runs_on_absolute_frames() {
    // a voice started at frame 5 has a first segment of 27 frames; its slide must reach the same pitch regardless
    // of which call the grid frames fall in (covered above) and the glide target at frame 32k is exact
    let edits = [(v::MODE, 1.0), (v::SLIDE, inv::inv_time(0.0008))]; // ≈ 35 frames
    with_ins(&patch(&edits), 44100.0, |ins| {
        let evs = [on(0, 60), on(5, 72)];
        render(ins, 44100.0, &evs, 31, 31);
        let mid = ins.voices()[0].pitch();
        assert!(mid > 60.0, "sliding at the end of the first segment: {mid}");
        render_more(ins, &evs, 31, 100);
        assert_eq!(ins.voices()[0].pitch(), 72.0);
    });
}

// ---- through the song player ----

#[cfg(all(feature = "us-voice-glide", feature = "us-voice-unison"))]
#[test]
fn song_output_is_independent_of_sub_block_and_caller_buffer_size() {
    use crate::testing::{render as play, song, with_player, Track};
    let edits = [(v::UNISON, inv::inv_count(2, 1, 16)), (v::UNI_DETUNE, 0.3), (v::GLIDE, inv::inv_time(0.01))];
    let lane = vec![(0, 60, Some(100)), (5, 64, Some(90)), (700, 60, None), (1, 67, Some(80)), (999, 64, None)];
    let bytes = song(44100, 0.2, &[(64, patch(&edits))], &[lane], &[Track::new(&[0], 0)]);
    let reference = with_player(&bytes, 100, |p| play(p, 6000, 6000));
    assert!(peak(&reference) > 0.0);
    for (sub, call) in [(100, 64), (100, 44_100), (37, 6000), (37, 13), (1, 6000), (512, 100)] {
        let got = with_player(&bytes, sub, |p| play(p, 6000, call));
        assert!(got == reference, "sub-block {sub}, caller buffer {call}");
    }
}

#[cfg(feature = "us-voice-unison")]
#[test]
fn seek_restores_the_loaded_state() {
    use crate::testing::{render as play, song, with_player, Track};
    let lane = vec![(0, 60, Some(100)), (1000, 64, Some(100)), (900, 60, None)];
    let bytes = song(44100, 0.2, &[(64, patch(&[(v::UNISON, inv::inv_count(3, 1, 16))]))], &[lane], &[Track::new(&[0], 0)]);
    with_player(&bytes, 100, |p| {
        let a = play(p, 5000, 5000);
        p.seek(0);
        let b = play(p, 5000, 5000);
        assert!(a == b, "seek(0) then render = the first render");
        p.seek(2500);
        let c = play(p, 2500, 2500);
        assert!(c[..] == a[2500 * 2..], "seek into the song");
    });
}

#[cfg(feature = "us-automation")]
#[cfg(feature = "us-matrix")]
#[test]
fn an_automated_wheel_opens_a_route_and_seeking_gives_what_playing_gives() {
    use crate::testing::{render as play, song, with_player, Track};
    // the wheel drives the gain; its lane rises over the held note
    let chunk = patch(&route(0, source::WHEEL, target::GAIN, true, 1.0));
    let mut t = Track::new(&[0], 0);
    t.autos = vec![(0, pm::ctl::WHEEL as i32, vec![(0, 0), (8000, 255)])];
    let bytes = song(44100, 0.3, &[(64, chunk)], &[vec![(0, 69, Some(100))]], &[t]);
    with_player(&bytes, 100, |p| {
        let a = play(p, 8000, 8000);
        let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
        let (early, late) = (rms(&a[2 * 500..2 * 1500]), rms(&a[2 * 6500..2 * 7500]));
        assert!(late > 3.0 * early && early > 0.0, "the route opens as the lane rises: {early} -> {late}");
        p.seek(4000);
        let b = play(p, 4000, 4000);
        assert!(b[..] == a[4000 * 2..], "seeking to the middle gives the value playing through gave");
    });
}

#[test]
fn rendering_flushes_denormals_and_restores_the_float_mode() {
    with_ins(&patch(&[]), 44100.0, |ins| {
        let before = synth_core::denormal::mode();
        render(ins, 44100.0, &[on(0, 60)], 300, 100);
        assert_eq!(synth_core::denormal::mode(), before);
    });
}

// ---- 2.3: envelopes in the voice ----

fn env_edits(a_ms: f32, d_ms: f32, s: f32, r_ms: f32) -> Vec<(usize, f32)> {
    vec![
        (pm::env(0, pm::e::ATTACK), inv::inv_env_ms(a_ms)),
        (pm::env(0, pm::e::DECAY), inv::inv_env_ms(d_ms)),
        (pm::env(0, pm::e::SUSTAIN), s),
        (pm::env(0, pm::e::RELEASE), inv::inv_env_ms(r_ms)),
    ]
}

#[test]
fn voice_ends_when_the_release_completes() {
    // capacity 1, no stealing: a new note is accepted only once the voice is free
    let mut edits = env_edits(1.0, 5.0, 1.0, 100.0);
    edits.extend([(v::CAPACITY, cap(1)), (v::STEAL, 0.0)]);
    with_ins(&patch(&edits), 44100.0, |ins| {
        let evs = [on(0, 69), off(1000, 69), on(5000, 72), on(6000, 76)];
        let (l, _) = render(ins, 44100.0, &evs, 1000 + 4410 + 200, 500);
        // 5000 is inside the release (it ends at 1000 + 4410 = 5410): dropped, the old voice keeps releasing
        assert_eq!(ins.voices()[0].note(), 69);
        // release: quadratic from the sustain value, so half-way through (frame 3205) the level is ≈ 0.25
        let win = |c: usize| peak(&l[c - 60..c + 60]);
        let full = win(900);
        assert!((win(3205) / full - 0.25).abs() < 0.04, "{} / {full}", win(3205));
        assert!(l[5440..].iter().all(|&x| x == 0.0), "silent from the first control frame after the release ends");
        assert!(ins.voices().iter().all(|v| !v.is_on()), "free");
        // 6000 is after the release: accepted
        let (l2, _) = render_from(ins, 5610, 1500);
        assert!(l2.iter().all(|&x| x == 0.0), "the dropped note never sounds");
        ins.note_on(76, 100, 0);
        let (l3, _) = render_from(ins, 7110, 600);
        assert!(peak(&l3) > 0.5);
    });
}

#[test]
fn envelope_one_scales_the_output_unless_it_is_routed_only() {
    let level_at = |edits: &[(usize, f32)], frame: usize| {
        with_ins(&patch(edits), 44100.0, |ins| {
            let evs = [on(0, 69), off(3000, 69)];
            let (l, _) = render(ins, 44100.0, &evs, 6000, 6000);
            peak(&l[frame - 60..frame + 60])
        })
    };
    let at_output = env_edits(1.0, 5.0, 1.0, 100.0);
    let mut routed = at_output.clone();
    routed.push((v::ENV1_AT_OUTPUT, 0.0));
    // 50 % into the release (frame 3000 + 2205): −12 dB with the envelope at the output, unchanged without it
    let full = level_at(&at_output, 2000);
    assert!((level_at(&at_output, 5205) / full - 0.25).abs() < 0.04);
    assert!((level_at(&routed, 5205) / full - 1.0).abs() < 0.04, "the envelope no longer scales the output");
    // the voice still ends when envelope 1 finishes (3000 + 4410)
    with_ins(&patch(&routed), 44100.0, |ins| {
        render(ins, 44100.0, &[on(0, 69), off(3000, 69)], 7600, 7600);
        assert!(ins.voices().iter().all(|v| !v.is_on()));
    });
}

#[test]
fn envelope_timing_is_in_seconds_at_any_rate() {
    // attack 100 ms, decay 200 ms to 0.4: values at 50 ms (0.5), at 200 ms (= 100 ms into the decay: 0.7), at 400 ms
    for rate in [44100.0f64, 48000.0, 22050.0] {
        let edits = env_edits(100.0, 200.0, 0.4, 100.0);
        with_ins(&patch(&edits), rate, |ins| {
            let at = |ins: &mut UInstrument, from: u64, to: u64| {
                // renders [from, to) and returns envelope 1 at the end of the voice's segment (≤ 32 frames ahead)
                render_rate(ins, rate, from, (to - from) as usize);
                ins.voices()[0].env_value(0)
            };
            ins.note_on(69, 100, 0);
            let t = |sec: f64| (sec * rate).round() as u64 - 16; // the segment end is 16 frames later on average
            let a = at(ins, 0, t(0.05));
            assert!((a - 0.5).abs() < 0.01, "{rate}: attack {a}");
            let b = at(ins, t(0.05), t(0.2));
            assert!((b - (0.25 + 0.4 * 0.75)).abs() < 0.02, "{rate}: decay {b}");
            let c = at(ins, t(0.2), t(0.4));
            assert!((c - 0.4).abs() < 1e-3, "{rate}: sustain {c}");
        });
    }
}

fn render_rate(ins: &mut UInstrument, rate: f64, from: u64, n: usize) {
    let mut rng = Lcg(1);
    let (mut a, mut b, mut c, mut d) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    let mut ctx = Ctx { rate, tempo: 120, frame: from, rng: &mut rng };
    ins.run(&mut Io { ch: [&mut a, &mut b, &mut c, &mut d] }, n, &mut ctx);
}

#[test]
fn envelope_modes_are_selectable_per_envelope() {
    // the rate-mode curve on envelope 1: rate based, the attack takes tens of milliseconds, not the 1 ms minimum
    let mut edits = vec![(pm::env(0, pm::e::MODE), 1.0), (pm::env(0, pm::e::ATTACK), 0.5)];
    edits.push((pm::env(0, pm::e::SUSTAIN), 0.8));
    with_ins(&patch(&edits), 44100.0, |ins| {
        ins.note_on(69, 100, 0);
        render_rate(ins, 44100.0, 0, 800); // 18 ms
        let early = ins.voices()[0].env_value(0);
        assert!(early > 0.1 && early < 0.9, "still attacking at 18 ms: {early}");
        render_rate(ins, 44100.0, 800, 44100 * 4);
        assert!((ins.voices()[0].env_value(0) - 0.8).abs() < 0.01, "settles on the sustain level");
    });
}

// ---- 2.4: LFOs and the mod matrix in the voice ----

use crate::modmx::{source, target};

/// Edits that configure mod slot `i`.
fn route(i: usize, src: usize, tgt: u8, scale: bool, amount: f32) -> Vec<(usize, f32)> {
    let amount_p =
        if scale { inv::inv_scale_amount(amount) } else { inv::inv_offset_amount(amount, target::offset_range(tgt)) };
    vec![
        (pm::modslot(i, pm::m::SOURCE), inv::inv_choice(src, source::COUNT)),
        (pm::modslot(i, pm::m::TARGET), inv::inv_index(tgt)),
        (pm::modslot(i, pm::m::OPERATION), inv::inv_choice(if scale { 0 } else { 1 }, 2)),
        (pm::modslot(i, pm::m::AMOUNT), amount_p),
    ]
}

#[cfg(feature = "us-voice-lfo")]
/// LFO `k` at 1 Hz (rate p = 0.5), depth 1, sine, with the given key sync.
fn lfo_1hz(k: usize, synced: bool) -> Vec<(usize, f32)> {
    vec![
        (pm::lfo(k, pm::l::RATE), 0.5),
        (pm::lfo(k, pm::l::DEPTH), 1.0),
        (pm::lfo(k, pm::l::SHAPE), 0.0),
        (pm::lfo(k, pm::l::SYNC), if synced { 1.0 } else { 0.0 }),
    ]
}

fn merged(parts: &[Vec<(usize, f32)>]) -> Vec<(usize, f32)> {
    parts.iter().flatten().copied().collect()
}

#[cfg(feature = "us-voice-lfo")]
#[test]
fn lfo_key_sync_restarts_at_note_on_and_free_running_shares_the_phase() {
    let sr = 44100.0f64;
    // the accessor reads the phase at the end of the voice's current control segment; both voices are in their fine
    // window (8-frame segments) when the renders stop at frames 100 and 13 400, so those segments end at 104 and 13 400
    let (end_a, end_b) = (104u64, 13_400u64);
    for (synced, label) in [(true, "synced"), (false, "free")] {
        with_ins(&patch(&merged(&[lfo_1hz(0, synced), vec![(v::CAPACITY, cap(4))]])), sr, |ins| {
            let (fa, fb) = (5u64, 13_231u64); // 0.3 s apart, different positions in the control block
            render(ins, sr, &[on(fa, 60)], 100, 100);
            let a = ins.voices()[0].lfo_phase(0);
            render_more(ins, &[on(fb, 64)], 100, 13_300);
            let b = ins.voices()[1].lfo_phase(0);
            let (want_a, want_b) = if synced {
                ((end_a - fa) as f64 / sr, (end_b - fb) as f64 / sr)
            } else {
                ((end_a as f64 / sr).fract(), (end_b as f64 / sr).fract())
            };
            assert!((a - want_a).abs() < 1e-6 && (b - want_b).abs() < 1e-6, "{label}: {a} {b} vs {want_a} {want_b}");
        });
    }
}

#[cfg(feature = "us-matrix")]
#[cfg(feature = "us-voice-lfo")]
#[test]
fn an_lfo_offset_on_pitch_is_vibrato() {
    let edits = merged(&[lfo_1hz(0, true), route(0, source::LFO1, target::PITCH, false, 0.5)]);
    with_ins(&patch(&edits), 44100.0, |ins| {
        ins.note_on(69, 100, 0);
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for i in 0..441 {
            render_rate(ins, 44100.0, i * 100, 100);
            let o = ins.voices()[0].mod_offset(target::PITCH);
            lo = lo.min(o);
            hi = hi.max(o);
        }
        assert!((lo + 0.5).abs() < 0.03 && (hi - 0.5).abs() < 0.03, "±0.5 semitones: {lo} {hi}");
    });
}

#[cfg(feature = "us-matrix")]
#[test]
fn an_envelope_offset_on_pitch_raises_it_by_the_amount() {
    // envelope 2 (sustain 1, 1 ms attack) → pitch +12: one octave up
    let edits = route(0, source::ENV1 + 1, target::PITCH, false, 12.0);
    with_ins(&patch(&edits), 44100.0, |ins| {
        let (l, _) = render(ins, 44100.0, &[on(0, 69)], 44100 + 4410, 4096);
        let c = crossings(&l[4410..]) as f32;
        assert!((c - 880.0).abs() <= 3.0, "{c}");
    });
    // and a Scale route on pitch multiplies the frequency: env 2 · 2.0 = ×2 → also an octave
    let edits = route(0, source::ENV1 + 1, target::PITCH, true, 2.0);
    with_ins(&patch(&edits), 44100.0, |ins| {
        let (l, _) = render(ins, 44100.0, &[on(0, 69)], 44100 + 4410, 4096);
        let c = crossings(&l[4410..]) as f32;
        assert!((c - 880.0).abs() <= 3.0, "{c}");
    });
}

#[cfg(feature = "us-matrix")]
#[test]
fn envelope_one_routed_only_shapes_the_oscillator_and_still_ends_the_voice() {
    let mut edits = env_edits(1.0, 5.0, 1.0, 100.0);
    edits.push((v::ENV1_AT_OUTPUT, 0.0));
    edits.extend(route(0, source::ENV1, target::osc(0, target::OSC_GAIN), true, 1.0));
    with_ins(&patch(&edits), 44100.0, |ins| {
        let (l, _) = render(ins, 44100.0, &[on(0, 69), off(3000, 69)], 7600, 7600);
        let win = |c: usize| peak(&l[c - 60..c + 60]);
        // −12 dB halfway through the release: scaled once, by the route (not again at the output)
        assert!((win(5205) / win(2000) - 0.25).abs() < 0.04, "{} {}", win(5205), win(2000));
        assert!(ins.voices().iter().all(|v| !v.is_on()), "env 1 finished → the voice is free");
    });
}

#[cfg(feature = "us-matrix")]
#[test]
fn mod_wheel_velocity_and_key_are_sources() {
    let level = |edits: &[(usize, f32)], wheel: f32, vel: u8, note: u8| {
        with_ins(&patch(edits), 44100.0, |ins| {
            ins.mod_wheel(wheel);
            let (l, _) = render(ins, 44100.0, &[Ev { frame: 0, note, vel }], 2000, 2000);
            peak(&l[300..])
        })
    };
    let wheel = route(0, source::WHEEL, target::GAIN, true, 1.0);
    let (a, b) = (level(&wheel, 1.0, 100, 69), level(&wheel, 0.5, 100, 69));
    assert!((b / a - 0.5).abs() < 0.02, "wheel 0.5 halves the gain: {a} {b}");
    let key = route(0, source::KEY, target::GAIN, true, 1.0);
    let (hi, lo) = (level(&key, 0.0, 100, 96), level(&key, 0.0, 100, 48));
    assert!((lo / hi - 48.0 / 96.0).abs() < 0.02, "gain follows the note: {hi} {lo}");
    let mut off = wheel.clone();
    off.push((pm::modslot(0, pm::m::SOURCE), 0.0)); // no source: the slot is inert
    assert!((level(&off, 0.0, 100, 69) - a).abs() < 0.01);
}

#[cfg(feature = "us-matrix")]
#[test]
fn slot_amounts_can_be_modulated_by_other_slots() {
    // slot 1: velocity → gain (Scale ×1); slot 2: env 2 → slot 1's amount. Env 2 sits at 1.0 (sustain), so the
    // second pass onwards the gain is vel · env2 · 1 = vel
    let mut edits = route(0, source::VELOCITY, target::GAIN, true, 1.0);
    edits.extend(route(1, source::ENV1 + 1, target::SLOT, true, 1.0));
    with_ins(&patch(&edits), 44100.0, |ins| {
        ins.note_on(69, 64, 0);
        render_rate(ins, 44100.0, 0, 3000);
        let s = ins.voices()[0].mod_scale(target::GAIN);
        assert!((s - 0.5).abs() < 0.01, "{s}");
    });
}

#[cfg(feature = "us-matrix")]
#[cfg(feature = "us-voice-lfo")]
#[test]
fn modulated_rendering_is_independent_of_the_call_split() {
    let edits = merged(&[
        lfo_1hz(0, false),
        lfo_1hz(1, true),
        route(0, source::LFO1, target::PITCH, false, 1.0),
        route(1, source::LFO1 + 1, target::GAIN, true, 1.0),
        route(2, source::ENV1 + 1, target::SLOT + 1, true, 1.0),
        route(3, source::VELOCITY, target::PAN, false, 0.3),
        vec![(pm::env(1, pm::e::ATTACK), 0.3), (pm::env(1, pm::e::DECAY), 0.2), (pm::env(1, pm::e::SUSTAIN), 0.4)],
    ]);
    let evs = song_events();
    let whole = with_ins(&patch(&edits), 44100.0, |ins| render(ins, 44100.0, &evs, 1500, 1500));
    assert!(peak(&whole.0) > 0.0);
    for call in [1, 17, 32, 33, 100, 511] {
        let split = with_ins(&patch(&edits), 44100.0, |ins| render(ins, 44100.0, &evs, 1500, call));
        assert!(split == whole, "call size {call}");
    }
}

#[cfg(feature = "us-matrix")]
#[test]
fn the_matrix_is_evaluated_at_the_note_s_first_frame() {
    // velocity (0.5) → gain ×1, velocity sensitivity off, envelope 1 off the output: the first control segment already
    // runs at the routed gain (0.5 · centre pan 0.707), not ramping towards it
    let mut edits = route(0, source::VELOCITY, target::GAIN, true, 1.0);
    edits.extend([(v::ENV1_AT_OUTPUT, 0.0), (v::VELOCITY, 0.0)]);
    with_ins(&patch(&edits), 44100.0, |ins| {
        let (l, _) = render(ins, 44100.0, &[Ev { frame: 0, note: 69, vel: 64 }], 100, 100);
        assert!((peak(&l[..100]) - 0.5 * 0.5f32.sqrt()).abs() < 0.02, "{}", peak(&l[..100]));
    });
}

#[cfg(feature = "us-matrix")]
#[test]
fn offsets_on_pan_move_the_voice_across_the_field() {
    // envelope 2 (sustain 1) → pan offset +0.5 from the centre: all the way right
    let edits = route(0, source::ENV1 + 1, target::PAN, false, 0.5);
    with_ins(&patch(&edits), 44100.0, |ins| {
        let (l, r) = render(ins, 44100.0, &[on(0, 69)], 3000, 3000);
        assert!(peak(&l[500..]) < 0.02 && peak(&r[500..]) > 0.5, "{} {}", peak(&l[500..]), peak(&r[500..]));
    });
}

#[cfg(feature = "us-osc-pm")]
mod osc_pm;

#[cfg(all(feature = "us-osc-pm", feature = "us-osc-blit"))]
mod osc_blit;

#[cfg(all(feature = "us-osc-pm", feature = "us-osc-wt"))]
mod osc_wt;

#[cfg(all(feature = "us-osc-pm", feature = "us-osc-sample", feature = "us-sample-dls", feature = "us-sample-gsm"))]
mod osc_sample;
#[cfg(all(feature = "us-osc-pm", feature = "us-osc-sample", feature = "us-sample-dls", feature = "us-sample-gsm"))]
mod osc_dls_instrument;

#[cfg(all(feature = "us-osc-pm", feature = "us-noise", feature = "us-flt"))]
mod flt_noise;

/// Without the channel stage's build an aux oscillator has no extra input pairs: the patch does not load.
#[cfg(all(feature = "us-osc-naive", feature = "us-env-seconds", not(feature = "us-strip")))]
#[test]
fn an_aux_oscillator_needs_the_channel_stage_build() {
    let mut p = pm::DEFAULTS;
    p[pm::osc(0, o::TYPE)] = osc_type::encode(osc_type::V2);
    p[pm::osc(0, o::SHAPE + pm::shape::V2_MODE)] = 7.0 / 128.0;
    let c = crate::chunk::encode(&p, &[]);
    assert_eq!(UInstrument::need(&c).err(), Some(LoadError::UnsupportedModule(crate::module::STRIP)));
}

// V2 modules (change add-v2-import)
#[cfg(all(feature = "us-osc-naive", feature = "us-flt-feedback", feature = "us-flt-parallel", feature = "us-drive", feature = "us-env-frame", feature = "us-lfo-frame", feature = "us-voice-out", feature = "us-strip", feature = "us-matrix-add", feature = "us-voice-lfo"))]
mod v2;

#[cfg(all(feature = "probe", feature = "us-osc-pm", feature = "us-osc-sample", feature = "us-sample-dls", feature = "us-sample-gsm", feature = "us-flt-svf", feature = "us-voice-lfo"))]
mod probe;

#[cfg(all(
    feature = "us-osc-pm",
    feature = "us-osc-blit",
    feature = "us-osc-wt",
    feature = "us-osc-sample",
    feature = "us-sample-dls",
    feature = "us-sample-gsm",
    feature = "us-noise",
    feature = "us-flt"
))]
mod golden;

#[cfg(all(
    feature = "us-osc-pm",
    feature = "us-osc-blit",
    feature = "us-osc-wt",
    feature = "us-osc-sample",
    feature = "us-sample-dls",
    feature = "us-sample-gsm",
    feature = "us-env-seconds"
))]
mod pool;

/// Energy of a short percussive envelope: the voice samples its control values on an 8-frame grid for the first
/// 256 frames after a note-on, so a few-millisecond attack and decay keep the energy of the envelope stepped per frame
/// (on the 32-frame grid a 3 ms hit lost about 1.7 dB of it).
#[test]
fn a_few_millisecond_envelope_keeps_its_energy() {
    let tone = [(osc(0, o::TRACKING), 1.0), (osc(0, o::FIXED_HZ), inv::inv_fixed_hz(8000.0))];
    let shape = |decay: f32, sustain: f32| {
        let mut e = tone.to_vec();
        e.extend([(pm::env(0, pm::e::MODE), 0.0), (pm::env(0, pm::e::ATTACK), 0.0), (pm::env(0, pm::e::DECAY), decay), (pm::env(0, pm::e::SUSTAIN), sustain)]);
        e
    };
    let power = |x: &[f32]| x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
    let rate = 44100.0f64;
    // the level of the tone itself: a held note at full sustain, away from the attack
    let held = with_ins(&patch(&shape(0.0, 1.0)), rate, |ins| render(ins, rate, &[on(0, 60)], 2000, 2000).0);
    let per_frame = power(&held[800..1800]) / 1000.0;
    for decay in [0.0, 0.02, 0.05, 0.1] {
        let hit = with_ins(&patch(&shape(decay, 0.0)), rate, |ins| render(ins, rate, &[on(0, 60)], 2000, 2000).0);
        // the original envelope: stepped every frame
        let ep = crate::env::EnvP::read(&{
            let mut v = pm::DEFAULTS;
            shape(decay, 0.0).iter().for_each(|&(i, x)| v[i] = x);
            v
        }, 0);
        let mut e = crate::env::Env::new();
        e.trigger(false);
        let mut want = 0.0f64;
        for _ in 0..2000 {
            e.advance(1.0 / rate as f32, &ep, 1.0);
            want += (e.value() as f64).powi(2);
        }
        let got = power(&hit) / per_frame;
        assert!((got / want - 1.0).abs() < 0.10, "decay {decay}: energy {got:.1} vs the per-frame envelope's {want:.1}");
    }
}

/// A releasing voice that can no longer be heard is freed before its envelope has finished: an exponential release
/// takes seconds to reach the envelope's own end, and a fast arpeggio would hold twice the voices the original engine does.
#[test]
fn a_releasing_voice_is_freed_once_it_is_inaudible() {
    let edits = [
        (pm::env(0, pm::e::MODE), 1.0), // rate mode
        (pm::env(0, pm::e::ATTACK), 0.1),
        (pm::env(0, pm::e::DECAY), 0.5),
        (pm::env(0, pm::e::SUSTAIN), 0.6),
        (pm::env(0, pm::e::RELEASE), 0.9),
    ];
    with_ins(&patch(&edits), 44100.0, |ins| {
        let evs = [on(0, 69), off(8000, 69)];
        render(ins, 44100.0, &evs, 8100, 8100);
        assert!(ins.voices().iter().any(|v| v.is_on()), "still releasing");
        let mut at = 8100;
        while ins.voices().iter().any(|v| v.is_on()) {
            render_more(ins, &[], at, 512);
            at += 512;
            assert!(at < 44100 * 120, "the voice never ends");
        }
        let left = ins.voices()[0].env_value(0);
        assert!(left > 1e-4, "freed at an envelope of {left}, well before the envelope's own end (1e-5)");
        assert!(left < 5e-3, "but not while still audible: {left}");
    });
}

#[test]
fn a_held_quiet_note_is_not_freed() {
    // only a release can free a voice this way: a note held at a low level keeps sounding
    let edits = [(pm::env(0, pm::e::SUSTAIN), 0.0003), (pm::env(0, pm::e::MODE), 0.0)];
    with_ins(&patch(&edits), 44100.0, |ins| {
        render(ins, 44100.0, &[on(0, 69)], 20000, 20000);
        assert!(ins.voices().iter().any(|v| v.is_on()));
    });
}

#[test]
fn an_instrument_is_quiet_only_without_voices_or_queued_notes() {
    use synth_core::device::Device;
    with_ins(&patch(&[(pm::env(0, pm::e::RELEASE), 0.0)]), 44100.0, |ins| {
        assert_eq!(ins.quiet_after(), Some(0), "fresh");
        ins.note_on(60, 100, 50);
        assert_eq!(ins.quiet_after(), None, "a queued note");
        render(ins, 44100.0, &[], 100, 100);
        assert_eq!(ins.quiet_after(), None, "a sounding voice");
        ins.note_off(60, 0);
        render(ins, 44100.0, &[], 44100, 100);
        assert_eq!(ins.quiet_after(), Some(0), "released and freed");
    });
}
