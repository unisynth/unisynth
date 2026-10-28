//! Flush subnormals to zero for the duration of a render call (x86 MXCSR FTZ+DAZ, AArch64 FPCR.FZ), restoring the
//! caller's mode on drop. Other targets (wasm) and Miri (no inline asm) do nothing.

/// Current floating-point control word (0 where not applicable).
#[inline]
pub fn mode() -> u64 {
    imp::get()
}

/// A subnormal → 0 where [`Guard`] cannot set the CPU mode (wasm), so feedback state that decays into subnormals does
/// not slow every later sample (≈ 10–100× per op on x86). Identity elsewhere: there the CPU flushes them already.
#[inline(always)]
pub fn flush(x: f32) -> f32 {
    #[cfg(target_arch = "wasm32")]
    if x.abs() < f32::MIN_POSITIVE {
        return 0.0;
    }
    x
}

/// [`flush`] over a device's channel buffers, i.e. DAZ between devices (a reverb tail's subnormals would otherwise
/// slow every device after it). Nothing where the CPU flushes.
#[inline(always)]
pub fn flush_io(ch: &mut [&mut [f32]]) {
    #[cfg(target_arch = "wasm32")]
    for b in ch.iter_mut() {
        b.iter_mut().for_each(|x| *x = flush(*x));
    }
    let _ = ch;
}

pub struct Guard {
    saved: u64,
}

impl Guard {
    #[inline]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let saved = imp::get();
        imp::set(saved | imp::FLUSH);
        Guard { saved }
    }
}

impl Drop for Guard {
    #[inline]
    fn drop(&mut self) {
        imp::set(self.saved);
    }
}

#[cfg(all(any(target_arch = "x86", target_arch = "x86_64"), not(miri)))]
mod imp {
    use core::arch::asm;
    /// FTZ (bit 15) | DAZ (bit 6).
    pub const FLUSH: u64 = 0x8040;
    pub fn get() -> u64 {
        let mut m: u32 = 0;
        // SAFETY: stores MXCSR into a local.
        unsafe { asm!("stmxcsr [{}]", in(reg) &mut m, options(nostack, preserves_flags)) };
        m as u64
    }
    pub fn set(m: u64) {
        let m = m as u32;
        // SAFETY: loads a valid MXCSR value (read from the register, plus FTZ/DAZ).
        unsafe { asm!("ldmxcsr [{}]", in(reg) &m, options(nostack, readonly, preserves_flags)) };
    }
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
mod imp {
    use core::arch::asm;
    /// FPCR.FZ (bit 24).
    pub const FLUSH: u64 = 1 << 24;
    pub fn get() -> u64 {
        let m: u64;
        // SAFETY: reads FPCR.
        unsafe { asm!("mrs {}, fpcr", out(reg) m, options(nomem, nostack, preserves_flags)) };
        m
    }
    pub fn set(m: u64) {
        // SAFETY: writes back FPCR with only FZ changed.
        unsafe { asm!("msr fpcr, {}", in(reg) m, options(nomem, nostack, preserves_flags)) };
    }
}

#[cfg(any(miri, not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))))]
mod imp {
    pub const FLUSH: u64 = 0;
    pub fn get() -> u64 {
        0
    }
    pub fn set(_: u64) {}
}
