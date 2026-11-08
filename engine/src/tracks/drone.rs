use rand::Rng;
use rand::SeedableRng;

use crate::effects::{DattorroReverb, ResonantLpf};
use crate::instruments::oscillator::Oscillator;
use crate::instruments::pluck::PluckEngine;
use crate::track::{State, Track};
use crate::tuning::{NoteActivity, Tuning, MINOR_PENTATONIC};

const FADE_DURATION: f32 = 3.0;

/// A pentatonic minor scale: A, C, D, E, G
/// Base frequencies at octave 2, then generated across multiple octaves.
fn pentatonic_frequencies() -> Vec<f32> {
    let base_notes = [110.0, 130.81, 146.83, 164.81, 196.0]; // A2, C3, D3, E3, G3
    let mut freqs = Vec::new();
    for &octave_mult in &[0.5, 1.0, 2.0, 4.0] {
        for &f in &base_notes {
            freqs.push(f * octave_mult);
        }
    }
    freqs
}

/// A layer of oscillators in a frequency range, with optional effects.
pub struct Layer {
    pub oscillators: Vec<Oscillator>,
    base_rates: Vec<f32>,
    base_amps: Vec<f32>,
    pub filter: Option<ResonantLpf>,
    pub reverb: Option<DattorroReverb>,
}

impl Layer {
    pub fn new(freqs: &[f32], amplitude: f32, sparsity: f32, sample_rate: f32, rng: &mut impl Rng) -> Self {
        let oscillators = freqs
            .iter()
            .map(|&f| Oscillator::new(f, amplitude, sparsity, sample_rate, rng))
            .collect();
        let oscillators: Vec<Oscillator> = oscillators;
        let base_rates = oscillators.iter().map(|o: &Oscillator| o.envelope_rate).collect();
        let base_amps = oscillators.iter().map(|o: &Oscillator| o.amplitude).collect();
        Self { oscillators, base_rates, base_amps, filter: None, reverb: None }
    }

    // Detail controls (musicbox-web). Each leaves the layer as constructed at its default.

    /// Share of each swell cycle that is silent: 0 = always sounding.
    pub fn set_sparsity(&mut self, sparsity: f32) {
        for o in &mut self.oscillators { o.sparsity = sparsity; }
    }
    /// Multiplier on how fast notes swell in and out (1 = original).
    pub fn set_breath(&mut self, mul: f32) {
        for (o, &r) in self.oscillators.iter_mut().zip(&self.base_rates) { o.envelope_rate = r * mul; }
    }
    /// Multiplier on pitch drift depth (1 = original, 0 = none).
    pub fn set_drift(&mut self, mul: f32) {
        for o in &mut self.oscillators { o.drift_amount = o.freq * 0.003 * mul; }
    }
    /// Silence or restore oscillators below `below_hz`.
    pub fn set_sub(&mut self, on: bool, below_hz: f32) {
        for (o, &a) in self.oscillators.iter_mut().zip(&self.base_amps) {
            o.amplitude = if !on && o.freq < below_hz { 0.0 } else { a };
        }
    }

    pub fn with_filter(mut self, filter: ResonantLpf) -> Self {
        self.filter = Some(filter);
        self
    }

    pub fn with_reverb(mut self, reverb: DattorroReverb) -> Self {
        self.reverb = Some(reverb);
        self
    }

    pub fn next_sample(&mut self) -> (f32, f32) {
        let mut sample: f32 = self.oscillators.iter_mut().map(|o| o.next_sample()).sum();
        if let Some(f) = &mut self.filter {
            sample = f.process(sample);
        }
        if let Some(r) = &mut self.reverb {
            r.process(sample)
        } else {
            (sample, sample)
        }
    }
}

/// Drone generative engine: multiple layers of oscillators with effects,
/// implementing the `Track` trait.
pub struct Drone {
    bass: Layer,
    mid: Layer,
    high: Layer,
    plucks: PluckEngine,
    // Mixer and detail state (musicbox-web). Order: bass, mid, pluck, high.
    gain_target: [f32; 4],
    gain: [f32; 4],
    gain_coef: f32,
    levels: [f32; 4],
    level_decay: f32,
    warmth_drive: f32,
    warmth_norm: f32,
    limiter_on: bool,
    notes: NoteActivity,
    seen_plucks: u32,
    sr: f32,
    limiter_gain: f32,
    fade_pos: u32,
    fade_state: State,
    fade_samples: u32,
}

impl Drone {
    pub fn new(sample_rate: u32, seed: u64) -> Self {
        Self::with_tuning(sample_rate, seed, Tuning::default())
    }

