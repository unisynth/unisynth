//! The Elevated import from synthetic `music.asm` text (no original song is in the repository).

use super::*;
use crate::model::SongM;

/// A `music.asm` with these sections (each value on its own `DB` line, `DD` for the tempo) and its `music.h`.
fn asm(step: u32, rows: usize, patterns: &[u8], sequence: &[u8], tree: &[u8]) -> (String, String) {
    let db = |name: &str, b: &[u8]| {
        let mut s = format!("section x data\nglobal {name}\n{name}:\n");
        for c in b.chunks(16) {
            s += &format!("                DB      {}\n", c.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(","));
        }
        s
    };
    let text = format!(
        "; generated\nsection tempo data\nglobal _tempo_data\n_tempo_data:\ndd \t {step}\ndb\t{rows}\ndd \t -1\n\n{}{}{}",
        db("_pattern_data", patterns),
        db("_sequence_data", sequence),
        db("_machine_tree", tree)
    );
    let header = format!("static const int NUM_ROWS equ {rows};\nstatic const int MAX_NOTE_SAMPLES equ {step};\n");
    (text, header)
}

/// A synth record: segments, noise, pitch factor, base (cycles/sample), volume, stereo, pairs `(wave, op, shift, detune)`.
#[allow(clippy::too_many_arguments)]
fn synth(seg: [i32; 4], noise: f32, exp: f32, base: f32, vol: f32, st: f32, pairs: [(u8, u8, f32, f32); 3]) -> Vec<u8> {
    let mut b = vec![];
    seg.iter().for_each(|x| b.extend(x.to_le_bytes()));
    [noise, exp, base, vol, st].iter().for_each(|x| b.extend(x.to_le_bytes()));
    for (w, op, s, d) in pairs {
        b.extend([w, op, 0, 0]);
        b.extend(s.to_le_bytes());
        b.extend(d.to_le_bytes());
    }
    assert_eq!(b.len(), SYNTH_LEN);
    b
}

const SAW: [(u8, u8, f32, f32); 3] = [(3, 2, 0.0, 1.0), (3, 1, 0.0, 0.0), (3, 1, 0.0, 0.0)];

fn plain() -> Vec<u8> {
    synth([16, 1024, 2048, 2048], 0.0, 1.0, 0.0, 0.5, 1.0, SAW)
}

fn f4(x: f32) -> [u8; 4] {
    x.to_le_bytes()
}

fn i4(x: i32) -> [u8; 4] {
    x.to_le_bytes()
}

fn err<T>(r: Result<T, ConvError>) -> String {
    match r {
        Err(e) => e.to_string(),
        Ok(_) => panic!("expected an error"),
    }
}

// ---- 4.1 parser and errors ----

#[test]
fn the_parser_reads_bytes_words_hex_negatives_floats_and_skips_comments() {
    let text = "section a data\n_tempo_data:\n dd 5410 ; step\n db 112\n dd -1\n_pattern_data: ; patterns\n DB 0x10,0,255 , 7\n\
                _sequence_data:\n db 1\n_machine_tree:\n dd 0.5, -2\n dw 0x1234\n";
    let m = parse(text, "static const int NUM_ROWS equ 3;").unwrap();
    assert_eq!((m.step, m.rows), (5410, 3));
    assert_eq!(m.patterns, [16, 0, 255, 7]);
    assert_eq!(m.sequence, [1]);
    let mut tree = 0.5f32.to_le_bytes().to_vec();
    tree.extend((-2i32).to_le_bytes());
    tree.extend([0x34, 0x12]);
    assert_eq!(m.tree, tree);
}

#[test]
fn a_missing_section_or_header_value_is_named() {
    let (t, h) = asm(100, 1, &[0; 16], &[0], &[]);
    let no_tree = t.replace("_machine_tree:", "_other:");
    assert!(err(parse(&no_tree, &h)).contains("_machine_tree"));
    assert!(err(parse(&t, "")).contains("NUM_ROWS"));
    assert!(err(parse("_tempo_data:\n db ,zz\n", &h)).contains("'zz'"));
}

