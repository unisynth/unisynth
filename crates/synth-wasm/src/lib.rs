//! DAW engine module (change add-synth-daw, spec synth-live-host): the live [`Host`] and the `song` crate's
//! validation/export/import/presets/tables, behind a plain C ABI like `crates/wasm` (no wasm-bindgen). One module
//! serves the AudioWorklet (host functions), the main thread (song functions) and the bounce Worker.
//!
//! Byte inputs go through `alloc`/`dealloc`; replies (JSON or bytes) through `out_ptr`/`out_len`. Render returns a
//! pointer to interleaved f32 the host owns. wasm32 is single-threaded and the exports are not re-entrant.

mod host;
#[cfg(feature = "threads")]
mod par;

pub use host::{Edit, Host, MAX_FRAMES};

use serde_json::{json, Value};
use song::tables;

/// Parameter tables of every kind and effect mode (D8): `[{kind, mode, label, params: [{index, name, group,
/// default, law, unit, min, max, options, structural, raw}]}]`.
pub fn tables_json() -> Value {
    Value::Array(
        tables::tables()
            .iter()
            .map(|t| {
                json!({
                    "kind": t.kind, "mode": t.mode, "label": t.label,
                    "params": t.params.iter().map(|p| json!({
                        "index": p.index, "name": p.name, "group": p.group, "default": p.default,
                        "law": format!("{:?}", p.law), "unit": p.unit, "min": p.min, "max": p.max,
                        "options": p.options, "structural": p.structural, "raw": p.raw,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// Physical value of normalized `p` for `(kind, mode, index)` (display text in the UI).
pub fn physical(kind: u8, mode: usize, index: usize, p: f32) -> f32 {
    tables::table(kind, mode).and_then(|t| t.params.iter().find(|x| x.index == index)).map_or(p, |x| x.physical(p))
}

fn issues(v: &[song::Issue]) -> Value {
    json!({ "ok": false, "errors": v })
}

/// A GSM sample chunk (as stored in a device's `samples`) → mono f32 PCM at 44.1 kHz.
pub fn gsm_chunk_decode(c: &[u8]) -> Result<Vec<f32>, String> {
    let g = gsm::GsmChunk::parse(c).map_err(|e| format!("not a GSM sample: {e:?}"))?.ok_or("empty GSM sample")?;
    let mut out = vec![0f32; g.samples];
    let n = gsm::decode(g.blocks, &mut out).map_err(|e| format!("{e:?}"))?;
    out.truncate(n);
    Ok(out)
}

/// Mono f32 PCM at 44.1 kHz → a GSM sample chunk (header + WAV49 blocks), ready for a device's `samples`.
pub fn gsm_chunk_encode(pcm: &[f32]) -> Vec<u8> {
    let mut blocks = vec![0u8; gsm::blocks_for(pcm.len()) * gsm::BLOCK_BYTES];
    let n = gsm::encode(pcm, &mut blocks);
    blocks.truncate(n);
    let mut head = [0u8; gsm::CHUNK_HEADER];
    gsm::write_chunk_header(n, pcm.len(), &mut head);
    let mut c = head.to_vec();
    c.extend_from_slice(&blocks);
    c
}

// ---- C ABI ----

static mut HOST: Option<Host> = None;
static mut OUT: Vec<u8> = Vec::new();

#[allow(static_mut_refs)]
fn host() -> &'static mut Host {
    // SAFETY: single-threaded wasm32; exports are not re-entrant.
    unsafe { HOST.get_or_insert_with(Host::new) }
}

#[allow(static_mut_refs)]
fn reply(v: Value) -> u32 {
    let ok = v.get("ok").and_then(Value::as_bool).unwrap_or(true);
    unsafe { OUT = serde_json::to_vec(&v).unwrap_or_default() };
    ok as u32
}

#[allow(static_mut_refs)]
fn reply_bytes(b: Vec<u8>) -> u32 {
    unsafe { OUT = b };
    1
}

/// # Safety
/// `p` must point to `n` readable bytes previously obtained from `alloc`.
unsafe fn bytes<'a>(p: *const u8, n: usize) -> &'a [u8] {
    if n == 0 { &[] } else { unsafe { std::slice::from_raw_parts(p, n) } }
}

/// # Safety
/// See [`bytes`].
unsafe fn text(p: *const u8, n: usize) -> String {
    String::from_utf8_lossy(unsafe { bytes(p, n) }).into_owned()
}

fn layout(n: usize) -> std::alloc::Layout {
    std::alloc::Layout::array::<u8>(n.max(1)).expect("allocation size")
}

#[unsafe(no_mangle)]
pub extern "C" fn alloc(n: usize) -> *mut u8 {
    // SAFETY: layout has non-zero size.
    unsafe { std::alloc::alloc(layout(n)) }
}

/// # Safety
/// `p`/`n` must come from a prior `alloc(n)` with the same `n`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dealloc(p: *mut u8, n: usize) {
    unsafe { std::alloc::dealloc(p, layout(n)) }
}

#[unsafe(no_mangle)]
#[allow(static_mut_refs)]
pub extern "C" fn out_ptr() -> *const u8 {
    unsafe { OUT.as_ptr() }
}

#[unsafe(no_mangle)]
#[allow(static_mut_refs)]
pub extern "C" fn out_len() -> usize {
    unsafe { OUT.len() }
}

// -- live host (AudioWorklet) --

/// # Safety
/// See [`bytes`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn set_bank(p: *const u8, n: usize) {
    host().set_bank(unsafe { bytes(p, n) }.to_vec());
}

/// Loads a song blob. 1 = ok; 0 = error JSON in OUT.
///
/// # Safety
/// See [`bytes`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn load(p: *const u8, n: usize) -> u32 {
    match host().load(unsafe { bytes(p, n) }) {
        Ok(()) => reply(json!({ "ok": true, "tracks": host().peaks().len() })),
        Err(e) => reply(json!({ "ok": false, "errors": [{ "path": "", "message": e }] })),
    }
}

/// Applies an edited song blob; OUT = `{ok, params, rebuilt, schedules, mixer, reload}`.
///
/// # Safety
/// See [`bytes`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn update(p: *const u8, n: usize) -> u32 {
    match host().update(unsafe { bytes(p, n) }) {
        Ok(e) => reply(json!({ "ok": true, "params": e.params, "rebuilt": e.rebuilt, "schedules": e.schedules, "mixer": e.mixer, "reload": e.reload })),
        Err(e) => reply(json!({ "ok": false, "errors": [{ "path": "", "message": e }] })),
    }
}

/// Renders `frames` (≤ 4096) frames; returns a pointer to `2·frames` interleaved f32.
#[unsafe(no_mangle)]
pub extern "C" fn render(frames: usize) -> *const f32 {
    host().render(frames).as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn play(frame: f64) {
    host().play(frame.max(0.0) as u64);
}

#[unsafe(no_mangle)]
pub extern "C" fn stop() {
    host().stop();
}

#[unsafe(no_mangle)]
pub extern "C" fn seek(frame: f64) {
    host().seek(frame.max(0.0) as u64);
}

#[unsafe(no_mangle)]
pub extern "C" fn set_loop(start: f64, end: f64, on: u32) {
    host().set_loop((on != 0).then_some((start.max(0.0) as u64, end.max(0.0) as u64)));
}

#[unsafe(no_mangle)]
pub extern "C" fn position() -> f64 {
    host().position() as f64
}

#[unsafe(no_mangle)]
pub extern "C" fn is_playing() -> u32 {
    host().playing() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn note_on(track: u32, note: u32, velocity: u32) {
    host().note_on(track as usize, note.min(255) as u8, velocity.min(127) as u8);
}

#[unsafe(no_mangle)]
pub extern "C" fn note_off(track: u32, note: u32) {
    host().note_off(track as usize, note.min(255) as u8);
}

#[unsafe(no_mangle)]
pub extern "C" fn set_param(track: u32, device: u32, param: u32, value: f32) {
    host().set_param(track as usize, device as usize, param as usize, value);
}

/// Per-track peaks of the last render call: `[L0, R0, L1, R1, …]`, master last; length in floats via `peaks_len`.
#[unsafe(no_mangle)]
pub extern "C" fn peaks_ptr() -> *const f32 {
    host().peaks().as_ptr() as *const f32
}

#[unsafe(no_mangle)]
pub extern "C" fn peaks_len() -> usize {
    host().peaks().len() * 2
}

#[unsafe(no_mangle)]
pub extern "C" fn load_ratio() -> f32 {
    host().load_ratio()
}

// -- song functions (main thread) --

/// OUT = parameter tables JSON.
#[unsafe(no_mangle)]
pub extern "C" fn song_tables() -> u32 {
    reply(tables_json())
}

#[unsafe(no_mangle)]
pub extern "C" fn song_physical(kind: u32, mode: u32, index: u32, p: f32) -> f32 {
    physical(kind as u8, mode as usize, index as usize, p)
}

/// Validates a song JSON; OUT = `{ok, errors: [{path, message}]}`.
///
/// # Safety
/// See [`bytes`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn song_validate(p: *const u8, n: usize) -> u32 {
    match song::load(&unsafe { text(p, n) }) {
        Ok(_) => reply(json!({ "ok": true, "errors": [] })),
        Err(e) => reply(issues(&e)),
    }
}

/// Exports a song JSON; 1 = OUT holds the blob bytes, 0 = OUT holds the issues JSON.
///
/// # Safety
/// See [`bytes`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn song_export(p: *const u8, n: usize) -> u32 {
    match song::parse(&unsafe { text(p, n) }).and_then(|s| song::export(&s)) {
        Ok(b) => reply_bytes(b),
        Err(e) => {
            reply(issues(&e));
            0
        }
    }
}

/// Imports a blob; OUT = song JSON (1) or `{ok:false, errors}` (0).
///
/// # Safety
/// See [`bytes`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn song_import(p: *const u8, n: usize) -> u32 {
    match song::import(unsafe { bytes(p, n) }) {
        Ok(s) => reply_bytes(song::to_json(&s).into_bytes()),
        Err(e) => reply(json!({ "ok": false, "errors": [{ "path": "", "message": e }] })),
    }
}

/// Saves device `{device, name}` JSON as a preset; OUT = preset JSON.
///
/// # Safety
/// See [`bytes`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn preset_save(p: *const u8, n: usize) -> u32 {
    #[derive(serde::Deserialize)]
    struct In {
        device: Value,
        name: String,
    }
    let parsed = serde_json::from_str::<In>(&unsafe { text(p, n) })
        .map_err(|e| e.to_string())
        .and_then(|i| serde_json::from_value::<song::model::Device>(i.device).map(|d| (d, i.name)).map_err(|e| e.to_string()));
    match parsed {
        Ok((d, name)) => reply_bytes(song::preset::to_json(&song::preset::save(&d, &name)).into_bytes()),
        Err(e) => reply(json!({ "ok": false, "errors": [{ "path": "", "message": e }] })),
    }
}

/// Parses a preset; OUT = the device JSON it loads into (1) or issues (0).
///
/// # Safety
/// See [`bytes`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn preset_load(p: *const u8, n: usize) -> u32 {
    match song::preset::parse(&unsafe { text(p, n) }) {
        Ok(pr) => reply(serde_json::to_value(song::preset::load(&pr)).unwrap_or_default()),
        Err(e) => reply(issues(&e)),
    }
}

// -- bounce (Worker): the exported blob through the song player, in steps (progress, cancel) --

/// A song player with the memory it borrows (field order = drop order: player first).
struct Bouncer {
    player: synth_core::mix::Player<'static, unisynth::UDevice<'static>, unisynth::rng::Xorshift>,
    _mem: Box<[u8]>,
    _blob: Box<[u8]>,
    _bank: Option<Box<[u8]>>,
    left: u64,
    buf: Vec<f32>,
}

static mut BOUNCER: Option<Bouncer> = None;

fn bouncer(blob: &[u8], bank: Option<&[u8]>) -> Result<Bouncer, String> {
    use synth_core::mix::{Env, Player, Song, DEFAULT_SUB};
    let blob: Box<[u8]> = blob.into();
    let bank: Option<Box<[u8]>> = bank.map(Into::into);
    // SAFETY: the boxes never move and are dropped after the player (Bouncer field order).
    let b: &'static [u8] = unsafe { &*(&*blob as *const [u8]) };
    let k: Option<&'static [u8]> = bank.as_ref().map(|x| unsafe { &*(&**x as *const [u8]) });
    let s = Song::parse(b).map_err(|e| format!("not a valid song blob: {e:?}"))?;
    let env = Env::for_song(&s, k);
    let need = Player::<unisynth::UDevice, unisynth::rng::Xorshift>::required_pool(&s, &env, DEFAULT_SUB).map_err(|e| format!("cannot load: {e:?}"))?;
    let mut mem = vec![0u8; need].into_boxed_slice();
    let m: &'static mut [u8] = unsafe { &mut *(&mut *mem as *mut [u8]) };
    let mut pool = synth_core::pool::Pool::new(m);
    let player = Player::new(s, &env, &mut pool, unisynth::rng::Xorshift::new(song::render::SEED), DEFAULT_SUB).map_err(|e| format!("cannot load: {e:?}"))?;
    let left = player.length_frames();
    Ok(Bouncer { player, _mem: mem, _blob: blob, _bank: bank, left, buf: vec![0.0; MAX_FRAMES * 2] })
}

/// Starts a bounce of a song blob (D7: the exact intro sound, equal to `song render`); returns the total frames
/// (0 on error, issues in OUT). Uses the GM bank set with `set_bank`.
///
/// # Safety
/// See [`bytes`].
#[unsafe(no_mangle)]
#[allow(static_mut_refs)]
pub unsafe extern "C" fn bounce_open(p: *const u8, n: usize) -> f64 {
    let bank = host().bank_bytes().map(|b| b.to_vec());
    unsafe { BOUNCER = None };
    match bouncer(unsafe { bytes(p, n) }, bank.as_deref()) {
        Ok(b) => {
            let total = b.left as f64;
            unsafe { BOUNCER = Some(b) };
            total
        }
        Err(e) => {
            reply(json!({ "ok": false, "errors": [{ "path": "", "message": e }] }));
            0.0
        }
    }
}

/// Renders the next `min(frames, 4096, left)` frames of the bounce; returns a pointer to them (interleaved f32);
/// `bounce_left` tells how many remain.
#[unsafe(no_mangle)]
#[allow(static_mut_refs)]
pub extern "C" fn bounce_step(frames: usize) -> *const f32 {
    let Some(b) = (unsafe { BOUNCER.as_mut() }) else { return std::ptr::null() };
    let n = (frames.min(MAX_FRAMES) as u64).min(b.left) as usize;
    b.player.render_f32(&mut b.buf[..2 * n]);
    b.left -= n as u64;
    b.buf.as_ptr()
}

#[unsafe(no_mangle)]
#[allow(static_mut_refs)]
pub extern "C" fn bounce_left() -> f64 {
    unsafe { BOUNCER.as_ref().map_or(0.0, |b| b.left as f64) }
}

/// GM bank summary for the library (D8b): `{waves, instruments: [{name, drums, bank, prog, regions: [[lo, hi, wave]]}]}`.
pub fn bank_info(bank: &[u8]) -> Result<Value, String> {
    let d = dls::Dls::parse(bank).map_err(|e| format!("not a DLS bank: {e:?}"))?;
    let ins: Vec<Value> = d
        .instruments()
        .map(|i| {
            json!({
                "name": i.name.map(String::from_utf8_lossy).unwrap_or_default().trim().trim_end_matches('\0').to_string(),
                "drums": i.drums, "bank": i.bank_msb, "prog": i.program,
                "regions": i.regions().map(|r| json!([r.key_lo, r.key_hi, r.wave])).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({ "waves": d.wave_count(), "instruments": ins }))
}

/// OUT = [`bank_info`] JSON (1) or `{ok:false, errors}` (0).
///
/// # Safety
/// See [`bytes`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn song_bank_info(p: *const u8, n: usize) -> u32 {
    match bank_info(unsafe { bytes(p, n) }) {
        Ok(v) => reply(v),
        Err(e) => reply(json!({ "ok": false, "errors": [{ "path": "", "message": e }] })),
    }
}

/// OUT = the decoded sample as little-endian f32 (1), or `{ok:false, errors}` (0).
///
/// # Safety
/// See [`bytes`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gsm_decode(p: *const u8, n: usize) -> u32 {
    match gsm_chunk_decode(unsafe { bytes(p, n) }) {
        Ok(pcm) => reply_bytes(pcm.iter().flat_map(|x| x.to_le_bytes()).collect()),
        Err(e) => reply(json!({ "ok": false, "errors": [{ "path": "", "message": e }] })),
    }
}

/// OUT = a GSM sample chunk encoded from `n` little-endian f32 samples at `p` (mono, 44.1 kHz).
///
/// # Safety
/// `p` must point to `4 * n` readable bytes previously obtained from `alloc` (any alignment).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gsm_encode(p: *const u8, n: usize) -> u32 {
    let pcm: Vec<f32> = unsafe { bytes(p, 4 * n) }.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    reply_bytes(gsm_chunk_encode(&pcm))
}