    pub fn with_tuning(sample_rate: u32, seed: u64, tuning: Tuning) -> Self {
        let sr = sample_rate as f32;
        let fade_samples = (sr * FADE_DURATION) as u32;
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

        // Default tuning keeps the original note table exactly; otherwise build the
        // same four octaves (A1..G4 in the original) from the chosen scale and tuning.
        let scale: &[i32] = tuning.scale.unwrap_or(&MINOR_PENTATONIC);
        let all_freqs: Vec<f32> = if tuning.is_default() {
            pentatonic_frequencies()
        } else {
            (-3..=0).flat_map(|o| scale.iter().map(move |&i| (o, i)))
                .map(|(o, i)| tuning.pitch(o * 12 + i)).collect()
        };
        // Scales with more notes add oscillators; keep the summed level about the same.
        let k = (5.0 / scale.len() as f32).sqrt();

        let bass_freqs: Vec<f32> = all_freqs.iter().copied().filter(|&f| f < 130.0).collect();
        let mid_freqs: Vec<f32> = all_freqs
            .iter()
            .copied()
            .filter(|&f| (130.0..400.0).contains(&f))
            .collect();
        let high_freqs: Vec<f32> = all_freqs.iter().copied().filter(|&f| f >= 400.0).collect();
        let pluck_freqs: Vec<f32> = all_freqs
            .iter()
            .copied()
            .filter(|&f| (330.0..800.0).contains(&f))
            .collect();

        let bass = Layer::new(&bass_freqs, 0.15 * k, 0.7, sr, &mut rng);
        let mid_filter = ResonantLpf::new(200.0, 1200.0, 0.3, 0.065, sr, &mut rng);
        let mid = Layer::new(&mid_freqs, 0.08 * k, 0.5, sr, &mut rng).with_filter(mid_filter);
        let high_reverb = DattorroReverb::new(0.85, 0.4, 0.7, 0.04, sr, &mut rng);
        let high = Layer::new(&high_freqs, 0.04 * k, 0.3, sr, &mut rng).with_reverb(high_reverb);
        let mut plucks = PluckEngine::new(pluck_freqs, 0.3, sr, &mut rng);
        plucks.bbd_left.reserve_ms(1400.0);
        plucks.bbd_right.reserve_ms(1400.0);

        Self {
            bass,
            mid,
            high,
            plucks,
            gain_target: [1.0; 4],
            gain: [1.0; 4],
            gain_coef: 1.0 - (-1.0 / (0.02 * sr)).exp(),
            levels: [0.0; 4],
            level_decay: (-1.0 / (0.3 * sr)).exp(),
            warmth_drive: 1.0,
            warmth_norm: 1.0,
            limiter_on: true,
            notes: NoteActivity::new(&tuning),
            seen_plucks: 0,
            sr,
            limiter_gain: 1.0,
            fade_pos: 0,
            fade_state: State::FadingIn,
            fade_samples,
        }
    }

    /// Generate the next stereo sample pair, including fade and limiting.
    fn next_sample(&mut self) -> (f32, f32) {
        let master_gain = match self.fade_state {
            State::FadingIn => {
                self.fade_pos += 1;
                if self.fade_pos >= self.fade_samples {
                    self.fade_state = State::Playing;
                }
                let t = self.fade_pos as f32 / self.fade_samples as f32;
                t * t
            }
            State::Playing => 1.0,
            State::FadingOut => {
                if self.fade_pos == 0 {
                    self.fade_state = State::Done;
                    0.0
                } else {
                    self.fade_pos = self.fade_pos.saturating_sub(1);
                    let t = self.fade_pos as f32 / self.fade_samples as f32;
                    t * t
                }
            }
            State::Done => 0.0,
        };

        if self.fade_state == State::Done {
            return (0.0, 0.0);
        }

        let (mut bass_l, mut bass_r) = self.bass.next_sample();
        let (mut mid_l, mut mid_r) = self.mid.next_sample();
        let (mut high_l, mut high_r) = self.high.next_sample();
        let (mut pluck_l, mut pluck_r) = self.plucks.next_sample();

        if self.warmth_drive > 1.0 {
            bass_l = (bass_l * self.warmth_drive).tanh() / self.warmth_norm;
            bass_r = (bass_r * self.warmth_drive).tanh() / self.warmth_norm;
        }
        // Layer gains, smoothed over ~20 ms. At unity they leave samples untouched.
        for i in 0..4 {
            self.gain[i] += (self.gain_target[i] - self.gain[i]) * self.gain_coef;
        }
        let g = self.gain;
        bass_l *= g[0]; bass_r *= g[0];
        mid_l *= g[1]; mid_r *= g[1];
        pluck_l *= g[2]; pluck_r *= g[2];
        high_l *= g[3]; high_r *= g[3];
        for (i, (l, r)) in [(bass_l, bass_r), (mid_l, mid_r), (pluck_l, pluck_r), (high_l, high_r)].into_iter().enumerate() {
            let p = l.abs().max(r.abs());
            self.levels[i] = if p > self.levels[i] { p } else { self.levels[i] * self.level_decay };
        }

        let mut left = (bass_l + mid_l + high_l + pluck_l).tanh();
        let mut right = (bass_r + mid_r + high_r + pluck_r).tanh();

        // Peak limiter
        let peak = left.abs().max(right.abs());
        if !self.limiter_on {
            self.limiter_gain = 1.0;
        } else if peak * self.limiter_gain > 0.8 {
            let target = 0.8 / peak;
            self.limiter_gain += 0.002 * (target - self.limiter_gain);
        } else {
            self.limiter_gain += 0.0001 * (1.0 - self.limiter_gain);
        }

        left *= self.limiter_gain * master_gain;
        right *= self.limiter_gain * master_gain;

        (left, right)
    }
}

