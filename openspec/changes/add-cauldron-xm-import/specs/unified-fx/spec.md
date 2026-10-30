# Spec Delta

## ADDED Requirements

### Requirement: Comb flanger mode
The flanger kind SHALL gain a mode **comb** (`us-fx-flanger-comb`) with a delay K in frames at the song rate and an
amount g. With sweep rate 0 it outputs `y[n] = x[n] − g·x[n − K]`. The mode SHALL be stored at a free position of the
flanger's expanded mode choice, so the stored values of phaser and ring keep their meaning.

#### Scenario: Difference comb
- **WHEN** a sine at the frequency `rate/K` runs through comb with g = 1 and no sweep
- **THEN** the output is silent after the first K frames

### Requirement: Comb sweep
The comb mode SHALL have a sweep rate s and a restart parameter. With s above 0 it outputs
`(x[n − K] + x[n − K + s·t])/2`, where t counts frames since the last restart. A step of the restart parameter SHALL
set t to 0 at that frame.

#### Scenario: Sweep restarts
- **WHEN** the restart parameter steps during playback
- **THEN** the sweep's read offset returns to 0 at that frame

### Requirement: Wrap distortion mode
The distortion kind SHALL gain a mode **wrap** (`us-fx-distortion-wrap`) with a drive and an offset. It outputs
`v = x·drive + offset` wrapped into −1..1 (`((v + 1) mod 2) − 1`). The mode SHALL be stored at a free position of the
distortion's expanded mode choice, so the stored values of shaper, curve and power keep their meaning.

#### Scenario: Wrap-around
- **WHEN** a sample of value 0.9 passes through wrap with drive 1.25 and offset 0
- **THEN** the output is 1.125 − 2 = −0.875

#### Scenario: Old distortion values unchanged
- **WHEN** an existing song with a power-mode distortion loads in a build that has the wrap mode
- **THEN** the effect still runs power mode, and the song renders byte-identical to its golden
