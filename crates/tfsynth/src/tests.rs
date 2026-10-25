// ---- params ----
mod params_tests {
    use crate::params::*;

    #[test]
    fn layout_and_names() {
        assert_eq!(COUNT, 112);
        assert_eq!(NAMES.len(), COUNT);
        assert_eq!(NAMES[GAIN], "Gain");
        assert_eq!(NAMES[GEN_DETUNE], "GetDetn"); // the public spelling
        assert_eq!(NAMES[MM_FIRST], "Mm1Source");
        assert_eq!(NAMES[MM_FIRST + 29], "Mm10Targe");
        assert_eq!(NAMES[EFFECT_FIRST + 9], "Effect10");
        assert_eq!(NAMES[NT_Q], "FltNTQ");
        // every name unique
        for (i, a) in NAMES.iter().enumerate() {
            assert!(NAMES[i + 1..].iter().all(|b| b != a), "{a}");
        }
    }

    #[test]
    fn mapping_spot_values() {
        assert_eq!(octave_factor(0.5), 1.0);
        assert_eq!(octave_factor(0.0), 16.0);
        assert_eq!(octave_factor(1.0), 1.0 / 16.0);
        assert_eq!(unison(0.0), 1);
        assert_eq!(unison(1.0), 10);
        assert_eq!(unison(0.05), 1); // round(0.45)
        assert_eq!(unison(0.06), 2); // round(0.54)
        assert_eq!(polyphony(0.0), 1);
        assert_eq!(polyphony(1.0), 16);
        assert_eq!(harmonics(0.0), 1);
        assert_eq!(harmonics(1.0), 65);
        assert_eq!(gain_curve(0.5), 1.0);
        assert_eq!(gain_curve(0.25), 0.25);
        assert_eq!(gain_curve(1.0), 11.0);
        assert_eq!(mod_amount(0.5), 1.0);
        assert_eq!(mod_amount(1.0), 10.0);
        assert_eq!(mod_amount(0.25), 0.5);
        assert_eq!(eq_gain(0.5), 1.0);
        assert_eq!(eq_gain(1.0), 11.0);
        assert_eq!(effect_kind(0.2), 2);
        assert_eq!(effect_kind(0.75), 8); // round(7.5)
        assert_eq!(mod_source(1.0), 14);
        assert_eq!(mod_target(1.0), 39);
        assert_eq!(bend_semitones(0.0), 1);
        assert_eq!(bend_semitones(1.0), 13);
        assert_eq!(lfo_shape(0.5), 2);
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-0.5), 0.0);
    }

    #[test]
    fn byte_quantisation() {
        assert_eq!(from_byte(100), 1.0);
        assert_eq!(from_byte(50), 0.5);
        assert_eq!(from_byte(0), 0.0);
    }
}

// ---- rng ----
mod rng_tests {
    use crate::rng::Xorshift;
    use synth_core::rng::Rng;

    #[test]
    fn deterministic_and_in_range() {
        let (mut a, mut b) = (Xorshift::new(), Xorshift::new());
        let mut sum = 0.0f64;
        for _ in 0..100_000 {
            let x = a.next_f32();
            assert_eq!(x, b.next_f32());
            assert!((0.0..1.0).contains(&x));
            sum += x as f64;
        }
        assert!((sum / 100_000.0 - 0.5).abs() < 0.01);
        assert!(a.range(-1.0, 1.0) >= -1.0);
        assert!(a.below(32768) < 32768);
    }

    #[test]
    fn seeds_differ() {
        assert_ne!(Xorshift::with_seed(1).next_f32(), Xorshift::with_seed(2).next_f32());
        assert_eq!(Xorshift::with_seed(0).next_f32(), Xorshift::with_seed(0).next_f32()); // 0 seed is remapped
    }
}

// ---- tables + ifft ----
mod table_tests {
    use crate::ifft::ifft512;
    use crate::tables::{tables, BINS};

    #[test]
    fn tables_shapes() {
        let t = tables();
        assert_eq!(t.sin[0], 0.0);
        assert!((t.sin[1024] - 1.0).abs() < 1e-6); // quarter of the 4096 period
        assert_eq!(t.exp[0], 1.0);
        assert!((t.exp[4095] - libm::expf(-5.0 * 4095.0 / 4096.0)).abs() < 1e-7);
        assert!(t.rho.iter().all(|r| (-1.0..=1.0).contains(r)));
        assert!(t.lfo_noise.iter().all(|r| (0.0..1.0).contains(r)));
        let (lo, hi) = t.white.iter().fold((0.0f32, 0.0f32), |(a, b), &x| (a.min(x), b.max(x)));
        assert!(lo >= -1.001 && hi <= 1.001 && lo < -0.99 && hi > 0.99, "{lo} {hi}");
        let mean = t.white.iter().map(|&x| x as f64).sum::<f64>() / t.white.len() as f64;
        assert!(mean.abs() < 0.01);
        assert!((t.note_hz[69] - 440.0).abs() < 0.01);
        assert!((t.note_hz[0] - 8.1758).abs() < 0.001);
    }

    #[test]
    fn ifft_single_bin_is_a_sinusoid() {
        for k in [1usize, 3, 17, 200] {
            let mut x = [0.0f32; 2 * BINS];
            x[2 * k] = 1.0; // re
            ifft512(&mut x);
            let w = 2.0 * core::f64::consts::PI * k as f64 / BINS as f64;
            for n in 0..BINS {
                let (re, im) = (x[2 * n] as f64, x[2 * n + 1] as f64);
                assert!((re - (w * n as f64).cos()).abs() < 1e-4, "k {k} n {n} re {re}");
                assert!((im - (w * n as f64).sin()).abs() < 1e-4, "k {k} n {n} im {im}"); // positive exponent
            }
        }
    }

    #[test]
    fn ifft_is_linear_and_unscaled() {
        let mut x = [0.0f32; 2 * BINS];
        x[0] = 2.0; // DC
        ifft512(&mut x);
        assert!(x.chunks(2).all(|c| (c[0] - 2.0).abs() < 1e-5 && c[1].abs() < 1e-5));
    }
}

// ---- spectrum generator ----
mod wavetable_tests {
    use crate::wavetable::{Shape, Table};
    use crate::tables::BINS;

    fn shape(harm: f32, bw: f32, damp: f32, scale: f32, k: usize) -> Shape {
        Shape { harmonics: crate::params::harmonics(harm), bandwidth: bw, damp, scale, bins: k }
    }

    /// Magnitude (|re| + |im| as both are equal before the phase stage) per bin.
    fn mags(t: &Table) -> Vec<f32> {
        t.spectrum().chunks(2).map(|c| c[0]).collect()
    }

    #[test]
    fn scale_one_puts_partials_on_even_bins() {
        let mut t = Table::new();
        assert!(t.update(&shape(4.0 / 64.0, 0.0, 0.0, 1.0, BINS))); // 5 partials, narrow
        let m = mags(&t);
        assert_eq!(m[0], 1.0); // DC forced
        for h in 1..=5 {
            assert!((m[2 * h] - 1.0 / h as f32).abs() < 1e-6, "h {h}: {}", m[2 * h]); // exp(0)·h^-1
        }
        // narrow width 0.3: odd bins get exp(−1/0.3) ≈ 0.036 of each neighbour (bin 3: 0.036·1 + 0.036·½)
        let e = libm::expf(-1.0 / 0.3);
        assert!((m[3] - (e + e / 2.0)).abs() < 2e-3, "{}", m[3]);
        assert!(m[20..].iter().all(|&v| v < 1e-3));
    }

