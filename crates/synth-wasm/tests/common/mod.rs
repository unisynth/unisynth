//! Shared test songs (song/1 JSON → blob) and a reference render through the song player.
#![allow(dead_code)]

use synth_core::mix::{Env, Player, Song, DEFAULT_SUB};
use synth_core::pool::Pool;
use unisynth::rng::Xorshift;
use unisynth::UDevice;

/// 3 tracks + master at 120 BPM: `lead` (notes, filter cutoff automation, Echo), `pad` (held chord), `pump`
/// (Compressor keyed by `lead` on the sidechain, fed by `pad`). 4 bars.
pub fn song_json(lead_note: u8) -> serde_json::Value {
    serde_json::json!({
        "format": "song/1", "rate": 44100, "tempo": 120.0, "signature": [4, 4], "ticks_per_beat": 960, "length": 15360,
        "clips": [
            { "id": "a", "length": 3840, "notes": [[0, 480, 57, 100], [960, 480, 60, 90], [1920, 960, lead_note, 110]] },
            { "id": "c", "length": 15360, "notes": [[0, 7000, 48, 70], [0, 7000, 55, 70], [7680, 7000, 51, 70]] }
        ],
        "tracks": [
            { "id": "lead", "name": "Lead", "volume": 0.8, "mute": false, "receives": [],
              "devices": [ { "kind": 64, "values": { "osc1.type": 0.5, "filt1.type": 0.125, "filt1.resonance": 0.3 } },
                           { "kind": 68, "values": { "mode": 0.0, "feedback": 0.3, "dry_wet": 0.3 } } ],
              "placements": [{ "clip": "a", "start": 0, "length": 15360, "loop": true }],
              "lanes": [{ "device": 0, "param": "filt1.cutoff", "points": [[0, 0.2], [7680, 0.8], [15360, 0.3]] }] },
            { "id": "pad", "name": "Pad", "volume": 0.6, "mute": false, "receives": [],
              "devices": [ { "kind": 64, "values": { "osc1.type": 0.25, "env1.attack": 0.3, "env1.release": 0.4 } } ],
              "placements": [{ "clip": "c", "start": 0, "length": 15360, "loop": false }], "lanes": [] },
            { "id": "pump", "name": "Pump", "volume": 1.0, "mute": false,
              "receives": [ { "from": "pad", "channel": "main", "volume": 1.0 }, { "from": "lead", "channel": "side", "volume": 1.0 } ],
              "devices": [ { "kind": 72, "values": { "mode": 0.0, "sidechain": 1.0, "threshold": 0.5, "ratio": 0.6 } } ],
              "placements": [], "lanes": [] }
        ],
        "master": { "id": "master", "name": "Master", "volume": 1.0, "mute": false,
            "receives": [ { "from": "lead", "channel": "main", "volume": 1.0 }, { "from": "pump", "channel": "main", "volume": 1.0 } ],
            "devices": [ { "kind": 71, "values": { "mode": 0.0, "dry_wet": 0.2 } } ], "placements": [], "lanes": [] }
    })
}

pub fn blob(v: &serde_json::Value) -> Vec<u8> {
    song::export(&song::parse(&v.to_string()).unwrap()).unwrap()
}

/// `frames` frames of the player's render (one call), interleaved.
pub fn player(blob: &[u8], frames: usize) -> Vec<f32> {
    let s = Song::parse(blob).unwrap();
    let env = Env::for_song(&s, None);
    let mut mem = vec![0u8; Player::<UDevice, Xorshift>::required_pool(&s, &env, DEFAULT_SUB).unwrap()];
    let mut pool = Pool::new(&mut mem);
    let mut p = Player::<UDevice, Xorshift>::new(s, &env, &mut pool, Xorshift::new(song::render::SEED), DEFAULT_SUB).unwrap();
    let mut out = vec![0.0f32; frames * 2];
    p.render_f32(&mut out);
    out
}

/// Renders `frames` frames from `h` in calls of `call` frames.
pub fn live(h: &mut synth_wasm::Host, frames: usize, call: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(frames * 2);
    let mut left = frames;
    while left > 0 {
        let n = call.min(left);
        out.extend_from_slice(h.render(n));
        left -= n;
    }
    out
}
