//! `synth-convert`: legacy songs and Tunefish patches to unified songs; warnings go to stderr.
//!
//! ```text
//! synth-convert ws    <in.song> <out.song>             synth-mix song, legacy device kinds → unified kinds
//! synth-convert tfm   <in.tfm> <out.song> [tail-s]     .tfm song → unified song (tail: seconds after the last event, default 4)
//! synth-convert v2m   <in.v2m> <out.song> [tail-s]     Farbrausch V2 song (format versions 0–6) → unified song
//! synth-convert patch <patch.txt> <phrase.txt> <out.song>
//!                                                       one Tunefish patch (112 numbers) played by a phrase (`block note velocity` lines)
//! synth-convert xm-phrase <patches.json> <call> <record> <variant> <out.song>
//! synth-convert xm <in.xm> <patches.json> <out.song> [--names <tracks.json>] [--controls tick|row|note] [--thin <bytes>]
//! synth-convert xm-triggers <in.xm>
//!                                                       one mini-cauldron phrase trigger (call jizz | bluish | song0; parity entry)
//! synth-convert features <song>                          the unisynth cargo features the unified song needs (comma separated)
//! synth-convert features <song> --explain|--json [--cost marginal.csv]
//!                                                       the same with what calls for each feature, and its marginal size
//! ```

use std::process::ExitCode;
use synth_convert::Converted;

const USAGE: &str = "usage: synth-convert ws <in.song> <out.song>\n       synth-convert tfm <in.tfm> <out.song> [tail-seconds]\n       synth-convert v2m <in.v2m> <out.song> [tail-seconds] [--channels hex] [--era auto|kkrieger2004]   (Farbrausch V2, format versions 0-6)\n       synth-convert tf4m <in.tf4m> <out.song> [tail-seconds]   (2013 Tunefish 4 of Enigma Studio 4)\n       synth-convert tf4m-tfm <in.tf4m> <out.tfm>   (the same song in the final .tfm layout)\n       synth-convert patch <patch.txt> <phrase.txt> <out.song>\n       synth-convert xm <in.xm> <patches.json> <out.song> [--names <tracks.json>] [--controls tick|row|note] [--thin <bytes>]   (a mini-cauldron XM: TBL Jizz, Stash; XM volume/pan/pitch per tick (default), row or note; --thin: lanes within that many bytes of 255, slides as ramps)\n       synth-convert xm-phrase <patches.json> <jizz|bluish|song0> <record> <variant> <out.song>   (one mini-cauldron phrase)\n       synth-convert xm-triggers <in.xm>   (the XM replay's triggers: start s, volume, offset)\n       synth-convert features <unified.song> [--explain | --json] [--cost <marginal.csv>]\n       synth-convert rust <unified.song> <out.rs> [bank.dls]   (compo build: validated song as Rust + module assert)";

fn read(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{path}: {e}"))
}

fn text(path: &str) -> Result<String, String> {
    String::from_utf8(read(path)?).map_err(|_| format!("{path}: not text"))
}

/// Numbers separated by whitespace or commas; `#` starts a comment.
fn numbers(path: &str) -> Result<Vec<f32>, String> {
    let mut out = vec![];
    for line in text(path)?.lines() {
        for w in line.split('#').next().unwrap_or("").split(|c: char| c.is_whitespace() || c == ',').filter(|w| !w.is_empty()) {
            out.push(w.parse::<f32>().map_err(|_| format!("{path}: `{w}` is not a number"))?);
        }
    }
    Ok(out)
}

fn phrase(path: &str) -> Result<Vec<(u32, u8, u8)>, String> {
    let n = numbers(path)?;
    if n.len() % 3 != 0 {
        return Err(format!("{path}: expected `block note velocity` triples, got {} numbers", n.len()));
    }
    n.chunks(3)
        .map(|t| {
            let ok = t[0] >= 0.0 && t[0].fract() == 0.0 && (0.0..=255.0).contains(&t[1]) && (0.0..=255.0).contains(&t[2]);
            ok.then(|| (t[0] as u32, t[1] as u8, t[2] as u8)).ok_or_else(|| format!("{path}: bad event {t:?}"))
        })
        .collect()
}

