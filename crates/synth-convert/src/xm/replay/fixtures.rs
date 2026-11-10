//! FT2 reference fixtures for the replay extensions (change add-plain-xm-import, task 1.2): small one-channel XMs, one
//! per feature, and what `openmpt123` makes of them, recorded in `tests/data/ft2_fixtures.json` (per tick: left and
//! right peak and the pitch in Hz; per trigger: its onset frame and peak). No XM or audio is committed: the songs are
//! built here, and `cargo test -p synth-convert record_ft2_fixtures -- --ignored` (needs `openmpt123`) re-records.
//!
//! Every song runs at speed 6, BPM 125 (a tick is 882 frames at 44.1 kHz) on note C-5. Instrument 1 is a looped sine
//! (32 frames a cycle, half scale) for volume, pan and pitch; instrument 2 a 256-frame sine burst, unlooped, for
//! retrigger onsets. The pan song opens with set-pan rows (0, 0x40, 0x80, 0xC0, 0xFF) that calibrate openmpt's pan law.

use super::super::{Cell, Env, Instrument, Pattern, Sample, Xm};

pub const RATE: u32 = 44100;
pub const TICK_FRAMES: usize = 882;
const C5: u8 = 61;

fn sine(frames: usize, looped: bool) -> Sample {
    let data = (0..frames).map(|i| (16384.0 * (std::f64::consts::TAU * i as f64 / 32.0).sin()).round() as i16).collect();
    let (loop_start, loop_len, loop_kind) = if looped { (0, frames as u32, 1) } else { (0, 0, 0) };
    Sample { name: "sine".into(), loop_start, loop_len, volume: 48, finetune: 0, loop_kind, bits16: true, pan: 128, relative: 0, data }
}

fn ins(name: &str, s: Sample, vibrato: [u8; 4]) -> Instrument {
    Instrument { name: name.into(), keymap: [0; 96], vol_env: Env::default(), pan_env: Env::default(), vibrato, fadeout: 0, samples: vec![s] }
}

fn c(note: u8, instrument: u8, volume: u8, effect: u8, param: u8) -> Cell {
    Cell { note, instrument, volume, effect, param }
}

fn fx(effect: u8, param: u8) -> Cell {
    c(0, 0, 0, effect, param)
}

fn vol(volume: u8) -> Cell {
    c(0, 0, volume, 0, 0)
}

fn song(rows: Vec<Cell>, vibrato: [u8; 4]) -> Xm {
    let instruments = vec![ins("tone", sine(1024, true), vibrato), ins("burst", sine(256, false), [0; 4])];
    let n = rows.len() as u16 + 1;
    let mut cells = rows;
    cells.push(Cell::default());
    Xm { name: "ft2 fixture".into(), channels: 1, restart: 0, linear: true, speed: 6, bpm: 125, order: vec![0], patterns: vec![Pattern { rows: n, cells }], instruments, mix: Default::default() }
}

