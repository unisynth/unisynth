//! GSM sample chunks for the DAW (import a WAV as GSM, audition a stored sample).

#[test]
fn an_encoded_chunk_decodes_back_and_loads_in_an_instrument() {
    let n = 44100 / 2 + 77; // not a whole number of blocks
    let pcm: Vec<f32> = (0..n).map(|i| 0.5 * (i as f32 * 440.0 * std::f32::consts::TAU / 44100.0).sin()).collect();
    let c = synth_wasm::gsm_chunk_encode(&pcm);
    assert_eq!(c.len(), gsm::CHUNK_HEADER + gsm::blocks_for(n) * gsm::BLOCK_BYTES);
    let back = synth_wasm::gsm_chunk_decode(&c).unwrap();
    assert_eq!(back.len(), n, "exact length, not padded to a block");
    let err: f32 = pcm.iter().zip(&back).map(|(a, b)| (a - b) * (a - b)).sum::<f32>() / n as f32;
    let sig: f32 = pcm.iter().map(|a| a * a).sum::<f32>() / n as f32;
    assert!(10.0 * (sig / err).log10() > 5.0, "lossy but close");
    // a sampler instrument with this chunk as GSM sample 1 is a valid song device, and it sounds
    let song = serde_json::json!({
        "format": "song/1", "rate": 44100, "tempo": 120.0, "signature": [4, 4], "ticks_per_beat": 960, "length": 3840,
        "clips": [{ "id": "c", "length": 3840, "notes": [[0, 1920, 60, 100]] }],
        "tracks": [{ "id": "s", "name": "Sampler", "volume": 1.0, "mute": false, "receives": [],
            "devices": [{ "kind": 64, "values": { "osc1.type": 1.0, "osc1.s7": 0.0, "osc1.sample": 1.0, "env1.sustain": 1.0 },
                "samples": [song::b64::encode(&c)] }],
            "placements": [{ "clip": "c", "start": 0, "length": null, "loop": false }], "lanes": [] }],
        "master": { "id": "master", "name": "Master", "volume": 1.0, "mute": false, "receives": [{ "from": "s", "channel": "main", "volume": 1.0 }],
            "devices": [], "placements": [], "lanes": [] }
    });
    let s = song::load(&song.to_string()).unwrap_or_else(|e| panic!("{e:?}"));
    let blob = song::export(&s).unwrap_or_else(|e| panic!("{e:?}"));
    let mut h = synth_wasm::Host::new();
    h.load(&blob).unwrap();
    h.play(0);
    let peak = (0..100).flat_map(|_| h.render(128).to_vec()).fold(0f32, |m, x| m.max(x.abs()));
    assert!(peak > 0.05, "the GSM sampler sounds (peak {peak})");
}

#[test]
fn garbage_is_an_error_not_a_panic() {
    assert!(synth_wasm::gsm_chunk_decode(&[1, 2, 3]).is_err());
    assert!(synth_wasm::gsm_chunk_decode(&[]).is_err());
}
