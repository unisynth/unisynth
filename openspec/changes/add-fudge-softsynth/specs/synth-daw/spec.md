# Spec Delta

## ADDED Requirements

### Requirement: Oscillator fields follow the type
An oscillator slot's panel SHALL show the fields its type reads: its shape labels, its common fields, and value text
in the type's units. A type change SHALL re-render the slot with the new type's fields. Switching a slot to an
engine type (Shaper tone, Sweep drum) SHALL write that type's default shape values in the same undo step as the type
change. Other types SHALL keep today's fields and values.

#### Scenario: Switch to a drum
- **WHEN** a slot's type changes from Phase mod to Sweep drum
- **THEN** the slot shows volume, start frequency, sweep, growth, drive, shape, envelope start, slope and curve with
  the drum's defaults, plus only coarse, fine, octave, ratio, gain, mix and pan of the common fields, and one undo
  restores the Phase mod slot with its earlier values

#### Scenario: Back to another type
- **WHEN** a Sweep drum slot changes to Pair
- **THEN** the slot shows Pair's four fields and the common fields as before

### Requirement: Engine type fields
A Shaper tone slot SHALL show the following fields, each in its unit, and a drawing of its first generated frames
from the engine's frame function:
- carrier and modulator (periods), depth, volume
- drive, shape, FM, rate

A Sweep drum slot SHALL show its nine fields and a drawing of its envelope-shaped output over its length.

#### Scenario: Converted Fudge
- **WHEN** the converted Alien Sex Clone opens on the Device tab with instrument 8's track selected
- **THEN** Osc 1 shows "Sweep drum", volume 257, start frequency 0.11 rad, curve "1 − sin" and a falling drawing
