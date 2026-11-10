use super::*;

/// Writes an XM as FT2 does (header 276, pattern header 9, packed cells, instrument header 263 or 29 when empty,
/// sample headers 40, delta-coded data).
pub(crate) fn write(x: &Xm) -> Vec<u8> {
    let mut b = Vec::new();
    let pad = |b: &mut Vec<u8>, s: &str, n: usize| {
        let mut v = s.as_bytes().to_vec();
        v.resize(n, 0);
        b.extend(v);
    };
    let u16le = |b: &mut Vec<u8>, v: u16| b.extend(v.to_le_bytes());
    let u32le = |b: &mut Vec<u8>, v: u32| b.extend(v.to_le_bytes());
    b.extend(b"Extended Module: ");
    pad(&mut b, &x.name, 20);
    b.push(0x1A);
    pad(&mut b, "FastTracker v2.00", 20);
    u16le(&mut b, 0x0104);
    u32le(&mut b, 276);
    for v in [x.order.len() as u16, x.restart, x.channels, x.patterns.len() as u16, x.instruments.len() as u16, x.linear as u16, x.speed, x.bpm] {
        u16le(&mut b, v);
    }
    let mut ord = x.order.clone();
    ord.resize(256, 0);
    b.extend(ord);
    for p in &x.patterns {
        let mut d = Vec::new();
        for c in &p.cells {
            // FT2 packing: a flag byte with bit 7 and one bit per present field (all present written unpacked)
            let f = (c.note != 0) as u8 | (c.instrument != 0) as u8 * 2 | (c.volume != 0) as u8 * 4 | (c.effect != 0) as u8 * 8 | (c.param != 0) as u8 * 16;
            if f == 31 {
                d.extend([c.note, c.instrument, c.volume, c.effect, c.param]);
            } else {
                d.push(0x80 | f);
                for (bit, v) in [(1, c.note), (2, c.instrument), (4, c.volume), (8, c.effect), (16, c.param)] {
                    if f & bit != 0 {
                        d.push(v);
                    }
                }
            }
        }
        u32le(&mut b, 9);
        b.push(0);
        u16le(&mut b, p.rows);
        u16le(&mut b, d.len() as u16);
        b.extend(d);
    }
    for ins in &x.instruments {
        let start = b.len();
        u32le(&mut b, if ins.samples.is_empty() { 29 } else { 263 });
        pad(&mut b, &ins.name, 22);
        b.push(0);
        u16le(&mut b, ins.samples.len() as u16);
        if ins.samples.is_empty() {
            continue;
        }
        u32le(&mut b, 40);
        b.extend(ins.keymap);
        for e in [&ins.vol_env, &ins.pan_env] {
            for i in 0..12 {
                let (t, v) = e.points.get(i).copied().unwrap_or((0, 0));
                u16le(&mut b, t);
                u16le(&mut b, v);
            }
        }
        b.extend([ins.vol_env.points.len() as u8, ins.pan_env.points.len() as u8]);
        b.extend([ins.vol_env.sustain, ins.vol_env.loop_start, ins.vol_env.loop_end]);
        b.extend([ins.pan_env.sustain, ins.pan_env.loop_start, ins.pan_env.loop_end]);
        b.extend([ins.vol_env.flags, ins.pan_env.flags]);
        b.extend(ins.vibrato);
        u16le(&mut b, ins.fadeout);
        b.resize(start + 263, 0);
        for s in &ins.samples {
            let bytes = if s.bits16 { 2 } else { 1 };
            for v in [s.data.len() as u32 * bytes, s.loop_start * bytes, s.loop_len * bytes] {
                u32le(&mut b, v);
            }
            b.extend([s.volume, s.finetune as u8, s.loop_kind | if s.bits16 { 16 } else { 0 }, s.pan, s.relative as u8, 0]);
            pad(&mut b, &s.name, 22);
        }
        for s in &ins.samples {
            let mut prev = 0i16;
            for &v in &s.data {
                if s.bits16 {
                    b.extend(v.wrapping_sub(prev).to_le_bytes());
                } else {
                    b.push(((v >> 8) as i8).wrapping_sub((prev >> 8) as i8) as u8);
                }
                prev = v;
            }
        }
    }
    b
}

fn cell(note: u8, instrument: u8, volume: u8, effect: u8, param: u8) -> Cell {
    Cell { note, instrument, volume, effect, param }
}

