"""Unpack a UPX 0.20-era LE executable (DOS/4GW or PMODE/W stub, Watcom): load it flat, apply the fixups, run the
UPX stub in unicorn until it leaves object 2 (the original entry point), return object 1's memory.

    unpack(exe_bytes) -> (image, {"base", "oep", "size", "used"})
    python3 le_unupx.py <in.exe> <out.bin>     (also writes <out.bin>.meta)

Modern UPX refuses these files (its signature is stripped), hence the emulation. Change add-plain-xm-import."""
import struct, sys
from unicorn import *
from unicorn.x86_const import *
from capstone import Cs, CS_ARCH_X86, CS_MODE_32


def unpack(d, log=lambda *a: None):
    le = struct.unpack('<I', d[0x3c:0x40])[0]
    H = lambda o: struct.unpack_from('<I', d, le + o)[0]
    npages, pgsz, lastpg = H(0x14), H(0x28), H(0x2c)
    eipobj, eip, espobj, esp = H(0x18), H(0x1c), H(0x20), H(0x24)
    objtab, nobj, pgmap, fixpg, fixrec, datapg = H(0x40), H(0x44), H(0x48), H(0x68), H(0x6c), H(0x80)
    objs = []
    for i in range(nobj):
        vs, base, fl, pmi, pmc, _ = struct.unpack_from('<6I', d, le + objtab + i * 24)
        objs.append((vs, base, fl, pmi, pmc))

    mu = Uc(UC_ARCH_X86, UC_MODE_32)
    al = lambda x: (x + 0xfff) & ~0xfff
    lo = min(b for _, b, *_ in objs) & ~0xfff
    hi = al(max(b + v for v, b, *_ in objs) + 0x20000)
    mu.mem_map(lo, hi - lo)
    # page data
    def page_off(p):  # 1-based page
        return datapg + (p - 1) * pgsz
    for vs, base, fl, pmi, pmc in objs:
        for k in range(pmc):
            p = pmi + k
            n = lastpg if p == npages else pgsz
            mu.mem_write(base + k * pgsz, d[page_off(p):page_off(p) + n])
    # fixups
    nfix = 0
    for vs, base, fl, pmi, pmc in objs:
        for k in range(pmc):
            p = pmi + k
            a, b = struct.unpack_from('<2I', d, le + fixpg + (p - 1) * 4)
            o = le + fixrec + a
            end = le + fixrec + b
            while o < end:
                src, flg = d[o], d[o + 1]; o += 2
                assert not (src & 0x20), 'src list unsupported'
                soff = struct.unpack_from('<h', d, o)[0]; o += 2
                assert (flg & 3) == 0, 'internal only'
                tobj = d[o]; o += 1
                if flg & 0x40: tobj |= d[o] << 8; o += 1
                if (src & 0xf) == 2:  # selector
                    continue
                if flg & 0x10: toff = struct.unpack_from('<I', d, o)[0]; o += 4
                else: toff = struct.unpack_from('<H', d, o)[0]; o += 2
                tgt = objs[tobj - 1][1] + toff
                addr = base + k * pgsz + soff
                st = src & 0xf
                if st == 7: mu.mem_write(addr, struct.pack('<I', tgt))
                elif st == 8: mu.mem_write(addr, struct.pack('<i', tgt - (addr + 4)))
                else: raise SystemExit(f'fixup type {st}')
                nfix += 1
    log('fixups applied', nfix)
    # NOP segment push/pop (flat model) in the stub
    s_lo = objs[1][1]; blob = bytes(mu.mem_read(s_lo, objs[1][0]))
    k = blob.find(bytes([0x50,0x06,0x1e,0x07]))
    assert k > 0
    mu.mem_write(s_lo + k + 1, b'\x90\x90\x90')
    k2 = blob.find(bytes([0xf3,0xab,0x07,0xc3]), k)
    mu.mem_write(s_lo + k2 + 2, b'\x90')
    log('patched seg ops at', hex(s_lo+k+1), hex(s_lo+k2+2))
    o1base, o1size = objs[0][1], objs[0][0]
    o2base, o2size = objs[1][1], objs[1][0]
    start = objs[eipobj - 1][1] + eip
    mu.reg_write(UC_X86_REG_ESP, objs[espobj - 1][1] + esp)
    cs = Cs(CS_ARCH_X86, CS_MODE_32)
    state = {'n': 0, 'oep': None}
    def hook(uc, addr, size, _):
        state['n'] += 1
        if not (o2base <= addr < o2base + o2size + 0x10000):
            state['oep'] = addr; uc.emu_stop()
    def bad(uc, access, addr, size, value, _):
        eipv = uc.reg_read(UC_X86_REG_EIP)
        code = uc.mem_read(eipv, 16)
        log('fault', access, hex(addr), 'at', hex(eipv), [f'{i.mnemonic} {i.op_str}' for i in cs.disasm(bytes(code), eipv, 1)])
        return False
    def intr(uc, no, _):
        log('int', hex(no), 'eax', hex(uc.reg_read(UC_X86_REG_EAX)), 'at', hex(uc.reg_read(UC_X86_REG_EIP))); uc.emu_stop()
    mu.hook_add(UC_HOOK_BLOCK, hook)
    mu.hook_add(UC_HOOK_MEM_INVALID, bad)
    mu.hook_add(UC_HOOK_INTR, intr)
    mu.emu_start(start, 0xffffffff, count=400_000_000)
    log('blocks', state['n'], 'OEP', hex(state['oep'] or 0))
    img = bytes(mu.mem_read(o1base, o1size))
    last = len(img.rstrip(b'\0'))
    log(f'obj1 base {o1base:x} size {o1size:x}, nonzero up to {last:x}')
    return img, {"base": o1base, "oep": state["oep"], "size": o1size, "used": last}


if __name__ == '__main__':
    img, m = unpack(open(sys.argv[1], 'rb').read(), print)
    open(sys.argv[2], 'wb').write(img)
    open(sys.argv[2] + '.meta', 'w').write(''.join(f'{k}={v:#x}\n' for k, v in m.items()))
