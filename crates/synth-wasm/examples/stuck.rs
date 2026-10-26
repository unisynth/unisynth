//! Diagnostics: do voices accumulate across loop wraps, seeks and live edits? (`--example stuck -- song.json`)
use std::time::Instant;
fn voices(h: &synth_wasm::Host) -> usize {
    h.voice_counts().iter().sum()
}
fn run(h: &mut synth_wasm::Host, secs: f64) -> (f64, usize) {
    let n = (secs * 44100.0 / 128.0) as usize;
    let t = Instant::now();
    let mut peak = 0;
    for _ in 0..n {
        h.render(128);
        peak = peak.max(voices(h));
    }
    (t.elapsed().as_secs_f64() / secs * 100.0, peak)
}
fn main() {
    let path = std::env::args().nth(1).expect("song.json");
    let s = song::load(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let blob = song::export(&s).unwrap();
    let mut h = synth_wasm::Host::new();
    h.load(&blob).unwrap();
    let r = 44100u64;
    // 1. loop 60 s .. 64 s, 15 wraps
    h.set_loop(Some((60 * r, 64 * r)));
    h.play(60 * r);
    for i in 0..15 {
        let (cpu, pk) = run(&mut h, 4.0);
        println!("loop wrap {i:>2}: cpu {cpu:5.1}%  peak voices {pk:>4}  now {}", voices(&h));
    }
    h.set_loop(None);
    // 2. seeks while playing
    for i in 0..10 {
        h.seek((40 + 7 * i) * r);
        let (cpu, pk) = run(&mut h, 1.0);
        println!("seek {i:>2}: cpu {cpu:5.1}%  peak voices {pk:>4}  now {}", voices(&h));
    }
    // 3. live edits while playing (the same blob = no-op; a moved note = schedule swap)
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for i in 0..10 {
        if let Some(n) = v["clips"][0]["notes"].as_array_mut().and_then(|a| a.first_mut()) {
            n[2] = serde_json::json!(48 + (i % 12));
        }
        let b = song::export(&song::load(&v.to_string()).unwrap()).unwrap();
        h.update(&b).unwrap();
        let (cpu, pk) = run(&mut h, 1.0);
        println!("edit {i:>2}: cpu {cpu:5.1}%  peak voices {pk:>4}  now {}", voices(&h));
    }
    h.stop();
    let (cpu, pk) = run(&mut h, 3.0);
    println!("after stop: cpu {cpu:5.1}%  peak {pk}  now {}", voices(&h));
}
