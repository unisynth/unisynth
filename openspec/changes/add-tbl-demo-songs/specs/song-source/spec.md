## MODIFIED Requirements

### Requirement: Import from the song blob
Import SHALL read any blob `Song::parse` accepts and whose device kinds have parameter tables, and produce a song that
exports back to identical device chunks, track volumes, receives, device chains and automation bytes. Each lane SHALL
become one clip placed once at tick 0, with note times converted from samples to ticks (rounded); a note ended at the
sample where another note started just before it SHALL last one tick longer, so the export keeps the new note first.
Unsupported kinds (for example legacy kinds 0–39) SHALL fail with an error naming the kind and pointing to `synth-convert`.

#### Scenario: Blob round trip
- **WHEN** a blob of kind-64 devices is imported and exported again
- **THEN** device chunks, routing, volumes and automation bytes are identical and every note is within half a tick
  of its original sample

#### Scenario: Legato order kept
- **WHEN** a lane starts note 67 and then ends note 64 at one sample
- **THEN** the exported lane plays the note-on of 67 before the note-off of 64

#### Scenario: Legacy kind
- **WHEN** a WaveSabre blob with kind 1 (Slaughter) is imported
- **THEN** import fails with "kind 1 is a legacy device; convert the song with synth-convert first"

#### Scenario: Every engine mode imports
- **WHEN** a blob holds any effect mode the unified engine plays (the cauldron's wrap and comb included)
- **THEN** it has a parameter table and imports
