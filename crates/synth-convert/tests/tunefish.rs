//! Tunefish conversion: device-level parity (in process) of every `tfsynth` synthetic patch and of `.tfm` songs,
//! legacy render against the converted song through the unified player (design D11: 1 dB loudness, 3 dB per octave
//! band 63 Hz–16 kHz).

mod common;
#[path = "../../tfsynth/tests/patches.rs"]
#[allow(dead_code)]
mod patches;

use common::metrics::compare;
use common::{dev, loads_and_renders, song, track};
use synth_core::mix::{Env, Player, Song, DEFAULT_SUB};
use synth_core::pool::Pool;
use tfsynth::device::{kind, TfDevice};
use tfsynth::params::COUNT;
use unisynth::device::UDevice;
use unisynth::rng::Xorshift;
use wavesabre::helpers::WsRng;

fn legacy(blob: &[u8]) -> Vec<f32> {
    let s = Song::parse(blob).unwrap();
    let env = Env::for_song(&s, None);
    let need = Player::<TfDevice, WsRng>::required_pool(&s, &env, DEFAULT_SUB).unwrap();
    let mut mem = vec![0u8; need];
    let mut pool = Pool::new(&mut mem);
    let mut p = Player::<TfDevice, WsRng>::new(s, &env, &mut pool, WsRng::new(), DEFAULT_SUB).unwrap();
    let mut out = vec![0.0f32; 2 * p.length_frames() as usize];
    p.render_f32(&mut out);
    out
}

fn unified(blob: &[u8]) -> Vec<f32> {
    let s = Song::parse(blob).unwrap();
    let env = Env::for_song(&s, None);
    let need = Player::<UDevice, Xorshift>::required_pool(&s, &env, DEFAULT_SUB).unwrap();
    let mut mem = vec![0u8; need];
    let mut pool = Pool::new(&mut mem);
    let mut p = Player::<UDevice, Xorshift>::new(s, &env, &mut pool, Xorshift::new(7), DEFAULT_SUB).unwrap();
    let mut out = vec![0.0f32; 2 * p.length_frames() as usize];
    p.render_f32(&mut out);
    out
}

/// `PHRASE` played `REPEATS` times, so the random draws of both engines average out.
const REPEATS: usize = 6;

/// A one-instrument legacy song: the kind-32 device playing the phrase at block starts.
fn legacy_song(p: &[f32; COUNT]) -> Vec<u8> {
    let events: Vec<(usize, u8, u8)> =
        (0..REPEATS).flat_map(|r| patches::PHRASE.iter().map(move |&(b, n, v)| (b + r * patches::PHRASE_BLOCKS, n, v))).collect();
    legacy_song_of(p, &events, REPEATS * patches::PHRASE_BLOCKS)
}

/// The kind-32 device playing `(block, note, velocity)` events at block starts, `blocks` plus a tail long.
fn legacy_song_of(p: &[f32; COUNT], events: &[(usize, u8, u8)], blocks: usize) -> Vec<u8> {
    let mut prev = 0i32;
    let lane = events
        .iter()
        .map(|&(b, n, v)| {
            let f = b as i32 * 512;
            let d = f - prev;
            prev = f;
            (d, n, (v != 0).then_some(v))
        })
        .collect();
    let mut s = song(vec![dev(kind::INSTRUMENT, p)], vec![lane], vec![track(&[0], 0)]);
    s.length = (blocks as f64 + 60.0) * 512.0 / 44100.0;
    s.write()
}

fn parity(name: &str, p: &[f32; COUNT]) -> (f64, f64) {
    let blob = legacy_song(p);
    let want = legacy(&blob);
    let conv = synth_convert::convert(&blob).unwrap_or_else(|e| panic!("{name}: {e}"));
    for w in &conv.warnings {
        eprintln!("{name}: warning: {w}");
    }
    let got = unified(&conv.blob);
    assert_eq!(got.len(), want.len());
    assert!(got.iter().all(|v| v.is_finite()), "{name}");
    compare(name, &want, &got)
}

// ---------------------------------------------------------------------------------------------------------------------
// helpers

use synth_convert::model::{Dev, SongM};
use synth_convert::tfm::{convert_patch, convert_tfm};
use tfsynth::params::*;
use unisynth::law;
use unisynth::modmx::{source, target};
use unisynth::params::{self as pm, e, f, filter_type, l, m, n, o, osc_type, shape, v};

/// Writes a `.tfm`: `(patch, [(row delta, note, velocity)])` per instrument; values on the 1/100 grid.
fn tfm(tempo: u16, ins: &[([f32; COUNT], Vec<(u16, u8, u8)>)]) -> Vec<u8> {
    let mut b = vec![];
    b.extend((ins.len() as u16).to_le_bytes());
    b.extend(tempo.to_le_bytes());
    for (_, ev) in ins {
        b.extend((ev.len() as u16).to_le_bytes());
    }
    b.extend(b"INST");
    for (p, _) in ins {
        b.extend(p.iter().map(|v| (v * 100.0).round() as u8));
    }
    b.extend(b"SONG");
    for (_, ev) in ins {
        ev.iter().for_each(|e| b.extend(e.0.to_le_bytes()));
        ev.iter().for_each(|e| b.push(e.1));
        ev.iter().for_each(|e| b.push(e.2));
    }
    b.extend(b"ENDS");
    b
}

fn quantised(p: [f32; COUNT]) -> [f32; COUNT] {
    p.map(|v| (v * 100.0).trunc() / 100.0)
}

fn lead() -> [f32; COUNT] {
    let mut p = patches::base();
    p[GEN_HARMONICS] = 0.3;
    p[GEN_MOD] = 0.4;
    p[ENV1..ENV1 + 5].copy_from_slice(&[0.1, 0.4, 0.6, 0.4, 0.5]);
    patches::route(&mut p, 0, 3, 0.5, 5);
    p[EFFECT_FIRST] = 0.3; // chorus
    p[CHORUS_RATE] = 0.4;
    p[CHORUS_DEPTH] = 0.6;
    p[CHORUS_GAIN] = 0.8;
    quantised(p)
}

fn bass() -> [f32; COUNT] {
    let mut p = patches::base();
    p[GEN_HARMONICS] = 0.15;
    p[GEN_OCTAVE] = 0.37;
    p[LP_ON] = 1.0;
    p[LP_CUT] = 0.3;
    quantised(p)
}

const LEAD: [(u16, u8, u8); 6] = [(0, 60, 100), (2, 60, 0), (0, 64, 90), (3, 64, 0), (1, 0, 100), (2, 67, 80)];

fn unified_song(blob: &[u8]) -> SongM {
    SongM::read(blob).expect("a valid unified song")
}

/// The unified patch of device `d` of a converted song.
fn patch_of(song: &SongM, d: usize) -> [f32; pm::COUNT] {
    assert_eq!(song.devices[d].kind, unisynth::kind::INSTRUMENT);
    unisynth::chunk::Chunk::parse(&song.devices[d].chunk).expect("a valid chunk").values()
}

/// `(unified kind, mode)` of an effect device.
fn fx_of(d: &Dev) -> (u8, usize) {
    let mode = f32::from_le_bytes(d.chunk[..4].try_into().unwrap());
    (d.kind, law::choice(mode, unisynth::fx::mode_count(d.kind)))
}

fn convert_one(p: &[f32; COUNT]) -> (SongM, Vec<String>) {
    let c = convert_patch(p, &[(0, 60, 100), (8, 60, 0)], 120, 2.0).unwrap();
    (unified_song(&c.blob), c.warnings)
}

fn route(patch: &[f32; pm::COUNT], i: usize) -> (usize, u8, f32, usize) {
    let b = pm::modslot(i, 0);
    (
        law::choice(patch[b + m::SOURCE], source::COUNT),
        law::index(patch[b + m::TARGET]),
        patch[b + m::AMOUNT],
        law::choice(patch[b + m::OPERATION], 2),
    )
}

fn near(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() <= 1e-3 * b.abs().max(1.0), "{what}: {a} vs {b}");
}

// ---------------------------------------------------------------------------------------------------------------------
// the .tfm song

