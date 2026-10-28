//! Threaded bounce (change add-wasm-threads, feature `threads`, artifact `synth-mt.wasm`): the bounce Worker renders
//! through [`Player::render_f32_par`](synth_core::mix::Player::render_f32_par) on a [`song::workers::Pool`] whose
//! helpers are Web Workers that share this module's memory.
//!
//! Thread start (design D2): a helper Worker instantiates the module with the shared `env.memory`, then — before any
//! other call — sets the exported `__stack_pointer` to the top of a stack and calls `__wasm_init_tls` with a TLS
//! block, both allocated by the bounce Worker with [`alloc_block`]. It then calls [`pool_helper`], which blocks until
//! [`bounce_close`].
//!
//! Invariant: helpers call only `pool_helper` (plus the start above). Every other export, and therefore every access to
//! the module's `static mut` state, stays on the bounce Worker.
//!
//! Lifetime (design D4b): one shared memory per bounce attempt. Locks live in that memory and `Worker.terminate()` can
//! stop a thread holding one, so after a cancel or any failure the memory and every instance are discarded, never
//! reused. A pool created for `H` helpers waits for all `H` in every run: call [`bounce_step_par`] only after every
//! helper has started (else the step never returns).

use std::sync::{Arc, Mutex};

use song::workers::Pool;

/// The pool of the open threaded bounce. Read by helpers in [`pool_helper`], written by the bounce Worker.
static POOL: Mutex<Option<Arc<Pool>>> = Mutex::new(None);

/// Zeroed heap block of `size` bytes aligned to `align`, never freed: a helper's stack (`alloc_block(size, 16) +
/// size` is its top) or TLS block (`__tls_size`, `__tls_align`). Returns 0 when out of memory.
#[unsafe(no_mangle)]
pub extern "C" fn alloc_block(size: usize, align: usize) -> usize {
    match std::alloc::Layout::from_size_align(size.max(1), align.max(1)) {
        // SAFETY: non-zero size.
        Ok(l) => unsafe { std::alloc::alloc_zeroed(l) as usize },
        Err(_) => 0,
    }
}

/// [`bounce_open`](crate::bounce_open) with a pool for `helpers` helper Workers; returns the total frames (0 on error,
/// issues in OUT). The caller must start exactly `helpers` helpers (each calling [`pool_helper`]) before the first
/// [`bounce_step_par`].
///
/// # Safety
/// `p`/`n` as for `bounce_open`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bounce_open_par(p: *const u8, n: usize, helpers: usize) -> f64 {
    bounce_close();
    let total = unsafe { crate::bounce_open(p, n) };
    if total > 0.0 {
        *POOL.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(Pool::new(helpers)));
    }
    total
}

/// Helper Worker body: takes part in every parallel run of the open bounce until [`bounce_close`]. Returns 0 at once
/// when no threaded bounce is open, else 1 when stopped.
#[unsafe(no_mangle)]
pub extern "C" fn pool_helper() -> u32 {
    let Some(pool) = POOL.lock().unwrap_or_else(|e| e.into_inner()).clone() else { return 0 };
    pool.helper();
    1
}

/// [`bounce_step`](crate::bounce_step) through the pool (bit-identical output).
#[unsafe(no_mangle)]
#[allow(static_mut_refs)]
pub extern "C" fn bounce_step_par(frames: usize) -> *const f32 {
    let Some(pool) = POOL.lock().unwrap_or_else(|e| e.into_inner()).clone() else { return std::ptr::null() };
    // SAFETY: bounce state is only touched by the bounce Worker (module invariant).
    let Some(b) = (unsafe { crate::BOUNCER.as_mut() }) else { return std::ptr::null() };
    let n = (frames.min(crate::MAX_FRAMES) as u64).min(b.left) as usize;
    b.player.render_f32_par(&mut b.buf[..2 * n], &*pool);
    b.left -= n as u64;
    b.buf.as_ptr()
}

/// Ends the threaded bounce: helpers return from [`pool_helper`], the bounce state is dropped.
#[unsafe(no_mangle)]
pub extern "C" fn bounce_close() {
    if let Some(pool) = POOL.lock().unwrap_or_else(|e| e.into_inner()).take() {
        pool.stop();
    }
    // SAFETY: bounce Worker only.
    unsafe { crate::BOUNCER = None };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native stand-in for the Web Workers: std threads call `pool_helper`; the threaded bounce of a dense project
    /// song equals the single-thread bounce frame for frame.
    #[test]
    fn bounce_par_equals_bounce() {
        let json = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../project/music/elysian.song.json")).unwrap();
        let blob = song::export(&song::load(&json).unwrap()).unwrap();
        let frames = 3 * 44_100; // 3 s
        let render = |par: bool| -> Vec<f32> {
            let total = unsafe { if par { bounce_open_par(blob.as_ptr(), blob.len(), 3) } else { crate::bounce_open(blob.as_ptr(), blob.len()) } };
            assert!(total > 0.0);
            let hs: Vec<_> = if par { (0..3).map(|_| std::thread::spawn(|| assert_eq!(pool_helper(), 1))).collect() } else { vec![] };
            let mut out = vec![];
            while out.len() < 2 * frames {
                let k = 1000.min(frames - out.len() / 2);
                let p = if par { bounce_step_par(k) } else { crate::bounce_step(k) };
                out.extend_from_slice(unsafe { std::slice::from_raw_parts(p, 2 * k) });
            }
            bounce_close();
            hs.into_iter().for_each(|h| h.join().unwrap());
            out
        };
        let want = render(false);
        assert!(want.iter().any(|x| x.abs() > 0.01), "audible");
        assert!(render(true) == want);
        assert_eq!(pool_helper(), 0, "no bounce open");
    }
}
