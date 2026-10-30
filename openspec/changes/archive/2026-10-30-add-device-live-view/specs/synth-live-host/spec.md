# Spec Delta

## ADDED Requirements

### Requirement: Watched instrument probe
The host SHALL let the editor watch at most one track's instrument. While a track is watched, each status report SHALL
include that instrument's voice snapshot (per voice: identity, note, amplitude, released flag, envelope levels and
stages, LFO phases, sampler positions, modulated filter cutoffs) and the last 2048 frames of each active filter's
input and output summed over voices. With no track watched the host SHALL do no probe work.

#### Scenario: Watch the lead
- **WHEN** the editor watches the lead track and a chord plays on it
- **THEN** the next status reports one voice entry per sounding lead voice and the filter taps of its active filters

#### Scenario: Unwatched
- **WHEN** no track is watched
- **THEN** status reports carry no probe data and the render time is at most 5 % above the same engine built without the probe (the player build has no probe code)

### Requirement: Probe leaves the sound unchanged
Watching a track SHALL NOT change any rendered sample.

#### Scenario: Bounce while watching
- **WHEN** a song is rendered by the live host with a track watched and without
- **THEN** both renders are bit-identical and equal `song render`