#[test]
fn every_tree_fault_is_named_and_nothing_is_written() {
    let p = plain();
    let case = |tree: Vec<u8>| {
        let (t, h) = asm(100, 1, &[0; 16], &[0, 0], &tree);
        err(convert(&t, &h, Options::default()))
    };
    // a truncated synth
    assert!(case(p[..50].to_vec()).contains("truncated"), "{}", case(p[..50].to_vec()));
    // opcode 9 after the first synth (byte 72)
    let mut x = p.clone();
    x.extend([9, 255]);
    assert!(case(x).contains("opcode 9 at byte 72"));
    // a mixer with one track
    let mut x = p.clone();
    x.push(4);
    x.extend(f4(1.0));
    x.extend(f4(1.0));
    x.push(255);
    assert!(case(x).contains("fewer than two"));
    // two synths and no mixer
    let mut x = p.clone();
    x.push(0);
    x.extend(&p);
    x.push(255);
    assert!(case(x).contains("2 tracks"));
    // a zero-length segment
    let mut x = synth([16, 0, 2048, 2048], 0.0, 1.0, 0.0, 0.5, 1.0, SAW);
    x.push(255);
    assert!(case(x).contains("decay segment is 0"));
}

#[test]
fn a_pattern_index_outside_the_patterns_is_named() {
    let mut tree = plain();
    tree.push(255);
    let (t, h) = asm(100, 2, &[0; 16], &[0, 5], &tree);
    assert!(err(convert(&t, &h, Options::default())).contains("pattern 5"));
}

#[test]
fn the_machine_table_is_the_one_the_tree_walks_under() {
    // allpass is 7 with atan in the table (Atrium), 6 without it (Elevated)
    let line = |op: u8| {
        let mut x = plain();
        x.push(op);
        x.extend(i4(0));
        x.extend(i4(52));
        x.extend(f4(0.5));
        x.push(255);
        x
    };
    assert_eq!(machines(&line(7), None).unwrap().0, 8);
    assert_eq!(machines(&line(6), None).unwrap().0, 7);
    assert!(matches!(machines(&line(6), None).unwrap().1[1].1, Machine::Allpass { len: 52, .. }));
    assert!(machines(&line(6), Some(8)).is_err());
    assert!(err(machines(&line(7), Some(5))).contains("--table 5"));
    // a lone synth reads the same under both
    let mut lone = plain();
    lone.push(255);
    assert_eq!(machines(&lone, None).unwrap().1.len(), 1);
    assert_eq!(machines(&lone, Some(7)).unwrap().0, 7);
    // read as an atan (8 bytes), an allpass record leaves 4 bytes that start a truncated machine: only table 7 walks
    let mut ap = plain();
    ap.push(6);
    ap.extend(i4(0));
    ap.extend(i4(0));
    ap.extend(f4(0.5));
    ap.push(255);
    assert!(machines(&ap, Some(7)).is_ok() && machines(&ap, Some(8)).is_err());
}

// ---- 4.2 notes ----

#[test]
fn notes_follow_rows_and_steps_with_cuts_and_holds() {
    let mut pat = vec![0u8; 32];
    pat[0] = 61; // pattern 0: a note, a cut two steps later
    pat[2] = 0x7f;
    pat[16 + 15] = 70; // pattern 1: a note on the last step
    let m = Music { step: 100, rows: 3, patterns: pat, sequence: vec![0, 1, 1, 9, 9, 9], tree: vec![] };
    assert_eq!(notes(&m, 0).unwrap(), [(0, 61, Some(100)), (200, 61, None), (3100, 70, Some(100)), (4700, 70, Some(100))]);
    // synth 1 reads the second block of rows: pattern 9 is outside the patterns
    assert!(err(notes(&m, 1)).contains("pattern 9"));
    // a cut with nothing sounding sends nothing
    let m = Music { step: 100, rows: 1, patterns: [vec![0x7f], vec![0; 15]].concat(), sequence: vec![0], tree: vec![] };
    assert_eq!(notes(&m, 0).unwrap(), []);
}

/// Renders a converted blob for `secs` seconds (the unified player).
fn render(blob: &[u8], secs: f64) -> Vec<f32> {
    use synth_core::mix::{Env, Player, Song};
    let s = Song::parse(blob).expect("a valid song");
    let env = Env::for_song(&s, None);
    let need = Player::<unisynth::device::UDevice, unisynth::rng::Xorshift>::required_pool(&s, &env, 100).unwrap();
    let mut mem = vec![0u8; need];
    let mut pool = synth_core::pool::Pool::new(&mut mem);
    let mut p: Player<unisynth::device::UDevice, unisynth::rng::Xorshift> = Player::new(s, &env, &mut pool, unisynth::rng::Xorshift::new(1), 100).unwrap();
    let mut out = vec![0.0f32; 2 * (secs * 44100.0) as usize];
    p.render_f32(&mut out);
    out.chunks(2).map(|f| f[0]).collect()
}

