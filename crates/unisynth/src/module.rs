//! Module ids carried by `LoadError::UnsupportedModule`: what a patch asked for that this build does not contain.
//! One table gives each id its name, its cargo feature and whether this build has it (`compiled`), so the three cannot
//! drift apart (`needs::feature` and the generated songs of `synth-convert rust` read it).

macro_rules! modules {
    ($($id:ident = $n:literal, $feature:literal, $name:literal;)*) => {
        $(pub const $id: u8 = $n;)*

        /// Every module id.
        pub const ALL: &[u8] = &[$($id),*];

        /// The cargo feature that enables module `id` (empty for an unknown id).
        pub const fn feature(id: u8) -> &'static str {
            match id {
                $($id => $feature,)*
                _ => "",
            }
        }

        /// Whether this build contains module `id` (false for an unknown id).
        pub const fn compiled(id: u8) -> bool {
            match id {
                $($id => cfg!(feature = $feature),)*
                _ => false,
            }
        }

        /// Readable name (host side: messages, converter).
        #[cfg(any(test, feature = "std"))]
        pub fn name(id: u8) -> &'static str {
            match id {
                $($id => $name,)*
                _ => "unknown module",
            }
        }
    };
}

modules! {
    OSC_PM = 1, "us-osc-pm", "phase-mod oscillator";
    OSC_BLIT = 2, "us-osc-blit", "band-limited saw/pulse oscillator";
    OSC_WAVETABLE = 3, "us-osc-wt", "wavetable oscillator";
    OSC_SAMPLER = 4, "us-osc-sample", "sampler oscillator";
    SAMPLE_GSM = 5, "us-sample-gsm", "GSM sample source";
    SAMPLE_DLS = 6, "us-sample-dls", "GM.DLS sample source";
    NOISE = 7, "us-noise", "noise source";
    FILTER_SVF = 8, "us-flt-svf", "state-variable filter";
    FILTER_LADDER = 9, "us-flt-ladder", "ladder filter";
    FILTER_RBJ = 10, "us-flt-rbj", "RBJ filter";
    FILTER_NOTCH = 11, "us-flt-notch", "resonator notch filter";
    ENV_SECONDS = 12, "us-env-seconds", "seconds-law envelope";
    ENV_RATE = 13, "us-env-rate", "rate-law envelope";
    // effect (kind, mode) pairs
    FX_DISTORTION_SHAPER = 16, "us-fx-distortion-shaper", "distortion, shaper mode";
    FX_DISTORTION_POWER = 17, "us-fx-distortion-power", "distortion, power mode";
    FX_CRUSHER = 18, "us-fx-crusher", "crusher";
    FX_EQ_PARAMETRIC = 19, "us-fx-eq-parametric", "EQ, parametric mode";
    FX_EQ_3BAND = 20, "us-fx-eq-3band", "EQ, 3band mode";
    FX_DELAY_TEMPO = 21, "us-fx-delay-tempo", "delay, tempo mode";
    FX_DELAY_MS = 22, "us-fx-delay-ms", "delay, ms mode";
    FX_CHORUS = 23, "us-fx-chorus", "chorus";
    FX_FLANGER_PHASER = 24, "us-fx-flanger-phaser", "flanger, phaser mode";
    FX_FLANGER_RING = 25, "us-fx-flanger-ring", "flanger, ring mode";
    FX_REVERB_FREEVERB = 26, "us-fx-reverb-freeverb", "reverb, freeverb mode";
    FX_REVERB_COMPACT = 27, "us-fx-reverb-compact", "reverb, compact mode";
    FX_REVERB_ROOMS = 28, "us-fx-reverb-rooms", "reverb, rooms mode";
    FX_COMPRESSOR = 29, "us-fx-compressor", "compressor";
    FX_FORMANT = 30, "us-fx-formant", "formant";
    // optional voice paths
    VOICE_LFO = 31, "us-voice-lfo", "voice LFOs";
    VOICE_GLIDE = 32, "us-voice-glide", "glide and slide";
    VOICE_MONO = 33, "us-voice-mono", "mono-legato mode";
    VOICE_UNISON = 34, "us-voice-unison", "unison";
    VOICE_ONESHOT = 35, "us-voice-oneshot", "one-shot voices";
    MATRIX = 36, "us-matrix", "mod matrix";
    AUTOMATION = 37, "us-automation", "automation lanes";
    // added after the first release of the table: ids are never renumbered
    FX_DISTORTION_CURVE = 38, "us-fx-distortion-curve", "distortion, curve mode";
    MATRIX_ADD = 39, "us-matrix-add", "mod matrix Add operation, controller sources, slots 13–16";
    SPEECH = 40, "us-speech", "formant speech stage";
    ENV_FRAME = 41, "us-env-frame", "frame-law envelope";
    OSC_NAIVE = 42, "us-osc-naive", "naive oscillator generators";
    FILTER_FEEDBACK = 43, "us-flt-feedback", "feedback-resonance and 4-pole filters";
    FILTER_PARALLEL = 44, "us-flt-parallel", "parallel filter routing";
    DRIVE = 45, "us-drive", "voice drive stage";
    LFO_FRAME = 46, "us-lfo-frame", "frame-law LFO";
    VOICE_OUT = 47, "us-voice-out", "voice output settings";
    STRIP = 48, "us-strip", "channel strip";
    FX_EQ_CUT = 49, "us-fx-eq-cut", "EQ, cut mode";
    FX_DELAY_MODULATED = 50, "us-fx-delay-modulated", "delay, modulated mode";
    FX_REVERB_COMB = 51, "us-fx-reverb-comb", "reverb, comb mode";
    FX_COMPRESSOR_LOOKAHEAD = 52, "us-fx-compressor-lookahead", "compressor, look-ahead mode";
    CHUNK_WIDE = 53, "us-chunk-wide", "chunk escapes (index 240 or more, 255 or more pairs)";
    OSC_CAULDRON = 54, "us-osc-cauldron", "mini-cauldron oscillator";
    FX_DISTORTION_WRAP = 55, "us-fx-distortion-wrap", "distortion, wrap mode";
    FX_FLANGER_COMB = 56, "us-fx-flanger-comb", "flanger, comb mode";
}

/// Whether this build contains every module in `ids`: generated songs assert it at compile time
/// (`const _: () = assert!(compiled_all(&[…]))`), so a build that lacks a module the song needs does not compile.
pub const fn compiled_all(ids: &[u8]) -> bool {
    let mut i = 0;
    while i < ids.len() {
        if !compiled(ids[i]) {
            return false;
        }
        i += 1;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_id_has_one_feature_and_a_name_and_the_ids_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for &id in ALL {
            assert!(seen.insert(id), "id {id} twice");
            assert!(feature(id).starts_with("us-") && name(id) != "unknown module", "{id}");
        }
        assert_eq!(feature(0), "");
        assert!(!compiled(0) && !compiled_all(&[0]));
        assert!(compiled_all(&[]));
    }

    #[test]
    fn compiled_follows_the_cargo_features() {
        assert_eq!(compiled(OSC_PM), cfg!(feature = "us-osc-pm"));
        assert_eq!(compiled(MATRIX), cfg!(feature = "us-matrix"));
        assert_eq!(compiled_all(&[NOISE, ENV_SECONDS]), cfg!(feature = "us-noise") && cfg!(feature = "us-env-seconds"));
    }
}
