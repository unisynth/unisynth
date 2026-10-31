//! Mini-cauldron (TBL Jizz / Stash, 1997) patch data and its mapping to unified instruments (change
//! add-cauldron-xm-import). The data is `research/tbl-mini-cauldron/ref/patches_{jizz,stash}.json`, decoded from the
//! intros' patch tables; the algorithm is `research/tbl-mini-cauldron/mini-cauldron-spec.md`.

use serde_json::Value;

/// The synth version: v1 is Jizz's, v2 Stash's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    V1,
    V2,
}

/// One patch record. v1 records have no decimation choice (always 2), comb, doubler or gate: they fade each step.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    /// Filter envelope amount and base coefficient, per-frame envelope decay, damping, per-retrigger multiplier,
    /// pre-filter gain, echo feedback (f64 in v1, the exact f32 values in v2).
    pub a: f64,
    pub b: f64,
    pub decay: f64,
    pub damp: f64,
    pub mul: f64,
    pub gain: f64,
    pub echo: f64,
    /// Wave 0–5, variant count, voices 0–10.
    pub wave: u8,
    pub variants: u8,
    pub voices: u8,
    /// v2: decimation (1, 2, else 4), comb distance (0 = off), doubler on, gate hold and release (frames).
    pub decim: u8,
    pub comb: u8,
    pub doubler: bool,
    pub hold: u16,
    pub release: u16,
    pub pitch: [u16; 16],
    pub accent: [u8; 16],
    pub tie: [u8; 16],
}

/// One render call: Jizz's single call, or one of Stash's two (one per song).
#[derive(Clone, Debug)]
pub struct Call {
    /// `jizz`, `bluish` or `song0` (Stash's song played first).
    pub name: String,
    pub version: Version,
    pub records: Vec<Record>,
    /// Render order as record indices (Jizz: the order table; Stash: slot order, slot 1 being the drum kit).
    pub order: Vec<Slot>,
    /// Stash: song-1 specials (`flag`), drum samples copied into slot 1, the byte read as `tie[16]`.
    pub flag: u8,
    pub drum_count: usize,
    pub after_tie: u8,
}

/// A render slot of a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// Renders `record`'s variants (Stash slot number in `slot`; Jizz: the position in the order table).
    Render { slot: usize, record: usize },
    /// Stash slot 1: the stored drum kit.
    Drums,
}

/// The decoded patch data of one intro.
#[derive(Clone, Debug)]
pub struct Patches {
    pub calls: Vec<Call>,
    /// Stash: the stored drum samples' lengths.
    pub drum_lengths: Vec<usize>,
}

impl Patches {
    /// The call named `name` (`jizz`, `bluish`, `song0`).
    pub fn call(&self, name: &str) -> Option<&Call> {
        self.calls.iter().find(|c| c.name == name)
    }
}

fn num(v: &Value, k: &str) -> Result<f64, String> {
    v.get(k).and_then(Value::as_f64).ok_or_else(|| format!("patch data: missing number `{k}`"))
}

fn int(v: &Value, k: &str) -> Result<u64, String> {
    v.get(k).and_then(Value::as_u64).ok_or_else(|| format!("patch data: missing whole number `{k}`"))
}

fn arr16<T: TryFrom<u64> + Default + Copy>(v: &Value, k: &str) -> Result<[T; 16], String> {
    let a = v.get(k).and_then(Value::as_array).ok_or_else(|| format!("patch data: missing list `{k}`"))?;
    if a.len() != 16 {
        return Err(format!("patch data: `{k}` has {} entries, not 16", a.len()));
    }
    let mut out = [T::default(); 16];
    for (o, x) in out.iter_mut().zip(a) {
        *o = x.as_u64().and_then(|n| T::try_from(n).ok()).ok_or_else(|| format!("patch data: bad entry in `{k}`"))?;
    }
    Ok(out)
}

fn record(v: &Value, version: Version) -> Result<Record, String> {
    let v2 = version == Version::V2;
    let opt = |k: &str, d: u64| if v2 { int(v, k) } else { Ok(d) };
    Ok(Record {
        a: num(v, "A")?,
        b: num(v, "B")?,
        decay: num(v, "decay")?,
        damp: num(v, "damp")?,
        mul: num(v, "mul")?,
        gain: num(v, "gain")?,
        echo: num(v, "echo")?,
        wave: int(v, "wave")? as u8,
        variants: int(v, "variants")? as u8,
        voices: int(v, "voices")? as u8,
        decim: opt("decim", 2)? as u8,
        comb: opt("comb", 0)? as u8,
        doubler: opt("doubler", 0)? != 0,
        hold: opt("hold", 0)? as u16,
        release: opt("release", 0)? as u16,
        pitch: arr16(v, "pitch")?,
        accent: arr16(v, "accent")?,
        tie: arr16(v, "tie")?,
    })
}

