//! Bump allocator over caller memory. In the compo exe the memory is a zeroed `static` in BSS (no file bytes);
//! in tools and tests it is any byte slice. Nothing is freed; destructors of pool values never run.

use core::mem::{align_of, size_of};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfMemory;

/// Types whose all-zero bit pattern is a valid value.
///
/// # Safety
/// Implement only for types where every field accepts all-zero bytes.
pub unsafe trait Zeroed: Copy {}
unsafe impl Zeroed for u8 {}
unsafe impl Zeroed for i8 {}
unsafe impl Zeroed for u16 {}
unsafe impl Zeroed for i16 {}
unsafe impl Zeroed for u32 {}
unsafe impl Zeroed for i32 {}
unsafe impl Zeroed for u64 {}
unsafe impl Zeroed for i64 {}
unsafe impl Zeroed for usize {}
unsafe impl Zeroed for f32 {}
unsafe impl Zeroed for f64 {}

pub struct Pool<'a> {
    rest: &'a mut [u8],
}

impl<'a> Pool<'a> {
    pub fn new(mem: &'a mut [u8]) -> Self {
        Pool { rest: mem }
    }

    /// Bytes still free (before alignment padding).
    pub fn remaining(&self) -> usize {
        self.rest.len()
    }

    /// Upper bound of the bytes `slice::<T>(n)` / `value::<T>` (n = 1) consumes, including worst-case padding.
    /// Sum these to size a pool.
    pub const fn need<T>(n: usize) -> usize {
        size_of::<T>() * n + align_of::<T>() - 1
    }

    fn take(&mut self, size: usize, align: usize) -> Result<&'a mut [u8], OutOfMemory> {
        let pad = (self.rest.as_ptr() as usize).wrapping_neg() & (align - 1);
        if pad.checked_add(size).is_none_or(|end| end > self.rest.len()) {
            return Err(OutOfMemory);
        }
        let rest = core::mem::take(&mut self.rest);
        let (got, rest) = rest[pad..].split_at_mut(size);
        self.rest = rest;
        Ok(got)
    }

    /// `n` zeroed values.
    pub fn slice<T: Zeroed>(&mut self, n: usize) -> Result<&'a mut [T], OutOfMemory> {
        let size = size_of::<T>().checked_mul(n).ok_or(OutOfMemory)?;
        let bytes = self.take(size, align_of::<T>())?;
        bytes.fill(0);
        // SAFETY: aligned, in bounds, exclusively borrowed for 'a, and zero bytes are a valid `T` (`Zeroed`).
        Ok(unsafe { core::slice::from_raw_parts_mut(bytes.as_mut_ptr().cast::<T>(), n) })
    }

    /// `n` values built by `f(index)`.
    pub fn slice_with<T>(&mut self, n: usize, mut f: impl FnMut(usize) -> T) -> Result<&'a mut [T], OutOfMemory> {
        let size = size_of::<T>().checked_mul(n).ok_or(OutOfMemory)?;
        let p = self.take(size, align_of::<T>())?.as_mut_ptr().cast::<T>();
        for i in 0..n {
            // SAFETY: aligned, in bounds (n elements reserved above), each slot written once before the slice exists.
            unsafe { p.add(i).write(f(i)) };
        }
        // SAFETY: all n elements initialised; exclusively borrowed for 'a.
        Ok(unsafe { core::slice::from_raw_parts_mut(p, n) })
    }

    /// `n` uninitialised slots, for values whose construction itself allocates from this pool.
    pub fn uninit<T>(&mut self, n: usize) -> Result<&'a mut [core::mem::MaybeUninit<T>], OutOfMemory> {
        let size = size_of::<T>().checked_mul(n).ok_or(OutOfMemory)?;
        let p = self.take(size, align_of::<T>())?.as_mut_ptr().cast::<core::mem::MaybeUninit<T>>();
        // SAFETY: aligned, in bounds, exclusively borrowed for 'a; MaybeUninit needs no initialisation.
        Ok(unsafe { core::slice::from_raw_parts_mut(p, n) })
    }

    /// Moves `v` into the pool.
    pub fn value<T>(&mut self, v: T) -> Result<&'a mut T, OutOfMemory> {
        let bytes = self.take(size_of::<T>(), align_of::<T>())?;
        let p = bytes.as_mut_ptr().cast::<T>();
        // SAFETY: aligned, in bounds, exclusively borrowed for 'a; we initialise before handing it out.
        unsafe {
            p.write(v);
            Ok(&mut *p)
        }
    }
}
