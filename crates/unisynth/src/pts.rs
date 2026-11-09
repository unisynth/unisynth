//! The point oscillator's cycle (change add-tf3-import, feature `us-osc-points`; behaviour of Tunefish 3's oscillator,
//! `research/tunefish3-functional-spec.md` §3.1): a curve through `(0, 0)`, 1–4 control points and `(1, 0)`, each
//! segment held (step), linear or on one cubic spline through all points, sampled into a 257-sample cycle with its
//! mean removed. Played by up to 11 unison layers whose sum is driven and hard-clipped. One [`Pts`] lives in the pool
//! per voice and point slot.

/// Samples per cycle (plus one guard sample: the index rounds, so a phase of exactly 1 reads it).
pub const LEN: usize = 256;
/// Unison layers at most.
pub const MAX_LAYERS: usize = 11;
/// Control points at most.
pub const MAX_POINTS: usize = 4;
/// Segment modes.
pub const STEP: u8 = 0;
pub const LINEAR: u8 = 1;
pub const SPLINE: u8 = 2;

/// The cycle's inputs after the mod matrix: point count, each point's offset (a fraction of the span left after the
/// previous point) and value (0..1), and the mode of the segment ending at each point and of the last segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub count: usize,
    pub offset: [f32; MAX_POINTS],
    pub value: [f32; MAX_POINTS],
    /// `modes[k]` (k < count) ends at point k + 1; `modes[MAX_POINTS]` is the last segment's.
    pub modes: [u8; MAX_POINTS + 1],
}

/// Everything a voice keeps for one point slot (a PAD slot keeps its phases and decimation here too).
pub struct Pts {
    wave: [f32; LEN + 1],
    built: Option<Shape>,
    /// Left and right phase of each layer, in turns.
    pub phase: [[f32; 2]; MAX_LAYERS],
    /// The decimation stage (spec §3.3).
    pub decim: Decim,
}

impl Pts {
    pub const fn new() -> Self {
        Pts { wave: [0.0; LEN + 1], built: None, phase: [[0.0; 2]; MAX_LAYERS], decim: Decim::new() }
    }

    /// Rebuilds the cycle when `s` differs from the last build. Returns whether it rebuilt.
    pub fn update(&mut self, s: &Shape) -> bool {
        if self.built.as_ref() == Some(s) {
            return false;
        }
        self.wave = cycle(s);
        self.built = Some(*s);
        true
    }

    pub fn wave(&self) -> &[f32; LEN + 1] {
        &self.wave
    }
}

impl Default for Pts {
    fn default() -> Self {
        Self::new()
    }
}

/// The curve's points: `(0, 0)`, the control points, `(1, 0)`, then the phantom `(0, 0)` that the original's spline
/// solve runs to (one segment past the end; spec §3.1.1). Returns the points and how many are real (`count + 2`).
fn points(s: &Shape) -> ([(f32, f32); MAX_POINTS + 3], usize) {
    let n = s.count.max(1).min(MAX_POINTS);
    let mut p = [(0.0f32, 0.0f32); MAX_POINTS + 3];
    let mut last = 0.0f32;
    for k in 0..n {
        let x = last + (1.0 - last) * s.offset[k].max(0.01).min(0.99);
        last = x;
        p[k + 1] = (x, s.value[k] * 2.0 - 1.0);
    }
    p[n + 1] = (1.0, 0.0);
    (p, n + 2)
}

