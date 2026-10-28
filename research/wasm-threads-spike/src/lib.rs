//! add-wasm-threads task 1.1: several Web Workers share one memory; each sets its own `__stack_pointer` and TLS
//! block, then increments a shared atomic, keeps a thread-local and waits on a std `Condvar` (memory.atomic.wait32).
use std::cell::Cell;
use std::sync::atomic::{AtomicU32, Ordering::*};
use std::sync::{Condvar, Mutex};

static COUNTER: AtomicU32 = AtomicU32::new(0);
static DONE: Mutex<u32> = Mutex::new(0);
static CV: Condvar = Condvar::new();

thread_local! {
    static ID: Cell<u32> = const { Cell::new(0) };
}

/// Heap block of `size` bytes aligned to `align`, never freed (a stack or TLS block for one Worker).
#[unsafe(no_mangle)]
pub extern "C" fn alloc_block(size: usize, align: usize) -> usize {
    let layout = std::alloc::Layout::from_size_align(size.max(1), align.max(1)).unwrap();
    unsafe { std::alloc::alloc_zeroed(layout) as usize }
}

/// Worker body: thread-local id, `n` atomic increments with a recursive stack user in between, then report done.
/// Returns the thread-local read back at the end (must still be `id`).
#[unsafe(no_mangle)]
pub extern "C" fn work(id: u32, n: u32) -> u32 {
    ID.with(|c| c.set(id));
    let mut acc = 0u64;
    for i in 0..n {
        COUNTER.fetch_add(1, Relaxed);
        if i % 100_000 == 0 {
            acc += deep(64, id as u64); // uses this Worker's stack
        }
    }
    *DONE.lock().unwrap() += 1;
    CV.notify_all();
    let back = ID.with(|c| c.get());
    if acc != (n as u64).div_ceil(100_000) * id as u64 { 0xdead } else { back }
}

#[inline(never)]
fn deep(d: u32, v: u64) -> u64 {
    let local = [v; 16];
    if d == 0 { local.iter().sum::<u64>() / 16 } else { deep(d - 1, v) + std::hint::black_box(local)[d as usize % 16] - v + 1 - 1 }
}

/// Blocks (Condvar) until `workers` have reported; returns the counter. Must run on a Worker.
#[unsafe(no_mangle)]
pub extern "C" fn wait_all(workers: u32) -> u32 {
    let mut d = DONE.lock().unwrap();
    while *d < workers {
        d = CV.wait(d).unwrap();
    }
    COUNTER.load(SeqCst)
}

#[unsafe(no_mangle)]
pub extern "C" fn my_id() -> u32 {
    ID.with(|c| c.get())
}
