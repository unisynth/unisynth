//! tfsynth size probe (i686, no_std, no CRT): loads an embedded synthetic `.tfm`, renders all of it to 16-bit PCM
//! block by block and exits with a checksum, so the engine and every enabled effect are reachable. No audio output:
//! this measures the synth + song player code size.
#![no_std]
#![no_main]
#![allow(non_snake_case)]

use synth_core::pool::Pool;
use tfsynth::player::TfmPlayer;
use tfsynth::tfm::Tfm;

static SONG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/song.tfm"));
const MEM_BYTES: usize = 16 << 20; // .bss: costs no file size
static mut MEM: [u8; MEM_BYTES] = [0; MEM_BYTES];
static mut PCM: [i16; 1024] = [0; 1024];

#[link(name = "kernel32", kind = "raw-dylib", import_name_type = "undecorated")]
unsafe extern "system" {
    fn ExitProcess(code: u32) -> !;
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn start() -> ! {
    let Ok(song) = Tfm::parse(SONG) else { unsafe { ExitProcess(2) } };
    let mem = unsafe { &mut *core::ptr::addr_of_mut!(MEM) };
    let mut pool = Pool::new(mem);
    let Ok(mut p) = TfmPlayer::new(song, &mut pool, 0.0) else { unsafe { ExitProcess(3) } };
    let pcm = unsafe { &mut *core::ptr::addr_of_mut!(PCM) };
    p.start(0.0);
    let mut sum = 0u32;
    for _ in 0..p.blocks() {
        p.render_i16(pcm);
        sum = pcm.iter().fold(sum, |h, &s| h.rotate_left(5) ^ s as u16 as u32);
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