/// Cubic coefficients `(a, b, c)` per segment (value `((a·t + b)·t + c)·t + y` at `t = x − x_k`): the natural
/// spline's tridiagonal sweep over every real segment and the phantom one, widths and rises nudged by 1e-4.
fn spline(p: &[(f32, f32); MAX_POINTS + 3], m: usize) -> [(f32, f32, f32); MAX_POINTS + 3] {
    const N: usize = MAX_POINTS + 3;
    let (mut dx, mut dy) = ([0.0f32; N], [0.0f32; N]);
    for i in 0..m {
        dx[i] = p[i + 1].0 - p[i].0 + 1e-4;
        dy[i] = p[i + 1].1 - p[i].1 + 1e-4;
    }
    let (mut r, mut z, mut b) = ([0.0f32; N], [0.0f32; N], [0.0f32; N + 1]);
    for i in 1..m {
        r[i] = (dy[i] / dx[i] - dy[i - 1] / dx[i - 1]) * 3.0;
        z[i] = (dx[i - 1] + dx[i]) * 2.0;
    }
    for i in 2..m {
        r[i] -= r[i - 1] * dx[i - 1] / z[i - 1];
        z[i] -= dx[i - 1] * dx[i - 1] / z[i - 1];
    }
    for i in (1..m).rev() {
        b[i] = (r[i] - dx[i] * b[i + 1]) / z[i];
    }
    let mut out = [(0.0f32, 0.0f32, 0.0f32); N];
    for i in 0..m {
        let c = dy[i] / dx[i] - dx[i] * (b[i + 1] + 2.0 * b[i]) / 3.0;
        let a = (b[i + 1] - b[i]) / (3.0 * dx[i]);
        out[i] = (a, b[i], c);
    }
    out
}

/// The 257-sample cycle of `s`, its mean (the sum of all 257 over 256, as the original) removed.
pub fn cycle(s: &Shape) -> [f32; LEN + 1] {
    let (p, m) = points(s);
    let n = m - 2;
    let co = spline(&p, m);
    let mode = |i: usize| if i <= n { s.modes[i - 1] } else { s.modes[MAX_POINTS] };
    let mut w = [0.0f32; LEN + 1];
    let mut sum = 0.0f32;
    for (j, v) in w.iter_mut().enumerate() {
        let x = j as f32 / LEN as f32;
        // the segment ends at the first point at or after x
        let mut i = 1;
        while x > p[i].0 && i <= n {
            i += 1;
        }
        let k = i - 1;
        *v = match mode(i) {
            STEP => p[k].1,
            LINEAR => p[k].1 + (p[i].1 - p[k].1) * ((x - p[k].0) / (p[i].0 - p[k].0)),
            _ => {
                let t = x - p[k].0;
                let (a, b, c) = co[k];
                ((a * t + b) * t + c) * t + p[k].1
            }
        };
        sum += *v;
    }
    let mean = sum / LEN as f32;
    w.iter_mut().for_each(|v| *v -= mean);
    w
}

/// Most oversampling of the decimation stage.
pub const MAX_OVERSAMPLE: u32 = 8;

/// Tunefish 3's decimation stage (spec §3.3): the slot renders `factor` samples per output frame, each passes two
/// cascaded ladder low-passes at 12.83 kHz designed at `factor × rate` (no resonance), and the mean of the `factor`
/// filtered samples is the output. It runs at `factor` 1 too.
#[derive(Clone, Copy, Debug)]
pub struct Decim {
    pub factor: u32,
    /// Ladder coefficients `(k, p)` for the current factor and rate.
    k: f32,
    p: f32,
    rate: f32,
    /// Per filter and channel: previous input and stage outputs `[x, y1, y2, y3, y4]`.
    st: [[[f32; 5]; 2]; 2],
}

impl Decim {
    pub const fn new() -> Self {
        Decim { factor: 1, k: 0.0, p: 0.0, rate: 0.0, st: [[[0.0; 5]; 2]; 2] }
    }

    /// Oversampling for a note at `hz` (smallest `k` with `k·rate ≥ 64·hz`, at most 8) and the filters designed at it.
    pub fn set(&mut self, hz: f32, rate: f32) {
        let mut k = 1u32;
        while rate * (k as f32) < hz * 64.0 && k < MAX_OVERSAMPLE {
            k += 1;
        }
        if k == self.factor && rate == self.rate && self.p != 0.0 {
            return;
        }
        // the ladder law at cutoff parameter 0.8: 0.8² · 20 000 + 30 Hz
        let f = 2.0 * (0.8f32 * 0.8 * 20_000.0 + 30.0) / (rate * k as f32);
        self.k = 3.6 * f - 1.6 * f * f - 1.0;
        self.p = (self.k + 1.0) * 0.5;
        (self.factor, self.rate) = (k, rate);
    }