/// Reads `patches_jizz.json` (`records` + `order`) or `patches_stash.json` (`calls`); anything else is an error.
pub fn load(json: &str) -> Result<Patches, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("patch data: {e}"))?;
    let records = |list: &Value, version| -> Result<Vec<Record>, String> {
        list.as_array().ok_or("patch data: `records` is not a list")?.iter().map(|r| record(r, version)).collect()
    };
    if let (Some(recs), Some(order)) = (v.get("records"), v.get("order").and_then(Value::as_array)) {
        let records = records(recs, Version::V1)?;
        let order = order
            .iter()
            .enumerate()
            .map(|(slot, r)| match r.as_u64() {
                Some(r) if (r as usize) < records.len() => Ok(Slot::Render { slot, record: r as usize }),
                _ => Err("patch data: bad record in `order`".to_string()),
            })
            .collect::<Result<_, _>>()?;
        return Ok(Patches { calls: vec![Call { name: "jizz".into(), version: Version::V1, records, order, flag: 0, drum_count: 0, after_tie: 0 }], drum_lengths: vec![] });
    }
    if let Some(calls) = v.get("calls").and_then(Value::as_array) {
        let calls = calls
            .iter()
            .map(|c| {
                let records = records(c.get("records").unwrap_or(&Value::Null), Version::V2)?;
                let slots = int(c, "slots")? as usize;
                if slots != records.len() + 1 {
                    return Err(format!("patch data: {slots} slots for {} records", records.len()));
                }
                let order = (0..slots).map(|s| if s == 1 { Slot::Drums } else { Slot::Render { slot: s, record: if s == 0 { 0 } else { s - 1 } } }).collect();
                Ok(Call {
                    name: c.get("name").and_then(Value::as_str).ok_or("patch data: call without a name")?.to_string(),
                    version: Version::V2,
                    records,
                    order,
                    flag: int(c, "flag")? as u8,
                    drum_count: int(c, "drum_count")? as usize,
                    after_tie: int(c, "after_tie")? as u8,
                })
            })
            .collect::<Result<_, String>>()?;
        let drum_lengths = v.get("drum_lengths").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|n| n as usize).collect()).unwrap_or_default();
        return Ok(Patches { calls, drum_lengths });
    }
    Err("patch data: neither a v1 file (`records`, `order`) nor a v2 file (`calls`)".into())
}

impl Version {
    /// The accent factor on the envelope amount and the base at a retrigger.
    pub fn accent(self) -> f64 {
        match self {
            Version::V1 => 4.0,
            Version::V2 => 2.0,
        }
    }
}

/// The filter coefficients of one retrigger: the compounded envelope amount `a` and base `b` (stored back, so the
/// next retrigger multiplies them again), the retrigger's number `k` since the patch started, and the accent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Retrig {
    pub k: u32,
    pub a: f64,
    pub b: f64,
    pub accent: bool,
}

impl Retrig {
    /// The working envelope amount and base (`v1`, `v2`) after the accent.
    pub fn working(&self, version: Version) -> (f64, f64) {
        let f = if self.accent { version.accent() } else { 1.0 };
        (self.a * f, self.b * f)
    }
}

impl Record {
    /// The stored sample's decimation: v1 2, v2 1, 2, else 4.
    pub fn decimation(&self, version: Version) -> u32 {
        match (version, self.decim) {
            (Version::V1, _) => 2,
            (_, 1) => 1,
            (_, 2) => 2,
            _ => 4,
        }
    }

    /// The second saw sounds: voices above 0, and a wave that reads it (the spike uses its phase as scratch, the
    /// noise replaces the sum).
    pub fn second_saw(&self) -> bool {
        self.voices != 0 && self.wave != 2 && self.wave != 4
    }

    /// Untied steps of the phrase (each retriggers).
    pub fn untied(&self) -> usize {
        self.tie.iter().filter(|&&t| t == 0).count()
    }

    /// The retrigger at `step` of `variant`, or `None` for a tied step. Every variant starts with one extra retrigger
    /// (the driver's, before step 0's), and `a`/`b` compound across variants: variant v is steps `16v..16v + 15` of one
    /// sweep. Each product is rounded to 24 bits as the original stores it.
    pub fn retrig(&self, variant: usize, step: usize) -> Option<Retrig> {
        if step >= 16 || self.tie[step] != 0 {
            return None;
        }
        let before = self.tie[..=step].iter().filter(|&&t| t == 0).count();
        let k = (variant * (1 + self.untied()) + 1 + before) as u32;
        let (mut a, mut b) = (self.a, self.b);
        for _ in 0..k {
            a = (a * self.mul) as f32 as f64;
            b = (b * self.mul) as f32 as f64;
        }
        Some(Retrig { k, a, b, accent: self.accent[step] == 1 })
    }
}

/// The original's internal rate (Hz): `2·8363·2^(13/12)`, the rate of an XM sample played at C#5.
pub const R: f64 = 35_441.159_432_507_15;
/// Phrase frames per step.
pub const STEP: u32 = 3530;
/// The glide of a tied step: linear in the pitch word over its first `C/2` phrase frames (`C` = 3529).
pub const GLIDE: f64 = 1764.5;

/// The note's retrigger byte for the cauldron slot (`shape::CLK_RETRIG`): `2k + accent`. k is at most 96 in both
/// intros; a larger one would not fit and is a converter bug.
pub fn note_byte(r: &Retrig) -> u8 {
    assert!(r.k <= 127, "retrigger {} does not fit the note byte", r.k);
    (2 * r.k + r.accent as u32) as u8
}

/// The velocity that carries a note's phrase frame (`unisynth::osc::clocked_t0`): 1 + eighth steps, at most 127.
/// The age byte of a note entering a phrase at `phrase_frame`, `since` phrase frames after the retrigger that started
/// its step's tie (unisynth `osc::clocked_age`): `1 + eighth steps`, or 0 (a plain restart) at the retrigger itself.
pub fn age_byte(since: f64) -> u8 {
    match (since.max(0.0) * 8.0 / STEP as f64).round() {
        e if e < 1.0 => 0,
        e => (1.0 + e).min(255.0) as u8,
    }
}

pub fn velocity(phrase_frame: f64) -> u8 {
    (1.0 + (phrase_frame * 8.0 / STEP as f64).round()).clamp(1.0, 127.0) as u8
}

