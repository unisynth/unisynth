# v2-synth.md (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## Sections
- §1 What exists: only one V2 synth; "v1/v2/v3" = `.v2m` format versions 0–6 (param/global counts, added fields, oracle binaries). Sources: fr_public `v2/` (public domain), `spheenik/v2redux` (CC0 version-native reference player `v2dump`, carving toolkit), v2m-player/modland songs (© authors, local only). `.v2m` layout after upgrade to v6.
- §2 Corpus survey: 359 songs (v5 296, v6 40), feature counts.
- §3 Gap matrix vs `unisynth` (V2 osc/FM/ring, parallel filters + all-pass/Moog, voice dist, sustain-slope EG, S+H/one-shot/polarity LFO, additive matrix max 14 entries, channel strip, V2 reverb/delay, aux osc, Ronan).
- §4 Phased plan `add-v2-import`.
- §5 Outcome (2026-10-28): all 15 acceptance songs pass vs v2redux (loudness ≤ 0.9 dB, worst band 1.8 dB fr027 at 16 kHz, onset correlation 0.82–0.99); what parity needed; aux buses; known gap `Quickyman--tristar_bp2008-unsuspecting` (−1.7 dB, 63 Hz −5.5 dB); V2 songs 27.6–35.3 KB squishy; gated cost +180…+470 B xz.
- §6 Quirk survey (2026-11-01, unify-v2-algorithms 6.3) over 347 converted songs: kept `coarse_env`, `lcg_noise`, `exact_math`, `integer_fm`, `fixed_seed`; dropped `boxed_osc`, `dc_bias`, `filter bias`.

## History
See change: add-v2-import, unify-v2-algorithms.