    /// Clears the filters (note-on).
    pub fn clear(&mut self) {
        self.st = [[[0.0; 5]; 2]; 2];
    }

    #[inline]
    fn ladder(st: &mut [f32; 5], x: f32, k: f32, p: f32) -> f32 {
        let y1 = x * p + st[0] * p - k * st[1];
        let y2 = y1 * p + st[1] * p - k * st[2];
        let y3 = y2 * p + st[2] * p - k * st[3];
        let y4 = y3 * p + st[3] * p - k * st[4];
        *st = [x, y1, y2, y3, y4];
        y4 - y4 * y4 * y4 / 6.0
    }

    /// One oversampled sample through both filters.
    #[inline]
    pub fn filter(&mut self, l: f32, r: f32) -> (f32, f32) {
        let (k, p) = (self.k, self.p);
        let [a, b] = &mut self.st;
        let l = Self::ladder(&mut b[0], Self::ladder(&mut a[0], l, k, p), k, p);
        let r = Self::ladder(&mut b[1], Self::ladder(&mut a[1], r, k, p), k, p);
        (l, r)
    }
}

impl Default for Decim {
    fn default() -> Self {
        Self::new()
    }
}

/// One stereo output frame of a point slot through the decimation stage, the slot `gain` applied before it.
#[inline]
pub fn play(pts: &mut Pts, inc: f32, det: f32, spread: f32, drive: f32, layers: usize, gain: f32) -> (f32, f32) {
    let k = pts.decim.factor.max(1);
    let kf = k as f32;
    let (inc, det, spread) = (inc / kf, det / kf, spread / kf);
    let (mut l, mut r) = (0.0f32, 0.0f32);
    for _ in 0..k {
        let (a, b) = cycle_sample(pts, inc, det, spread, drive, layers);
        let (a, b) = pts.decim.filter(a * gain, b * gain);
        l += a;
        r += b;
    }
    (l / kf, r / kf)
}

/// One sample of the cycle at the oversampled rate: every layer reads the cycle without interpolation (the index
/// rounds half to even), the layers are summed, driven and hard-clipped. Layer `k` plays `|inc ± det ± k·spread|`
/// (left +, right −).
#[inline]
fn cycle_sample(pts: &mut Pts, inc: f32, det: f32, spread: f32, drive: f32, layers: usize) -> (f32, f32) {
    let (mut l, mut r) = (0.0f32, 0.0f32);
    let (f1, f2) = (inc + det, inc - det);
    for (k, ph) in pts.phase.iter_mut().take(layers).enumerate() {
        l += pts.wave[index(ph[0])];
        r += pts.wave[index(ph[1])];
        ph[0] += libm::fabsf(f1 + k as f32 * spread);
        ph[0] -= if ph[0] > 1.0 { ph[0] as u32 as f32 } else { 0.0 };
        ph[1] += libm::fabsf(f2 - k as f32 * spread);
        ph[1] -= if ph[1] > 1.0 { ph[1] as u32 as f32 } else { 0.0 };
    }
    ((l * drive).max(-1.0).min(1.0), (r * drive).max(-1.0).min(1.0))
}

