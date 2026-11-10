# XM softsynth intros (1998): Fudge's synth and Quad's poespack

Change `add-plain-xm-import`. Three 64K intros for MS-DOS with a Gravis Ultrasound synthesize most of their instruments
at start-up, then play an XM-like module with Niklas Beisert's Cubic tiny GUS XM player. The engines were read from
the released executables; this note describes their behaviour in our own words. No code or data of the programs is
copied here. The executables, XMs, emulation tools and local ports stay outside the repository (`$ZENEKK_DIR`, default
`~/Downloads/zenekk`).

| Intro | Release | Music | Synth |
|---|---|---|---|
| Fudge – Alien Sex Clone (`KKLONE.EXE`) | The Party 1998, 1st PC 64K (pouet 444, Demozoo 21494) | Simon (Fudge) | Fudge's own: a tone and a drum generator |
| Quad – Pure (`PURE.EXE`) | The Party 1998, 4th PC 64K (pouet 4666, Demozoo 21496) | Darkxceed (Quad) | Darkxceed's "poespack" |
| Quad – Theta (`THETA.EXE`) | Wired 1998, 1st PC 64K (pouet 4665, Demozoo 41468) | G-Day (Quad) | poespack |

## 1. Stored and generated samples

Rule for the conversions (user, 2026-11-10): a song stores only the samples the intro stored; every sample the intro
synthesized is played by live instruments.

**Method.** The intro's sample calculation runs once in an x86 emulator (unicorn) from the unpacked image, with a
write hook over the module buffer that records, for every byte, the instruction that wrote it last. Each XM sample is
then located in the buffer (exact bytes, else a 64-byte probe from its middle) and its writers tallied. A sample whose
bytes all come from a copy or unpack loop is stored PCM; one written by the generators (FPU code) is synthesized. Tools:
`$ZENEKK_DIR/re/tools/runsynth.py` (env `WHO=<buffer>`, `WHOOFF`) and `whowrote.py`.

| Intro | Stored (XM instrument) | How the intro keeps them | Generated (XM instrument) |
|---|---|---|---|
| Fudge | 7, 14, 15 (8-bit) | copied byte for byte from the module template in the executable | 1–6, 8–13, 16–25 (16-bit). Two generators write them: one for 1–6, 17, 21–23, 25; the other for 8–13, 16, 18–20, 24. Instruments 18, 19 and 24 use the very same generated data as 11, 13 and 11. |
| Theta | 1, 2, 3, 18 (8-bit) | 4-bit samples in the executable, widened to 8 bits by one unpack loop each | 4–17, 19–27 |
| Pure | 33, 34, 35 (8-bit) | 4-bit samples in the executable, widened by one shared unpack routine (two nibbles per byte) | 1–20, 22–32 (21 is empty). 32 comes from an inline 3000-step loop of the instrument code, the rest from the voice generators. |

Theta's 2, 3 and 18 and Pure's 34, 35 and 33 have the same lengths (6334, 3612, 1985 frames): the two Quad intros share
these three stored samples.

Pure was emulated here with the time-sliced runner (2 s slices), which disturbs the x87 state at each slice: its
instrument 4 then differs from the XM and is not located by its bytes. The 50 105 bytes between instruments 3 and 5,
where it lies, are all written by voice-generator stores. The uninterrupted run reproduces all 34 samples of Pure's
XM.

## 2. Provenance and packing

- The executables come from the scene.org archives `fdg_asc.zip`, `qd-pure.zip` and `qd-theta.zip` (with
  `DOS4GW.EXE`); the XMs were given to us separately, saved by FastTracker v2.00 with their titles stripped.
- All three are Watcom-compiled 32-bit LE programs, compressed with an early UPX (0.20 era, NRV). Pure and Theta carry
  the DOS/4GW stub, Alien Sex Clone PMODE/W. The UPX signature is stripped, so a modern UPX refuses them; the stub is
  instead run in an emulator until it jumps to the original entry point, and object 1 is dumped
  (`tools/xm-gen/le_unupx.py`).
- Each intro embeds Niklas Beisert's Cubic tiny GUS XM player (Fudge v1.4, Quad v1.6) and a minimal module: Pure's is
  `phase4.mxm`, Theta's `spirit4.mxm`; Theta prints "Calculating samples" and "Uploading samples" while it starts.

## 3. Method

1. Unpack (above). 2. Find the sample calculation: the code between the module load and the GUS upload. 3. Run only
that code in unicorn, from a register and memory state the skipped startup would have left: the x87 control word the
Watcom runtime sets (0x1F7F: truncating, 64-bit mantissa), the C library's random seed as `main` sets it, the DPMI
allocator's free-slot table, and (Pure) a scratch target for its progress graphics. 4. Compare the module buffer with
the XM's samples.