/// The unified instrument of a record, for a track whose phrases play at `ratio` times the original rate (design D2,
/// D4, D6): a cauldron slot with the record's wave, voices, version, gain and filter (base and amount uncompounded:
/// each note's retrigger byte compounds them), mono legato with the glide as the slide, an amplitude release with the
/// energy of the original's linear note-end fade, and a voice master of √2 (the centred slot pans at √½ per channel).
/// The 16-bit clamp is the first effect of the chain ([`effects`]): the original fades before it clamps. The per-note
/// values (retrigger byte, phrase frame as the velocity) and the levels are the song's.
pub fn instrument(version: Version, r: &Record, ratio: f64) -> [f32; unisynth::params::COUNT] {
    use crate::patch::Patch;
    use unisynth::law::host as inv;
    use unisynth::params::{o, osc_type, shape as sp, v};
    let clock = R * ratio;
    let mut p = Patch::new();
    p.osc(0, o::TYPE, osc_type::encode(osc_type::CLOCKED));
    p.shape(0, sp::CLK_WAVE, inv::inv_choice(r.wave as usize, 6));
    p.shape(0, sp::CLK_VOICES, r.voices as f32 / 10.0);
    p.shape(0, sp::CLK_VERSION, inv::inv_choice((version == Version::V2) as usize, 2));
    p.shape(0, sp::CLK_CLOCK, (clock / 1_048_576.0) as f32);
    p.shape(0, sp::CLK_F_BASE, (r.b / 8.0) as f32);
    p.shape(0, sp::CLK_F_AMOUNT, (r.a / 8.0) as f32);
    p.shape(0, sp::CLK_F_DECAY, (0.5 + (1.0 - r.decay) * 100.0) as f32);
    p.shape(0, sp::CLK_F_DAMP, r.damp as f32);
    p.osc(0, o::FEEDBACK, (r.mul / 4.0) as f32);
    p.osc(0, o::GAIN, inv::inv_gain(r.gain as f32));
    // the shift that centres the record's words (`Tuning`)
    Tuning::of(&r.pitch, ratio).set(&mut p.v);
    p.set(v::MODE, inv::inv_choice(1, 2));
    // one voice: a retrigger restarts it (phase −32766, the filter's output memory 0) while the filter's `y` and the
    // resampler run on, as the original's retrigger keeps its state; the previous note's release is cut there, where
    // the original's fade has already reached 0
    p.set(v::CAPACITY, inv::inv_count(1, 1, 64));
    p.set(v::SLIDE, inv::inv_time((GLIDE / clock) as f32));
    p.set(v::VELOCITY, 0.0);
    // note end (D6): v1 fades over the step, v2 over `release` frames after the gate; a quadratic release
    // (1 − t/T)² has the energy of a linear fade over 3/5 T
    let fade = if version == Version::V1 { (STEP - 1) as f64 } else { r.release as f64 };
    let ms = (5.0 / 3.0 * fade / clock * 1000.0) as f32;
    p.env(0, 0.0, 0.0, 1.0, inv::inv_env_ms(ms.max(1.0)));
    // the centred slot pans at √½ per channel: the master makes each channel the original's level
    p.set(v::MASTER, inv::inv_gain(core::f32::consts::SQRT_2));
    p.finish()
}

/// A track's effect chain (design D7): devices in order as `(kind, [mode value, p0, …])`, plus what the converter
/// must apply around it.
#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    pub devices: Vec<(u8, Vec<f32>)>,
    /// Output frames the comb and the doubler delay the track by: the converter moves the notes earlier by this.
    pub latency: u32,
    /// The level the chain leaves the signal at (the echo mix scales it by `1/(1 + echo)` unless a wrap after it makes
    /// up for that): the converter multiplies the track gain by `1/level`.
    pub level: f64,
    /// The device whose parameter 4 restarts the doubler's sweep: the converter steps it at each phrase trigger.
    pub restart: Option<usize>,
    /// The echo's time (ms) when it exceeds the tempo delay's longest (16 eighths + 200 ms at the tempo): it plays
    /// at that longest instead.
    pub echo_clamped: Option<f64>,
    /// The band-limiting low-pass (a decimated record, or a sliding track): the first of its `LP_STAGES` devices and
    /// the stored Nyquist at ratio 1 (Hz). Their high cut (parameter 6, [`lowpass_p`]) follows a slide: the converter
    /// automates it.
    pub lowpass: Option<(usize, f64)>,
}

/// The band-limiting low-pass's stages: SVF high cuts of 12 dB/octave in series. One let a wrap's jumps through as
/// clicks above the stored Nyquist (Stash song 1's record 10: decimated by 4, its +48 wrap clicked at 6–12 kHz where
/// the original, wrapped before decimation, has nothing above 4.4 kHz).
pub const LP_STAGES: usize = 2;

/// A tempo delay's time parameters for `ms` at `tempo` BPM: (coarse, fine, clamped). The unit is an eighth of
/// `120/tempo` s (16 at most), the rest whole milliseconds (200 at most: past that `clamped`). Half a step up survives
/// the engine's truncation; the top (16 units, 200 ms) is exact at 1.
pub(crate) fn delay_time(ms: f64, tempo: i32) -> (f32, f32, bool) {
    let unit = 120.0 / tempo as f64 / 8.0 * 1000.0;
    let coarse = (ms / unit).floor().min(16.0);
    let fine = (ms - coarse * unit).round();
    let clamped = fine > 200.0;
    let fine = fine.min(200.0);
    ((((coarse + 0.5) / 16.0) as f32).min(1.0), (((fine + 0.5) / 200.0) as f32).min(1.0), clamped)
}

/// The decimation low-pass's high-cut value for `hz`: the tempo delay's SVF law, 20 Hz–20 kHz, open above.
pub fn lowpass_p(hz: f64) -> f32 {
    ((hz.clamp(20.0, 20_000.0) - 20.0) / 19980.0).sqrt() as f32
}

/// The doubler's read-rate excess per frame (it reads at `0xFF3B/65536`, the inverse below 1).
const SWEEP: f64 = 65536.0 / 0xFF3B as f64 - 1.0;
/// The doubler's last frame: the largest read offset is `SWEEP` times this.
const DOUBLER_END: f64 = 58000.0;

