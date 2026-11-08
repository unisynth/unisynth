# Cargo.toml (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Deps `libm`, `synth-core`, optional `gsm`/`dls`; no dependency on the original engines.
- Features: `us-osc-{pm,blit,wt,sample,naive,clocked,combine}`, `us-sample-{gsm,dls,stereo}`, `us-noise`, `us-flt-{svf,ladder,rbj,notch,feedback,parallel}`, `us-env-{seconds,rate,frame}`, `us-voice-{lfo,glide,mono,unison,oneshot}`, `us-drive us-lfo-frame us-voice-out`, `us-matrix`, `us-matrix-add`, `us-speech`, `us-strip`, `us-chunk-wide`, `us-quirk-{exact-math,integer-fm,fixed-seed,lcg-noise,coarse-env}`, `us-automation`, `us-quiet`, `us-fx-<kind>` and `us-fx-<kind>-<mode>`; groups `us-osc us-flt us-env us-voice us-quirk us-fx`, `us-all` (default);
- `us-trusted` (generated compo songs only, never default); internal `us-mono-sum`; host-only `std`, `probe` (= `std`);
- `threads` → `synth-core/threads`.
- Size gating: design D11.
