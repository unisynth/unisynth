//! The XM conversion flow shared by every sample source (change add-plain-xm-import, design D2): the replay at the
//! control rate, trigger paths, dropped layers and handovers, track packing (a pitch class per track, the playback
//! speed as transpose + bend lanes), volume/pan/bend lanes, phrase tracks from the source's instrument, chain and
//! events, stored-sample tracks, lead, length, master and names. What a source supplies is the [`Source`] trait: the
//! mini-cauldron (`cauldron::import::CauldronSource`) and the generator map (`xmgen`). The flow's helpers live in
//! `cauldron::import`, where they began.

use crate::cauldron::import::{track_names, Converted, Options, Path};
use crate::cauldron::{Chain, PhraseEv};
use crate::xm::Xm;
use std::collections::BTreeMap;

/// What the flow asks a sample source.
pub trait Source {
    /// Per XM instrument and sample: a generated sample's `(key, variant)`, or `None` (stored, or empty).
    fn ids(&self, xm: &Xm) -> Result<Vec<Vec<Option<(usize, usize)>>>, String>;
    /// A key's frames per phrase frame (its paths run in phrase frames).
    fn decimation(&self, key: usize) -> u32;
    /// The echo a key's chain carries at a playback ratio (ms), if any: the song tempo fits the longest.
    fn echo_ms(&self, key: usize, ratio: f64) -> Option<f64>;
    /// A key's effect chain at a playback ratio (`slides`: its triggers slide past a semitone; `exact`: keep every
    /// stage).
    fn chain(&self, key: usize, ratio: f64, rate: f64, tempo: i32, slides: bool, exact: bool) -> Chain;
    /// A key's instrument values at a playback ratio (the track's class: sample frames per second over
    /// `cauldron::R` × the decimation).
    fn instrument(&self, key: usize, ratio: f64) -> [f32; unisynth::params::old::COUNT];
    /// A trigger's notes along its path, in output frames from the trigger.
    fn events(&self, key: usize, variant: usize, ratio: f64, path: &Path) -> Vec<PhraseEv>;
    /// A key's track gain factor.
    fn gain(&self, _key: usize) -> f64 {
        1.0
    }
    /// The note lane and the instrument's per-note byte lanes for a track's events at song frames.
    fn lanes(&self, _key: usize, evs: Vec<(i64, crate::cauldron::PEv)>, values: &mut [f32; unisynth::params::old::COUNT]) -> Result<crate::xmgen::Lanes, String> {
        Ok(crate::cauldron::note_lanes(evs.into_iter(), values))
    }
    /// A track's name before numbering (`key` None: a stored sample's track; `instrument`, `sample` 0-based): the
    /// mini-cauldron's `<sample> (record <r>[, <class>])` and `<sample> (sample)` ([`track_names`]).
    fn track_name(&self, key: Option<usize>, class: i32, ins: &crate::xm::Instrument, _instrument: usize, sample: usize) -> String {
        track_names(&[(key, ins.samples[sample].name.clone(), class)]).remove(0)
    }
    /// A ping-pong backward pass of this key plays its events in reverse order (warned once per sample).
    fn reverses(&self, _key: usize) -> bool {
        true
    }
}

