//! Effect devices: a legacy effect chunk becomes the unified chunk of its (kind, mode) by putting the mode in front;
//! its parameters and its automation keep their meaning (automation index + 1).

use unisynth::kind as u;
use unisynth::law::host as inv;
use wavesabre::kind as w;
use tfsynth::device::kind as t;

/// `(unified kind, mode)` of a legacy effect kind, or `None` for anything else.
pub fn target(kind: u8) -> Option<(u8, usize)> {
    Some(match kind {
        w::SCISSOR => (u::DISTORTION, 0),
        w::SCREWDRIVER => (u::DISTORTION, 1),
        w::LEVELLER => (u::EQ, 0),
        w::CRUSHER => (u::CRUSHER, 0),
        w::ECHO => (u::DELAY, 0),
        w::SMASHER => (u::COMPRESSOR, 0),
        w::CHAMBER => (u::REVERB, 2),
        w::TWISTER => (u::FLANGER, 0),
        w::CATHEDRAL => (u::REVERB, 0),
        t::DISTORTION => (u::DISTORTION, 2),
        t::DELAY => (u::DELAY, 1),
        t::CHORUS => (u::CHORUS, 0),
        t::FLANGER => (u::FLANGER, 1),
        t::REVERB => (u::REVERB, 1),
        t::FORMANT => (u::FORMANT, 0),
        t::EQ => (u::EQ, 1),
        _ => return None,
    })
}

/// The unified chunk: `[mode, legacy params…]` and the legacy trailer. A legacy chunk under 4 bytes sets nothing, so
/// it becomes the mode alone.
pub fn chunk(unified_kind: u8, mode: usize, legacy: &[u8]) -> Vec<u8> {
    let p = inv::inv_choice(mode, unisynth::fx::mode_count(unified_kind));
    let mut c = p.to_le_bytes().to_vec();
    if legacy.len() < 4 {
        c.extend([0u8; 4]);
    } else {
        c.extend(legacy);
    }
    c
}
