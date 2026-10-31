//! Mini-cauldron instruments (change add-cauldron-xm-import): every record of both intros builds a unified
//! instrument that loads in the unified player and, through the chain's clip, plays a finite note within full scale.
mod common;
use common::*;
use synth_convert::cauldron::{effects, instrument, load, velocity};
use synth_convert::model::Dev;

fn data(name: &str) -> String {
    std::fs::read_to_string(format!("{}/../../research/tbl-mini-cauldron/ref/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn every_record_loads_and_plays_within_full_scale() {
    let mut n = 0;
    for f in ["patches_jizz.json", "patches_stash.json"] {
        for c in load(&data(f)).unwrap().calls {
            for (i, r) in c.records.iter().enumerate() {
                for ratio in [1.0, 2.0] {
                    let chunk = unisynth::chunk::encode(&instrument(c.version, r, ratio), &[]);
                    let lane = vec![(0, 61, Some(velocity(0.0))), (10000, 61, None)];
                    // the instrument and the chain's first device, the 16-bit clamp
                    let (kind, clip) = effects(&c, i, ratio, 44100.0, 125).devices.remove(0);
                    let devs = vec![Dev { kind: unisynth::kind::INSTRUMENT, chunk }, Dev { kind, chunk: unisynth::fx::encode(&clip) }];
                    let s = song(devs, vec![lane], vec![track(&[0, 1], 0)]);
                    let out = loads_and_renders(&s.write());
                    let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
                    assert!(peak > 1e-3, "{} record {i} ratio {ratio}: silent", c.name);
                    assert!(peak <= 1.0 + 1e-4, "{} record {i} ratio {ratio}: peak {peak} past the clip", c.name);
                    n += 1;
                }
            }
        }
    }
    assert_eq!(n, 2 * (12 + 10 + 16));
}

#[test]
fn every_record_with_its_effect_chain_loads_and_plays() {
    let mut n = 0;
    for f in ["patches_jizz.json", "patches_stash.json"] {
        for c in load(&data(f)).unwrap().calls {
            for (i, r) in c.records.iter().enumerate() {
                for ratio in [1.0, 2.0] {
                    let mut s = song(vec![], vec![vec![(0, 61, Some(velocity(0.0))), (10000, 61, None)]], vec![]);
                    let chain = effects(&c, i, ratio, s.rate as f64, s.tempo);
                    s.devices.push(Dev { kind: unisynth::kind::INSTRUMENT, chunk: unisynth::chunk::encode(&instrument(c.version, r, ratio), &[]) });
                    for (kind, v) in &chain.devices {
                        s.devices.push(Dev { kind: *kind, chunk: unisynth::fx::encode(v) });
                    }
                    s.tracks.push(track(&(0..s.devices.len()).collect::<Vec<_>>(), 0));
                    let out = loads_and_renders(&s.write());
                    let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
                    // a comb doubles a full-scale signal at most (the original clamped it to 8 bits; the port does not)
                    // and the echo feeds back below 1: anything far past that is a broken chain
                    assert!(peak > 1e-3 && peak <= 4.0, "{} record {i} ratio {ratio}: peak {peak}", c.name);
                    n += 1;
                }
            }
        }
    }
    assert_eq!(n, 2 * (12 + 10 + 16));
}

#[test]
fn every_phrase_song_loads_and_plays() {
    use synth_convert::cauldron::phrase_song;
    let mut n = 0;
    for f in ["patches_jizz.json", "patches_stash.json"] {
        for c in load(&data(f)).unwrap().calls {
            for (i, r) in c.records.iter().enumerate() {
                for v in 0..r.variants as usize {
                    let (s, lead) = phrase_song(&c, i, v, 44100);
                    assert!(lead > 0 && lead < 2000, "{} {i}/{v}: lead {lead}", c.name);
                    let out = loads_and_renders(&s.write());
                    assert!(out[2 * lead..].iter().any(|x| x.abs() > 1e-3), "{} {i}/{v}: silent", c.name);
                    n += 1;
                }
            }
        }
    }
    assert_eq!(n, 32 + 53, "every generated phrase");
}

/// The three rebuilt XMs (opt-in: `research/tbl-mini-cauldron/out`, made by `run_all.sh`) read, and a generated
/// sample decodes to its reference render.
#[test]
fn the_tbl_xms_read() {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../research/tbl-mini-cauldron/out");
    if !out.join("listen/jizz.xm").exists() {
        eprintln!("skip: {} absent", out.display());
        return;
    }
    for f in ["jizz", "stash_song1", "stash_bluish"] {
        let x = synth_convert::xm::parse(&std::fs::read(out.join(format!("listen/{f}.xm"))).unwrap()).unwrap_or_else(|e| panic!("{f}: {e}"));
        assert!(!x.order.is_empty() && !x.instruments.is_empty(), "{f}");
    }
    let x = synth_convert::xm::parse(&std::fs::read(out.join("listen/jizz.xm")).unwrap()).unwrap();
    assert_eq!((x.channels, x.patterns.len(), x.instruments.len(), x.speed, x.bpm), (14, 17, 82, 6, 151));
    let s = x.instruments.iter().flat_map(|i| &i.samples).find(|s| s.name == "Probe11.smp").expect("Probe11");
    let want: Vec<i16> = std::fs::read(out.join("render_jizz/Probe11.s8")).unwrap().iter().map(|&b| b as i8 as i16 * 256).collect();
    assert_eq!(s.data, want);
}

/// The three TBL songs replay (opt-in, as `the_tbl_xms_read`): no unsupported effect, and the order list ends at
/// openmpt's song length (the reference renders, to the frame). The trigger times against openmpt's are checked by
/// `bun tools/unisynth-parity/cauldron.ts --onsets` (every sample a click).
#[test]
fn the_tbl_songs_replay() {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../research/tbl-mini-cauldron/out");
    if !out.join("listen/jizz.xm").exists() {
        eprintln!("skip: {} absent", out.display());
        return;
    }
    for (f, secs) in [("jizz", 166.261905), ("stash_song1", 203.506803), ("stash_bluish", 84.279864)] {
        let x = synth_convert::xm::parse(&std::fs::read(out.join(format!("listen/{f}.xm"))).unwrap()).unwrap();
        let r = synth_convert::xm::replay::replay(&x, 44100).unwrap_or_else(|e| panic!("{f}: {e}"));
        eprintln!("{f}: {} triggers, {:.3} s", r.triggers.len(), r.length);
        // openmpt123 renders 0.1 s past the song's end
        assert!((r.length + 0.1 - secs).abs() < 1.0 / 44100.0, "{f}: {} s vs openmpt {secs} s", r.length);
    }
}

/// Every generated-sample trigger of the three songs unrolls into notes (opt-in): the playback ratio of a trigger at
/// the phrase's native note is 1 (within the XM finetune's resolution), notes alternate on/off, and the ping-pong
/// seconds, track counts and bend warnings per song are printed.
#[test]
fn the_tbl_songs_unroll() {
    use synth_convert::cauldron::{self, import, PEv};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../research/tbl-mini-cauldron");
    let out = root.join("out");
    if !out.join("listen/jizz.xm").exists() {
        eprintln!("skip: {} absent", out.display());
        return;
    }
    for (f, patches, call) in [("jizz", "patches_jizz.json", "jizz"), ("stash_song1", "patches_stash.json", "song0"), ("stash_bluish", "patches_stash.json", "bluish")] {
        let p = cauldron::load(&std::fs::read_to_string(root.join("ref").join(patches)).unwrap()).unwrap();
        let call = p.call(call).unwrap();
        let x = synth_convert::xm::parse(&std::fs::read(out.join(format!("listen/{f}.xm"))).unwrap()).unwrap();
        let ids = import::identify(call, &x).unwrap();
        let r = synth_convert::xm::replay::replay(&x, 44100).unwrap();
        let (mut n, mut notes, mut back, mut ratios) = (0, 0, 0.0, vec![]);
        let mut spans = vec![(0.0, 0.0); r.triggers.len()];
        for (ti, t) in r.triggers.iter().enumerate() {
            let Some((record, variant)) = ids[t.instrument][t.sample] else { continue };
            let smp = &x.instruments[t.instrument].samples[t.sample];
            let rec = &call.records[record];
            let decim = rec.decimation(call.version);
            let l = import::Loop { len: smp.data.len() as u32, start: smp.loop_start, length: smp.loop_len, kind: smp.loop_kind };
            let ratio = t.ticks[0].freq * decim as f64 / cauldron::R;
            ratios.push(ratio);
            let path = import::path(&t.ticks, t.offset, &l, decim, 44100.0);
            spans[ti] = (t.frame as f64, path.end);
            let ev = import::unroll(call, record, variant, ratio, &path);
            let mut held = false;
            for e in &ev {
                match e.what {
                    PEv::On { .. } => {
                        assert!(!held, "{f}: trigger at {} s", t.at);
                        held = true;
                        notes += 1;
                    }
                    PEv::Off => held = false,
                    _ => {}
                }
                assert!(e.at >= t.frame as f64 - 1e-6 && e.at <= path.end + 1e-6, "{f}: event outside its trigger");
            }
            assert!(!held);
            n += 1;
            back += path.backward / 44100.0;
        }
        // the semitone classes of the triggers' ratios
        let mut classes: Vec<i32> = ratios.iter().map(|r| (12.0 * r.log2()).round() as i32).collect();
        classes.sort();
        classes.dedup();
        let off = ratios.iter().map(|r| (12.0 * r.log2() - (12.0 * r.log2()).round()).abs()).fold(0.0, f64::max);
        assert!(ratios.iter().any(|r| (r - 1.0).abs() < 1e-9), "{f}: no trigger at the native ratio 1");
        let dec = |rec: usize| call.records[rec].decimation(call.version);
        let tracks = import::group(&r, &ids, dec, |i| spans[i]);
        assert_eq!(tracks.iter().map(|t| t.triggers.len()).sum::<usize>(), n, "{f}: every phrase trigger on one track");
        let (mut warn, mut range) = (0, 0.0f64);
        for t in &tracks {
            let c = import::controls(&r, t, dec, |i| spans[i].1);
            warn += c.warnings.len();
            range = range.max(c.range);
        }
        let mut keys: Vec<(usize, i32)> = tracks.iter().map(|t| (t.record, t.class)).collect(); keys.sort(); keys.dedup();
        let conc: usize = keys.iter().map(|k| {
            let mut ev: Vec<(f64, i32)> = tracks.iter().filter(|t| (t.record, t.class) == *k).flat_map(|t| t.triggers.iter().flat_map(|&i| [(spans[i].0, 1), (spans[i].1, -1)])).collect();
            ev.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            ev.iter().scan(0, |c, e| { *c += e.1; Some(*c) }).max().unwrap_or(0) as usize
        }).sum();
        // the merge reaches the lower bound: per (record, class), the most triggers sounding at once
        assert_eq!(tracks.len(), conc, "{f}");
        eprintln!("{f}: {} tracks ({} (record, class) pairs, {} records), widest bend ±{range:.2}, {warn} bend warnings", tracks.len(), keys.len(), { let mut v: Vec<usize> = tracks.iter().map(|t| t.record).collect(); v.sort(); v.dedup(); v.len() });
        eprintln!("{f}: {n} phrase triggers, {notes} notes, {back:.1} s ping-pong backward, semitone classes {classes:?}, worst off-semitone {off:.3}");
    }
}

/// Every stored sample of the three songs (opt-in) as a sampler instrument plays its GSM sample at the XM frequency:
/// C-4 rendered against the decoded GSM read at `freq/44100` per frame. The codec's own loss (waveform correlation with
/// the XM's PCM) is printed per sample.
#[test]
fn the_tbl_stored_samples_play() {
    use synth_convert::cauldron::{self, import};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../research/tbl-mini-cauldron");
    if !root.join("out/listen/jizz.xm").exists() {
        eprintln!("skip: reference material absent");
        return;
    }
    let corr = |a: &[f64], b: &[f64]| {
        let (mut xy, mut xx, mut yy) = (0.0, 0.0, 0.0);
        for (x, y) in a.iter().zip(b) {
            xy += x * y;
            xx += x * x;
            yy += y * y;
        }
        xy / (xx * yy).sqrt().max(1e-12)
    };
    for (f, patches, call) in [("jizz", "patches_jizz.json", "jizz"), ("stash_song1", "patches_stash.json", "song0"), ("stash_bluish", "patches_stash.json", "bluish")] {
        let p = cauldron::load(&std::fs::read_to_string(root.join("ref").join(patches)).unwrap()).unwrap();
        let x = synth_convert::xm::parse(&std::fs::read(root.join(format!("out/listen/{f}.xm"))).unwrap()).unwrap();
        let ids = import::identify(p.call(call).unwrap(), &x).unwrap();
        let (mut names, mut bytes, mut pcm) = (vec![], 0, 0);
        for (ii, ins) in x.instruments.iter().enumerate() {
            for (si, s) in ins.samples.iter().enumerate() {
                if ids[ii][si].is_some() || s.data.is_empty() {
                    continue;
                }
                let st = import::sampler(s, 44100);
                let g = gsm::GsmChunk::parse(&st.gsm).unwrap().unwrap();
                let mut dec = vec![0.0f32; g.samples];
                gsm::decode(g.blocks, &mut dec).unwrap();
                let dec: Vec<f64> = dec.iter().map(|&v| v as f64).collect();
                let orig: Vec<f64> = s.data.iter().map(|&v| v as f64 / 32768.0).collect();
                let lane = vec![(0, st.note(49), Some(100)), (20000, st.note(49), None)];
                let dev = Dev { kind: unisynth::kind::INSTRUMENT, chunk: unisynth::chunk::encode(&st.values, &[&st.gsm]) };
                let out = loads_and_renders(&song(vec![dev], vec![lane], vec![track(&[0], 0)]).write());
                let mono: Vec<f64> = out.chunks(2).map(|c| c[0] as f64).collect();
                let step = st.up as f64 * 8363.0 * 2f64.powf((s.relative as f64 + s.finetune as f64 / 128.0) / 12.0) / 44100.0;
                let at = |pos: f64| {
                    let (i, fr) = (pos as usize, pos.fract());
                    let g = |k: usize| dec.get(k).copied().unwrap_or(0.0);
                    g(i) * (1.0 - fr) + g(i + 1) * fr
                };
                let m = 4000.min((dec.len() as f64 / step) as usize);
                let want: Vec<f64> = (0..m).map(|i| at(i as f64 * step)).collect();
                let best = (0..64).map(|lag| corr(&mono[lag..lag + m], &want)).fold(f64::MIN, f64::max);
                assert!(best > 0.95, "{f} {}: correlation {best:.3}", s.name.trim());
                let orig_at: Vec<f64> = (0..m).map(|i| { let pos = i as f64 * step / st.up as f64; let (k, fr) = (pos as usize, pos.fract()); let g = |k: usize| orig.get(k).copied().unwrap_or(0.0); g(k) * (1.0 - fr) + g(k + 1) * fr }).collect();
                let e = |v: &[f64]| v.iter().map(|x| x * x).sum::<f64>();
                eprintln!("  {} level ours/orig {:+.2} dB", s.name.trim(), 10.0 * (e(&mono[..m]) / e(&orig_at)).log10());
                // the codec's loss: the decode taken back to the XM's rate (every `up`-th sample) against its PCM
                let back: Vec<f64> = (0..orig.len()).map(|i| dec[i * st.up]).collect();
                names.push(format!("{} ×{} {:.2}", s.name.trim(), st.up, corr(&back, &orig)));
                bytes += st.gsm.len();
                pcm += s.data.len();
            }
        }
        // spec "Drum kit": Stash's stored samples are exactly its drum kit (the generated ones carry no data)
        let call = p.call(call).unwrap();
        if call.version == cauldron::Version::V2 {
            assert_eq!(names.len(), call.drum_count, "{f}");
            assert_eq!(pcm, p.drum_lengths[..call.drum_count].iter().sum::<usize>(), "{f}");
        }
        eprintln!("{f}: {} stored samples, {pcm} samples as {bytes} GSM bytes; codec correlation: {}", names.len(), names.join(", "));
    }
}

/// Jizz's XM with the first effect of pattern 0 replaced by `7` (tremolo, which the replay does not implement).
fn with_tremolo(xm: &[u8]) -> Vec<u8> {
    let mut b = xm.to_vec();
    let u32_at = |b: &[u8], p: usize| u32::from_le_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]]) as usize;
    let p0 = 60 + u32_at(&b, 60); // pattern 0's header
    let (mut p, end) = (p0 + u32_at(&b, p0), p0 + u32_at(&b, p0) + u16::from_le_bytes([b[p0 + 7], b[p0 + 8]]) as usize);
    while p < end {
        let f = b[p];
        let bits = if f & 0x80 != 0 { f } else { 0x1F };
        let mut q = p + (f & 0x80 != 0) as usize;
        for bit in [1, 2, 4] {
            q += (bits & bit != 0) as usize;
        }
        if bits & 8 != 0 && b[q] != 0 {
            b[q] = 7;
            return b;
        }
        p = q + (bits & 8 != 0) as usize + (bits & 16 != 0) as usize;
    }
    panic!("pattern 0 has no effect");
}