// XM effect numbers past F: G 0x10, H 0x11, P 0x19, R 0x1B, X 0x21
pub fn fixtures() -> Vec<(&'static str, Xm)> {
    let tone = |volume: u8, effect: u8, param: u8| c(C5, 1, volume, effect, param);
    vec![
        // volume 40, then the volume column's fine slide down 3, then up 2
        ("volcol_fine", song(vec![tone(0x10 + 40, 0, 0), vol(0x83), vol(0x92)], [0; 4])),
        // R43 (every 3 ticks, x = 4: −8), its memory (R00), then every tick without a volume change (R81)
        ("retrig", song(vec![c(C5, 2, 0x10 + 48, 0x1B, 0x43), fx(0x1B, 0x43), fx(0x1B, 0x00), fx(0x1B, 0x81)], [0; 4])),
        // global 32, slide down 2 a tick, the memory, then up 3
        ("global_slide", song(vec![tone(0x50, 0x10, 0x20), fx(0x11, 0x02), fx(0x11, 0x00), fx(0x11, 0x30)], [0; 4])),
        // calibration 0, 0x40, 0x80, 0xC0, 0xFF; centre; slide left 4; the memory; slide right 4
        (
            "pan_slide",
            song(vec![tone(0x50, 8, 0), fx(8, 0x40), fx(8, 0x80), fx(8, 0xC0), fx(8, 0xFF), fx(8, 0x80), fx(0x19, 0x04), fx(0x19, 0x00), fx(0x19, 0x40)], [0; 4]),
        ),
        // extra-fine porta up 2, its memory, down 3, its memory
        ("extra_fine", song(vec![tone(0x50, 0, 0), fx(0x21, 0x12), fx(0x21, 0x10), fx(0x21, 0x23), fx(0x21, 0x20)], [0; 4])),
        // auto-vibrato (type, sweep, depth, rate): sine with a sweep, square, ramp down, ramp up
        ("autovib_sine_sweep", song(vec![tone(0x50, 0, 0), Cell::default(), Cell::default(), Cell::default()], [0, 4, 8, 16])),
        ("autovib_square", song(vec![tone(0x50, 0, 0), Cell::default(), Cell::default(), Cell::default()], [1, 0, 8, 16])),
        ("autovib_ramp_down", song(vec![tone(0x50, 0, 0), Cell::default(), Cell::default(), Cell::default()], [2, 0, 8, 16])),
        ("autovib_ramp_up", song(vec![tone(0x50, 0, 0), Cell::default(), Cell::default(), Cell::default()], [3, 0, 8, 16])),
        // F00 (speed 0) on row 2 with a burst note; row 3 would play another
        ("stop", song(vec![tone(0x50, 0, 0), Cell::default(), c(C5, 2, 0x50, 0xF, 0), c(C5, 2, 0x50, 0, 0)], [0; 4])),
    ]
}

/// What openmpt rendered for one fixture.
#[derive(Debug)]
pub struct Rendered {
    /// Per tick: left and right peak over the tick's last 90 frames (one cycle of the tone at C-5; openmpt ramps a
    /// volume change across the tick, so its end holds the tick's setting) and the pitch over its last 256 frames (Hz,
    /// from rising zero crossings; 0 when silent).
    pub ticks: Vec<(f64, f64, f64)>,
    /// Per trigger: its onset frame (a 32-frame block peaking at 2.5 times the one before, 64 frames after the last), and its peak over the next 256 frames.
    pub onsets: Vec<(usize, f64)>,
}

impl Rendered {
    fn json(&self) -> serde_json::Value {
        serde_json::json!({ "ticks": self.ticks.iter().map(|t| [t.0, t.1, t.2]).collect::<Vec<_>>(), "onsets": self.onsets.iter().map(|o| serde_json::json!([o.0, o.1])).collect::<Vec<_>>() })
    }
}

pub fn recorded() -> std::collections::BTreeMap<String, Rendered> {
    let v: serde_json::Value = serde_json::from_str(include_str!("../../../tests/data/ft2_fixtures.json")).unwrap();
    let f = |x: &serde_json::Value| x.as_f64().unwrap();
    v.as_object()
        .unwrap()
        .iter()
        .map(|(k, r)| {
            let ticks = r["ticks"].as_array().unwrap().iter().map(|t| (f(&t[0]), f(&t[1]), f(&t[2]))).collect();
            let onsets = r["onsets"].as_array().unwrap().iter().map(|o| (o[0].as_u64().unwrap() as usize, f(&o[1]))).collect();
            (k.clone(), Rendered { ticks, onsets })
        })
        .collect()
}

/// The fixture's tone (instrument 1) pitch at a frequency `f`: the sample plays 32 frames a cycle.
pub fn tone_hz(freq: f64) -> f64 {
    freq / 32.0
}

