use super::*;

/// Fudge's tone 17 (FM on, shape ½) and drum 9 (falling sweep, `1 − sin` envelope), on the D3 laws.
fn tone() -> ToneT {
    ToneT { carrier: 128.0, modulator: 128.0, depth: 2.5, amp: f32t(110.0 * 255.0), drive: 0.0, shape: 0.5, fm: true, rate: 1.0 }
}
fn drum() -> DrumT {
    DrumT { amp: f32t(127.0 * 255.0), freq: 0.1, sweep: 0.03, accel: -0.0013, drive: 0.0, shape: 0.0, env: 1.0, slope: 1.0 / 1556.0, curve: 1 }
}

fn forward(e: &EngineT, n: usize) -> Vec<f32> {
    let mut s = EngineSlot::new();
    s.seek(e, 0, false);
    (0..n).map(|_| s.next(e)).collect()
}

#[test]
fn stores_truncate_toward_zero() {
    for x in [0.1f64, -0.1, 1.0 / 3.0, -2.0 / 3.0, 12345.678, 1e-12] {
        let r = f32t(x);
        assert!((r as f64).abs() <= x.abs(), "{x}");
        assert!(x.abs() - (r as f64).abs() < x.abs() * 2f64.powi(-23), "{x}");
    }
    assert_eq!(f32t(0.5), 0.5);
}

#[test]
fn the_laws_give_back_fudge_values_exactly() {
    for x in [32.0f32, 64.0, 128.0, 16.0, 256.0] {
        assert_eq!(law::tone_period(x / 1024.0), x);
    }
    for x in [0.05f32, 1.0, 2.5, 30.0] {
        assert_eq!(law::tone_depth(x / 64.0), x);
    }
    assert_eq!(law::tone_volume(110.0 / 256.0), 110.0);
    assert_eq!(law::tone_rate(2.0 / 16.0), 2.0);
    assert_eq!(law::drum_volume(257.0 / 512.0), 257.0);
    assert_eq!(law::drum_freq(0.11f32 / 4.0), 0.11);
    let g = f32::from_bits(0xb75a1a93).abs();
    assert_eq!(law::drum_growth(g * 64.0), g);
    // the slope's affine law comes back within half its step, 2⁻²⁸ per frame (design D3)
    for x in [0.7f64 / 1380.0, -1.0 / 1556.0, 0.0] {
        let p = ((x * 16.0 + 1.0) / 2.0) as f32;
        assert!((law::drum_slope(p) - x).abs() <= 2f64.powi(-28), "{x}");
    }
}

#[test]
fn the_tone_drive_and_fold() {
    let t = ToneT { drive: 0.5, shape: 0.0, ..tone() };
    assert_eq!(t.drive_stage(1000), (2500, Some(4000)));
    // above the high threshold: e = (20000 − 16320)/4 = 920, (16320 − 920)·127/80 = 24447
    assert_eq!(t.drive_stage(20000), (20000, Some(24447)));
    assert_eq!(t.drive_stage(-20000), (-20000, Some(-24447)));
    // between the thresholds nothing is set
    assert_eq!(t.drive_stage(10000), (10000, None));
    // the shape blend reads the carried value
    let t = ToneT { shape: 0.5, ..t };
    assert_eq!(t.frame(0, 999).0, 0);
}

#[test]
fn frame_0_resets_the_carried_value() {
    let t = tone();
    let run = |c0: i64| {
        let mut c = c0;
        (0..2000).map(|i| {
            let (y, n) = t.frame(i, c);
            c = n;
            y
        }).collect::<Vec<_>>()
    };
    assert_eq!(run(0), run(12345));
    assert_eq!(t.frame(0, 12345).0, 0);
    let d = drum();
    let mut a = DrumState { carried: -19747.3, ..d.state0() };
    let mut b = d.state0();
    for i in 0..1556 {
        assert_eq!(d.frame(i, &mut a), d.frame(i, &mut b));
    }
}

