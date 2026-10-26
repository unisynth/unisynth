//! "No allocation while rendering": 10 s of a song in 128-frame calls (with audition and parameter edits between
//! calls, and loop wraps inside calls) under a counting allocator.

mod common;
use common::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;
static COUNT: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static A: Counting = Counting;

#[test]
fn render_calls_do_not_allocate() {
    let b = blob(&song_json(64));
    let mut h = synth_wasm::Host::new();
    h.load(&b).unwrap();
    h.set_loop(Some((22050, 88200 + 77)));
    h.play(0);
    let mut inside = 0;
    for i in 0..(44100 * 10 / 128) {
        if i % 50 == 0 {
            h.note_on(1, 60 + (i % 12) as u8, 90);
            h.set_param(0, 0, unisynth::params::FILT + 1, (i % 100) as f32 / 100.0);
        }
        if i % 50 == 25 {
            h.note_off(1, 60 + ((i - 25) % 12) as u8);
        }
        let before = COUNT.load(Ordering::Relaxed);
        let _ = h.render(128);
        inside += COUNT.load(Ordering::Relaxed) - before;
    }
    assert_eq!(inside, 0, "allocations inside render calls");
}
