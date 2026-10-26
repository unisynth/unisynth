//! Diagnostics: sounding voices vs held notes over a song (`cargo run --release -p synth-wasm --example voices -- x.song.json`).
use std::time::Instant;
use synth_core::device::EventKind;
fn main() {
    let path = std::env::args().nth(1).expect("song.json");
    let s = song::load(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let m = song::export_model(&s).unwrap();
    let blob = m.write();
    // held notes per lane over time, from the blob's note events
    let len = (m.length * 44100.0) as u64;
    let mut held_at = vec![0i32; (len / 44100 + 2) as usize];
    for lane in &m.lanes {
        let mut on: std::collections::BTreeMap<u8, u64> = Default::default();
        for &(time, ref e) in lane {
            match *e {
                EventKind::NoteOn { note, .. } => {
                    on.insert(note, time as u64);
                }
                EventKind::NoteOff { note } => {
                    if let Some(t0) = on.remove(&note) {
                        for sec in (t0 / 44100)..=(time as u64 / 44100) {
                            if (sec as usize) < held_at.len() {
                                held_at[sec as usize] += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    let mut h = synth_wasm::Host::new();
    h.load(&blob).unwrap();
    h.play(0);
    let (mut f, t) = (0u64, Instant::now());
    let mut sec_cost = 0f64;
    let mut peak = (0, 0);
    while f < len {
        let c = Instant::now();
        h.render(128);
        sec_cost += c.elapsed().as_secs_f64();
        f += 128;
        if f % 44100 < 128 {
            let v = h.voice_counts();
            let tot: usize = v.iter().sum();
            if tot > peak.0 {
                peak = (tot, f / 44100);
            }
            let busiest: Vec<String> = v
                .iter()
                .enumerate()
                .filter(|x| *x.1 > 8)
                .map(|(i, n)| format!("d{i}:{n}"))
                .collect();
            if f / 44100 % 10 == 0 || tot > 64 {
                println!(
                    "{:>4}s voices {:>4} notes~{:>3} cpu {:>5.1}%  {}",
                    f / 44100,
                    tot,
                    held_at[(f / 44100) as usize],
                    sec_cost * 100.0,
                    busiest.join(" ")
                );
            }
            sec_cost = 0.0;
        }
    }
    println!(
        "peak {} voices at {}s; {:.1}s audio in {:.2}s",
        peak.0,
        peak.1,
        len as f64 / 44100.0,
        t.elapsed().as_secs_f64()
    );
}
