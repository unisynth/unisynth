"""Alias-free phrase references for the parity suite (change add-cauldron-xm-import).

The stored phrases are decimated 2:1 or 4:1 without a filter, so their upper bands carry folded aliasing (up to
+7 dB in an octave band, measured). The port drops the decimation by design and keeps the stored bandwidth with a
low-pass, so the phrase suite judges it against this: each phrase from the bit-exact references, taken at the full
internal rate just before the decimation (after Jizz's record-7 halving and Stash's song0 specials), band-limited to
its stored Nyquist and resampled to 44.1 kHz in one FFT step (ideal brick wall).

Usage: fullrate_phrases.py <out_dir>   (expects rebuild_xm.py's <out>/listen/phrases.json)
Writes <out>/listen/ref44bw/<phrase>.wav (mono 16-bit 44.1 kHz, sample = int8 / 128 as in ref44/).
"""
import json, os, sys, wave
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..', 'ref'))
import mcauldron1 as m1, mcauldron2 as m2

R = 2 * 8363 * 2 ** (13 / 12)
OUT_RATE = 44100


def i8(b):
    return np.frombuffer(bytes(b), dtype=np.int8)


def capture_v1(data):
    """{(record, variant): int8 frames at R} for Jizz: buf[0:2·OUT_LEN] before the 2:1 decimation."""
    syn, got, cur = m1.Synth(data['rng_state']), {}, {}
    post = syn.post

    def hooked(record_index):
        b = bytearray(syn.buf[:2 * m1.OUT_LEN])
        if record_index == 7:                         # the halving runs before the decimation
            b = bytearray((m1.i8(x) >> 1) & 0xFF for x in b)
        got[(record_index, cur['v'])] = i8(b)
        cur['v'] += 1
        return post(record_index)
    syn.post = hooked
    for step in data['order']:
        cur['v'] = 0
        syn.render_patch(data['records'][step], step)
    return got


def capture_v2(data):
    """{(call name, record, variant): int8 frames at R} for Stash: the buffer before the decimation."""
    syn, got, cur = m2.Synth(), {}, {}
    post = syn.post

    def hooked(P, slot, flag):
        f = P['decim'] if P['decim'] in (1, 2) else 4
        n = 59000 if f == 1 else (29000 if f == 2 else 14000) * f
        b = bytearray(syn.buf[:n])
        if flag:                                      # the specials run before the decimation
            if slot == 9:
                b = bytearray((x + (m2.i8(x) >> 2)) & 0xFF for x in b)
            if slot in (0x0B, 0x0F):
                b = bytearray((x + 0x30) & 0xFF for x in b)
        rec = 0 if slot == 0 else slot - 1
        key = (cur['call'], rec)
        v = cur.setdefault(key, 0)
        got[(cur['call'], rec, v)] = i8(b)
        cur[key] = v + 1
        return post(P, slot, flag)
    syn.post = hooked
    for song in data['calls']:
        cur['call'] = song['name']
        syn.render_song(song)
    return got


def band_limit(x, nyquist):
    """int8 frames at R → float at 44.1 kHz with nothing above `nyquist` (FFT brick wall + resample)."""
    x = x.astype(np.float64) / 128.0
    n_out = int(round(len(x) * OUT_RATE / R))
    X = np.fft.rfft(x)
    f = np.fft.rfftfreq(len(x), 1 / R)
    X[f > min(nyquist, OUT_RATE / 2)] = 0
    Y = np.zeros(n_out // 2 + 1, dtype=complex)
    k = min(len(X), len(Y))
    Y[:k] = X[:k]
    return np.fft.irfft(Y, n_out) * n_out / len(x)


def write(path, y):
    s = np.clip(np.round(y * 32768), -32768, 32767).astype('<i2')
    with wave.open(path, 'wb') as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(OUT_RATE)
        w.writeframes(s.tobytes())


def main():
    out = sys.argv[1]
    L = os.path.join(out, 'listen')
    D = os.path.join(L, 'ref44bw')
    os.makedirs(D, exist_ok=True)
    phrases = json.load(open(os.path.join(L, 'phrases.json')))
    ref = os.path.join(HERE, '..', 'ref')
    jizz = capture_v1(json.load(open(os.path.join(ref, 'patches_jizz.json'))))
    stash = capture_v2(json.load(open(os.path.join(ref, 'patches_stash.json'))))
    for p in phrases:
        if p['song'] == 'jizz':
            x = jizz[(p['record'], p['variant'])]
        else:
            x = stash[(p['song'][len('stash_'):], p['record'], p['variant'])]
        write(os.path.join(D, p['name'] + '.wav'), band_limit(x, p['nyquist_hz']))
        print(p['name'], flush=True)


if __name__ == '__main__':
    main()
