//! Prints the `wavesabre` cargo features (`ws-falcon,ws-echo,…`) a legacy WaveSabre song needs, for
//! `research/size-compare`: `legacy_features <song.bin>`.
use synth_core::mix::Song;

const NAMES: [&str; 13] = [
    "falcon", "slaughter", "thunder", "scissor", "leveller", "crusher", "echo", "smasher", "chamber", "twister",
    "cathedral", "adultery", "specimen",
];

fn main() {
    let path = std::env::args().nth(1).expect("legacy_features <song.bin>");
    let blob = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let song = Song::parse(&blob).unwrap_or_else(|e| panic!("{path}: {e:?}"));
    let mut used = [false; 13];
    for d in song.devices() {
        if let Some(u) = used.get_mut(d.kind as usize) {
            *u = true;
        }
    }
    let list: Vec<String> = NAMES.iter().zip(used).filter(|(_, u)| *u).map(|(n, _)| format!("ws-{n}")).collect();
    println!("{}", list.join(","));
}
