"""Disassemble executed basic blocks (bb.json from trace_bb.py) in [lo, hi) of reloc.bin.

Usage: disbb.py <outdir> <lo> <hi>   (hex). Gaps = data or never-executed code (use disx.py there).
"""
import sys, json, capstone
D=sys.argv[1]; lo=int(sys.argv[2],16); hi=int(sys.argv[3],16)
BASE=0x400000
img=open(f'{D}/reloc.bin','rb').read()
bb=json.load(open(f'{D}/bb.json'))
md=capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
seen=set(); prev=None
for a,(c,s,o) in sorted(((int(k,16),v) for k,v in bb.items())):
    if not lo<=a<hi: continue
    if prev is not None and a>prev: print(f"   ; ---- gap {prev:#x}..{a:#x} (data/unexecuted)")
    print(f"; block {a:#x} n={c}")
    for i in md.disasm(img[a-BASE:a-BASE+s], a):
        if i.address in seen: continue
        seen.add(i.address)
        print(f"  {i.address:#x}: {i.mnemonic} {i.op_str}")
    prev=max(prev or 0,a+s)
