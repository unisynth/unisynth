#!/usr/bin/env python3
"""Deterministic encoder test vectors for crates/gsm (see `encoder_matches_libgsm` in src/lib.rs).

    python3 testdata/gen_enc_vectors.py enc_in.pcm            # s16le mono input
    /tmp/libgsm/enc49 < enc_in.pcm > enc_ref.gsm              # libgsm 1.0.24 -DWAV49 reference (enc49.c)

enc49.c = gsm_create + GSM_OPT_WAV49, gsm_encode per 160 samples, 32 + 33 bytes per pair, zero-padded tail.
"""
import math, struct, sys

out = []
lcg = 12345
def rnd():
    global lcg
    lcg = (lcg * 1103515245 + 12345) & 0x7FFFFFFF
    return (lcg >> 8) & 0xFFFF  # 0..65535

def clip(v): return max(-32768, min(32767, int(v)))

out += [0] * 4000                                            # silence
ph = 0.0                                                     # sine sweep 80 Hz -> 3.5 kHz, amp 12000
for i in range(20000):
    f = 80 + (3500 - 80) * i / 20000
    ph += 2 * math.pi * f / 44100
    out.append(clip(12000 * math.sin(ph)))
out += [clip(rnd() - 32768) for _ in range(16000)]           # full-scale white noise
out += [clip(rnd() % 81 - 40) for _ in range(4000)]          # very quiet noise (small dmax / norm paths)
for i in range(20000):                                       # decaying plucked tone (pitch 220 Hz + harmonics)
    t = i / 44100
    v = sum(math.sin(2 * math.pi * 220 * h * t) / h for h in (1, 2, 3, 4)) * math.exp(-t * 6)
    out.append(clip(9000 * v))
for b in range(6):                                           # full-scale square bursts, incl. -32768 / 32767
    per = 40 + 37 * b
    for i in range(2000):
        out.append(32767 if (i // per) % 2 == 0 else -32768)
    out += [0] * 300
out += [(-32768 if i % 2 else 32767) for i in range(1500)]   # Nyquist full-scale alternation
out += [clip(8000 * math.sin(i * 0.05)) + 3000 for i in range(1000)]  # DC offset sine
tail = [clip(rnd() - 32768) // 2 for _ in range(123)]        # odd-length tail (not a multiple of 320)
out += tail
open(sys.argv[1], 'wb').write(struct.pack('<%dh' % len(out), *out))
print(len(out), 'samples', len(out) % 320, 'tail')
