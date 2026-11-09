# Emulate a kkrunchy stub until the unpacked program makes its first API call, then dump the image.
import sys, struct, pefile
from unicorn import *
from unicorn.x86_const import *
exe, out = sys.argv[1], sys.argv[2]
p = pefile.PE(exe)
base, size = p.OPTIONAL_HEADER.ImageBase, p.OPTIONAL_HEADER.SizeOfImage
mu = Uc(UC_ARCH_X86, UC_MODE_32)
mu.mem_map(base, (size + 0xfff) & ~0xfff)
raw = open(exe, 'rb').read()
mu.mem_write(base, raw[:p.OPTIONAL_HEADER.SizeOfHeaders])
for s in p.sections:
    mu.mem_write(base + s.VirtualAddress, raw[s.PointerToRawData:s.PointerToRawData + s.SizeOfRawData])
API = 0x70000000; mu.mem_map(API, 0x1000000)
mu.mem_write(API, b'\xc3' * 0x1000000)
names = {}
for e in p.DIRECTORY_ENTRY_IMPORT:
    for i in e.imports:
        a = API + len(names) * 16; names[a] = i.name.decode(); mu.mem_write(i.address, struct.pack('<I', a))
STACK = 0x10000000; mu.mem_map(STACK - 0x100000, 0x100000)
mu.reg_write(UC_X86_REG_ESP, STACK - 0x1000)
# TEB/PEB stubs (fs not set; most stubs do not need it)
strings = {}
nxt = [API + 0x10000]
state = {'calls': 0, 'stop': None}
def read_str(a):
    b = mu.mem_read(a, 256); return bytes(b[:b.index(0)]).decode('latin1') if 0 in b else '?'
def hook(uc, addr, sz, ud):
    if addr < API or addr >= API + 0x1000000: return
    esp = uc.reg_read(UC_X86_REG_ESP)
    ret, a1, a2 = struct.unpack('<III', uc.mem_read(esp, 12))
    n = names.get(addr)
    if n == 'LoadLibraryA':
        uc.reg_write(UC_X86_REG_EAX, 0x60000000 + len(strings)); strings[read_str(a1)] = 1
        uc.reg_write(UC_X86_REG_ESP, esp + 8); uc.reg_write(UC_X86_REG_EIP, ret)
    elif n == 'GetProcAddress':
        f = nxt[0]; nxt[0] += 16; names[f] = read_str(a2) if a2 > 0xffff else f'#{a2}'
        uc.reg_write(UC_X86_REG_EAX, f); uc.reg_write(UC_X86_REG_ESP, esp + 12); uc.reg_write(UC_X86_REG_EIP, ret)
    else:
        state['stop'] = (n, hex(ret)); uc.emu_stop()
mu.hook_add(UC_HOOK_CODE, hook, begin=API, end=API + 0x1000000)
try:
    mu.emu_start(base + p.OPTIONAL_HEADER.AddressOfEntryPoint, 0, count=4_000_000_000)
except UcError as e:
    print('emu error', e, hex(mu.reg_read(UC_X86_REG_EIP)))
print('stopped at', state['stop'], 'dlls', list(strings), 'imports', len(names))
open(out, 'wb').write(bytes(mu.mem_read(base, size)))
