use super::super::{Cell, Env, Instrument, Pattern, Sample, Xm};
use super::*;

const C5: u8 = 61; // C-5 (1 = C-0)

fn smp(relative: i8, finetune: i8) -> Sample {
    Sample { name: "s".into(), loop_start: 0, loop_len: 0, volume: 48, finetune, loop_kind: 0, bits16: false, pan: 128, relative, data: vec![0; 1000] }
}

fn ins(vol_env: Env, fadeout: u16) -> Instrument {
    Instrument { name: "i".into(), keymap: [0; 96], vol_env, pan_env: Env::default(), vibrato: [0; 4], fadeout, samples: vec![smp(0, 0)] }
}

/// One channel, `rows` rows of `cells` (row → cell), speed 6, bpm 125 (a tick is 20 ms).
fn song(rows: u16, cells: &[(usize, Cell)], instruments: Vec<Instrument>) -> Xm {
    let mut c = vec![Cell::default(); rows as usize];
    for &(r, cell) in cells {
        c[r] = cell;
    }
    Xm { name: "t".into(), channels: 1, restart: 0, linear: true, speed: 6, bpm: 125, order: vec![0], patterns: vec![Pattern { rows, cells: c }], instruments }
}

fn note(n: u8, effect: u8, param: u8) -> Cell {
    Cell { note: n, instrument: 1, volume: 0, effect, param }
}

const TICK: f64 = 2.5 / 125.0;

fn run(x: &Xm) -> Replay {
    replay(x, 44100).unwrap()
}

#[test]
fn a_tick_is_whole_frames() {
    // as openmpt renders: trunc(rate·2.5/bpm) frames (bpm 151: 730, not 730.13)
    let mut x = song(2, &[(0, note(C5, 0, 0)), (1, note(C5, 0, 0))], vec![ins(Env::default(), 0)]);
    x.bpm = 151;
    let r = run(&x);
    assert_eq!(r.triggers[1].at, 6.0 * 730.0 / 44100.0);
    assert_eq!(r.triggers[1].frame, 6 * 730);
}

#[test]
fn timing_follows_speed_and_bpm() {
    let r = run(&song(4, &[(0, note(C5, 0, 0)), (2, note(C5, 0xF, 3)), (3, note(C5, 0xF, 250))], vec![ins(Env::default(), 0)]));
    let at: Vec<f64> = r.triggers.iter().map(|t| t.at).collect();
    // rows 0, 1 at speed 6; row 2 sets speed 3 on its tick 0; row 3 sets bpm 250 (a tick 10 ms)
    for (a, b) in at.iter().zip([0.0, 12.0 * TICK, 15.0 * TICK]) {
        assert!((a - b).abs() < 1e-12, "{at:?}");
    }
    assert!((r.length - (15.0 * TICK + 3.0 * 0.01)).abs() < 1e-9, "{}", r.length);
}

#[test]
fn linear_frequency_table() {
    // C-4 with no relative note or finetune is 8363 Hz; relative +12 an octave; finetune 64 half a semitone
    let mut i = ins(Env::default(), 0);
    let r = run(&song(1, &[(0, note(49, 0, 0))], vec![i.clone()]));
    assert!((r.triggers[0].ticks[0].freq - 8363.0).abs() < 1e-6);
    i.samples[0] = smp(12, 64);
    let r = run(&song(1, &[(0, note(49, 0, 0))], vec![i]));
    let want = 8363.0 * 2f64.powf(12.5 / 12.0);
    assert!((r.triggers[0].ticks[0].freq - want).abs() < 1e-6);
}

#[test]
fn note_delay_cut_and_retrigger() {
    let r = run(&song(1, &[(0, note(C5, 0xE, 0xD3))], vec![ins(Env::default(), 0)]));
    assert_eq!(r.triggers[0].at, 3.0 * TICK, "ED3: the note starts 3 ticks in");
    let r = run(&song(1, &[(0, note(C5, 0xE, 0x93))], vec![ins(Env::default(), 0)]));
    assert_eq!(r.triggers.iter().map(|t| t.tick).collect::<Vec<_>>(), [0, 3], "E93 at speed 6: ticks 0 and 3");
    let r = run(&song(1, &[(0, note(C5, 0xE, 0xC2))], vec![ins(Env::default(), 0)]));
    let v: Vec<f64> = r.triggers[0].ticks.iter().map(|c| c.vol).collect();
    assert!(v[1] > 0.0 && v[2..].iter().all(|&x| x == 0.0), "EC2: silent from tick 2 {v:?}");
}