/// `synth-convert xm` (opt-in, task 6.1): converts Jizz to a song that loads and holds no `Probe` sample data; an
/// unsupported effect is an error naming it, exits non-zero and writes nothing. Compact by default (change
/// compact-cauldron-xm-import): layers dropped, fewer tracks than `--keep-layers --split-pitches`.
#[test]
fn the_xm_cli_converts_and_writes_nothing_on_error() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../research/tbl-mini-cauldron");
    let xm = root.join("out/listen/jizz.xm");
    if !xm.exists() {
        eprintln!("skip: reference material absent");
        return;
    }
    let dir = std::env::temp_dir().join(format!("xm-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let run_with = |input: &std::path::Path, out: &std::path::Path, opts: &[&str]| std::process::Command::new(env!("CARGO_BIN_EXE_synth-convert")).arg("xm").arg(input).arg(root.join("ref/patches_jizz.json")).arg(out).args(opts).output().unwrap();
    let run = |input: &std::path::Path, out: &std::path::Path| run_with(input, out, &[]);
    // the layered layout (the parity's)
    let layered = dir.join("layered.song");
    let o = run_with(&xm, &layered, &["--keep-layers", "--split-pitches"]);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(o.status.success() && err.contains("tracks 58 phrase, 13 stored") && err.contains("layers 0 dropped"), "{err}");
    let out = dir.join("jizz.song");
    let o = run(&xm, &out);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(o.status.success(), "{err}");
    assert!(err.contains("lead-frames") && err.contains("tracks 13 phrase") && err.contains("layers 591 dropped"), "{err}");
    assert!(std::fs::metadata(&out).unwrap().len() < std::fs::metadata(&layered).unwrap().len());
    let blob = std::fs::read(&out).unwrap();
    loads_and_renders(&blob);
    // no generated sample data: the only samples are the stored ones, one per stored-sample track (11)
    let song = synth_convert::model::SongM::read(&blob).unwrap();
    let samples: usize = song.devices.iter().filter(|d| d.kind == unisynth::kind::INSTRUMENT).map(|d| unisynth::chunk::Chunk::parse(&d.chunk).unwrap().sample_count()).sum();
    assert!(err.contains("11 stored"), "{err}");
    assert_eq!(samples, 11);
    // the dropped layers' echo and room as effects on the tracks (no extra tracks); none with --dry
    let fx = |s: &synth_convert::model::SongM, k: u8| s.devices.iter().filter(|d| d.kind == k).count();
    assert!(err.contains("ambience ") && fx(&song, unisynth::kind::REVERB) > 0, "{err}");
    let dry = dir.join("dry.song");
    let o = run_with(&xm, &dry, &["--dry"]);
    let dry = synth_convert::model::SongM::read(&std::fs::read(&dry).unwrap()).unwrap();
    assert!(o.status.success() && fx(&dry, unisynth::kind::REVERB) == 0 && dry.tracks.len() == song.tracks.len());
    assert!(fx(&dry, unisynth::kind::DELAY) < fx(&song, unisynth::kind::DELAY));
    // an unsupported effect
    let bad = dir.join("bad.xm");
    std::fs::write(&bad, with_tremolo(&std::fs::read(&xm).unwrap())).unwrap();
    let out2 = dir.join("bad.song");
    let o = run(&bad, &out2);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(!o.status.success());
    assert!(err.contains("effect 7") && err.contains("pattern 0") && err.contains("row") && err.contains("channel"), "{err}");
    assert!(!out2.exists(), "nothing written on error");
    std::fs::remove_dir_all(&dir).unwrap();
}
