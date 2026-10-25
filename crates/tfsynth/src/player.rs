//! `.tfm` playback (spec tf-song, reference §2.3/§6.2): one instrument per song slot, events at 4 rows per beat
//! (`row · 60/(tempo·4)` s + a caller start offset, in f32 like the reference) fired at the start of the 512-frame
//! block whose window `[t, t + 512/sr)` contains them, per instrument in stored order; note 0 ignored, velocity 0 =
//! note-off. Output: interleaved stereo i16 with the reference mix (each instrument ×11 into the bus, bus
//! ×(2000·volume), clamp, truncate) or f32 = the plain instrument sum before any scaling.

use crate::fx::Chain;
use crate::instrument::Instrument;
use crate::params::gain_curve;
use crate::tfm::{Tfm, MAX_INSTRUMENTS};
use crate::voice::BLOCK;
use synth_core::mix::LoadError;
use synth_core::pool::Pool;

const RATE: f32 = 44100.0;

#[derive(Clone, Copy, Debug, Default)]
struct Cursor {
    next: usize,
    row: u32,
}

pub struct TfmPlayer<'a> {
    song: Tfm<'a>,
    ins: &'a mut [Instrument<'a>],
    cursor: [Cursor; MAX_INSTRUMENTS],
    muted: [bool; MAX_INSTRUMENTS],
    secs_per_row: f32,
    offset: f32,
    time: f32,
    playing: bool,
    /// Player volume (reference default 0.1).
    pub volume: f32,
    tmp: [[f32; BLOCK]; 2],
}

impl<'a> TfmPlayer<'a> {
    /// Pool bytes for `song` (instruments + the effect slots their patches use).
    pub fn required_pool(song: &Tfm) -> usize {
        Pool::need::<Instrument>(song.instruments())
            + (0..song.instruments()).map(|i| Instrument::need(Chain::need(&song.patch(i)))).sum::<usize>()
    }

    /// Builds the instruments; `offset` = seconds added to every event time (the reference's load delay).
    pub fn new(song: Tfm<'a>, pool: &mut Pool<'a>, offset: f32) -> Result<Self, LoadError> {
        let n = song.instruments();
        let slots = pool.uninit::<Instrument<'a>>(n)?;
        for (i, s) in slots.iter_mut().enumerate() {
            let p = song.patch(i);
            let mut ins = Instrument::new(pool, Chain::need(&p))?;
            ins.set_patch(&p).map_err(|_| LoadError::BadChunk)?;
            s.write(ins);
        }
        // SAFETY: all n slots were written above (an error returns before the cast).
        let ins = unsafe { &mut *(slots as *mut [core::mem::MaybeUninit<Instrument<'a>>] as *mut [Instrument<'a>]) };
        let rows_per_sec = (song.tempo as u32 * 4) as f32 / 60.0;
        Ok(TfmPlayer {
            song,
            ins,
            cursor: [Cursor::default(); MAX_INSTRUMENTS],
            muted: [false; MAX_INSTRUMENTS],
            secs_per_row: 1.0 / rows_per_sec,
            offset,
            time: 0.0,
            playing: false,
            volume: 0.1,
            tmp: [[0.0; BLOCK]; 2],
        })
    }

    fn event_time(&self, row: u32) -> f32 {
        row as f32 * self.secs_per_row + self.offset
    }

    /// Song length: the latest last-event time over instruments that have events (0 if none). Empty instruments
    /// are skipped (the reference reads before their array there).
    pub fn length(&self) -> f32 {
        (0..self.song.instruments())
            .filter_map(|i| self.song.events(i).last().map(|e| self.event_time(e.row)))
            .fold(0.0, f32::max)
    }

    /// Blocks for a full prerender: `trunc(sr·T/512) + 1`.
    pub fn blocks(&self) -> usize {
        (RATE * self.length() / BLOCK as f32) as usize + 1
    }

    pub fn instruments(&mut self) -> &mut [Instrument<'a>] {
        self.ins
    }

    pub fn mute(&mut self, i: usize, muted: bool) {
        if i < MAX_INSTRUMENTS {
            self.muted[i] = muted;
        }
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    pub fn start(&mut self, t: f32) {
        self.place(t);
        self.playing = true;
    }

    /// Releases all held voices and continues from `t` (events before `t` never fire).
    pub fn seek(&mut self, t: f32) {
        self.ins.iter_mut().for_each(Instrument::all_notes_off);
        self.place(t);
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.ins.iter_mut().for_each(Instrument::all_notes_off);
    }

    fn place(&mut self, t: f32) {
        self.time = t;
        for i in 0..self.song.instruments() {
            let mut c = Cursor::default();
            while c.next < self.song.event_count(i) {
                let e = self.song.event(i, c.next, c.row);
                if self.event_time(e.row) >= t {
                    break;
                }
                c = Cursor { next: c.next + 1, row: e.row };
            }
            self.cursor[i] = c;
        }
    }

    /// Fires this block's events and renders every instrument; `each(i, block)` receives each instrument's output.
    /// Returns false (nothing rendered) when stopped.
    fn block(&mut self, mut each: impl FnMut(&[[f32; BLOCK]; 2])) -> bool {
        if !self.playing {
            return false;
        }
        let next = self.time + BLOCK as f32 / RATE;
        for i in 0..self.song.instruments() {
            let mut c = self.cursor[i];
            while c.next < self.song.event_count(i) {
                let e = self.song.event(i, c.next, c.row);
                let t = self.event_time(e.row);
                if t >= next {
                    break;
                }
                c = Cursor { next: c.next + 1, row: e.row };
                if t < self.time || self.muted[i] || e.note == 0 {
                    continue;
                }
                if e.velocity == 0 {
                    self.ins[i].note_off(e.note);
                } else {
                    self.ins[i].note_on(e.note, e.velocity);
                }
            }
            self.cursor[i] = c;
        }
        for ins in self.ins.iter_mut() {
            ins.render(&mut self.tmp);
            each(&self.tmp);
        }
        self.time = next;
        true
    }

    /// One block as interleaved i16 with the reference mix. Silence (and no time advance) when stopped.
    pub fn render_i16(&mut self, out: &mut [i16; 2 * BLOCK]) {
        let mut bus = [[0.0f32; BLOCK]; 2];
        let g = gain_curve(1.0); // the reference mixes each instrument at volume 1.0 → ×11
        let rendered = self.block(|t| {
            for c in 0..2 {
                for (b, v) in bus[c].iter_mut().zip(t[c].iter()) {
                    *b = *v * g + *b;
                }
            }
        });
        let gain = 1000.0 * 2.0 * self.volume;
        for i in 0..BLOCK {
            for c in 0..2 {
                let v = if rendered { (bus[c][i] * gain).clamp(-32768.0, 32767.0) } else { 0.0 };
                out[2 * i + c] = v as i32 as i16;
            }
        }
    }

    /// One block as interleaved f32: the plain sum of the instrument outputs (no ×11, no volume).
    pub fn render_f32(&mut self, out: &mut [f32; 2 * BLOCK]) {
        out.fill(0.0);
        self.block(|t| {
            for i in 0..BLOCK {
                out[2 * i] += t[0][i];
                out[2 * i + 1] += t[1][i];
            }
        });
    }
}