#[test]
fn a_seek_equals_a_run_from_frame_0() {
    for e in [EngineT::Tone(tone()), EngineT::Tone(ToneT { fm: false, depth: 30.0, ..tone() }), EngineT::Drum(drum())] {
        let all = forward(&e, 1556);
        for start in [1i64, 17, 255, 800, 1500] {
            let mut s = EngineSlot::new();
            s.seek(&e, start, false);
            let got: Vec<f32> = (0..50).map(|_| s.next(&e)).collect();
            let n = got.len().min(all.len() - start as usize);
            assert_eq!(&got[..n], &all[start as usize..start as usize + n], "{e:?} from {start}");
        }
    }
}

#[test]
fn a_backward_pass_is_the_forward_run_reversed() {
    for e in [EngineT::Tone(tone()), EngineT::Drum(drum())] {
        let all = forward(&e, 300);
        let mut s = EngineSlot::new();
        s.seek(&e, 255, true);
        let got: Vec<f32> = (0..258).map(|_| s.next(&e)).collect();
        let want: Vec<f32> = (0..=255).rev().map(|i| all[i]).chain([all[0], all[0]]).collect();
        assert_eq!(got, want, "{e:?}");
    }
}

#[test]
fn the_envelope_curves() {
    // a full-scale constant: volume past the clip, frequency ½π so frame 1's sine is 1 (sweep 0)
    let base = DrumT { amp: 40000.0, freq: core::f32::consts::FRAC_PI_2, sweep: 0.0, accel: 0.0, drive: 0.0, shape: 0.0, env: 0.5, slope: 0.0, curve: 0 };
    let at1 = |curve: u8| {
        let d = DrumT { curve, ..base };
        let mut st = d.state0();
        d.frame(0, &mut st);
        d.frame(1, &mut st)
    };
    let s = 32385.0f64;
    assert_eq!(at1(0) as f64, (s * 0.5).trunc());
    assert_eq!(at1(1) as f64, f32t((1.0 - libm::sin(HALF_PI - 0.5 * HALF_PI)) * s).trunc() as f64);
    assert_eq!(at1(2) as f64, f32t(libm::cos(HALF_PI - 0.5 * HALF_PI) * s).trunc() as f64);
}

#[test]
fn a_rising_envelope_holds_at_1() {
    let d = DrumT { env: 0.0, slope: -1.0 / 256.0, curve: 0, ..drum() };
    let mut st = d.state0();
    let e = |i: i64| (f32t(d.env as f64 - i as f64 * d.slope) as f64).clamp(0.0, 1.0);
    assert_eq!(e(0), 0.0);
    assert_eq!(e(1000), 1.0);
    assert_eq!(d.frame(0, &mut st), 0.0);
}

#[test]
fn the_note_on_bytes() {
    assert_eq!(start_of(0.0, 0.0, 0.0), (0, false));
    assert_eq!(start_of(1.0 / 255.0, 2.0 / 255.0, 128.0 / 255.0), (4096 + 32, true));
    assert_eq!(start_of(1.0, 1.0, 127.0 / 255.0), (255 * 4096 + 255 * 16, false));
}

#[test]
fn a_drum_state_from_the_sweep_alone_is_the_full_replays() {
    for d in [drum(), DrumT { drive: 0.5, shape: 0.5, ..drum() }, DrumT { amp: f32t(257.0 * 255.0), freq: 0.11, sweep: 4e-5, accel: 0.0, shape: 0.5, ..drum() }] {
        let mut full = d.state0();
        for i in 0..1600 {
            assert_eq!(d.state_at(i), full, "{d:?} at {i}");
            d.frame(i, &mut full);
        }
    }
}

#[test]
fn a_period_of_0_plays_silence() {
    // the phase is NaN; the integer conversion gives 0 (the DAW drawing mirrors it)
    let t = ToneT { modulator: 0.0, carrier: 0.0, ..tone() };
    let mut c = 0;
    for i in 0..64 {
        let (y, n) = t.frame(i, c);
        assert_eq!(y, 0, "frame {i}");
        c = n;
    }
}
