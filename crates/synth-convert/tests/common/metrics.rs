//! Loudness and octave-band metrics shared by the parity tests (design D11: 1 dB loudness, 3 dB per band).

fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        for i in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (c, s) = ((ang * k as f64).cos(), (ang * k as f64).sin());
                let (a, b) = (i + k, i + k + len / 2);
                let (tr, ti) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

/// RMS in dB and the energy in dB of the octave bands centred at 63 Hz … 16 kHz.
pub fn metrics(x: &[f32], rate: f64) -> (f64, [f64; 9]) {
    let mono: Vec<f64> = x.chunks(2).map(|c| (c[0] + c[1]) as f64 * 0.5).collect();
    let rms = (mono.iter().map(|v| v * v).sum::<f64>() / mono.len() as f64).sqrt();
    let n = mono.len().next_power_of_two();
    let (mut re, mut im) = (vec![0.0; n], vec![0.0; n]);
    re[..mono.len()].copy_from_slice(&mono);
    fft(&mut re, &mut im);
    let mut bands = [0.0f64; 9];
    for (b, e) in bands.iter_mut().enumerate() {
        let c = 62.5 * 2f64.powi(b as i32);
        let (lo, hi) = (c / 2f64.sqrt(), c * 2f64.sqrt());
        let e_sum: f64 = (1..n / 2)
            .filter(|&k| {
                let f = k as f64 * rate / n as f64;
                f >= lo && f < hi
            })
            .map(|k| re[k] * re[k] + im[k] * im[k])
            .sum();
        *e = 10.0 * (e_sum / (n as f64 * n as f64) + 1e-20).log10();
    }
    (20.0 * (rms + 1e-12).log10(), bands)
}

/// Worst loudness difference and worst band difference (bands where either side has real energy).
pub fn compare(name: &str, a: &[f32], b: &[f32]) -> (f64, f64) {
    let (la, ba) = metrics(a, 44100.0);
    let (lb, bb) = metrics(b, 44100.0);
    let loud = (la - lb).abs();
    // bands within 24 dB of the loudest one (the others are leakage and noise floor)
    let top = ba.iter().chain(&bb).cloned().fold(f64::MIN, f64::max);
    let band = ba
        .iter()
        .zip(&bb)
        .filter(|(x, y)| x.max(**y) > top - 24.0)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max);
    if std::env::var_os("BANDS").is_some() {
        eprintln!("  bands legacy  {:?}", ba.map(|v| (v * 10.0).round() / 10.0));
        eprintln!("  bands unified {:?}", bb.map(|v| (v * 10.0).round() / 10.0));
    }
    eprintln!("{name:28} legacy {la:7.2} dB  unified {lb:7.2} dB  loudness Δ {loud:5.2}  worst band Δ {band:5.2}");
    (loud, band)
}

