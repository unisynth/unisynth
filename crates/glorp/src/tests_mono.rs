//! Tests of the shared scheduler's mono (voice mode 1) note-stack mode with a recording dummy voice (no DSP).
//!
//! Behaviour under test (research/glorp-devices.md §ID 14, "voice mode"): in mono mode a note-on pushes onto a 128-cell
//! stack and marks the note held; it starts a voice only when no note was sounding (`playing` false); later notes
//! change the pitch of a sounding voice (not audible for ID 14, so not modelled). A note-off clears the held flag;
//! only when the released note is the stack top does the stack pop (skipping notes no longer held): an empty stack
//! releases every active voice and clears the state. The stack shares its 129th cell with its own depth counter.
use crate::synth::{ActiveNote, Synth, Voice};
use std::cell::RefCell;

#[derive(Clone, Copy, Default)]
struct M {
    id: usize,
    active: bool,
    note: u8,
    released: u32,
    life: u32,
}

#[derive(Default)]
struct Ctx {
    starts: RefCell<Vec<(u8, usize)>>,
}

impl Voice<Ctx> for M {
    fn active(&self) -> bool {
        self.active
    }
    fn note(&self) -> u8 {
        self.note
    }
    fn start(&mut self, note: u8, c: &Ctx) {
        *self = M { id: self.id, active: true, note, released: 0, life: 1_000_000 };
        c.starts.borrow_mut().push((note, self.id));
    }
    fn release(&mut self) {
        self.released += 1;
    }
    fn render(&mut self, _: &Ctx, l: &mut [f32], _r: &mut [f32]) {
        for x in l.iter_mut() {
            *x += 1.0;
            self.life = self.life.saturating_sub(1);
            if self.life == 0 {
                self.active = false;
            }
        }
    }
}
impl ActiveNote for M {
    fn is_active(&self) -> bool {
        self.active
    }
    fn held_note(&self) -> u8 {
        self.note
    }
}

fn mono() -> (Synth<M>, Ctx) {
    let mut s = Synth::new(std::array::from_fn(|id| M { id, ..M::default() }));
    s.set_mono(true, |v| v.active = false);
    (s, Ctx::default())
}

fn step(s: &mut Synth<M>, c: &Ctx) {
    let (mut l, mut r) = ([0.0f32; 4], [0.0f32; 4]);
    s.process(c, &mut l, &mut r);
}

fn on(s: &mut Synth<M>, c: &Ctx, n: u8) {
    s.note_on(n, 0).unwrap();
    step(s, c);
}
fn off(s: &mut Synth<M>, c: &Ctx, n: u8) {
    s.note_off(n, 0).unwrap();
    step(s, c);
}
fn released(s: &Synth<M>) -> u32 {
    s.voices().iter().map(|v| v.released).sum()
}

#[test]
fn mode_flag_and_default() {
    let s: Synth<M> = Synth::new(std::array::from_fn(|id| M { id, ..M::default() }));
    assert!(!s.is_mono());
    let (s, _) = mono();
    assert!(s.is_mono());
}

#[test]
fn the_first_note_starts_one_voice_and_notes_while_held_start_none() {
    let (mut s, c) = mono();
    on(&mut s, &c, 60);
    assert_eq!(*c.starts.borrow(), vec![(60, 0)]);
    on(&mut s, &c, 64);
    on(&mut s, &c, 67);
    assert_eq!(*c.starts.borrow(), vec![(60, 0)], "legato notes start nothing");
    assert_eq!(s.active_voices(), 1);
    assert_eq!(s.mono_depth(), 3);
}

#[test]
fn releasing_the_top_falls_back_to_a_held_note_and_the_last_release_releases_all() {
    let (mut s, c) = mono();
    on(&mut s, &c, 60);
    on(&mut s, &c, 64);
    off(&mut s, &c, 64); // top popped, 60 still held
    assert_eq!(released(&s), 0);
    assert_eq!(s.mono_depth(), 1);
    off(&mut s, &c, 60);
    assert_eq!(released(&s), 1, "stack empty: the sounding voice is released");
    assert_eq!(s.mono_depth(), 0);
    // a fresh note now starts a new voice (the first one is still releasing, so it takes slot 1)
    on(&mut s, &c, 62);
    assert_eq!(*c.starts.borrow(), vec![(60, 0), (62, 1)]);
}

