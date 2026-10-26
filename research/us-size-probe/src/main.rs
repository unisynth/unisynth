//! Unified-synth size probe (i686, no_std, no CRT): loads an embedded unified song, renders all of it to 16-bit PCM in
//! 4096-frame chunks and exits with a checksum, so every enabled module is reachable and nothing is dead-stripped.
//! No audio output: this measures the synth's code size (plus the song's bytes), not a player.
#![no_std]
#![no_main]
#![allow(non_snake_case, unexpected_cfgs)]

use synth_core::mix::{Env, Player, Song, DEFAULT_SUB};
use synth_core::pool::Pool;
use unisynth::device::UDevice;
use unisynth::rng::Xorshift;

static SONG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/song.bin"));
const MEM_BYTES: usize = 32 << 20; // .bss: costs no file size
static mut MEM: [u8; MEM_BYTES] = [0; MEM_BYTES];
static mut PCM: [i16; 2 * 4096] = [0; 2 * 4096];

#[link(name = "kernel32", kind = "raw-dylib", import_name_type = "undecorated")]
unsafe extern "system" {
    fn ExitProcess(code: u32) -> !;
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn start() -> ! {
    let Ok(song) = Song::parse(SONG) else { unsafe { ExitProcess(2) } };
    let env = Env::for_song(&song, None);
    let mem = unsafe { &mut *core::ptr::addr_of_mut!(MEM) };
    let mut pool = Pool::new(mem);
    let Ok(mut p) = Player::<UDevice, Xorshift>::new(song, &env, &mut pool, Xorshift::new(1), DEFAULT_SUB) else {
        unsafe { ExitProcess(3) }
    };
    let pcm = unsafe { &mut *core::ptr::addr_of_mut!(PCM) };
    let (mut left, mut sum) = (p.length_frames(), 0u32);
    while left > 0 {
        let n = left.min(4096) as usize;
        p.render_i16(&mut pcm[..2 * n]);
        sum = pcm[..2 * n].iter().fold(sum, |h, &s| h.rotate_left(5) ^ s as u16 as u32);
        left -= n as u64;
    }
    unsafe { ExitProcess(sum) }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    unsafe { ExitProcess(1) }
}

// MSVC targets reference this symbol whenever floats are used.
#[unsafe(no_mangle)]
pub static _fltused: i32 = 0;

// LLVM lowers 64-bit div/rem to the MSVC CRT helpers (callee-pops); forward them to compiler_builtins' cdecl versions.
core::arch::global_asm!(
    ".globl __aulldiv", "__aulldiv:",
    "push dword ptr [esp+16]", "push dword ptr [esp+16]", "push dword ptr [esp+16]", "push dword ptr [esp+16]",
    "call ___udivdi3", "add esp, 16", "ret 16",
    ".globl __aullrem", "__aullrem:",
    "push dword ptr [esp+16]", "push dword ptr [esp+16]", "push dword ptr [esp+16]", "push dword ptr [esp+16]",
    "call ___umoddi3", "add esp, 16", "ret 16",
    ".globl __alldiv", "__alldiv:",
    "push dword ptr [esp+16]", "push dword ptr [esp+16]", "push dword ptr [esp+16]", "push dword ptr [esp+16]",
    "call ___divdi3", "add esp, 16", "ret 16",
    ".globl __allrem", "__allrem:",
    "push dword ptr [esp+16]", "push dword ptr [esp+16]", "push dword ptr [esp+16]", "push dword ptr [esp+16]",
    "call ___moddi3", "add esp, 16", "ret 16",
);

// Stack probe for frames over 4 KB (the Player value in `start`): MSVC x86 contract — size in eax, touch every page
// from the old top down, then move esp itself (callee-adjusting, like compiler-rt's i386 `_alloca`).
core::arch::global_asm!(
    ".globl __chkstk", "__chkstk:",
    "push ecx",
    "lea ecx, [esp+8]",          // esp before the call
    "cmp eax, 0x1000",
    "jb 3f",
    "2:",
    "sub ecx, 0x1000",
    "test dword ptr [ecx], ecx",
    "sub eax, 0x1000",
    "cmp eax, 0x1000",
    "ja 2b",
    "3:",
    "sub ecx, eax",
    "test dword ptr [ecx], ecx",
    "lea eax, [esp+4]",          // -> return address
    "mov esp, ecx",              // new stack top
    "mov ecx, dword ptr [eax-4]", // restore ecx
    "push dword ptr [eax]",      // return address on the new top
    "sub eax, esp",              // restore eax (original size)
    "ret",
);

// The compiler's block copies and fills call the C runtime, which this probe does not link: `rep` string instructions
// (cdecl, direction flag clear on entry per the x86 ABI; esi/edi are callee-saved).
#[cfg(not(builtin_mem))]
core::arch::global_asm!(
    ".globl _memcpy", "_memcpy:",
    "push esi", "push edi",
    "mov edi, [esp+12]", "mov esi, [esp+16]", "mov ecx, [esp+20]", "mov eax, edi",
    "rep movsb", "pop edi", "pop esi", "ret",
    ".globl _memmove", "_memmove:",
    "push esi", "push edi",
    "mov edi, [esp+12]", "mov esi, [esp+16]", "mov ecx, [esp+20]", "mov eax, edi",
    "cmp edi, esi", "jbe 2f",
    "lea edx, [esi+ecx]", "cmp edi, edx", "jae 2f",
    "lea edi, [edi+ecx-1]", "lea esi, [esi+ecx-1]", "std", "rep movsb", "cld", "jmp 3f",
    "2:", "rep movsb",
    "3:", "pop edi", "pop esi", "ret",
    ".globl _memset", "_memset:",
    "push edi",
    "mov edi, [esp+8]", "movzx eax, byte ptr [esp+12]", "mov ecx, [esp+16]", "mov edx, edi",
    "rep stosb", "mov eax, edx", "pop edi", "ret",
);

#[cfg(not(builtin_mem))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(a: *const core::ffi::c_void, b: *const core::ffi::c_void, n: usize) -> i32 {
    let (a, b) = (a as *const u8, b as *const u8);
    let mut i = 0;
    while i < n {
        let (x, y) = unsafe { (*a.add(i), *b.add(i)) };
        if x != y {
            return x as i32 - y as i32;
        }
        i += 1;
    }
    0
}

// compiler_builtins' 64-bit division carries an exception-table reference this target never uses.
#[unsafe(no_mangle)]
pub extern "C" fn __CxxFrameHandler3() {}
