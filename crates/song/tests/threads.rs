//! Multi-threaded rendering of the committed songs (change add-synth-threads): every `project/music` song takes the
//! parallel path and renders bit-identically to the single-thread player with any worker count.
#![cfg(feature = "threads")]

use synth_core::mix::{Env, Player, Song, DEFAULT_SUB};
use synth_core::pool::Pool;
use unisynth::rng::Xorshift;
use unisynth::UDevice;

/// Seconds rendered per song (debug builds are slow; the release CLI check covers whole songs).
const SECONDS: usize = 2;

fn blobs() -> Vec<(String, Vec<u8>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../project/music");
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".song.json"))
        .map(|p| {
            let s = song::load(&std::fs::read_to_string(&p).unwrap()).unwrap();
            (p.file_name().unwrap().to_string_lossy().into_owned(), song::export(&s).unwrap())
        })
        .collect();
    v.sort();
    v
}

fn render(blob: &[u8], frames: usize, threads: usize, seek: Option<u64>) -> (bool, Vec<f32>) {
    let s = Song::parse(blob).unwrap();
    let env = Env::for_song(&s, None);
    let mut mem = vec![0u8; Player::<UDevice, Xorshift>::required_pool(&s, &env, DEFAULT_SUB).unwrap()];
    let mut pool = Pool::new(&mut mem);
    let mut p = Player::<UDevice, Xorshift>::new(s, &env, &mut pool, Xorshift::new(song::render::SEED), DEFAULT_SUB).unwrap();
    let mut out = vec![0.0f32; 2 * frames];
    if let Some(f) = seek {
        p.seek(f);
    }
    if threads == 0 {
        p.render_f32(&mut out);
    } else {
        song::workers::with_workers(threads, |w| {
            for c in out.chunks_mut(2 * 777) {
                p.render_f32_par(c, w);
            }
        });
    }
    (p.parallel(), out)
}

#[test]
fn committed_songs_render_in_parallel_bit_identically() {
    for (name, b) in blobs() {
        let rate = Song::parse(&b).unwrap().rate as usize;
        let (_, want) = render(&b, SECONDS * rate, 0, None);
        for w in [2, 5] {
            let (parallel, got) = render(&b, SECONDS * rate, w, None);
            assert!(parallel, "{name} takes the parallel path");
            assert!(got == want, "{name}: {w} workers differ from the single-thread render");
        }
        // after a seek into the song
        let at = (rate * 3) as u64;
        let (_, want) = render(&b, rate / 2, 0, Some(at));
        let (_, got) = render(&b, rate / 2, 3, Some(at));
        assert!(got == want, "{name}: seek differs");
    }
}

#[test]
fn render_threads_equals_render() {
    let (_, b) = blobs().into_iter().find(|(n, _)| n == "demo.song.json").unwrap();
    let want = song::render::render(&b, None).unwrap();
    for n in [1, 2, 8] {
        assert!(song::render::render_threads(&b, None, n).unwrap() == want, "{n} threads");
    }
}
