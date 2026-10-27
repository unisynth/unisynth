//! Persistent std worker pool implementing `synth_core::mix::Par` (change add-synth-threads, feature `threads`).
//!
//! [`with_workers`] starts `n − 1` scoped helper threads for the duration of a closure; the calling thread is the
//! n-th worker. Each [`Par::run`] is a barrier: every helper takes part in every run (taking jobs from a shared
//! counter until none is left) and reports that it finished, and `run` returns only after all helpers have, so no
//! helper can still hold the job of a finished run. Helpers spin and yield briefly between runs (the mixer issues one run
//! per dependency level per 100-frame sub-block) and park on a condition variable when idle.

use std::any::Any;
use std::cell::UnsafeCell;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::*};
use std::sync::{Condvar, Mutex};

use synth_core::mix::Par;

/// Waiting: `SPIN` busy iterations, then `YIELD` `yield_now`s (helpers; the caller keeps yielding), then park
/// (helpers). Measured on an 8-core / 16-thread machine (research/size-threads): a long busy spin is fastest while
/// workers ≤ physical cores but up to 4× slower once helpers spin on the hyperthread siblings of busy workers;
/// yielding keeps both cases near the best.
const SPIN: u32 = 64;
const YIELD: u32 = 2000;

type Job = *const (dyn Fn(usize) + Sync);

struct Shared {
    /// Run counter: a new value means a new run is published.
    run_id: AtomicU64,
    stop: AtomicBool,
    /// Job of the current run. Written by the caller only while every helper waits for the next `run_id`.
    job: UnsafeCell<Option<Job>>,
    jobs: AtomicUsize,
    next: AtomicUsize,
    /// Helpers done with the current run.
    finished: AtomicUsize,
    /// Parked helpers.
    parked: Mutex<usize>,
    wake: Condvar,
    panic: Mutex<Option<Box<dyn Any + Send>>>,
}

// SAFETY: `job` is written only between runs (all helpers past `finished`, waiting on `run_id`) and read only during a
// run, ordered by the Release/Acquire pairs on `run_id` and `finished`. The job itself is `Sync`.
unsafe impl Sync for Shared {}

impl Shared {
    fn take(&self) {
        // SAFETY: called during a run, after the Acquire of the run's `run_id` (helpers) or by the publishing caller.
        let job = unsafe { &*(*self.job.get()).expect("job published") };
        let jobs = self.jobs.load(Relaxed);
        loop {
            let i = self.next.fetch_add(1, Relaxed);
            if i >= jobs {
                break;
            }
            job(i);
        }
    }

    fn take_catching(&self) {
        if let Err(p) = catch_unwind(AssertUnwindSafe(|| self.take())) {
            self.panic.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert(p);
        }
    }

    fn helper(&self) {
        let mut seen = 0;
        loop {
            let mut spins = 0;
            loop {
                let g = self.run_id.load(Acquire);
                if g != seen {
                    seen = g;
                    break;
                }
                if self.stop.load(Acquire) {
                    return;
                }
                if spins < SPIN {
                    spins += 1;
                    std::hint::spin_loop();
                } else if spins < SPIN + YIELD {
                    spins += 1;
                    std::thread::yield_now();
                } else {
                    let mut p = self.parked.lock().unwrap_or_else(|e| e.into_inner());
                    while self.run_id.load(Acquire) == seen && !self.stop.load(Acquire) {
                        *p += 1;
                        p = self.wake.wait(p).unwrap_or_else(|e| e.into_inner());
                        *p -= 1;
                    }
                }
            }
            self.take_catching();
            self.finished.fetch_add(1, Release);
        }
    }

    fn shut_down(&self) {
        let _p = self.parked.lock().unwrap_or_else(|e| e.into_inner());
        self.stop.store(true, Release);
        self.wake.notify_all();
    }
}

/// `n` workers (the caller plus `n − 1` helper threads); see [`with_workers`].
pub struct Workers<'s> {
    shared: &'s Shared,
    helpers: usize,
}

