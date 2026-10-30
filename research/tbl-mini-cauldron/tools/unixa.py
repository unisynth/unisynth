"""Unpack an iXalance .ixa container and relocate the DOS32 ("Adam") image.

Usage: unixa.py <file.ixa> <outdir>
Writes into outdir:
  script.bin   iXalance script (1 n = push exe n, 2 = pop/run, 3 n = play XM ...)
  exe.bin      decompressed DOS32 executable (MZ stub + Adam header + image + fixups)
  image.bin    raw image (load at any base), BSS included as zeros
  fixups.bin   DOS32 relocation stream
  reloc.bin    image relocated to BASE = 0x400000 (addresses in docs refer to this)
  ip.txt       entry point offset (hex)

Formats (from iXalance-1.0.5 d32load.c / lzss.c / unrle.c / code.asm):
  header: char name[8]="IXALANCE", char demoname[32], int dirstart, int type
  dir at dirstart: int n; n x {int pos, int size, int fullsize}
  entry 0 = script (raw); others = LZSS(10-bit window, 4-bit length, break-even 1,
  bits read MSB-first from little-endian 32-bit words) then RLE (int len; ch>127:
  repeat next byte ch-127 times; else copy ch+1 bytes).
"""
import os
import struct
import sys

BASE = 0x400000


def lzss_expand(data):
    data = data + b'\0' * 8
    words = struct.unpack('<%dI' % (len(data) // 4), data[:len(data) // 4 * 4])
    wi = cnt = rack = 0
    out = bytearray()
    win = bytearray(1024)
    wp = 1

    def bit():
        nonlocal wi, cnt, rack
        if cnt == 0:
            rack = words[wi]; wi += 1; cnt = 31
        else:
            cnt -= 1
        v = rack >> 31
        rack = (rack << 1) & 0xFFFFFFFF
        return v

    def bits(n):
        nonlocal wi, cnt, rack
        if cnt < n:
            sp = n - cnt
            v = rack >> (32 - n)
            rack = words[wi]; wi += 1
            v |= rack >> (32 - sp)
            rack = (rack << sp) & 0xFFFFFFFF
            cnt = 32 - sp
        else:
            v = rack >> (32 - n)
            cnt -= n
            rack = (rack << n) & 0xFFFFFFFF
        return v

    while True:
        if bit():
            c = bits(8)
            out.append(c); win[wp] = c; wp = (wp + 1) & 1023
        else:
            mp = bits(10)
            if mp == 0:
                break
            ml = bits(4) + 1
            for _ in range(ml + 1):
                c = win[mp]; mp = (mp + 1) & 1023
                win[wp] = c; wp = (wp + 1) & 1023
                out.append(c)
    return bytes(out)


def unrle(inp):
    ln = struct.unpack('<i', inp[:4])[0]
    pos = 4
    out = bytearray()
    while pos < ln:
        ch = inp[pos]; pos += 1
        if ch > 127:
            out += bytes([inp[pos]]) * (ch - 127); pos += 1
        else:
            out += inp[pos:pos + ch + 1]; pos += ch + 1
    return bytes(out)


def relocate(img, fx, base=BASE, ds=0x10):
    """code.asm `reloc`: 7-bit varint deltas (nibble-swapped bytes); a 0 byte after a delta = 16-bit segment fixup."""
    buf = bytearray(img)
    i = edi = 0
    edx = len(fx) - 1
    while True:
        ebx = 0
        while True:
            ebx <<= 7
            al = fx[i]; i += 1; edx -= 1
            al = ((al << 4) | (al >> 4)) & 0xFF
            ebx += al & 127
            if not al & 128:
                break
        edi += ebx
        if edx >= 0 and fx[i] == 0:
            v = struct.unpack_from('<H', buf, edi)[0]
            struct.pack_into('<H', buf, edi, (v + ds) & 0xFFFF)
            i += 1; edx -= 1
        else:
            v = struct.unpack_from('<I', buf, edi)[0]
            struct.pack_into('<I', buf, edi, (v + base) & 0xFFFFFFFF)
        if edx < 0:
            break
    return bytes(buf)


def main():
    f = open(sys.argv[1], 'rb').read()
    od = sys.argv[2]
    os.makedirs(od, exist_ok=True)
    name, demo, dirstart, typ = struct.unpack('<8s32sii', f[:48])
    n = struct.unpack('<i', f[dirstart:dirstart + 4])[0]
    ents = [struct.unpack('<3i', f[dirstart + 4 + 12 * i:dirstart + 16 + 12 * i]) for i in range(n)]
    print(name, demo.rstrip(b'\0'), 'entries', n)
    open(os.path.join(od, 'script.bin'), 'wb').write(f[ents[0][0]:ents[0][0] + ents[0][1]])
    exes = [i for i in range(1, n)]
    if len(exes) != 1:
        print('note: more than one packed entry; only entry 1 is unpacked as exe')
    p, s, full = ents[1]
    exe = unrle(lzss_expand(f[p:p + s]))
    assert len(exe) == full, (len(exe), full)
    open(os.path.join(od, 'exe.bin'), 'wb').write(exe)
    h = struct.unpack('<32H', exe[:64])
    stub = h[1] + (h[2] - 1) * 512
    hdr = struct.unpack('<16i', exe[stub:stub + 64])
    assert exe[stub:stub + 4] == b'Adam'
    exestart, ip, exesize = hdr[4], hdr[5], hdr[8]
    fixsize = hdr[2] - exesize
    img = exe[stub + exestart:stub + exestart + exesize]
    fx = exe[stub + exestart + exesize:stub + exestart + exesize + fixsize]
    open(os.path.join(od, 'image.bin'), 'wb').write(img)
    open(os.path.join(od, 'fixups.bin'), 'wb').write(fx)
    open(os.path.join(od, 'reloc.bin'), 'wb').write(relocate(img, fx))
    open(os.path.join(od, 'ip.txt'), 'w').write('%x\n' % ip)
    print(f'image {len(img)} bytes, entry +{ip:#x} ({BASE + ip:#x}), fixups {len(fx)} bytes')


if __name__ == '__main__':
    main()
