use crate::dsp::{AllPass, Biquad, Coefs, Comb, OnePole, Ring};
use crate::math::*;
use crate::pool::{OutOfMemory, Pool};

fn close(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() <= eps
}

// ---- math ----

#[test]
fn note_to_hz_reference_points() {
    assert!(close(note_to_hz(69.0), 440.0, 1e-3));
    assert!(close(note_to_hz(81.0), 880.0, 1e-3));
    assert!(close(note_to_hz(57.0), 220.0, 1e-3));
    assert!((note_to_hz_f64(69.0) - 440.0).abs() < 1e-9);
}

#[test]
fn db_gain_round_trip() {
    assert!(close(db_to_gain(0.0), 1.0, 1e-6));
    assert!(close(db_to_gain(-20.0), 0.1, 1e-6));
    assert!(close(gain_to_db(0.5), -6.0206, 1e-3));
}

#[test]
fn equal_power_pan() {
    assert!(close(pan_left(0.5), core::f32::consts::FRAC_1_SQRT_2, 1e-6));
    assert!(close(pan_right(0.5), core::f32::consts::FRAC_1_SQRT_2, 1e-6));
    assert_eq!((pan_left(0.0), pan_right(0.0)), (1.0, 0.0));
    assert_eq!((pan_left(1.0), pan_right(1.0)), (0.0, 1.0));
}

#[test]
fn clamp_mix_exp2() {
    assert_eq!(clamp(2.0, 0.0, 1.0), 1.0);
    assert_eq!(clamp(-1.0, 0.0, 1.0), 0.0);
    assert_eq!(mix(2.0, 4.0, 0.25), 2.5);
    assert_eq!(exp2(3.0), 8.0);
}

// ---- pool ----

#[test]
fn pool_alignment_and_zeroing() {
    let mut mem = [0xAAu8; 256];
    let mut pool = Pool::new(&mut mem);
    let a: &mut [u8] = pool.slice(3).unwrap();
    assert_eq!(a, &[0, 0, 0]);
    let b: &mut [f64] = pool.slice(4).unwrap();
    assert_eq!(b.as_ptr() as usize % core::mem::align_of::<f64>(), 0);
    assert!(b.iter().all(|&x| x == 0.0));
    let v = pool.value(7u32).unwrap();
    assert_eq!(*v, 7);
}

#[test]
fn pool_overflow_is_an_error() {
    let mut mem = [0u8; 64];
    let mut pool = Pool::new(&mut mem);
    let r: Result<&mut [f32], OutOfMemory> = pool.slice(17);
    assert!(r.is_err());
    // a failed request consumes nothing
    let ok: &mut [f32] = pool.slice(8).unwrap();
    assert_eq!(ok.len(), 8);
}

#[test]
fn pool_need_is_an_upper_bound() {
    let need = Pool::need::<u8>(3) + Pool::need::<f64>(4) + Pool::need::<u32>(1);
    let mut mem = vec![0u8; need];
    let mut pool = Pool::new(&mut mem);
    let _: &mut [u8] = pool.slice(3).unwrap();
    let _: &mut [f64] = pool.slice(4).unwrap();
    pool.value(1u32).unwrap();
}

// ---- denormal guard ----

#[test]
fn denormal_guard_restores_mode() {
    let before = crate::denormal::mode();
    {
        let _g = crate::denormal::Guard::new();
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            let tiny = core::hint::black_box(1e-38f32);
            assert_eq!(core::hint::black_box(tiny * 1e-3), 0.0, "subnormal result flushed");
        }
    }
    assert_eq!(crate::denormal::mode(), before);
}

// ---- comb / allpass ----

#[test]
fn comb_impulse_response() {
    let mut buf = [0.0f32; 7];
    let mut c = Comb::new(&mut buf);
    c.feedback = 0.5;
    c.damp = 0.0;
    let out: Vec<f32> = (0..30).map(|i| c.process(if i == 0 { 1.0 } else { 0.0 })).collect();
    for (i, &y) in out.iter().enumerate() {
        let want = match i {
            7 => 1.0,
            14 => 0.5,
            21 => 0.25,
            28 => 0.125,
            _ => 0.0,
        };
        assert_eq!(y, want, "sample {i}");
    }
}

#[test]
fn comb_damping_is_a_one_pole_in_the_loop() {
    let mut buf = [0.0f32; 1];
    let mut c = Comb::new(&mut buf);
    c.feedback = 1.0;
    c.damp = 0.5;
    // len 1: out[n] = buf; z = out*0.5 + z*0.5; buf = in + z
    let y: Vec<f32> = [1.0, 0.0, 0.0].iter().map(|&x| c.process(x)).collect();
    assert_eq!(y, vec![0.0, 1.0, 0.5]);
}

#[test]
fn comb_and_allpass_clear() {
    let (mut b1, mut b2) = ([0.0f32; 3], [0.0f32; 3]);
    let mut c = Comb::new(&mut b1);
    let mut a = AllPass::new(&mut b2);
    c.feedback = 0.5;
    a.feedback = 0.5;
    for x in [1.0, 0.5, 0.25, 2.0] {
        c.process(x);
        a.process(x);
    }
    c.clear();
    a.clear();
    assert!((0..6).all(|_| c.process(0.0) == 0.0 && a.process(0.0) == 0.0));
}

#[test]
fn allpass_freeverb_form() {
    let mut buf = [0.0f32; 3];
    let mut a = AllPass::new(&mut buf);
    a.feedback = 0.5;
    let y: Vec<f32> = (0..7).map(|i| a.process(if i == 0 { 1.0 } else { 0.0 })).collect();
    // out = buf - in; buf = in + buf*fb
    assert_eq!(y, vec![-1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5]);
}

// ---- ring delay ----

#[test]
fn ring_integer_taps() {
    let mut buf = [0.0f32; 8];
    let mut r = Ring::new(&mut buf);
    for i in 1..=5 {
        r.write(i as f32);
    }
    assert_eq!(r.at(1), 5.0); // latest
    assert_eq!(r.at(3), 3.0);
    assert_eq!(r.oldest(), 0.0); // 8 slots, only 5 written
}