fn finish(out: Converted, output: &str) -> Result<(), String> {
    for w in &out.warnings {
        eprintln!("synth-convert: warning: {w}");
    }
    std::fs::write(output, out.blob).map_err(|e| format!("{output}: {e}"))
}

fn run(args: &[String]) -> Result<(), String> {
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["ws", input, output] => finish(synth_convert::convert(&read(input)?).map_err(|e| format!("{input}: {e}"))?, output),
        ["tfm", input, output] | ["tfm", input, output, _] => {
            let tail = match args.get(3) {
                Some(t) => t.parse::<f64>().ok().filter(|t| *t >= 0.0).ok_or_else(|| format!("tail `{t}` is not a number of seconds"))?,
                None => 4.0,
            };
            finish(synth_convert::tfm::convert_tfm(&read(input)?, tail).map_err(|e| format!("{input}: {e}"))?, output)
        }
        ["v2m", input, output, ..] => {
            let mut o = synth_convert::v2conv::Opts::default();
            let mut rest = args[3..].iter();
            while let Some(a) = rest.next() {
                match a.as_str() {
                    // channels to keep, hex mask (bit 0 = MIDI channel 1): a parity aid
                    "--channels" => {
                        let m = rest.next().ok_or("--channels needs a hex mask")?;
                        o.mask = u16::from_str_radix(m.trim_start_matches("0x"), 16).map_err(|_| format!("channel mask `{m}` is not hex"))?;
                    }
                    "--era" => {
                        let e = rest.next().ok_or("--era needs a profile")?;
                        o.profile = synth_convert::v2conv::Profile::parse(e).ok_or_else(|| format!("unknown engine profile `{e}` (auto, kkrieger2004)"))?;
                    }
                    t => o.tail = t.parse::<f64>().ok().filter(|t| *t >= 0.0).ok_or_else(|| format!("tail `{t}` is not a number of seconds"))?,
                }
            }
            finish(synth_convert::v2conv::convert(&read(input)?, o).map_err(|e| format!("{input}: {e}"))?, output)
        }
        ["tf4m-tfm", input, output] => {
            let (tfm, warnings) = synth_convert::tfm::tf4m_to_tfm(&read(input)?).map_err(|e| format!("{input}: {e}"))?;
            warnings.iter().for_each(|w| eprintln!("synth-convert: warning: {w}"));
            std::fs::write(output, tfm).map_err(|e| format!("{output}: {e}"))
        }
        ["tf4m", input, output] | ["tf4m", input, output, _] => {
            let tail = match args.get(3) {
                Some(t) => t.parse::<f64>().ok().filter(|t| *t >= 0.0).ok_or_else(|| format!("tail `{t}` is not a number of seconds"))?,
                None => 4.0,
            };
            finish(synth_convert::tfm::convert_tf4m(&read(input)?, tail).map_err(|e| format!("{input}: {e}"))?, output)
        }
        ["patch", patch, events, output] => {
            let p = numbers(patch)?;
            let ph = phrase(events)?;
            let last = ph.iter().map(|e| e.0).max().unwrap_or(0) as f64;
            let length = (last + 1.0) * 512.0 / 44_100.0 + 2.0;
            finish(synth_convert::tfm::convert_patch(&p, &ph, 120, length).map_err(|e| format!("{patch}: {e}"))?, output)
        }
        ["xm-phrase", patches, call, record, variant, output] => {
            let p = synth_convert::cauldron::load(&text(patches)?).map_err(|e| format!("{patches}: {e}"))?;
            let c = p.call(call).ok_or_else(|| format!("{patches}: no call `{call}` (jizz, bluish, song0)"))?;
            let num = |s: &str, what: &str| s.parse::<usize>().map_err(|_| format!("{what} `{s}` is not a number"));
            let (record, variant) = (num(record, "record")?, num(variant, "variant")?);
            let r = c.records.get(record).ok_or_else(|| format!("{call} has {} records, not {}", c.records.len(), record + 1))?;
            if variant >= r.variants as usize {
                return Err(format!("{call} record {record} has {} variants", r.variants));
            }
            let (song, lead) = synth_convert::cauldron::phrase_song(c, record, variant, 44100);
            eprintln!("lead-frames {lead}");
            std::fs::write(output, song.write()).map_err(|e| format!("{output}: {e}"))
        }
        ["xm", input, patches, output, ref rest @ ..] => {
            // options: --names <tracks.json>, --controls tick|row|note, --thin <bytes>
            use synth_convert::cauldron::import::{ControlRate, Options};
            let (mut names, mut opts) = (None, Options::default());
            let mut it = rest.iter();
            while let Some(&o) = it.next() {
                match (o, it.next()) {
                    ("--names", Some(&f)) => names = Some(f),
                    ("--controls", Some(&c)) => opts.controls = ControlRate::parse(c).ok_or_else(|| format!("--controls {c}: expected tick, row or note"))?,
                    ("--thin", Some(&b)) => opts.thin = b.parse().map_err(|_| format!("--thin {b}: expected bytes 0-255"))?,
                    _ => return Err(USAGE.to_string()),
                }
            }
            // a mini-cauldron XM with its patch data (change add-cauldron-xm-import); an error writes nothing
            let p = synth_convert::cauldron::load(&text(patches)?).map_err(|e| format!("{patches}: {e}"))?;
            let x = synth_convert::xm::parse(&read(input)?).map_err(|e| format!("{input}: {e}"))?;
            let c = synth_convert::cauldron::import::convert_with(&p, &x, 44100, opts).map_err(|e| format!("{input}: {e}"))?;
            for w in &c.warnings {
                eprintln!("warning: {w}");
            }
            eprintln!("tracks {} phrase, {} stored", c.tracks.0, c.tracks.1);
            eprintln!("pingpong-seconds {:.1}", c.pingpong);
            eprintln!("muted-seconds {:.1}", c.muted);
            eprintln!("lead-frames {}", c.lead);
            if let Some(names) = names {
                // the track names as a JSON array (the master excluded), for `song import --meta`'s `tracks`
                let json = format!("[{}]", c.names.iter().map(|n| format!("{n:?}")).collect::<Vec<_>>().join(","));
                std::fs::write(names, json).map_err(|e| format!("{names}: {e}"))?;
            }
            std::fs::write(output, c.song.write()).map_err(|e| format!("{output}: {e}"))
        }
        ["xm-triggers", input] => {
            // the replay's triggers as "<start s> <first-tick volume> <offset> <channel> <instrument> <sample> <note>" lines (parity onset check, task 5.2)
            let x = synth_convert::xm::parse(&read(input)?).map_err(|e| format!("{input}: {e}"))?;
            let r = synth_convert::xm::replay::replay(&x, 44100).map_err(|e| format!("{input}: {e}"))?;
            for t in &r.triggers {
                println!("{:.6} {:.4} {} {} {} {} {}", t.at, t.ticks.first().map_or(0.0, |c| c.vol), t.offset, t.channel, t.instrument + 1, t.sample, t.note);
            }
            eprintln!("length {:.6}", r.length);
            Ok(())
        }
        ["features", input] => {
            println!("{}", synth_convert::features::features(&read(input)?).map_err(|e| format!("{input}: {e}"))?.join(","));
            Ok(())
        }
        ["features", input, ref rest @ ..] => {
            use synth_convert::features as f;
            let (mut json, mut cost) = (false, None);
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match *a {
                    "--explain" => json = false,
                    "--json" => json = true,
                    "--cost" => cost = Some(text(it.next().ok_or("--cost needs a file")?)?),
                    _ => return Err(USAGE.into()),
                }
            }
            let table = cost.as_deref().map(f::read_cost);
            let needs = f::explain(&read(input)?, table.as_ref()).map_err(|e| format!("{input}: {e}"))?;
            print!("{}", if json { f::render_json(&needs) } else { f::render_text(&needs) });
            Ok(())
        }
        ["rust", input, output] | ["rust", input, output, _] => {
            let bank = args.get(3).map(|b| read(b)).transpose()?;
            let name = std::path::Path::new(input).file_name().and_then(|n| n.to_str()).unwrap_or(input);
            let src = synth_convert::rust::generate(&read(input)?, bank.as_deref(), name).map_err(|e| format!("{input}: {e}"))?;
            std::fs::write(output, src).map_err(|e| format!("{output}: {e}"))
        }
        _ => Err(USAGE.into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(m) => {
            eprintln!("synth-convert: {m}");
            ExitCode::FAILURE
        }
    }
}
