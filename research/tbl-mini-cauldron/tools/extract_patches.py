"""Extract mini-cauldron patch data from the relocated images into JSON.

Usage: extract_patches.py <out_root>   (expects out_root/jizz/reloc.bin, out_root/stash/reloc.bin)
Writes ../ref/patches_jizz.json, ../ref/patches_stash.json (records only) and
out_root/stash/drums.json (the 7 stored drum samples, delta-coded bytes as hex).

Addresses (BASE 0x400000):
  Jizz  v1: records 123 B at 0x41175A, 12-entry 1-based order table at 0x41172A (dword),
            render loop 0x411658, RNG state at synth start = 80 draws from (0,0,0).
  Stash v2: records 103 B; call 1 (BLUISH, 11 slots) table 0x4037ED (10 records),
            call 2 (song 1, 17 slots, self-modified) table 0x40317D (16 records);
            slot 0 -> record 0, slot 1 -> drums, slot s>=2 -> record s-1.
            Drum kit: lengths dword at 0x403BF7 + 8*i (7 entries), data at 0x408C7C.
"""
import json, os, struct, sys
B = 0x400000
HERE = os.path.dirname(os.path.abspath(__file__))
REF = os.path.join(HERE, '..', 'ref')

def v1_records(m, base, n):
    out = []
    for r in range(n):
        b = m[base - B + 123 * r: base - B + 123 * (r + 1)]
        A, Bv, d, q = struct.unpack_from('<4d', b, 0)
        mul, gain, echo = struct.unpack_from('<3d', b, 0x22)
        out.append(dict(A=A, B=Bv, decay=d, damp=q, wave=b[0x20], variants=b[0x21] + 1, mul=mul, gain=gain, echo=echo,
                        pad=b[0x39], voices=b[0x3A], pitch=list(struct.unpack_from('<16H', b, 0x3B)),
                        accent=list(b[0x5B:0x6B]), tie=list(b[0x6B:0x7B]), raw=b.hex()))
    return out

def v2_records(m, base, n):
    out = []
    for r in range(n):
        b = m[base - B + 103 * r: base - B + 103 * (r + 1)]
        f = struct.unpack_from('<7f', b, 0)
        R = dict(zip(['B', 'damp', 'A', 'decay', 'mul', 'gain', 'echo'], f))
        R.update(wave=b[0x1C], variants=b[0x1D] + 1, decim=b[0x1E], voices=b[0x1F], doubler=b[0x20], unused21=b[0x21],
                 comb=b[0x22], hold=struct.unpack_from('<H', b, 0x23)[0], release=struct.unpack_from('<H', b, 0x25)[0],
                 pitch=list(struct.unpack_from('<16H', b, 0x27)), accent=list(b[0x47:0x57]), tie=list(b[0x57:0x67]),
                 raw=b.hex())
        out.append(R)
    return out

def main():
    root = sys.argv[1]
    m = open(os.path.join(root, 'jizz', 'reloc.bin'), 'rb').read()
    order = [struct.unpack_from('<I', m, 0x41172A - B + 4 * i)[0] - 1 for i in range(12)]
    json.dump(dict(source='Jizz (TBL 1997), iXalance jizz.ixa; mini-cauldron v1 by Probe',
                   rng_state=[0x86684929, 0x6525C79E, 0xB3EECD3E], order=order,
                   records=v1_records(m, 0x41175A, 12)), open(os.path.join(REF, 'patches_jizz.json'), 'w'), indent=1)
    m = open(os.path.join(root, 'stash', 'reloc.bin'), 'rb').read()
    lens = [struct.unpack_from('<I', m, 0x403BF7 - B + 8 * i)[0] for i in range(7)]
    drums, off = [], 0
    for L in lens:
        drums.append(dict(len=L, data_hex=m[0x408C7C - B + off:0x408C7C - B + off + L].hex())); off += L
    json.dump(dict(source='Stash (TBL 1997), iXalance stash.ixa; mini-cauldron v2',
                   note='call 0 renders the BLUISH XM (played second), call 1 the untitled XM (played first); '
                        'the work buffer persists across calls; slot 1 = stored drum kit (not synthesized)',
                   drum_lengths=lens,
                   calls=[dict(name='bluish', slots=11, flag=0, drum_count=6, after_tie=0, records=v2_records(m, 0x4037ED, 10)),
                          dict(name='song0', slots=17, flag=1, drum_count=7, after_tie=0, records=v2_records(m, 0x40317D, 16))]),
              open(os.path.join(REF, 'patches_stash.json'), 'w'), indent=1)
    json.dump(drums, open(os.path.join(root, 'stash', 'drums.json'), 'w'))
    print('order', order, 'drum lengths', lens)

if __name__ == '__main__':
    main()
