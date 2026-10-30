"""Reference re-implementation of "mini-cauldron" v1 (Probe / TBL, Jizz 1997).

Bit-exact against the samples the original x86 code writes into jizz.xm
(verified by tools/verify.py against the emulated intro).

Usage:  python mcauldron1.py patches_jizz.json out_dir
Writes one raw signed-8-bit PCM file per render (<sample name>.s8, 28000 bytes,
NOT delta coded) plus renders.json (name, record, variant).

Structure (see docs/mini-cauldron.md for the prose spec):
  patch (123-byte record) -> variants (1..8) -> 16 sequencer steps x 3530
  frames at the internal rate -> echo over the whole 56480-frame phrase ->
  2:1 decimation (no filter) -> 28000 signed 8-bit samples.
"""
import json
import os
import sys

from x87pc24 import add, sub, mul, div, fistp

STEP = 3530           # frames per sequencer step (loop 0..0xDC9 inclusive)
ECHO_STRIDE = 3529    # 0xDC9: echo addresses step s at s*3529 (one frame overlap)
GLIDE = 1764          # 0x6E4: glide window in frames
C = 3529.0            # qword at 0x4110D5
DETUNE = 1.02         # qword at 0x41136E
ACCENT = 4.0          # dword float at 0x41159A
FULL = 32767.0        # qword at 0x411324
OUT_LEN = 28000
BUF_LEN = 70000


def i16(x):
    x &= 0xFFFF
    return x - 0x10000 if x & 0x8000 else x


def i8(x):
    x &= 0xFF
    return x - 0x100 if x & 0x80 else x


def rol(x, n):
    return ((x << n) | (x >> (32 - n))) & 0xFFFFFFFF


def s32(x):
    x &= 0xFFFFFFFF
    return x - (1 << 32) if x & 0x80000000 else x


class Rng:
    """TBL's shared 3x32-bit RNG (code at 0x40F640, state at 0x40F634)."""

    def __init__(self, state):
        self.s = list(state)

    def next(self):
        s = self.s
        eax = (s[0] + 0x0E372471) & 0xFFFFFFFF
        ebx = (s[1] + 0x00BC5EA9) & 0xFFFFFFFF
        ecx = (s[2] + 0x00C8ADE7) & 0xFFFFFFFF
        eax ^= ebx
        p = s32(eax) * s32(ecx)                    # imul ecx (signed 32x32->64)
        eax = p & 0xFFFFFFFF
        edx = (p >> 32) & 0xFFFFFFFF
        edx = rol(edx, 13)
        edx = (edx + eax) & 0xFFFFFFFF
        ecx = rol(ecx, 14)
        edx = (edx - ebx) & 0xFFFFFFFF
        eax ^= edx
        ebx = (ebx + ecx) & 0xFFFFFFFF
        ecx ^= edx
        eax = (eax - ebx) & 0xFFFFFFFF
        ebx = rol(ebx, 9)
        p = eax * ecx                              # mul ecx (unsigned)
        eax = p & 0xFFFFFFFF
        edx = p >> 32
        ebx ^= edx
        eax = (eax + 0x01E42AC3) & 0xFFFFFFFF
        ebx = (ebx + 0x005EDF04) & 0xFFFFFFFF
        ecx = (ecx + 0x011A2EC1) & 0xFFFFFFFF
        s[0], s[1], s[2] = ebx, ecx, eax
        return eax


def shape7(ph):
    """0x411264 inner: x = phase (int16); t = x>0 ? (32767-x)/32767 : x/32767; 32767*t^7."""
    x = float(ph)
    if ph > 0:
        x = add(-x, FULL)
    t = div(x, FULL)
    t2 = mul(t, t)
    t4 = mul(t2, t2)
    t6 = mul(t4, t2)
    return mul(mul(t6, FULL), t)       # (t^6*32767)*t, rounded per op


