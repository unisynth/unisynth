# Spec Delta

## Purpose

Turns a local audio file into per-stem note data that a synth can play, in a stable JSON contract.

## ADDED Requirements

### Requirement: Stem separation
The `separate` stage SHALL decode the input with ffmpeg to 44.1 kHz stereo wav and write stems `bass`, `drums`, `vocals`, `other` under a cache directory keyed by the input's sha256. A stem already in the cache SHALL NOT be recomputed. A `music-map/1` file, if given, SHALL supply the tempo and bar grid; otherwise the stage derives them with the librosa tier.

#### Scenario: Cache hit
- **WHEN** the same file is processed twice
- **THEN** the second run does no separation and returns the same stem paths

#### Scenario: Missing dependency
- **WHEN** demucs is not importable
- **THEN** the stage exits with a non-zero code and a one-line install hint, and writes no partial output

### Requirement: Note extraction
The `transcribe` stage SHALL write `note-map/1` with `{schema, source_sha256, tempo_bpm, meter, tracks[]}`. Each track has `{stem, role: bass|lead|chords|drums, notes[]}` and each note `{pitch (MIDI int), start_s, dur_s, start_beat, dur_beat, bar (1-based), velocity (1..127), confidence (0..1)}`. Bass tracks SHALL be monophonic (no overlapping notes). Note times SHALL be taken from the audio, never snapped to the grid in the file; a separate `quantized` flag applies only if `--quantize` is given.

#### Scenario: Monophonic bass
- **WHEN** a synthetic bass line with 4 known notes is transcribed
- **THEN** exactly 4 notes are returned with pitches equal to the input and onsets within 30 ms

#### Scenario: Lead melody
- **WHEN** a synthetic lead with a 5-note melody over silence is transcribed
- **THEN** pitch accuracy is at least 90% of notes and onset error is under 50 ms

#### Scenario: Silent stem
- **WHEN** a stem has no energy above -60 dBFS
- **THEN** the track has an empty `notes` array and a `warnings` entry, and no error is raised

### Requirement: Quality gate
The stage SHALL drop notes shorter than 40 ms or with confidence below `--min-conf` (default 0.3), merge repeated detections of one pitch with a gap under 30 ms, and report the dropped count.

#### Scenario: Octave error
- **WHEN** the detector reports a bass note one octave above the dominant spectral peak of its stem segment
- **THEN** the note is corrected to the octave of the peak and flagged `octave_fixed`
