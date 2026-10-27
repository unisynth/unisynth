//! Import reads untrusted bytes (security-hardening): every truncation and thousands of byte mutations of a valid
//! blob must give `Ok` or `Err`, never a panic; whatever imports must also validate and export without panicking.

fn demo_blob() -> Vec<u8> {
    let v = serde_json::json!({
        "format": "song/1", "rate": 44100, "tempo": 120.0, "signature": [4, 4], "ticks_per_beat": 960, "length": 7680,
        "clips": [{ "id": "a", "length": 3840, "notes": [[0, 480, 60, 100], [480, 960, 64, 90], [1920, 240, 67, 80]] }],
        "tracks": [
            { "id": "lead", "name": "Lead", "volume": 0.8, "mute": false, "receives": [],
              "devices": [ { "kind": 64, "values": { "osc1.type": 0.25, "filt1.type": 0.125 } },
                           { "kind": 68, "values": { "mode": 0.0, "feedback": 0.4 } } ],
              "placements": [{ "clip": "a", "start": 0, "length": 7680, "loop": true }],
              "lanes": [{ "device": 0, "param": "filt1.cutoff", "points": [[0, 0.2], [3840, 0.9]] }] },
            { "id": "pump", "name": "Pump", "volume": 1.0, "mute": false,
              "receives": [{ "from": "lead", "channel": "side", "volume": 1.0 }],
              "devices": [ { "kind": 72, "values": { "mode": 0.0, "sidechain": 1.0 } } ], "placements": [], "lanes": [] }
        ],
        "master": { "id": "master", "name": "Master", "volume": 1.0, "mute": false,
            "receives": [{ "from": "lead", "channel": "main", "volume": 1.0 }, { "from": "pump", "channel": "main", "volume": 0.5 }],
            "devices": [ { "kind": 71, "values": { "mode": 1.0 } } ], "placements": [], "lanes": [] }
    });
    song::export(&song::parse(&v.to_string()).unwrap()).unwrap()
}

fn survive(b: &[u8]) {
    if let Ok(s) = song::import(b) {
        let _ = song::validate(&s);
        let _ = song::export(&s);
        let _ = song::to_json(&s);
    }
}

#[test]
fn every_truncation_is_handled() {
    let b = demo_blob();
    assert!(song::import(&b).is_ok());
    for n in 0..b.len() {
        survive(&b[..n]);
        assert!(song::import(&b[..n]).is_err(), "truncated to {n} bytes must fail");
    }
}

#[test]
fn byte_mutations_never_panic() {
    let b = demo_blob();
    let mut x = 0x9e37_79b9u32;
    let mut rnd = || {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x
    };
    for _ in 0..20_000 {
        let mut m = b.clone();
        for _ in 0..1 + rnd() % 4 {
            let i = rnd() as usize % m.len();
            m[i] = match rnd() % 4 {
                0 => rnd() as u8,
                1 => 0xff,
                2 => 0x80,
                _ => m[i] ^ (1 << (rnd() % 8)),
            };
        }
        survive(&m);
    }
}

#[test]
fn automation_on_a_missing_device_is_an_error_not_a_panic() {
    let mut m = song::blob::BlobM::read(&demo_blob()).unwrap();
    let t = m.tracks.iter_mut().find(|t| !t.autos.is_empty()).unwrap();
    t.autos[0].device = t.devices.len(); // one past the track's devices
    let b = m.write();
    // Song::parse checks automation devices against the track's list, so every reader refuses the blob
    assert!(synth_core::mix::Song::parse(&b).is_err());
    assert!(song::import(&b).is_err());
    assert!(song::blob::BlobM::read(&b).is_err());
    let mut h = Vec::new();
    h.extend_from_slice(&b);
    assert!(song::import(&h[..h.len() - 1]).is_err());
}