#[test]
fn a_tfm_gives_a_track_per_non_empty_instrument_a_chain_after_it_and_a_master() {
    let mut reverb_delay = patches::base();
    reverb_delay[EFFECT_FIRST] = 0.5; // reverb
    reverb_delay[EFFECT_FIRST + 2] = 0.2; // delay (slot 2 stays none)
    let b = tfm(125, &[(lead(), LEAD.to_vec()), (bass(), vec![]), (quantised(reverb_delay), vec![(0, 40, 100), (4, 40, 0)])]);
    let c = convert_tfm(&b, 4.0).unwrap();
    let s = unified_song(&c.blob);
    assert_eq!((s.tempo, s.rate), (125, 44100));
    assert_eq!(s.tracks.len(), 3, "two non-empty instruments and the master");
    // lead: instrument, chorus; third: instrument, reverb, delay
    let kinds = |t: usize| s.tracks[t].devices.iter().map(|&d| s.devices[d].kind).collect::<Vec<_>>();
    assert_eq!(kinds(0), vec![unisynth::kind::INSTRUMENT, unisynth::kind::CHORUS]);
    assert_eq!(kinds(1), vec![unisynth::kind::INSTRUMENT, unisynth::kind::REVERB, unisynth::kind::DELAY]);
    assert_eq!(fx_of(&s.devices[s.tracks[1].devices[1]]), (unisynth::kind::REVERB, 1), "the Tunefish reverb mode");
    assert_eq!(fx_of(&s.devices[s.tracks[1].devices[2]]), (unisynth::kind::DELAY, 1), "the Tunefish delay mode");
    assert_eq!(fx_of(&s.devices[s.tracks[0].devices[1]]), (unisynth::kind::CHORUS, 0));
    // the effect chunk is the mode followed by the legacy parameters of that effect
    let rev = &s.devices[s.tracks[1].devices[1]].chunk;
    let vals: Vec<f32> = rev.chunks_exact(4).take(5).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
    assert_eq!(&vals[1..5], &[REV_ROOM, REV_DAMP, REV_WET, REV_WIDTH].map(|i| reverb_delay_value(i)));
    // the master: no devices, every instrument ×11, the bus gain of the .tfm player
    let master = s.tracks.last().unwrap();
    assert!(master.devices.is_empty());
    assert_eq!(master.receives, vec![(0, 0, 11.0), (1, 0, 11.0)]);
    assert!((master.volume - 200.0 / 32768.0).abs() < 1e-9);
    assert!(s.tracks[..2].iter().all(|t| t.volume == 1.0 && t.receives.is_empty()));
    assert!(c.warnings.is_empty(), "{:?}", c.warnings);
}

/// The value the test patch holds at `i` after the `.tfm` quantisation.
fn reverb_delay_value(i: usize) -> f32 {
    let mut p = patches::base();
    p[EFFECT_FIRST] = 0.5;
    quantised(p)[i]
}

#[test]
fn tfm_events_land_at_the_start_of_the_block_that_contains_them() {
    // tempo 125: a row lasts 0.12 s = 5292 frames, so row 1 lies in block 10 (frames 5120..5632)
    let b = tfm(125, &[(lead(), vec![(1, 60, 100), (1, 60, 0), (0, 0, 90), (2, 64, 90), (0, 64, 0)])]);
    let s = unified_song(&convert_tfm(&b, 0.0).unwrap().blob);
    let lane = &s.lanes[s.tracks[0].lane];
    let mut at = 0i64;
    let abs: Vec<(i64, u8, Option<u8>)> = lane
        .iter()
        .map(|&(d, n, v)| {
            at += d as i64;
            (at, n, v)
        })
        .collect();
    assert_eq!(abs[0], (10 * 512, 60, Some(100)));
    // row 2 = 10584 frames → block 20; note 0 is ignored; velocity 0 is a note-off; rows 4 = 21168 → block 41
    assert_eq!(abs[1], (20 * 512, 60, None));
    assert_eq!(abs[2], (41 * 512, 64, Some(90)));
    assert_eq!(abs[3], (41 * 512, 64, None), "same row, same block, stored order");
    assert_eq!(abs.len(), 4);
    // the master lane is empty and the song is as long as the last event plus the tail
    assert!(s.lanes[s.tracks.last().unwrap().lane].is_empty());
    let b = tfm(125, &[(lead(), vec![(4, 60, 100)])]);
    let s = unified_song(&convert_tfm(&b, 3.0).unwrap().blob);
    assert!((s.length - (4.0 * 0.12 + 3.0)).abs() < 1e-4, "{}", s.length);
    // the last event of any instrument, not of the last instrument; notes use their low 7 bits like the legacy tables
    let b = tfm(125, &[(lead(), vec![(8, 200, 100)]), (bass(), vec![(2, 60, 100)])]);
    let s = unified_song(&convert_tfm(&b, 1.0).unwrap().blob);
    assert!((s.length - (8.0 * 0.12 + 1.0)).abs() < 1e-4, "{}", s.length);
    assert_eq!(s.lanes[s.tracks[0].lane][0].1, 200 & 0x7f);
}

fn tfm_loudness(name: &str, b: &[u8]) -> f64 {
    use tfsynth::player::TfmPlayer;
    use tfsynth::tfm::Tfm;
    let leaked: &'static [u8] = Box::leak(b.to_vec().into_boxed_slice());
    let t = Tfm::parse(leaked).unwrap();
    let mem: &'static mut [u8] = Box::leak(vec![0u8; TfmPlayer::required_pool(&t)].into_boxed_slice());
    let pool: &'static mut Pool<'static> = Box::leak(Box::new(Pool::new(mem)));
    let mut p = TfmPlayer::new(t, pool, 0.0).unwrap();
    p.start(0.0);
    let blocks = p.blocks() + 100;
    let mut want = vec![];
    let mut out = [0i16; 1024];
    for _ in 0..blocks {
        p.render_i16(&mut out);
        want.extend(out.iter().map(|&v| v as f32 / 32768.0));
    }
    let conv = convert_tfm(b, blocks as f64 * 512.0 / 44100.0 - p.length() as f64).unwrap();
    let s = Song::parse(&conv.blob).unwrap();
    let env = Env::for_song(&s, None);
    let need = Player::<UDevice, Xorshift>::required_pool(&s, &env, DEFAULT_SUB).unwrap();
    let mut mem = vec![0u8; need];
    let mut pool = Pool::new(&mut mem);
    let mut up: Player<UDevice, Xorshift> = Player::new(s, &env, &mut pool, Xorshift::new(7), DEFAULT_SUB).unwrap();
    let mut got16 = vec![0i16; want.len()];
    up.render_i16(&mut got16);
    let got: Vec<f32> = got16.iter().map(|&v| v as f32 / 32768.0).collect();
    assert!(want.iter().any(|v| v.abs() > 0.001), "the legacy render is not silent");
    compare(name, &want, &got).0
}

/// A busy two-instrument song: 24 short notes each, so the random waveform of every note averages out.
fn busy_song() -> Vec<u8> {
    let notes = |base: u8, mul: usize| -> Vec<(u16, u8, u8)> {
        (0..24)
            .flat_map(|k| {
                let n = base + (k * mul % 30) as u8;
                [(if k == 0 { 0 } else { 2 }, n, 100), (3, n, 0)]
            })
            .collect()
    };
    tfm(125, &[(lead(), notes(48, 7)), (bass(), notes(30, 5))])
}

#[test]
fn a_converted_tfm_is_as_loud_as_the_legacy_i16_render() {
    let loud = tfm_loudness("tfm song", &busy_song());
    assert!(loud <= 1.0, "loudness differs by {loud} dB");
}

/// Opt-in: every `.tfm` in `$TF_SONGS` (a file or a directory) must convert and match its legacy render within 1 dB.
#[test]
fn songs_from_the_environment_match_when_present() {
    let Some(path) = std::env::var_os("TF_SONGS") else {
        eprintln!("TF_SONGS is not set: skipped");
        return;
    };
    let path = std::path::PathBuf::from(path);
    let files: Vec<_> = if path.is_dir() {
        std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "tfm")).collect()
    } else {
        vec![path]
    };
    for f in files {
        let loud = tfm_loudness(&f.display().to_string(), &std::fs::read(&f).unwrap());
        assert!(loud <= 1.0, "{}: {loud} dB", f.display());
    }
}

#[test]
fn malformed_tfm_files_are_rejected_with_a_message() {
    let good = tfm(120, &[(lead(), LEAD.to_vec())]);
    for cut in 0..good.len() {
        assert!(convert_tfm(&good[..cut], 1.0).is_err(), "truncated at {cut}");
    }
    let mut bad = good.clone();
    bad[4 + 2 + 1] = b'X'; // INST tag
    assert!(convert_tfm(&bad, 1.0).err().unwrap().to_string().contains(".tfm"));
    let zero = tfm(0, &[(lead(), LEAD.to_vec())]);
    assert!(convert_tfm(&zero, 1.0).err().unwrap().to_string().contains("tempo"));
    // rows far past what a song can hold
    let far = tfm(1, &[(lead(), vec![(65535, 60, 100); 30])]);
    assert!(convert_tfm(&far, 1.0).err().unwrap().to_string().contains("longer"));
    // all instruments empty: just a master
    let empty = tfm(120, &[(lead(), vec![])]);
    assert_eq!(unified_song(&convert_tfm(&empty, 1.0).unwrap().blob).tracks.len(), 1);
}

