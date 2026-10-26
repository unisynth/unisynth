//! Pattern detection for import (change add-import-patterns, design P1–P4). Turns each one-clip lane (placed once at
//! tick 0) into patterns on the 2-, 4- or 8-bar grid with the least note data: identical segments share one clip,
//! back-to-back repeats become one looping placement, a segment sharing at least half its notes with an earlier
//! pattern is named as its variant (`A.1`). Notes never move in absolute time and no segment is cut through a note,
//! so the export is unchanged.

use crate::model::*;

/// Grid sizes tried, in bars; on equal cost the longer grid wins.
const GRIDS: [i64; 3] = [8, 4, 2];
/// Cost of one placement, in notes.
const PLACEMENT_COST: usize = 8;
/// Songs longer than this many 2-bar windows (hostile lengths) keep one clip per lane.
const MAX_WINDOWS: i64 = 1 << 16;

struct Plan {
    /// `(name, length, notes relative to the clip start)`.
    clips: Vec<(String, i64, Vec<Note>)>,
    /// `(clip index, start, length, looping)`.
    placed: Vec<(usize, i64, i64, bool)>,
    cost: usize,
}

/// `A`…`Z`, `AA`, `AB`, …
pub fn letters(mut i: usize) -> String {
    let mut s = vec![];
    loop {
        s.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap()
}

/// Share of notes of the larger list that the other list has too (same pitch, start within `tol` ticks).
fn similarity(a: &[Note], b: &[Note], tol: i64) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut used = vec![false; b.len()];
    let mut m = 0;
    for x in a {
        if let Some(j) = (0..b.len()).find(|&j| !used[j] && b[j].pitch == x.pitch && (b[j].start - x.start).abs() <= tol) {
            used[j] = true;
            m += 1;
        }
    }
    m as f64 / a.len().max(b.len()) as f64
}

fn plan(notes: &[Note], song_len: i64, bar: i64, bars: i64) -> Plan {
    let w = bar * bars;
    let windows = ((song_len + w - 1) / w).max(1) as usize;
    // grid lines a note crosses (strictly inside the note) are not cut points
    let mut crossed = vec![false; windows + 1];
    for n in notes {
        let (a, b) = (n.start, n.start.saturating_add(n.len));
        let first = a / w + 1;
        let mut k = first;
        while k * w < b && (k as usize) < windows {
            crossed[k as usize] = true;
            k += 1;
        }
    }
    let mut segs = vec![];
    let mut a = 0usize;
    for k in 1..=windows {
        if k == windows || !crossed[k] {
            segs.push((a as i64 * w, k as i64 * w));
            a = k;
        }
    }
    let tol = bar / 64;
    let mut p = Plan { clips: vec![], placed: vec![], cost: 0 };
    // base pattern index → variants so far; base patterns in order
    let mut bases: Vec<(usize, usize)> = vec![];
    let mut i = 0;
    for (a, b) in segs {
        let mut seg = vec![];
        while i < notes.len() && notes[i].start < b {
            let n = notes[i];
            seg.push(Note { start: n.start - a, ..n });
            i += 1;
        }
        if seg.is_empty() {
            continue;
        }
        seg.sort_by_key(|n| (n.start, n.pitch, n.len, n.vel));
        let len = b - a;
        let ci = match p.clips.iter().position(|c| c.1 == len && c.2 == seg) {
            Some(ci) => ci,
            None => {
                let best = bases
                    .iter()
                    .enumerate()
                    .filter(|&(_, &(ci, _))| p.clips[ci].1 == len)
                    .map(|(bi, &(ci, _))| (bi, similarity(&seg, &p.clips[ci].2, tol)))
                    .filter(|&(_, s)| s >= 0.5)
                    .max_by(|x, y| x.1.total_cmp(&y.1).then(y.0.cmp(&x.0)));
                let name = match best {
                    Some((bi, _)) => {
                        bases[bi].1 += 1;
                        format!("{}.{}", p.clips[bases[bi].0].0, bases[bi].1)
                    }
                    None => {
                        bases.push((p.clips.len(), 0));
                        letters(bases.len() - 1)
                    }
                };
                p.cost += seg.len();
                p.clips.push((name, len, seg));
                p.clips.len() - 1
            }
        };
        let plen = b.min(song_len) - a;
        match p.placed.last_mut() {
            // back-to-back repeat of a clip whose placement so far is whole repeats: loop it
            Some(last) if last.0 == ci && last.1 + last.2 == a && last.2 % len == 0 => {
                last.2 += plen;
                last.3 = true;
            }
            _ => p.placed.push((ci, a, plen, false)),
        }
    }
    p.cost += PLACEMENT_COST * p.placed.len();
    p
}

/// Splits every clip of a one-clip import (placed once at tick 0 on each track that plays it) into patterns. Clip ids
/// become `<first track id>/<name>`; every track that played the lane gets the same placements.
pub fn split(s: &mut Song) {
    let bar = (s.ticks_per_beat as i64 * 4 * s.signature[0] as i64 / s.signature[1].max(1) as i64).max(1);
    let song_len = s.length.max(1);
    if song_len / (2 * bar) > MAX_WINDOWS {
        return;
    }
    let old = std::mem::take(&mut s.clips);
    for c in old {
        let mut notes = c.notes.clone();
        notes.sort_by_key(|n| (n.start, n.pitch));
        let owner = s.tracks.iter().chain([&s.master]).find(|t| t.placements.iter().any(|p| p.clip == c.id)).map(|t| t.id.clone());
        let Some(owner) = owner.filter(|_| !notes.is_empty() && c.wheel.is_empty() && c.bend.is_empty()) else {
            s.clips.push(c);
            continue;
        };
        let best = GRIDS.iter().map(|&g| plan(&notes, song_len, bar, g)).min_by_key(|p| p.cost).unwrap();
        let ids: Vec<String> = best.clips.iter().map(|(n, _, _)| format!("{owner}/{n}")).collect();
        let placements: Vec<Placement> =
            best.placed.iter().map(|&(ci, start, len, looping)| Placement { clip: ids[ci].clone(), start, length: Some(len), looping }).collect();
        for (id, (_, length, notes)) in ids.iter().zip(best.clips) {
            s.clips.push(Clip { id: id.clone(), length, notes, wheel: vec![], bend: vec![] });
        }
        for t in s.tracks.iter_mut().chain([&mut s.master]) {
            if t.placements.iter().any(|p| p.clip == c.id) {
                t.placements = placements.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_count_like_spreadsheet_columns() {
        assert_eq!([0, 1, 25, 26, 27, 51, 52, 701, 702].map(letters), ["A", "B", "Z", "AA", "AB", "AZ", "BA", "ZZ", "AAA"]);
    }
}
