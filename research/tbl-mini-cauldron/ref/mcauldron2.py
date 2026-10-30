"""Reference re-implementation of "mini-cauldron" v2 (TBL, Stash 1997).

Bit-exact against the samples the original x86 code writes into both Stash
XMs (verified by tools/verify.py against the emulated intro).

Usage:  python mcauldron2.py patches_stash.json out_dir
Writes <call>_<slot>_<variant>.s8 (raw signed 8-bit PCM, NOT delta coded)
and renders.json.  The drum slot (slot 1) is not synthesized: it copies
stored samples (see patches_stash.json "drums").

Differences from v1 (mcauldron1.py): float32 record fields, 6 waveforms,
hold/release gate instead of the linear fade, comb (difference) filter,
"doubler" (0.997x self-resample), decimation factor 1/2/4, 15-frame tail
fade + 4 zero bytes, buffer is delta coded IN PLACE after every render
(stale bytes are re-read by comb/doubler of later renders), and the work
buffer persists across the two song calls.
"""
import json
import os
import sys

from x87pc24 import add, sub, mul, div, fistp

STEP = 3530
ECHO_STRIDE = 3529
GLIDE = 1764
C = 3529.0            # qword 0x401998
DETUNE = 1.02         # qword 0x402C1D
ACCENT = 2.0          # dword float 0x402ECD
FULL = 32767.0        # qword 0x402BD3
BUF_LEN = 0x20000


def i16(x):
    x &= 0xFFFF
    return x - 0x10000 if x & 0x8000 else x


def i8(x):
    x &= 0xFF
    return x - 0x100 if x & 0x80 else x


def s32(x):
    x &= 0xFFFFFFFF
    return x - (1 << 32) if x & 0x80000000 else x


def clamp16(v):
    return 32767 if v > 32767 else (-32767 if v < -32767 else v)


def shape_t(x):
    """32767 * t^7 with per-op x87 PC24 rounding (t already computed)."""
    t2 = mul(x, x)
    t4 = mul(t2, t2)
    t6 = mul(t4, t2)
    return mul(mul(t6, FULL), x)