impl Drone {
    /// Peak level per layer (bass, mid, pluck, high), after layer gain.
    pub fn levels(&self) -> &[f32; 4] {
        &self.levels
    }

    /// Sounding notes by semitone above the key root, 0..1.
    pub fn notes(&self) -> &[f32; 12] {
        &self.notes.bins
    }

    /// Update the note display once per rendered block.
    fn update_notes(&mut self, frames: usize) {
        self.notes.decay((-(frames as f32) / (0.4 * self.sr)).exp());
        for (layer, g) in [(&self.bass, self.gain[0]), (&self.mid, self.gain[1]), (&self.high, self.gain[3])] {
            let g = g.min(1.0);
            for o in &layer.oscillators {
                if o.amplitude > 0.0 { self.notes.hit(o.freq, o.envelope() * g); }
            }
        }
        if self.plucks.plucked != self.seen_plucks {
            self.seen_plucks = self.plucks.plucked;
            self.notes.hit(self.plucks.last_freq, self.gain[2].min(1.0));
        }
    }

    /// Live controls (musicbox-web). Unknown names are ignored.
    pub fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "bass_gain" => self.gain_target[0] = v.clamp(0.0, 4.0),
            "mid_gain" => self.gain_target[1] = v.clamp(0.0, 4.0),
            "pluck_gain" => self.gain_target[2] = v.clamp(0.0, 4.0),
            "high_gain" => self.gain_target[3] = v.clamp(0.0, 4.0),
            "freeze" => if let Some(r) = &mut self.high.reverb { r.freeze = if v >= 0.5 { 1.0 } else { 0.0 } },
            "bass_sub" => self.bass.set_sub(v >= 0.5, 70.0),
            "bass_warmth" => {
                let w = v.clamp(0.0, 1.0);
                self.warmth_drive = 1.0 + 6.0 * w;
                self.warmth_norm = self.warmth_drive.powf(0.75);
            }
            "mid_cut_lo" => if let Some(f) = &mut self.mid.filter { f.cutoff_min = v.clamp(40.0, 4000.0) },
            "mid_cut_hi" => if let Some(f) = &mut self.mid.filter { f.cutoff_max = v.clamp(80.0, 8000.0) },
            "mid_res" => if let Some(f) = &mut self.mid.filter { f.resonance = v.clamp(0.0, 0.9) },
            "mid_sweep" => if let Some(f) = &mut self.mid.filter { f.cutoff_lfo_rate = v.clamp(0.005, 2.0) },
            "high_decay" => if let Some(r) = &mut self.high.reverb { r.reverb_time = v.clamp(0.3, 0.97) },
            "high_damp" => if let Some(r) = &mut self.high.reverb { r.lp = v.clamp(0.2, 1.0) },
            "high_mix" => if let Some(r) = &mut self.high.reverb {
                r.amount_min = (0.4 * v).clamp(0.0, 1.0);
                r.amount_max = (0.7 * v).clamp(0.0, 1.0);
            },
            "pluck_rate" => {
                let r = v.clamp(0.1, 8.0);
                let p = &mut self.plucks;
                p.min_interval = ((self.sr / r) as u32).max(1);
                p.max_interval = ((4.0 * self.sr / r) as u32).max(p.min_interval + 1);
                p.next_pluck_in = p.next_pluck_in.min(p.max_interval);
            }
            "pluck_bright" => for voice in &mut self.plucks.voices { voice.damp = v.clamp(0.1, 0.99) },
            "pluck_decay" => for voice in &mut self.plucks.voices { voice.feedback = v.clamp(0.9, 0.9995) },
            "bbd_time" => {
                let ms = v.clamp(40.0, 1200.0);
                self.plucks.bbd_left.delay_samples = ms * self.sr / 1000.0;
                self.plucks.bbd_right.delay_samples = ms * (370.0 / 340.0) * self.sr / 1000.0;
            }
            "bbd_feedback" => { let f = v.clamp(0.0, 0.92); self.plucks.bbd_left.feedback = f; self.plucks.bbd_right.feedback = f; }
            "bbd_mix" => { let m = v.clamp(0.0, 1.0); self.plucks.bbd_left.mix = m; self.plucks.bbd_right.mix = m; }
            "bbd_wow" => {
                let m = v.clamp(0.0, 4.0);
                self.plucks.bbd_left.wobble_depth = 1.5 * m * self.sr / 1000.0;
                self.plucks.bbd_right.wobble_depth = 1.8 * m * self.sr / 1000.0;
            }
            "fade_time" => {
                let new = (v.clamp(0.5, 120.0) * self.sr) as u32;
                self.fade_pos = ((self.fade_pos as u64 * new as u64) / self.fade_samples.max(1) as u64) as u32;
                self.fade_samples = new.max(1);
            }
            "engine_limiter" => self.limiter_on = v >= 0.5,
            _ => {
                // "<layer>_sparsity" | "<layer>_breath" | "<layer>_drift" for bass, mid, high
                if let Some((pre, what)) = name.split_once('_') {
                    let l = match pre {
                        "bass" => &mut self.bass,
                        "mid" => &mut self.mid,
                        "high" => &mut self.high,
                        _ => return,
                    };
                    {
                        match what {
                            "sparsity" => l.set_sparsity(v.clamp(0.0, 0.95)),
                            "breath" => l.set_breath(v.clamp(0.1, 8.0)),
                            "drift" => l.set_drift(v.clamp(0.0, 6.0)),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

impl Track for Drone {
    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());
        for i in 0..len {
            let (l, r) = self.next_sample();
            left[i] = l;
            right[i] = r;
        }
        self.update_notes(len);
    }

    fn start_fade_out(&mut self) {
        if self.fade_state == State::FadingIn || self.fade_state == State::Playing {
            self.fade_state = State::FadingOut;
        }
    }

    fn state(&self) -> State {
        self.fade_state
    }
}

#[cfg(test)]
mod mixer_tests {
    use super::*;
    use crate::effects::DattorroReverb;
    use rand::SeedableRng;

    fn tail_energy(freeze: bool) -> f32 {
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        let mut rv = DattorroReverb::new(0.85, 1.0, 1.0, 0.0, 48000.0, &mut rng);
        let mut x: u32 = 1;
        for _ in 0..48000 {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            rv.process((x >> 8) as f32 / 16777216.0 - 0.5);
        }
        rv.freeze = if freeze { 1.0 } else { 0.0 };
        for _ in 0..48000 * 8 { rv.process(0.0); }
        (0..48000).map(|_| { let (l, r) = rv.process(0.0); l * l + r * r }).sum()
    }

    #[test]
    fn freeze_holds_the_tail() {
        let frozen = tail_energy(true);
        let free = tail_energy(false);
        assert!(frozen > 1.0, "frozen tail should still be loud after 8 s, got {frozen}");
        assert!(free < frozen * 1e-3, "unfrozen tail should have decayed ({free} vs {frozen})");
    }

    #[test]
    fn zero_gains_silence_the_drone() {
        let mut d = Drone::new(48000, 9);
        for n in ["bass_gain", "mid_gain", "pluck_gain", "high_gain"] { d.set_param(n, 0.0); }
        let (mut l, mut r) = (vec![0.0; 48000], vec![0.0; 48000]);
        d.render(&mut l, &mut r); // smoothing settles well within this second
        d.render(&mut l, &mut r);
        assert!(l.iter().chain(&r).all(|s| s.abs() < 1e-6));
        assert!(d.levels().iter().all(|&v| v < 1e-3));
    }

    #[test]
    fn detail_params_stay_finite() {
        let mut d = Drone::new(48000, 5);
        for (n, v) in [("mid_res", 5.0), ("mid_cut_hi", 1e9), ("high_decay", 2.0), ("high_damp", 1.0),
                       ("bbd_time", 99999.0), ("bbd_feedback", 3.0), ("bbd_wow", 9.0), ("pluck_rate", 100.0),
                       ("bass_warmth", 1.0), ("bass_sparsity", 0.0), ("mid_breath", 8.0), ("fade_time", 0.0)] {
            d.set_param(n, v);
        }
        let (mut l, mut r) = (vec![0.0; 4096], vec![0.0; 4096]);
        for _ in 0..200 {
            d.render(&mut l, &mut r);
            assert!(l.iter().chain(&r).all(|s| s.is_finite() && s.abs() <= 1.0));
        }
    }
}