#[test]
fn ring_read_then_write_is_a_len_sample_delay() {
    let mut buf = [0.0f32; 4];
    let mut r = Ring::new(&mut buf);
    let out: Vec<f32> = (1..=8)
        .map(|i| {
            let y = r.oldest();
            r.write(i as f32);
            y
        })
        .collect();
    assert_eq!(out, vec![0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn ring_fractional_read_is_linear() {
    let mut buf = [0.0f32; 32];
    let mut r = Ring::new(&mut buf);
    for i in 0..32 {
        r.write(i as f32 * 2.0); // ramp
    }
    let d10 = r.at(10);
    let d11 = r.at(11);
    assert_eq!(r.read_frac(10.5), (d10 + d11) * 0.5);
    assert_eq!(r.read_frac(10.0), d10);
}

#[test]
fn ring_resize_clear_and_keep() {
    let mut buf = [0.0f32; 16];
    let mut r = Ring::new(&mut buf);
    for i in 1..=10 {
        r.write(i as f32);
    }
    r.set_len_keep(4); // position 10 clamped to 3, contents kept
    assert_eq!(r.len(), 4);
    assert_eq!([r.at(1), r.at(2), r.at(3), r.at(4)], [3.0, 2.0, 1.0, 4.0]);
    r.write(9.0);
    r.set_len(4); // same length: nothing cleared
    assert_eq!(r.at(1), 9.0);
    r.set_len(6); // clears and resets
    assert_eq!(r.len(), 6);
    assert!((0..6).all(|k| r.at(k) == 0.0));
    r.write(5.0);
    r.clear();
    assert!((0..6).all(|k| r.at(k) == 0.0) && r.len() == 6);
    r.set_len(100); // clamped to capacity
    assert_eq!(r.len(), 16);
    r.set_len(0); // minimum one sample
    assert_eq!(r.len(), 1);
}

// ---- biquad / one-pole ----

fn gain_db_at(c: Coefs, freq: f32, rate: f32) -> f32 {
    let mut f = Biquad::default();
    f.coefs = c;
    let n = rate as usize; // 1 s
    let mut peak = 0.0f32;
    for i in 0..n {
        let x = libm::sinf(2.0 * core::f32::consts::PI * freq * i as f32 / rate);
        let y = f.process(x);
        if i > n / 2 {
            peak = peak.max(y.abs());
        }
    }
    20.0 * libm::log10f(peak)
}

#[test]
fn biquad_lowpass_matches_designer() {
    let rate = 44100.0;
    let c = Coefs::lowpass_hz(1000.0, core::f32::consts::FRAC_1_SQRT_2, rate);
    assert!(gain_db_at(c, 100.0, rate).abs() < 0.1);
    assert!(close(gain_db_at(c, 1000.0, rate), -3.01, 0.1));
    let g10k = gain_db_at(c, 10000.0, rate);
    assert!(g10k < -38.0 && g10k > -44.0, "10 kHz gain {g10k}");
}

#[test]
fn biquad_highpass_and_peak() {
    let rate = 44100.0;
    let hp = Coefs::highpass_hz(1000.0, core::f32::consts::FRAC_1_SQRT_2, rate);
    assert!(gain_db_at(hp, 10000.0, rate).abs() < 0.1);
    assert!(close(gain_db_at(hp, 1000.0, rate), -3.01, 0.1));
    let pk = Coefs::peak_hz(1000.0, 1.0, 6.0, rate);
    assert!(close(gain_db_at(pk, 1000.0, rate), 6.0, 0.1));
    assert!(gain_db_at(pk, 100.0, rate).abs() < 0.3);
}

#[test]
fn biquad_df1_expression_order() {
    // c1*x + c2*x1 + c3*x2 - c4*y1 - c5*y2, evaluated left to right (parity with upstream)
    let mut f = Biquad::default();
    f.coefs = Coefs { c1: 0.1, c2: 0.2, c3: 0.3, c4: 0.4, c5: 0.5 };
    let (x0, x1, x2) = (1.0f32, -0.5, 0.25);
    let y0 = f.process(x0);
    let y1 = f.process(x1);
    let y2 = f.process(x2);
    assert_eq!(y0, 0.1 * x0);
    assert_eq!(y1, 0.1 * x1 + 0.2 * x0 - 0.4 * y0);
    assert_eq!(y2, 0.1 * x2 + 0.2 * x1 + 0.3 * x0 - 0.4 * y1 - 0.5 * y0);
}

#[test]
fn one_pole_step_response() {
    let mut p = OnePole::default();
    let k = 0.25;
    let y: Vec<f32> = (0..3).map(|_| p.lowpass(1.0, k)).collect();
    assert_eq!(y, vec![0.25, 0.4375, 0.578125]);
}

// ---- device / events ----

mod device_tests {
    use crate::device::*;
    use crate::rng::Rng;

    struct Seed(f32);
    impl Rng for Seed {
        fn next_f32(&mut self) -> f32 {
            self.0 += 1.0;
            self.0
        }
    }

    /// Logs every event with the absolute song frame it fires at; writes the absolute frame index as audio.
    #[derive(Default)]
    struct Recorder {
        events: Events<8>,
        log: Vec<(u64, EventKind)>,
    }

    impl Device for Recorder {
        fn param_count(&self) -> usize {
            0
        }
        fn set_param(&mut self, _: usize, _: f32) {}
        fn note_on(&mut self, note: u8, velocity: u8, offset: u32) {
            self.events.push(Event { offset, kind: EventKind::NoteOn { note, velocity } });
        }
        fn note_off(&mut self, note: u8, offset: u32) {
            self.events.push(Event { offset, kind: EventKind::NoteOff { note } });
        }
        fn reset(&mut self) {
            self.events.clear();
            self.log.clear();
        }
        fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
            let base = ctx.frame;
            let log = &mut self.log;
            self.events.run(n, |s| match s {
                Step::Event { at, kind } => log.push((base + at as u64, kind)),
                Step::Render { start, len } => {
                    for i in start..start + len {
                        io.ch[MAIN_L][i] = (base + i as u64) as f32;
                    }
                }
            });
        }
    }

    /// Runs one call of `n` frames starting at song frame `frame`; returns main L.
    fn call(d: &mut Recorder, frame: u64, n: usize) -> Vec<f32> {
        let (mut a, mut b, mut c, mut e) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
        let mut io = Io { ch: [&mut a, &mut b, &mut c, &mut e] };
        let mut rng = Seed(0.0);
        let mut ctx = Ctx { rate: 44100.0, tempo: 120, frame, rng: &mut rng };
        d.run(&mut io, n, &mut ctx);
        a
    }

    const ON: EventKind = EventKind::NoteOn { note: 60, velocity: 100 };
    const OFF: EventKind = EventKind::NoteOff { note: 60 };

    #[test]
    fn events_fire_at_their_offsets() {
        let mut d = Recorder::default();
        d.note_on(60, 100, 10);
        d.note_off(60, 30);
        let out = call(&mut d, 0, 64);
        assert_eq!(out[63], 63.0);
        assert_eq!(d.log, vec![(10, ON), (30, OFF)]);
    }

    #[test]
    fn offset_zero_and_boundary() {
        let mut d = Recorder::default();
        d.note_on(60, 100, 0);
        d.note_off(60, 64); // == n: belongs to the next call
        call(&mut d, 0, 64);
        assert_eq!(d.log, vec![(0, ON)]);
        call(&mut d, 64, 1); // n = 1
        assert_eq!(d.log, vec![(0, ON), (64, OFF)]);
    }

    #[test]
    fn lookahead_events_carry_over_calls() {
        let mut d = Recorder::default();
        d.note_on(60, 100, 150); // beyond this 100-frame call
        call(&mut d, 0, 100);
        assert!(d.log.is_empty());
        call(&mut d, 100, 100);
        assert_eq!(d.log, vec![(150, ON)]);
    }

    #[test]
    fn output_and_event_frames_independent_of_call_split() {
        let events = [(0u32, ON), (5, OFF), (700, ON), (3000, OFF), (4095, ON)];
        let render = |splits: &[usize]| {
            let mut d = Recorder::default();
            let (mut frame, mut out) = (0u64, vec![]);
            let mut pending = events.to_vec();
            for &n in splits {
                // deliver the events of this call (mixer behaviour with lookahead 0)
                pending.retain(|&(f, k)| {
                    if (f as u64) < frame + n as u64 {
                        let off = (f as u64 - frame) as u32;
                        match k {
                            EventKind::NoteOn { note, velocity } => d.note_on(note, velocity, off),
                            EventKind::NoteOff { note } => d.note_off(note, off),
                        }
                        false
                    } else {
                        true
                    }
                });
                out.extend(call(&mut d, frame, n));
                frame += n as u64;
            }
            (out, d.log)
        };
        let whole = render(&[4096]);
        assert_eq!(whole, render(&[1, 100, 3995]));
        let mut hundreds = vec![100usize; 40];
        hundreds.push(96);
        assert_eq!(whole, render(&hundreds));
    }

    #[test]
    fn render_spans_split_at_events_only() {
        let mut ev: Events<8> = Events::default();
        ev.push(Event { offset: 5, kind: EventKind::NoteOff { note: 1 } });
        ev.push(Event { offset: 0, kind: EventKind::NoteOff { note: 2 } });
        ev.push(Event { offset: 5, kind: EventKind::NoteOff { note: 3 } });
        let mut steps = vec![];
        ev.run(12, |s| steps.push(s));
        use EventKind::NoteOff as Off;
        assert_eq!(
            steps,
            vec![
                Step::Event { at: 0, kind: Off { note: 2 } },
                Step::Render { start: 0, len: 5 },
                Step::Event { at: 5, kind: Off { note: 1 } },
                Step::Event { at: 5, kind: Off { note: 3 } },
                Step::Render { start: 5, len: 7 },
            ]
        );
        assert!(ev.is_empty());
    }

    #[test]
    fn full_event_array_drops_new_events() {
        let mut ev: Events<2> = Events::default();
        let e = Event { offset: 1, kind: EventKind::NoteOff { note: 0 } };
        assert!(ev.push(e) && ev.push(e));
        assert!(!ev.push(e));
    }

    #[test]
    fn reset_drops_queued_events() {
        let mut d = Recorder::default();
        d.note_on(60, 100, 10);
        d.reset();
        call(&mut d, 0, 64);
        assert!(d.log.is_empty());
    }
}

// ---- voice pool ----

mod voice_tests {
    use crate::pool::Pool;
    use crate::voice::*;

    #[derive(Debug, Clone, PartialEq)]
    enum Call {
        On(u8, u8, f32, f32),
        Off,
        Slide(u8),
    }

    #[derive(Default)]
    struct V {
        on: bool,
        note: u8,
        calls: Vec<Call>,
    }

    impl VoiceState for V {
        fn is_on(&self) -> bool {
            self.on
        }
        fn note(&self) -> u8 {
            self.note
        }
        fn kill(&mut self) {
            self.on = false;
        }
    }

    impl Voice<()> for V {
        fn note_on(&mut self, note: u8, velocity: u8, detune: f32, pan: f32, _: &mut ()) {
            self.on = true;
            self.note = note;
            self.calls.push(Call::On(note, velocity, detune, pan));
        }
        fn note_off(&mut self, _: &mut ()) {
            self.calls.push(Call::Off); // stays on until its release ends (upstream)
        }
        fn slide(&mut self, note: u8, _: &mut ()) {
            self.calls.push(Call::Slide(note));
        }
    }

    fn pool_of(mem: &mut [u8], n: usize) -> VoicePool<'_, V> {
        let mut p = Pool::new(mem);
        VoicePool::new(&mut p, n, |_| V::default()).unwrap()
    }

    #[test]
    fn no_stealing_drops_the_note() {
        let mut mem = vec![0u8; 4096];
        let mut vp = pool_of(&mut mem, 4);
        for n in 60..64 {
            vp.note_on(n, 100, &mut ());
        }
        vp.note_on(70, 100, &mut ());
        assert!(vp.voices().iter().all(|v| v.calls.len() == 1));
        assert_eq!(vp.voices().iter().map(|v| v.note).collect::<Vec<_>>(), vec![60, 61, 62, 63]);
    }

    #[test]
    fn oldest_stealing_restarts_first_started_voice() {
        let mut mem = vec![0u8; 4096];
        let mut vp = pool_of(&mut mem, 4);
        vp.steal = Steal::Oldest;
        for n in [60, 61, 62, 63] {
            vp.note_on(n, 100, &mut ());
            vp.tick();
        }
        vp.note_on(70, 100, &mut ());
        assert_eq!(vp.voices()[0].note, 70);
        assert_eq!(vp.voices()[0].calls.len(), 2);
        // ties (same age) go to the lowest index
        let mut mem2 = vec![0u8; 4096];
        let mut vp2 = pool_of(&mut mem2, 3);
        vp2.steal = Steal::Oldest;
        for n in [60, 61, 62] {
            vp2.note_on(n, 100, &mut ());
        }
        vp2.note_on(70, 100, &mut ());
        assert_eq!(vp2.voices().iter().map(|v| v.note).collect::<Vec<_>>(), vec![70, 61, 62]);
    }

    #[test]
    fn polyphony_limit_restricts_candidates() {
        let mut mem = vec![0u8; 4096];
        let mut vp = pool_of(&mut mem, 4);
        vp.limit = 2;
        vp.steal = Steal::Oldest;
        vp.note_on(60, 1, &mut ());
        vp.tick();
        vp.note_on(61, 1, &mut ());
        vp.note_on(62, 1, &mut ());
        assert_eq!(vp.voices().iter().map(|v| v.note).collect::<Vec<_>>(), vec![62, 61, 0, 0]);
    }

    #[test]
    fn note_off_releases_matching_voices() {
        let mut mem = vec![0u8; 4096];
        let mut vp = pool_of(&mut mem, 4);
        vp.note_on(60, 1, &mut ());
        vp.note_on(62, 1, &mut ());
        vp.note_off(60, &mut ());
        assert_eq!(vp.voices()[0].calls.last(), Some(&Call::Off));
        assert_eq!(vp.voices()[1].calls.len(), 1);
    }

    #[test]
    fn unison_spread_matches_upstream() {
        let mut mem = vec![0u8; 4096];
        let mut vp = pool_of(&mut mem, 4);
        vp.unison = 3;
        vp.detune = 0.2;
        vp.pan_spread = 1.0; // VoicesPan = 1 -> full spread
        vp.note_on(60, 9, &mut ());
        // f = j/(U-1) with j counting down from U-1: 1, 0.5, 0
        let want = [(1.0f32, 1.0f32), (0.5, 0.5), (0.0, 0.0)];
        for (v, (f, p)) in vp.voices().iter().zip(want) {
            assert_eq!(v.calls[0], Call::On(60, 9, f * 0.2, p));
        }
        // VoicesPan 0.5 = no spread (all centred)
        let mut mem2 = vec![0u8; 4096];
        let mut vp2 = pool_of(&mut mem2, 4);
        vp2.unison = 2;
        vp2.pan_spread = 0.5;
        vp2.note_on(60, 9, &mut ());
        assert!(vp2.voices()[..2].iter().all(|v| matches!(v.calls[0], Call::On(_, _, _, p) if p == 0.5)));
    }

    #[test]
    fn mono_legato_slides_without_retrigger() {
        let mut mem = vec![0u8; 4096];
        let mut vp = pool_of(&mut mem, 4);
        vp.set_mode(Mode::MonoLegato, &mut ());
        vp.note_on(60, 100, &mut ());
        vp.note_on(64, 100, &mut ());
        vp.note_off(64, &mut ());
        let v = &vp.voices()[0];
        assert_eq!(v.calls, vec![Call::On(60, 100, 0.0, 0.5), Call::Slide(64), Call::Slide(60)]);
        assert!(vp.voices()[1].calls.is_empty());
        vp.note_off(60, &mut ());
        assert_eq!(vp.voices()[0].calls.last(), Some(&Call::Off));
    }

    #[test]
    fn mono_release_of_non_last_note_only_marks_it() {
        let mut mem = vec![0u8; 4096];
        let mut vp = pool_of(&mut mem, 2);
        vp.set_mode(Mode::MonoLegato, &mut ());
        vp.note_on(60, 1, &mut ());
        vp.note_on(64, 1, &mut ());
        vp.note_off(60, &mut ()); // not the last note: no audible change
        assert_eq!(vp.voices()[0].calls.len(), 2);
        vp.note_off(64, &mut ()); // 60 no longer held: all released
        assert_eq!(vp.voices()[0].calls.last(), Some(&Call::Off));
    }

    #[test]
    fn mono_guards_upstream_overflows() {
        let mut mem = vec![0u8; 4096];
        let mut vp = pool_of(&mut mem, 1);
        vp.set_mode(Mode::MonoLegato, &mut ());
        vp.note_off(60, &mut ()); // empty log: upstream reads log[-1]
        for i in 0..300u32 {
            vp.note_on((i % 128) as u8, 1, &mut ()); // > 128 entries: upstream overflows the log
        }
        vp.all_notes_off(&mut ());
        assert_eq!(vp.voices()[0].calls.last(), Some(&Call::Off));
    }

    #[test]
    fn mode_change_kills_voices() {
        let mut mem = vec![0u8; 4096];
        let mut vp = pool_of(&mut mem, 2);
        vp.note_on(60, 1, &mut ());
        vp.set_mode(Mode::MonoLegato, &mut ());
        assert!(!vp.voices()[0].on);
    }
}

// ---- mix: song decoder + registry ----

pub(crate) fn device_tests_rng() -> impl crate::rng::Rng {
    struct C(f32);
    impl crate::rng::Rng for C {
        fn next_f32(&mut self) -> f32 {
            self.0 += 1.0;
            self.0
        }
    }
    C(0.0)
}

pub(crate) mod blob {
    //! Test-only writer for the WaveSabre song blob.
    #[derive(Default)]
    pub struct W(pub Vec<u8>);
    impl W {
        pub fn i(&mut self, v: i32) -> &mut Self {
            self.0.extend(v.to_le_bytes());
            self
        }
        pub fn f(&mut self, v: f32) -> &mut Self {
            self.0.extend(v.to_le_bytes());
            self
        }
        pub fn d(&mut self, v: f64) -> &mut Self {
            self.0.extend(v.to_le_bytes());
            self
        }
        pub fn b(&mut self, v: u8) -> &mut Self {
            self.0.push(v);
            self
        }
    }

    /// 3 devices (kinds 1, 3, 10), 2 lanes, 3 tracks (2 instruments → master), 2 automations.
    pub fn sample() -> Vec<u8> {
        let mut w = W::default();
        w.i(125).i(44100).d(12.5);
        w.i(3);
        w.b(1).i(12).f(0.25).f(0.5).i(12); // chunk: 2 floats + size
        w.b(3).i(8).f(0.75).i(8);
        w.b(10).i(4).i(4);
        w.i(2);
        w.i(3).i(0).b(60).b(100).i(1000).b(60 | 0x80).i(10).b(64).b(1);
        w.i(0);
        w.i(3);
        // track 0: Slaughter-ish instrument on lane 0, one automation of param 1 on its device
        w.f(1.0).i(0).i(1).i(0).i(0).i(1);
        w.i(0).i(1).i(2).i(0).b(0).i(1000).b(255);
        // track 1: effect device 1 on lane 1, receives nothing, automation of device 0 (local index) param 0
        w.f(0.5).i(0).i(1).i(1).i(1).i(1);
        w.i(0).i(0).i(1).i(5).b(128);
        // track 2 (master): receives track 0 into main at 0.8 and track 1 into sidechain at 0.5, device 2
        w.f(0.9).i(2).i(0).i(0).f(0.8).i(1).i(2).f(0.5).i(1).i(2).i(1).i(0);
        w.0
    }
}

mod mix_song_tests {
    use super::blob::{sample, W};
    use crate::device::*;
    use crate::mix::*;
    use crate::pool::Pool;

    #[test]
    fn round_trip() {
        let bytes = sample();
        let s = Song::parse(&bytes).unwrap();
        assert_eq!((s.tempo, s.rate, s.length), (125, 44100, 12.5));
        let devs: Vec<_> = s.devices().map(|d| (d.kind, d.chunk.len())).collect();
        assert_eq!(devs, vec![(1, 12), (3, 8), (10, 4)]);
        let lane0: Vec<_> = s.lane(0).unwrap().events().collect();
        assert_eq!(
            lane0,
            vec![
                (0, EventKind::NoteOn { note: 60, velocity: 100 }),
                (1000, EventKind::NoteOff { note: 60 }),
                (10, EventKind::NoteOn { note: 64, velocity: 1 }),
            ]
        );
        assert_eq!(s.lane(1).unwrap().events().count(), 0);
        let t: Vec<_> = s.tracks().collect();
        assert_eq!(t.len(), 3);
        assert_eq!((t[0].volume, t[0].lane, t[0].devices().collect::<Vec<_>>()), (1.0, 0, vec![0]));
        let a: Vec<_> = t[0].automations().collect();
        assert_eq!((a[0].device, a[0].param), (0, 1));
        assert_eq!(a[0].points().collect::<Vec<_>>(), vec![(0, 0.0), (1000, 1.0)]);
        assert_eq!(t[1].automations().next().unwrap().points().collect::<Vec<_>>(), vec![(5, (128.0f64 / 255.0) as f32)]);
        let r: Vec<_> = t[2].receives().collect();
        assert_eq!(
            r,
            vec![Receive { track: 0, channel: 0, volume: 0.8 }, Receive { track: 1, channel: 2, volume: 0.5 }]
        );
    }

    #[test]
    fn truncation_at_every_byte_is_an_error() {
        let bytes = sample();
        for n in 0..bytes.len() {
            assert!(Song::parse(&bytes[..n]).is_err(), "prefix {n}");
        }
    }

    #[test]
    fn trailing_bytes_are_an_error() {
        let mut bytes = sample();
        bytes.push(0);
        assert_eq!(Song::parse(&bytes).err(), Some(LoadError::Malformed));
    }

    /// Patches the i32 at `at` in the sample blob.
    fn patched(at: usize, v: i32) -> Vec<u8> {
        let mut b = sample();
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        b
    }

    fn offset_of_tracks() -> usize {
        // header 16 + devices (4 + 3 records) + lanes (4 + lane0 + lane1)
        let devs = 4 + (1 + 4 + 12) + (1 + 4 + 8) + (1 + 4 + 4);
        let lanes = 4 + (4 + 6 + 5 + 6) + 4;
        16 + devs + lanes
    }

    #[test]
    fn out_of_range_references_are_errors() {
        let t0 = offset_of_tracks() + 4; // first track
        // track 0 layout: vol f32, recv count, dev count, dev idx, lane, auto count, (dev, param, n, points...)
        let dev_idx = t0 + 12;
        let lane = t0 + 16;
        let auto_dev = t0 + 24;
        assert_eq!(Song::parse(&patched(dev_idx, 3)).err(), Some(LoadError::BadIndex));
        assert_eq!(Song::parse(&patched(dev_idx, -1)).err(), Some(LoadError::BadIndex));
        assert_eq!(Song::parse(&patched(lane, 2)).err(), Some(LoadError::BadIndex));
        assert_eq!(Song::parse(&patched(auto_dev, 1)).err(), Some(LoadError::BadIndex));
        // master receive: sending track 3 and channel 1
        let mut w = W::default();
        w.0 = sample();
        let master = w.0.len() - (4 + 4 + 12 + 12 + 4 + 4 + 4 + 4);
        let send = master + 8;
        assert_eq!(Song::parse(&patched(send, 3)).err(), Some(LoadError::BadIndex));
        assert_eq!(Song::parse(&patched(send + 4, 1)).err(), Some(LoadError::BadIndex));
    }

    #[test]
    fn huge_counts_fail_fast() {
        // device count claims 2^31-1 records
        let mut b = sample();
        b[16..20].copy_from_slice(&i32::MAX.to_le_bytes());
        assert_eq!(Song::parse(&b).err(), Some(LoadError::Malformed));
        let mut b = sample();
        b[16..20].copy_from_slice(&(-1i32).to_le_bytes());
        assert_eq!(Song::parse(&b).err(), Some(LoadError::Malformed));
    }

    /// A test engine supporting only kinds 1 and 3 (as if `ws-cathedral`, kind 10, were disabled).
    enum TestDev {
        A(usize),
    }
    impl Device for TestDev {
        fn param_count(&self) -> usize {
            2
        }
        fn set_param(&mut self, _: usize, _: f32) {}
        fn reset(&mut self) {}
        fn run(&mut self, _: &mut Io, _: usize, _: &mut Ctx) {
            let TestDev::A(_) = self;
        }
    }
    impl<'a> Kinds<'a> for TestDev {
        fn need(kind: u8, _: &'a [u8], _: &Env<'a>) -> Result<usize, LoadError> {
            match kind {
                1 | 3 => Ok(64),
                k => Err(LoadError::UnsupportedKind(k)),
            }
        }
        fn load(
            kind: u8,
            chunk: &'a [u8],
            env: &Env<'a>,
            pool: &mut Pool<'a>,
            _: &mut dyn crate::rng::Rng,
        ) -> Result<Self, LoadError> {
            Self::need(kind, chunk, env)?;
            pool.slice::<u8>(64)?;
            Ok(TestDev::A(chunk.len()))
        }
    }

    #[test]
    fn unsupported_kind_fails_at_load() {
        let bytes = sample();
        let song = Song::parse(&bytes).unwrap();
        let env = Env::for_song(&song, None);
        assert_eq!(devices_need::<TestDev>(&song, &env).err(), Some(LoadError::UnsupportedKind(10)));
        let mut mem = vec![0u8; 4096];
        let mut pool = Pool::new(&mut mem);
        let mut rng = super::device_tests_rng();
        assert_eq!(load_devices::<TestDev>(&song, &env, &mut pool, &mut rng).err(), Some(LoadError::UnsupportedKind(10)));
    }

    #[test]
    fn supported_kinds_load_in_song_order() {
        let mut w = W::default();
        w.i(120).i(44100).d(1.0).i(2).b(3).i(4).i(4).b(1).i(8).f(0.5).i(8).i(1).i(0).i(1);
        w.f(1.0).i(0).i(2).i(0).i(1).i(0).i(0);
        let song = Song::parse(&w.0).unwrap();
        let env = Env::for_song(&song, None);
        let need = devices_need::<TestDev>(&song, &env).unwrap();
        let mut mem = vec![0u8; need];
        let mut pool = Pool::new(&mut mem);
        let devs = load_devices::<TestDev>(&song, &env, &mut pool, &mut super::device_tests_rng()).unwrap();
        let lens: Vec<usize> = devs.iter().map(|TestDev::A(n)| *n).collect();
        assert_eq!(lens, vec![4, 8]);
    }
}

/// Parses every `*.bin` WaveSabre song in `$WS_SONG_DIR` (upstream test refs converted outside the repo).
#[test]
fn real_songs_parse() {
    let Ok(dir) = std::env::var("WS_SONG_DIR") else {
        eprintln!("WS_SONG_DIR not set: skipped");
        return;
    };
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "bin") {
            let b = std::fs::read(&p).unwrap();
            let s = crate::mix::Song::parse(&b).unwrap_or_else(|e| panic!("{}: {e:?}", p.display()));
            assert_eq!(s.devices().count(), s.device_count());
            assert_eq!(s.lanes().count(), s.lane_count());
            let events: usize = s.lanes().map(|l| l.events().count()).sum();
            assert_eq!(events, s.lanes().map(|l| l.len()).sum::<usize>());
            assert_eq!(s.tracks().count(), s.track_count());
            n += 1;
        }
    }
    assert!(n > 0);
}