Pitfalls, each of which cost a wrong result before it was understood:

- **Never time-slice the emulator.** Resuming unicorn after a timeout corrupts the x87 register stack: a sliced Theta
  run differed in 512 bytes, and Pure looked like 32 of 34 matches instead of 34.
- **Truncation applies to every float store**, not only to integer conversions: a port must round each 32-bit store
  toward zero, and a C port must force values through `volatile long double`, or clang fuses the product into an SSE
  multiply that rounds to nearest.
- **Raw arguments, not floats.** Fudge's driver pushes 32-bit words; most are float bit patterns, but 0 and 1 are
  integer flags (FM on/off, envelope mode). Decoding `1` as a float (1.4e-45) silently disabled FM.
- **Carried state.** Fudge's drum generator keeps its shaper value in a stack slot that the next call inherits; the
  value at entry is part of each sample's parameters.

## 4. The engines (behaviour, in our words)

**Fudge – tone generator** (11 samples). A sine oscillator whose phase is either a slow sine sweep or a carrier
frequency-modulated by a second sine, then a two-stage shaper: below about a quarter of full scale it blends in a 4×
drive, past half scale it folds back, and a final blend mixes toward a shaper value that carries from sample to
sample. Arguments: carrier and modulator periods, modulation depth, volume, drive, shape, the FM flag and a time scale.

**Fudge – drum generator** (11 samples, 256–1556 frames). A sine whose frequency falls by a sweep that itself
accelerates, the same shaper family, and an amplitude envelope that is linear, `1 − sin` or `cos` over the call's
length (so the call's frame count matters even where the XM keeps fewer frames).

**Quad – poespack.** Darkxceed's generator (PURE.NFO: "strings, reverb, chorus, distortion, sequencer, formant"):

- a phrase voice: three wavetable oscillators (saw or square, 1024 entries) at their own ratios and gains, a fourth
  table and noise, an optional DC blocker or leaky integrator, a gate after a share of each step, a resonant filter
  with six modes whose cutoff is a base plus a low-frequency table plus a rise per sample, note-to-note slides at a
  fixed rate, and an 8-bit clip;
- a sequencer: per track, 16 steps of note, gate share and slide flag, unrolled into the voice;
- additive pads: summed saw or square partials with table vibrato, then the filter (Theta 4, 6, 8);
- a decaying sine chirp (Theta 17) and, in Pure, a short inline loop (instrument 32).

## 5. Match results

| Intro | Emulated original vs XM | Local port vs emulation |
|---|---|---|
| Fudge | 23 of 25 bit-identical, 4 and 5 within ±1 LSB (71 samples) | Python port: all 385 620 frames identical |
| Pure | 34 of 34 bit-identical (uninterrupted run) | – |
| Theta | 23 of 27 bit-identical; 4 differs in its first 82 bytes, 7 in its last byte, 6 and 8 throughout | C port: all 997 311 bytes identical |

Theta's 6 and 8 are different sounds, not errors: instrument 6 of the XM crosses zero 2209 times where the executable's
crosses 4545 times at a similar level, about an octave lower. The XM comes from a later build than the 1998 compo
executable. In the parity reference the XM's generated samples are the executable's; the two differ by 0.06 dB in
loudness and at most 2.06 dB in one octave band (`tools/unisynth-parity/plain-xm-report.md`).

## 6. Credits research

| Song | Credited music | Sources |
|---|---|---|
| Alien Sex Clone | Simon (Fudge; also known as Simn, Denmark, also in Acme) | Demozoo production 21494. Demozoo music 202313, "Alien Seckse Clone (final version)", credits Echo and Simn: its XM (Modland) shares the order list, tempo and 86 % of note cells (48 280 of 56 320) with this one, and seven samples. The executable's credit text has an "acme music maker" line. |
| Pure | Darkxceed (Quad, Netherlands) | Demozoo 21496 and PURE.NFO, which also names poespack as his softsynth |
| Theta | G-Day (Quad; Weert, Netherlands) | pouet 4665 ("gday [Music]"); Demozoo 41468 lists no credits |

Ruled out by comparing the XMs: Theta is not G-day's "Spiritual Feeling" (3rd, Wired 1998 multichannel music: 26
channels against 18, BPM 189 against 134, no pattern or sample in common), and Pure is not Darkxceed's "Knock Down
Your Brains" (9th, The Party 1998 music: no pattern in common, one shared sample). Neither Quad module has a title;
the intros name them `phase4.mxm` and `spirit4.mxm`.
