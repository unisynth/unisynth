//! Tests of the shared 16-voice scheduler with a recording dummy voice (no DSP): event timing, sub-block splitting, voice
//! allocation (lowest free, no stealing), note-off matching, queue slots and capacity, voices that free themselves.
use crate::synth::{ActiveNote, Error, Synth, Voice, MAX_NOTE, QUEUE, VOICES};
use std::cell::RefCell;

#[derive(Clone, Copy, Default)]
struct Dummy {
    id: usize,
    active: bool,
    note: u8,
    left: u32,
}

#[derive(Default)]
struct Ctx {
    /// `(note, voice)` in the order voices were started.
    starts: RefCell<Vec<(u8, usize)>>,
}

/// A voice lives `10 · (note + 1)` samples; it adds 1 to the left channel and `2^voice` to the right one per sample, and
/// clears its own `active` flag when its life is over (keeping on rendering the rest of that sub-block, like the real ones).
impl Voice<Ctx> for Dummy {
    fn active(&self) -> bool {
        self.active
    }
    fn note(&self) -> u8 {
        self.note
    }
    fn start(&mut self, note: u8, c: &Ctx) {
        *self = Dummy { id: self.id, active: true, note, left: 10 * (note as u32 + 1) };
        c.starts.borrow_mut().push((note, self.id));
    }
    fn release(&mut self) {} // a note-off does not end the dummy: only its life does
    fn render(&mut self, c: &Ctx, l: &mut [f32], r: &mut [f32]) {
        let _ = c;
        for i in 0..l.len() {
            l[i] += 1.0;
            r[i] += (1u32 << self.id) as f32;
            if self.left > 0 {
                self.left -= 1;
                if self.left == 0 {
                    self.active = false;
                }
            }
        }
    }
}

/// Voice that logs releases (needs interior state in the context, so it keeps its own counter).
#[derive(Clone, Copy, Default)]
struct Rel {
    d: Dummy,
    released: u32,
}
impl Voice<Ctx> for Rel {
    fn active(&self) -> bool {
        self.d.active
    }
    fn note(&self) -> u8 {
        self.d.note
    }
    fn start(&mut self, note: u8, c: &Ctx) {
        self.d.start(note, c);
    }
    fn release(&mut self) {
        self.released += 1;
    }
    fn render(&mut self, c: &Ctx, l: &mut [f32], r: &mut [f32]) {
        self.d.render(c, l, r)
    }
}
impl ActiveNote for Rel {
    fn is_active(&self) -> bool {
        self.d.active
    }
    fn held_note(&self) -> u8 {
        self.d.note
    }
}

fn synth() -> Synth<Dummy> {
    Synth::new(std::array::from_fn(|id| Dummy { id, ..Dummy::default() }))
}

fn run(s: &mut Synth<Dummy>, c: &Ctx, n: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut l, mut r) = (vec![7.0; n], vec![7.0; n]);
    s.process(c, &mut l, &mut r);
    (l, r)
}

#[test]
fn events_fire_at_their_offset_and_split_the_sub_block() {
    let (mut s, c) = (synth(), Ctx::default());
    s.note_on(100, 10).unwrap();
    s.note_on(101, 10).unwrap();
    s.note_on(102, 50).unwrap();
    let (l, r) = run(&mut s, &c, 128);
    assert!(l[..10].iter().all(|&x| x == 0.0) && r[..10].iter().all(|&x| x == 0.0), "nothing before the first event");
    assert!(l[10..50].iter().all(|&x| x == 2.0) && r[10..50].iter().all(|&x| x == 3.0), "voices 0 and 1 from sample 10");
    assert!(l[50..].iter().all(|&x| x == 3.0) && r[50..].iter().all(|&x| x == 7.0), "voice 2 from sample 50");
    assert_eq!(*c.starts.borrow(), vec![(100, 0), (101, 1), (102, 2)]);
}

#[test]
fn output_is_zeroed_first_and_idle_voices_do_not_render() {
    let (mut s, c) = (synth(), Ctx::default());
    let (l, r) = run(&mut s, &c, 64);
    assert!(l.iter().chain(&r).all(|&x| x == 0.0));
}

#[test]
fn delays_persist_across_calls_and_shrink() {
    let (mut s, c) = (synth(), Ctx::default());
    s.note_on(60, 300).unwrap();
    assert!(run(&mut s, &c, 128).0.iter().all(|&x| x == 0.0));
    assert!(run(&mut s, &c, 128).0.iter().all(|&x| x == 0.0));
    let (l, _) = run(&mut s, &c, 128);
    assert_eq!(l.iter().position(|&x| x != 0.0), Some(300 - 256));
}

