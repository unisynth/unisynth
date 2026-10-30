"""Which instructions write into [lo, hi)?  (e.g. the XM sample area)

Usage: trace_writes.py <outdir> <lo> <hi>   (hex)
"""
import collections, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ixa_host import Host, BASE
from unicorn import UC_HOOK_MEM_WRITE
from unicorn.x86_const import UC_X86_REG_EIP
h = Host(sys.argv[1]); lo = int(sys.argv[2], 16); hi = int(sys.argv[3], 16)
hist = collections.Counter(); first = {}
def hw(uc, access, addr, size, value, ud):
    e = uc.reg_read(UC_X86_REG_EIP); hist[e] += 1; first.setdefault(e, (addr, len(first)))
h.mu.hook_add(UC_HOOK_MEM_WRITE, hw, begin=lo, end=hi)
h.run()
for e, c in sorted(hist.items(), key=lambda x: first[x[0]][1]):
    print(f'{e:#x} writes={c} first_addr={first[e][0]:#x}')
