"""Basic-block coverage of an intro run until the first TBL1 (precalc done).

Usage: trace_bb.py <outdir> [lo hi]   (hex, default whole image)
Writes <outdir>/bb.json: {addr: [exec_count, size, first_seen_order]} - feed to disbb.py.
"""
import collections, json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ixa_host import Host, BASE
from unicorn import UC_HOOK_BLOCK
d = sys.argv[1]
h = Host(d)
lo = int(sys.argv[2], 16) if len(sys.argv) > 2 else BASE
hi = int(sys.argv[3], 16) if len(sys.argv) > 3 else BASE + h.memreq
bb = collections.Counter(); order = {}; sizes = {}
def hb(uc, addr, size, ud):
    bb[addr] += 1; sizes[addr] = size
    order.setdefault(addr, len(order))
h.mu.hook_add(UC_HOOK_BLOCK, hb, begin=lo, end=hi)
h.run()
json.dump({hex(a): [bb[a], sizes[a], order[a]] for a in bb}, open(os.path.join(d, 'bb.json'), 'w'))
print(len(bb), 'blocks')
