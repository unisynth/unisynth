//! `song` CLI (spec "Song command-line tool"):
//!
//! ```text
//! song export <song.json> -o <blob>           validate + export the synth-mix blob
//! song import <blob> -o <song.json> [--meta <meta.json>] [--one-clip]   blob → song/2 (lanes cut into patterns, or
//!                                             one clip per lane), with authorship, track names and descriptions
//! song render <song.json|blob> -o <wav> [--f32] [--bank gm.dls|plus] [--threads <n>]   through the song player (16-bit
//!                                             default); n ≥ 2 renders independent tracks on n threads, same output
//! song info <song.json|blob>                  tracks, devices, length, required cargo features
//! song check <song.json|blob> [--bank gm.dls|plus]  validate, export and load into the song player without rendering
//!                                             (exit 0 = loads); prints the file's format (a song/1 file migrates)
//! song migrate <song.json|patch.json> [-o <out>]   a song/1 or us-patch/1 file saved as song/2 or us-patch/2 (in
//!                                             place without -o); a /2 file is written back unchanged
//! ```
//! `--bank plus` generates the gmplus procedural bank (change add-gmplus-bank) instead of reading a file.
//! Errors print as `<json pointer>: <reason>` on stderr; any failure exits non-zero and writes no output.

use std::process::exit;

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("song: {msg}");
    exit(1)
}

fn read(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| die(format!("{path}: {e}")))
}

fn load_song(path: &str) -> song::Song {
    let text = String::from_utf8(read(path)).unwrap_or_else(|_| die(format!("{path}: not UTF-8")));
    song::load(&text).unwrap_or_else(|issues| {
        for i in &issues {
            eprintln!("{path}: {i}");
        }
        exit(1)
    })
}

