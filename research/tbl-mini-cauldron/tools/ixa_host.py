"""Run a TBL iXalance intro image under unicorn (32-bit flat) as an oracle.

Usage: ixa_host.py <outdir from unixa.py> [--max-showp N]
Stops at the first 'TBL1' call (start XM: ESI = XM, ECX = size), which comes
after the whole precalc (graphics + mini-cauldron sample synthesis). Saves:
  <outdir>/xm_<addr>.xm   every "Extended Module:" found in the image at that moment
                          (Jizz: 1, Stash: 2 - both songs are generated up front)
  <outdir>/emu.log        callback log

Import as a module (`from ixa_host import Host`) to add tracing hooks; see
trace_bb.py / trace_writes.py.

Host ABI (iXalance code.asm `startdemo`): entry via far call with 9 far
pointers / data pointers pushed (selector at the LOWER address of each 6-byte
pair), ESI -> gfxmodeinfo.  Trampolines: farmalloc (EDX=bytes -> EDX=ptr,
EAX=0, CF=0), fardoint (EAX='TBL1' start XM, 'TBL3' -> EAX=(row+1)+(pos<<8)),
farshowp (present frame), basic (poll).  The intro does fninit and sets
FPU CW = 0x007F (24-bit precision) itself.
"""
import os
import struct
import sys
import time

from unicorn import Uc, UcError, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE, UC_HOOK_MEM_INVALID, UC_HOOK_INTR
from unicorn.x86_const import (UC_X86_REG_GDTR, UC_X86_REG_CS, UC_X86_REG_DS, UC_X86_REG_ES, UC_X86_REG_FS,
                               UC_X86_REG_GS, UC_X86_REG_SS, UC_X86_REG_ESP, UC_X86_REG_ESI, UC_X86_REG_EAX,
                               UC_X86_REG_EDI, UC_X86_REG_EDX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_EFLAGS)

BASE = 0x00400000
HEAP, HEAPSZ = 0x10000000, 64 << 20
STACK, STACKSZ = 0x0F000000, 0x100000
HOST = 0x0E000000
GDT = 0x0D000000


def xm_size(b, o):
    hs = struct.unpack_from('<I', b, o + 60)[0]
    _, _, _, npat, nins = struct.unpack_from('<5H', b, o + 64)
    p = o + 60 + hs
    for _ in range(npat):
        phl = struct.unpack_from('<I', b, p)[0]
        psz = struct.unpack_from('<H', b, p + 7)[0]
        p += phl + psz
    for _ in range(nins):
        ihs = struct.unpack_from('<I', b, p)[0]
        ns = struct.unpack_from('<H', b, p + 27)[0]
        if ns:
            q = p + ihs
            tot = sum(struct.unpack_from('<I', b, q + 40 * j)[0] for j in range(ns))
            p = q + 40 * ns + tot
        else:
            p += ihs
    return p - o


