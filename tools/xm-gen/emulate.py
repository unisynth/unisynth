"""Run an unpacked intro's sample calculation in unicorn, in ONE uninterrupted `emu_start` (time-slicing unicorn
corrupts the x87 state: a sliced Theta run differed by 512 bytes). Change add-plain-xm-import.

    Emu(image, base=0x10000)            the image mapped flat at `base`, a 64 MB heap, DPMI 0x501 allocation
    .poke(addr, dword) .seed(addr, value) .alloc_table(addr)
    .who(start, length)                 per byte of [start, start+length): the EIP of its last writer
    .on(addr, fn)                       call fn(emu) when execution reaches addr
    .run(start, stop)                   from `start` (after setting the FPU control word as the Watcom startup does,
                                        0x1F7F: truncating, 64-bit mantissa) to `stop`
"""
import array, struct
from unicorn import *
from unicorn.x86_const import *

HEAP = 0x01000000
HEAP_SIZE = 0x04000000
STACK = HEAP + 0x03ff0000
STUB = HEAP + 0x03ffff00


class Emu:
    def __init__(self, image, base=0x10000):
        self.mu = mu = Uc(UC_ARCH_X86, UC_MODE_32)
        mu.mem_map(0, (base + len(image) + 0xfffff) & ~0xfffff)
        mu.mem_write(base, image)
        mu.mem_map(HEAP, HEAP_SIZE)
        self.next = HEAP
        self.writers = None
        self.hooks = {}
        mu.hook_add(UC_HOOK_INTR, self._intr)
        mu.hook_add(UC_HOOK_MEM_INVALID, self._bad)

    def reg(self, r):
        return self.mu.reg_read(r)

    def dword(self, addr):
        return struct.unpack('<I', self.mu.mem_read(addr, 4))[0]

    def poke(self, addr, value):
        self.mu.mem_write(addr, struct.pack('<I', value))

    def seed(self, addr, value):
        """What the intro's srand() stores: the low and high words."""
        self.mu.mem_write(addr, struct.pack('<2H', value & 0xffff, value >> 16))

    def alloc_table(self, addr, size=0x20000):
        """The DPMI allocator's handle table, as its startup leaves it (free slots are -1)."""
        self.mu.mem_write(addr, struct.pack('<2I', 0, 0xffffffff) * (size // 8))

    def who(self, start, length):
        self.who_at = start
        self.writers = array.array('I', bytes(4 * length))

        def wr(uc, acc, addr, sz, val, _):
            e = uc.reg_read(UC_X86_REG_EIP)
            for i in range(max(addr, start), min(addr + sz, start + length)):
                self.writers[i - start] = e
        self.mu.hook_add(UC_HOOK_MEM_WRITE, wr, begin=start, end=start + length - 1)

    def on(self, addr, fn):
        self.mu.hook_add(UC_HOOK_CODE, lambda uc, a, sz, _: fn(self), begin=addr, end=addr)

    def _intr(self, uc, no, _):
        eax = uc.reg_read(UC_X86_REG_EAX)
        if no == 0x31 and (eax & 0xffff) == 0x501:  # DPMI allocate: BX:CX bytes -> BX:CX address, SI:DI handle
            n = (uc.reg_read(UC_X86_REG_EBX) & 0xffff) << 16 | (uc.reg_read(UC_X86_REG_ECX) & 0xffff)
            a = self.next
            self.next = (a + n + 0xfff) & ~0xfff
            for r, v in ((UC_X86_REG_EBX, a >> 16), (UC_X86_REG_ECX, a & 0xffff), (UC_X86_REG_ESI, a >> 16), (UC_X86_REG_EDI, a & 0xffff)):
                uc.reg_write(r, (uc.reg_read(r) & ~0xffff) | v)
        elif no not in (0x31, 0x21):  # other DPMI calls succeed; DOS prints are ignored
            raise SystemExit(f'unhandled int {no:#x} eax {eax:#x} at {uc.reg_read(UC_X86_REG_EIP):#x}')
        uc.reg_write(UC_X86_REG_EFLAGS, uc.reg_read(UC_X86_REG_EFLAGS) & ~1)

    def _bad(self, uc, access, addr, sz, val, _):
        raise SystemExit(f'invalid memory access {access} at {addr:#x}, eip {uc.reg_read(UC_X86_REG_EIP):#x}')

    def run(self, start, stop, ebx=0x30):
        mu = self.mu
        # fninit; push 0x1f7f; fldcw [esp]; add esp, 4; jmp start
        code = bytes.fromhex('dbe3' '687f1f0000' 'd92c24' '83c404' 'e9') + struct.pack('<i', start - (STUB + 18))
        mu.mem_write(STUB, code)
        mu.reg_write(UC_X86_REG_ESP, STACK)
        mu.reg_write(UC_X86_REG_EBP, STACK + 0x100)
        mu.reg_write(UC_X86_REG_EBX, ebx)  # the GUS port key the intros ask for ('0': 0x200)
        mu.emu_start(STUB, stop, timeout=0, count=0)
