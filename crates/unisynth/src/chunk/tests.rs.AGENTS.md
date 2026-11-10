# src/chunk/tests.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Sparse/canonical round trip, defaults = 3 bytes, worst case 1529 bytes, truncation at every byte, version/index/order/value/trailing rejection, sample entry checks, hostile sizes, GSM index rule, `(int)` sample index, high-index escapes + canonical escaped counts, escapes need `us-chunk-wide`, speech text (v4: round trip, after the samples).

## History

- See change: add-v2-import, unify-v2-algorithms 6.5–6.8
- Worst case 1751 bytes (331 values: 240 one-byte, 91 two-byte pairs). See change: add-elevated-import.