fn analyse(l: &[f32], r: &[f32], ticks: usize) -> Rendered {
    let mono: Vec<f64> = l.iter().zip(r).map(|(a, b)| (*a + *b) as f64).collect();
    let mut out = Rendered { ticks: vec![], onsets: vec![] };
    for t in 0..ticks {
        let (a, b) = ((t + 1) * TICK_FRAMES - 256, (t + 1) * TICK_FRAMES);
        if b > mono.len() {
            break;
        }
        let top = |x: &[f32]| x[b - 90..b].iter().fold(0.0f64, |m, v| m.max(v.abs() as f64));
        let ups: Vec<f64> = (a..b - 1).filter(|&i| mono[i] <= 0.0 && mono[i + 1] > 0.0).map(|i| i as f64 + mono[i] / (mono[i] - mono[i + 1])).collect();
        let hz = if ups.len() >= 3 { (ups.len() - 1) as f64 / (ups[ups.len() - 1] - ups[0]) * RATE as f64 } else { 0.0 };
        let round = |v: f64, d: i32| (v * 10f64.powi(d)).round() / 10f64.powi(d);
        out.ticks.push((round(top(l), 7), round(top(r), 7), round(hz, 4)));
    }
    // a trigger: a 32-frame block whose peak is 2.5 times the previous block's; its onset is the block's first frame
    // above half that peak
    let peak = |a: usize, b: usize| mono[a..b.min(mono.len())].iter().fold(0.0f64, |m, x| m.max(x.abs()));
    let mut prev = 0.0;
    for blk in (0..mono.len()).step_by(32) {
        let p = peak(blk, blk + 32);
        if p > 1e-3 && p > 2.5 * prev && out.onsets.last().is_none_or(|o| blk >= o.0 + 64) {
            let i = (blk..blk + 32).find(|&i| mono[i].abs() > p / 2.0).unwrap();
            out.onsets.push((i, (peak(i, i + 256) * 1e6).round() / 1e6));
        }
        prev = p;
    }
    out
}

fn wav_f32(b: &[u8]) -> (Vec<f32>, Vec<f32>) {
    // RIFF, then chunks: fmt (format 3, 2 channels, 44100) and data
    let mut p = 12;
    while p + 8 <= b.len() {
        let id = &b[p..p + 4];
        let n = u32::from_le_bytes(b[p + 4..p + 8].try_into().unwrap()) as usize;
        if id == b"fmt " {
            assert_eq!(u16::from_le_bytes([b[p + 8], b[p + 9]]), 3, "float WAV");
            assert_eq!(u16::from_le_bytes([b[p + 10], b[p + 11]]), 2, "stereo");
        }
        if id == b"data" {
            let s: Vec<f32> = b[p + 8..p + 8 + n].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
            return (s.iter().step_by(2).copied().collect(), s.iter().skip(1).step_by(2).copied().collect());
        }
        p += 8 + n + (n & 1);
    }
    panic!("no data chunk");
}

#[test]
#[ignore = "re-records tests/data/ft2_fixtures.json with openmpt123"]
fn record_ft2_fixtures() {
    let dir = std::env::temp_dir().join(format!("ft2-fixtures-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut all = std::collections::BTreeMap::new();
    for (name, x) in fixtures() {
        let xm = dir.join(format!("{name}.xm"));
        std::fs::write(&xm, super::super::tests::write(&x)).unwrap();
        let o = std::process::Command::new("openmpt123")
            .args(["--render", "--force", "--samplerate", "44100", "--channels", "2", "--float", "--output-type", "wav", "--ramping", "0", "--stereo", "100", "--quiet"])
            .arg(&xm)
            .output()
            .expect("openmpt123");
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let (l, r) = wav_f32(&std::fs::read(dir.join(format!("{name}.xm.wav"))).unwrap());
        let ticks = x.patterns[0].rows as usize * x.speed as usize;
        all.insert(name.to_string(), analyse(&l, &r, ticks));
    }
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/ft2_fixtures.json");
    let mut s = String::from("{\n");
    for (i, (k, v)) in all.iter().enumerate() {
        s += &format!("  {}: {}{}\n", serde_json::to_string(k).unwrap(), serde_json::to_string(&v.json()).unwrap(), if i + 1 < all.len() { "," } else { "" });
    }
    std::fs::write(file, s + "}\n").unwrap();
    match std::env::var_os("FT2_FIXTURES_KEEP") {
        Some(_) => eprintln!("kept {}", dir.display()),
        None => std::fs::remove_dir_all(&dir).unwrap(),
    }
}