/// Table index of a phase in `0..=1`: `round(phase · 256)`, half to even (the original's float → int).
#[inline]
fn index(phase: f32) -> usize {
    let x = phase * LEN as f32;
    let f = x as u32; // floor of a non-negative value
    let d = x - f as f32;
    let i = if d > 0.5 || (d == 0.5 && f % 2 == 1) { f + 1 } else { f };
    (i as usize).min(LEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(count: usize, pts: &[(f32, f32)], modes: [u8; 5]) -> Shape {
        let mut s = Shape { count, offset: [0.5; 4], value: [0.5; 4], modes };
        for (k, &(o, v)) in pts.iter().enumerate() {
            s.offset[k] = o;
            s.value[k] = v;
        }
        s
    }

    #[test]
    fn a_step_cycle_holds_the_left_point() {
        // one point at x 0.5, value 1: 0 up to 0.5, then 1; both segments step
        let w = cycle(&shape(1, &[(0.5, 1.0)], [STEP, STEP, STEP, STEP, STEP]));
        // samples 0..=128 (129) sit on the first segment (0), 129..=256 (128) on the second (1): mean 128/256
        let mean = 0.5;
        assert!((w[0] + mean).abs() < 1e-6 && (w[128] + mean).abs() < 1e-6, "{} {}", w[0], w[128]);
        assert!((w[129] - (1.0 - mean)).abs() < 1e-6 && (w[256] - (1.0 - mean)).abs() < 1e-6);
    }

    #[test]
    fn a_linear_cycle_is_a_triangle() {
        let w = cycle(&shape(1, &[(0.5, 1.0)], [LINEAR, LINEAR, LINEAR, LINEAR, LINEAR]));
        let mean: f32 = (0..=256).map(|j| 1.0 - (j as f32 / 128.0 - 1.0).abs()).sum::<f32>() / 256.0;
        for (j, v) in w.iter().enumerate() {
            let want = 1.0 - (j as f32 / 128.0 - 1.0).abs() - mean;
            assert!((v - want).abs() < 1e-5, "{j}: {v} {want}");
        }
    }

    #[test]
    fn offsets_are_fractions_of_the_span_left() {
        let (p, m) = points(&shape(3, &[(0.5, 1.0), (0.5, 0.0), (0.0, 0.5)], [LINEAR; 5]));
        assert_eq!(m, 5);
        assert_eq!([p[1].0, p[2].0, p[3].0, p[4].0], [0.5, 0.75, 0.7525, 1.0]); // the last offset clamps to 0.01
        assert_eq!([p[1].1, p[2].1, p[3].1], [1.0, -1.0, 0.0]);
        assert_eq!(p[5], (0.0, 0.0), "the phantom point");
    }

    #[test]
    fn a_spline_passes_through_its_points_and_is_smooth() {
        let s = shape(2, &[(0.25, 1.0), (0.5, 0.0)], [SPLINE; 5]);
        let (p, m) = points(&s);
        let co = spline(&p, m);
        for k in 0..m - 1 {
            // segment k ends at point k + 1 (within the 1e-4 nudge)
            let t = p[k + 1].0 - p[k].0;
            let (a, b, c) = co[k];
            let end = ((a * t + b) * t + c) * t + p[k].1;
            assert!((end - p[k + 1].1).abs() < 2e-3, "segment {k}: {end} vs {}", p[k + 1].1);
        }
        let w = cycle(&s);
        let jump = w.windows(2).map(|x| (x[1] - x[0]).abs()).fold(0.0f32, f32::max);
        assert!(jump < 0.1, "{jump}");
    }

    #[test]
    fn the_cycle_is_rebuilt_only_when_its_inputs_change() {
        let mut p = Pts::new();
        let s = shape(2, &[(0.3, 0.9), (0.5, 0.1)], [SPLINE, LINEAR, STEP, STEP, LINEAR]);
        assert!(p.update(&s));
        assert!(!p.update(&s));
        assert!(p.update(&Shape { value: [0.9, 0.2, 0.5, 0.5], ..s }));
        assert!(p.update(&Shape { modes: [SPLINE, SPLINE, STEP, STEP, LINEAR], ..s }));
    }

    #[test]
    fn layers_are_summed_then_driven_and_clipped() {
        let mut p = Pts::new();
        p.wave = [0.4; LEN + 1];
        let (l, r) = cycle_sample(&mut p, 0.01, 0.0, 0.0, 1.0, 2);
        assert!((l - 0.8).abs() < 1e-6 && (r - 0.8).abs() < 1e-6);
        let (l, _) = cycle_sample(&mut p, 0.01, 0.0, 0.0, 2.0, 2);
        assert_eq!(l, 1.0, "the sum is clipped, not each layer");
    }

    /// Energy of `x` between `lo` and `hi` Hz (a plain DFT over the band's bins).
    fn band(x: &[f32], lo: f64, hi: f64) -> f64 {
        let n = x.len();
        let (a, b) = ((lo * n as f64 / 44100.0) as usize, ((hi * n as f64 / 44100.0) as usize).min(n / 2));
        (a..b)
            .map(|k| {
                let (mut re, mut im) = (0.0f64, 0.0f64);
                for (i, v) in x.iter().enumerate() {
                    let ph = 2.0 * core::f64::consts::PI * (k * i) as f64 / n as f64;
                    re += *v as f64 * ph.cos();
                    im -= *v as f64 * ph.sin();
                }
                re * re + im * im
            })
            .sum()
    }

    /// `n` frames of a cycle at `hz`, through the stage (`staged`) or read once per frame without it.
    fn render(s: &Shape, hz: f32, staged: bool, n: usize) -> std::vec::Vec<f32> {
        let mut p = Pts::new();
        p.update(s);
        p.decim.set(hz, 44100.0);
        let inc = hz / 44100.0;
        (0..n).map(|_| if staged { play(&mut p, inc, 0.0, 0.0, 1.0, 1, 1.0).0 } else { cycle_sample(&mut p, inc, 0.0, 0.0, 1.0, 1).0 }).collect()
    }

    #[test]
    fn a_high_note_loses_its_top_octaves_in_the_stage() {
        // a sawtooth-like cycle (one point at the far right, linear) at note 100 (2637 Hz, oversampled ×4)
        let saw = shape(1, &[(0.99, 1.0)], [LINEAR; 5]);
        let hz = 440.0 * libm::exp2f((100.0 - 69.0) / 12.0);
        let (with, without) = (render(&saw, hz, true, 2048), render(&saw, hz, false, 2048));
        let (a, b) = (band(&with[1024..], 10_000.0, 22_050.0), band(&without[1024..], 10_000.0, 22_050.0));
        assert!(a < b * 0.5, "{a} vs {b}");
    }

    #[test]
    fn a_low_note_is_filtered_too() {
        // a square at note 45 (110 Hz, no oversampling): the 16 kHz octave band still drops
        let square = shape(1, &[(0.5, 1.0)], [STEP; 5]);
        let (with, without) = (render(&square, 110.0, true, 2048), render(&square, 110.0, false, 2048));
        let (a, b) = (band(&with[1024..], 11_314.0, 22_050.0), band(&without[1024..], 11_314.0, 22_050.0));
        assert!(a < b * 0.5, "{a} vs {b}");
    }

    /// Cost of the stage (run with `--ignored --nocapture`): frames per second of one 11-layer slot at ×1 and ×8.
    #[test]
    #[ignore]
    fn speed_of_the_stage() {
        let s = shape(2, &[(0.3, 0.9), (0.5, 0.1)], [SPLINE; 5]);
        for hz in [220.0f32, 6000.0] {
            let mut p = Pts::new();
            p.update(&s);
            p.decim.set(hz, 44100.0);
            let n = 441_000;
            let t = std::time::Instant::now();
            let mut acc = 0.0f32;
            for _ in 0..n {
                acc += play(&mut p, hz / 44100.0, 1e-4, 1e-5, 2.0, 11, 0.5).0;
            }
            let rt = n as f64 / 44100.0 / t.elapsed().as_secs_f64();
            std::eprintln!("{hz} Hz (×{}), 11 layers: {rt:.0}× real time per voice ({acc})", p.decim.factor);
        }
    }

    #[test]
    fn the_oversampling_follows_the_note_frequency() {
        let mut d = Decim::new();
        for (hz, k) in [(440.0, 1), (689.0, 1), (690.0, 2), (2000.0, 3), (4823.0, 7), (4824.0, 8), (12_000.0, 8)] {
            d.set(hz, 44100.0);
            assert_eq!(d.factor, k, "{hz} Hz");
        }
    }

    #[test]
    fn the_decimation_filter_passes_dc_and_cuts_near_nyquist() {
        let mut d = Decim::new();
        d.set(100.0, 44100.0);
        let mut y = 0.0;
        for _ in 0..4000 {
            y = d.filter(0.25, 0.25).0;
        }
        assert!((y - 0.25 * (1.0 - 0.25 * 0.25 / 6.0 * 0.25 / 0.25)).abs() < 0.01, "unity at DC (with the soft clip): {y}");
        d.clear();
        let mut e = 0.0f32;
        for i in 0..4000 {
            let x = if i % 2 == 0 { 0.25 } else { -0.25 }; // Nyquist
            let v = d.filter(x, x).0;
            if i > 2000 {
                e = e.max(v.abs());
            }
        }
        assert!(e < 0.25 * 0.01, "{e}");
    }

    #[test]
    fn the_index_rounds_half_to_even() {
        assert_eq!([index(0.0), index(0.5 / 256.0), index(1.5 / 256.0), index(0.999), index(1.0)], [0, 0, 2, 256, 256]);
    }
}

/// Opt-in parity with the original's cycle (`tools/tf3-parity/build-ref.sh` builds `$TF3_REF/build/cycle-ref`; absent
/// → the test passes without checking). Every mode, 1–4 points, spread and edge offsets.
#[cfg(all(test, feature = "std"))]
mod parity {
    use super::*;

    fn reference(p: &[f32; 14]) -> Option<std::vec::Vec<f32>> {
        let dir = std::env::var("TF3_REF").unwrap_or_else(|_| "/tmp/tf3-ref".into());
        let exe = std::path::Path::new(&dir).join("build/cycle-ref");
        if !exe.exists() {
            return None;
        }
        let out = std::process::Command::new(exe).args(p.iter().map(|v| std::format!("{v}"))).output().ok()?;
        Some(std::string::String::from_utf8(out.stdout).ok()?.lines().map(|l| l.parse().unwrap()).collect())
    }

    #[test]
    fn the_cycle_matches_the_original() {
        let cases: &[(usize, [(f32, f32); 4], [u8; 5])] = &[
            (1, [(0.5, 1.0), (0.5, 0.5), (0.5, 0.5), (0.5, 0.5)], [STEP; 5]),
            (1, [(0.5, 1.0), (0.5, 0.5), (0.5, 0.5), (0.5, 0.5)], [LINEAR; 5]),
            (1, [(0.25, 1.0), (0.5, 0.5), (0.5, 0.5), (0.5, 0.5)], [SPLINE; 5]),
            (2, [(0.25, 1.0), (0.66, 0.0), (0.6, 0.0), (0.8, 0.2)], [SPLINE; 5]),
            (3, [(0.1, 0.9), (0.3, 0.1), (0.5, 0.7), (0.8, 0.2)], [SPLINE, LINEAR, STEP, SPLINE, LINEAR]),
            (4, [(0.06, 1.0), (0.64, 0.06), (0.79, 0.98), (0.8, 0.2)], [STEP, LINEAR, LINEAR, SPLINE, SPLINE]),
            (4, [(0.0, 0.0), (1.0, 1.0), (0.3, 0.3), (0.99, 0.9)], [SPLINE; 5]),
        ];
        let mut checked = 0;
        for (n, at, modes) in cases {
            // the original's 14 values: point count, then per point offset, value, mode, then the last mode
            let mut p = [0.0f32; 14];
            p[0] = (*n as f32 - 1.0) / 3.0;
            for k in 0..MAX_POINTS {
                p[1 + 3 * k] = at[k].0;
                p[2 + 3 * k] = at[k].1;
                p[3 + 3 * k] = modes[k] as f32 / 2.0;
            }
            p[13] = modes[MAX_POINTS] as f32 / 2.0;
            let Some(want) = reference(&p) else { return };
            let s = Shape { count: *n, offset: at.map(|a| a.0), value: at.map(|a| a.1), modes: *modes };
            let got = cycle(&s);
            let err = got.iter().zip(&want).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(err < 1e-4, "{n} points {modes:?}: max error {err}");
            checked += 1;
        }
        std::eprintln!("{checked} cycles match the original");
    }
}

