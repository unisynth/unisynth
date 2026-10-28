# Spec Delta

## Purpose

Multi-threaded offline rendering in the browser DAW: a threaded engine module used only when the page is
cross-origin isolated, with output identical to the single-thread engine and `song render`.

## ADDED Requirements

### Requirement: Threaded bounce when isolated
When the DAW page is cross-origin isolated and the threaded engine module loads, an offline bounce SHALL render the
song's independent tracks on several Web Workers. The number of workers SHALL be at least 2 and at most the browser's
reported hardware concurrency, capped by a fixed maximum.

#### Scenario: Isolated page
- **WHEN** the DAW is served with cross-origin isolation headers on a machine with 4 or more logical CPUs and a song is bounced
- **THEN** the bounce uses more than one worker

### Requirement: Identical output
A threaded bounce SHALL produce a WAV byte-identical to the single-thread bounce and to `song render` of the same song
and range.

#### Scenario: Whole song
- **WHEN** a dense project song is bounced threaded, bounced single-threaded and rendered with `song render`
- **THEN** the three WAV files are identical

#### Scenario: Loop range
- **WHEN** a loop range is bounced threaded and single-threaded
- **THEN** both outputs are identical

### Requirement: Single-thread fallback
The DAW SHALL use the current single-thread engine, with unchanged behaviour, when the page is not cross-origin
isolated, the threaded module fails to compile or instantiate, or a helper worker fails to start.

#### Scenario: Not isolated
- **WHEN** the DAW is opened in a context without cross-origin isolation (for example a sandboxed frame)
- **THEN** it loads, plays and bounces with the single-thread engine and shows no error

#### Scenario: Helper fails
- **WHEN** a helper worker cannot start during a bounce
- **THEN** the bounce completes on the single-thread engine and its output equals `song render`

### Requirement: Bounce lifecycle with helpers
Cancelling a threaded bounce SHALL stop the bounce worker and every helper worker. An error in any worker SHALL end
the bounce with an error message and stop all its workers. No worker SHALL outlive its bounce.

#### Scenario: Cancel
- **WHEN** the user cancels a threaded bounce midway
- **THEN** all workers of that bounce are terminated and a new bounce can start

### Requirement: Cross-origin isolation headers
The local server SHALL send `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy:
require-corp` on the DAW page, and `Cross-Origin-Resource-Policy: same-origin` on DAW assets. The existing foreign-
origin protections of the music API SHALL stay unchanged.

#### Scenario: Headers present
- **WHEN** `/daw/` and `/daw/synth-mt.wasm` are requested from the local server
- **THEN** the page response carries the COOP and COEP headers and the asset carries the CORP header

#### Scenario: API unchanged
- **WHEN** a foreign origin writes to `/api/music`
- **THEN** the request is refused as before

### Requirement: Default module unchanged
The single-thread engine module SHALL be built and behave as before this change; the threaded module SHALL be a
separate artifact.

#### Scenario: Same exports
- **WHEN** the single-thread module is built before and after this change
- **THEN** it exports the same functions and its bounce output for the demo song is identical
