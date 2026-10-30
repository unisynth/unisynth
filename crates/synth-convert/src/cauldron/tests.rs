use super::*;

pub(crate) fn data(name: &str) -> String {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../research/tbl-mini-cauldron/ref/");
    std::fs::read_to_string(format!("{p}{name}")).unwrap()
}

#[test]
fn both_patch_files_load_with_their_record_counts() {
    let j = load(&data("patches_jizz.json")).unwrap();
    assert_eq!(j.calls.len(), 1);
    assert_eq!((j.calls[0].name.as_str(), j.calls[0].version, j.calls[0].records.len()), ("jizz", Version::V1, 12));
    let order: Vec<usize> = j.calls[0].order.iter().map(|s| if let Slot::Render { record, .. } = s { *record } else { 99 }).collect();
    assert_eq!(order, [10, 11, 8, 9, 5, 6, 3, 7, 0, 2, 4, 1]);
    let s = load(&data("patches_stash.json")).unwrap();
    let counts: Vec<(&str, usize, usize, u8)> = s.calls.iter().map(|c| (c.name.as_str(), c.records.len(), c.drum_count, c.flag)).collect();
    assert_eq!(counts, [("bluish", 10, 6, 0), ("song0", 16, 7, 1)]);
    assert_eq!(s.call("song0").unwrap().order[1], Slot::Drums);
    assert_eq!(s.call("song0").unwrap().order[16], Slot::Render { slot: 16, record: 15 });
    assert_eq!(s.drum_lengths, [2950, 1128, 1284, 4104, 1456, 3419, 313]);
}

#[test]
fn jizz_record_0_has_the_values_of_the_spec_table() {
    let j = load(&data("patches_jizz.json")).unwrap();
    let r = &j.calls[0].records[0];
    let close = |a: f64, b: f64| (a - b).abs() < 1e-12;
    assert!(close(r.a, 0.01) && close(r.b, 0.005) && close(r.decay, 0.998) && close(r.damp, 0.1));
    assert!(close(r.mul, 1.1) && close(r.gain, 1.0) && close(r.echo, 0.3));
    assert_eq!((r.wave, r.voices, r.variants, r.decim), (0, 3, 4, 2));
    assert_eq!(r.pitch[..4], [199, 211, 199, 199]);
    assert_eq!(r.tie[..4], [0, 0, 0, 1]);
}

