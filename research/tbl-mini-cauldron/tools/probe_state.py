"""Dump RNG state, FPU control word and dwords when execution first reaches an address.

Usage: probe_state.py <outdir> <at> <rng_state_addr> [dword_addr ...]   (hex)
Jizz:  probe_state.py out/jizz 411658 40f634 4110dd   -> rng 86684929 6525c79e b3eecd3e, CW 0x7f
Stash: probe_state.py out/stash 40302d 44b63f 401cb2  -> rng 0 0 0, CW 0x7f
"""
import os, struct, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ixa_host import Host
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_FPCW
h = Host(sys.argv[1]); at = int(sys.argv[2], 16); rng = int(sys.argv[3], 16)
extra = [int(x, 16) for x in sys.argv[4:]]
def hc(uc, addr, size, ud):
    print('rng', [hex(x) for x in struct.unpack('<3I', uc.mem_read(rng, 12))], 'FPCW', hex(uc.reg_read(UC_X86_REG_FPCW)))
    for e in extra: print(hex(e), '=', hex(h.u32(e)))
    uc.emu_stop()
h.mu.hook_add(UC_HOOK_CODE, hc, begin=at, end=at)
h.run()
