//! WaveSabre devices draw from the shared song random source while rendering (upstream's global `rand()`), so with
//! `synth-core/threads` their songs stay on the single-thread path and the parallel entry renders the same output
//! (change add-synth-threads).

#[allow(dead_code)]
mod songs;

use synth_core::mix::{Env, Kinds, Par, Player, Song, DEFAULT_SUB};
use synth_core::pool::Pool;
use wavesabre::helpers::WsRng;
use wavesabre::WsDevice;

struct Threads(usize);
impl Par for Threads {
    fn run(&self, jobs: usize, job: &(dyn Fn(usize) + Sync)) {
        std::thread::scope(|s| {
            for k in 0..self.0 {
                let w = self.0;
                s.spawn(move || (0..jobs).filter(|i| i % w == k).for_each(job));
            }
        });
    }
}

fn render(blob: &[u8], par: bool) -> (bool, Vec<f32>) {
    let s = Song::parse(blob).unwrap();
    let env = Env::for_song(&s, None);
    let mut mem = vec![0u8; Player::<WsDevice, WsRng>::required_pool(&s, &env, DEFAULT_SUB).unwrap()];
    let mut pool = Pool::new(&mut mem);
    let mut p = Player::<WsDevice, WsRng>::new(s, &env, &mut pool, WsRng::new(), DEFAULT_SUB).unwrap();
    let mut out = vec![0.0f32; 2 * 44100];
    if par { p.render_f32_par(&mut out, &Threads(3)) } else { p.render_f32(&mut out) }
    (p.parallel(), out)
}

#[test]
fn wavesabre_songs_fall_back_to_the_single_thread_path() {
    assert!(<WsDevice as Kinds>::RENDER_RNG);
    for (name, b) in songs::all() {
        let (parallel, got) = render(&b, true);
        assert!(!parallel, "{name}");
        assert!(got == render(&b, false).1, "{name}");
    }
}