fn is_json(path: &str, bytes: &[u8]) -> bool {
    path.ends_with(".json") || bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{')
}

/// The blob of a song file or blob file.
fn blob_of(path: &str) -> Vec<u8> {
    let b = read(path);
    if is_json(path, &b) { song::export(&load_song(path)).unwrap_or_else(|_| exit(1)) } else { b }
}

/// The GM bank: `--bank <path>` or `GM_DLS` (empty = none); `--bank plus` generates the gmplus procedural bank.
fn bank_of(opt: Option<String>) -> Option<Vec<u8>> {
    match opt.or_else(|| std::env::var("GM_DLS").ok()).filter(|p| !p.is_empty()) {
        Some(p) if p == "plus" => Some(gmgen::plus::build(None)),
        Some(p) => Some(read(&p)),
        None => None,
    }
}

fn write(path: &str, bytes: &[u8]) {
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, path)).unwrap_or_else(|e| die(format!("{path}: {e}")));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let cmd = args.first().map(String::as_str).filter(|c| ["export", "import", "render", "info", "check", "migrate"].contains(c));
    if cmd.is_none() {
        eprintln!("usage: song export|import|render|info|check|migrate <file> [-o <out>] [--meta meta.json] [--one-clip] [--f32] [--bank gm.dls] [--threads n]");
        exit(if matches!(args.first().map(String::as_str), Some("help" | "--help" | "-h")) { 0 } else { 2 });
    }
    let input = args.get(1).cloned().unwrap_or_else(|| die("missing input file (see `song help`)"));
    let out = || opt("-o").unwrap_or_else(|| die("missing -o <output>"));
    match cmd {
        Some("export") => {
            let s = load_song(&input);
            let b = song::export(&s).unwrap_or_else(|_| exit(1));
            write(&out(), &b);
        }
        Some("import") => {
            let blob = read(&input);
            let mut s = if flag("--one-clip") { song::import_one_clip(&blob) } else { song::import(&blob) }.unwrap_or_else(|e| die(e));
            if let Some(path) = opt("--meta") {
                let text = String::from_utf8(read(&path)).unwrap_or_else(|_| die(format!("{path}: not UTF-8")));
                song::apply_meta(&mut s, &text).unwrap_or_else(|e| die(format!("{path}: {e}")));
                let issues = song::validate(&s);
                for i in &issues {
                    eprintln!("{path}: {i}");
                }
                if !issues.is_empty() {
                    exit(1)
                }
            }
            write(&out(), song::to_json(&s).as_bytes());
        }
        Some("render") => {
            let threads = match opt("--threads") {
                None if flag("--threads") => die("--threads needs a count"),
                None => 1,
                Some(n) => n.parse::<usize>().ok().filter(|&n| n >= 1).unwrap_or_else(|| die(format!("--threads {n}: expected a whole number ≥ 1"))),
            };
            let b = blob_of(&input);
            let bank = bank_of(opt("--bank"));
            #[cfg(feature = "threads")]
            let pcm = song::render::render_threads(&b, bank.as_deref(), threads).unwrap_or_else(|e| die(e));
            #[cfg(not(feature = "threads"))]
            let pcm = if threads == 1 { song::render::render(&b, bank.as_deref()).unwrap_or_else(|e| die(e)) } else { die("built without the `threads` feature") };
            let rate = synth_core::mix::Song::parse(&b).map(|s| s.rate as u32).unwrap_or(44_100);
            write(&out(), &song::render::wav(&pcm, rate, flag("--f32")));
        }
        Some("info") => {
            let b = blob_of(&input);
            let s = if is_json(&input, &read(&input)) { load_song(&input) } else { song::import(&b).unwrap_or_else(|e| die(e)) };
            let secs = synth_core::mix::Song::parse(&b).map(|x| x.length).unwrap_or(0.0);
            if let Some(m) = &s.meta {
                println!("{} by {} ({}); {}", m.title.as_deref().unwrap_or("untitled"), m.author, m.copyright, m.url);
                if let Some(from) = &m.adapted_from {
                    println!("adapted from {from}");
                }
                println!("{}", m.description);
            }
            if let Some(a) = &s.about {
                println!("{}", [Some(&a.kind), a.style.as_ref(), a.tempo.as_ref()].into_iter().flatten().cloned().collect::<Vec<_>>().join("; "));
            }
            println!("{} Hz, {} BPM, {:.2} s, {} tracks + master, {} clips", s.rate, s.tempo, secs, s.tracks.len(), s.clips.len());
            for t in s.tracks.iter().chain([&s.master]) {
                let devs: Vec<String> = t.devices.iter().map(|d| song::tables::of_device(d).map_or(format!("kind {}", d.kind), |t| t.label.clone())).collect();
                let role = t.about.as_ref().map_or(String::new(), |a| format!(" [{}]", a.kind));
                println!("  {:<10} {}{role}", t.name, if devs.is_empty() { "(no devices)".into() } else { devs.join(" → ") });
            }
            println!("features: {}", song::features(&s).join(" "));
        }
        Some("check") => {
            let raw = read(&input);
            let format = if is_json(&input, &raw) {
                let v: serde_json::Value = serde_json::from_slice(&raw).unwrap_or_else(|e| die(format!("{input}: {e}")));
                v["format"].as_str().unwrap_or("?").to_string()
            } else {
                "blob".into()
            };
            let b = blob_of(&input);
            let bank = bank_of(opt("--bank"));
            let frames = song::render::load(&b, bank.as_deref()).unwrap_or_else(|e| die(format!("{input}: {e}")));
            println!("{input}: loads ({format}, {frames} frames)");
        }
        Some("migrate") => {
            let text = if input.ends_with(".patch.json") {
                let t = String::from_utf8(read(&input)).unwrap_or_else(|_| die(format!("{input}: not UTF-8")));
                let p = song::preset::parse(&t).unwrap_or_else(|issues| {
                    issues.iter().for_each(|i| eprintln!("{input}: {i}"));
                    exit(1)
                });
                song::preset::to_json(&p)
            } else {
                song::to_json(&load_song(&input))
            };
            write(&opt("-o").unwrap_or_else(|| input.clone()), text.as_bytes());
        }
        _ => {
            eprintln!("usage: song export|import|render|info|check|migrate <file> [-o <out>] [--f32] [--bank gm.dls] [--threads n]");
            exit(2)
        }
    }
}
