//! `synth-convert`: legacy songs and Tunefish patches to unified songs; warnings go to stderr.
//!
//! ```text
//! synth-convert ws    <in.song> <out.song>             synth-mix song, legacy device kinds → unified kinds
//! synth-convert tfm   <in.tfm> <out.song> [tail-s]     .tfm song → unified song (tail: seconds after the last event, default 4)
//! synth-convert patch <patch.txt> <phrase.txt> <out.song>
//!                                                       one Tunefish patch (112 numbers) played by a phrase (`block note velocity` lines)
//! synth-convert features <song>                          the unisynth cargo features the unified song needs (comma separated)
//! synth-convert features <song> --explain|--json [--cost marginal.csv]
//!                                                       the same with what calls for each feature, and its marginal size
//! ```

use std::process::ExitCode;
use synth_convert::Converted;

const USAGE: &str = "usage: synth-convert ws <in.song> <out.song>\n       synth-convert tfm <in.tfm> <out.song> [tail-seconds]\n       synth-convert tf4m <in.tf4m> <out.song> [tail-seconds]   (2013 Tunefish 4 of Enigma Studio 4)\n       synth-convert tf4m-tfm <in.tf4m> <out.tfm>   (the same song in the final .tfm layout)\n       synth-convert patch <patch.txt> <phrase.txt> <out.song>\n       synth-convert features <unified.song> [--explain | --json] [--cost <marginal.csv>]\n       synth-convert rust <unified.song> <out.rs> [bank.dls]   (compo build: validated song as Rust + module assert)";

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