// ---- mix: player ----

mod player_tests {
    use super::blob::W;
    use crate::device::*;
    use crate::mix::*;
    use crate::pool::Pool;
    use crate::rng::Rng;

    #[derive(Clone)]
    struct Lcg(u32);
    impl Rng for Lcg {
        fn next_f32(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
            (self.0 >> 8) as f32 / (1u32 << 24) as f32
        }
    }

    /// Kind 1: instrument (gate from notes, level 1 + param0, plus 1e-3 · song rng noise). Kind 2: probe effect
    /// (records main/side input, outputs main + side). Kind 3: lookahead 511 recorder.
    enum TD {
        Inst { ev: Events<64>, gate: bool, p: [f32; 2], log: Vec<(u64, EventKind)> },
        Probe { p: [f32; 2], main: Vec<f32>, side: Vec<f32>, params: Vec<(u64, f32)> },
        Look { log: Vec<(u64, u32, EventKind)> },
    }

    impl Device for TD {
        fn param_count(&self) -> usize {
            2
        }
        fn set_param(&mut self, i: usize, v: f32) {
            match self {
                TD::Inst { p, .. } | TD::Probe { p, .. } if i < 2 => p[i] = v,
                _ => {}
            }
        }
        fn note_on(&mut self, note: u8, velocity: u8, offset: u32) {
            match self {
                TD::Inst { ev, .. } => {
                    ev.push(Event { offset, kind: EventKind::NoteOn { note, velocity } });
                }
                TD::Look { log } => log.push((u64::MAX, offset, EventKind::NoteOn { note, velocity })),
                _ => {}
            }
        }
        fn note_off(&mut self, note: u8, offset: u32) {
            if let TD::Inst { ev, .. } = self {
                ev.push(Event { offset, kind: EventKind::NoteOff { note } });
            }
        }
        fn event_lookahead(&self) -> u32 {
            if matches!(self, TD::Look { .. }) { 511 } else { 0 }
        }
        fn reset(&mut self) {
            match self {
                TD::Inst { ev, gate, log, .. } => {
                    ev.clear();
                    *gate = false;
                    log.clear();
                }
                TD::Probe { main, side, params, .. } => {
                    main.clear();
                    side.clear();
                    params.clear();
                }
                TD::Look { log } => log.clear(),
            }
        }
        fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
            match self {
                TD::Inst { ev, gate, p, log } => {
                    let level = 1.0 + p[0];
                    ev.run(n, |s| match s {
                        Step::Event { at, kind } => {
                            *gate = matches!(kind, EventKind::NoteOn { .. });
                            log.push((ctx.frame + at as u64, kind));
                        }
                        Step::Render { start, len } => {
                            for i in start..start + len {
                                let v = if *gate { level + 1e-3 * ctx.rng.next_f32() } else { 0.0 };
                                io.ch[MAIN_L][i] = v;
                                io.ch[MAIN_R][i] = -v;
                            }
                        }
                    });
                }
                TD::Probe { p, main, side, params } => {
                    params.push((ctx.frame, p[0]));
                    for i in 0..n {
                        main.push(io.ch[MAIN_L][i]);
                        side.push(io.ch[SIDE_L][i]);
                        io.ch[MAIN_L][i] += io.ch[SIDE_L][i];
                        io.ch[MAIN_R][i] += io.ch[SIDE_R][i];
                    }
                }
                TD::Look { log } => {
                    for e in log.iter_mut().filter(|e| e.0 == u64::MAX) {
                        e.0 = ctx.frame;
                    }
                }
            }
        }
    }

    impl<'a> Kinds<'a> for TD {
        fn need(kind: u8, _: &'a [u8], _: &Env<'a>) -> Result<usize, LoadError> {
            match kind {
                1..=3 => Ok(0),
                k => Err(LoadError::UnsupportedKind(k)),
            }
        }
        fn load(kind: u8, _: &'a [u8], _: &Env<'a>, _: &mut Pool<'a>, rng: &mut dyn Rng) -> Result<Self, LoadError> {
            rng.next_f32(); // a load-time draw (like upstream voice constructors)
            Ok(match kind {
                1 => TD::Inst { ev: Events::default(), gate: false, p: [0.0; 2], log: vec![] },
                2 => TD::Probe { p: [0.0; 2], main: vec![], side: vec![], params: vec![] },
                3 => TD::Look { log: vec![] },
                k => return Err(LoadError::UnsupportedKind(k)),
            })
        }
    }

    /// Song builder: devices (kinds), lanes (events: (delta, note, vel or None for off)), tracks.
    struct T {
        vol: f32,
        recv: Vec<(i32, i32, f32)>,
        devs: Vec<i32>,
        lane: i32,
        autos: Vec<(i32, i32, Vec<(i32, u8)>)>,
    }
    fn t(devs: &[i32], lane: i32) -> T {
        T { vol: 1.0, recv: vec![], devs: devs.to_vec(), lane, autos: vec![] }
    }
    fn song(kinds: &[u8], lanes: &[Vec<(i32, u8, Option<u8>)>], tracks: &[T]) -> Vec<u8> {
        let mut w = W::default();
        w.i(120).i(44100).d(1.0).i(kinds.len() as i32);
        for &k in kinds {
            w.b(k).i(4).i(4);
        }
        w.i(lanes.len() as i32);
        for l in lanes {
            w.i(l.len() as i32);
            for &(d, n, v) in l {
                w.i(d);
                match v {
                    Some(v) => w.b(n).b(v),
                    None => w.b(n | 0x80),
                };
            }
        }
        w.i(tracks.len() as i32);
        for tr in tracks {
            w.f(tr.vol).i(tr.recv.len() as i32);
            for &(s, c, v) in &tr.recv {
                w.i(s).i(c).f(v);
            }
            w.i(tr.devs.len() as i32);
            for &d in &tr.devs {
                w.i(d);
            }
            w.i(tr.lane).i(tr.autos.len() as i32);
            for (d, p, pts) in &tr.autos {
                w.i(*d).i(*p).i(pts.len() as i32);
                for &(dt, v) in pts {
                    w.i(dt).b(v);
                }
            }
        }
        w.0
    }

    fn with_player<R>(bytes: &[u8], sub: usize, f: impl FnOnce(&mut Player<'_, TD, Lcg>) -> R) -> R {
        let s = Song::parse(bytes).unwrap();
        let env = Env::for_song(&s, None);
        let need = Player::<TD, Lcg>::required_pool(&s, &env, sub).unwrap();
        let mut mem = vec![0u8; need];
        let mut pool = Pool::new(&mut mem);
        let mut p = Player::new(s, &env, &mut pool, Lcg(1), sub).unwrap();
        f(&mut p)
    }

    fn load_err(bytes: &[u8]) -> LoadError {
        let s = Song::parse(bytes).unwrap();
        let env = Env::for_song(&s, None);
        let mut mem = vec![0u8; 1 << 16];
        let mut pool = Pool::new(&mut mem);
        Player::<TD, Lcg>::new(s, &env, &mut pool, Lcg(1), 100).err().unwrap()
    }

    fn note_at_0() -> Vec<(i32, u8, Option<u8>)> {
        vec![(0, 60, Some(100))]
    }

    /// An owned, editable source (the live host's shape): absolute-frame events per track, routing as plain data.
    struct VecSource {
        tracks: Vec<VTrack>,
    }
    struct VTrack {
        devices: Vec<usize>,
        receives: Vec<Receive>,
        volume: f32,
        events: Vec<(i64, EventKind)>,
        pos: usize,
    }
    impl Source for VecSource {
        type Track<'s> = &'s mut VTrack;
        fn track_count(&self) -> usize {
            self.tracks.len()
        }
        fn track(&mut self, t: usize) -> &mut VTrack {
            &mut self.tracks[t]
        }
    }
    impl TrackSource for &mut VTrack {
        fn devices(&self) -> impl Iterator<Item = usize> + '_ {
            self.devices.iter().copied()
        }
        fn receives(&self) -> impl Iterator<Item = Receive> + '_ {
            self.receives.iter().copied()
        }
        fn volume(&self) -> f32 {
            self.volume
        }
        fn peek(&self) -> Option<(i64, EventKind)> {
            self.events.get(self.pos).copied()
        }
        fn advance(&mut self) {
            self.pos += 1;
        }
        fn automate(&mut self, _: usize, _: impl FnMut(usize, usize, f32)) {}
    }
    fn vec_source(s: &Song) -> VecSource {
        VecSource {
            tracks: s
                .tracks()
                .map(|t| {
                    let mut at = 0i64;
                    let events = s.lane(t.lane).unwrap().events().map(|(d, k)| {
                        at += d as i64;
                        (at, k)
                    });
                    VTrack { devices: t.devices().collect(), receives: t.receives().collect(), volume: t.volume, events: events.collect(), pos: 0 }
                })
                .collect(),
        }
    }

    #[test]
    fn owned_source_renders_like_the_player() {
        // instrument → probe (sidechain at 0.5) → master (last track), odd sub size, notes across sub-blocks, a
        // second instrument on its own lane fed straight into the master
        let mut probe = t(&[1], 1);
        probe.recv.push((0, 2, 0.5));
        let mut master = t(&[], 1);
        master.recv.push((1, 0, 1.0));
        master.recv.push((2, 0, 0.25));
        master.vol = 0.8;
        let lane = vec![(0, 60, Some(100)), (130, 60, None), (77, 64, Some(90)), (301, 64, None)];
        let lane2 = vec![(50, 48, Some(70)), (400, 48, None)];
        let b = song(&[1, 2, 1], &[lane, vec![], lane2], &[t(&[0], 0), probe, t(&[2], 2), master]);
        let mut want = vec![0.0f32; 2 * 1000];
        with_player(&b, 37, |p| p.render_f32(&mut want));

        let s = Song::parse(&b).unwrap();
        let env = Env::for_song(&s, None);
        let mut mem = vec![0u8; 1 << 16];
        let mut pool = Pool::new(&mut mem);
        let mut rng = Lcg(1);
        let devices = load_devices::<TD>(&s, &env, &mut pool, &mut rng).unwrap();
        let mut src = vec_source(&s);
        assert_eq!(Mixer::check(&mut src, devices.len()), Ok(()));
        let mut m = Mixer::new(&mut src, &mut pool, 37).unwrap();
        m.update_look(devices, &mut src);
        let mut got = vec![0.0f32; 2 * 1000];
        let mut i = 0;
        while i < 1000 {
            m.render_sub(devices, &mut src, 44100.0, 120, &mut rng);
            for j in 0..37.min(1000 - i) {
                (got[2 * (i + j)], got[2 * (i + j) + 1]) = m.master_frame(j);
            }
            i += 37;
        }
        assert!(want.iter().filter(|x| x.abs() > 0.1).count() > 500, "the reference is audible, not silence");
        assert_eq!(got, want);
    }

    #[test]
    fn mixer_check_rejects_bad_routing() {
        let b = song(&[1], &[vec![]], &[t(&[0], 0), t(&[], 0)]);
        let s = Song::parse(&b).unwrap();
        let ok = vec_source(&s);
        let bad = |f: &dyn Fn(&mut VecSource)| {
            let mut v = VecSource { tracks: ok.tracks.iter().map(|t| VTrack { devices: t.devices.clone(), receives: t.receives.clone(), volume: t.volume, events: vec![], pos: 0 }).collect() };
            f(&mut v);
            Mixer::check(&mut v, 1)
        };
        assert_eq!(bad(&|_| {}), Ok(()));
        assert_eq!(bad(&|v| v.tracks.clear()), Err(LoadError::Malformed));
        assert_eq!(bad(&|v| v.tracks[1].receives.push(Receive { track: 7, channel: 0, volume: 1.0 })), Err(LoadError::BadIndex));
        for ch in [1, 3, 4] {
            assert_eq!(bad(&|v| v.tracks[1].receives.push(Receive { track: 0, channel: ch, volume: 1.0 })), Err(LoadError::BadIndex), "channel {ch}");
        }
        assert_eq!(bad(&|v| v.tracks[1].receives.push(Receive { track: 1, channel: 0, volume: 1.0 })), Err(LoadError::BadIndex), "self receive");
        assert_eq!(bad(&|v| v.tracks[0].devices.push(1)), Err(LoadError::BadIndex), "device out of range");
    }

    #[test]
    fn send_into_sidechain() {
        let mut probe = t(&[1], 1);
        probe.recv.push((0, 2, 0.5));
        let mut master = t(&[], 1);
        master.recv.push((1, 0, 1.0));
        let b = song(&[1, 2], &[note_at_0(), vec![]], &[t(&[0], 0), probe, master]);
        with_player(&b, 100, |p| {
            let mut out = vec![0.0f32; 400];
            p.render_f32(&mut out);
            let TD::Probe { main, side, .. } = &p.devices()[1] else { panic!() };
            assert_eq!(side.len(), 200);
            assert!(main.iter().all(|&x| x == 0.0), "main input unaffected");
            // side = 0.5 × instrument main output = master output (master = probe main + side)
            for (i, &s) in side.iter().enumerate() {
                assert!(s > 0.5 && s < 0.5006, "frame {i}: {s}");
                assert_eq!(out[2 * i], s);
            }
        });
    }

    #[test]
    fn sending_track_renders_first() {
        // track 0 (probe) receives from track 1 (instrument) listed after it
        let mut probe = t(&[1], 1);
        probe.recv.push((1, 0, 1.0));
        let mut master = t(&[], 1);
        master.recv.push((0, 0, 1.0));
        let b = song(&[1, 2], &[note_at_0(), vec![]], &[probe, t(&[0], 0), master]);
        with_player(&b, 100, |p| {
            let mut out = vec![0.0f32; 200];
            p.render_f32(&mut out);
            let TD::Probe { main, .. } = &p.devices()[1] else { panic!() };
            assert!(main[0] >= 1.0, "instrument output visible in the same sub-block");
        });
    }

    #[test]
    fn receive_cycle_is_rejected() {
        let mut a = t(&[], 0);
        a.recv.push((1, 0, 1.0));
        let mut b = t(&[], 0);
        b.recv.push((0, 0, 1.0));
        let mut master = t(&[], 0);
        master.recv.push((0, 0, 1.0)); // the cycle feeds the master (an unused cycle is never rendered, as upstream)
        let bytes = song(&[], &[vec![]], &[a, b, master]);
        assert_eq!(load_err(&bytes), LoadError::BadIndex);
    }

    #[test]
    fn automation_param_out_of_range_is_rejected() {
        let mut tr = t(&[0], 0);
        tr.autos.push((0, 2, vec![(0, 0)])); // param 2 of a 2-param device
        let bytes = song(&[2], &[vec![]], &[tr]);
        assert_eq!(load_err(&bytes), LoadError::BadIndex);
    }

    #[test]
    fn automation_interpolates_at_sub_block_start() {
        let mut tr = t(&[0], 0);
        tr.autos.push((0, 0, vec![(0, 0), (1000, 255)]));
        let b = song(&[2], &[vec![]], &[tr]);
        with_player(&b, 100, |p| {
            let mut out = vec![0.0f32; 2 * 1200];
            p.render_f32(&mut out);
            let TD::Probe { params, .. } = &p.devices()[0] else { panic!() };
            let at = |f: u64| params.iter().find(|x| x.0 == f).unwrap().1;
            assert_eq!(at(0), 0.0);
            assert_eq!(at(500), 0.5);
            assert_eq!(at(1100), 1.0); // held after the last point
        });
    }

    #[test]
    fn automation_before_first_point_and_negative_times() {
        let mut tr = t(&[0], 0);
        tr.autos.push((0, 0, vec![(-500, 51), (1000, 255)])); // points at -500 and 500
        let b = song(&[2], &[vec![]], &[tr]);
        with_player(&b, 100, |p| {
            let mut out = vec![0.0f32; 2 * 600];
            p.render_f32(&mut out);
            let TD::Probe { params, .. } = &p.devices()[0] else { panic!() };
            let v0 = (51.0f64 / 255.0) as f32;
            let want = crate::math::mix(v0, 1.0, 500.0 / 1000.0); // frame 0 is halfway
            assert_eq!(params[0], (0, want));
            assert_eq!(params[5], (500, 1.0));
        });
    }

    #[test]
    fn output_independent_of_caller_buffer_size() {
        let lane = vec![(0, 60, Some(100)), (300, 60, None), (1234, 64, Some(1)), (99, 64, None)];
        let b = song(&[1], &[lane], &[t(&[0], 0)]);
        let whole = with_player(&b, 100, |p| {
            let mut out = vec![0.0f32; 2 * 3000];
            p.render_f32(&mut out);
            out
        });
        let chunked = with_player(&b, 100, |p| {
            let mut out = vec![];
            while out.len() < 2 * 3000 {
                let mut c = vec![0.0f32; 2 * 64];
                p.render_f32(&mut c);
                out.extend(c);
            }
            out.truncate(2 * 3000);
            out
        });
        assert_eq!(whole, chunked);
        assert!(whole[0] >= 1.0 && whole[2 * 299] >= 1.0 && whole[2 * 300] == 0.0 && whole[2 * 1534] >= 1.0);
    }

    #[test]
    fn seek_equals_render_from_start() {
        let lane = vec![(0, 60, Some(100)), (900, 60, None), (100, 62, Some(9))];
        let b = song(&[1], &[lane], &[t(&[0], 0)]);
        let full = with_player(&b, 100, |p| {
            let mut out = vec![0.0f32; 2 * 2000];
            p.render_f32(&mut out);
            out
        });
        for f in [0usize, 777, 1000, 1999] {
            let part = with_player(&b, 100, |p| {
                let mut warm = vec![0.0f32; 2 * 1500]; // play past the target first
                p.render_f32(&mut warm);
                p.seek(f as u64);
                let mut out = vec![0.0f32; 2 * (2000 - f)];
                p.render_f32(&mut out);
                out
            });
            assert_eq!(part, full[2 * f..], "seek {f}");
        }
    }

    #[test]
    fn lookahead_delivers_early_once() {
        let b = song(&[3], &[vec![(1023, 60, Some(1))]], &[t(&[0], 0)]);
        with_player(&b, 100, |p| {
            let mut out = vec![0.0f32; 2 * 2000];
            p.render_f32(&mut out);
            let TD::Look { log } = &p.devices()[0] else { panic!() };
            assert_eq!(*log, vec![(500, 523, EventKind::NoteOn { note: 60, velocity: 1 })]);
        });
    }

    #[test]
    fn track_volume_and_i16_output() {
        let mut tr = t(&[0], 0);
        tr.vol = 0.5;
        let b = song(&[1], &[note_at_0()], &[tr]);
        with_player(&b, 100, |p| {
            let mut out = vec![0i16; 2 * 10];
            p.render_i16(&mut out);
            // (1 + 1e-3·r)·0.5 → trunc(x·32767)
            assert!(out[0] >= 16383 && out[0] <= 16400, "{}", out[0]);
            assert_eq!(out[1], -out[0]);
        });
    }

    #[test]
    fn length_frames_from_header() {
        let b = song(&[], &[vec![]], &[t(&[], 0)]);
        with_player(&b, 100, |p| assert_eq!(p.length_frames(), 44100));
    }
}