/// The effect chain of `call`'s record `record` on a track at `ratio`, for a song at `rate` Hz and `tempo` BPM
/// (the tempo delay's unit is a quarter beat). Original order: 16-bit clamp → comb → echo → doubler → song0's specials
/// → decimation, which becomes a low-pass at the stored Nyquist frequency (D6). The clamp is the shaper's clipper at
/// full scale: after the voice's note-end release, as the original fades before it clamps.
pub fn effects(call: &Call, record: usize, ratio: f64, rate: f64, tempo: i32) -> Chain {
    use unisynth::fx::mode_value;
    use unisynth::kind::{DELAY, DISTORTION, FLANGER};
    let r = &call.records[record];
    let kappa = R * ratio / rate; // phrase frames per output frame
    let mut out = Chain { devices: vec![], latency: 0, level: 1.0, restart: None, echo_clamped: None, lowpass: None };
    // shaper clipper at ±`at`: drive 0.2 (unity), threshold `at`, no foldover, fully wet, clipper, no oversampling.
    // The original clamps after its 16-bit store, its comb and its echo: the comb difference and the echo sum reach
    // up to twice full scale on the hot Stash patches.
    let clip = |at: f64| (DISTORTION, vec![mode_value(DISTORTION, 0), 0.2, at as f32, 0.0, 1.0, 0.0, 0.0]);
    out.devices.push(clip(1.0));
    let comb = |k: f64, sweep: f64| vec![mode_value(FLANGER, 2), (k / 16384.0) as f32, 1.0, (sweep / 0.01) as f32, 0.0];
    if call.version == Version::V2 && r.comb > 0 {
        let k = (r.comb as f64 / kappa).round().max(1.0);
        out.devices.push((FLANGER, comb(k, 0.0)));
        out.devices.push(clip(1.0));
        out.latency += k as u32;
    }
    let mut level = 1.0;
    if r.echo > 0.0 {
        // y = x + e·y[n − D], D = 3 steps of 3529 phrase frames: the tempo delay with feedback e, wet e/(1 + e)
        let e = r.echo;
        let ms = 3.0 * 3529.0 / kappa / rate * 1000.0;
        let (c, f, clamped) = delay_time(ms, tempo);
        if clamped {
            out.echo_clamped = Some(ms);
        }
        out.devices.push((DELAY, vec![mode_value(DELAY, 0), c, f, c, f, 0.0, 1.0, e as f32, 0.0, (e / (1.0 + e)) as f32]));
        level = 1.0 / (1.0 + e);
        out.devices.push(clip(level)); // the wet mix scaled the original's full scale by 1/(1 + e)
    }
    if call.version == Version::V2 && r.doubler {
        let k = (SWEEP * DOUBLER_END / kappa).ceil();
        out.restart = Some(out.devices.len());
        out.devices.push((FLANGER, comb(k, SWEEP)));
        out.latency += k as u32 + 1;
    }
    // song0's specials on Stash slots 9 (×1.25) and 11, 15 (+48 of 128), wrapping as 8-bit samples. Only slot 9's
    // wraps: on 11 and 15 the spike wave's DC keeps every value under 80, so +48 is a DC shift (their stored samples
    // have no wrap jump, minimum −79). Ours, the DC lost to the echo's low cut, wrapped on every peak (song 1, 18–25 s:
    // record 10 crackling, +5 to +21 dB above 1.6 kHz), so they get none
    let special = match (call.flag, record + 1) {
        (1, 9) => Some((1.25, 0.0)),
        _ => None,
    };
    if let Some((drive, offset)) = special {
        let drive = drive / level; // the wrap sees the original's values
        out.devices.push((DISTORTION, vec![mode_value(DISTORTION, 3), (drive / 4.0) as f32, ((offset + 1.0) / 2.0) as f32]));
        level = 1.0;
    }
    out.level = level;
    let decim = r.decimation(call.version) as f64;
    let hz = R * ratio / (2.0 * decim);
    if decim > 1.0 && hz < 20_000.0 && hz < 0.45 * rate {
        // the tempo delay as a filter: no time (1 frame), fully wet, no feedback, its SVF high cut (12 dB/octave) at
        // the stored Nyquist and its low cut at 20 Hz. Flat to ±0.3 dB from 60 Hz up (the parametric EQ's fixed low
        // cut is not: WaveSabre's table cosine bends it to a resonant ~45 Hz corner).
        out.lowpass = Some((out.devices.len(), R / (2.0 * decim)));
        for _ in 0..LP_STAGES {
            out.devices.push(lowpass_device(hz));
        }
        out.latency += LP_STAGES as u32;
    }
    out
}

/// The band-limiting low-pass device (the tempo delay's SVF high cut) at `hz`, open (20 kHz) past 20 kHz.
fn lowpass_device(hz: f64) -> (u8, Vec<f32>) {
    use unisynth::fx::mode_value;
    (unisynth::kind::DELAY, vec![mode_value(unisynth::kind::DELAY, 0), 0.0, 0.0, 0.0, 0.0, 0.0, lowpass_p(hz), 0.0, 0.0, 1.0])
}

/// A track whose triggers slide (an XM pitch slide, played as a rate: the slot's phrase clock follows it) gets the
/// band-limiting low-pass even when its record is not decimated: the original's clamps and wrap ran on the sample
/// before the player slowed it, so their harmonics slowed too; ours run at the output rate, and the low-pass (its cut
/// following the slide, automated by the converter) trims them to the stored Nyquist times the playing ratio.
pub fn slide_lowpass(chain: &mut Chain, call: &Call, record: usize, ratio: f64) {
    if chain.lowpass.is_none() {
        let nyq = R / (2.0 * call.records[record].decimation(call.version) as f64);
        chain.lowpass = Some((chain.devices.len(), nyq));
        for _ in 0..LP_STAGES {
            chain.devices.push(lowpass_device(nyq * ratio));
        }
        chain.latency += LP_STAGES as u32;
    }
}

/// Phrase frames from a note-on to the earliest note-off (the release starts from the attack's peak, 1 ms).
pub const ATTACK: f64 = 64.0;

