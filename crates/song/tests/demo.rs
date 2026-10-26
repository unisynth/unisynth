//! The committed example song and presets (project/music) stay valid and the song renders audibly through the
//! player (task 6.3).

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../project/music")
}

#[test]
fn demo_song_validates_exports_and_renders() {
    let text = std::fs::read_to_string(root().join("demo.song.json")).unwrap();
    let s = song::load(&text).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(song::to_json(&song::parse(&song::to_json(&s)).unwrap()), song::to_json(&s), "stable save");
    let b = song::export(&s).unwrap();
    let pcm = song::render::render(&b, None).unwrap();
    assert!(pcm.iter().all(|x| x.is_finite()));
    let peak = pcm.iter().fold(0f32, |m, x| m.max(x.abs()));
    assert!(peak > 0.05 && peak < 1.0, "peak {peak}");
    // every track is heard: rendering with one track muted changes the output
    for i in 0..s.tracks.len() {
        let mut m = s.clone();
        m.tracks[i].mute = true;
        let other = song::render::render(&song::export(&m).unwrap(), None).unwrap();
        assert_ne!(other, pcm, "muting {} changes nothing", s.tracks[i].id);
    }
}

#[test]
fn presets_validate_and_match_the_demo_devices() {
    for f in ["acid-bass", "glass-pad"] {
        let text = std::fs::read_to_string(root().join(format!("patches/{f}.patch.json"))).unwrap();
        let p = song::preset::parse(&text).unwrap_or_else(|e| panic!("{f}: {e:?}"));
        assert_eq!(p.kind, 64);
    }
}

/// The reference songs built by `tools/demo-songs` (converted from WaveSabre and Tunefish originals).
const CONVERTED: [&str; 6] = ["global-groove", "elysian", "engage", "eladamri", "no-remorse", "tf4testsong"];

#[test]
fn converted_demo_songs_validate_export_and_credit_the_original() {
    for name in CONVERTED {
        let path = root().join(format!("{name}.song.json"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        let s = song::load(&text).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(song::to_json(&s), text, "{name}: saved as the tool writes it");
        let m = s.meta.as_ref().unwrap_or_else(|| panic!("{name}: no meta"));
        for (field, v) in [("author", &m.author), ("description", &m.description), ("copyright", &m.copyright), ("url", &m.url)] {
            assert!(!v.trim().is_empty(), "{name}: {field}");
        }
        assert!(m.url.starts_with("https://github.com/"), "{name}: {}", m.url);
        assert!(m.license.as_deref().is_some_and(|l| l == "MIT" || l == "GPL-3.0-or-later"), "{name}: {:?}", m.license);
        let from = m.adapted_from.as_deref().unwrap_or_else(|| panic!("{name}: not marked as adapted"));
        assert!(from.contains(&m.url), "{name}: adapted_from names the original");
        assert!(m.description.to_lowercase().contains("adapted"), "{name}: the description says it is adapted");
        assert!(m.description.contains("not the original"), "{name}");
        // the license follows the code the original came from
        let engine_license = if name == "no-remorse" || name == "tf4testsong" { "GPL-3.0-or-later" } else { "MIT" };
        assert_eq!(m.license.as_deref(), Some(engine_license), "{name}");
        // exports, and the blob imports back with the same shape
        let blob = song::export(&s).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let back = song::import(&blob).unwrap();
        assert_eq!(back.tracks.len(), s.tracks.len(), "{name}");
        assert!(back.meta.is_none(), "{name}: the blob carries no attribution; the JSON is the source of record");
    }
}

#[test]
fn the_smallest_converted_demo_renders_audibly() {
    let s = song::load(&std::fs::read_to_string(root().join("global-groove.song.json")).unwrap()).unwrap();
    let pcm = song::render::render(&song::export(&s).unwrap(), None).unwrap();
    assert!(pcm.iter().all(|x| x.is_finite()));
    assert!(pcm.iter().fold(0f32, |m, x| m.max(x.abs())) > 0.05);
}