impl Par for Workers<'_> {
    fn run(&self, jobs: usize, job: &(dyn Fn(usize) + Sync)) {
        if self.helpers == 0 || jobs <= 1 {
            (0..jobs).for_each(job);
            return;
        }
        let sh = self.shared;
        // SAFETY: every helper finished the previous run (barrier below) and waits for the next `run_id`, so nobody reads
        // `job`. The lifetime is erased only for the duration of this call: `run` returns after every helper has
        // finished with it.
        unsafe { *sh.job.get() = Some(std::mem::transmute::<&(dyn Fn(usize) + Sync), Job>(job)) };
        sh.jobs.store(jobs, Relaxed);
        sh.next.store(0, Relaxed);
        sh.finished.store(0, Relaxed);
        {
            let parked = sh.parked.lock().unwrap_or_else(|e| e.into_inner());
            sh.run_id.fetch_add(1, Release);
            if *parked > 0 {
                sh.wake.notify_all();
            }
        }
        sh.take_catching();
        let mut spins = 0;
        while sh.finished.load(Acquire) < self.helpers {
            if spins < SPIN {
                spins += 1;
                std::hint::spin_loop();
            } else {
                std::thread::yield_now();
            }
        }
        // SAFETY: all helpers are done with this run.
        unsafe { *sh.job.get() = None };
        if let Some(p) = sh.panic.lock().unwrap_or_else(|e| e.into_inner()).take() {
            resume_unwind(p);
        }
    }
}

/// Runs `f` with `n ≥ 1` workers; the helper threads exist only while `f` runs (also when it panics).
pub fn with_workers<R>(n: usize, f: impl FnOnce(&Workers) -> R) -> R {
    let shared = Shared {
        run_id: AtomicU64::new(0),
        stop: AtomicBool::new(false),
        job: UnsafeCell::new(None),
        jobs: AtomicUsize::new(0),
        next: AtomicUsize::new(0),
        finished: AtomicUsize::new(0),
        parked: Mutex::new(0),
        wake: Condvar::new(),
        panic: Mutex::new(None),
    };
    struct Stop<'s>(&'s Shared);
    impl Drop for Stop<'_> {
        fn drop(&mut self) {
            self.0.shut_down();
        }
    }
    std::thread::scope(|s| {
        let helpers = n.max(1) - 1;
        for _ in 0..helpers {
            s.spawn(|| shared.helper());
        }
        let _stop = Stop(&shared);
        f(&Workers { shared: &shared, helpers })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Mutex;

    #[test]
    fn every_job_once_and_run_waits_for_all() {
        for n in [1, 2, 3, 8] {
            with_workers(n, |w| {
                for jobs in [0, 1, 2, 5, 64] {
                    for _ in 0..50 {
                        let seen = Mutex::new(vec![]);
                        let threads = Mutex::new(HashSet::new());
                        w.run(jobs, &|i| {
                            std::thread::sleep(std::time::Duration::from_micros((i % 3) as u64 * 20));
                            seen.lock().unwrap().push(i);
                            threads.lock().unwrap().insert(std::thread::current().id());
                        });
                        // returned only after every job finished
                        let mut v = seen.into_inner().unwrap();
                        v.sort();
                        assert_eq!(v, (0..jobs).collect::<Vec<_>>(), "{n} workers, {jobs} jobs");
                        assert!(threads.into_inner().unwrap().len() <= n);
                    }
                }
            });
        }
    }

    #[test]
    fn helpers_park_and_wake() {
        with_workers(4, |w| {
            std::thread::sleep(std::time::Duration::from_millis(50)); // helpers park
            let count = AtomicUsize::new(0);
            w.run(10, &|_| {
                count.fetch_add(1, Relaxed);
            });
            assert_eq!(count.load(Relaxed), 10);
        });
    }

    #[test]
    fn job_panic_reaches_the_caller_and_threads_end() {
        let r = catch_unwind(AssertUnwindSafe(|| {
            with_workers(3, |w| {
                w.run(6, &|i| assert!(i != 4, "job 4 fails"));
            })
        }));
        assert!(r.is_err());
    }
}