#[test]
fn releasing_a_note_that_is_not_the_top_does_nothing_until_it_is_popped() {
    let (mut s, c) = mono();
    on(&mut s, &c, 60);
    on(&mut s, &c, 64);
    off(&mut s, &c, 60); // not the top: only the held flag is cleared
    assert_eq!((released(&s), s.mono_depth()), (0, 2));
    off(&mut s, &c, 64); // top: popped, then 60 (no longer held) popped too, stack empty
    assert_eq!((released(&s), s.mono_depth()), (1, 0));
}

#[test]
fn a_repeated_note_pushes_again_and_one_release_pops_only_the_top_copy() {
    let (mut s, c) = mono();
    on(&mut s, &c, 60);
    on(&mut s, &c, 60);
    assert_eq!(s.mono_depth(), 2);
    off(&mut s, &c, 60); // held flag cleared; top is 60: pop, pop, empty
    assert_eq!((released(&s), s.mono_depth()), (1, 0));
}

#[test]
fn a_voice_that_died_while_keys_are_held_is_not_replaced() {
    let (mut s, c) = mono();
    on(&mut s, &c, 60);
    s.set_mono(true, |_| ()); // same mode: must not touch anything
    assert_eq!(s.active_voices(), 1);
    let (mut l, mut r) = (vec![0.0f32; 1_000_000], vec![0.0f32; 1_000_000]);
    s.process(&c, &mut l, &mut r);
    assert_eq!(s.active_voices(), 0, "the dummy ended");
    on(&mut s, &c, 62); // keys still held (60): playing stays set, nothing starts
    assert_eq!(c.starts.borrow().len(), 1);
    off(&mut s, &c, 60);
    off(&mut s, &c, 62);
    on(&mut s, &c, 64); // everything released: starts again
    assert_eq!(c.starts.borrow().len(), 2);
}

#[test]
fn a_stray_note_off_of_note_zero_with_an_empty_stack_releases_every_voice() {
    let (mut s, c) = mono();
    on(&mut s, &c, 60);
    off(&mut s, &c, 60); // released once, stack empty
    assert_eq!(released(&s), 1);
    off(&mut s, &c, 5); // not the (zero) top cell: nothing
    assert_eq!(released(&s), 1);
    off(&mut s, &c, 0); // equals the cell the empty-stack test reads: releases every active voice again
    assert_eq!(released(&s), 2);
}

#[test]
fn the_129th_cell_of_the_stack_is_its_own_depth() {
    let (mut s, c) = mono();
    for _ in 0..128 {
        on(&mut s, &c, 3);
    }
    assert_eq!(s.mono_depth(), 128);
    on(&mut s, &c, 9); // writes the note into the depth cell, then increments it
    assert_eq!(s.mono_depth(), 10);
}

#[test]
fn changing_the_mode_clears_the_queue_the_stack_and_stops_the_voices() {
    let (mut s, c) = mono();
    on(&mut s, &c, 60);
    s.note_on(61, 50).unwrap(); // still queued
    let mut stopped = 0;
    s.set_mono(false, |v| {
        v.active = false;
        stopped += 1;
    });
    assert_eq!(stopped, 16);
    assert!(!s.is_mono());
    assert_eq!((s.active_voices(), s.mono_depth()), (0, 0));
    // the queue is empty: a full queue's worth fits again and the queued note-on never fires
    let (mut l, mut r) = ([0.0f32; 128], [0.0f32; 128]);
    s.process(&c, &mut l, &mut r);
    assert_eq!(c.starts.borrow().len(), 1, "the queued note-on was dropped");
    // the same mode again does nothing
    s.set_mono(false, |_| panic!("no stop on an unchanged mode"));
    // poly now: every note starts its own voice
    s.note_on(70, 0).unwrap();
    s.note_on(71, 0).unwrap();
    step(&mut s, &c);
    assert_eq!(s.active_voices(), 2);
}
