"""Unpack a Crinkler-compressed Win32 exe (change add-receptor-import): emulates its decompressor with unicorn and dumps
the image from the section start once execution reaches the unpacked code. Ours, MIT; it runs only the exe's own
header code, no API calls (Crinkler decompresses before it imports anything).

  uv run --with unicorn tools/elevated-parity/crinkler-unpack.py <in.exe> <out.bin>

The dump starts at image base + section VA (0x410000 for Crinkler 1.x) and runs to its last non-zero byte. Crinkler
keeps its context-model hash tables inside the section's virtual size (~270 MB), so the whole virtual image is mapped:
a run takes minutes under unicorn."""
import struct
import sys

from unicorn import UC_ARCH_X86, UC_HOOK_CODE, UC_MODE_32, Uc, UcError
from unicorn.x86_const import UC_X86_REG_EIP, UC_X86_REG_ESP

src, out = sys.argv[1], sys.argv[2]
d = open(src, "rb").read()
pe = struct.unpack_from("<I", d, 0x3C)[0]
ep, = struct.unpack_from("<I", d, pe + 0x28)
ib, = struct.unpack_from("<I", d, pe + 0x34)
so = pe + 0x18 + struct.unpack_from("<H", d, pe + 0x14)[0]
_, vs, va, _, ra = struct.unpack_from("<8sIIII", d, so)
size = (va + vs + 0xFFFF) & ~0xFFFF
mu = Uc(UC_ARCH_X86, UC_MODE_32)
mu.mem_map(ib, size)
mu.mem_write(ib, d)  # Crinkler runs its header in place ...
mu.mem_write(ib + va, d[ra & ~0x1FF:])  # ... and maps the file again at the section (raw pointer rounds down)
mu.mem_map(0x7F000000, 0x100000)
mu.reg_write(UC_X86_REG_ESP, 0x7F0FF000)
hit = []
unpacked = ib + va + 0x10000  # the decompressed code starts at image base + 0x20000


def hook(uc, addr, _size, _):
    if addr >= unpacked and not hit:
        hit.append(addr)
        uc.emu_stop()


mu.hook_add(UC_HOOK_CODE, hook)
try:
    mu.emu_start(ib + ep, 0, count=2_000_000_000)
except UcError as e:
    sys.exit(f"crinkler-unpack: {e} at {mu.reg_read(UC_X86_REG_EIP):#x}")
if not hit:
    sys.exit("crinkler-unpack: never reached the unpacked code")
img = bytes(mu.mem_read(ib + va, 0x400000)).rstrip(b"\0")
open(out, "wb").write(img)
print(f"{out}: entry {hit[0]:#x}, base {ib + va:#x}, {len(img)} bytes")
