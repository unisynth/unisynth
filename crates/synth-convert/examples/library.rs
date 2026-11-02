//! Converts the legacy library entries the DAW lists (change add-synth-daw task 5.7b) to song/1 devices, so the
//! library can load them into a track: every Tunefish 4 program (`tf4programs/programN.txt`, instrument + its insert
//! effects) and every WaveSabre instrument instance of the reference songs (`ws/refs/*.bin`, in device order).
//!
//!   cargo run --release -p synth-convert --example library -- [<synth-lab dir>] [<out.json>]
//!
//! Defaults: `$SYNTH_LAB` or `~/Downloads/64k-tools/synth-lab`, `project/music/.library.json` (git-ignored: built
//! from the local lab; the DAW server attaches it to `/api/library`). Output:
//! `{ "tunefish": { "<n>": { "devices": [...], "warnings": [...] } }, "wavesabre": { "<file>": [ { "devices", "warnings" } ] } }`.
//! Each entry goes through the same path as a song: legacy → `synth-convert` → `song::import` (the track's devices).

use std::collections::BTreeMap;
use std::path::PathBuf;
use synth_convert::model::{Dev, SongM, Trk};
use wavesabre::kind as w;

type Entry = serde_json::Value;

/// The converted devices of the first track of a converted blob.
fn devices_of(conv: synth_convert::Converted) -> Result<Entry, String> {
    let s = song::import(&conv.blob)?;
    let devs = serde_json::to_value(&s.tracks.first().ok_or("no track")?.devices).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "devices": devs, "warnings": conv.warnings }))
}

fn tunefish(dir: &PathBuf) -> BTreeMap<String, Entry> {
    let mut out = BTreeMap::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for f in rd.flatten() {
        let name = f.file_name().to_string_lossy().to_string();
        let Some(n) = name.strip_prefix("program").and_then(|r| r.strip_suffix(".txt")).and_then(|r| r.parse::<u32>().ok()) else { continue };
        let text = std::fs::read_to_string(f.path()).unwrap_or_default();
        let mut p = vec![0.0f32; tfsynth::params::COUNT]; // a program lists all 112 values
        for line in text.lines().skip(1) {
            let mut it = line.splitn(2, ';');
            let (Some(k), Some(v)) = (it.next(), it.next()) else { continue };
            if let (Some(i), Ok(v)) = (tfsynth::params::NAMES.iter().position(|x| *x == k.trim()), v.trim().parse::<f32>()) {
                p[i] = v;
            }
        }
        let r = synth_convert::tfm::convert_patch(&p, &[(0, 60, 100), (8, 60, 0)], 120, 1.0)
            .map_err(|e| e.to_string())
            .and_then(devices_of);
        match r {
            Ok(e) => {
                out.insert(n.to_string(), e);
            }
            Err(e) => eprintln!("program{n}: {e}"),
        }
    }
    out
}

/// One instrument device alone on a track, converted.
fn ws_instrument(src: &SongM, di: usize) -> Result<Entry, String> {
    let one = SongM {
        tempo: src.tempo,
        rate: src.rate,
        length: 1.0,
        devices: vec![Dev { kind: src.devices[di].kind, chunk: src.devices[di].chunk.clone() }],
        lanes: vec![vec![]],
        tracks: vec![
            Trk { volume: 1.0, receives: vec![], devices: vec![0], lane: 0, autos: vec![] },
            Trk { volume: 1.0, receives: vec![(0, 0, 1.0)], devices: vec![], lane: 0, autos: vec![] },
        ],
    };
    devices_of(synth_convert::convert(&one.write().unwrap()).map_err(|e| e.to_string())?)
}

fn wavesabre(dir: &PathBuf) -> BTreeMap<String, Vec<Entry>> {
    let mut out = BTreeMap::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    let mut files: Vec<_> = rd.flatten().map(|f| f.path()).filter(|p| p.extension().is_some_and(|e| e == "bin")).collect();
    files.sort();
    for f in files {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let Ok(s) = std::fs::read(&f).map_err(|e| e.to_string()).and_then(|b| SongM::read(&b).map_err(|e| e.to_string())) else {
            eprintln!("{name}: unreadable");
            continue;
        };
        let insts: Vec<Entry> = (0..s.devices.len())
            .filter(|&i| matches!(s.devices[i].kind, w::FALCON | w::SLAUGHTER | w::THUNDER | w::ADULTERY | w::SPECIMEN))
            .map(|i| ws_instrument(&s, i).unwrap_or_else(|e| serde_json::json!({ "error": e })))
            .collect();
        out.insert(name, insts);
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let lab = args.first().map(PathBuf::from).or_else(|| std::env::var("SYNTH_LAB").ok().map(PathBuf::from)).unwrap_or_else(|| {
        PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Downloads/64k-tools/synth-lab")
    });
    let out = args.get(1).cloned().unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/../../project/music/.library.json").into());
    let tf = tunefish(&lab.join("tunefish/patches/tf4programs"));
    let ws = wavesabre(&lab.join("ws/refs"));
    let n_ws: usize = ws.values().map(|v| v.len()).sum();
    let json = serde_json::json!({ "format": "us-library/1", "tunefish": tf, "wavesabre": ws });
    std::fs::write(&out, serde_json::to_string(&json).unwrap()).unwrap();
    println!("{out}: {} Tunefish programs, {n_ws} WaveSabre instruments", tf.len());
}