/// How a record's pitch words become notes at `ratio`. The slot steps by the word it is asked for, so a note asks for
/// the **unsigned** word's frequency `w·R·ratio/65536`: a word above 32767 then runs the phase backwards and its
/// second saw by `round(w·1.02)` of the unsigned word, as the original's (Stash's words are unsigned). Words span up to
/// 172 semitones (3 to 60736) and a note 128, so the instrument shifts the slot to centre the record's words (its
/// octave and the rest as `1 + ratio_fine`, a static value), and a word still outside 0–127 moves by the slot's coarse
/// pitch (±24 semitones, a per-note byte).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// The slot's `o::OCTAVE`, `o::RATIO` and `o::RATIO_FINE` values.
    octave: f32,
    ratio_fine: f32,
    /// The semitones they shift by, through the laws.
    shift: f64,
    ratio: f64,
}

impl Tuning {
    pub fn of(words: &[u16], ratio: f64) -> Tuning {
        use unisynth::law;
        let semis = |w: u16| Tuning::semis_at(w, ratio);
        // word 0 (a frozen phase) only has to round to 0: it plays as low as the notes reach, the word byte the rest
        let (lo, hi) = words.iter().filter(|&&w| w > 0).map(|&w| semis(w)).fold((f64::MAX, f64::MIN), |(a, b), s| (a.min(s), b.max(s)));
        let (lo, hi) = if lo > hi { (60.0, 60.0) } else { (lo, hi) };
        let want = ((lo + hi) / 2.0 - 63.5).clamp(-48.0, 59.0);
        let oct = (want / 12.0).floor();
        let rf = 2f64.powf((want - 12.0 * oct) / 12.0) - 1.0; // 0..1: ratio_fine = (2p − 1)³
        let (octave, ratio_fine) = (law::host::inv_octave(oct as f32), ((rf.cbrt() + 1.0) / 2.0) as f32);
        let r = (law::ratio(0.0) + law::ratio_fine(ratio_fine)) as f64;
        Tuning { octave, ratio_fine, shift: 12.0 * law::octave(octave) as f64 + 12.0 * r.log2(), ratio }
    }

    /// Writes the slot's octave and ratio into instrument values.
    pub fn set(&self, values: &mut [f32; unisynth::params::COUNT]) {
        use unisynth::params::{o, osc};
        values[osc(0, o::OCTAVE)] = self.octave;
        values[osc(0, o::RATIO)] = 0.0;
        values[osc(0, o::RATIO_FINE)] = self.ratio_fine;
    }

    /// The word's pitch in semitones (MIDI scale); word 0 as word 1.
    fn semis_at(word: u16, ratio: f64) -> f64 {
        let hz = word.max(1) as f64 * R * ratio / 65536.0;
        69.0 + 12.0 * (hz / 440.0).log2()
    }

    /// (note, coarse byte, fine byte, word byte) for `word`. The slot's word is `rint(f·65536/clock) + b − 128`
    /// (`ClockedT::word`) and the word byte makes up the rest. The fine byte is the nearest one unless its word lies
    /// within 0.2 of a rounding edge, where the slot's f32 path could flip it: then the one of ±2 farthest from it.
    pub fn note(&self, word: u16) -> (u8, u8, u8, u8) {
        use unisynth::law;
        let n = Tuning::semis_at(word, self.ratio) - self.shift;
        let nr = n.round();
        let coarse = (nr - nr.clamp(0.0, 127.0)).clamp(-24.0, 24.0);
        let note = (nr - coarse).clamp(0.0, 127.0);
        let byte = |p: f32| (p * 255.0).round() as u8;
        let (cb, ideal) = (byte(law::host::inv_coarse(coarse as f32)), byte(law::host::inv_fine((n - note - coarse) as f32)));
        // the word this (note, coarse, fine) asks for, through the laws and the stored clock
        let clock = (R * self.ratio / 1_048_576.0) as f32 as f64 * 1_048_576.0;
        let enc = |fb: u8| {
            let semis = note + law::coarse(cb as f32 / 255.0) as f64 + law::fine(fb as f32 / 255.0) as f64 + self.shift;
            440.0 * 2f64.powf((semis - 69.0) / 12.0) * 65536.0 / clock
        };
        let edge = |fb: u8| (enc(fb) - enc(fb).round()).abs();
        let fb = if edge(ideal) <= 0.3 {
            ideal
        } else {
            (ideal.saturating_sub(2)..=ideal.saturating_add(2)).min_by(|&a, &b| edge(a).total_cmp(&edge(b))).unwrap()
        };
        let d = word as i64 - enc(fb).round() as i64;
        assert!(d.abs() <= 127, "word {word}: {d} words from its note");
        (note as u8, cb, fb, (128 + d) as u8)
    }
}

/// The second saw's phase word at the start of `step` of `variant` (it runs on across the record's variants, from 0):
/// the original's `phase2 += fistp(pitch·1.02)` per frame, with the pitch of the frame before (a tied step glides its
/// pitch word over its first `GLIDE` frames). x87 24-bit rounding is f32 rounding here.
pub fn phase2_at(call: &Call, record: usize, variant: usize, step: usize) -> u16 {
    let r = &call.records[record];
    let (c, detune) = (3529.0f32, 1.02f32);
    let i16w = |w: u16| w as i16 as f32;
    let mut phase2: u16 = 0;
    for v in 0..=variant {
        let mut pitch: i32 = r.pitch[0] as i32;
        for s in 0..16 {
            if v == variant && s == step {
                return phase2;
            }
            if s == 0 || r.tie[s] == 0 {
                pitch = r.pitch[s] as i32; // the retrigger reads the word unsigned
            }
            for n in 0..STEP {
                phase2 = phase2.wrapping_add(((pitch as f32) * detune).round_ties_even() as i32 as u16);
                if s != 0 && r.tie[s] == 1 {
                    pitch = if (n as f64) < GLIDE - 0.5 {
                        let n2 = (2 * n) as f32;
                        ((c - n2) / c * i16w(r.pitch[s - 1]) + n2 / c * i16w(r.pitch[s])).round_ties_even() as i32
                    } else {
                        r.pitch[s] as i32
                    };
                }
            }
        }
    }
    phase2
}

