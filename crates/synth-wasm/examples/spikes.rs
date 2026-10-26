//! Diagnostics: the slowest 128-frame render calls of a song and what happened in them.
use std::time::Instant;
use synth_core::device::EventKind;
fn main() {
    let path = std::env::args().nth(1).expect("song.json");
    let s = song::load(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let m = song::export_model(&s).unwrap();
    let blob = m.write();
    let len = (m.length * 44100.0) as u64;
    // note-ons per 128-frame call
    let mut ons = vec![0u32; (len / 128 + 2) as usize];
    for lane in &m.lanes {
        for &(t, ref e) in lane {
            if let EventKind::NoteOn { .. } = e {
                if t >= 0 && ((t as u64) / 128) < ons.len() as u64 {
                    ons[(t as u64 / 128) as usize] += 1;
                }
            }
        }
    }
    let mut runs: Vec<Vec<f64>> = Vec::new();
    for pass in 0..3 {
        let mut h = synth_wasm::Host::new();
        h.load(&blob).unwrap();
        h.play(0);
        let mut calls: Vec<(f64, u64, usize)> = Vec::new();
        let mut f = 0u64;
        while f < len {
            let c = Instant::now();
            h.render(128);
            let dt = c.elapsed().as_secs_f64() * 1000.0;
            calls.push((dt, f, 0));
            f += 128;
        }
        let mean = calls.iter().map(|c| c.0).sum::<f64>() / calls.len() as f64;
        let mut sorted: Vec<f64> = calls.iter().map(|c| c.0).collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let pct = |p: f64| sorted[((sorted.len() - 1) as f64 * p) as usize];
        println!(
            "pass {pass}: mean {:.3} p99 {:.3} p99.9 {:.3} max {:.3} ms; over 2.9 ms: {}",
            mean,
            pct(0.99),
            pct(0.999),
            sorted[sorted.len() - 1],
            sorted.iter().filter(|&&x| x > 2.9).count()
        );
        runs.push(calls.iter().map(|c| c.0).collect());
        if pass == 99 {
            calls.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            for c in calls.iter().take(12) {
                let i = (c.1 / 128) as usize;
                let near: u32 = ons[i.saturating_sub(1)..(i + 2).min(ons.len())]
                    .iter()
                    .sum();
                println!(
                    "  {:.3} ms at {:.2}s (frame {}), note-ons in this±1 call: {}, sub-block start in call: {}",
                    c.0,
                    c.1 as f64 / 44100.0,
                    c.1,
                    near,
                    (c.1 / 256 != (c.1 + 127) / 256) as u8
                );
            }
        }
    }
    // a slow call that repeats in every pass is engine work; one that does not is the machine
    let n = runs[0].len();
    let rep = (0..n)
        .filter(|&i| runs.iter().all(|r| r[i] > 2.0))
        .collect::<Vec<_>>();
    let any = (0..n).filter(|&i| runs.iter().any(|r| r[i] > 2.0)).count();
    println!(
        "calls > 2 ms in any pass: {any}; in all 3 passes: {} {:?}",
        rep.len(),
        rep.iter()
            .take(10)
            .map(|&i| format!(
                "{:.2}s {:.1}/{:.1}/{:.1}",
                i as f64 * 128.0 / 44100.0,
                runs[0][i],
                runs[1][i],
                runs[2][i]
            ))
            .collect::<Vec<_>>()
    );
    let mut med: Vec<f64> = (0..n)
        .map(|i| {
            let mut v = [runs[0][i], runs[1][i], runs[2][i]];
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v[1]
        })
        .collect();
    med.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "median-of-3 per call: p99 {:.3} p99.9 {:.3} max {:.3} ms",
        med[n * 99 / 100],
        med[n * 999 / 1000],
        med[n - 1]
    );
}