class Host:
    def __init__(self, d):
        self.d = d
        img = open(os.path.join(d, 'reloc.bin'), 'rb').read()
        self.ip = int(open(os.path.join(d, 'ip.txt')).read(), 16)
        self.memreq = (len(img) + 0x20000 + 0xFFF) & ~0xFFF
        mu = self.mu = Uc(UC_ARCH_X86, UC_MODE_32)
        mu.mem_map(0, BASE)                       # low memory + underflow slack
        mu.mem_map(BASE, self.memreq)
        mu.mem_write(BASE, img)
        mu.mem_map(HEAP, HEAPSZ)
        mu.mem_map(STACK, STACKSZ)
        mu.mem_map(HOST, 0x10000)
        mu.mem_map(GDT, 0x1000)

        def desc(base, limit, access, flags):
            return struct.pack('<HHBBBB', limit & 0xFFFF, base & 0xFFFF, (base >> 16) & 0xFF, access,
                               ((limit >> 16) & 0xF) | (flags << 4), (base >> 24) & 0xFF)
        gdt = b'\0' * 8 + desc(0, 0xFFFFF, 0x9A, 0xC) + desc(0, 0xFFFFF, 0x92, 0xC)
        mu.mem_write(GDT, gdt)
        mu.reg_write(UC_X86_REG_GDTR, (0, GDT, len(gdt) - 1, 0))
        mu.reg_write(UC_X86_REG_CS, 0x08)
        for r in (UC_X86_REG_DS, UC_X86_REG_ES, UC_X86_REG_FS, UC_X86_REG_GS, UC_X86_REG_SS):
            mu.reg_write(r, 0x10)

        self.T_BASIC, self.T_SHOWP, self.T_DOINT, self.T_MALLOC, self.T_RET = (HOST + 0x10, HOST + 0x20, HOST + 0x30,
                                                                                HOST + 0x40, HOST + 0x50)
        for t in (self.T_BASIC, self.T_SHOWP, self.T_DOINT, self.T_MALLOC):
            mu.mem_write(t, b'\xcb')              # retf
        mu.mem_write(self.T_RET, b'\xf4')
        self.HERZ, self.MUSTIME, MODEINFO = HOST + 0x100, HOST + 0x104, HOST + 0x200
        screen = HEAP + HEAPSZ - 0x40000
        mi = struct.pack('<4B3I', 0, 0, 0, 0, screen, screen, screen) + struct.pack('<4IH', 320, 200, 640, 0, 0x10E)
        mi += struct.pack('<4IH8B', 640, 320, 200, 0, 0x10E, 16, 11, 5, 0, 5, 6, 5, 1)
        mi += struct.pack('<2IH8B', 640, 0, 0x10E, 16, 11, 5, 0, 5, 6, 5, 0) + b'\0' * 32
        mu.mem_write(MODEINFO, mi)

        sp = STACK + STACKSZ - 0x1000
        def push(v, n):
            nonlocal sp
            sp -= n
            mu.mem_write(sp, v.to_bytes(n, 'little'))
        for t in (self.T_BASIC, self.T_BASIC, self.T_SHOWP, self.T_BASIC, self.T_BASIC):
            push(t, 4); push(0x08, 2)
        push(self.HERZ, 4); push(self.MUSTIME, 4)
        push(self.T_DOINT, 4); push(0x08, 2)
        push(self.T_MALLOC, 4); push(0x08, 2)
        push(0x08, 4); push(self.T_RET, 4)
        mu.reg_write(UC_X86_REG_ESP, sp)
        mu.reg_write(UC_X86_REG_ESI, MODEINFO)
        mu.reg_write(UC_X86_REG_EAX, 0)
        mu.reg_write(UC_X86_REG_EDI, 0)

        self.heapused = 0
        self.herz = 0
        self.musrow = 0
        self.nshowp = 0
        self.max_showp = 10 ** 9
        self.xms = []
        self.stop_at_tbl1 = True
        self.t0 = time.time()
        self.log = open(os.path.join(d, 'emu.log'), 'w')
        mu.hook_add(UC_HOOK_CODE, self._tramp, begin=HOST, end=HOST + 0x100)
        mu.hook_add(UC_HOOK_MEM_INVALID, self._bad)
        mu.hook_add(UC_HOOK_INTR, self._intr)

    def u32(self, a):
        return struct.unpack('<I', self.mu.mem_read(a, 4))[0]

    def _tick(self):
        self.herz += 1
        self.mu.mem_write(self.HERZ, struct.pack('<I', self.herz))

    def _tramp(self, uc, eip, size, ud):
        self._tick()
        if eip == self.T_SHOWP:
            self.nshowp += 1
            if self.nshowp >= self.max_showp:
                uc.emu_stop()
        elif eip == self.T_MALLOC:
            n = uc.reg_read(UC_X86_REG_EDX)
            ptr = 0
            if n > 0:
                ptr = HEAP + self.heapused
                self.heapused += (n + 4096) & ~0xFFF
            self.log.write(f'malloc {n:#x} -> {ptr:#x}\n')
            uc.reg_write(UC_X86_REG_EAX, 0)
            uc.reg_write(UC_X86_REG_EDX, ptr)
            uc.reg_write(UC_X86_REG_EFLAGS, uc.reg_read(UC_X86_REG_EFLAGS) & ~1)
        elif eip == self.T_DOINT:
            eax = uc.reg_read(UC_X86_REG_EAX)
            tag = struct.pack('>I', eax)
            if tag == b'TBL1':
                self._capture(uc.reg_read(UC_X86_REG_ESI), uc.reg_read(UC_X86_REG_ECX))
                self.musrow = 0
                if self.stop_at_tbl1:
                    uc.emu_stop()
            elif tag == b'TBL3':                   # music position: fast-forward one row per poll
                self.musrow += 1
                pos, row = divmod(self.musrow, 64)
                uc.reg_write(UC_X86_REG_EAX, (row + 1) + (pos << 8))
        elif eip == self.T_RET:
            self.log.write('demo returned\n')
            uc.emu_stop()

    def _capture(self, esi, ecx):
        self.log.write(f'TBL1 xm={esi:#x} size={ecx:#x} t={time.time() - self.t0:.1f}s\n')
        mem = bytes(self.mu.mem_read(BASE, self.memreq))
        i = -1
        while True:
            i = mem.find(b'Extended Module:', i + 1)
            if i < 0:
                break
            n = xm_size(mem, i)
            path = os.path.join(self.d, f'xm_{BASE + i:x}.xm')
            open(path, 'wb').write(mem[i:i + n])
            self.xms.append((BASE + i, n, path))
            self.log.write(f'  saved {path} ({n} bytes){"  <- started" if BASE + i == esi else ""}\n')

    def _bad(self, uc, access, addr, size, value, ud):
        self.log.write(f'BADMEM access={access} addr={addr:#x} eip={uc.reg_read(UC_X86_REG_EIP):#x}\n')
        return False

    def _intr(self, uc, intno, ud):
        self.log.write(f'INT {intno:#x} eip={uc.reg_read(UC_X86_REG_EIP):#x}\n')

    def run(self):
        try:
            self.mu.emu_start(BASE + self.ip, 0)
        except UcError as e:
            self.log.write(f'UcError {e} eip={self.mu.reg_read(UC_X86_REG_EIP):#x}\n')
        self.log.write(f'done t={time.time() - self.t0:.1f}s showp={self.nshowp}\n')
        self.log.flush()


if __name__ == '__main__':
    h = Host(sys.argv[1])
    if '--max-showp' in sys.argv:
        h.max_showp = int(sys.argv[sys.argv.index('--max-showp') + 1])
    h.run()
    for a, n, p in h.xms:
        print(f'{a:#x} {n} {p}')