// ---------------------------------------------------------------------------------------------------------------------
// the patch

#[test]
fn every_synthetic_patch_matches_its_legacy_render() {
    let mut bad = vec![];
    let mut all: Vec<(String, [f32; COUNT])> = patches::instrument_patches().into_iter().map(|(n, p)| (n.to_string(), p)).collect();
    // the insert effects with a single layer: the effect is what is compared, not the random phases of a unison stack
    for (name, _, mut p) in patches::effect_patches() {
        p[GEN_UNISON] = 0.0;
        all.push((name.to_string(), p));
    }
    for (name, p) in &all {
        let (loud, band) = parity(name, p);
        // the stepped-random LFO draws its own sequence: only the level is comparable
        let band_ok = band <= 3.0 || *name == "lfo_random";
        if loud > 1.0 || !band_ok {
            bad.push(format!("{name}: loudness Δ {loud:.2} dB, band Δ {band:.2} dB"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// Opt-in: `PARITY_DUMP=<dir>` writes the legacy song of every synthetic patch (as `every_synthetic_patch_...` and
/// `unison_stacks_...` build them) as `<name>.song`, for `tools/unisynth-parity`.
#[test]
fn dump_the_synthetic_patch_songs_for_the_parity_tool() {
    let Some(dir) = std::env::var_os("PARITY_DUMP") else { return };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = |name: &str| format!("{}.song", name.replace(' ', "_"));
    for (name, p) in patches::instrument_patches() {
        std::fs::write(dir.join(file(name)), legacy_song(&p)).unwrap();
    }
    for (name, _, mut p) in patches::effect_patches() {
        p[GEN_UNISON] = 0.0;
        std::fs::write(dir.join(file(name)), legacy_song(&p)).unwrap();
    }
    let mut b = patches::base();
    b[GEN_HARMONICS] = 0.4;
    for (u, sp, dt) in [(0.2, 0.4, 0.0), (0.5, 0.4, 0.0), (1.0, 0.0, 0.0), (0.5, 0.0, 0.4), (1.0, 0.4, 0.4)] {
        let mut q = b;
        q[GEN_UNISON] = u;
        q[GEN_SPREAD] = sp;
        q[GEN_DETUNE] = dt;
        std::fs::write(dir.join(file(&format!("unison {u} spread {sp} detune {dt}"))), legacy_song(&q)).unwrap();
    }
}

#[test]
fn unison_stacks_match_on_average() {
    // random phases differ per engine; over many notes the stacks agree
    let mut b = patches::base();
    b[GEN_HARMONICS] = 0.4;
    let mut bad = vec![];
    for (u, sp, dt) in [(0.2, 0.4, 0.0), (0.5, 0.4, 0.0), (1.0, 0.0, 0.0), (0.5, 0.0, 0.4), (1.0, 0.4, 0.4)] {
        let mut q = b;
        q[GEN_UNISON] = u;
        q[GEN_SPREAD] = sp;
        q[GEN_DETUNE] = dt;
        let (loud, band) = parity(&format!("unison {u} spread {sp} detune {dt}"), &q);
        if loud > 1.0 || band > 4.0 {
            bad.push(format!("unison {u} spread {sp} detune {dt}: {loud:.2} dB, band {band:.2} dB"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn an_envelope_that_is_not_routed_to_volume_keeps_the_note_alive_until_it_has_finished() {
    let mut b = patches::base();
    b[GEN_HARMONICS] = 0.4;
    let mut worst = 0.0f64;
    for rel in [0.1, 0.4, 0.6, 0.8] {
        let mut q = b;
        q[ENV2..ENV2 + 5].copy_from_slice(&[0.1, 0.3, 0.5, rel, 0.5]);
        q[LP_ON] = 1.0;
        q[LP_CUT] = 0.8;
        patches::route(&mut q, 0, 4, 0.7, 12);
        worst = worst.max(parity(&format!("gate release {rel}"), &q).0);
    }
    assert!(worst <= 1.0, "{worst} dB");
}

/// The first frame after which both channels stay below `1e-5` for the rest of a render.
fn last_sound(x: &[f32]) -> usize {
    x.chunks(2).rposition(|c| c[0].abs().max(c[1].abs()) > 1e-5).map_or(0, |i| i + 1)
}

#[test]
fn the_note_ends_where_the_legacy_gate_closes() {
    // a note on at block 2, off at block 30: Tunefish keeps it sounding until every envelope in use has finished
    let mut base = patches::base();
    base[GEN_HARMONICS] = 0.4;
    let mut cases = vec![("no envelope".to_string(), base)];
    for (rel, sus) in [(0.3, 0.5), (0.5, 1.0), (0.7, 0.2), (0.4, 0.99)] {
        let mut p = base;
        p[ENV2..ENV2 + 5].copy_from_slice(&[0.1, 0.3, sus, rel, 0.5]);
        // ADSR 2 is a source, so it keeps the voice alive; what it modulates (the decay of the unused ADSR 1) is inaudible
        patches::route(&mut p, 0, 4, 0.7, 20);
        cases.push((format!("release {rel} sustain {sus}"), p));
    }
    for (name, p) in &cases {
        let blob = legacy_song_of(p, &[(2, 60, 100), (30, 60, 0)], 500);
        let want = last_sound(&legacy(&blob));
        let got = last_sound(&unified(&synth_convert::convert(&blob).unwrap().blob));
        assert!(want > 30 * 512, "{name}: the legacy note ends at {want}");
        // Tunefish fades the closing block out linearly; the converted note is cut after a third of it, which has the same
        // energy: it ends 2/3 of a block (341 frames) before the end of the legacy fade
        let lead = want as i64 - got as i64;
        assert!((250..=440).contains(&lead), "{name}: legacy ends at frame {want}, converted at {got}");
    }
}

#[test]
fn without_a_routed_envelope_the_note_gates_at_key_up() {
    let mut p = patches::base();
    p[GEN_HARMONICS] = 0.4;
    let (s, _) = convert_one(&p);
    let u = patch_of(&s, 0);
    // envelope 1: instant attack/decay, sustain 1, a third-of-a-block release, in timed mode; it does not scale the output
    assert_eq!(law::choice(u[pm::env(0, e::MODE)], 2), 0);
    near(u[pm::env(0, e::SUSTAIN)], 1.0, "sustain");
    near(law::env_ms(u[pm::env(0, e::RELEASE)]), 512.0 / 44.1 / 3.0, "release: the key-up block's fade, as a cut");
    assert_eq!(law::choice(u[v::ENV1_AT_OUTPUT], 2), 0);
    // an envelope routed elsewhere sets the release of the gate to its own length
    p[ENV2..ENV2 + 5].copy_from_slice(&[0.1, 0.3, 0.5, 0.6, 0.5]);
    patches::route(&mut p, 0, 4, 0.7, 12);
    let (s, _) = convert_one(&p);
    let u = patch_of(&s, 0);
    let gate = law::env_ms(u[pm::env(0, e::RELEASE)]);
    assert!((gate - synth_convert::tf::gate_ms(&p)).abs() < 1.0 && gate > 200.0, "{gate} ms");
    // ADSR 2 is envelope 3 of the unified patch (envelope 1 is the gate, envelope 2 is ADSR 1), in rate mode, with its own curve values
    assert_eq!(law::choice(u[pm::env(2, e::MODE)], 2), 1);
    near(u[pm::env(2, e::RELEASE)], 0.6, "release");
    near(u[pm::env(2, e::SLOPE)], 0.5, "slope");
}

#[test]
fn the_envelope_routed_to_volume_is_envelope_one_and_ends_the_voice() {
    // ADSR 2 → volume: it swaps places with ADSR 1, and every route to either follows
    let mut p = patches::base();
    p[ENV1..ENV1 + 5].copy_from_slice(&[0.2, 0.3, 0.4, 0.5, 0.6]);
    p[ENV2..ENV2 + 5].copy_from_slice(&[0.7, 0.6, 0.5, 0.4, 0.3]);
    patches::route(&mut p, 0, 4, 0.5, 5); // ADSR 2 → volume
    patches::route(&mut p, 1, 3, 0.7, 12); // ADSR 1 → LP cutoff
    patches::route(&mut p, 2, 4, 0.5, 20); // ADSR 2 → ADSR 1 decay
    let (s, _) = convert_one(&p);
    let u = patch_of(&s, 0);
    near(u[pm::env(0, e::ATTACK)], 0.7, "envelope 1 is ADSR 2");
    near(u[pm::env(1, e::ATTACK)], 0.2, "envelope 2 is ADSR 1");
    assert_eq!(law::choice(u[pm::env(0, e::MODE)], 2), 1);
    assert_eq!(route(&u, 0), (source::ENV1, target::osc(0, target::OSC_GAIN), 0.5, 0));
    assert_eq!(route(&u, 1), (source::ENV1 + 1, target::filter_cutoff(0), 0.7, 0));
    assert_eq!(route(&u, 2), (source::ENV1, target::ENV_DECAY + 1, 0.5, 0));
}

#[test]
fn the_parameters_decode_to_the_same_physical_values() {
    let mut p = patches::base();
    p[GAIN] = 0.75;
    p[GEN_VOLUME] = 0.8;
    p[GEN_OCTAVE] = 0.25; // 2^(4 − 2) = ×4: two octaves up
    p[GEN_UNISON] = 0.5; // round(4.5) + 1 = 6 layers
    p[GEN_DETUNE] = 0.4;
    p[GEN_SPREAD] = 0.3;
    p[GEN_HARMONICS] = 0.35;
    p[GEN_BANDWIDTH] = 0.2;
    p[GEN_DAMP] = 0.15;
    p[GEN_SCALE] = 0.3;
    p[GEN_MOD] = 0.6;
    p[GEN_DRIVE] = 0.45;
    p[GEN_POLY] = 0.2; // trunc(15·0.2 + 1) = 4
    p[GEN_GLIDE] = 0.5;
    p[NOISE_AMOUNT] = 0.4;
    p[NOISE_FREQ] = 0.5;
    p[NOISE_BW] = 0.1;
    p[LP_ON] = 1.0;
    p[LP_CUT] = 0.5;
    p[LP_RES] = 0.3;
    p[HP_ON] = 1.0;
    p[HP_CUT] = 0.5;
    p[HP_RES] = 0.2;
    p[BP_ON] = 0.5; // 0.5 is not above 0.5: off
    p[NT_ON] = 0.6;
    p[NT_CUT] = 0.5;
    p[NT_Q] = 0.4;
    p[LFO1..LFO1 + 4].copy_from_slice(&[0.6, 0.7, 0.25, 0.0]);
    p[LFO2..LFO2 + 4].copy_from_slice(&[0.0, 1.0, 0.75, 0.5]);
    let (s, w) = convert_one(&p);
    assert!(w.is_empty(), "{w:?}");
    let u = patch_of(&s, 0);
    let (os, ob) = (0, pm::OSC);
    let _ = (os, ob);
    let osc = |fld| u[pm::osc(0, fld)];
    let shp = |k| u[pm::osc(0, o::SHAPE + k)];
    near(law::gain(u[v::MASTER]), 1.0 + (0.75 - 0.5) * 20.0, "master is the gain curve");
    assert_eq!(law::count(u[v::CAPACITY], 1, 64), 4);
    near(law::time(u[v::GLIDE]), (0.5 * 10.0 + 1.0) * 512.0 / 44100.0, "glide: 6 blocks");
    assert_eq!(law::choice(osc(o::TYPE), osc_type::COUNT), osc_type::WAVETABLE);
    assert_eq!(law::choice(u[pm::osc(1, o::TYPE)], osc_type::COUNT), osc_type::OFF);
    near(law::octave(osc(o::OCTAVE)), 2.0, "octave");
    near(law::gain(osc(o::GAIN)), 0.4 * 0.8 * 2f32.sqrt(), "generator level: 0.4·vol, √2 for the centred balance");
    assert_eq!(law::count(shp(shape::WT_LAYERS), 1, 10), 6);
    near(shp(shape::WT_LAYER_DETUNE), 0.4, "detune");
    near(shp(shape::WT_LAYER_SPREAD), 0.3, "spread");
    near(shp(shape::WT_HARMONICS), 0.35, "harmonics");
    near(shp(shape::WT_BANDWIDTH), 0.2, "bandwidth");
    near(shp(shape::WT_DAMP), 0.15, "damp");
    near(shp(shape::WT_SCALE), 0.3, "scale");
    near(shp(shape::WT_MOD), 0.6, "mod");
    near(shp(shape::WT_DRIVE), 0.45, "drive");
    near(law::gain(u[v::NOISE_LEVEL]), 0.4 * 2f32.sqrt(), "noise level");
    assert_eq!(u[pm::ext::NOISE_WIDTH], 1.0, "Tunefish's noise has its own signal on each channel");
    // noise band: ladder low-pass at f + bw, RBJ high-pass at f − bw
    let nz = |k| u[pm::NOISE + k];
    assert_eq!(law::choice(nz(n::BAND_ON), 2), 1);
    near(law::cutoff(nz(n::LP)), 30.0 + 20000.0 * 0.36, "noise low-pass");
    near(law::cutoff(nz(n::HP)), 30.0 + 10000.0 * 0.16, "noise high-pass");
    // the band is on up to 0.98 and off from 0.99 up
    for (bw, on) in [(0.7, 1), (0.98, 1), (0.99, 0), (1.0, 0)] {
        let mut q = p;
        q[NOISE_BW] = bw;
        let (s, _) = convert_one(&q);
        assert_eq!(law::choice(patch_of(&s, 0)[pm::NOISE + n::BAND_ON], 2), on, "bandwidth {bw}");
    }
    // the four filters in Tunefish's order
    let ft = |i| law::choice(u[pm::filt(i, f::TYPE)], filter_type::COUNT);
    assert_eq!((ft(0), ft(1), ft(2), ft(3)), (filter_type::LADDER_LP, filter_type::RBJ_HP, filter_type::OFF, filter_type::NOTCH));
    near(law::cutoff(u[pm::filt(0, f::CUTOFF)]), 30.0 + 20000.0 * 0.25, "ladder cutoff");
    near(law::cutoff(u[pm::filt(1, f::CUTOFF)]), 30.0 + 10000.0 * 0.25, "RBJ high-pass cutoff");
    near(law::cutoff(u[pm::filt(3, f::CUTOFF)]), 30.0 + 20000.0 * 0.25, "notch cutoff");
    near(u[pm::filt(0, f::RESONANCE)], 0.3, "resonance");
    near(u[pm::filt(3, f::RESONANCE)], 0.4, "notch q");
    // LFOs: 50·p²/2π Hz, same shapes, key sync from 0.5 up
    near(law::lfo_rate(u[pm::lfo(0, l::RATE)]), 50.0 * 0.36 / std::f32::consts::TAU, "LFO 1 rate");
    assert_eq!(law::choice(u[pm::lfo(0, l::SHAPE)], 5), 1);
    assert_eq!(law::choice(u[pm::lfo(0, l::SYNC)], 2), 0, "free running");
    near(u[pm::lfo(0, l::DEPTH)], 0.7, "LFO 1 depth");
    assert_eq!(law::choice(u[pm::lfo(1, l::SHAPE)], 5), 3);
    assert_eq!(law::choice(u[pm::lfo(1, l::SYNC)], 2), 1, "synced");
    near(law::lfo_rate(u[pm::lfo(1, l::RATE)]), 0.01, "rate 0 is the slowest unified LFO");
    // velocity scales the generator and the noise; nothing else is routed
    assert_eq!(route(&u, 8), (source::VELOCITY, target::osc(0, target::OSC_GAIN), 0.5, 0));
    assert_eq!(route(&u, 9), (source::VELOCITY, target::NOISE_LEVEL, 0.5, 0));
    assert!((0..8).all(|i| route(&u, i).0 == source::NONE));
    assert_eq!(law::choice(u[v::VELOCITY], 2), 0);
}

#[test]
fn the_pan_keeps_the_left_and_right_levels_of_the_linear_balance() {
    for pan in [0.0f32, 0.1, 0.3, 0.5, 0.7, 1.0] {
        let mut p = patches::base();
        p[GEN_PAN] = pan;
        let (s, _) = convert_one(&p);
        let u = patch_of(&s, 0);
        let (l, r) = law::pan(u[pm::osc(0, o::PAN)]);
        let g = law::gain(u[pm::osc(0, o::GAIN)]) / 0.4;
        // Tunefish: the louder side 1, the other 2·min(p, 1 − p)
        let quiet = 2.0 * pan.min(1.0 - pan);
        let (want_l, want_r) = if pan <= 0.5 { (1.0, quiet) } else { (quiet, 1.0) };
        near(l * g, want_l, &format!("left at {pan}"));
        near(r * g, want_r, &format!("right at {pan}"));
    }
}

#[test]
fn mod_slots_become_scale_slots_at_the_same_index() {
    let mut p = patches::base();
    patches::route(&mut p, 0, 1, 0.5, 5); // LFO 1 → volume
    patches::route(&mut p, 1, 2, 0.7, 4); // LFO 2 → scale
    patches::route(&mut p, 2, 3, 0.25, 12); // ADSR 1 → LP cutoff
    patches::route(&mut p, 3, 5, 0.9, 31); // wheel → LFO 2 depth
    patches::route(&mut p, 4, 1, 0.6, 23); // LFO 1 → slot 2's own amount
    patches::route(&mut p, 5, 0, 0.6, 3); // no source: nothing
    patches::route(&mut p, 6, 7, 0.6, 3); // a reserved source: nothing
    patches::route(&mut p, 7, 1, 0.6, 38); // no such destination: nothing
    patches::route(&mut p, 8, 1, 0.6, 3); // slot 9 of 10: never evaluated, so dropped
    let (s, w) = convert_one(&p);
    let u = patch_of(&s, 0);
    let osc0 = |t| target::osc(0, t);
    assert_eq!(route(&u, 0), (source::LFO1, osc0(target::OSC_GAIN), 0.5, 0));
    assert_eq!(route(&u, 1), (source::LFO1 + 1, osc0(target::OSC_SHAPE + shape::WT_SCALE as u8), 0.7, 0));
    assert_eq!(route(&u, 2), (source::ENV1 + 1, target::filter_cutoff(0), 0.25, 0));
    assert_eq!(route(&u, 3), (source::WHEEL, target::LFO_DEPTH + 1, 0.9, 0));
    assert_eq!(route(&u, 4), (source::LFO1, target::SLOT + 1, 0.6, 0));
    for i in 5..8 {
        assert_eq!(route(&u, i).0, source::NONE, "slot {i}");
    }
    assert!(w.iter().any(|m| m.contains("mod slot 9")), "{w:?}");
}

#[test]
fn every_mod_target_maps_to_its_unified_counterpart() {
    use tfsynth::modmx::dst;
    let osc0 = |t| target::osc(0, t);
    let shp = |k: usize| osc0(target::OSC_SHAPE + k as u8);
    let want: [(u8, u8); 31] = [
        (dst::BANDWIDTH, shp(shape::WT_BANDWIDTH)),
        (dst::DAMP, shp(shape::WT_DAMP)),
        (dst::HARMONICS, shp(shape::WT_HARMONICS)),
        (dst::SCALE, shp(shape::WT_SCALE)),
        (dst::VOLUME, osc0(target::OSC_GAIN)),
        (dst::FREQ, osc0(target::OSC_PITCH)),
        (dst::PAN, osc0(target::OSC_PAN)),
        (dst::DETUNE, shp(shape::WT_LAYER_DETUNE)),
        (dst::SPREAD, shp(shape::WT_LAYER_SPREAD)),
        (dst::DRIVE, shp(shape::WT_DRIVE)),
        (dst::NOISE, target::NOISE_LEVEL),
        (dst::LP_CUT, target::filter_cutoff(0)),
        (dst::LP_RES, target::filter_resonance(0)),
        (dst::HP_CUT, target::filter_cutoff(1)),
        (dst::HP_RES, target::filter_resonance(1)),
        (dst::BP_CUT, target::filter_cutoff(2)),
        (dst::BP_Q, target::filter_resonance(2)),
        (dst::NT_CUT, target::filter_cutoff(3)),
        (dst::NT_Q, target::filter_resonance(3)),
        (dst::ADSR1_DECAY, target::ENV_DECAY + 1),
        (dst::ADSR2_DECAY, target::ENV_DECAY + 2),
        (dst::SLOT1, target::SLOT),
        (dst::SLOT1 + 1, target::SLOT + 1),
        (dst::SLOT1 + 2, target::SLOT + 2),
        (dst::SLOT1 + 3, target::SLOT + 3),
        (dst::SLOT1 + 4, target::SLOT + 4),
        (dst::SLOT1 + 5, target::SLOT + 5),
        (dst::SLOT1 + 6, target::SLOT + 6),
        (dst::SLOT1 + 7, target::SLOT + 7),
        (dst::LFO1_DEPTH, target::LFO_DEPTH),
        (dst::LFO2_DEPTH, target::LFO_DEPTH + 1),
    ];
    for (tf, uni) in want {
        let mut p = patches::base();
        patches::route(&mut p, 0, 1, 0.5, tf);
        let (s, _) = convert_one(&p);
        let u = patch_of(&s, 0);
        assert_eq!(route(&u, 0), (source::LFO1, uni, 0.5, 0), "destination {tf}");
    }
}

#[test]
fn a_fixed_frequency_and_the_slop_are_handled() {
    let mut p = patches::base();
    p[GEN_FREQ] = 0.4;
    p[GEN_DETUNE] = 0.5;
    p[GEN_OCTAVE] = 0.0;
    let (s, _) = convert_one(&p);
    let u = patch_of(&s, 0);
    assert_eq!(law::choice(u[pm::osc(0, o::TRACKING)], 2), 1);
    near(law::fixed_hz(u[pm::osc(0, o::FIXED_HZ)]), 160.0, "0.4² · 1000 Hz");
    near(u[pm::osc(0, o::SHAPE + shape::WT_LAYER_DETUNE)], 0.0, "a fixed frequency has no left/right detune");
    // slop below 1 Hz is inaudible; above it the converter says it is not converted
    let mut q = patches::base();
    q[GEN_SLOP] = 0.4;
    assert!(convert_one(&q).1.is_empty());
    q[GEN_SLOP] = 0.6;
    assert!(convert_one(&q).1.iter().any(|m| m.contains("GenSlop")));
}

// ---------------------------------------------------------------------------------------------------------------------
// kind 32 devices in synth-mix songs

fn device32(p: &[f32; COUNT]) -> Dev {
    dev(kind::INSTRUMENT, p)
}

fn lane(ev: &[(i32, u8, Option<u8>)]) -> Vec<(i32, u8, Option<u8>)> {
    ev.to_vec()
}

#[test]
fn a_tunefish_instrument_in_a_song_gets_its_effects_as_track_devices() {
    let mut p = lead();
    p[EFFECT_FIRST + 1] = 0.6; // formant after the chorus
    p[FORMANT_TYPE] = 0.5;
    p[FORMANT_WET] = 0.6;
    // the instrument is listed in two tracks (a shared device), each followed by a Tunefish reverb of its own
    let mut s = song(
        vec![device32(&p), dev(kind::REVERB, &[0.7, 0.4, 0.4, 0.8])],
        vec![lane(&[(0, 60, Some(100)), (20000, 60, None)]), vec![]],
        vec![track(&[0, 1], 0), track(&[0], 1), track(&[], 1)],
    );
    s.tracks[2].receives = vec![(0, 0, 1.0), (1, 0, 1.0)];
    let c = synth_convert::convert(&s.write()).unwrap();
    let out = unified_song(&c.blob);
    // song devices: the converted ones in place, then the inserts
    assert_eq!(out.devices[0].kind, unisynth::kind::INSTRUMENT);
    assert_eq!(fx_of(&out.devices[1]), (unisynth::kind::REVERB, 1));
    let chain: Vec<usize> = out.tracks[0].devices.clone();
    assert_eq!(chain.len(), 4, "instrument, chorus, formant, then the track's own reverb");
    assert_eq!(chain[0], 0);
    assert_eq!(out.devices[chain[1]].kind, unisynth::kind::CHORUS);
    assert_eq!(out.devices[chain[2]].kind, unisynth::kind::FORMANT);
    assert_eq!(chain[3], 1);
    // the other track lists the instrument again: the same chain devices follow it
    assert_eq!(out.tracks[1].devices, vec![0, chain[1], chain[2]]);
    assert_eq!(out.tracks[2].receives, s.tracks[2].receives);
    loads_and_renders(&c.blob);
}

#[test]
fn automation_of_the_instrument_and_of_its_effects_lands_on_the_right_devices() {
    let p = lead();
    let mut s = song(
        vec![device32(&p), dev(kind::REVERB, &[0.7, 0.4, 0.4, 0.8])],
        vec![lane(&[(0, 60, Some(100)), (20000, 60, None)])],
        vec![track(&[0, 1], 0), track(&[], 0)],
    );
    s.tracks[0].autos = vec![
        common::auto(0, CHORUS_DEPTH, &[(0, 0), (22050, 255)]),
        common::auto(0, GAIN, &[(0, 128), (22050, 255)]),
        common::auto(1, 2, &[(0, 0), (100, 255)]), // the standalone reverb's third parameter
        common::auto(0, 111 + 5, &[(0, 0)]),       // past the 112 values the device reads: nothing to drive
    ];
    s.tracks[1].receives = vec![(0, 0, 1.0)];
    let c = synth_convert::convert(&s.write()).unwrap();
    let out = unified_song(&c.blob);
    let t = &out.tracks[0];
    assert_eq!(t.devices.len(), 3, "instrument, chorus, reverb");
    let on = |dev: usize| t.autos.iter().filter(|a| a.device == dev).map(|a| a.param).collect::<Vec<_>>();
    // chorus parameters: rate, depth, gain → depth is the 2nd, +1 for the mode
    assert_eq!(on(1), vec![2]);
    assert_eq!(on(2), vec![3], "the reverb: 2 + 1");
    // the gain curve moves the master and nothing else
    assert_eq!(on(0), vec![v::MASTER]);
    // the chorus lane keeps the legacy points
    let a = t.autos.iter().find(|a| a.device == 1).unwrap();
    assert_eq!(a.points, vec![(0, 0), (22050, 255)]);
}

#[test]
fn automating_the_routing_of_a_tunefish_instrument_is_rejected() {
    let p = lead();
    for (what, param) in [("an effect slot's kind", EFFECT_FIRST + 3), ("a mod slot's source", MM_FIRST + 3 * 2), ("a mod slot's target", MM_FIRST + 3 * 7 + 2)] {
        let mut s = song(vec![device32(&p)], vec![lane(&[(0, 60, Some(100))])], vec![track(&[0], 0)]);
        s.tracks[0].autos = vec![common::auto(0, param, &[(0, 0), (1000, 255)])];
        let e = synth_convert::convert(&s.write()).err().unwrap_or_else(|| panic!("{what} was accepted"));
        assert!(e.to_string().contains(&format!("parameter {param}")) && e.to_string().contains("routing"), "{what}: {e}");
    }
    // the amount of a slot, and the (never evaluated) slots 9 and 10, may move
    for param in [MM_FIRST + 1, MM_FIRST + 3 * 7 + 1, MM_FIRST + 3 * 8, MM_FIRST + 3 * 9 + 2] {
        let mut s = song(vec![device32(&p)], vec![lane(&[(0, 60, Some(100))])], vec![track(&[0], 0)]);
        s.tracks[0].autos = vec![common::auto(0, param, &[(0, 0), (1000, 255)])];
        synth_convert::convert(&s.write()).unwrap_or_else(|e| panic!("parameter {param}: {e}"));
    }
}

#[test]
fn automated_tunefish_parameters_follow_the_legacy_curves() {
    // cutoff: the unified cutoff in Hz stays within one byte step of 30 + 20 000·p² at every 100-frame step
    let mut p = patches::base();
    p[LP_ON] = 1.0;
    p[LP_CUT] = 0.2;
    let mut s = song(vec![device32(&p)], vec![lane(&[(0, 60, Some(100))])], vec![track(&[0], 0)]);
    s.tracks[0].autos = vec![common::auto(0, LP_CUT, &[(0, 0), (44100, 255)])];
    let c = synth_convert::convert(&s.write()).unwrap();
    let out = unified_song(&c.blob);
    let a = out.tracks[0].autos.iter().find(|a| a.param == pm::filt(0, f::CUTOFF)).expect("a cutoff lane");
    let mut t = 0i32;
    let pts: Vec<(i32, u8)> = a
        .points
        .iter()
        .map(|&(d, b)| {
            t += d;
            (t, b)
        })
        .collect();
    let got = synth_convert::auto::read_lane(&pts, 440);
    let legacy = synth_convert::auto::read_lane(&[(0, 0), (44100, 255)], 440);
    for (k, (g, l)) in got.iter().zip(&legacy).enumerate() {
        let want = 30.0 + 20000.0 * l * l;
        let have = law::cutoff(*g);
        let step = law::cutoff(*g + 1.0 / 255.0) - have;
        assert!((have - want).abs() <= step.abs() + 1.0, "step {k}: {have} Hz vs {want} Hz");
    }
    // the filter switch (on above 0.5) steps instead of ramping
    let mut s = song(vec![device32(&p)], vec![lane(&[(0, 60, Some(100))])], vec![track(&[0], 0)]);
    s.tracks[0].autos = vec![common::auto(0, LP_ON, &[(0, 0), (44100, 255)])];
    let out = unified_song(&synth_convert::convert(&s.write()).unwrap().blob);
    let a = out.tracks[0].autos.iter().find(|a| a.param == pm::filt(0, f::TYPE)).expect("a type lane");
    let mut t = 0i32;
    let pts: Vec<(i32, u8)> = a.points.iter().map(|&(d, b)| { t += d; (t, b) }).collect();
    let v = synth_convert::auto::read_lane(&pts, 440);
    let first_on = v.iter().position(|x| law::choice(*x, filter_type::COUNT) == filter_type::LADDER_LP).unwrap();
    assert!((first_on as i64 * 100 - 22050).abs() <= 200, "switches on at {}", first_on * 100);
    assert!(v[..first_on].iter().all(|x| law::choice(*x, filter_type::COUNT) == filter_type::OFF));
}

#[test]
fn the_voice_limit_is_the_capacity() {
    for (poly, want) in [(0.0, 1), (0.2, 4), (0.5, 8), (1.0, 16)] {
        let mut p = patches::base();
        p[GEN_POLY] = poly;
        let (s, _) = convert_one(&p);
        assert_eq!(law::count(patch_of(&s, 0)[v::CAPACITY], 1, 64), want, "GenPoly {poly}");
    }
    // the largest value an automation reaches counts
    let mut p = patches::base();
    p[GEN_POLY] = 0.2;
    let mut s = song(vec![device32(&p)], vec![lane(&[(0, 60, Some(100))])], vec![track(&[0], 0)]);
    s.tracks[0].autos = vec![common::auto(0, GEN_POLY, &[(0, 0), (1000, 255)])];
    let out = unified_song(&synth_convert::convert(&s.write()).unwrap().blob);
    assert_eq!(law::count(patch_of(&out, 0)[v::CAPACITY], 1, 64), 16);
    assert!(out.tracks[0].autos.iter().all(|a| a.param != v::CAPACITY), "structural: never automated");
}

#[test]
fn the_patch_command_makes_a_song_from_text_files() {
    use std::process::Command;
    let dir = std::env::temp_dir().join(format!("synth-convert-tf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (pf, ef, out) = (dir.join("p.txt"), dir.join("e.txt"), dir.join("o.song"));
    let mut p = lead();
    p[GEN_HARMONICS] = 0.3;
    let txt: String = p.iter().map(|v| format!("{v}\n")).collect::<Vec<_>>().join("");
    std::fs::write(&pf, format!("# a patch\n{txt}")).unwrap();
    std::fs::write(&ef, "0 60 100  # on\n8 60 0\n").unwrap();
    let run = |a: &[&str]| Command::new(env!("CARGO_BIN_EXE_synth-convert")).args(a).output().unwrap();
    let r = run(&["patch", pf.to_str().unwrap(), ef.to_str().unwrap(), out.to_str().unwrap()]);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let s = unified_song(&std::fs::read(&out).unwrap());
    assert_eq!(s.tracks.len(), 2);
    assert_eq!(s.lanes[0], vec![(0, 60, Some(100)), (8 * 512, 60, None)]);
    loads_and_renders(&std::fs::read(&out).unwrap());
    // a `.tfm` by the command line
    let t = dir.join("a.tfm");
    std::fs::write(&t, tfm(125, &[(lead(), LEAD.to_vec())])).unwrap();
    let r = run(&["tfm", t.to_str().unwrap(), out.to_str().unwrap(), "2"]);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    loads_and_renders(&std::fs::read(&out).unwrap());
    // problems exit non-zero and leave no output
    std::fs::remove_file(&out).unwrap();
    std::fs::write(&pf, "1 2 3").unwrap();
    let r = run(&["patch", pf.to_str().unwrap(), ef.to_str().unwrap(), out.to_str().unwrap()]);
    assert!(!r.status.success() && !out.exists());
    assert!(String::from_utf8_lossy(&r.stderr).contains("112"));
    std::fs::write(&ef, "0 60").unwrap();
    assert!(!run(&["patch", pf.to_str().unwrap(), ef.to_str().unwrap(), out.to_str().unwrap()]).status.success());
    assert!(!run(&["tfm", pf.to_str().unwrap(), out.to_str().unwrap()]).status.success());
    assert!(!run(&["tfm", t.to_str().unwrap(), out.to_str().unwrap(), "x"]).status.success());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The files the walkthrough in `docs/synth/unisynth.md` uses stay valid: they convert and the song sounds.
#[test]
fn the_example_patch_and_phrase_convert_to_a_song_that_sounds() {
    use std::process::Command;
    let ex = concat!(env!("CARGO_MANIFEST_DIR"), "/examples");
    let out = std::env::temp_dir().join(format!("synth-convert-example-{}.song", std::process::id()));
    let r = Command::new(env!("CARGO_BIN_EXE_synth-convert"))
        .args(["patch", &format!("{ex}/patch.txt"), &format!("{ex}/phrase.txt")])
        .arg(&out)
        .output()
        .unwrap();
    assert!(r.status.success() && r.stderr.is_empty(), "{}", String::from_utf8_lossy(&r.stderr));
    let s = common::loads_and_renders(&std::fs::read(&out).unwrap());
    let _ = std::fs::remove_file(&out);
    assert!(s.iter().any(|v| v.abs() > 1e-3), "the example is silent");
    // 112 numbers, as the format says
    let text = std::fs::read_to_string(format!("{ex}/patch.txt")).unwrap();
    let n = text.lines().flat_map(|l| l.split('#').next().unwrap().split_whitespace().map(|w| w.parse::<f32>().unwrap()).collect::<Vec<_>>()).count();
    assert_eq!(n, tfsynth::params::COUNT);
}

/// How long the sound rings after the note-off: seconds until the 50 ms RMS stays 50 dB below the loudest window.
fn ring_time(x: &[f32], off_frame: usize) -> f64 {
    let w = 2205;
    let rms: Vec<f64> = x.chunks(2 * w).map(|c| (c.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / c.len() as f64).sqrt()).collect();
    let peak = rms.iter().cloned().fold(0.0, f64::max);
    let last = rms.iter().rposition(|r| *r > peak * 10f64.powf(-50.0 / 20.0)).unwrap_or(0);
    ((last + 1) * w) as f64 / 44100.0 - off_frame as f64 / 44100.0
}

#[test]
fn no_remorse_instruments_ring_as_long_as_their_legacy_render() {
    let path = std::env::var("NO_REMORSE_TFM").unwrap_or_else(|_| format!("{}/Downloads/64k-tools/synth-lab/tunefish/media/tf4modules/no_remorse.tfm", std::env::var("HOME").unwrap_or_default()));
    let Ok(b) = std::fs::read(&path) else { return eprintln!("skip: {path} not found") };
    let t = tfsynth::tfm::Tfm::parse(&b).unwrap();
    let mut bad = vec![];
    for i in 0..t.instruments() {
        let p = t.patch(i);
        // one note: 0.5 s held (43 blocks), then 8 s for the tail
        let blob = legacy_song_of(&p, &[(0, 60, 100), (43, 60, 0)], 43 + 690);
        let want = legacy(&blob);
        let got = unified(&synth_convert::convert(&blob).unwrap().blob);
        let (a, z) = (ring_time(&want, 43 * 512), ring_time(&got, 43 * 512));
        eprintln!("instrument {i:2}: legacy rings {a:6.2} s, unified {z:6.2} s");
        if (a - z).abs() > 0.25 + 0.1 * a {
            bad.push(format!("instrument {i}: legacy {a:.2} s, unified {z:.2} s"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// Diagnostic (opt-in, `NR_SOLO=1`): each no_remorse instrument solo through the reference player and the converted
/// song; prints the mean level difference in the quiet windows (tails 20–50 dB under the loudest) and in loud ones.
#[test]
fn no_remorse_solo_tails() {
    if std::env::var_os("NR_SOLO").is_none() { return; }
    use tfsynth::player::TfmPlayer;
    use tfsynth::tfm::Tfm;
    let path = format!("{}/Downloads/64k-tools/synth-lab/tunefish/media/tf4modules/no_remorse.tfm", std::env::var("HOME").unwrap_or_default());
    let b = std::fs::read(&path).unwrap();
    let n = Tfm::parse(&b).unwrap().instruments();
    let po = 4 + 2 * n + 4;
    for i in 0..n {
        let mut s = b.clone();
        for j in (0..n).filter(|&j| j != i) { s[po + j * COUNT + GAIN] = 0; }
        let leaked: &'static [u8] = Box::leak(s.clone().into_boxed_slice());
        let t = Tfm::parse(leaked).unwrap();
        let mem: &'static mut [u8] = Box::leak(vec![0u8; TfmPlayer::required_pool(&t)].into_boxed_slice());
        let pool: &'static mut Pool<'static> = Box::leak(Box::new(Pool::new(mem)));
        let mut p = TfmPlayer::new(t, pool, 0.0).unwrap();
        p.start(0.0);
        let blocks = p.blocks() + 400;
        let mut want = vec![];
        let mut out = [0i16; 1024];
        for _ in 0..blocks { p.render_i16(&mut out); want.extend(out.iter().map(|&v| v as f32 / 32768.0)); }
        let conv = convert_tfm(&s, blocks as f64 * 512.0 / 44100.0 - p.length() as f64).unwrap();
        let sg = Song::parse(&conv.blob).unwrap();
        let env = Env::for_song(&sg, None);
        let need = Player::<UDevice, Xorshift>::required_pool(&sg, &env, DEFAULT_SUB).unwrap();
        let mut mem = vec![0u8; need];
        let mut pool = Pool::new(&mut mem);
        let mut up: Player<UDevice, Xorshift> = Player::new(sg, &env, &mut pool, Xorshift::new(7), DEFAULT_SUB).unwrap();
        let mut got = vec![0f32; want.len()];
        up.render_f32(&mut got);
        let w = 4410;
        let db = |x: &[f32]| -> Vec<f64> { x.chunks(2 * w).map(|c| 10.0 * (c.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / c.len() as f64 + 1e-14).log10()).collect() };
        let (a, z) = (db(&want), db(&got));
        let pk = a.iter().cloned().fold(-200.0, f64::max);
        let (mut q, mut nq, mut l, mut nl) = (0.0, 0, 0.0, 0);
        for k in 0..a.len().min(z.len()) {
            if a[k] < pk - 20.0 && a[k] > pk - 50.0 { q += z[k] - a[k]; nq += 1; }
            if a[k] >= pk - 10.0 { l += z[k] - a[k]; nl += 1; }
        }
        if let (Some(dir), Ok(k)) = (std::env::var_os("NR_WAV"), std::env::var("NR_DUMP").map(|v| v.parse::<usize>())) {
            if k == Ok(i) {
                let wav = |x: &[f32], name: &str| {
                    let pcm: Vec<u8> = x.iter().flat_map(|v| ((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
                    let mut w = b"RIFF".to_vec(); w.extend((36 + pcm.len() as u32).to_le_bytes()); w.extend(b"WAVEfmt ");
                    for v in [16u32] { w.extend(v.to_le_bytes()); } w.extend(1u16.to_le_bytes()); w.extend(2u16.to_le_bytes());
                    w.extend(44100u32.to_le_bytes()); w.extend((44100u32 * 4).to_le_bytes()); w.extend(4u16.to_le_bytes()); w.extend(16u16.to_le_bytes());
                    w.extend(b"data"); w.extend((pcm.len() as u32).to_le_bytes()); w.extend(pcm);
                    std::fs::write(std::path::Path::new(&dir).join(name), w).unwrap();
                };
                wav(&want, &format!("lane{}-tunefish-reference.wav", i + 1)); wav(&got, &format!("lane{}-daw-unisynth.wav", i + 1));
            }
        }
        if std::env::var("NR_DUMP").ok().and_then(|v| v.parse::<usize>().ok()) == Some(i) {
            for k in 0..a.len().min(z.len()) { if a[k] > pk - 60.0 || z[k] > pk - 60.0 { eprintln!("  {:6.1}s ref {:6.1} ours {:6.1} Δ {:+5.1}", k as f64 / 10.0, a[k], z[k], z[k] - a[k]); } }
        }
        {
            let (la, ba) = common::metrics::metrics(&want, 44100.0);
            let (lz, bz) = common::metrics::metrics(&got, 44100.0);
            let bands: Vec<String> = ba.iter().zip(&bz).map(|(x, y)| format!("{:+5.1}", y - x)).collect();
            eprintln!("instrument {i:2}: bands Δ 63..16k [{}] (ref bands {:?}) loudness Δ {:+.1}", bands.join(" "), ba.map(|v| v.round() as i32), lz - la);
        }
        eprintln!("instrument {i:2}: loud Δ {:+.1} dB ({nl}), quiet Δ {:+.1} dB ({nq}), last loud end ref {:.1}s ours {:.1}s", l / nl.max(1) as f64, q / nq.max(1) as f64,
            a.iter().rposition(|v| *v > pk - 50.0).unwrap_or(0) as f64 / 10.0, z.iter().rposition(|v| *v > pk - 50.0).unwrap_or(0) as f64 / 10.0);
    }
}

/// Diagnostic (opt-in, `NR_NOTE=<instrument>`): short notes as in the song (one row on, one row off), 10 ms RMS of
/// the legacy and converted render side by side, with and without the instrument's effects.
const LIVE_SLOTS: usize = tfsynth::modmx::LIVE;
#[test]
fn no_remorse_short_note_shape() {
    let Some(i) = std::env::var("NR_NOTE").ok().and_then(|v| v.parse::<usize>().ok()) else { return };
    let path = format!("{}/Downloads/64k-tools/synth-lab/tunefish/media/tf4modules/no_remorse.tfm", std::env::var("HOME").unwrap_or_default());
    let b = std::fs::read(&path).unwrap();
    let t = tfsynth::tfm::Tfm::parse(&b).unwrap();
    for fx in [false, true] {
        let mut p = t.patch(i);
        if !fx { for k in 0..tfsynth::fx::SLOTS { p[EFFECT_FIRST + k] = 0.0; } }
        if std::env::var_os("NR_DET").is_some() { p[GEN_UNISON] = 0.0; p[GEN_SLOP] = 0.0; }
        if std::env::var_os("NR_NOMOD").is_some() { for k in 0..LIVE_SLOTS { p[MM_FIRST + 3 * k] = 0.0; } }
        if std::env::var_os("NR_NOLP").is_some() { p[LP_ON] = 0.0; }
        if let Ok(keep) = std::env::var("NR_KEEP") { let keep: Vec<usize> = keep.split(',').filter_map(|x| x.parse().ok()).collect(); for k in (0..LIVE_SLOTS).filter(|k| !keep.contains(k)) { p[MM_FIRST + 3 * k] = 0.0; } }
        // rows of 0.15 s = 12.92 blocks: on at 0, off at 13, on at 26 (another pitch), off at 39
        let blob = legacy_song_of(&p, &[(0, 55, 127), (13, 55, 0), (26, 67, 127), (39, 67, 0)], 39 + 120);
        let want = legacy(&blob);
        let got = unified(&synth_convert::convert(&blob).unwrap().blob);
        let w = 441;
        let db = |x: &[f32]| -> Vec<f64> { x.chunks(2 * w).map(|c| 10.0 * (c.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / c.len() as f64 + 1e-14).log10()).collect() };
        let (a, z) = (db(&want), db(&got));
        let (_, ba) = common::metrics::metrics(&want, 44100.0);
        let (_, bz) = common::metrics::metrics(&got, 44100.0);
        eprintln!("--- instrument {i}, effects {fx}: bands Δ [{}]", ba.iter().zip(&bz).map(|(x, y)| format!("{:+5.1}", y - x)).collect::<Vec<_>>().join(" "));
        if std::env::var_os("NR_BANDS").is_some() {
            let w = 2205 * 2; // 50 ms stereo
            for k in 0..8 {
                let (sa, sz) = (&want[k * w..(k + 1) * w], &got[k * w..(k + 1) * w]);
                let (_, ba) = common::metrics::metrics(sa, 44100.0);
                let (_, bz) = common::metrics::metrics(sz, 44100.0);
                eprintln!("{:4} ms legacy [{}]\n        unified[{}]", k * 50, ba.map(|v| format!("{:5.0}", v)).join(""), bz.map(|v| format!("{:5.0}", v)).join(""));
            }
            continue;
        }
        if std::env::var_os("NR_QUIET").is_some() { continue; }
        // brightness proxy: sign changes of the first difference (left channel) per 10 ms window
        let zc = |x: &[f32]| -> Vec<usize> { let l: Vec<f32> = x.iter().step_by(2).cloned().collect(); l.chunks(w).map(|c| c.windows(3).filter(|t| (t[1] - t[0]) * (t[2] - t[1]) < 0.0).count()).collect() };
        let (za, zz) = (zc(&want), zc(&got));
        for k in (0..a.len().min(z.len()).min(150)).step_by(3) { eprintln!("{:5} ms  legacy {:6.1}  unified {:6.1}  Δ {:+5.1}   peaks/10ms legacy {:4} unified {:4}", k * 10, a[k], z[k], z[k] - a[k], za[k], zz[k]); }
    }
}

/// An envelope (attack 0) scaling the wavetable harmonics: the note's first 50 ms is as bright as in Tunefish, which
/// builds the table from the matrix already stepped once (a volume envelope makes the attack what you hear most).
#[test]
fn an_enveloped_wavetable_is_bright_from_the_first_block() {
    let mut p = patches::base();
    p[GEN_UNISON] = 0.0;
    p[GEN_SLOP] = 0.0;
    p[GEN_HARMONICS] = 0.41;
    p[GEN_BANDWIDTH] = 1.0;
    p[LP_ON] = 0.0;
    p[ENV1..ENV1 + 5].copy_from_slice(&[0.0, 0.2, 0.1, 0.16, 1.0]);
    p[ENV2..ENV2 + 5].copy_from_slice(&[0.0, 0.21, 0.0, 0.05, 0.94]);
    for k in 0..LIVE_SLOTS { p[MM_FIRST + 3 * k] = 0.0; }
    p[MM_FIRST..MM_FIRST + 6].copy_from_slice(&[4.0 / 14.0, 0.5, 5.0 / 39.0, 3.0 / 14.0, 0.5, 3.0 / 39.0]); // ADSR2→volume, ADSR1→harmonics
    let blob = legacy_song_of(&p, &[(0, 55, 127), (13, 55, 0)], 13 + 40);
    let (want, got) = (legacy(&blob), unified(&synth_convert::convert(&blob).unwrap().blob));
    let w = 2 * 2205;
    let (_, a) = common::metrics::metrics(&want[..w], 44100.0);
    let (_, z) = common::metrics::metrics(&got[..w], 44100.0);
    for b in 3..7 { // 500 Hz – 4 kHz
        assert!((z[b] - a[b]).abs() < 4.0, "band {b}: legacy {:.1} dB, unified {:.1} dB", a[b], z[b]);
    }
}

/// A 2013 `.tf4m` instrument lands on the final layout: indices shift past the dropped generator values, the mod
/// matrix is re-encoded (ADSR 2 → volume at ×1 stays ADSR 2 → volume at ×1), and a reverb gets width 1.
#[test]
fn a_tf4m_instrument_is_translated_to_the_final_layout() {
    use synth_convert::tfm::{tf4m_to_tfm, TF4M_COUNT};
    let mut old = [0u8; TF4M_COUNT];
    old[0] = 70; // gain
    old[1] = 100; // profile (unused in 2013)
    old[3] = 41; // harmonics
    old[7] = 90; // volume
    old[23] = 33; // noise amount → final 17
    old[MM_FIRST + 6] = 100; // source round(4p) = 4: ADSR 2
    old[MM_FIRST + 7] = 55; // amount ((0.55 − 0.5)·20)² = 1
    old[MM_FIRST + 8] = 25; // target round(16p) = 4: VOL
    old[EFFECT_FIRST + 6] = 50; // slot 1: reverb
    let mut b = vec![1, 0, 120, 0, 2, 0];
    b.extend(b"INST");
    b.extend(old);
    b.extend(b"SONG");
    b.extend([0, 0, 4, 0, 60, 60, 100, 0]);
    b.extend(b"ENDS");
    let (tfm, warnings) = tf4m_to_tfm(&b).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let t = tfsynth::tfm::Tfm::parse(&tfm).unwrap();
    let p = t.patch(0);
    assert_eq!([p[GAIN], p[GEN_HARMONICS], p[GEN_VOLUME], p[NOISE_AMOUNT]], [0.7, 0.41, 0.9, 0.33]);
    assert_eq!(mod_source(p[MM_FIRST]), 4);
    assert_eq!(mod_target(p[MM_FIRST + 2]), tfsynth::modmx::dst::VOLUME);
    assert!((mod_amount(p[MM_FIRST + 1]) - 1.0).abs() < 1e-6);
    assert_eq!(effect_kind(p[EFFECT_FIRST]), 5);
    assert_eq!(p[REV_WIDTH], 1.0);
    assert_eq!(t.event_count(0), 2);
    // a final-layout .tfm is refused, not misread
    assert!(tf4m_to_tfm(&tfm).is_err());
}