pub fn convert<S: Source>(src: &S, xm: &Xm, rate: i32, opts: Options) -> Result<Converted, String> {
    let thin = opts.thin;
    use crate::model::{Auto, Dev, SongM, Trk};
    use crate::xm::replay::replay;
    use crate::cauldron::import::*;
    use crate::cauldron::{byte_lane, lowpass_p, thin_lane, Chain, PEv, LANE_LEAD};
    use unisynth::law::host as inv;
    use unisynth::params::old::{ctl, osc, shape as sp, v};
    let ids = src.ids(xm)?;
    let mut r = replay(xm, rate as u32)?;
    hold_controls(&mut r, opts.controls);
    let dec = |key: usize| src.decimation(key);
    let sample = |t: &crate::xm::replay::Trigger| &xm.instruments[t.instrument].samples[t.sample];
    let loop_of = |s: &crate::xm::Sample| Loop { len: s.data.len() as u32, start: s.loop_start, length: s.loop_len, kind: s.loop_kind };
    // every trigger's path (phrases: in phrase frames; stored samples: only its end matters)
    let paths: Vec<Path> = r
        .triggers
        .iter()
        .map(|t| {
            let d = ids[t.instrument][t.sample].map_or(1, |(rec, _)| dec(rec));
            path(&t.ticks, t.offset, &loop_of(sample(t)), d, rate as f64)
        })
        .collect();
    // the stereo and echo layers dropped (compact::layers): kept triggers and their paths
    // what each kept trigger absorbed (its track's ambience)
    let mut dropped = (0, 0);
    let (r, paths, absorbed) = if opts.keep_layers {
        let n = r.triggers.len();
        (r, paths, vec![compact::Absorbed::default(); n])
    } else {
        let l = compact::layers(&mut r, |i| paths[i].end, rate as f64);
        dropped = (l.same_tick, l.echoes);
        let triggers = l.keep.iter().map(|&i| r.triggers[i].clone()).collect();
        let paths = l.keep.iter().map(|&i| paths[i].clone()).collect();
        (crate::xm::replay::Replay { triggers, length: r.length }, paths, l.absorbed)
    };
    // a slide past one track's reach hands over: its segments become triggers of their own (handovers, clip)
    let (r, paths, absorbed) = {
        let (mut trig, mut ps, mut ab) = (vec![], vec![], vec![]);
        for ((t, p), took) in r.triggers.iter().zip(paths).zip(absorbed) {
            let cuts = ids[t.instrument][t.sample].map_or(vec![], |(rec, _)| handovers(t, dec(rec), p.end));
            if cuts.is_empty() {
                trig.push(t.clone());
                ps.push(p);
                ab.push(took);
                continue;
            }
            let mut b = vec![0];
            b.extend(cuts);
            b.push(t.ticks.len());
            for w in b.windows(2) {
                let ticks = t.ticks[w[0]..w[1]].to_vec();
                let (a, z) = (ticks[0].frame as f64, t.ticks.get(w[1]).map_or(f64::MAX, |k| k.frame as f64));
                trig.push(crate::xm::replay::Trigger { tick: t.tick + w[0], frame: ticks[0].frame, at: ticks[0].at, ticks, ..t.clone() });
                ps.push(clip(&p, a, z));
                ab.push(took.clone());
            }
        }
        (crate::xm::replay::Replay { triggers: trig, length: r.length }, ps, ab)
    };
    let span = |i: usize| (r.triggers[i].frame as f64, paths[i].end);
    let plans = if opts.split_pitches { group(&r, &ids, dec, span) } else { compact::pack(&r, &ids, dec, span) };
    let is_stored = |t: &crate::xm::replay::Trigger| ids[t.instrument][t.sample].is_none() && !sample(t).data.is_empty();
    let stored = if opts.split_pitches {
        let mut stored: BTreeMap<((usize, usize), usize), Vec<usize>> = BTreeMap::new();
        for (i, t) in r.triggers.iter().enumerate() {
            if is_stored(t) {
                stored.entry(((t.instrument, t.sample), t.channel)).or_default().push(i);
            }
        }
        merge(&r, stored, span)
    } else {
        compact::pack_by(&r, |i| Some(&r.triggers[i]).filter(|t| is_stored(t)).map(|t| (t.instrument, t.sample)), span)
    };
    // the dropped layers come back as effects (compact::ambience): phrase tracks a stereo delay and a reverb, stored
    // ones the delay alone
    let amb = |triggers: &[usize], reverb: bool| {
        if opts.dry {
            return compact::Ambience::default();
        }
        let a = compact::ambience(&triggers.iter().map(|&i| &absorbed[i]).collect::<Vec<_>>());
        compact::Ambience { reverb: a.reverb && reverb, ..a }
    };
    let plan_amb: Vec<compact::Ambience> = plans.iter().map(|t| amb(&t.triggers, true)).collect();
    let stored_amb: Vec<compact::Ambience> = stored.iter().map(|(_, t)| amb(t, false)).collect();
    let longest = plans.iter().filter_map(|t| src.echo_ms(t.record, t.ratio())).filter(|&ms| ms <= ECHO_MAX_MS).fold(0.0, f64::max);
    let longest = plan_amb.iter().chain(&stored_amb).map(|a| a.longest_ms()).filter(|&ms| ms <= ECHO_MAX_MS).fold(longest, f64::max);
    let tempo = tempo_for(longest);
    // a track whose triggers slide more than a semitone gets the band-limiting low-pass (slide_lowpass)
    let slides = |t: &TrackPlan| t.triggers.iter().any(|&i| {
        let tr = &r.triggers[i];
        let f: Vec<f64> = tr.ticks.iter().take_while(|k| (k.frame as f64) < paths[i].end).filter(|k| sounds(k)).map(|k| k.freq).collect();
        f.iter().zip(f.iter().skip(1)).any(|_| true) && {
            let (lo, hi) = f.iter().fold((f64::MAX, 0.0f64), |(a, b), &x| (a.min(x), b.max(x)));
            12.0 * (hi / lo).log2() > 1.0
        }
    });
    let chains: Vec<Chain> = plans.iter().map(|t| src.chain(t.record, t.ratio(), rate as f64, tempo, slides(t), opts.exact_chain)).collect();
    let lead = chains.iter().map(|c| c.latency as i64 + POST_DELAY).max().unwrap_or(0) + 2 * LANE_LEAD;
    let mut warnings = vec![];
    let mut song = SongM { tempo, rate, length: 0.0, devices: vec![], lanes: vec![], tracks: vec![] };
    // gain, pan and bend as the instrument's automation, a lane lead early: the voice ramps a parameter over the
    // sub-block after the player's step, so a gain stepping up at a note-on must land a sub-block before it (centred
    // on the step, a drum hit's first 100 frames rose from the previous hit's tail: −3 dB)
    // `post`: the track's post-chain gain device (phrases): the voice takes `master`, the device `post`
    let levels = |c: &Controls, latency: i64, values: &mut [f32; unisynth::params::old::COUNT], autos: &mut Vec<Auto>, post: Option<usize>| {
        // the shift as transpose + bend (split_shift); a track within ±MAX_BEND keeps bend alone
        let lane_t = c.bend.iter().any(|&(_, b)| split_shift(b, false).0 != 0.0);
        let parts: Vec<(u64, (f64, f64))> = c.bend.iter().map(|&(f, b)| (f, split_shift(b, lane_t))).collect();
        let (trans, bends): (Vec<_>, Vec<_>) = parts.iter().map(|&(f, (t, b))| ((f, t), (f, b))).unzip();
        let range = bends.iter().fold(0.0f64, |m, x| m.max(x.1.abs()));
        // the stored range is read back through its law: bend to it
        let back = if range > 0.0 {
            values[ctl::BEND_RANGE] = inv::inv_bend_range(range.max(1e-3) as f32);
            unisynth::law::bend_range(values[ctl::BEND_RANGE]) as f64
        } else {
            0.0
        };
        let at = |f: u64| lead - latency + f as i64 - LANE_LEAD;
        let byte = |p: f32| (p.clamp(0.0, 1.0) * 255.0).round() as u8;
        // a transpose change re-centres the bend: there the bend steps exactly, with the transpose (never thinned)
        let breaks: Vec<i64> = trans.windows(2).filter(|w| w[0].1 != w[1].1).map(|w| at(w[1].0)).collect();
        let mut lane = |param: usize, pts: &[(u64, f64)], to_p: &dyn Fn(f64) -> f32, tol: u8| {
            let ch: Vec<(i64, u8)> = pts.iter().map(|&(f, x)| (at(f), byte(to_p(x)))).collect();
            let b0 = ch.first().map_or(byte(values[param]), |x| x.1);
            values[param] = b0 as f32 / 255.0;
            if ch.iter().any(|x| x.1 != b0) {
                autos.push(Auto { device: 0, param, points: thin_lane(b0, &ch, tol, &breaks) });
            }
        };
        lane(v::MASTER, if post.is_some() { &c.master } else { &c.gain }, &|g| inv::inv_gain(g as f32), thin);
        // a phrase voice plays centred: its pan is the post device's
        let centre = [(0u64, 0.5f64)];
        lane(v::PAN, if post.is_some() { &centre } else { &c.pan }, &|p| p as f32, thin);
        if back > 0.0 {
            lane(ctl::BEND, &bends, &|b| (0.5 + b / (2.0 * back)) as f32, thin);
        }
        if lane_t {
            lane(v::TRANSPOSE, &trans, &|x| inv::inv_transpose(x as f32), 0);
        }
        if let Some(d) = post {
            // EQ 3band, all three bands at the gain (linear below 0.5): flat, 3 frames late; it sees output frame
            // `lead + f` at `lead + f − 3`
            let ch: Vec<(i64, u8)> = c.post.iter().map(|&(f, g)| (lead + f - POST_DELAY, byte(post_value(g)))).collect();
            let b0 = ch.first().map_or(0, |x| x.1);
            let pts = thin_lane(b0, &ch, thin, &[]);
            for param in 1..=3 {
                autos.push(Auto { device: d, param, points: pts.clone() });
            }
            // its pan: a byte 1..255, hard left to hard right
            let ch: Vec<(i64, u8)> = c.post_pan.iter().map(|&(f, p)| (lead + f - POST_DELAY, 1 + (p.clamp(0.0, 1.0) * 254.0).round() as u8)).collect();
            let b0 = ch.first().map_or(128, |x| x.1);
            autos.push(Auto { device: d, param: 4, points: thin_lane(b0, &ch, thin, &[]) });
        }
    };
    let (mut pingpong, mut muted) = (0.0, 0.0);
    // openmpt123 renders 0.1 s past the end
    let length = (lead as f64 + (r.length + 0.1) * rate as f64) / rate as f64;
    let mut idle_clips = 0;
    // phrase tracks as built: devices, automations, lane, volume, clips to check
    let mut built: Vec<(Vec<Dev>, Vec<Auto>, Vec<crate::model::Ev>, f32, Vec<(usize, f64)>)> = vec![];
    let mut backward_samples: Vec<(usize, usize)> = vec![];
    for ((plan, chain), ambience) in plans.iter().zip(&chains).zip(&plan_amb) {
        if let Some(ms) = chain.echo_clamped {
            warnings.push(format!("record {} at {:+} semitones: echo of {ms:.0} ms clamped to the delay's longest", plan.record, plan.class));
        }
        let mut values = src.instrument(plan.record, plan.ratio());
        let latency = chain.latency as i64 + POST_DELAY;
        let mut evs: Vec<(i64, PEv)> = vec![];
        let mut restarts = vec![];
        for &i in &plan.triggers {
            let t = &r.triggers[i];
            let (_, variant) = ids[t.instrument][t.sample].unwrap();
            let pth = &paths[i];
            if pth.backward > 0.0 && src.reverses(plan.record) {
                pingpong += pth.backward / rate as f64;
                if !backward_samples.contains(&(t.instrument, t.sample)) {
                    backward_samples.push((t.instrument, t.sample));
                    warnings.push(format!("sample `{}` (instrument {}) plays past its end in a ping-pong loop: its steps play in reverse order", sample(t).name.trim(), t.instrument + 1));
                }
            }
            evs.extend(src.events(plan.record, variant, plan.ratio(), pth).into_iter().map(|e| (lead - latency + e.at.round() as i64, e.what)));
            restarts.push((lead + t.frame as i64 - POST_DELAY, if restarts.len() % 2 == 0 { 255 } else { 0 }));
        }
        let (lane, mut autos) = src.lanes(plan.record, evs, &mut values)?;
        let c = controls(&r, plan, dec, |i| paths[i].end);
        muted += c.muted;
        warnings.extend(c.warnings.iter().cloned());
        levels(&c, latency, &mut values, &mut autos, Some(1 + chain.devices.len()));
        // the decimation low-pass follows the slide: the stored Nyquist times the playing ratio (it sits at the
        // chain's end, before the post gain)
        if let Some((k, nyq)) = chain.lowpass {
            let open = |hz: f64| if hz < 0.45 * rate as f64 { lowpass_p(hz) } else { 1.0 };
            let byte = |p: f32| (p.clamp(0.0, 1.0) * 255.0).round() as u8;
            let ch: Vec<(i64, u8)> = c.bend.iter().map(|&(f, b)| (lead + f as i64 - LANE_LEAD - POST_DELAY, byte(open(nyq * plan.ratio() * 2f64.powf(b / 12.0))))).collect();
            if ch.iter().any(|x| x.1 != ch[0].1) {
                for s in 0..chain.lp_stages {
                    autos.push(Auto { device: 1 + k + s, param: 6, points: thin_lane(ch[0].1, &ch, thin, &[]) });
                }
            }
        }
        if let Some(k) = chain.restart {
            autos.push(Auto { device: 1 + k, param: 4, points: byte_lane(0, &restarts) });
        }
        let mut devs = vec![Dev { kind: unisynth::kind::INSTRUMENT, chunk: unisynth::migrate::encode_old(&values, &[], &[]) }];
        devs.extend(chain.devices.iter().map(|(k, v)| Dev { kind: *k, chunk: unisynth::migrate::encode_old_fx(v) }));
        devs.push(Dev { kind: unisynth::kind::EQ, chunk: unisynth::migrate::encode_old_fx(&post_gain()) });
        let (extra, make_up) = ambience.devices(tempo);
        devs.extend(extra.iter().map(|(k, v)| Dev { kind: *k, chunk: unisynth::migrate::encode_old_fx(v) }));
        let halve = src.gain(plan.record);
        let clips = if opts.exact_chain { vec![] } else { compact::clips(chain) };
        built.push((devs, autos, lane, (XM_GAIN * halve / chain.level * make_up) as f32, clips));
    }
    // the clips each track's notes never reach (compact::idle_clips: a solo render per clip, tracks in parallel),
    // left out last first so the indices hold
    let idle: Vec<Vec<usize>> = {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        let next = std::sync::atomic::AtomicUsize::new(0);
        let found = std::sync::Mutex::new(vec![vec![]; built.len()]);
        std::thread::scope(|sc| {
            for _ in 0..threads.min(built.len()) {
                sc.spawn(|| loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((devs, autos, lane, _, clips)) = built.get(i) else { break };
                    let idle = if clips.is_empty() { vec![] } else { compact::idle_clips(devs, autos, lane, clips, tempo, rate, length) };
                    found.lock().unwrap()[i] = idle;
                });
            }
        });
        found.into_inner().unwrap()
    };
    for ((mut devs, mut autos, lane, volume, _), idle) in built.into_iter().zip(idle) {
        for &i in idle.iter().rev() {
            compact::remove_device(&mut devs, &mut autos, i);
        }
        idle_clips += idle.len();
        let base = song.devices.len();
        song.devices.extend(devs);
        let devices = (base..song.devices.len()).collect();
        song.tracks.push(Trk { volume, receives: vec![], devices, lane: song.lanes.len(), autos });
        song.lanes.push(lane);
    }
    for (((ins, smp), trig), ambience) in stored.iter().zip(&stored_amb) {
        let s = &xm.instruments[*ins].samples[*smp];
        let st = sampler(s, rate as u32);
        let mut values = st.values;
        let (mut lane, mut starts) = (vec![], vec![]);
        let mut t0 = 0i64;
        for &i in trig {
            let t = &r.triggers[i];
            let (on, off) = (lead + t.frame as i64, lead + paths[i].end.round() as i64);
            let note = st.note(t.note);
            lane.push(((on - t0) as i32, note, Some(127)));
            lane.push(((off - on) as i32, note, None));
            t0 = off;
            starts.push((on - LANE_LEAD, ((t.offset as f64 / s.data.len().max(1) as f64) * 255.0).round().min(255.0) as u8));
        }
        let freq = |i: usize| {
            let t = &r.triggers[i];
            8363.0 * 2f64.powf((t.note as f64 - 49.0 + s.relative as f64 + s.finetune as f64 / 128.0) / 12.0)
        };
        let c = controls_of(&r, trig, &format!("sample `{}`", s.name.trim()), freq, |i| paths[i].end);
        muted += c.muted;
        warnings.extend(c.warnings.iter().cloned());
        let mut autos = vec![];
        levels(&c, 0, &mut values, &mut autos, None);
        if starts.iter().any(|x| x.1 != 0) {
            let id = osc(0, unisynth::params::old::o::SHAPE + sp::SMP_START);
            values[id] = starts[0].1 as f32 / 255.0;
            autos.push(Auto { device: 0, param: id, points: byte_lane(starts[0].1, &starts) });
        }
        let base = song.devices.len();
        song.devices.push(Dev { kind: unisynth::kind::INSTRUMENT, chunk: unisynth::migrate::encode_old(&values, &[&st.gsm], &[]) });
        let (extra, make_up) = ambience.devices(tempo);
        song.devices.extend(extra.iter().map(|(k, v)| Dev { kind: *k, chunk: unisynth::migrate::encode_old_fx(v) }));
        let devices = (base..song.devices.len()).collect();
        song.tracks.push(Trk { volume: (XM_GAIN * make_up) as f32, receives: vec![], devices, lane: song.lanes.len(), autos });
        song.lanes.push(lane);
    }
    // the master: every track summed, no devices
    let n = song.tracks.len();
    song.tracks.push(Trk { volume: 1.0, receives: (0..n).map(|t| (t, 0, 1.0)).collect(), devices: vec![], lane: song.lanes.len(), autos: vec![] });
    song.lanes.push(vec![]);
    song.length = length;
    let name = |key: Option<usize>, i: usize, class: i32| {
        let t = &r.triggers[i];
        src.track_name(key, class, &xm.instruments[t.instrument], t.instrument, t.sample)
    };
    let names = number_names(plans.iter().map(|t| name(Some(t.record), t.triggers[0], t.class)).chain(stored.iter().map(|(_, trig)| name(None, trig[0], 0))).collect());
    Ok(Converted { song, lead: lead as usize, warnings, pingpong, muted, tracks: (plans.len(), stored.len()), dropped, idle_clips,
        ambience: (plan_amb.iter().chain(&stored_amb).filter(|a| a.echo.is_some()).count(), plan_amb.iter().chain(&stored_amb).filter(|a| a.reverb).count()),
        records: plans.iter().map(|t| Some(t.record)).chain(stored.iter().map(|_| None)).collect(), names })
}