/// Loads every `$WS_SONG_DIR` song into the player with a pass-through stand-in for every kind and renders 1 s.
#[test]
fn real_songs_load_and_render() {
    use crate::device::*;
    use crate::mix::*;
    use crate::pool::Pool;
    let Ok(dir) = std::env::var("WS_SONG_DIR") else {
        eprintln!("WS_SONG_DIR not set: skipped");
        return;
    };
    struct Any;
    impl Device for Any {
        fn param_count(&self) -> usize {
            256
        }
        fn set_param(&mut self, _: usize, _: f32) {}
        fn reset(&mut self) {}
        fn run(&mut self, _: &mut Io, _: usize, _: &mut Ctx) {}
    }
    impl<'a> Kinds<'a> for Any {
        fn need(k: u8, _: &'a [u8], _: &Env<'a>) -> Result<usize, LoadError> {
            if k <= 12 { Ok(0) } else { Err(LoadError::UnsupportedKind(k)) }
        }
        fn load(k: u8, c: &'a [u8], e: &Env<'a>, _: &mut Pool<'a>, _: &mut dyn crate::rng::Rng) -> Result<Self, LoadError> {
            Self::need(k, c, e).map(|_| Any)
        }
    }
    #[derive(Clone)]
    struct Zero;
    impl crate::rng::Rng for Zero {
        fn next_f32(&mut self) -> f32 {
            0.0
        }
    }
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "bin") {
            continue;
        }
        let b = std::fs::read(&p).unwrap();
        let s = Song::parse(&b).unwrap();
        let env = Env::for_song(&s, None);
        let need = Player::<Any, Zero>::required_pool(&s, &env, DEFAULT_SUB).unwrap();
        let mut mem = vec![0u8; need];
        let mut pool = Pool::new(&mut mem);
        let mut pl = Player::<Any, Zero>::new(s, &env, &mut pool, Zero, DEFAULT_SUB)
            .unwrap_or_else(|e| panic!("{}: {e:?}", p.display()));
        let mut out = vec![0.0f32; 2 * 44100];
        pl.render_f32(&mut out);
    }
}