#[test]
fn a_long_note_plays_its_whole_envelope_over_empty_steps() {
    // step 1000 frames; the envelope lasts 100 + 100 + 2500 + 100 = 2800 frames, the next two steps are empty
    let mut tree = synth([100, 100, 2500, 100], 0.0, 1.0, 0.0, 0.5, 1.0, SAW);
    tree.push(255);
    let mut pat = vec![0u8; 16];
    pat[0] = 69;
    let (t, h) = asm(1000, 1, &pat, &[0], &tree);
    let l = render(&convert(&t, &h, Options::default()).unwrap().blob, 0.1);
    let level = |a: usize, b: usize| l[a..b].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(level(2400, 2600) > 0.1, "still holding in the third step: {}", level(2400, 2600));
    assert!(level(2900, 4000) == 0.0, "ended by itself after 2800 frames");
}

#[test]
fn a_cut_silences_the_note_at_its_step() {
    let mut tree = synth([16, 100, 20000, 100], 0.0, 1.0, 0.0, 0.5, 1.0, SAW);
    tree.push(255);
    let mut pat = vec![0u8; 16];
    pat[0] = 69;
    pat[2] = 0x7f;
    let (t, h) = asm(1000, 1, &pat, &[0], &tree);
    let l = render(&convert(&t, &h, Options::default()).unwrap().blob, 0.1);
    assert!(l[1500..1990].iter().any(|v| v.abs() > 0.1), "sounding before the cut");
    assert!(l[2000 + 32..].iter().all(|v| *v == 0.0), "silent from the cut's step");
}

// ---- 4.3 routing and mapping ----

fn read(t: &str, h: &str, opt: Options) -> SongM {
    SongM::read(&convert(t, h, opt).unwrap().blob).unwrap()
}

#[test]
fn a_mixer_becomes_a_track_receiving_both_at_their_gains() {
    // synth A, delay, synth B, mixer (0.5, 1): A (instrument, delay), B (instrument), master ← B·0.5 + A·1
    let mut tree = plain();
    tree.push(1);
    tree.extend(i4(0));
    tree.extend(i4(1082));
    tree.extend(f4(0.5));
    tree.push(0);
    tree.extend(plain());
    tree.push(4);
    tree.extend(f4(0.5));
    tree.extend(f4(1.0));
    tree.push(255);
    let (t, h) = asm(100, 1, &[0; 16], &[0, 0], &tree);
    let s = read(&t, &h, Options::default());
    assert_eq!(s.tracks.len(), 3);
    let kinds = |tr: usize| s.tracks[tr].devices.iter().map(|&d| s.devices[d].kind).collect::<Vec<_>>();
    assert_eq!(kinds(0), [unisynth::kind::INSTRUMENT, unisynth::kind::DELAY]);
    assert_eq!(kinds(1), [unisynth::kind::INSTRUMENT]);
    assert_eq!(s.tracks[2].receives, [(1, 0, 0.5), (0, 0, 1.0)]);
    assert_eq!(s.tempo, (44100.0 * 60.0 / 400.0f64).round() as i32);
}