class Synth:
    def __init__(self, rng_state):
        self.rng = Rng(rng_state)
        self.buf = bytearray(BUF_LEN)     # persistent work buffer (signed bytes)
        self.seg_reg = 0                  # word 0x41110F: last step index (survives renders)
        self.t = 0                        # dword 0x4110E1: frame counter (1-based on write)

    # ---- 0x411501: (re)trigger -------------------------------------------
    def retrigger(self, P, st):
        st['prev'] = 0                                    # 0x411109
        st['phase'] = i16(0x8002)                         # 0x4110E5
        m = P['mul']
        P['B'] = mul(m, P['B'])                           # compounding: stored back
        st['v2'] = P['B']
        P['A'] = mul(m, P['A'])
        st['v1'] = P['A']
        st['q'] = P['damp']                               # exact qword copy
        st['d'] = P['decay']
        s = self.seg_reg
        if P['raw'][0x5B + s] == 1:                       # accent[s] (s may be 16 -> tie[0])
            st['v2'] = mul(st['v2'], ACCENT)
            st['v1'] = mul(st['v1'], ACCENT)
        st['pitch'] = P['raw'][0x3B + 2 * s] | (P['raw'][0x3C + 2 * s] << 8)  # pitch[s] (s=16 -> accent bytes)

    # ---- 0x4111A2: filter --------------------------------------------------
    @staticmethod
    def filt(st, x):
        st['v1'] = mul(st['v1'], st['d'])
        c = add(st['v1'], st['v2'])
        y = sub(st['y'], mul(float(x + st['prev']), c))   # fild x; fiadd prev (exact)
        y = mul(y, sub(1.0, st['q']))
        st['y'] = y
        return fistp(add(y, float(st['prev'])))

    # ---- 0x41132C: one frame ------------------------------------------------
    def frame(self, P, st, n):
        s = self.seg_reg
        out = st['phase']                                  # movsx word phase
        if P['voices'] != 0:
            st['phase2'] = i16(st['phase2'] + fistp(mul(float(s32(st['pitch'])), DETUNE)))   # fild dword: signed
            out += st['phase2']
        if P['wave'] == 2:
            out = 0
            V = P['voices']
            k = 0
            while True:                                   # quirk: result = base + voice(V)
                out = fistp(shape7(st['phase']))
                if V != 0:
                    off = ((self.t * k) & 0xFFFFFFFF) * (st['pitch'] & 0xFFFFFFFF)   # mul dword: unsigned
                    off = (off >> 7) & 0xFFFFFFFF
                    if not (k & 1):
                        off = (-off) & 0xFFFFFFFF
                    st['phase2'] = i16(st['phase'] + off)
                    out = fistp(add(shape7(st['phase2']), float(out)))
                k += 1
                if k > V:
                    break
        elif P['wave'] == 4:
            r = self.rng.next() & 0xFFFFFF
            out = r % 0xFA01 - 0x7D00
        # pitch: glide on tied steps during the first 1764 frames
        if s != 0 and P['tie'][s] == 1:
            if n < GLIDE:
                n2 = float(2 * n)
                a = mul(div(sub(C, n2), C), float(i16(P['pitch'][s - 1])))   # fimul word: signed
                b = mul(div(n2, C), float(i16(P['pitch'][s])))
                st['pitch'] = fistp(add(a, b))
            else:
                st['pitch'] = P['pitch'][s]
        st['phase'] = i16(st['phase'] + st['pitch'])
        x = fistp(mul(float(out), P['gain']))
        y = self.filt(st, x)
        self.t += 1
        st['prev'] = y
        if s + 1 <= 15:
            nxt_tie = P['tie'][s + 1]
        else:
            nxt_tie = 0xDD                                # reads first code byte -> never fades
        if nxt_tie == 0:                                  # step not tied into next: linear fade-out
            y = fistp(mul(sub(1.0, div(float(n), C)), float(y)))
        y = max(-32767, min(32767, y))
        self.buf[self.t] = (y >> 8) & 0xFF                # high byte of the 16-bit value

    def segments(self, P, st):
        for s in range(16):
            self.seg_reg = s
            if P['tie'][s] == 0:
                self.retrigger(P, st)
            for n in range(STEP):
                self.frame(P, st, n)
        self.seg_reg = 16

    def echo(self, P):
        b = self.buf
        fb = P['echo']
        for e in range(16):
            for k in range(ECHO_STRIDE + 1):
                i = e * ECHO_STRIDE + k
                cur = i8(b[i])
                src = i8(b[i - 3 * ECHO_STRIDE]) if e >= 3 else i8(b[i + 13 * ECHO_STRIDE])
                v = cur + fistp(mul(float(src), fb))
                if v > 127:
                    v = 127
                elif v < -127:
                    v = -127
                b[i] = v & 0xFF

    def post(self, record_index):
        b = self.buf
        if record_index == 7:                             # hard-coded: halve record 7
            for i in range(58000):
                b[i] = (i8(b[i]) >> 1) & 0xFF
        for i in range(OUT_LEN + 1):                       # 2:1 decimation, no filter
            b[i] = b[2 * i]
        self.t = 0
        return bytes(b[:OUT_LEN])

    def render_patch(self, rec, record_index):
        P = dict(rec)
        P['raw'] = bytes.fromhex(rec['raw'])
        st = dict(phase=0, phase2=0, v1=0.0, v2=0.0, q=0.0, d=0.0, y=0.0, prev=0, pitch=0)
        outs = []
        for v in range(P['variants']):
            self.retrigger(P, st)
            self.segments(P, st)
            self.echo(P)
            outs.append(self.post(record_index))
        return outs


def main():
    data = json.load(open(sys.argv[1]))
    od = sys.argv[2]
    os.makedirs(od, exist_ok=True)
    syn = Synth(data['rng_state'])
    renders = []
    for step in data['order']:
        rec = data['records'][step]
        for v, pcm in enumerate(syn.render_patch(rec, step)):
            name = f"Probe{step + 1}{v + 1}"
            open(os.path.join(od, name + '.s8'), 'wb').write(pcm)
            renders.append(dict(name=name, record=step, variant=v))
            print(name, flush=True)
    json.dump(renders, open(os.path.join(od, 'renders.json'), 'w'), indent=1)


if __name__ == '__main__':
    main()
