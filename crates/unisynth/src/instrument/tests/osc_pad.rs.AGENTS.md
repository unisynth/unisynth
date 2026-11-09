# src/instrument/tests/osc_pad.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- PAD table (add-tf3-import): fundamental an octave low on the harmonics of 219.56 Hz at note 69 (band energy, not single probes), wider band spreads partials, table ignores later shape changes (built at load), `need` = one 2^16-f32 table per slot + one `Pts` per voice; ignored `load_time_of_a_pad_table`. v3 patches (own `patch`; shape 1 set to 0: it defaults to the pulse width). Needs `us-osc-pm`, `us-osc-pad`.
- See change: add-tf3-import (tasks 3.1–3.3).
