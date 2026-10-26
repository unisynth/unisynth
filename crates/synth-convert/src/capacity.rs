//! Voice capacity of a converted instrument (design D10): WaveSabre has 256 voices that never steal, the unified
//! instrument has 1..=64 and steals the oldest. The capacity is the most voices that sound at once in the song: the
//! overlapping notes (each from its note-on to its note-off plus the amp release) times the unison count, capped at 64.

use crate::model::Ev;

pub const MAX: u32 = 64;

/// How a legacy voice ends.
pub enum Life {
    /// At the note-off plus `tail` frames of release.
    Note { tail: i64 },
    /// After a fixed number of frames whatever the note-off (Thunder).
    Fixed(i64),
}

/// The most voices sounding at once for one device fed by `lanes` (one per place the device is listed in a track:
/// devices are song-wide, so notes of every lane add up). Returns `(voices, uncapped)`; `voices` is within `1..=MAX`.
pub fn analyse(lanes: &[&[Ev]], life: &Life, unison: u32, legato: bool) -> (u32, u32) {
    let unison = unison.max(1);
    if legato {
        return (unison.min(MAX), unison);
    }
    let mut spans: Vec<(i64, i64)> = vec![];
    for lane in lanes {
        spans_of(lane, life, &mut spans);
    }
    let mut edges: Vec<(i64, i32)> = spans.iter().flat_map(|&(s, e)| [(s, 1), (e, -1)]).collect();
    // at one instant the ends come first: a note that ends where another begins does not overlap it
    edges.sort_by_key(|&(t, d)| (t, d));
    let (mut now, mut most) = (0i32, 0i32);
    for (_, d) in edges {
        now += d;
        most = most.max(now);
    }
    let uncapped = (most.max(1) as u32).saturating_mul(unison);
    (uncapped.min(MAX), uncapped)
}

/// The `(start, end)` frames of every voice a lane starts.
fn spans_of(lane: &[Ev], life: &Life, spans: &mut Vec<(i64, i64)>) {
    let mut t = 0i64;
    let mut active: Vec<(u8, i64)> = vec![];
    for &(delta, note, vel) in lane {
        t += delta as i64;
        match vel {
            Some(_) => active.push((note & 0x7f, t)),
            None => {
                let note = note & 0x7f;
                active.retain(|&(n, s)| {
                    if n != note {
                        return true;
                    }
                    if let Life::Note { tail } = life {
                        spans.push((s, t.saturating_add(*tail)));
                    }
                    false
                });
            }
        }
        if let Life::Fixed(len) = life {
            if vel.is_some() {
                spans.push((t, t.saturating_add(*len)));
            }
        }
    }
    // notes never released sound to the end
    for (_, s) in active {
        if matches!(life, Life::Note { .. }) {
            spans.push((s, i64::MAX));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(d: i32, n: u8) -> Ev {
        (d, n, Some(100))
    }
    fn off(d: i32, n: u8) -> Ev {
        (d, n, None)
    }

    #[test]
    fn overlapping_notes_and_their_release_tails_count() {
        let life = Life::Note { tail: 1000 };
        // a chord of 3, then a note starting inside the first chord's tail
        let lane = [on(0, 60), on(0, 64), on(0, 67), off(5000, 60), off(0, 64), off(0, 67), on(500, 72), off(100, 72)];
        assert_eq!(analyse(&[&lane], &life, 1, false), (4, 4));
        // …but not when it starts after every tail
        let lane = [on(0, 60), on(0, 64), on(0, 67), off(5000, 60), off(0, 64), off(0, 67), on(1500, 72), off(100, 72)];
        assert_eq!(analyse(&[&lane], &life, 1, false), (3, 3));
    }

    #[test]
    fn unison_multiplies_and_the_cap_is_reported() {
        let life = Life::Note { tail: 0 };
        let lane: Vec<Ev> = (0..10).map(|i| on(0, 40 + i)).collect();
        assert_eq!(analyse(&[&lane], &life, 7, false), (64, 70));
        assert_eq!(analyse(&[&lane], &life, 3, false), (30, 30));
    }

    #[test]
    fn legato_is_one_voice_per_unison_and_an_empty_lane_one_voice() {
        assert_eq!(analyse(&[&[on(0, 60), on(10, 62)]], &Life::Note { tail: 0 }, 4, true), (4, 4));
        assert_eq!(analyse(&[&[]], &Life::Note { tail: 0 }, 1, false), (1, 1));
    }

    #[test]
    fn a_note_that_ends_where_another_starts_does_not_overlap_it() {
        let lane = [on(0, 60), off(100, 60), on(0, 60), off(100, 60)];
        assert_eq!(analyse(&[&lane], &Life::Note { tail: 0 }, 1, false).0, 1);
    }

    #[test]
    fn fixed_length_voices_ignore_the_note_off() {
        let lane = [on(0, 60), off(10, 60), on(100, 60), on(100, 62)];
        assert_eq!(analyse(&[&lane], &Life::Fixed(150), 1, false).0, 2);
        assert_eq!(analyse(&[&lane], &Life::Fixed(50), 1, false).0, 1);
    }

    #[test]
    fn lanes_of_a_shared_device_add_up() {
        let a = [on(0, 60), on(0, 64), off(1000, 60), off(0, 64)];
        let b = [on(500, 67), on(0, 71), off(1000, 67), off(0, 71)];
        let life = Life::Note { tail: 0 };
        assert_eq!(analyse(&[&a, &b], &life, 1, false).0, 4);
        assert_eq!(analyse(&[&a, &a], &life, 1, false).0, 4, "a device listed twice hears every note twice");
        assert_eq!(analyse(&[&a], &life, 1, false).0, 2);
    }

    #[test]
    fn unreleased_notes_sound_to_the_end() {
        let lane = [on(0, 60), on(1000, 62), on(1000, 64)];
        assert_eq!(analyse(&[&lane], &Life::Note { tail: 10 }, 1, false).0, 3);
    }
}