fn sample() -> Xm {
    let env = Env { points: vec![(0, 48), (4, 64), (8, 44)], sustain: 1, loop_start: 0, loop_end: 2, flags: 3 };
    let mut keymap = [0u8; 96];
    keymap[60] = 1;
    let blank = Cell::default();
    Xm {
        name: "test".into(),
        channels: 2,
        restart: 0,
        linear: true,
        speed: 6,
        bpm: 151,
        order: vec![0, 1, 0],
        patterns: vec![
            Pattern { rows: 2, cells: vec![cell(61, 1, 0x40, 0x0F, 3), blank, blank, cell(97, 0, 0, 0x0D, 0)] },
            Pattern { rows: 1, cells: vec![cell(49, 2, 0, 0, 0), cell(0, 0, 0, 0x0C, 0x20)] },
        ],
        instruments: vec![
            Instrument {
                name: "Probe11".into(),
                keymap,
                vol_env: env.clone(),
                pan_env: Env { points: vec![(0, 32), (10, 0)], sustain: 0, loop_start: 0, loop_end: 0, flags: 1 },
                vibrato: [0, 0, 0, 0],
                fadeout: 128,
                samples: vec![
                    Sample { name: "a".into(), loop_start: 1, loop_len: 2, volume: 64, finetune: -3, loop_kind: 2, bits16: false, pan: 128, relative: 12, data: vec![0, 256, -512, 32512] },
                    Sample { name: "b".into(), loop_start: 0, loop_len: 0, volume: 32, finetune: 5, loop_kind: 0, bits16: true, pan: 0, relative: -1, data: vec![1, -2, 30000, -30000, 7] },
                ],
            },
            Instrument { name: "empty".into(), keymap: [0; 96], vol_env: Env::default(), pan_env: Env::default(), vibrato: [0; 4], fadeout: 0, samples: vec![] },
        ],
        mix: Mix::Ft2,
    }
}

#[test]
fn a_written_xm_reads_back() {
    let x = sample();
    assert_eq!(parse(&write(&x)).unwrap(), x);
}

#[test]
fn every_truncation_is_an_error() {
    let b = write(&sample());
    for n in 0..b.len() {
        assert!(parse(&b[..n]).is_err(), "{n} of {} bytes read", b.len());
    }
}

#[test]
fn unsupported_files_name_their_cause() {
    let b = write(&sample());
    let err = |f: &dyn Fn(&mut Vec<u8>)| {
        let mut c = b.clone();
        f(&mut c);
        parse(&c).unwrap_err()
    };
    assert!(err(&|c| c[0] = b'x').contains("not an XM"));
    assert!(err(&|c| c[58] = 3).contains("version"));
    assert!(err(&|c| c[60] = 0x15).contains("header size"));
    assert!(err(&|c| c[74] = 0).contains("linear"), "Amiga frequency table");
    let pat = 60 + 276;
    assert!(err(&|c| c[pat] = 10).contains("pattern 0 header"));
    assert!(err(&|c| c[pat + 4] = 1).contains("packing"));
    let mut x = sample();
    x.patterns[0].cells.pop(); // a pattern's data must hold rows × channels cells
    assert!(parse(&write(&x)).unwrap_err().contains("pattern 0"));
}

/// openmpt's mix by its tracker detection (change add-fudge-softsynth): the 1998 softsynth XMs as openmpt123 --info
/// names them (Alien Sex Clone and Theta "ModPlug Tracker 1.0 - 1.16", Pure "FastTracker 2 or compatible").
#[test]
fn the_mix_follows_openmpts_detection() {
    let zenekk = std::env::var("ZENEKK_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Downloads/zenekk"));
    for (f, want) in [("Fudge - Alien Sex Clone.xm", Mix::ModPlug), ("Quad - Theta.xm", Mix::ModPlug), ("Quad - Pure.xm", Mix::Ft2)] {
        let Ok(b) = std::fs::read(zenekk.join(f)) else {
            eprintln!("skip: {f} absent");
            continue;
        };
        assert_eq!(parse(&b).unwrap().mix, want, "{f}");
    }
}

#[test]
fn a_modplug_files_linear_pan_as_equal_power() {
    // hard left and right keep the level; the centre is linear's ½ per side: √(1 − p′)·g = 1 − p, √p′·g = p
    for p in [0.0, 0.2, 0.5, 0.9, 1.0] {
        let (g, q) = Mix::ModPlug.pan(1.0, p);
        let (l, r) = ((1.0 - q).sqrt() * g, q.sqrt() * g);
        assert!((l - (1.0 - p)).abs() < 1e-12 && (r - p).abs() < 1e-12, "{p}: {l} {r}");
    }
    assert_eq!(Mix::Ft2.pan(0.7, 0.3), (0.7, 0.3));
    // the level follows openmpt's pre-amp table: 20 channels 168, 18 channels 164
    assert!((Mix::ModPlug.level(20) / Mix::ModPlug.level(18) - 164.0 / 168.0).abs() < 1e-12);
    assert_eq!(Mix::Ft2.level(20), crate::cauldron::import::XM_GAIN);
}
