//! Fudge's two generators (Alien Sex Clone, 1998; change add-fudge-softsynth) as generator-map engines: `fm-shaper`
//! plays on a Shaper tone slot, `sweep-drum` on a Sweep drum slot (`unisynth::osc::engine`). A map record is
//! `{"frames": n, "args": [a0 … a8], "carry": c}`: the nine pushed arguments as the original passed them, float bit
//! patterns except the integer flags (the tone's FM switch a7, the drum's envelope curve a8). Each argument is written
//! on its law (design D3), which gives the f32 back exactly; the drum's envelope end and call length become its slope.
//! Ignored: `carry` (frame 0 always resets the shaper value) and the tone's a3 (the original never reads it).

use super::Engine;
use serde_json::Value;
use unisynth::law::host as inv;
use unisynth::params::shape as sp;

/// Fudge's tone with a two-stage shaper.
pub struct ShaperTone;
/// Fudge's sweep drum.
pub struct SweepDrum;

/// The record's `args` as raw 32-bit words, and its `frames`.
fn record(params: &Value) -> Result<([u32; 9], u32), String> {
    let args = params.get("args").and_then(Value::as_array).ok_or("no `args` array")?;
    if args.len() != 9 {
        return Err(format!("{} arguments, not 9", args.len()));
    }
    let mut a = [0u32; 9];
    for (k, x) in args.iter().enumerate() {
        a[k] = x.as_u64().filter(|&v| v <= u32::MAX as u64).ok_or(format!("argument {k} is not a 32-bit word"))? as u32;
    }
    let frames = params.get("frames").and_then(Value::as_u64).filter(|&n| n > 0 && n <= u32::MAX as u64).ok_or("no `frames`")? as u32;
    Ok((a, frames))
}

/// Argument `k` (`name`) as the stored value of a power-of-two law `x = p·scale`: in 0..1 and exact.
fn exact(a: &[u32; 9], k: usize, name: &str, scale: f32) -> Result<f32, String> {
    exact_value(f32::from_bits(a[k]), k, name, scale)
}

fn exact_value(x: f32, k: usize, name: &str, scale: f32) -> Result<f32, String> {
    let p = x / scale;
    if !(0.0..=1.0).contains(&p) || (p * scale).to_bits() != x.to_bits() {
        return Err(format!("argument {k} ({name}) = {x} is outside its law (0..{scale})"));
    }
    Ok(p)
}

/// The clock `c` (engine frames per second) as slot 1's pitch at note 60: `256·hz = c` (design D2), as octave,
/// coarse and fine.
fn pitch(p: &mut crate::patch::Patch, clock: f64) {
    use unisynth::params::old::o;
    let st = 12.0 * (clock / (unisynth::osc::engine::FRAMES_PER_CYCLE * 261.625_565_300_598_6)).log2();
    let oct = (st / 12.0).round().clamp(-4.0, 4.0);
    let coarse = (st - 12.0 * oct).round().clamp(-24.0, 24.0);
    let fine = (st - 12.0 * oct - coarse).clamp(-1.0, 1.0);
    p.osc(0, o::OCTAVE, inv::inv_octave(oct as f32));
    p.osc(0, o::COARSE, inv::inv_coarse(coarse as f32));
    p.osc(0, o::FINE, inv::inv_fine(fine as f32));
}

/// An engine instrument: slot 1 the stand-in clocked type (the old layout has no engine types: `v3_type` sets it after
/// the migration, design D5) with the engine's shape values, played like a stored sample (one voice, full level from
/// the note-on, a 1 ms release, the centred slot's √½ made up by the master).
fn instrument(shapes: &[(usize, f32)], clock: f64) -> [f32; unisynth::params::old::COUNT] {
    use unisynth::params::old::{o, osc_type, v};
    let mut p = crate::patch::Patch::new();
    p.osc(0, o::TYPE, osc_type::encode(osc_type::CLOCKED));
    for &(k, x) in shapes {
        p.shape(0, k, x);
    }
    pitch(&mut p, clock);
    p.osc(0, o::GAIN, inv::inv_gain(1.0));
    p.set(v::VELOCITY, 0.0);
    p.set(v::MASTER, inv::inv_gain(std::f32::consts::SQRT_2));
    p.set(v::CAPACITY, inv::inv_count(1, 1, 64));
    p.env(0, 0.0, 0.0, 1.0, inv::inv_env_ms(1.0));
    p.finish()
}