#[test]
fn sample_offset_and_its_memory() {
    let r = run(&song(2, &[(0, note(C5, 9, 4)), (1, note(C5, 9, 0))], vec![ins(Env::default(), 0)]));
    assert_eq!(r.triggers.iter().map(|t| t.offset).collect::<Vec<_>>(), [1024, 1024]);
}

#[test]
fn volume_column_slides_and_set_volume() {
    let mut c = note(C5, 0xA, 0x20); // up 2 per tick after tick 0
    c.volume = 0x10 + 20; // set volume 20
    let r = run(&song(2, &[(0, c), (1, Cell { effect: 0xC, param: 64, ..Cell::default() })], vec![ins(Env::default(), 0)]));
    let v: Vec<f64> = r.triggers[0].ticks.iter().map(|c| c.vol * 64.0).collect();
    assert_eq!(&v[..6], [20.0, 22.0, 24.0, 26.0, 28.0, 30.0]);
    assert_eq!(v[6], 64.0, "C40 on the next row");
    // no volume column: the sample's volume
    let r = run(&song(1, &[(0, note(C5, 0, 0))], vec![ins(Env::default(), 0)]));
    assert_eq!(r.triggers[0].ticks[0].vol, 48.0 / 64.0);
}

#[test]
fn panning_from_effect_and_volume_column() {
    let mut c = note(C5, 8, 0x40);
    let r = run(&song(1, &[(0, c)], vec![ins(Env::default(), 0)]));
    assert_eq!(r.triggers[0].ticks[0].pan, 64.0 / 256.0);
    c.effect = 0;
    c.volume = 0xC0 + 15;
    let r = run(&song(1, &[(0, c)], vec![ins(Env::default(), 0)]));
    assert_eq!(r.triggers[0].ticks[0].pan, 240.0 / 256.0);
}

#[test]
fn portamento_and_tone_portamento() {
    // 1xx: period −4·xx per tick after tick 0 (linear table: 64 per semitone), so 16 per tick is ¼ semitone
    let r = run(&song(1, &[(0, note(49, 1, 4))], vec![ins(Env::default(), 0)]));
    let f = |t: usize| r.triggers[0].ticks[t].freq;
    assert!((f(5) / f(0) - 2f64.powf(5.0 / 48.0)).abs() < 1e-9);
    // 3xx towards the row's note: no new trigger, stops at the target
    let r = run(&song(2, &[(0, note(49, 0, 0)), (1, note(51, 3, 0x20))], vec![ins(Env::default(), 0)]));
    assert_eq!(r.triggers.len(), 1, "tone portamento does not retrigger");
    let f: Vec<f64> = r.triggers[0].ticks.iter().map(|c| c.freq).collect();
    let target = 8363.0 * 2f64.powf(2.0 / 12.0);
    assert!(f[7] > f[6] && (f[11] - target).abs() < 1e-6, "{f:?}");
    // E1x: once, on tick 0 (E18: 32 period units, half a semitone)
    let r = run(&song(1, &[(0, note(49, 0xE, 0x18))], vec![ins(Env::default(), 0)]));
    let f = |t: usize| r.triggers[0].ticks[t].freq;
    assert!((f(0) / 8363.0 - 2f64.powf(0.5 / 12.0)).abs() < 1e-9 && f(5) == f(0));
}