#[test]
fn other_json_is_rejected_naming_the_problem() {
    assert!(load("{}").unwrap_err().contains("neither a v1 file"));
    assert!(load("not json").unwrap_err().starts_with("patch data:"));
    assert!(load(r#"{"records": [{"A": 1}], "order": [0]}"#).unwrap_err().contains("missing number `B`"));
}

#[test]
fn retriggers_follow_the_reference_trace() {
    // tests/data/cauldron_retriggers.json: research/tbl-mini-cauldron/tools/retrigger_trace.py (the bit-exact
    // references' retrigger calls, render order, step -1 = a variant's first call)
    let trace: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/cauldron_retriggers.json")).unwrap()).unwrap();
    let j = load(&data("patches_jizz.json")).unwrap();
    let s = load(&data("patches_stash.json")).unwrap();
    let mut checked = 0;
    for call in [&j.calls[0], s.call("bluish").unwrap(), s.call("song0").unwrap()] {
        for entry in trace[&call.name].as_array().unwrap() {
            let rec = &call.records[entry["record"].as_u64().unwrap() as usize];
            let mut k = 0;
            for c in entry["calls"].as_array().unwrap() {
                let c: Vec<f64> = c.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
                k += 1;
                if c[1] < 0.0 {
                    continue;
                }
                let r = rec.retrig(c[0] as usize, c[1] as usize).expect("a traced step is untied");
                let (v1, v2) = r.working(call.version);
                let rel = |x: f64, y: f64| (x - y).abs() <= 1e-6 * y.abs().max(1e-30);
                assert_eq!(r.k, k, "{} variant {} step {}", call.name, c[0], c[1]);
                assert!(rel(r.a, c[2]) && rel(r.b, c[3]) && rel(v1, c[4]) && rel(v2, c[5]), "{} {c:?} vs {r:?}", call.name);
                checked += 1;
            }
        }
    }
    assert!(checked > 300, "{checked}");
}

#[test]
fn tied_steps_do_not_retrigger() {
    let j = load(&data("patches_jizz.json")).unwrap();
    let r = &j.calls[0].records[0];
    assert_eq!(r.retrig(0, 3), None, "step 3 is tied");
    assert_eq!(r.retrig(0, 0).unwrap().k, 2, "the variant's own retrigger, then step 0's");
}

mod instrument {
    use super::super::*;
    use super::data;
    use unisynth::law;
    use unisynth::osc::CauldronT;
    use unisynth::params::{self as pm, o, shape as sp, v};

    fn all() -> Vec<(Version, Record)> {
        let mut out = vec![];
        for f in ["patches_jizz.json", "patches_stash.json"] {
            for c in load(&data(f)).unwrap().calls {
                out.extend(c.records.into_iter().map(|r| (c.version, r)));
            }
        }
        out
    }

    fn shapes(p: &[f32; pm::COUNT]) -> [f32; pm::o::SHAPES] {
        core::array::from_fn(|k| p[pm::osc(0, o::SHAPE + k)])
    }

    #[test]
    fn each_retrigger_gives_the_reference_filter_values() {
        // the slot compounds base and amount from the note's retrigger byte: equal to Record::retrig (the reference
        // trace) for every untied step of every variant
        for (version, r) in all() {
            let p = instrument(version, &r, 1.0);
            for variant in 0..r.variants as usize {
                for step in 0..16 {
                    let Some(rt) = r.retrig(variant, step) else { continue };
                    let mut sh = shapes(&p);
                    sh[sp::CD_RETRIG] = note_byte(&rt) as f32 / 255.0;
                    let f = CauldronT::from_shape(&sh, R as f32, p[pm::osc(0, o::FEEDBACK)], 0.0, 0.0).flt.unwrap();
                    let (a, b) = rt.working(version);
                    // mul and A/B pass through f32 chunk values (v1 keeps doubles): ~1e-7 per compounding step
                    let rel = |x: f64, y: f64| (x - y).abs() <= 1e-5 * y.abs();
                    assert!(rel(f.a, a) && rel(f.b, b), "{version:?} variant {variant} step {step}: {f:?} vs {a} {b}");
                }
            }
        }
    }

    #[test]
    fn voice_settings_follow_the_record_at_the_ratio() {
        for (version, r) in all() {
            for ratio in [1.0, 2.0] {
                let p = instrument(version, &r, ratio);
                let clock = R * ratio;
                let sh = shapes(&p);
                let t = CauldronT::from_shape(&sh, clock as f32, p[pm::osc(0, o::FEEDBACK)], 0.0, 0.0);
                let f = t.flt.unwrap();
                let close = |x: f64, y: f64, tol: f64| (x - y).abs() <= tol * y.abs().max(1e-12);
                assert_eq!((t.wave, t.voices, t.v2), (r.wave, r.voices, version == Version::V2));
                assert!(close(t.clock as f64, 1.0, 1e-6), "clock {}", t.clock);
                assert!(close(f.d, r.decay, 1e-6) && close(f.q1, 1.0 - r.damp, 1e-6), "{f:?}");
                assert!(close(law::gain(p[pm::osc(0, o::GAIN)]) as f64, r.gain, 1e-5));
                assert_eq!(law::choice(p[v::MODE], 2), 1, "mono legato");
                assert_eq!(law::count(p[v::CAPACITY], 1, 64), 1, "one voice: a retrigger restarts it");
                assert!(close(law::time(p[v::SLIDE]) as f64, GLIDE / clock, 1e-4), "slide");
                // the note end: a quadratic release with the energy of the original's linear fade
                let fade = if version == Version::V1 { (STEP - 1) as f64 } else { r.release as f64 };
                let rel_ms = law::env_ms(p[pm::env(0, pm::e::RELEASE)]) as f64;
                assert!(close(rel_ms, (5.0 / 3.0 * fade / clock * 1000.0).max(1.0), 1e-4), "release {rel_ms}");
                assert_eq!(p[v::VELOCITY], 0.0, "the velocity is the phrase position, not a level");
                // the voice: a centred slot pans at √½ per channel, the master makes it up, and no voice clip (the
                // clip is the track's first effect, after the note-end release, as the original fades before its clamp)
                let (pan, _) = law::pan(0.5);
                assert!((law::gain(p[v::MASTER]) * pan - 1.0).abs() < 1e-5, "unity per channel");
                assert_eq!(p[pm::VDIST + pm::dist::MODE], 0.0, "no voice distortion");
            }
        }
    }

    #[test]
    fn note_data_bytes() {
        let rt = Retrig { k: 5, a: 0.0, b: 0.0, accent: true };
        assert_eq!(note_byte(&rt), 11);
        assert_eq!(velocity(0.0), 1);
        assert_eq!(velocity(5.0 * STEP as f64), 41, "a step start is exact");
        assert_eq!(velocity(1e9), 127, "clamped");
    }
}

mod effects {
    use super::super::*;
    use super::data;
    use unisynth::fx::mode_value;
    use unisynth::kind;

    const RATE: f64 = 44100.0;
    const TEMPO: i32 = 151;

    fn chain(call: &str, record: usize, ratio: f64) -> Chain {
        let f = if call == "jizz" { "patches_jizz.json" } else { "patches_stash.json" };
        let p = load(&data(f)).unwrap();
        let c = p.call(call).unwrap();
        effects(c, record, ratio, RATE, TEMPO)
    }

    fn kinds(c: &Chain) -> Vec<(u8, f32)> {
        c.devices.iter().map(|(k, v)| (*k, v[0])).collect()
    }

    /// The echo line's length in frames, through the tempo mode's laws (coarse `trunc(16v)` quarter beats, fine
    /// `trunc(200v)` ms, `trunc(ms·rate/1000)`).
    fn echo_frames(v: &[f32]) -> usize {
        let unit = 120.0 / TEMPO as f64 / 8.0 * 1000.0;
        let ms = (v[1] * 16.0) as i32 as f64 * unit + (v[2] * 200.0) as i32 as f64;
        (ms as f32 as f64 * RATE / 1000.0) as usize
    }

    #[test]
    fn the_chain_keeps_the_original_order() {
        let (fl, de, di, eq) = (kind::FLANGER, kind::DELAY, kind::DISTORTION, kind::EQ);
        let (comb, tempo, wrap) = (mode_value(fl, 2), mode_value(de, 0), mode_value(di, 3));
        // the bandwidth low-pass is a tempo delay too: zero time, fully wet, no feedback (its SVF high cut)
        let lp = (de, tempo);
        let _ = eq;
        let clip = (di, mode_value(di, 0));
        // 16-bit clamp → comb → 8-bit clamp → echo → 8-bit clamp → doubler → (specials) → bandwidth low-pass (2 stages)
        assert_eq!(kinds(&chain("song0", 3, 1.0)), [clip, (fl, comb), clip, (de, tempo), clip, (fl, comb), lp, lp]);
        assert_eq!(kinds(&chain("song0", 8, 1.0)), [clip, (fl, comb), clip, (de, tempo), clip, (fl, comb), (di, wrap)], "slot 9: ×1.25 wrap");
        assert_eq!(kinds(&chain("song0", 10, 1.0)), [clip, (de, tempo), clip, (fl, comb), (di, wrap), lp, lp], "slot 11: +48 wrap");
        assert_eq!(kinds(&chain("bluish", 8, 1.0)), [clip, (fl, comb), clip, (de, tempo), clip, (fl, comb)], "the specials are song0's only");
        assert_eq!(kinds(&chain("jizz", 0, 1.0)), [clip, (de, tempo), clip, lp, lp]);
        // the clip: the shaper's clipper at unity drive and threshold 1, fully wet, no foldover or oversampling
        let c = chain("jizz", 0, 1.0);
        assert_eq!(c.devices[0].1, [mode_value(di, 0), 0.2, 1.0, 0.0, 1.0, 0.0, 0.0]);
        // after the echo it clamps at the original's full scale, which the echo's wet mix has scaled by 1/(1 + e)
        assert_eq!(c.devices[2].1, [mode_value(di, 0), 0.2, c.level as f32, 0.0, 1.0, 0.0, 0.0]);
        assert!(c.level < 1.0);
    }

    #[test]
    fn times_follow_the_ratio() {
        for ratio in [1.0, 2.0] {
            let c = chain("song0", 3, ratio);
            let kappa = R * ratio / RATE; // phrase frames per output frame
            // comb: k = 150 phrase frames
            let k = (c.devices[1].1[1] as f64 * 16384.0).round();
            assert_eq!(k, (150.0 / kappa).round(), "comb at ratio {ratio}");
            assert_eq!(c.devices[1].1[3], 0.0, "no sweep");
            // echo: 3 steps of 3529 phrase frames, within half a millisecond
            let want = 3.0 * 3529.0 / kappa;
            let got = echo_frames(&c.devices[3].1) as f64;
            assert!((got - want).abs() <= 0.5e-3 * RATE + 1.0, "echo {got} vs {want} at ratio {ratio}");
            // doubler: the original's read rate, the largest offset (58000 phrase frames in) as the delay
            let d = &c.devices[5].1;
            assert!(((d[3] * 0.01) as f64 - (65536.0 / 0xFF3B as f64 - 1.0)).abs() < 1e-8, "sweep rate");
            let kd = (d[1] as f64 * 16384.0).round();
            assert_eq!(kd, ((65536.0 / 0xFF3B as f64 - 1.0) * 58000.0 / kappa).ceil(), "doubler delay at ratio {ratio}");
            // latency: the comb's K, the doubler's K + 1, the low-pass's 1 per stage
            assert_eq!(c.latency as f64, k + kd + 1.0 + LP_STAGES as f64);
            assert_eq!(c.restart, Some(5), "the doubler takes the restart lane");
            // bandwidth: R·ratio/8 (decimation 4): the tempo delay's high cut, wet only, no time, no feedback
            assert_eq!(c.lowpass.map(|l| l.0), Some(6));
            for lp in &c.devices[6..6 + LP_STAGES] {
                let lp = &lp.1;
                assert_eq!((lp[1], lp[2], lp[5], lp[7], lp[9]), (0.0, 0.0, 0.0, 0.0, 1.0), "{lp:?}");
                let hz = 20.0 + 19980.0 * (lp[6] as f64).powi(2);
                assert!((hz - R * ratio / 8.0).abs() < 0.5, "low-pass {hz} at ratio {ratio}");
            }
        }
    }

    #[test]
    fn the_echo_level_is_folded_into_the_wrap_or_reported() {
        // tempo delay: out = (1 − w)·x + w·d with feedback e; w = e/(1 + e) is the original's x + e·d over (1 + e)
        let c = chain("song0", 3, 1.0);
        let d = &c.devices[3].1;
        let e = 0.5f32;
        assert!((d[7] - e).abs() < 1e-6 && (d[9] - e / (1.0 + e)).abs() < 1e-6 && d[8] == 0.0, "{d:?}");
        assert!((c.level - 1.0 / 1.5).abs() < 1e-6, "no wrap: the track gain makes up {}", c.level);
        // a wrap after the echo: its drive makes up for the echo level, so the wrap sees the original's values
        let c = chain("song0", 8, 1.0); // echo 0.4, ×1.25
        let w = &c.devices[6].1;
        assert!(((w[1] * 4.0) as f64 - 1.25 * 1.4).abs() < 1e-5 && (w[2] - 0.5).abs() < 1e-6, "{w:?}");
        assert_eq!(c.level, 1.0);
        let c = chain("song0", 10, 1.0); // echo 0.4, +48
        let w = &c.devices[4].1;
        assert!(((w[1] * 4.0) as f64 - 1.4).abs() < 1e-5 && ((w[2] * 2.0 - 1.0) as f64 - 48.0 / 128.0).abs() < 1e-6, "{w:?}");
    }

    #[test]
    fn no_low_pass_above_the_song_s_band() {
        // a full-rate phrase has no decimation; a high ratio puts the stored Nyquist past 20 kHz
        let lp = |c: Chain| c.lowpass.is_some();
        assert!(!lp(chain("song0", 8, 1.0)));
        assert!(!lp(chain("jizz", 0, 3.0)), "R·3/4 = 26.6 kHz");
        assert!(lp(chain("jizz", 0, 1.0)));
    }
}

mod phrase {
    use super::super::*;
    use super::data;

    fn jizz() -> Call {
        load(&data("patches_jizz.json")).unwrap().calls.remove(0)
    }

    /// The frequency a (note, coarse, fine) plays on `t`'s instrument (its octave and ratio, through the laws).
    fn hz_of(t: &Tuning, (note, coarse, fine, _): (u8, u8, u8, u8)) -> f64 {
        use unisynth::{law, params::{osc, o}};
        let p = |v: f32| v as f64;
        let mut v = [0.0f32; unisynth::params::COUNT];
        t.set(&mut v);
        let semis = note as f64 + p(law::coarse(coarse as f32 / 255.0)) + p(law::fine(fine as f32 / 255.0)) + 12.0 * p(law::octave(v[osc(0, o::OCTAVE)]));
        let ratio = p(law::ratio(v[osc(0, o::RATIO)])) + p(law::ratio_fine(v[osc(0, o::RATIO_FINE)]));
        440.0 * 2f64.powf((semis - 69.0) / 12.0) * ratio
    }

    #[test]
    fn every_pitch_word_plays_unsigned() {
        // the slot steps by the word as the original does: a word above 32767 runs the phase backwards and its
        // second saw by `round(w·1.02)` of the unsigned word, so the note asks for the unsigned word's frequency
        let words = [3u16, 99, 211, 1007, 16383, 32767, 32768, 51199, 60736];
        for ratio in [1.0, 2.0] {
            let t = Tuning::of(&words, ratio);
            for w in words {
                let hz = hz_of(&t, t.note(w));
                let want = w as f64 * R * ratio / 65536.0;
                // within half a fine step, or 2½ steps where it moved off a rounding edge
                assert!((hz / want).log2().abs() * 1200.0 < 2.0, "word {w} at ratio {ratio}: {hz} vs {want}");
            }
        }
    }

    #[test]
    fn the_word_byte_makes_every_word_exact() {
        // the slot's word is rint(frequency·65536/clock) + b − 128: exactly the original's, with the frequency's
        // word clear of a rounding edge (the slot's f32 path is within ~0.005 words); word 0 too (a frozen phase)
        let stash = load(&data("patches_stash.json")).unwrap();
        let mut all: Vec<Vec<u16>> = jizz().records.iter().map(|r| r.pitch.to_vec()).collect();
        all.extend(stash.calls.iter().flat_map(|c| c.records.iter().map(|r| r.pitch.to_vec())));
        all.push(vec![0, 3, 1007, 51199, 60736]);
        for ratio in [1.0, 2.0] {
            let clock = (R * ratio / 1_048_576.0) as f32 as f64 * 1_048_576.0; // CD_CLOCK as stored
            for words in &all {
                let t = Tuning::of(words, ratio);
                for &w in words {
                    let n = t.note(w);
                    let enc = hz_of(&t, n) * 65536.0 / clock;
                    assert!((enc - enc.round()).abs() < 0.45, "word {w}: encoded {enc}");
                    assert_eq!(enc.round() as i64 + n.3 as i64 - 128, w as i64, "word {w} at ratio {ratio}");
                    assert!(n.3 >= 1);
                }
            }
        }
    }

    #[test]
    fn a_high_word_s_glide_window_is_marked() {
        // song0 record 3 holds word 51199 through 15 tied steps: each glide window (the first 1764 frames of a tied
        // step) plays the signed word; Jizz's words are all below 32768
        let p = load(&data("patches_stash.json")).unwrap();
        let evs = phrase_events(p.call("song0").unwrap(), 3, 0, 1.0);
        let g: Vec<_> = evs.iter().filter_map(|e| if let PEv::Glide { on } = e.what { Some((e.at, on)) } else { None }).collect();
        let want: Vec<_> = (1..16).flat_map(|s| [((s * STEP) as f64, true), ((s * STEP) as f64 + 1764.0, false)]).collect();
        assert_eq!(g, want);
        let c = jizz();
        for ri in 0..c.records.len() {
            assert!(!phrase_events(&c, ri, 0, 1.0).iter().any(|e| matches!(e.what, PEv::Glide { .. })));
        }
    }

    #[test]
    fn a_narrow_record_needs_no_coarse_byte() {
        let zero = (unisynth::law::host::inv_coarse(0.0) * 255.0).round() as u8;
        for r in &jizz().records {
            let t = Tuning::of(&r.pitch, 1.0);
            assert!(r.pitch.iter().all(|&w| t.note(w).1 == zero), "{:?}", r.pitch);
            assert!(r.pitch.iter().all(|&w| t.note(w).3.abs_diff(128) <= 2), "low words need little correction: {:?}", r.pitch);
        }
        // the instrument's octave and ratio are the tuning's
        let c = jizz();
        let p = instrument(c.version, &c.records[0], 1.0);
        let mut v = [0.0f32; unisynth::params::COUNT];
        Tuning::of(&c.records[0].pitch, 1.0).set(&mut v);
        for f in [unisynth::params::o::OCTAVE, unisynth::params::o::RATIO, unisynth::params::o::RATIO_FINE] {
            assert_eq!(p[unisynth::params::osc(0, f)], v[unisynth::params::osc(0, f)]);
        }
    }

    #[test]
    fn untied_steps_are_note_ons_with_their_retrigger_and_position() {
        let c = jizz();
        for (ri, r) in c.records.iter().enumerate() {
            for variant in 0..r.variants as usize {
                let evs = phrase_events(&c, ri, variant, 1.0);
                let ons: Vec<_> = evs.iter().filter_map(|e| if let PEv::On { retrig, vel, .. } = e.what { Some((e.at, retrig, vel)) } else { None }).collect();
                let want: Vec<_> = (0..16)
                    .filter_map(|s| r.retrig(variant, s).map(|rt| ((s as u32 * STEP) as f64, note_byte(&rt), velocity((s as u32 * STEP) as f64))))
                    .collect();
                assert_eq!(ons, want, "record {ri} variant {variant}");
            }
        }
    }

    #[test]
    fn every_note_on_follows_a_note_off_and_ties_slide() {
        // mono legato: a retrigger must find no held note; a tie must find one (or be a fine change only)
        let c = jizz();
        for (ri, _) in c.records.iter().enumerate() {
            let mut held = false;
            for e in phrase_events(&c, ri, 0, 1.0) {
                match e.what {
                    PEv::On { .. } => {
                        assert!(!held, "record {ri}: retrigger at {} over a held note", e.at);
                        held = true;
                    }
                    PEv::Legato { .. } => assert!(held, "record {ri}: tie at {} with nothing held", e.at),
                    PEv::Off => {
                        assert!(held, "record {ri}: off at {} with nothing held", e.at);
                        held = false;
                    }
                    PEv::Tune { .. } | PEv::Glide { .. } => {}
                }
            }
            assert!(!held, "record {ri}: the phrase ends released");
        }
    }

    #[test]
    fn v1_note_ends_fade_over_a_chain_s_last_step() {
        // Jizz record 0, ties [0,0,0,1,...]: steps 0 and 1 are single-step notes (off just after their on), step 2
        // ties into 3, so its note ends at step 3's start (the fade of the chain's last step)
        let c = jizz();
        let evs = phrase_events(&c, 0, 0, 1.0);
        let offs: Vec<f64> = evs.iter().filter(|e| matches!(e.what, PEv::Off)).map(|e| e.at).collect();
        assert_eq!(offs[..3], [ATTACK, STEP as f64 + ATTACK, 3.0 * STEP as f64]);
    }

    #[test]
    fn v2_note_ends_follow_the_gate() {
        let s = load(&data("patches_stash.json")).unwrap();
        let c = s.call("bluish").unwrap();
        let r = &c.records[0];
        let evs = phrase_events(c, 0, 0, 1.0);
        let first_off = evs.iter().find(|e| matches!(e.what, PEv::Off)).unwrap().at;
        // the first chain ends at the step before the next untied one; the gate closes `hold` frames into it
        let end = (0..16).find(|&s| s + 1 < 16 && r.tie[s + 1] == 0).unwrap();
        assert_eq!(first_off, (end as u32 * STEP) as f64 + r.hold as f64);
    }
}

#[test]
fn the_second_saw_s_phase_follows_the_reference_renders() {
    // tests/data/cauldron_phase2.json: research/tbl-mini-cauldron/tools/phase2_trace.py (full renders)
    let trace: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/cauldron_phase2.json")).unwrap()).unwrap();
    let j = load(&data("patches_jizz.json")).unwrap();
    let s = load(&data("patches_stash.json")).unwrap();
    let (mut checked, mut off) = (0, 0);
    for call in [&j.calls[0], s.call("bluish").unwrap(), s.call("song0").unwrap()] {
        for entry in trace[&call.name].as_array().unwrap() {
            let rec = entry["record"].as_u64().unwrap() as usize;
            if !call.records[rec].second_saw() {
                continue; // no second saw, or the spike / noise waves, which do not read it
            }
            for c in entry["calls"].as_array().unwrap() {
                let c: Vec<i64> = c.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect();
                let step = c[1].max(0) as usize; // a variant's first retrigger is at step 0's start too
                let got = phase2_at(call, rec, c[0] as usize, step);
                let d = (got as i64 - c[2]).rem_euclid(65536).min((c[2] - got as i64).rem_euclid(65536));
                off = off.max(d);
                checked += 1;
            }
        }
    }
    // the reference's x87 rounding of `pitch·1.02` and of the glide is emulated in f32: a word or two at most
    assert!(off <= 2, "worst {off} of 65536");
    assert!(checked > 300, "{checked}");
}

#[test]
fn the_phase_byte_rounds_a_word_to_1_of_255_turn() {
    assert_eq!(phase_byte(0), 1);
    assert_eq!(phase_byte(32768), 1 + 128);
    assert_eq!(phase_byte(65535), 1, "a whole turn is 0");
}