#[test]
fn lowest_free_voice_first_no_stealing() {
    let (mut s, c) = (synth(), Ctx::default());
    for n in 0..17u8 {
        s.note_on(n, 0).unwrap();
    }
    let (l, _) = run(&mut s, &c, 1);
    assert_eq!(l[0], 16.0, "17th note dropped: nothing to steal");
    assert_eq!(c.starts.borrow().len(), 16);
    assert_eq!(c.starts.borrow()[15], (15, 15));
    // note n lives 10(n+1) samples: after 25 samples voices 0 and 1 are free; a new note takes the lowest, voice 0
    let _ = run(&mut s, &c, 24);
    s.note_on(40, 0).unwrap();
    s.note_on(41, 0).unwrap();
    s.note_on(42, 0).unwrap();
    let _ = run(&mut s, &c, 1);
    let st = c.starts.borrow();
    assert_eq!(&st[16..], &[(40, 0), (41, 1)], "voice 0 then voice 1; the third finds none free");
}

#[test]
fn a_voice_that_frees_itself_renders_the_rest_of_the_sub_block_then_stops() {
    let (mut s, c) = (synth(), Ctx::default());
    s.note_on(0, 0).unwrap(); // lives 10 samples
    let (l, _) = run(&mut s, &c, 128);
    assert!(l.iter().all(|&x| x == 1.0), "all 128 samples rendered although the voice ended after 10");
    let (l, _) = run(&mut s, &c, 128);
    assert!(l.iter().all(|&x| x == 0.0), "freed: not rendered in the next block");
}

#[test]
fn same_delay_events_fire_in_slot_order_and_a_new_event_can_overtake() {
    let (mut s, c) = (synth(), Ctx::default());
    // slot 0 = a (fires now, slot freed), slot 1 = b (delay 300)
    s.note_on(1, 0).unwrap();
    s.note_on(2, 300).unwrap();
    let _ = run(&mut s, &c, 128);
    // b has 172 left; c asks for the same remaining delay but gets the freed slot 0, so it fires first
    s.note_on(3, 172).unwrap();
    let _ = run(&mut s, &c, 300);
    let order: Vec<u8> = c.starts.borrow().iter().map(|s| s.0).collect();
    assert_eq!(order, vec![1, 3, 2]);
    // without a freed slot in between, queue order is firing order
    let (mut s, c) = (synth(), Ctx::default());
    s.note_on(5, 20).unwrap();
    s.note_on(6, 20).unwrap();
    let _ = run(&mut s, &c, 64);
    assert_eq!(*c.starts.borrow(), vec![(5, 0), (6, 1)]);
}

#[test]
fn queue_holds_256_events_and_refuses_more_until_drained() {
    let (mut s, c) = (synth(), Ctx::default());
    for i in 0..QUEUE {
        s.note_on((i % 128) as u8, 5).unwrap();
    }
    assert_eq!(s.note_on(1, 5), Err(Error::QueueFull));
    assert_eq!(s.note_off(1, 5), Err(Error::QueueFull));
    let _ = run(&mut s, &c, 16);
    assert_eq!(c.starts.borrow().len(), VOICES, "256 events fired, 16 voices started, the rest lost");
    s.note_on(1, 0).unwrap();
}

#[test]
fn rejects_bad_notes_and_offsets_without_queueing() {
    let (mut s, c) = (synth(), Ctx::default());
    assert_eq!(s.note_on(MAX_NOTE + 1, 0), Err(Error::Note));
    assert_eq!(s.note_off(255, 0), Err(Error::Note));
    assert_eq!(s.note_on(0, i32::MAX as u32 + 1), Err(Error::Offset));
    assert_eq!(s.note_off(0, u32::MAX), Err(Error::Offset));
    s.note_on(MAX_NOTE, i32::MAX as u32).unwrap(); // accepted: waits ~48 hours
    let (l, _) = run(&mut s, &c, 256);
    assert!(l.iter().all(|&x| x == 0.0));
}

#[test]
fn note_off_releases_every_active_voice_of_that_note_only() {
    let c = Ctx::default();
    let mut s: Synth<Rel> = Synth::new([Rel::default(); VOICES]);
    for n in [7u8, 8, 7, 9] {
        s.note_on(n, 0).unwrap();
    }
    s.note_off(7, 3).unwrap();
    s.note_off(50, 3).unwrap(); // nothing plays it
    let (mut l, mut r) = (vec![0.0; 8], vec![0.0; 8]);
    s.process(&c, &mut l, &mut r);
    let rel: Vec<u32> = s.voices().iter().map(|v| v.released).collect();
    assert_eq!(&rel[..4], &[1, 0, 1, 0]);
    assert!(rel[4..].iter().all(|&x| x == 0));
    // a voice that already freed itself is not released any more
    let (mut l, mut r) = (vec![0.0; 200], vec![0.0; 200]);
    s.process(&c, &mut l, &mut r);
    s.note_off(7, 0).unwrap();
    s.process(&c, &mut l, &mut r);
    assert_eq!(s.voices().iter().map(|v| v.released).sum::<u32>(), 2, "voices 0 and 2 ended (life 80): off is a no-op");
    assert_eq!(s.voice_notes()[..4], [None, None, None, None]);
}

#[test]
#[should_panic(expected = "channel length mismatch")]
fn unequal_lengths_panic() {
    let mut s = synth();
    s.process(&Ctx::default(), &mut [0.0; 3], &mut [0.0; 4]);
}