#[test]
fn slides_stop_at_ft2_s_period_limits() {
    // FT2 clamps a slid period to 1–31 999 (BLUISH slides C-1 up by 1 30 for rows)
    let up = Cell { note: 0, instrument: 0, volume: 0, effect: 1, param: 0xFF };
    let r = run(&song(8, &[(0, note(13, 1, 0xFF)), (1, up), (2, up), (3, up), (4, up), (5, up), (6, up), (7, up)], vec![ins(Env::default(), 0)]));
    let top = 8363.0 * 2f64.powf((4608.0 - 1.0) / 768.0);
    assert_eq!(r.triggers[0].ticks.last().unwrap().freq, top);
    let down = Cell { effect: 2, ..up };
    let r = run(&song(8, &[(0, note(13, 2, 0xFF)), (1, down), (2, down), (3, down), (4, down), (5, down), (6, down), (7, down)], vec![ins(Env::default(), 0)]));
    assert_eq!(r.triggers[0].ticks.last().unwrap().freq, 8363.0 * 2f64.powf((4608.0 - 31999.0) / 768.0));
}

#[test]
fn envelopes_hold_at_sustain_loop_and_fade_after_key_off() {
    // volume envelope 64 → 0 over 10 ticks, sustain at point 0; key off on row 1 (tick 6), fadeout 4096 (8 ticks)
    let env = Env { points: vec![(0, 64), (10, 0)], sustain: 0, loop_start: 0, loop_end: 0, flags: 3 };
    let off = Cell { note: 97, ..Cell::default() };
    let r = run(&song(3, &[(0, note(C5, 0xC, 64)), (1, off)], vec![ins(env, 4096)]));
    let v: Vec<f64> = r.triggers[0].ticks.iter().map(|c| c.vol).collect();
    assert!(v[..6].iter().all(|&x| x == 1.0), "held at the sustain point {v:?}");
    assert!(v[7] < v[6] && v[7] > 0.0, "released: the envelope runs on, fading");
    assert_eq!(*v.last().unwrap(), 0.0);
    assert!(r.triggers[0].ticks.len() < 18, "a faded-out note ends");
    // a loop: points 0..2 cycle
    let env = Env { points: vec![(0, 64), (2, 0), (4, 64)], sustain: 0, loop_start: 0, loop_end: 2, flags: 5 };
    let r = run(&song(2, &[(0, note(C5, 0xC, 64))], vec![ins(env, 0)]));
    let v: Vec<f64> = r.triggers[0].ticks.iter().map(|c| (c.vol * 64.0).round()).collect();
    assert_eq!(&v[..9], [64.0, 32.0, 0.0, 32.0, 64.0, 32.0, 0.0, 32.0, 64.0]);
    // key off without a volume envelope cuts the note
    let r = run(&song(2, &[(0, note(C5, 0, 0)), (1, off)], vec![ins(Env::default(), 0)]));
    assert_eq!(r.triggers[0].ticks.len(), 6);
}

#[test]
fn pattern_break_and_delay() {
    let mut x = song(4, &[(0, note(C5, 0xD, 0x02))], vec![ins(Env::default(), 0)]);
    x.patterns[0].cells[3] = note(C5, 0, 0);
    x.order = vec![0, 0];
    // D02 on row 0: order 1 starts at row 2; the song ends when the order list ends
    let r = run(&x);
    assert_eq!(r.triggers.iter().map(|t| t.tick).collect::<Vec<_>>(), [0, 12], "rows 0 | 2, 3");
    let r = run(&song(2, &[(0, note(C5, 0xE, 0xE2)), (1, note(C5, 0, 0))], vec![ins(Env::default(), 0)]));
    assert_eq!(r.triggers[1].tick, 18, "EE2: row 0 lasts three rows");
}

#[test]
fn unsupported_effects_are_named() {
    let e = replay(&song(3, &[(2, note(C5, 0x7, 0x11))], vec![ins(Env::default(), 0)]), 44100).unwrap_err();
    assert!(e.contains("effect 7") && e.contains("pattern 0") && e.contains("row 2") && e.contains("channel 0"), "{e}");
    let mut i = ins(Env::default(), 0);
    i.vibrato = [0, 0, 4, 2];
    assert!(replay(&song(1, &[(0, note(C5, 0, 0))], vec![i]), 44100).unwrap_err().contains("auto-vibrato"));
}
