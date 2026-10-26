//! Diagnostics: live host (128-frame calls) against the offline song render (`--example livecmp -- x.song.json [seconds]`).
fn main() {
    let path = std::env::args().nth(1).expect("song.json");
    let secs: f64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(30.0);
    let s = song::load(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let blob = song::export(&s).unwrap();
    let off = song::render::render(&blob, None).unwrap();
    let mut h = synth_wasm::Host::new();
    h.load(&blob).unwrap();
    h.play(0);
    let n = ((secs * 44100.0) as usize).min(off.len() / 2);
    let mut live = Vec::with_capacity(2 * n + 256);
    while live.len() < 2 * n {
        live.extend_from_slice(h.render(128));
    }
    let (mut first, mut maxd) = (None, 0f32);
    for i in 0..2 * n {
        let d = (live[i] - off[i]).abs();
        if d > 1e-6 && first.is_none() { first = Some(i / 2); }
        maxd = maxd.max(d);
    }
    println!("{n} frames: first difference at {:?} ({:?} s), max |Δ| {maxd}", first, first.map(|f| f as f64 / 44100.0));
}