/// The cauldron slot's phase byte (`o::PM_OUT`) for a phase word: `1 + round(255·w/65536)` mod 255 (0 is "keep").
pub fn phase_byte(word: u16) -> u8 {
    1 + ((word as f64 * 255.0 / 65536.0).round() as u32 % 255) as u8
}

/// A phrase event (`at` in phrase frames from the trigger).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhraseEv {
    pub at: f64,
    pub what: PEv,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PEv {
    /// A retrigger: a new note with its coarse and fine bytes (`Tuning`), retrigger byte (`shape::CLK_RETRIG`),
    /// second-saw phase byte (`o::PM_OUT`, 0 when the record has no second saw), the phrase position as the velocity,
    /// and the filter envelope's age byte (`o::TRACKING`: 0 restarts it, as a retrigger; `1 + eighth steps` since the
    /// last retrigger for a note entering the phrase mid-way, [`age_byte`]).
    On { note: u8, coarse: u8, fine: u8, word: u8, retrig: u8, phase: u8, vel: u8, age: u8 },
    /// A tie to another semitone: the new note on (mono legato slides), then the held note `from` off; `vel` the
    /// phrase position, as an `On`'s.
    Legato { note: u8, coarse: u8, fine: u8, word: u8, from: u8, vel: u8 },
    /// A tie within the same semitone: only the coarse, fine and word bytes change.
    Tune { coarse: u8, fine: u8, word: u8 },
    /// The held note ends (its release stands for the original's fade or gate).
    Off,
    /// A v2 tie's glide window (its first `GLIDE` frames) starts or ends where either word is above 32767: the
    /// original's pitch register then holds the signed word (`shape::CLK_VERSION` ½).
    Glide { on: bool },
}

/// The notes of one trigger of `variant` of `call`'s record `record` (design D1, D6), in phrase time. Untied steps
/// retrigger; tied steps slide. A note ends where the original starts fading: v1 fades the last step of a tie chain
/// (step 15 never), v2 gates `hold` frames into it; the rest of the phrase ends at the sample's end.
pub fn phrase_events(call: &Call, record: usize, variant: usize, ratio: f64) -> Vec<PhraseEv> {
    let r = &call.records[record];
    let v2 = call.version == Version::V2;
    let len = match (v2, r.decim) {
        (false, _) => 56000.0,
        (true, 1) => 59000.0,
        (true, 2) => 58000.0,
        _ => 56000.0,
    };
    let mut out = vec![];
    let tuning = Tuning::of(&r.pitch, ratio);
    let mut held: Option<(u8, (u8, u8, u8), f64)> = None; // note, (coarse, fine, word), on frame
    for s in 0..16 {
        let at = (s as u32 * STEP) as f64;
        let (note, coarse, fine, word) = tuning.note(r.pitch[s]);
        match (r.retrig(variant, s), held) {
            (Some(rt), _) => {
                let phase = if r.second_saw() { phase_byte(phase2_at(call, record, variant, s)) } else { 0 };
                out.push(PhraseEv { at, what: PEv::On { note, coarse, fine, word, retrig: note_byte(&rt), phase, vel: velocity(at), age: 0 } });
                held = Some((note, (coarse, fine, word), at));
            }
            (None, Some((n, t, on))) => {
                if v2 && (r.pitch[s - 1] > 32767 || r.pitch[s] > 32767) {
                    out.push(PhraseEv { at, what: PEv::Glide { on: true } });
                    out.push(PhraseEv { at: at + (GLIDE - 0.5), what: PEv::Glide { on: false } });
                }
                if note != n {
                    out.push(PhraseEv { at, what: PEv::Legato { note, coarse, fine, word, from: n, vel: velocity(at) } });
                } else if (coarse, fine, word) != t {
                    out.push(PhraseEv { at, what: PEv::Tune { coarse, fine, word } });
                }
                held = Some((note, (coarse, fine, word), on));
            }
            (None, None) => {} // a tie after the note ended: the original keeps the voice silent
        }
        let next_tied = if s < 15 { r.tie[s + 1] != 0 } else if v2 { call.after_tie != 0 } else { true };
        if let (false, Some((_, _, on))) = (next_tied, held) {
            let end = if v2 { at + r.hold as f64 } else { at };
            let end = end.max(on + ATTACK).min(at + STEP as f64 - ATTACK);
            out.push(PhraseEv { at: end, what: PEv::Off });
            held = None;
        }
    }
    if held.is_some() {
        out.push(PhraseEv { at: len, what: PEv::Off });
    }
    out.sort_by(|a, b| a.at.total_cmp(&b.at)); // a glide window can outlast a v2 gate
    out
}

