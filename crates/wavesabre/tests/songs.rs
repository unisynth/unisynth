//! Synthetic WaveSabre song blobs for the golden test: every device kind, sends, sidechain, automation, unison,
//! mono legato. Written with upstream's blob layout (`WaveSabreConvert` `Serializer.CreateBinary`).

use wavesabre::kind;

#[derive(Default)]
struct W(Vec<u8>);
impl W {
    fn i(&mut self, v: i32) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn f(&mut self, v: f32) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn b(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
}

/// `params` as an upstream `Device` chunk (f32s + trailing size).
pub fn chunk(params: &[f32]) -> Vec<u8> {
    let mut c: Vec<u8> = params.iter().flat_map(|p| p.to_le_bytes()).collect();
    c.extend((c.len() as i32 + 4).to_le_bytes());
    c
}

/// Specimen/Thunder chunk around the checked-in libgsm vectors.
pub fn gsm_chunk(params: &[f32]) -> Vec<u8> {
    let blocks: &[u8] = include_bytes!("../src/testdata/gsm_vectors.gsm");
    let mut c = vec![];
    c.extend((blocks.len() as i32).to_le_bytes());
    c.extend((blocks.len() as i32 / 65 * 320).to_le_bytes());
    for v in [0x31u16, 1] {
        c.extend(v.to_le_bytes());
    }
    c.extend(44100u32.to_le_bytes());
    c.extend(8957u32.to_le_bytes());
    for v in [65u16, 0, 2, 320] {
        c.extend(v.to_le_bytes());
    }
    c.extend(blocks);
    c.extend(params.iter().flat_map(|p| p.to_le_bytes()));
    c.extend((c.len() as i32 + 4).to_le_bytes());
    c
}

pub struct Track {
    pub volume: f32,
    /// `(sending track, channel 0 main / 2 sidechain, volume)`.
    pub receives: Vec<(i32, i32, f32)>,
    pub devices: Vec<i32>,
    pub lane: i32,
    /// `(local device index, param, [(delta frames, byte)])`.
    pub automations: Vec<(i32, i32, Vec<(i32, u8)>)>,
}

impl Track {
    pub fn new(devices: &[i32], lane: i32) -> Self {
        Track { volume: 1.0, receives: vec![], devices: devices.to_vec(), lane, automations: vec![] }
    }
    pub fn recv(mut self, from: i32, ch: i32, vol: f32) -> Self {
        self.receives.push((from, ch, vol));
        self
    }
}

/// A song; the last track is the master.
pub fn song(length_s: f64, devices: &[(u8, Vec<u8>)], lanes: &[Vec<(i32, u8, Option<u8>)>], tracks: &[Track]) -> Vec<u8> {
    let mut w = W::default();
    w.i(120).i(44100);
    w.0.extend(length_s.to_le_bytes());
    w.i(devices.len() as i32);
    for (k, c) in devices {
        w.b(*k).i(c.len() as i32);
        w.0.extend(c);
    }
    w.i(lanes.len() as i32);
    for lane in lanes {
        w.i(lane.len() as i32);
        for &(delta, note, vel) in lane {
            w.i(delta);
            match vel {
                Some(v) => w.b(note).b(v),
                None => w.b(note | 0x80),
            };
        }
    }
    w.i(tracks.len() as i32);
    for t in tracks {
        w.f(t.volume).i(t.receives.len() as i32);
        for &(from, ch, vol) in &t.receives {
            w.i(from).i(ch).f(vol);
        }
        w.i(t.devices.len() as i32);
        for &d in &t.devices {
            w.i(d);
        }
        w.i(t.lane).i(t.automations.len() as i32);
        for (dev, param, pts) in &t.automations {
            w.i(*dev).i(*param).i(pts.len() as i32);
            for &(delta, v) in pts {
                w.i(delta).b(v);
            }
        }
    }
    w.0
}

/// A short phrase with overlapping notes (polyphony) and a held chord.
pub fn melody() -> Vec<(i32, u8, Option<u8>)> {
    vec![
        (0, 48, Some(100)),
        (5512, 55, Some(90)),
        (5512, 48, None),
        (0, 60, Some(110)),
        (5512, 55, None),
        (5512, 60, None),
        (0, 64, Some(80)),
        (0, 67, Some(80)),
        (0, 72, Some(127)),
        (22050, 64, None),
        (0, 67, None),
        (0, 72, None),
    ]
}

/// Four-on-the-floor kick-ish hits (sidechain key).
pub fn hits() -> Vec<(i32, u8, Option<u8>)> {
    (0..4).flat_map(|i| [(if i == 0 { 0 } else { 11025 - 100 }, 36, Some(127)), (100, 36, None)]).collect()
}

/// Slaughter params: saw + detuned pulse, resonant LP, 3-voice unison.
fn slaughter_params() -> Vec<f32> {
    let mut p = vec![0.0f32; 42];
    p[2] = 0.8; // osc1 volume
    p[5] = 1.0; // osc2 pulse
    p[6] = 0.3; // osc2 width
    p[7] = 0.6;
    p[9] = 0.52; // osc2 fine
    p[15] = 0.1; // noise
    p[17] = 0.5; // cutoff
    p[18] = 0.4; // resonance
    p[19] = 0.7; // mod env → cutoff
    p[20] = 0.05;
    p[21] = 0.3;
    p[22] = 0.6;
    p[23] = 0.2;
    p[24] = 0.0;
    p[25] = 0.3;
    p[26] = 0.0;
    p[27] = 0.2;
    p[28] = 0.6;
    p[29] = 2.0 / 15.0; // 3 voices
    p[30] = 0.3;
    p[31] = 0.8;
    p[32] = 0.3; // vibrato
    p[33] = 0.1;
    p[35] = 0.0;
    p[36] = 0.2;
    p[39] = 0.6; // pitch env up
    p
}

/// Every device kind, mixer path and voice mode, one short song each: `(name, blob)`.
pub fn all() -> Vec<(&'static str, Vec<u8>)> {
    let sl = || (kind::SLAUGHTER, chunk(&slaughter_params()));
    let src = |devs: &[i32]| Track::new(devs, 0);
    let mut out = vec![];

    // instruments, each alone on the master
    let mut falcon = vec![0.0f32; 32];
    for (i, v) in [(1, 0.05), (2, 0.5), (3, 0.4), (4, 0.5), (5, 0.0), (6, 0.4), (7, 0.3), (8, 0.3), (10, 0.0)] {
        falcon[i] = v;
    }
    for (i, v) in [(11, 0.5), (13, 0.0), (14, 0.3), (15, 0.7), (16, 0.3), (17, 0.7), (20, 0.5), (28, 0.5), (29, 0.5)] {
        falcon[i] = v;
    }
    out.push(("falcon", song(2.0, &[(kind::FALCON, chunk(&falcon))], &[melody()], &[src(&[0])])));
    let mut t = src(&[0]);
    t.automations.push((0, 17, vec![(0, 20), (44100, 255), (22050, 60)]));
    out.push(("slaughter_automation", song(2.0, &[sl()], &[melody()], &[t])));
    let mut mono = slaughter_params();
    mono[29] = 0.0;
    mono[40] = 1.0;
    mono[41] = 0.3;
    out.push(("slaughter_mono_legato", song(2.0, &[(kind::SLAUGHTER, chunk(&mono))], &[melody()], &[src(&[0])])));
    out.push(("thunder", song(1.0, &[(kind::THUNDER, gsm_chunk(&[]))], &[hits()], &[src(&[0])])));
    let mut sp = vec![0.0f32; 26];
    for (i, v) in [(1, 0.3), (2, 0.7), (3, 0.2), (5, 0.5), (6, 0.25), (7, 0.25), (9, 1.0), (10, 0.5), (11, 0.6)] {
        sp[i] = v;
    }
    for (i, v) in [(13, 0.6), (14, 0.3), (15, 0.6), (17, 0.4), (18, 0.5), (20, 1.0 / 15.0), (21, 0.2), (22, 0.7)] {
        sp[i] = v;
    }
    sp[23] = 0.8;
    out.push(("specimen_loop", song(2.0, &[(kind::SPECIMEN, gsm_chunk(&sp))], &[melody()], &[src(&[0])])));
    let mut ad = vec![0.0f32; 28];
    ad[0] = 1.0;
    ad[25] = 0.8;
    out.push(("adultery_no_bank", song(0.5, &[(kind::ADULTERY, chunk(&ad))], &[melody()], &[src(&[0])])));

    // effects: Slaughter track → effect on the master (receive into main)
    let fx: [(&str, u8, Vec<f32>); 8] = [
        ("scissor", kind::SCISSOR, vec![0.7, 0.6, 1.0, 0.5, 0.8, 1.0]),
        ("leveller", kind::LEVELLER, vec![0.2, 0.5, 0.5, 0.3, 0.7, 0.5, 0.6, 0.2, 0.5, 0.8, 0.6, 0.5, 0.9, 0.5]),
        ("crusher", kind::CRUSHER, vec![0.6, 0.5, 0.7]),
        ("echo", kind::ECHO, vec![0.375, 0.25, 0.5, 0.5, 0.2, 0.3, 0.6, 0.5]),
        ("chamber", kind::CHAMBER, vec![0.5, 0.3, 0.6, 0.4, 0.2, 0.8, 0.5, 0.5, 0.4]),
        ("twister", kind::TWISTER, vec![0.0, 0.3, 0.5, 0.4, 0.2, 0.6, 0.5, 0.3, 0.8, 0.5]),
        ("twister_phaser", kind::TWISTER, vec![0.7, 0.3, 0.5, 0.4, 0.6, 0.6, 0.5, 0.3, 0.8, 0.5]),
        ("cathedral", kind::CATHEDRAL, vec![0.0, 0.7, 0.4, 0.8, 0.5, 0.3, 0.5, 0.8, 0.5]),
    ];
    for (name, k, params) in fx {
        let tracks = [src(&[0]), Track::new(&[1], 0).recv(0, 0, 0.9)];
        out.push((name, song(2.0, &[sl(), (k, chunk(&params))], &[melody()], &tracks)));
    }

    // Smasher keyed by a Thunder track through the sidechain; master also hears the key
    let tracks = [
        src(&[0]),
        Track::new(&[1], 1),
        Track::new(&[2], 0).recv(0, 0, 1.0).recv(1, 2, 1.0).recv(1, 0, 0.3),
    ];
    let smasher = vec![1.0, 0.3, 0.5, 0.6, 0.1, 0.3, 0.5];
    let devs = [sl(), (kind::THUNDER, gsm_chunk(&[])), (kind::SMASHER, chunk(&smasher))];
    out.push(("smasher_sidechain", song(2.0, &devs, &[melody(), hits()], &tracks)));
    out
}

/// Songs that need a GM.DLS bank (Adultery): rendered by the parity tool only when a bank is given.
pub fn bank_songs() -> Vec<(&'static str, Vec<u8>)> {
    let mut p = vec![0.0f32; 28];
    for (i, v) in [(0, 25.0), (1, 0.1), (2, 0.4), (3, 0.6), (4, 0.3), (11, 1.0), (12, 0.5), (13, 0.55)] {
        p[i] = v; // wave 24, envelopes, linear interpolation, +0.6 semitone fine
    }
    for (i, v) in [(15, 0.7), (16, 0.2), (17, 0.6), (19, 0.3), (20, 0.5), (22, 1.0 / 15.0), (23, 0.2), (24, 0.8), (25, 0.8)] {
        p[i] = v;
    }
    let mut ping = p.clone();
    ping[0] = 61.0;
    ping[6] = 1.0; // ping-pong
    ping[7] = 1.0; // manual bounds
    ping[8] = 0.3;
    ping[9] = 0.2;
    ping[10] = 1.0; // reverse
    let tracks = [Track::new(&[0], 0), Track::new(&[1], 0).recv(0, 0, 1.0)];
    vec![
        ("adultery", song(2.0, &[(wavesabre::kind::ADULTERY, chunk(&p))], &[melody()], &tracks[..1])),
        (
            "adultery_pingpong_reverse",
            song(2.0, &[(wavesabre::kind::ADULTERY, chunk(&ping)), (wavesabre::kind::CHAMBER, chunk(&[0.5; 9]))], &[melody()], &tracks),
        ),
    ]
}

/// One song using every device kind once (size probe `ws-all`): Slaughter through every effect but Smasher, the
/// other instruments on their own tracks, master Smasher keyed by Thunder. Adultery is silent without a bank.
pub fn showcase() -> Vec<u8> {
    use wavesabre::kind::*;
    let devices = [
        (SLAUGHTER, chunk(&slaughter_params())),
        (SCISSOR, chunk(&[0.7, 0.6, 1.0, 0.5, 0.8, 1.0])),
        (LEVELLER, chunk(&[0.2, 0.5, 0.5, 0.3, 0.7])),
        (CRUSHER, chunk(&[0.8, 0.5, 0.9])),
        (ECHO, chunk(&[0.375, 0.25, 0.5, 0.5, 0.2, 0.3, 0.6, 0.5])),
        (CHAMBER, chunk(&[0.5, 0.3, 0.6, 0.4, 0.2, 0.8, 0.5, 0.5, 0.4])),
        (TWISTER, chunk(&[0.0, 0.3, 0.5, 0.4, 0.2, 0.6, 0.5, 0.3, 0.8, 0.5])),
        (CATHEDRAL, chunk(&[0.0, 0.7, 0.4, 0.8, 0.5, 0.3, 0.5, 0.8, 0.5])),
        (FALCON, chunk(&[])),
        (THUNDER, gsm_chunk(&[])),
        (SPECIMEN, gsm_chunk(&[0.0, 0.3, 1.0])),
        (ADULTERY, chunk(&[1.0])),
        (SMASHER, chunk(&[1.0, 0.3, 0.5, 0.6, 0.1, 0.3, 0.5])),
    ];
    let tracks = [
        Track::new(&[0, 1, 2, 3, 4, 5, 6, 7], 0),
        Track::new(&[8], 0),
        Track::new(&[9], 1),
        Track::new(&[10], 0),
        Track::new(&[11], 0),
        Track::new(&[12], 0).recv(0, 0, 0.8).recv(1, 0, 0.5).recv(2, 0, 0.3).recv(2, 2, 1.0).recv(3, 0, 0.5).recv(4, 0, 0.5),
    ];
    song(2.0, &devices, &[melody(), hits()], &tracks)
}