    #[test]
    fn width_gain_and_cutoff() {
        let mut t = Table::new();
        t.update(&shape(0.0, 1.0, 1.0, 1.0, BINS)); // one partial at bin (2·1−1)·1+1 = 2, width 1.3, gain 1
        let m = mags(&t);
        assert_eq!(m[2], 1.0);
        assert!((m[3] - libm::expf(-1.0 / 1.3)).abs() < 2e-3);
        // cut at 5 widths: 2 + 6.5 → bin 8 is inside (4.6 widths), bin 9 outside (5.4)
        assert!(m[8] > 0.0 && m[9] == 0.0);
        // damp 1 → 1/h²
        t.update(&shape(1.0 / 64.0, 0.0, 1.0, 1.0, BINS));
        assert!((mags(&t)[4] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn band_limit_zeroes_upper_bins() {
        let mut t = Table::new();
        t.update(&shape(1.0, 0.5, 0.0, 1.0, 40));
        let m = mags(&t);
        assert!(m[39] > 0.0 && m[40..].iter().all(|&v| v == 0.0));
    }

    #[test]
    fn rebuild_only_on_change() {
        let mut t = Table::new();
        assert!(t.update(&shape(0.5, 0.2, 0.3, 1.0, BINS)));
        assert!(!t.update(&shape(0.5, 0.2, 0.3, 1.0, BINS)));
        assert!(!t.update(&shape(0.5, 0.2 + 5e-6, 0.3, 1.0, BINS))); // below the 1e-5 tolerance
        assert!(t.update(&shape(0.5, 0.21, 0.3, 1.0, BINS)));
        assert!(t.update(&shape(0.5, 0.21, 0.3, 1.0, 100)));
        assert!(t.update(&shape(0.6, 0.21, 0.3, 1.0, 100)));
    }

    #[test]
    fn band_limit_heuristic() {
        use crate::wavetable::band_bins;
        assert_eq!(band_bins(8.0), 512);
        assert_eq!(band_bins(2008.0), 4);
        assert_eq!(band_bins(5000.0), 4);
        assert_eq!(band_bins(1008.0), 64); // (1 − 0.5)³ · 512
    }

    #[test]
    fn phase_stage_quirk_range() {
        // GenMod 0: every real part 0, every imaginary part × sin(2π·128/4096) (constant phase)
        let mut t = Table::new();
        t.update(&shape(4.0 / 64.0, 0.0, 0.0, 1.0, BINS));
        let spec = t.spectrum().to_vec();
        let mut mu = 37.5;
        let out = t.phase_stage(0.0, &mut mu);
        let k = libm::sinf(2.0 * core::f32::consts::PI * 128.0 / 4096.0);
        for b in 1..BINS {
            assert_eq!(out[2 * b], 0.0);
            assert!((out[2 * b + 1] - spec[2 * b + 1] * k).abs() < 1e-7);
        }
        assert_eq!((out[0], out[1]), (1.0, 0.0));
        assert_eq!(mu, 37.5);
        // any GenMod: multipliers stay in [0, sin(π/4)], never negative
        let mut mu = 99.999;
        let out = t.phase_stage(0.8, &mut mu).to_vec();
        for b in 1..BINS {
            for c in 0..2 {
                let s = spec[2 * b + c];
                if s > 0.0 {
                    let r = out[2 * b + c] / s;
                    assert!((-1e-6..=0.7072).contains(&r), "bin {b}: {r}");
                }
            }
        }
        // drift μ += g³/100, wrapping at 100
        assert!((mu - (99.999 + 0.512 / 100.0 - 100.0)).abs() < 1e-4, "{mu}");
    }

    /// Spec scenario "Harmonic series": GenScale 0.25 (s = 1) → the rendered table's own spectrum has its partials
    /// on even bins (harmonics of bin 2: one octave above the table's bin-1 reference).
    #[test]
    fn rendered_table_peaks_at_even_bins() {
        for g in [0.0, 0.6] {
            let mut t = Table::new();
            t.update(&shape(5.0 / 64.0, 0.0, 0.0, 4.0 * 0.25, BINS));
            let mut mu = 3.0;
            t.render(g, &mut mu);
            let w = t.wave();
            let dft = |k: usize| {
                let (mut re, mut im) = (0.0f64, 0.0f64);
                for (n, &v) in w.iter().enumerate() {
                    let a = 2.0 * core::f64::consts::PI * (k * n) as f64 / BINS as f64;
                    re += v as f64 * a.cos();
                    im += v as f64 * a.sin();
                }
                (re * re + im * im).sqrt()
            };
            for h in 1..=6 {
                let (even, odd) = (dft(2 * h), dft(2 * h + 1));
                assert!(even > 10.0 * odd, "g {g} bin {}: {even} vs {odd}", 2 * h);
            }
            assert!(dft(2) > 10.0 * dft(1), "fundamental is bin 2, bin 1 is empty");
        }
    }

    #[test]
    fn table_is_normalised_and_dc_free() {
        let mut t = Table::new();
        t.update(&shape(0.3, 0.1, 0.2, 1.0, BINS));
        let mut mu = 10.0;
        t.render(0.3, &mut mu);
        let w = t.wave();
        // peak-normalised before the DC removal, so the span is at most 2 and the mean is 0
        let (lo, hi) = w.iter().fold((0.0f32, 0.0f32), |(a, b), &v| (a.min(v), b.max(v)));
        let mean = w.iter().sum::<f32>() / w.len() as f32;
        assert!(hi - lo <= 2.0 + 1e-5 && hi - lo > 0.5, "{lo} {hi}");
        assert!(mean.abs() < 1e-5);
    }
}

/// Shared helpers for instrument-level tests.
pub(crate) mod harness {
    use crate::instrument::Instrument;
    use crate::params::*;
    use crate::voice::BLOCK;
    use synth_core::pool::Pool;

    /// An instrument in leaked pool memory.
    pub fn instrument() -> Instrument<'static> {
        let mem: &'static mut [u8] = Box::leak(vec![0u8; Instrument::need(0) + 4096].into_boxed_slice());
        let pool: &'static mut Pool<'static> = Box::leak(Box::new(Pool::new(mem)));
        Instrument::new(pool, 0).unwrap()
    }

    /// A plain sine patch: one partial on bin 1 (GenScale 0), full volume, gain ×1, no envelope (organ gate),
    /// pitch-bend ranges 2 semitones.
    pub fn sine() -> [f32; COUNT] {
        let mut p = [0.0f32; COUNT];
        p[GAIN] = 0.5;
        p[GEN_VOLUME] = 1.0;
        p[GEN_PAN] = 0.5;
        p[GEN_OCTAVE] = 0.5;
        p[GEN_POLY] = 1.0;
        p[NOISE_BW] = 1.0;
        p[BEND_UP] = 1.0 / 12.0;
        p[BEND_DOWN] = 1.0 / 12.0;
        for i in 0..10 {
            p[MM_FIRST + 3 * i + 1] = 0.5; // amount 1, source none
        }
        p
    }

    pub fn render(ins: &mut Instrument, blocks: usize) -> (Vec<f32>, Vec<f32>) {
        let (mut l, mut r) = (vec![], vec![]);
        let mut out = [[0.0f32; BLOCK]; 2];
        for _ in 0..blocks {
            ins.render_voices(&mut out);
            l.extend_from_slice(&out[0]);
            r.extend_from_slice(&out[1]);
        }
        (l, r)
    }

    /// Goertzel magnitude of `hz`.
    pub fn mag(x: &[f32], hz: f64) -> f64 {
        let w = 2.0 * core::f64::consts::PI * hz / 44100.0;
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for &v in x {
            let s = v as f64 + 2.0 * w.cos() * s1 - s2;
            s2 = s1;
            s1 = s;
        }
        (s1 * s1 + s2 * s2 - 2.0 * w.cos() * s1 * s2).sqrt() / x.len() as f64
    }

    /// Strongest frequency within ±`span` Hz of `around` (0.05 Hz steps).
    pub fn peak(x: &[f32], around: f64, span: f64) -> f64 {
        let mut best = (0.0, 0.0);
        let mut f = around - span;
        while f <= around + span {
            let m = mag(x, f);
            if m > best.1 {
                best = (f, m);
            }
            f += 0.05;
        }
        best.0
    }

    pub fn rms(x: &[f32]) -> f64 {
        (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
    }
}

// ---- voice playback (task 2.3) ----
mod voice_tests {
    use super::harness::*;
    use crate::params::*;

    fn tone(p: [f32; COUNT], note: u8) -> (Vec<f32>, Vec<f32>) {
        let mut ins = instrument();
        ins.set_patch(&p).unwrap();
        ins.note_on(note, 100);
        render(&mut ins, 90) // ≈ 1 s
    }

    #[test]
    fn pitch_matches_note_and_octave_doubles_it() {
        let (l, r) = tone(sine(), 69);
        assert!((peak(&l[4096..], 440.0, 20.0) - 440.0).abs() < 0.5);
        assert!((peak(&r[4096..], 440.0, 20.0) - 440.0).abs() < 0.5);
        let mut p = sine();
        p[GEN_OCTAVE] = 3.0 / 8.0; // round(3) → ×2
        let (l, _) = tone(p, 69);
        assert!((peak(&l[4096..], 880.0, 20.0) - 880.0).abs() < 0.5);
        let (l, _) = tone(sine(), 57);
        assert!((peak(&l[4096..], 220.0, 20.0) - 220.0).abs() < 0.5);
    }

    #[test]
    fn fixed_frequency_ignores_the_note() {
        let mut p = sine();
        p[GEN_FREQ] = 0.5; // 0.25 · 1000 Hz
        for note in [40, 80] {
            let (l, _) = tone(p, note);
            assert!((peak(&l[4096..], 250.0, 20.0) - 250.0).abs() < 0.5, "note {note}");
        }
    }

    #[test]
    fn detune_splits_left_and_right() {
        let mut p = sine();
        p[GEN_DETUNE] = 0.5; // 0.125 · 10 = 1.25 Hz
        let (l, r) = tone(p, 69);
        assert!((peak(&l[4096..], 441.25, 5.0) - 441.25).abs() < 0.1);
        assert!((peak(&r[4096..], 438.75, 5.0) - 438.75).abs() < 0.1);
    }

    #[test]
    fn pan_is_a_linear_balance() {
        let mut p = sine();
        p[GEN_PAN] = 0.0;
        let (l, r) = tone(p, 69);
        assert!(rms(&l[4096..]) > 0.01 && rms(&r[4096..]) == 0.0);
        p[GEN_PAN] = 0.75; // left × 2(1 − 0.75) = 0.5
        let (l, r) = tone(p, 69);
        assert!((rms(&l[4096..]) / rms(&r[4096..]) - 0.5).abs() < 0.01);
    }

    #[test]
    fn velocity_scales_and_unison_adds_up() {
        let (a, _) = tone(sine(), 69);
        let mut ins = instrument();
        ins.set_patch(&sine()).unwrap();
        ins.note_on(69, 50);
        let (b, _) = render(&mut ins, 90);
        assert!((rms(&b[4096..]) / rms(&a[4096..]) - 0.5).abs() < 0.01);
        let mut p = sine();
        p[GEN_UNISON] = 1.0; // 10 layers, no normalisation
        p[GEN_SPREAD] = 0.5; // layers apart: no coherent cancellation over time
        let (u, _) = tone(p, 69);
        assert!(rms(&u[4096..]) > 2.0 * rms(&a[4096..]));
    }

    #[test]
    fn drive_clips_to_unit_gain() {
        let mut p = sine();
        p[GEN_DRIVE] = 1.0; // ×33 then clip: a square of amplitude vol
        let (l, _) = tone(p, 69);
        let peak_abs = l[4096..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let vol = 1.0 * 4.0 * (100.0 / 128.0) * 0.1;
        assert!((peak_abs - vol).abs() < 1e-5, "{peak_abs} vs {vol}");
    }

    #[test]
    fn first_sample_in_range_at_maximum_offset() {
        // every note-on draws right = base + [0, 0.1): many retriggers with 10 layers must never read past the table
        let mut p = sine();
        p[GEN_UNISON] = 1.0;
        let mut ins = instrument();
        ins.set_patch(&p).unwrap();
        for i in 0..400 {
            ins.note_on(40 + (i % 40) as u8, 100);
            let (l, r) = render(&mut ins, 1);
            assert!(l.iter().chain(&r).all(|v| v.is_finite()));
        }
    }

    #[test]
    fn slop_and_glide() {
        let mut p = sine();
        p[GEN_SLOP] = 1.0; // ±8 Hz per note
        let (l, _) = tone(p, 69);
        let f = peak(&l[4096..], 440.0, 10.0);
        assert!((f - 440.0).abs() <= 8.05 && (f - 440.0).abs() > 0.05, "{f}");
        // glide: one voice, second note slews towards its pitch block by block
        let mut p = sine();
        p[GEN_POLY] = 0.0;
        p[GEN_GLIDE] = 1.0; // 1/11 of the gap per block
        let mut ins = instrument();
        ins.set_patch(&p).unwrap();
        ins.note_on(57, 100);
        render(&mut ins, 4);
        ins.note_on(69, 100);
        render(&mut ins, 1);
        let f1 = ins.voices()[0].freq;
        assert!((f1 - (220.0 + (440.0 - 220.0) / 11.0)).abs() < 0.01, "{f1}");
        render(&mut ins, 200); // 220·(10/11)^200 ≈ 1e-6 Hz left
        assert!((ins.voices()[0].freq - 440.0).abs() < 0.01);
    }

    #[test]
    fn organ_gate_without_envelope() {
        let mut ins = instrument();
        ins.set_patch(&sine()).unwrap();
        ins.note_on(69, 100);
        let (held, _) = render(&mut ins, 10);
        assert!(rms(&held[2048..]) > 0.01); // DC bin forced to 1 → a lone partial sits at ≈0.16 of the peak
        ins.note_off(69);
        let (tail, _) = render(&mut ins, 3);
        assert!(tail[512..].iter().all(|v| *v == 0.0), "fades over one block then frees");
        assert!(ins.voices().iter().all(|v| !v.active()));
    }
}

// ---- note handling (task 2.4) ----
mod note_tests {
    use super::harness::*;
    use crate::params::*;
    use synth_core::voice::VoiceState;

    fn notes(ins: &crate::instrument::Instrument) -> Vec<(u8, bool)> {
        ins.voices().iter().map(|v| (v.note(), v.held)).collect()
    }

    #[test]
    fn stealing_takes_the_oldest_and_keeps_the_rest() {
        let mut p = sine();
        p[GEN_POLY] = 0.07; // trunc(15·0.07 + 1) = 2
        let mut ins = instrument();
        ins.set_patch(&p).unwrap();
        ins.note_on(60, 100); // A
        render(&mut ins, 2);
        ins.note_on(64, 100); // B
        render(&mut ins, 2);
        ins.note_on(67, 100); // C steals A's voice
        assert_eq!(&notes(&ins)[..3], &[(67, true), (64, true), (0, false)]);
        // note-off of the stolen note releases nothing
        ins.note_off(60);
        assert_eq!(&notes(&ins)[..2], &[(67, true), (64, true)]);
    }

    #[test]
    fn polyphony_limit_restricts_voices() {
        let mut p = sine();
        p[GEN_POLY] = 0.0; // one voice
        let mut ins = instrument();
        ins.set_patch(&p).unwrap();
        for n in [60, 62, 64] {
            ins.note_on(n, 100);
            render(&mut ins, 1);
        }
        assert_eq!(notes(&ins)[0], (64, true));
        assert!(ins.voices()[1..].iter().all(|v| !v.active()));
    }

    #[test]
    fn pitch_bend_is_linear_in_hz_on_all_voices() {
        let mut ins = instrument();
        ins.set_patch(&sine()).unwrap();
        ins.note_on(69, 100);
        ins.pitch_bend(0.5); // halfway (in Hz) to 2 semitones up
        let (l, _) = render(&mut ins, 90);
        let up = 440.0 * crate::tables::SEMITONE as f64 * crate::tables::SEMITONE as f64;
        let want = 440.0 + (up - 440.0) * 0.5;
        assert!((peak(&l[4096..], want, 5.0) - want).abs() < 0.1);
        // released voices follow bend too
        ins.note_off(69);
        ins.pitch_bend(-1.0);
        assert!(ins.voices().iter().all(|v| v.bend == -1.0));
    }

    #[test]
    fn mod_wheel_is_a_source() {
        let mut p = sine();
        p[MM_FIRST] = 5.0 / 14.0; // source: mod wheel
        p[MM_FIRST + 2] = 5.0 / 39.0; // target: volume
        let mut ins = instrument();
        ins.set_patch(&p).unwrap();
        ins.note_on(69, 100);
        let (quiet, _) = render(&mut ins, 4);
        assert_eq!(rms(&quiet[1024..]), 0.0);
        ins.mod_wheel(1.0);
        let (loud, _) = render(&mut ins, 4);
        assert!(rms(&loud[1024..]) > 0.01);
    }

    #[test]
    fn wrong_patch_length_is_rejected() {
        let mut ins = instrument();
        ins.set_patch(&sine()).unwrap();
        let before = ins.params;
        assert!(ins.set_patch(&[0.5; 111]).is_err());
        assert_eq!(ins.params, before);
    }
}

// ---- determinism (task 2.5) ----
mod determinism_tests {
    use super::harness::*;
    use crate::params::*;

    #[test]
    fn same_patch_and_notes_render_bit_identical() {
        let mut p = sine();
        p[GEN_MOD] = 0.7;
        p[GEN_SLOP] = 0.5;
        p[GEN_UNISON] = 0.5;
        p[NOISE_AMOUNT] = 0.3;
        p[NOISE_BW] = 0.4;
        p[NOISE_FREQ] = 0.5;
        let go = || {
            let mut ins = instrument();
            ins.set_patch(&p).unwrap();
            let mut out = vec![];
            for (i, n) in [60u8, 64, 67, 72].iter().enumerate() {
                ins.note_on(*n, 90 + i as u8);
                out.extend(render(&mut ins, 20).0);
            }
            out.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        };
        assert_eq!(go(), go());
    }
}

// ---- ADSR + LFO (task 3.1) ----
mod env_lfo_tests {
    use crate::env::{Adsr, Stage, STEP};
    use crate::lfo::{rate, Lfo};

    fn blocks_until(e: &mut Adsr, p: &[f32], m: f32, stop: impl Fn(&Adsr) -> bool) -> usize {
        let mut n = 0;
        while !stop(e) {
            e.step(p, m);
            n += 1;
            assert!(n < 100_000);
        }
        n
    }

    #[test]
    fn attack_times() {
        let mut e = Adsr::new();
        e.trigger();
        assert_eq!(blocks_until(&mut e, &[0.0, 0.5, 0.5, 0.5, 0.0], 1.0, |e| e.stage != Stage::Attack), 1);
        let step = -libm::log10f(0.94) * STEP; // a = 1
        let want = (1.0 / step).ceil() as usize;
        let mut e = Adsr::new();
        e.trigger();
        let n = blocks_until(&mut e, &[1.0, 0.5, 0.5, 0.5, 0.0], 1.0, |e| e.stage != Stage::Attack);
        assert!((want..=want + 1).contains(&n), "{n} vs {want}");
        assert!((140..=150).contains(&n)); // ≈ 1.7 s
    }

    #[test]
    fn linear_decay_and_release_reach_their_targets() {
        let (d, s, r) = (0.5f32, 0.4f32, 0.6f32);
        let p = [0.0, d, s, r, 0.0]; // slope 0: linear
        let mut e = Adsr::new();
        e.trigger();
        e.step(&p, 1.0); // attack done in one block
        let delta = 0.25 * libm::log10f(libm::powf(d, 3.0) * 0.94) * STEP;
        let n = blocks_until(&mut e, &p, 1.0, |e| e.stage == Stage::Sustain);
        // pos^0 = 1 → v drops by |δ| per block from 1 to s (one extra block detects v ≤ s)
        let want = ((1.0 - s) / -delta).ceil() as usize;
        assert!((want..=want + 1).contains(&n), "{n} vs {want}");
        assert_eq!(e.value, s);
        e.release();
        let rel = 0.25 * libm::log10f(libm::powf(r, 3.0) * 0.94) * STEP;
        let n = blocks_until(&mut e, &p, 1.0, |e| e.done());
        let want = (s / -rel).ceil() as usize;
        assert!((want..=want + 1).contains(&n), "{n} vs {want}");
        assert_eq!(e.value, 0.0);
    }

    #[test]
    fn decay_can_rise_when_modulated_above_one() {
        let p = [0.0, 1.0, 0.5, 0.5, 0.0];
        let mut e = Adsr::new();
        e.trigger();
        e.step(&p, 1.1); // attack → 1, decay
        let v0 = e.step(&p, 1.1);
        let v1 = e.step(&p, 1.1);
        assert!(v1 > v0 && v0 >= 1.0, "decay step positive for d·M ≈ 1.1: {v0} {v1}");
        assert_eq!(e.stage, Stage::Decay);
    }

    #[test]
    fn sustain_chases_a_changed_level() {
        let mut p = [0.0, 0.3, 0.5, 0.5, 0.0];
        let mut e = Adsr::new();
        e.trigger();
        blocks_until(&mut e, &p, 1.0, |e| e.stage == Stage::Sustain);
        p[2] = 0.8;
        let before = e.value;
        e.step(&p, 1.0);
        assert!(e.value > before && e.value <= 0.8);
        blocks_until(&mut e, &p, 1.0, |e| e.value == 0.8);
    }

    #[test]
    fn lfo_shapes_and_rate() {
        let two_pi = 2.0 * core::f32::consts::PI;
        let shape = |s: f32, phase: f32, depth: f32| {
            let mut l = Lfo { phase };
            l.step(&[0.0, depth, s], 1.0)
        };
        let ph = 1.0f32;
        assert!((shape(0.0, ph, 1.0) - (libm::sinf(ph) + 1.0) / 2.0).abs() < 1e-6);
        assert!((shape(0.25, ph, 1.0) - ph / two_pi).abs() < 1e-6);
        assert!((shape(0.5, ph, 1.0) - (1.0 - ph / two_pi)).abs() < 1e-6);
        assert_eq!(shape(0.75, 3.0, 1.0), 1.0);
        assert_eq!(shape(0.75, 3.2, 1.0), 0.0);
        let t = crate::tables::tables();
        assert_eq!(shape(1.0, ph, 1.0), t.lfo_noise[(ph / two_pi * 256.0) as usize]);
        // depth d²: output in [1 − d², 1]
        let v = shape(0.75, 3.2, 0.5);
        assert!((v - 0.75).abs() < 1e-6);
        assert_eq!(shape(0.0, ph, 0.0), 1.0);
        // rate 50p²·512/44100 rad per block, wraps at 2π
        let mut l = Lfo { phase: 0.0 };
        l.step(&[1.0, 1.0, 0.0], 1.0);
        assert!((l.phase - rate(1.0)).abs() < 1e-7 && (rate(1.0) - 50.0 * 512.0 / 44100.0).abs() < 1e-6);
        let mut l = Lfo { phase: two_pi - 0.01 };
        l.step(&[1.0, 1.0, 0.0], 1.0);
        assert!(l.phase < 1.0);
    }
}

// ---- mod matrix (task 3.2) ----
mod modmx_tests {
    use super::harness::*;
    use crate::modmx::{dst, ModMatrix};
    use crate::params::*;

    fn route(p: &mut [f32; COUNT], slot: usize, src: u8, amount: f32, target: u8) {
        let b = MM_FIRST + 3 * slot;
        p[b] = src as f32 / 14.0;
        p[b + 1] = amount;
        p[b + 2] = target as f32 / 39.0;
    }

    fn lfo_patch() -> [f32; COUNT] {
        let mut p = sine();
        p[LP_ON] = 1.0;
        p[LP_CUT] = 0.5;
        p[LFO1..LFO1 + 4].copy_from_slice(&[0.8, 1.0, 0.0, 0.0]);
        p
    }

    fn play(p: &[f32; COUNT]) -> Vec<u32> {
        let mut ins = instrument();
        ins.set_patch(p).unwrap();
        ins.note_on(57, 100);
        render(&mut ins, 40).0.iter().map(|v| v.to_bits()).collect()
    }

    #[test]
    fn slots_nine_and_ten_are_inert() {
        let empty = play(&lfo_patch());
        for slot in [8, 9] {
            let mut p = lfo_patch();
            route(&mut p, slot, 1, 0.5, dst::LP_CUT);
            assert_eq!(play(&p), empty, "slot {}", slot + 1);
        }
        let mut p = lfo_patch();
        route(&mut p, 7, 1, 0.5, dst::LP_CUT);
        assert_ne!(play(&p), empty, "slot 8 is live");
    }

    #[test]
    fn products_compose_per_target() {
        let mut p = sine();
        p[ENV1..ENV1 + 5].copy_from_slice(&[0.0, 0.0, 1.0, 0.5, 0.0]); // jumps to 1 (sustain 0.99)
        route(&mut p, 0, 3, 0.25, dst::VOLUME); // amount 0.5 · ADSR1
        route(&mut p, 1, 5, 0.5, dst::VOLUME); // amount 1 · wheel (0)
        route(&mut p, 2, 3, 1.0, dst::DRIVE); // amount 10 · ADSR1
        let mut m = ModMatrix::new();
        m.note_on([0.0; 2]);
        m.step(&p, 0.5);
        assert_eq!(m.get(dst::VOLUME), 0.5 * 1.0 * (1.0 * 0.5));
        assert_eq!(m.get(dst::DRIVE), 10.0);
        assert_eq!(m.get(dst::PAN), 1.0); // nothing routed
    }

    #[test]
    fn self_modulation_takes_one_block() {
        let mut p = sine();
        route(&mut p, 0, 5, 0.5, dst::VOLUME); // wheel → volume
        route(&mut p, 1, 5, 0.25, dst::SLOT1); // wheel · 0.5 → slot 1's own amount
        let mut m = ModMatrix::new();
        m.step(&p, 1.0);
        assert_eq!(m.get(dst::VOLUME), 1.0, "slot 1 not yet scaled");
        m.step(&p, 1.0);
        assert_eq!(m.get(dst::VOLUME), 0.5, "previous block's factor applies");
    }

    #[test]
    fn routed_envelope_releases_instead_of_gating() {
        let mut p = sine();
        p[ENV1..ENV1 + 5].copy_from_slice(&[0.0, 0.0, 1.0, 0.8, 0.0]); // long release
        route(&mut p, 0, 3, 0.5, dst::VOLUME);
        let mut ins = instrument();
        ins.set_patch(&p).unwrap();
        ins.note_on(69, 100);
        render(&mut ins, 10);
        ins.note_off(69);
        let (tail, _) = render(&mut ins, 6);
        assert!(rms(&tail[2048..]) > 0.005, "envelope release keeps sounding");
        assert!(ins.voices()[0].active());
    }
}

// ---- noise + filters (task 3.3) ----
mod filter_tests {
    use super::harness::*;
    use crate::filter::{Ladder, Notch, Rbj, RbjKind};
    use crate::noise::Noise;
    use crate::params::*;

    /// Steady-state gain (dB) of a stereo filter at `hz`.
    fn gain_db(mut run: impl FnMut(&mut [f32], &mut [f32]), hz: f64) -> f64 {
        let n = 44100;
        let mut l: Vec<f32> = (0..n).map(|i| (0.1 * (2.0 * core::f64::consts::PI * hz * i as f64 / 44100.0).sin()) as f32).collect();
        let mut r = l.clone();
        for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
            run(a, b);
        }
        20.0 * (rms(&l[n / 2..]) / (0.1 / 2f64.sqrt())).log10()
    }

    /// Analytic |H| (dB) of a biquad `[b0,b1,b2,a1,a2]` at `hz` (a0 = 1).
    fn analytic(c: [f64; 5], hz: f64) -> f64 {
        let w = 2.0 * core::f64::consts::PI * hz / 44100.0;
        let z = |k: f64| (-(k * w)).cos();
        let zi = |k: f64| (-(k * w)).sin();
        let (nr, ni) = (c[0] + c[1] * z(1.0) + c[2] * z(2.0), c[1] * zi(1.0) + c[2] * zi(2.0));
        let (dr, di) = (1.0 + c[3] * z(1.0) + c[4] * z(2.0), c[3] * zi(1.0) + c[4] * zi(2.0));
        10.0 * ((nr * nr + ni * ni) / (dr * dr + di * di)).log10()
    }

    fn rbj_coefs(kind: RbjKind, f: f32, q: f32) -> [f64; 5] {
        let f0 = (f * f * 10000.0 + 30.0) as f64;
        let w0 = 2.0 * core::f64::consts::PI * f0 / 44100.0;
        let alpha = w0.sin() * ((2f64.log10() / 2.0) * (1.0 - q as f64) * w0 / w0.sin()).sinh();
        let a0 = 1.0 + alpha;
        let (b0, b1, b2) = match kind {
            RbjKind::HighPass => ((1.0 + w0.cos()) / 2.0, -(1.0 + w0.cos()), (1.0 + w0.cos()) / 2.0),
            RbjKind::BandPass => (w0.sin() / 2.0, 0.0, -w0.sin() / 2.0),
        };
        [b0 / a0, b1 / a0, b2 / a0, -2.0 * w0.cos() / a0, (1.0 - alpha) / a0]
    }

    #[test]
    fn rbj_highpass_and_bandpass_follow_their_design() {
        for kind in [RbjKind::HighPass, RbjKind::BandPass] {
            for (f, q) in [(0.3f32, 0.2f32), (0.5, 0.7)] {
                let fc = (f * f * 10000.0 + 30.0) as f64;
                let mut flt = Rbj::default();
                flt.set(kind, f, q);
                let c = rbj_coefs(kind, f, q);
                for hz in [fc / 2.0, fc, fc * 2.0] {
                    let got = gain_db(|l, r| flt.run(l, r), hz);
                    assert!((got - analytic(c, hz)).abs() < 0.1, "{kind:?} f {f} q {q} @ {hz}: {got} vs {}", analytic(c, hz));
                }
            }
        }
        // shape: high-pass passes above, cuts below; band-pass peaks at fc
        let mut hp = Rbj::default();
        hp.set(RbjKind::HighPass, 0.3, 0.0);
        let fc = 0.09 * 10000.0 + 30.0;
        assert!(gain_db(|l, r| hp.run(l, r), fc * 2.0) > -3.0);
        let mut hp = Rbj::default();
        hp.set(RbjKind::HighPass, 0.3, 0.0);
        assert!(gain_db(|l, r| hp.run(l, r), fc / 2.0) < -9.0);
    }

    #[test]
    fn notch_cuts_at_cutoff_and_follows_its_design() {
        for (f, q) in [(0.3f32, 0.8f32), (0.4, 0.3)] {
            let fc = (f * f * 20000.0 + 30.0) as f64;
            let z = (2.0 * core::f64::consts::PI * fc / 44100.0).cos();
            let (qd, b0) = (q as f64, (1.0 - q as f64).powi(2) / (2.0 * (z.abs() + 1.0)) + q as f64);
            let c = [b0, -2.0 * z * b0, b0, -2.0 * z * qd, qd * qd];
            for hz in [fc / 2.0, fc * 2.0] {
                let mut n = Notch::default();
                n.set(f, q);
                let got = gain_db(|l, r| n.run(l, r), hz);
                assert!((got - analytic(c, hz)).abs() < 0.1, "f {f} q {q} @ {hz}: {got} vs {}", analytic(c, hz));
            }
            let mut n = Notch::default();
            n.set(f, q);
            assert!(gain_db(|l, r| n.run(l, r), fc) < -30.0, "zero on the unit circle at fc");
        }
        // one sample of latency: an impulse comes out one sample late
        let mut n = Notch::default();
        n.set(0.3, 0.8);
        let (mut l, mut r) = ([0.0f32; 4], [0.0f32; 4]);
        l[0] = 1.0;
        r[0] = 1.0;
        n.run(&mut l, &mut r);
        assert_eq!(l[0], 0.0);
        assert!(l[1] != 0.0);
    }

    #[test]
    fn ladder_lowpass_slopes() {
        let f = 0.2f32; // 830 Hz
        let fc = (f * f * 20000.0 + 30.0) as f64;
        let g = |hz: f64| {
            let mut lp = Ladder::default();
            lp.set(f, 0.0);
            gain_db(|l, r| lp.run(l, r), hz)
        };
        let (below, at, above) = (g(fc / 2.0), g(fc), g(fc * 2.0));
        assert!(below > -3.0, "{below}");
        assert!(at < below && at > -12.0, "{at}");
        assert!(above < -15.0, "4-pole: {above}");
        // resonance lifts the cutoff region
        let mut lp = Ladder::default();
        lp.set(f, 0.8);
        assert!(gain_db(|l, r| lp.run(l, r), fc) > at + 3.0);
    }

    #[test]
    fn filters_off_are_bypass() {
        // setting cutoffs/resonances with every On switch off changes nothing
        let mut a = sine();
        a[NOISE_AMOUNT] = 0.5;
        let mut b = a;
        for (on, cut, res) in [(LP_ON, LP_CUT, LP_RES), (HP_ON, HP_CUT, HP_RES), (BP_ON, BP_CUT, BP_Q), (NT_ON, NT_CUT, NT_Q)] {
            b[on] = 0.5; // on only if > 0.5
            b[cut] = 0.3;
            b[res] = 0.6;
        }
        let go = |p: &[f32; COUNT]| {
            let mut ins = instrument();
            ins.set_patch(p).unwrap();
            ins.note_on(60, 100);
            render(&mut ins, 10).0
        };
        assert_eq!(go(&a), go(&b));
    }

    #[test]
    fn noise_is_uniform_decorrelated_and_band_passable() {
        let mut p = [0.0f32; COUNT];
        p[NOISE_AMOUNT] = 1.0;
        p[NOISE_BW] = 1.0;
        let mut n = Noise::default();
        n.offset = [100, 20000];
        n.update(&p, 1.0, 1.0);
        let (mut l, mut r) = (vec![0.0f32; 44100], vec![0.0f32; 44100]);
        for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
            n.run(a, b);
        }
        // uniform on [−1, 1]: variance 1/3, flat histogram
        let var = l.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / l.len() as f64;
        assert!((var - 1.0 / 3.0).abs() < 0.01, "{var}");
        let mut hist = [0usize; 4];
        l.iter().for_each(|v| hist[((v + 1.0) * 2.0).clamp(0.0, 3.99) as usize] += 1);
        assert!(hist.iter().all(|h| (*h as f64 / 11025.0 - 1.0).abs() < 0.05), "{hist:?}");
        let corr = l.iter().zip(&r).map(|(a, b)| (*a * *b) as f64).sum::<f64>() / l.len() as f64;
        assert!(corr.abs() < 0.01);
        // silent at ≤ 0.01
        n.update(&p, 0.01, 1.0);
        let (mut a, mut b) = ([1.0f32; 512], [1.0f32; 512]);
        n.run(&mut a, &mut b);
        assert!(a.iter().all(|v| *v == 0.0));
        // band-pass around the middle: energy concentrated near its centre
        p[NOISE_BW] = 0.1;
        p[NOISE_FREQ] = 0.4;
        let mut n = Noise::default();
        n.update(&p, 1.0, 1.0);
        let mut l = vec![0.0f32; 44100];
        let mut r = vec![0.0f32; 44100];
        for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
            n.run(a, b);
        }
        let (lo, mid, hi) = (mag(&l, 100.0), mag(&l, 2000.0), mag(&l, 15000.0));
        assert!(mid > 3.0 * lo && mid > 3.0 * hi, "{lo} {mid} {hi}");
    }
}

// ---- effects (tasks 4.1–4.5) ----
#[cfg(feature = "tf-fx-all")]
mod fx_tests {
    use super::harness::*;
    use crate::fx::{kind, Chain, Fx};
    use crate::params::*;
    use crate::rng::Xorshift;
    use crate::voice::BLOCK;
    use synth_core::pool::Pool;