// ---- mix: Either facade ----
mod either_tests {
    use crate::device::*;
    use crate::mix::*;
    use crate::pool::Pool;

    /// Engine with kinds `LO..=HI`; renders its kind id as a constant so tests can see which engine built it.
    struct Eng<const LO: u8, const HI: u8>(u8, u32);
    impl<const LO: u8, const HI: u8> Device for Eng<LO, HI> {
        fn param_count(&self) -> usize {
            1
        }
        fn set_param(&mut self, _: usize, _: f32) {}
        fn event_lookahead(&self) -> u32 {
            self.1
        }
        fn reset(&mut self) {}
        fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
            io.ch[MAIN_L][..n].fill(self.0 as f32);
        }
    }
    impl<'a, const LO: u8, const HI: u8> Kinds<'a> for Eng<LO, HI> {
        fn need(k: u8, _: &'a [u8], _: &Env<'a>) -> Result<usize, LoadError> {
            if (LO..=HI).contains(&k) { Ok(8) } else { Err(LoadError::UnsupportedKind(k)) }
        }
        fn load(k: u8, c: &'a [u8], e: &Env<'a>, _: &mut Pool<'a>, _: &mut dyn crate::rng::Rng) -> Result<Self, LoadError> {
            Self::need(k, c, e).map(|_| Eng(k, if k >= 32 { 511 } else { 0 }))
        }
    }

    #[derive(Clone)]
    struct Zero;
    impl crate::rng::Rng for Zero {
        fn next_f32(&mut self) -> f32 {
            0.0
        }
    }

    type Mixed = Either<Eng<0, 12>, Eng<32, 39>>;

    /// Engine that owns kind 5 but cannot build it: a module of the kind is compiled out.
    struct NoModule;
    impl Device for NoModule {
        fn param_count(&self) -> usize {
            0
        }
        fn set_param(&mut self, _: usize, _: f32) {}
        fn reset(&mut self) {}
        fn run(&mut self, _: &mut Io, _: usize, _: &mut Ctx) {}
    }
    impl<'a> Kinds<'a> for NoModule {
        fn need(k: u8, _: &'a [u8], _: &Env<'a>) -> Result<usize, LoadError> {
            if k == 5 { Err(LoadError::UnsupportedModule(9)) } else { Err(LoadError::UnsupportedKind(k)) }
        }
        fn load(k: u8, c: &'a [u8], e: &Env<'a>, _: &mut Pool<'a>, _: &mut dyn crate::rng::Rng) -> Result<Self, LoadError> {
            Self::need(k, c, e).map(|_| NoModule)
        }
    }

    #[test]
    fn unsupported_module_is_not_a_missing_kind() {
        type M = Either<NoModule, Eng<0, 12>>;
        let env = Env { rate: 44100.0, tempo: 120, bank: None };
        // kind 5 exists in A, so B (which would accept it) is never consulted
        assert_eq!(M::need(5, &[], &env), Err(LoadError::UnsupportedModule(9)));
        assert_eq!(M::need(6, &[], &env), Ok(8));
        let mut mem = [0u8; 64];
        let mut pool = Pool::new(&mut mem);
        assert_eq!(M::load(5, &[], &env, &mut pool, &mut Zero).err(), Some(LoadError::UnsupportedModule(9)));
    }

    #[test]
    fn dispatches_by_kind_and_forwards() {
        let env = Env { rate: 44100.0, tempo: 120, bank: None };
        assert_eq!(Mixed::need(5, &[], &env), Ok(8));
        assert_eq!(Mixed::need(37, &[], &env), Ok(8));
        assert_eq!(Mixed::need(20, &[], &env), Err(LoadError::UnsupportedKind(20)));
        let mut mem = [0u8; 64];
        let mut pool = Pool::new(&mut mem);
        let a = Mixed::load(3, &[], &env, &mut pool, &mut Zero).unwrap();
        let b = Mixed::load(37, &[], &env, &mut pool, &mut Zero).unwrap();
        assert!(matches!(a, Either::A(_)) && matches!(b, Either::B(_)));
        assert_eq!((a.event_lookahead(), b.event_lookahead()), (0, 511));
        let (mut l, mut r, mut s0, mut s1) = ([0.0f32; 4], [0.0f32; 4], [0.0f32; 4], [0.0f32; 4]);
        let mut rng = Zero;
        let mut ctx = Ctx { rate: 44100.0, tempo: 120, frame: 0, rng: &mut rng };
        let mut b = b;
        b.run(&mut Io { ch: [&mut l, &mut r, &mut s0, &mut s1] }, 4, &mut ctx);
        assert_eq!(l, [37.0; 4]);
    }
}