#[test]
fn hostile_json_is_rejected_without_panics() {
    for text in [
        "", "{", "[]", "null", r#"{"format":"song/1"}"#,
        r#"{"format":"song/1","rate":44100,"tempo":1e308,"signature":[4,4],"ticks_per_beat":960,"length":9223372036854775807,"clips":[],"tracks":[],"master":{"id":"m","name":"M","volume":1,"mute":false,"receives":[],"devices":[],"placements":[],"lanes":[]}}"#,
        r#"{"format":"song/1","rate":0,"tempo":-5,"signature":[0,3],"ticks_per_beat":1,"length":-1,"clips":[{"id":"","length":-4,"notes":[[-1,-1,255,0]]}],"tracks":[],"master":{"id":"","name":"","volume":-1,"mute":false,"receives":[{"from":"x","channel":"side","volume":-1}],"devices":[{"kind":255,"values":{}},{"kind":64,"values":{"osc1.s8":1e9},"samples":["!!"]}],"placements":[{"clip":"z","start":-9,"length":0,"loop":true}],"lanes":[{"device":99,"param":"q","points":[[1e300,2]]}]}}"#,
        r#"{"format":"song/1","rate":44100,"tempo":120,"signature":[4,4],"ticks_per_beat":960,"length":3840,"clips":[{"id":"a","length":9223372036854775807,"notes":[[0,9223372036854775807,60,100]]}],"tracks":[{"id":"t","name":"T","volume":1,"mute":false,"receives":[],"devices":[],"placements":[{"clip":"a","start":9223372036854775807,"length":9223372036854775807,"loop":true}],"lanes":[]}],"master":{"id":"m","name":"M","volume":1,"mute":false,"receives":[],"devices":[],"placements":[],"lanes":[]}}"#,
    ] {
        match song::parse(text) {
            Err(e) => assert!(!e.is_empty()),
            Ok(s) => {
                let issues = song::validate(&s);
                assert!(!issues.is_empty(), "{text}");
                assert!(song::export(&s).is_err());
            }
        }
    }
}

/// Parallel render of hostile blobs (change add-synth-threads): whatever loads renders through the threaded entry
/// without panicking and equal to the single-thread render; shared devices fall back to the single-thread path.
#[cfg(feature = "threads")]
mod threads {
    use synth_core::mix::{Env, Player, Song, DEFAULT_SUB};
    use synth_core::pool::Pool;
    use unisynth::rng::Xorshift;
    use unisynth::UDevice;

    /// `(parallel, output)` of 4410 frames, or `None` when the blob does not load (or wants > 64 MB of pool).
    fn render(b: &[u8], threads: usize) -> Option<(bool, Vec<f32>)> {
        let s = Song::parse(b).ok()?;
        let env = Env::for_song(&s, None);
        let need = Player::<UDevice, Xorshift>::required_pool(&s, &env, DEFAULT_SUB).ok().filter(|&n| n < 64 << 20)?;
        let mut mem = vec![0u8; need];
        let mut pool = Pool::new(&mut mem);
        let mut p = Player::<UDevice, Xorshift>::new(s, &env, &mut pool, Xorshift::new(song::render::SEED), DEFAULT_SUB).ok()?;
        let mut out = vec![0.0f32; 2 * 4410];
        if threads == 0 {
            p.render_f32(&mut out);
        } else {
            song::workers::with_workers(threads, |w| p.render_f32_par(&mut out, w));
        }
        Some((p.parallel(), out))
    }

    fn same(a: &[f32], b: &[f32]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
    }

    #[test]
    fn mutated_blobs_render_in_parallel_like_serial() {
        let b = super::demo_blob();
        let (parallel, want) = render(&b, 0).unwrap();
        assert!(parallel && same(&render(&b, 4).unwrap().1, &want));
        let mut x = 0x2545_f491u32;
        let mut rnd = || {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x
        };
        let mut loaded = 0;
        for _ in 0..1500 {
            let mut m = b.clone();
            for _ in 0..1 + rnd() % 3 {
                let i = rnd() as usize % m.len();
                m[i] = match rnd() % 3 {
                    0 => rnd() as u8,
                    1 => m[i].wrapping_add(1),
                    _ => m[i] ^ (1 << (rnd() % 8)),
                };
            }
            if let Some((_, want)) = render(&m, 0) {
                loaded += 1;
                let (_, got) = render(&m, 4).unwrap();
                assert!(same(&got, &want), "mutated blob differs in parallel");
            }
        }
        assert!(loaded > 100, "only {loaded} mutations loaded");
    }

    #[test]
    fn device_on_two_tracks_falls_back() {
        let mut m = song::blob::BlobM::read(&super::demo_blob()).unwrap();
        let d = m.tracks[0].devices[0];
        m.tracks[1].devices.push(d); // the lead instrument also runs on the pump track
        let b = m.write();
        let (parallel, want) = render(&b, 0).unwrap();
        assert!(!parallel);
        assert!(same(&render(&b, 4).unwrap().1, &want));
    }
}