    fn leak_pool(bytes: usize) -> Pool<'static> {
        Pool::new(Box::leak(vec![0u8; bytes].into_boxed_slice()))
    }

    fn fx(k: u8) -> Fx<'static> {
        let mut pool = leak_pool(Fx::need(k) + 64);
        Fx::create(k, &mut pool, &mut Xorshift::new()).unwrap()
    }

    /// Runs `f` over `x` (same signal both channels) in 512-frame blocks.
    fn run(f: &mut Fx, p: &[f32; COUNT], l: &[f32], r: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let (mut a, mut b) = (l.to_vec(), r.to_vec());
        for (x, y) in a.chunks_mut(BLOCK).zip(b.chunks_mut(BLOCK)) {
            f.run(p, x, y);
        }
        (a, b)
    }

    fn impulse(n: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; n];
        v[0] = 1.0;
        v
    }

    fn fx_patch(slots: &[u8]) -> [f32; COUNT] {
        let mut p = sine();
        for (i, &k) in slots.iter().enumerate() {
            p[EFFECT_FIRST + i] = k as f32 / 10.0;
        }
        p
    }

    // 4.1
    #[test]
    fn reserved_kinds_pass_through() {
        let dry = {
            let mut ins = instrument();
            ins.set_patch(&sine()).unwrap();
            ins.note_on(60, 100);
            render(&mut ins, 6).0
        };
        for k in [8u8, 9, 10] {
            let mut ins = instrument();
            ins.set_patch(&fx_patch(&[k])).unwrap();
            ins.note_on(60, 100);
            let mut out = vec![];
            let mut buf = [[0.0f32; BLOCK]; 2];
            for _ in 0..6 {
                ins.render(&mut buf);
                out.extend_from_slice(&buf[0]);
            }
            assert_eq!(out, dry, "kind {k}");
            assert!(!ins.fx().active(0));
        }
    }

    #[test]
    fn chain_need_and_out_of_pool_slot_acts_as_none() {
        let p = fx_patch(&[kind::DELAY, kind::REVERB, kind::DISTORTION]);
        assert_eq!(Chain::need(&p), Fx::need(kind::DELAY) + Fx::need(kind::REVERB));
        // a pool too small for the delay: that slot passes through, the others still run
        let mut chain = Chain::new(leak_pool(Fx::need(kind::REVERB) + 64));
        let (mut l, mut r) = ([0.5f32; BLOCK], [0.5f32; BLOCK]);
        chain.run(&p, &mut Xorshift::new(), &mut l, &mut r);
        assert!(!chain.active(0) && chain.active(1) && chain.active(2));
    }

    #[test]
    fn effects_sleep_after_two_silent_seconds_and_wake_on_note() {
        let mut p = fx_patch(&[kind::DELAY]);
        p[DELAY_LEFT] = 0.1;
        p[DELAY_RIGHT] = 0.1;
        p[DELAY_DECAY] = 0.5;
        let mem: &'static mut [u8] = Box::leak(vec![0u8; crate::instrument::Instrument::need(Chain::need(&p)) + 64].into_boxed_slice());
        let pool: &'static mut Pool<'static> = Box::leak(Box::new(Pool::new(mem)));
        let mut ins = crate::instrument::Instrument::new(pool, Chain::need(&p)).unwrap();
        ins.set_patch(&p).unwrap();
        let mut buf = [[0.0f32; BLOCK]; 2];
        ins.note_on(60, 100);
        for _ in 0..10 {
            ins.render(&mut buf);
        }
        ins.note_off(60);
        let mut blocks = 0;
        while !ins.asleep() {
            ins.render(&mut buf);
            blocks += 1;
            assert!(blocks < 2000);
        }
        // echoes decay by ½ per 100 ms → below 1e-5 after ~1.5 s, then 2 s of silence
        assert!(blocks as f32 * 512.0 / 44100.0 > 2.0);
        // asleep: the delay line still holds tiny echoes, but nothing comes out
        ins.render(&mut buf);
        assert!(buf.iter().flatten().all(|v| *v == 0.0));
        ins.note_on(60, 100);
        ins.render(&mut buf);
        assert!(!ins.asleep() && buf[0].iter().any(|v| *v != 0.0));
    }

    // 4.2
    #[test]
    fn distortion_transfer_curve() {
        use crate::fx::distortion::shape;
        let q = |x: f32| (x * 32767.0) as u32 as f32 / 32768.0;
        assert_eq!(shape(0.5, 1.0), q(0.5)); // amount 0: (quantised) identity
        assert_eq!(shape(-0.25, 1.0), -q(0.25));
        assert_eq!(shape(3.0, 1.0), q(1.0)); // clips at 1
        assert!((shape(0.25, 0.5) - libm::sqrtf(q(0.25))).abs() < 1e-6); // amount 0.5: √
        assert_eq!(shape(0.0, 0.5), 0.0);
        assert_eq!(shape(0.001, 0.0), 1.0); // amount 1: square
        assert_eq!(shape(-0.001, 0.0), -1.0);
    }

    #[test]
    fn delay_impulse_positions_and_gains() {
        let mut p = [0.0f32; COUNT];
        p[DELAY_LEFT] = 0.01; // 10 ms = 441
        p[DELAY_RIGHT] = 0.02; // 882
        p[DELAY_DECAY] = 0.5;
        let mut d = fx(kind::DELAY);
        let (l, r) = run(&mut d, &p, &impulse(4096), &impulse(4096));
        assert_eq!(l[0], 1.5, "dry boosted by 1 + k (zero-latency copy)");
        assert_eq!((l[441], l[882], l[1323]), (0.25, 0.125, 0.0625));
        assert_eq!((r[882], r[1764]), (0.25, 0.125));
        let echoes_l: Vec<usize> = (1..4096).filter(|&i| l[i] != 0.0).collect();
        assert_eq!(&echoes_l[..3], &[441, 882, 1323]);
        assert_eq!(crate::fx::delay::length(1.0), 44100);
        assert_eq!(crate::fx::delay::length(0.0), 1);
    }

    // 4.3
    #[test]
    fn reverb_impulse_arrivals() {
        use crate::fx::reverb::{ALLPASSES, COMBS, SPREAD};
        let mut p = [0.0f32; COUNT];
        p[REV_ROOM] = 0.5;
        p[REV_WET] = 1.0; // dry 0
        p[REV_WIDTH] = 1.0; // no cross-feed
        let mut rv = fx(kind::REVERB);
        let n = 8192;
        let (l, r) = run(&mut rv, &p, &impulse(n), &vec![0.0; n]);
        let first = |x: &[f32]| x.iter().position(|v| *v != 0.0).unwrap();
        // shortest comb, then straight through the allpasses' direct path: same on both channels
        assert_eq!(first(&l), COMBS[0]);
        assert_eq!(first(&r), COMBS[0]);
        // identical combs: the channels match until the shortest allpass echoes (225 L, 248 R), then differ
        let ap = COMBS[0] + ALLPASSES.iter().min().unwrap();
        assert!(l[..ap].iter().zip(&r[..ap]).all(|(a, b)| a == b));
        assert_ne!(l[ap], r[ap]);
        assert!(r[ap + SPREAD] != 0.0);
        // more room → longer tail
        let tail = |room: f32| {
            let mut q = p;
            q[REV_ROOM] = room;
            let mut rv = fx(kind::REVERB);
            let (l, _) = run(&mut rv, &q, &impulse(3 * n), &vec![0.0; 3 * n]);
            rms(&l[2 * n..])
        };
        assert!(tail(1.0) > 2.0 * tail(0.0));
    }

    // 4.4
    #[test]
    fn chorus_flat_spot_and_wrapped_phase() {
        use crate::fx::Fx;
        let mut p = [0.0f32; COUNT];
        p[CHORUS_DEPTH] = 1.0;
        p[CHORUS_GAIN] = 1.0;
        p[CHORUS_RATE] = 1.0;
        let Fx::Chorus(mut c) = fx(kind::CHORUS) else { unreachable!() };
        assert!(c.phase.iter().all(|ph| (0.0..1.0).contains(ph)), "random start in [0, 1) rad");
        // put line 0 at sin φ = −1: (−1 + 0.5)·9 + 1 < 1 → flat at 1 ms (44 samples)
        c.phase[0] = 1.5 * core::f32::consts::PI;
        c.control(&p);
        let mut l = impulse(BLOCK);
        let mut r = vec![0.0; BLOCK];
        c.process(&mut l, &mut r);
        // three cascaded taps: first echo of line 0 at 44 samples
        assert!(l[44] != 0.0 && l[1..44].iter().all(|v| *v == 0.0));
        // ten hours of block-rate control: phases stay wrapped and finite
        let blocks = 10 * 3600 * 44100 / BLOCK;
        for _ in 0..blocks {
            c.control(&p);
        }
        assert!(c.phase.iter().all(|ph| ph.is_finite() && (0.0..2.0 * core::f32::consts::PI).contains(ph)));
        let mut l = vec![0.5f32; BLOCK];
        let mut r = vec![0.5f32; BLOCK];
        c.process(&mut l, &mut r);
        assert!(l.iter().chain(&r).all(|v| v.is_finite()));
        // the sweep keeps its rate: one more block advances each phase by the same step as at the start
        let before = c.phase[1];
        c.control(&p);
        let step = (c.phase[1] - before).rem_euclid(2.0 * core::f32::consts::PI);
        assert!((step - crate::lfo::rate(1.0)).abs() < 1e-5);
    }

    #[test]
    fn flanger_sweep_rates() {
        use crate::fx::Fx;
        let mut p = [0.0f32; COUNT];
        p[FLANGER_LFO] = 0.5;
        p[FLANGER_FREQ] = 0.5;
        p[FLANGER_AMP] = 1.0;
        p[FLANGER_WET] = 0.5;
        let Fx::Flanger(mut f) = fx(kind::FLANGER) else { unreachable!() };
        let (mut l, mut r) = (vec![0.1f32; 44100], vec![0.1f32; 44100]);
        for (a, b) in l.chunks_mut(BLOCK).zip(r.chunks_mut(BLOCK)) {
            f.run(&p, a, b);
        }
        // counter flips every sample (mean f ≈ 0.505): θ advances ≈ π·f/(2·sr) per sample
        let [tl, tr] = f.theta();
        let (hl, hr) = (tl as f64 / (2.0 * core::f64::consts::PI), tr as f64 / (2.0 * core::f64::consts::PI));
        assert!((hl - 0.126).abs() < 0.003, "left {hl} Hz");
        assert!((hr - 0.124).abs() < 0.003, "right {hr} Hz");
        assert!(l.iter().chain(&r).all(|v| v.is_finite() && v.abs() <= 1.0));
    }

    #[test]
    fn flanger_lfo_zero_sweeps_both_channels_together_and_stays_bounded() {
        use crate::fx::Fx;
        let mut p = [0.0f32; COUNT];
        p[FLANGER_AMP] = 1.0;
        p[FLANGER_WET] = 0.5;
        let Fx::Flanger(mut f) = fx(kind::FLANGER) else { unreachable!() };
        // past one fold (2²² samples ≈ 95 s): delays equal on L and R throughout, phase wrapped, output finite
        let mut block = [[0.2f32; BLOCK]; 2];
        let mut seen = 0usize;
        let total = (1usize << 22) + 20 * BLOCK;
        let mut max_delta = 0;
        while seen < total {
            let [l, r] = &mut block;
            f.run(&p, l, r);
            let d = f.deltas(1.0);
            assert_eq!(d[0], d[1]);
            max_delta = max_delta.max(d[0]);
            seen += BLOCK;
        }
        // 480 s period: after ≈ 95 s the running phase is ≈ 2π·95/480 ≈ 1.25 rad, delay already well up the sweep
        let run_phase = seen as f64 * core::f64::consts::PI / (44100.0 * 240.0);
        assert!((run_phase - 2.0 * core::f64::consts::PI * seen as f64 / 44100.0 / 480.0).abs() < 1e-9);
        assert!(max_delta > 50, "{max_delta}");
        assert!(f.theta().iter().all(|t| (0.0..2.0 * core::f32::consts::PI).contains(t)));
        assert!(block.iter().flatten().all(|v| v.is_finite()));
    }

    // 4.5
    #[test]
    fn formant_vowel_peaks() {
        // soprano formants F1/F2 (Hz) of the published vowels
        let want = [(795.0, 1150.0), (345.0, 2000.0), (265.0, 2145.0), (445.0, 795.0), (320.0, 700.0)];
        for (v, &(f1, f2)) in want.iter().enumerate() {
            let mut p = [0.0f32; COUNT];
            p[FORMANT_TYPE] = v as f32 / 4.0;
            p[FORMANT_WET] = 1.0;
            let mut flt = fx(kind::FORMANT);
            let n = 16384;
            let (h, _) = run(&mut flt, &p, &impulse(n), &impulse(n));
            // magnitude of the impulse response on a 5 Hz grid; first two local maxima above 200 Hz
            let m: Vec<(f64, f64)> = (40..1000).map(|k| (k as f64 * 5.0, mag(&h, k as f64 * 5.0))).collect();
            let peaks: Vec<f64> = (1..m.len() - 1).filter(|&i| m[i].1 > m[i - 1].1 && m[i].1 > m[i + 1].1).map(|i| m[i].0).collect();
            assert!((peaks[0] - f1).abs() / f1 < 0.03, "vowel {v}: F1 {} vs {f1}", peaks[0]);
            assert!((peaks[1] - f2).abs() / f2 < 0.03, "vowel {v}: F2 {} vs {f2}", peaks[1]);
        }
    }

    #[test]
    fn eq_band_gains() {
                let probe = |gains: [f32; 3], hz: f64| {
            let mut p = [0.0f32; COUNT];
            p[EQ_LOW..EQ_LOW + 3].copy_from_slice(&gains);
            let mut e = fx(kind::EQ);
            let x: Vec<f32> = (0..22050).map(|i| (0.1 * (2.0 * core::f64::consts::PI * hz * i as f64 / 44100.0).sin()) as f32).collect();
            let (l, _) = run(&mut e, &p, &x, &x);
            20.0 * (rms(&l[11025..]) / (0.1 / 2f64.sqrt())).log10()
        };
        for hz in [100.0, 1000.0, 10000.0] {
            assert!(probe([0.5; 3], hz).abs() < 0.1, "unity at {hz}");
        }
        // a band at ×0 (p = 0) removes it where it dominates
        assert!(probe([0.0, 0.5, 0.5], 100.0) < -8.0);
        assert!(probe([0.5, 0.0, 0.5], 1000.0) < -8.0);
        assert!(probe([0.5, 0.5, 0.0], 10000.0) < -11.0);
        // boosts: p = 1 → ×11
        assert!(probe([1.0, 0.5, 0.5], 100.0) > 15.0);
        assert!(probe([0.5, 1.0, 0.5], 1000.0) > 15.0);
        assert!(probe([0.5, 0.5, 1.0], 10000.0) > 15.0);
    }
}

/// Spec "Disabled kind": built without `tf-fx-reverb`, a reverb slot acts as none and later slots still process.
#[cfg(all(not(feature = "tf-fx-reverb"), feature = "tf-fx-delay"))]
#[test]
fn disabled_kind_acts_as_none() {
    use crate::fx::{kind, Chain, Fx};
    use crate::params::*;
    let mut p = [0.0f32; COUNT];
    p[EFFECT_FIRST] = kind::REVERB as f32 / 10.0;
    p[EFFECT_FIRST + 1] = kind::DELAY as f32 / 10.0;
    p[DELAY_DECAY] = 0.5;
    assert_eq!(Fx::need(kind::REVERB), 0);
    let mem: &'static mut [u8] = Box::leak(vec![0u8; Chain::need(&p) + 64].into_boxed_slice());
    let mut chain = Chain::new(synth_core::pool::Pool::new(mem));
    let (mut l, mut r) = ([0.5f32; 512], [0.5f32; 512]);
    chain.run(&p, &mut crate::rng::Xorshift::new(), &mut l, &mut r);
    assert!(!chain.active(0) && chain.active(1));
    assert_eq!(l[0], 0.75); // delay's zero-latency copy: x·(1 + k)
}
