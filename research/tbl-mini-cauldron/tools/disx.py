"""Linear disassembly of reloc.bin (BASE 0x400000). Usage: disx.py <outdir> <addr hex> [count]
Beware inline data (constants live between functions in these intros).
"""
import sys, capstone, struct
sys.argv_save=list(sys.argv)
D=sys.argv[1]; a=int(sys.argv[2],16); n=int(sys.argv[3]) if len(sys.argv)>3 else 60
BASE=0x400000
img=open(f'{D}/reloc.bin','rb').read()
md=capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
o=a-BASE
for i in md.disasm(img[o:o+n*12], a):
    print(f"{i.address:#x}: {i.bytes.hex():<20} {i.mnemonic} {i.op_str}")
    n-=1
    if n<=0: break