/// The note lane and the instrument's per-note byte lanes (device 0) for `evs` at song frames, in time order: each
/// note's retrigger, coarse, fine, word and phase bytes stepped `LANE_LEAD` frames before it, the v2 glide window as
/// the version lane; `values` get each lane's first byte.
pub fn note_lanes(evs: impl Iterator<Item = (i64, PEv)>, values: &mut [f32; unisynth::params::COUNT]) -> (Vec<(i32, u8, Option<u8>)>, Vec<crate::model::Auto>) {
    use crate::model::Auto;
    use unisynth::params::{self as pm, o, shape as sp};
    let (mut retrig, mut fine, mut phase, mut coarse, mut glide, mut word, mut age) = (vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
    let mut lane: Vec<(i32, u8, Option<u8>)> = vec![];
    let (mut t, mut held) = (0i64, 0u8);
    let mut push = |lane: &mut Vec<(i32, u8, Option<u8>)>, f: i64, note: u8, vel: Option<u8>| {
        lane.push(((f - t) as i32, note, vel));
        t = f;
    };
    for (f, what) in evs {
        match what {
            PEv::On { note, coarse: cb, fine: fb, word: wb, retrig: rb, phase: pb, vel, age: ab } => {
                word.push((f - LANE_LEAD, wb));
                age.push((f - LANE_LEAD, ab));
                retrig.push((f - LANE_LEAD, rb));
                phase.push((f - LANE_LEAD, pb));
                fine.push((f - LANE_LEAD, fb));
                coarse.push((f - LANE_LEAD, cb));
                push(&mut lane, f, note, Some(vel));
                held = note;
            }
            PEv::Legato { note, coarse: cb, fine: fb, word: wb, from, vel } => {
                word.push((f - LANE_LEAD, wb));
                fine.push((f - LANE_LEAD, fb));
                coarse.push((f - LANE_LEAD, cb));
                push(&mut lane, f, note, Some(vel));
                push(&mut lane, f, from, None);
                held = note;
            }
            PEv::Tune { coarse: cb, fine: fb, word: wb } => {
                word.push((f - LANE_LEAD, wb));
                fine.push((f - LANE_LEAD, fb));
                coarse.push((f - LANE_LEAD, cb));
            }
            PEv::Off => push(&mut lane, f, held, None),
            // a window edge, not a note start: half a lane lead centres the player's sub-block step on it
            PEv::Glide { on } => glide.push((f - LANE_LEAD / 2, if on { 128 } else { 255 })),
        }
    }
    let (rid, fid, pid) = (pm::osc(0, o::SHAPE + sp::CLK_RETRIG), pm::osc(0, o::FINE), pm::osc(0, o::PM_OUT));
    let (r0, f0, p0) = (retrig.first().map_or(0, |x| x.1), fine.first().map_or(128, |x| x.1), phase.first().map_or(0, |x| x.1));
    values[rid] = r0 as f32 / 255.0;
    values[fid] = f0 as f32 / 255.0;
    values[pid] = p0 as f32 / 255.0;
    let mut autos = vec![
        // the retrigger and phase bytes are read at note-ons only (the filter keeps its note-on settings): free between
        Auto { device: 0, param: rid, points: latched_lane(r0, &retrig, LATCH_WINDOW) },
        Auto { device: 0, param: fid, points: byte_lane(f0, &fine) },
        Auto { device: 0, param: pid, points: latched_lane(p0, &phase, LATCH_WINDOW) },
    ];
    if !glide.is_empty() {
        let vid = pm::osc(0, o::SHAPE + sp::CLK_VERSION);
        autos.push(Auto { device: 0, param: vid, points: byte_lane(255, &glide) });
    }
    // the age byte (`o::TRACKING`): only a track with jumps into a phrase needs the lane
    let (a0, aid) = (age.first().map_or(0, |x| x.1), pm::osc(0, o::TRACKING));
    values[aid] = a0 as f32 / 255.0;
    if age.iter().any(|x| x.1 != a0) {
        autos.push(Auto { device: 0, param: aid, points: latched_lane(a0, &age, LATCH_WINDOW) });
    }
    // the word byte (`o::FIXED_HZ`, the slot's exact integer word)
    let (w0, wid) = (word.first().map_or(128, |x| x.1), pm::osc(0, o::FIXED_HZ));
    values[wid] = w0 as f32 / 255.0;
    autos.push(Auto { device: 0, param: wid, points: byte_lane(w0, &word) });
    // a coarse lane only for a record whose words need one (most fit the notes after the octave)
    let c0 = coarse.first().map_or(128, |x| x.1);
    let cid = pm::osc(0, o::COARSE);
    values[cid] = c0 as f32 / 255.0;
    if coarse.iter().any(|x| x.1 != c0) {
        autos.push(Auto { device: 0, param: cid, points: byte_lane(c0, &coarse) });
    }
    (lane, autos)
}

/// Output frames a per-note byte steps before its note: the player applies automation once per sub-block (100
/// frames by default), at the sub-block's start, so the step must fall in an earlier sub-block.
pub const LANE_LEAD: i64 = 128;

/// Automation points for a byte lane: `initial` from frame 0, then a step at each `(frame, value)` change.
pub(crate) fn byte_lane(initial: u8, changes: &[(i64, u8)]) -> Vec<(i32, u8)> {
    let mut pts = vec![(0, initial)];
    let (mut last, mut cur) = (0i64, initial);
    for &(f, v) in changes {
        if v == cur {
            continue;
        }
        let f = f.max(last);
        pts.push(((f - last) as i32, cur));
        pts.push((0, v));
        (last, cur) = (f, v);
    }
    pts
}

/// [`byte_lane`] with fewer points: the lane may differ from the step function the changes mean by up to `tol` bytes
/// at any frame (the player interpolates linearly between points). Small steps (a slide, a fade) become ramps through
/// as few points as fit; a step larger than `tol`, or one at a frame in `breaks`, stays an exact step. `tol` 0 is
/// [`byte_lane`].
pub(crate) fn thin_lane(initial: u8, changes: &[(i64, u8)], tol: u8, breaks: &[i64]) -> Vec<(i32, u8)> {
    if tol == 0 {
        return byte_lane(initial, changes);
    }
    // the step function as segments (start frame, value), as `byte_lane` reads the changes
    let mut seg: Vec<(i64, u8)> = vec![(0, initial)];
    for &(f, v) in changes {
        let (lf, lv) = *seg.last().unwrap();
        if v == lv {
            continue;
        }
        let f = f.max(lf);
        if f == lf && seg.len() > 1 {
            // a zero-length segment: the later value wins (merged into the one before if equal)
            seg.last_mut().unwrap().1 = v;
            if seg[seg.len() - 2].1 == v {
                seg.pop();
            }
            continue;
        }
        seg.push((f, v));
    }
    let tol = tol as f64;
    let hard = |k: usize| (seg[k + 1].1 as f64 - seg[k].1 as f64).abs() > tol || breaks.contains(&seg[k + 1].0);
    // runs of samples between hard steps: each segment contributes its start and its last frame (a soft boundary) or
    // the next segment's start (a hard one, where the step is exact: two points at one frame)
    let mut out: Vec<(i64, u8)> = vec![];
    let mut run: Vec<(i64, f64)> = vec![];
    let fit = |run: &[(i64, f64)], out: &mut Vec<(i64, u8)>| fit_line(run, tol, out);
    for k in 0..seg.len() {
        let (s0, v) = (seg[k].0, seg[k].1 as f64);
        if run.last().is_none_or(|x: &(i64, f64)| x.0 < s0) {
            run.push((s0, v));
        }
        if k + 1 == seg.len() {
            break;
        }
        let next = seg[k + 1].0;
        if hard(k) {
            run.push((next, v));
            fit(&run, &mut out);
            run.clear();
        } else if next - 1 > s0 {
            run.push((next - 1, v));
        }
    }
    if !run.is_empty() {
        fit(&run, &mut out);
    }
    deltas(out)
}

/// Greedy polyline through `run` (frames ascending, byte values): from each vertex, the farthest sample whose line
/// keeps every sample between within `tol`; vertices are samples, so their bytes are exact.
fn fit_line(run: &[(i64, f64)], tol: f64, out: &mut Vec<(i64, u8)>) {
    let mut i = 0;
    out.push((run[0].0, run[0].1 as u8));
    while i + 1 < run.len() {
        let mut j = i + 1;
        while j + 1 < run.len() {
            let (a, b) = (run[i], run[j + 1]);
            let ok = (i + 1..=j).all(|m| {
                let (f, v) = run[m];
                let line = a.1 + (b.1 - a.1) * (f - a.0) as f64 / (b.0 - a.0) as f64;
                (line - v).abs() <= tol
            });
            if !ok {
                break;
            }
            j += 1;
        }
        out.push((run[j].0, run[j].1 as u8));
        i = j;
    }
}

/// Absolute `(frame, byte)` points → automation points (frame deltas).
fn deltas(out: Vec<(i64, u8)>) -> Vec<(i32, u8)> {
    let mut last = 0i64;
    out.into_iter()
        .map(|(f, v)| {
            let d = (f - last) as i32;
            last = f;
            (d, v)
        })
        .collect()
}

/// Frames after a per-note byte's step (`LANE_LEAD` before its note) through which the byte must hold: the lead plus a
/// player sub-block (100 frames by default; the note-on reads the parameter there).
pub const LATCH_WINDOW: i64 = LANE_LEAD + 128;

/// A per-note byte lane the oscillator reads only at note-ons (the cauldron's retrigger, start phase and age bytes):
/// each change's byte holds exactly (within half a byte, so it decodes to the same byte) from its frame for `window`
/// frames (or until the next change); between those windows the lane is free, so notes whose bytes lie on a line
/// (a retrigger count rising by one per note) share one ramp.
pub(crate) fn latched_lane(initial: u8, changes: &[(i64, u8)], window: i64) -> Vec<(i32, u8)> {
    let mut run: Vec<(i64, f64)> = vec![(0, initial as f64)];
    for (k, &(f, v)) in changes.iter().enumerate() {
        let f = f.max(run.last().unwrap().0 + 1);
        // the last byte needs no window end: a lane holds its last point
        let end = changes.get(k + 1).map_or(f, |n| (f + window).min(n.0 - 1));
        run.push((f, v as f64));
        if end > f {
            run.push((end, v as f64));
        }
    }
    let mut out = vec![];
    fit_line(&run, 0.45, &mut out);
    deltas(out)
}

/// One trigger of `variant` of `call`'s record `record` at native speed as a song at `rate` (the per-phrase parity
/// entry, `synth-convert xm-phrase`): the record's instrument and effect chain on one track, its phrase events as
/// notes, the retrigger and fine bytes as lanes, the doubler's restart at the phrase start. Returns the song and the
/// frame where the phrase's output starts (a lead-in covers the chain's latency and the first lane step).
pub fn phrase_song(call: &Call, record: usize, variant: usize, rate: i32) -> (crate::model::SongM, usize) {
    use crate::model::{Auto, Dev, SongM, Trk};
    let (ratio, tempo) = (1.0, 125);
    let r = &call.records[record];
    let kappa = R * ratio / rate as f64;
    let chain = effects(call, record, ratio, rate as f64, tempo);
    let lead = chain.latency as i64 + 2 * LANE_LEAD;
    let frame = |at: f64| lead - chain.latency as i64 + (at / kappa).round() as i64;
    let evs = phrase_events(call, record, variant, ratio);
    let mut values = instrument(call.version, r, ratio);
    let (lane, mut autos) = note_lanes(evs.iter().map(|e| (frame(e.at), e.what)), &mut values);
    if let Some(i) = chain.restart {
        autos.push(Auto { device: 1 + i, param: 4, points: byte_lane(0, &[(lead, 255)]) });
    }
    let mut devices = vec![Dev { kind: unisynth::kind::INSTRUMENT, chunk: unisynth::chunk::encode(&values, &[]) }];
    devices.extend(chain.devices.iter().map(|(k, v)| Dev { kind: *k, chunk: unisynth::fx::encode(v) }));
    // the echo mix's level, and Jizz's record 7, which the original halves after rendering
    let halve = if call.version == Version::V1 && record == 7 { 0.5 } else { 1.0 };
    let track = Trk { volume: (halve / chain.level) as f32, receives: vec![], devices: (0..devices.len()).collect(), lane: 0, autos };
    let end = evs.iter().map(|e| e.at).fold(0.0, f64::max);
    let length = (frame(end) as f64 + 0.5 * rate as f64) / rate as f64;
    (SongM { tempo, rate, length, devices, lanes: vec![lane], tracks: vec![track] }, lead as usize)
}

pub mod import;

#[cfg(test)]
mod tests;