#[test]
fn pairs_map_to_pair_slots_with_polarity_and_combine() {
    let pairs = [(1, 2, 0.25, 1.0), (2, 3, 0.1, 1.01), (3, 4, 0.0, 1.5)];
    let mut tree = synth([3, 512, 4992, 6912], 1.0, 0.9993, 50.0 / 44100.0, 1.0, 4.0, pairs);
    tree.push(255);
    let (t, h) = asm(100, 1, &[0; 16], &[0], &tree);
    let c = convert(&t, &h, Options { table: Some(8), ..Options::default() }).unwrap();
    let s = SongM::read(&c.blob).unwrap();
    let v = unisynth::chunk::Chunk::parse(&s.devices[0].chunk).unwrap().values();
    let at = |j: usize, k: usize| v[p::osc(j, o::SHAPE + k)];
    let choice = |x: f32, n: usize| unisynth::law::choice(x, n);
    for j in 0..3 {
        assert_eq!(p::osc_type::decode(v[p::osc(j, o::TYPE)]), p::osc_type::PAIR);
    }
    assert_eq!([0, 1, 2].map(|j| choice(at(j, sh::PAIR_WAVE), 3)), [0, 1, 2], "sine, square, saw");
    assert_eq!([0, 1, 2].map(|j| choice(at(j, sh::PAIR_POLARITY), 2)), [0, 1, 0], "op 3 negates");
    assert!((at(1, sh::PAIR_DETUNE) * 2.0 - 1.01).abs() < 1e-6 && (at(0, sh::PAIR_SHIFT) - 0.25).abs() < 1e-6);
    assert_eq!([0, 1, 2].map(|j| choice(v[p::osc(j, o::COMBINE)], p::combine::COUNT)), [0, 0, p::combine::MUL]);
    // the voice: one voice, linear envelope, ½ sustain, drop to 50 Hz at k = 0.9993, noise on both channels alike
    assert_eq!(unisynth::law::count(v[v::CAPACITY], 1, 64), 1);
    assert_eq!(v[p::el_env(0, el::LINEAR)], 1.0);
    assert!((unisynth::law::lin_ms(v[p::el_env(0, el::HOLD)]) - 4992.0 / 44.1).abs() < 0.01);
    assert!((unisynth::law::drop_hz(v[el::DROP_BASE]) - 50.0).abs() < 0.01);
    assert!(((unisynth::law::drop_log2k(v[el::DROP_RATE]) as f64).exp2() - 0.9993).abs() < 1e-6);
    assert_eq!(v[p::NOISE + p::n::WIDTH], 0.0);
    // stereo 4: right = 4 × left
    let (pl, pr) = unisynth::law::pan(v[v::PAN]);
    assert!((pr / pl - 4.0).abs() < 1e-4 && (unisynth::law::gain(v[v::MASTER]) * pl - 1.0).abs() < 1e-4);
    assert!(c.warnings.is_empty(), "{:?}", c.warnings);
}

#[test]
fn effects_keep_their_order_on_the_track() {
    // synth, filter, atan (table 8: 6), delay
    let mut tree = plain();
    tree.push(2);
    for x in [8192.0f32, 0.3125, 7e-6, 7168.0, 0.0, 0.0, 0.0] {
        tree.extend(f4(x));
    }
    tree.extend(i4(0));
    tree.push(6);
    tree.extend(f4(2.0));
    tree.extend(f4(0.5));
    tree.push(1);
    tree.extend(i4(0));
    tree.extend(i4(15630));
    tree.extend(f4(0.40625));
    tree.push(255);
    let (t, h) = asm(100, 1, &[0; 16], &[0], &tree);
    let s = read(&t, &h, Options::default());
    let mode = |d: usize| if s.devices[d].kind == unisynth::kind::INSTRUMENT { 0 } else { unisynth::fx::mode_of(s.devices[d].kind, &s.devices[d].chunk) };
    let chain: Vec<(u8, usize)> = s.tracks[0].devices.iter().map(|&d| (s.devices[d].kind, mode(d))).collect();
    use unisynth::kind::*;
    assert_eq!(chain, [(INSTRUMENT, 0), (EQ, 3), (DISTORTION, 4), (DELAY, 3)]);
}

#[test]
fn a_negative_stereo_factor_warns_and_solo_silences_the_others() {
    let mut tree = synth([16, 100, 100, 100], 0.0, 1.0, 0.0, 0.5, -2.0, SAW);
    tree.push(0);
    tree.extend(plain());
    tree.push(4);
    tree.extend(f4(1.0));
    tree.extend(f4(1.0));
    tree.push(255);
    let (t, h) = asm(100, 1, &[0; 16], &[0, 0], &tree);
    let c = convert(&t, &h, Options::default()).unwrap();
    assert!(c.warnings.iter().any(|w| w.contains("synth 0: stereo factor -2")), "{:?}", c.warnings);
    let s = SongM::read(&convert(&t, &h, Options { solo: Some(1), ..Options::default() }).unwrap().blob).unwrap();
    let master = |d: usize| unisynth::law::gain(unisynth::chunk::Chunk::parse(&s.devices[d].chunk).unwrap().values()[v::MASTER]);
    assert_eq!(master(s.tracks[0].devices[0]), 0.0);
    assert!(master(s.tracks[1].devices[0]) > 0.0);
    assert!(err(convert(&t, &h, Options { solo: Some(2), ..Options::default() })).contains("--solo 2"));
}