impl ShaperTone {
    fn shapes(params: &Value) -> Result<Vec<(usize, f32)>, String> {
        let (a, _) = record(params)?;
        Ok(vec![
            (sp::TONE_CARRIER, exact(&a, 0, "carrier period", 1024.0)?),
            (sp::TONE_MODULATOR, exact(&a, 1, "modulator period", 1024.0)?),
            (sp::TONE_DEPTH, exact(&a, 2, "depth", 64.0)?),
            (sp::TONE_VOLUME, exact(&a, 4, "volume", 256.0)?),
            (sp::TONE_DRIVE, exact(&a, 5, "drive", 1.0)?),
            (sp::TONE_SHAPE, exact(&a, 6, "shape", 1.0)?),
            // compared as an integer by the original (a raw 1 is a denormal as a float)
            (sp::TONE_FM, inv::inv_choice((a[7] != 0) as usize, 2)),
            (sp::TONE_RATE, exact(&a, 8, "rate", 16.0)?),
        ])
    }
}

impl SweepDrum {
    fn shapes(params: &Value) -> Result<Vec<(usize, f32)>, String> {
        let (a, frames) = record(params)?;
        let accel = f32::from_bits(a[3]);
        if accel > 0.0 {
            return Err(format!("argument 3 (acceleration) = {accel} is positive: the law holds a growing sweep only"));
        }
        let g = exact_value(-accel, 3, "sweep growth (−acceleration)", 1.0 / 64.0)?;
        let (env0, env1) = (f32::from_bits(a[6]) as f64, f32::from_bits(a[7]) as f64);
        let slope = (env0 - env1) / frames as f64;
        let p = ((slope * 16.0 + 1.0) / 2.0) as f32;
        if !(0.0..=1.0).contains(&p) || (unisynth::osc::engine::law::drum_slope(p) - slope).abs() > 2f64.powi(-28) {
            return Err(format!("envelope slope {slope} per frame is outside its law (±1/16)"));
        }
        if a[8] > 2 {
            return Err(format!("argument 8 (envelope curve) = {} is not 0, 1 or 2", a[8]));
        }
        Ok(vec![
            (sp::DRUM_VOLUME, exact(&a, 0, "volume", 512.0)?),
            (sp::DRUM_FREQ, exact(&a, 1, "start frequency", 4.0)?),
            (sp::DRUM_SWEEP, exact(&a, 2, "sweep", 1.0)?),
            (sp::DRUM_GROWTH, g),
            (sp::DRUM_DRIVE, exact(&a, 4, "drive", 1.0)?),
            (sp::DRUM_SHAPE, exact(&a, 5, "shape", 1.0)?),
            (sp::DRUM_ENV, exact(&a, 6, "envelope start", 1.0)?),
            (sp::DRUM_SLOPE, p),
            (sp::DRUM_CURVE, inv::inv_choice(a[8] as usize, 3)),
        ])
    }
}

impl Engine for ShaperTone {
    fn id(&self) -> &'static str {
        "fm-shaper"
    }
    fn seekable(&self, _: &Value) -> bool {
        true
    }
    fn validate(&self, params: &Value) -> Result<(), String> {
        Self::shapes(params).map(|_| ())
    }
    fn instrument(&self, params: &Value, clock: f64) -> [f32; unisynth::params::old::COUNT] {
        instrument(&Self::shapes(params).expect("validated"), clock)
    }
    fn v3_type(&self) -> Option<usize> {
        Some(unisynth::params::osc_type::SHAPER_TONE)
    }
}

impl Engine for SweepDrum {
    fn id(&self) -> &'static str {
        "sweep-drum"
    }
    fn seekable(&self, _: &Value) -> bool {
        true
    }
    fn validate(&self, params: &Value) -> Result<(), String> {
        Self::shapes(params).map(|_| ())
    }
    fn instrument(&self, params: &Value, clock: f64) -> [f32; unisynth::params::old::COUNT] {
        instrument(&Self::shapes(params).expect("validated"), clock)
    }
    fn v3_type(&self) -> Option<usize> {
        Some(unisynth::params::osc_type::SWEEP_DRUM)
    }
}

#[cfg(test)]
mod tests;