class Synth:
    def __init__(self):
        self.buf = bytearray(BUF_LEN)   # persistent across renders AND song calls
        self.seg_reg = 0                # word 0x401D04
        self.t = 0                      # dword 0x401CD6

    # ---- 0x402E34 ------------------------------------------------------------
    def retrigger(self, P, st):
        st['prev'] = 0
        st['phase'] = i16(0x8002)
        m = P['mul']
        P['B'] = mul(m, P['B'])          # fst dword: value already has a 24-bit significand
        st['v2'] = P['B']
        P['A'] = mul(m, P['A'])
        st['v1'] = P['A']
        st['q'] = P['damp']
        st['d'] = P['decay']
        s = self.seg_reg
        raw = P['raw']
        if raw[0x47 + s] == 1:           # accent[s]  (s == 16 reads tie[0])
            st['v2'] = mul(st['v2'], ACCENT)
            st['v1'] = mul(st['v1'], ACCENT)
        st['pitch'] = raw[0x27 + 2 * s] | (raw[0x28 + 2 * s] << 8)   # movzx word; s=16 reads accent[0..1]

    @staticmethod
    def filt(st, x):
        st['v1'] = mul(st['v1'], st['d'])
        c = add(st['v1'], st['v2'])
        y = sub(st['y'], mul(float(x + st['prev']), c))
        y = mul(y, sub(1.0, st['q']))
        st['y'] = y
        return fistp(add(y, float(st['prev'])))

    def wave2(self, P, st):
        """0x402B26: like v1 but the base voice has no sign branch and voices are not negated."""
        V = P['voices']
        k = 0
        while True:
            out = fistp(shape_t(div(add(-float(st['phase']), FULL), FULL)))
            if V != 0:
                off = ((self.t * k) & 0xFFFF) * (st['pitch'] & 0xFFFFFFFF)   # mul bx; mul dword
                off = (off >> 7) & 0xFFFFFFFF
                st['phase2'] = i16(st['phase'] + off)
                x = float(st['phase2'])
                if st['phase2'] > 0:
                    x = add(-x, FULL)
                out = fistp(add(shape_t(div(x, FULL)), float(out)))
            k += 1
            if k > V:
                return out

    # ---- 0x402BDB: one frame --------------------------------------------------
    def frame(self, P, st, n):
        s = self.seg_reg
        out = st['phase']
        if P['voices'] != 0:
            st['phase2'] = i16(st['phase2'] + fistp(mul(float(s32(st['pitch'])), DETUNE)))
            out += st['phase2']
        w = P['wave']
        if w == 1:
            out = 30000 if out > 0 else -30000
        elif w == 2:
            out = self.wave2(P, st)
        elif w == 3:
            out = 30000 if out > 31000 else 0
        elif w == 4:
            raise NotImplementedError('noise: same as v1 (RNG at 0x44B64B); unused by Stash')
        elif w == 5:
            if out > 0:
                out = 0x7FFF - out
        out = clamp16(out)
        if s != 0 and P['tie'][s] == 1:
            if n < GLIDE:
                n2 = float(2 * n)
                a = mul(div(sub(C, n2), C), float(i16(P['pitch'][s - 1])))   # fimul word: signed
                b = mul(div(n2, C), float(i16(P['pitch'][s])))
                st['pitch'] = fistp(add(a, b))
            else:
                st['pitch'] = P['pitch'][s]
        st['phase'] = i16(st['phase'] + st['pitch'])
        self.t += 1
        x = fistp(mul(float(out), P['gain']))
        y = self.filt(st, x)
        st['prev'] = y
        nxt_tie = P['tie'][s + 1] if s + 1 <= 15 else P['after_tie']
        if nxt_tie == 0:
            hold, rel = P['hold'], P['release']
            if i16(n) > i16(hold):                          # signed 16-bit compare
                if n > ((hold + rel) & 0xFFFF):
                    y = 0
                else:
                    r = div(sub(float(n), float(i16(hold))), float(i16(rel)))   # fisub/fidiv word: signed
                    y = fistp(mul(sub(1.0, r), float(y)))
        y = clamp16(y)
        self.buf[self.t] = (y >> 8) & 0xFF

    def segments(self, P, st):
        for s in range(16):
            self.seg_reg = s
            if P['tie'][s] == 0:
                self.retrigger(P, st)
            for n in range(STEP):
                self.frame(P, st, n)
        self.seg_reg = 16

    def comb(self, k):
        """0x402A7C: x[i] = clamp(x[i+k] - x[i], -126, 127), i = 0 .. 59000-k-1, in place."""
        b = self.buf
        for i in range(59000 - k):
            v = i8(b[i + k]) - i8(b[i])
            if v >= 127:
                v = 127
            if v <= -126:
                v = -126
            b[i] = v & 0xFF

    def echo(self, fb):
        b = self.buf
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

    def doubler(self):
        """0x402FF0: y[r(i)-1] = (x[r(i)] + x[i]) >> 1, r(i) = (i*0xFF3B + 0x8000) >> 16, i = 2..58000."""
        b = self.buf
        for i in range(2, 0xE290 + 1):
            j = ((i * 0xFF3B + 0x8000) & 0xFFFFFFFF) >> 16
            v = (i8(b[j]) + i8(b[i])) >> 1
            j -= 1
            if j < 0xE290:
                b[j] = v & 0xFF

    def post(self, P, slot, song_flag):
        b = self.buf
        if song_flag:
            if slot == 9:
                for i in range(60000):
                    b[i] = (b[i] + (i8(b[i]) >> 2)) & 0xFF
            if slot in (0x0B, 0x0F):
                for i in range(60000):
                    b[i] = (b[i] + 0x30) & 0xFF
        f = P['decim']
        if f == 1:
            n = 59000
        else:
            n = 29000 if f == 2 else 14000
            for i in range(n):
                b[i] = b[i * f]
        for j in range(15):                       # 15-byte tail fade: x*15/16 .. x*1/16
            i = n - 19 + j
            b[i] = ((i8(b[i]) * (15 - j)) >> 4) & 0xFF
        b[n - 4:n] = b'\0\0\0\0'
        self.t = 0
        pcm = bytes(b[:n])
        prev = 0                                  # delta coding in place (0x402FDD)
        for i in range(n):
            cur = b[i]
            b[i] = (cur - prev) & 0xFF
            prev = cur
        return pcm

    def render_song(self, song):
        recs = song['records']
        outs = []
        for slot in range(song['slots']):
            if slot == 1:
                outs.append(('drums', slot, None))
                continue
            rec = recs[0 if slot == 0 else slot - 1]
            P = dict(rec)
            P['raw'] = bytes.fromhex(rec['raw'])
            P['after_tie'] = song['after_tie']
            st = dict(phase=0, phase2=0, v1=0.0, v2=0.0, q=0.0, d=0.0, y=0.0, prev=0, pitch=0)
            for v in range(P['variants']):
                self.retrigger(P, st)
                self.segments(P, st)
                if P['comb']:
                    self.comb(P['comb'])
                self.echo(P['echo'])
                if P['doubler']:
                    self.doubler()
                outs.append(('render', slot, v, self.post(P, slot, song['flag'])))
        return outs


def main():
    data = json.load(open(sys.argv[1]))
    od = sys.argv[2]
    os.makedirs(od, exist_ok=True)
    syn = Synth()
    renders = []
    for call, song in enumerate(data['calls']):
        for o in syn.render_song(song):
            if o[0] != 'render':
                renders.append(dict(call=call, slot=o[1], kind='drums'))
                continue
            _, slot, v, pcm = o
            name = f"{song['name']}_{slot}_{v}"
            open(os.path.join(od, name + '.s8'), 'wb').write(pcm)
            renders.append(dict(call=call, slot=slot, variant=v, name=name, len=len(pcm), kind='render'))
            print(name, len(pcm), flush=True)
    json.dump(renders, open(os.path.join(od, 'renders.json'), 'w'), indent=1)


if __name__ == '__main__':
    main()
