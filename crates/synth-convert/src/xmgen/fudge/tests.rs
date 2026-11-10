use super::*;
use serde_json::json;
use unisynth::osc::engine::{DrumT, EngineSlot, EngineT, ToneT};
use unisynth::params::old::{o, osc};

/// Fudge's committed map.
fn map() -> super::super::Map {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/xm-gen/maps/fudge-asc.json");
    super::super::load(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The engine settings an entry's instrument holds (the shape values of slot 1, decoded by the slot's laws).
fn settings(engine: &str, params: &Value) -> EngineT {
    let v = match engine {
        "fm-shaper" => ShaperTone.instrument(params, 44100.0),
        _ => SweepDrum.instrument(params, 44100.0),
    };
    let sh: [f32; 9] = core::array::from_fn(|k| v[osc(0, o::SHAPE + k)]);
    match engine {
        "fm-shaper" => EngineT::Tone(ToneT::from_shape(&sh)),
        _ => EngineT::Drum(DrumT::from_shape(&sh)),
    }
}

fn render(e: &EngineT, start: i64, back: bool, n: usize) -> Vec<i32> {
    let mut s = EngineSlot::new();
    s.seek(e, start, back);
    (0..n).map(|_| (s.next(e) * 32768.0) as i32).collect()
}

#[test]
fn every_fudge_record_converts() {
    let m = map();
    let mut n = 0;
    for e in &m.samples {
        let Some(g) = &e.generated else { continue };
        let en: &dyn Engine = if g.engine == "fm-shaper" { &ShaperTone } else { &SweepDrum };
        en.validate(&g.params).unwrap_or_else(|err| panic!("instrument {}: {err}", e.instrument));
        n += 1;
    }
    assert_eq!(n, 22);
}

#[test]
fn integer_flags_are_read_as_integers() {
    // instrument 4: FM on as the raw word 1 (a denormal as a float); instrument 1: FM off
    let m = map();
    let p = |i: usize| &m.samples[i - 1].generated.as_ref().unwrap().params;
    assert!(matches!(settings("fm-shaper", p(4)), EngineT::Tone(t) if t.fm));
    assert!(matches!(settings("fm-shaper", p(1)), EngineT::Tone(t) if !t.fm));
    // drum 8's curve 1 (1 − sin), drum 11's 0 (linear)
    assert!(matches!(settings("sweep-drum", p(8)), EngineT::Drum(d) if d.curve == 1));
    assert!(matches!(settings("sweep-drum", p(11)), EngineT::Drum(d) if d.curve == 0));
}

#[test]
fn values_outside_the_laws_are_named() {
    let drum = |accel: u32, a8: u32| json!({"frames": 256, "args": [1123942400u32, 1036831949u32, 966609234u32, accel, 0, 0, 1065353216u32, 1065353216u32, a8]});
    // a positive acceleration (a slowing sweep) has no law
    let e = SweepDrum.validate(&drum(0.001f32.to_bits(), 0)).unwrap_err();
    assert!(e.contains("argument 3") && e.contains("positive"), "{e}");
    assert!(SweepDrum.validate(&drum(0, 3)).unwrap_err().contains("argument 8"));
    // a carrier period that a power-of-two law cannot hold
    let e = ShaperTone.validate(&json!({"frames": 16, "args": [2000.0f32.to_bits(), 1124073472u32, 1065353216u32, 0, 1121714176u32, 0, 0, 0, 1065353216u32]})).unwrap_err();
    assert!(e.contains("argument 0") && e.contains("carrier"), "{e}");
    assert!(ShaperTone.validate(&json!({"frames": 16, "args": [1]})).unwrap_err().contains("9"));
}

#[test]
fn the_clock_is_the_slot_pitch_times_256() {
    use unisynth::law;
    for clock in [1100.0f64, 8363.0, 35441.159 * 0.5, 44100.0, 66977.0, 180_000.0, 2.0e6] {
        let v = instrument(&[], clock);
        let st = law::coarse(v[osc(0, o::COARSE)]) + law::fine(v[osc(0, o::FINE)]) + 12.0 * law::octave(v[osc(0, o::OCTAVE)]);
        let hz = 440.0 * 2f64.powf((60.0 + st as f64 - 69.0) / 12.0);
        assert!((256.0 * hz / clock - 1.0).abs() < 2e-7, "{clock}: {}", 256.0 * hz);
    }
}

/// Spec "Fudge per-sample parity": each generated sample against the reference XM (the frames the original
/// generated), at most 1 LSB off on ≥ 99.9 % of frames and no frame more than 64; a ping-pong loop's backward pass
/// from its end is the forward run reversed.
#[test]
fn every_generated_sample_matches_the_reference() {
    let zenekk = std::env::var("ZENEKK_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Downloads/zenekk"));
    let Ok(bytes) = std::fs::read(zenekk.join("xm-gen/fudge-asc.reference.xm")) else {
        eprintln!("skip: the reference XM is absent");
        return;
    };
    let x = crate::xm::parse(&bytes).unwrap();
    let m = map();
    let mut report = vec![];
    for e in &m.samples {
        let Some(g) = &e.generated else { continue };
        let s = &x.instruments[e.instrument - 1].samples[e.sample];
        let eng = settings(&g.engine, &g.params);
        let got = render(&eng, 0, false, s.data.len());
        let diffs: Vec<i32> = got.iter().zip(&s.data).map(|(a, b)| (a - *b as i32).abs()).collect();
        let off = diffs.iter().filter(|&&d| d > 1).count();
        let worst = diffs.iter().copied().max().unwrap_or(0);
        report.push(format!("instrument {:2} {:10} {:6} frames: {off} over 1 LSB, worst {worst}", e.instrument, g.engine, s.data.len()));
        assert!(off as f64 <= s.data.len() as f64 * 0.001 && worst <= 64, "instrument {}: {off} frames over 1 LSB, worst {worst}", e.instrument);
        if s.loop_kind == 2 {
            let end = (s.loop_start + s.loop_len) as i64 - 1;
            let n = (s.loop_len as usize).min(4096);
            let back = render(&eng, end, true, n);
            let fwd: Vec<i32> = (0..n).map(|k| got[end as usize - k]).collect();
            assert_eq!(back, fwd, "instrument {}: backward pass from {end}", e.instrument);
        }
    }
    eprintln!("{}", report.join("\n"));
}

/// Task 3.6 (`performance-optimization`): the worst note-on of each engine over every start position the byte
/// contract can carry inside the samples (each 16th frame), forwards and backwards, its first frame included (a drum's
/// backward block fills there). Release build: `cargo test --release -p synth-convert --lib note_on_budget -- --ignored
/// --nocapture`.
#[test]
#[ignore]
fn note_on_budget() {
    let m = map();
    let (mut worst, mut at) = (0.0f64, String::new());
    for e in &m.samples {
        let Some(g) = &e.generated else { continue };
        let eng = settings(&g.engine, &g.params);
        for start in (0..g.length as i64).step_by(16) {
            for back in [false, true] {
                let mut s = EngineSlot::new();
                let t = std::time::Instant::now();
                s.seek(&eng, start, back);
                std::hint::black_box(s.next(&eng));
                let us = t.elapsed().as_secs_f64() * 1e6;
                if us > worst {
                    (worst, at) = (us, format!("instrument {} start {start} back {back}", e.instrument));
                }
            }
        }
    }
    eprintln!("worst note-on {worst:.1} µs ({at})");
    assert!(worst < 50.0, "the 50 µs budget (design D4)");
}
