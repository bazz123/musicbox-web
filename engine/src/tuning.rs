//! Key, scale, reference pitch and intonation.
//!
//! Added in musicbox-web; not part of upstream musicbox. The tracks were written
//! with hard-coded pitches in A (A minor pentatonic for the drone, A natural minor
//! for ambient techno). `Tuning::map` re-reads each of those pitches as an interval
//! above A, moves it to the same scale degree of the chosen scale, transposes it to
//! the chosen key, and tunes it in equal temperament or just intonation.
//!
//! `Tuning::default()` (A, original scale, A4 = 440 Hz, equal temperament) returns
//! every pitch unchanged, so the default output is identical to upstream.

/// Semitone intervals above the root.
pub const MINOR_PENTATONIC: [i32; 5] = [0, 3, 5, 7, 10];
pub const AEOLIAN: [i32; 7] = [0, 2, 3, 5, 7, 8, 10];
pub const DORIAN: [i32; 7] = [0, 2, 3, 5, 7, 9, 10];
pub const PHRYGIAN: [i32; 7] = [0, 1, 3, 5, 7, 8, 10];
pub const IN_SEN: [i32; 5] = [0, 1, 5, 7, 10];
pub const HIRAJOSHI: [i32; 5] = [0, 2, 3, 7, 8];

/// Scale ids used across the wasm boundary. 0 = the track's original scale.
pub fn scale_by_id(id: u32) -> Option<&'static [i32]> {
    match id {
        1 => Some(&MINOR_PENTATONIC),
        2 => Some(&AEOLIAN),
        3 => Some(&DORIAN),
        4 => Some(&PHRYGIAN),
        5 => Some(&IN_SEN),
        6 => Some(&HIRAJOSHI),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Intonation {
    Equal,
    /// 5-limit just intonation. The minor seventh is 9/5, the same ratio as one of
    /// the techno track's pulse rates.
    Just5,
    /// 7-limit: harmonic seventh 7/4 (also a pulse ratio) and septimal tritone 7/5.
    Just7,
}

const JUST5: [f64; 12] = [
    1.0, 16.0 / 15.0, 9.0 / 8.0, 6.0 / 5.0, 5.0 / 4.0, 4.0 / 3.0,
    45.0 / 32.0, 3.0 / 2.0, 8.0 / 5.0, 5.0 / 3.0, 9.0 / 5.0, 15.0 / 8.0,
];
const JUST7: [f64; 12] = [
    1.0, 16.0 / 15.0, 9.0 / 8.0, 6.0 / 5.0, 5.0 / 4.0, 4.0 / 3.0,
    7.0 / 5.0, 3.0 / 2.0, 8.0 / 5.0, 5.0 / 3.0, 7.0 / 4.0, 15.0 / 8.0,
];

#[derive(Clone, Debug)]
pub struct Tuning {
    /// Key root in semitones from A, -5..=6 (E up to D#), so the register stays close to the original.
    pub root: i32,
    /// Target scale; None keeps each track's original scale.
    pub scale: Option<&'static [i32]>,
    /// Frequency of A4 in Hz.
    pub reference: f32,
    pub intonation: Intonation,
}

impl Default for Tuning {
    fn default() -> Self {
        Self { root: 0, scale: None, reference: 440.0, intonation: Intonation::Equal }
    }
}

impl Tuning {
    pub fn is_default(&self) -> bool {
        self.root == 0 && self.scale.is_none() && self.reference == 440.0 && self.intonation == Intonation::Equal
    }

    /// Frequency of the note `n` semitones above the key root in octave 4.
    pub fn pitch(&self, n: i32) -> f32 {
        let root_hz = self.reference as f64 * 2f64.powf(self.root as f64 / 12.0);
        let f = match self.intonation {
            Intonation::Equal => root_hz * 2f64.powf(n as f64 / 12.0),
            Intonation::Just5 | Intonation::Just7 => {
                let table = if self.intonation == Intonation::Just5 { &JUST5 } else { &JUST7 };
                root_hz * table[n.rem_euclid(12) as usize] * 2f64.powi(n.div_euclid(12))
            }
        };
        f as f32
    }

    /// Re-tune a pitch written in A, whose original scale is `src_scale`.
    pub fn map(&self, src_hz: f32, src_scale: &[i32]) -> f32 {
        if self.is_default() || src_hz <= 0.0 {
            return src_hz;
        }
        let x = 12.0 * (src_hz as f64 / 440.0).log2();
        let n = x.round() as i32;
        // Source tables are equal-tempered values rounded to 0.01 Hz; offsets under
        // 5 cents are that rounding, larger ones are deliberate detune and are kept.
        let cents = if (x - n as f64).abs() < 0.05 { 0.0 } else { x - n as f64 };
        let (oct, pc) = (n.div_euclid(12), n.rem_euclid(12));
        let pc = match self.scale {
            Some(target) => degree_map(pc, src_scale, target),
            None => pc,
        };
        self.pitch(oct * 12 + pc) * 2f64.powf(cents / 12.0) as f32
    }

    pub fn map3(&self, notes: [f32; 3], src_scale: &[i32]) -> [f32; 3] {
        notes.map(|f| self.map(f, src_scale))
    }
}

/// Move pitch class `pc` from `src` to `tgt`. Same-size scales map degree to degree,
/// so a mode change alters exactly the notes that differ. Otherwise the nearest
/// target note wins, ties going up (12 stands for the root an octave up).
fn degree_map(pc: i32, src: &[i32], tgt: &[i32]) -> i32 {
    if src.len() == tgt.len() {
        if let Some(i) = src.iter().position(|&s| s == pc) {
            return tgt[i];
        }
    }
    let mut best = 0;
    let mut best_d = i32::MAX;
    for &t in tgt.iter().chain(std::iter::once(&12)) {
        let d = (t - pc).abs();
        if d < best_d || (d == best_d && t > best) {
            best = t;
            best_d = d;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_identity() {
        let t = Tuning::default();
        for f in [41.2_f32, 110.0, 130.81, 523.25] {
            assert_eq!(t.map(f, &AEOLIAN), f);
        }
    }

    #[test]
    fn mode_change_moves_only_the_changed_degree() {
        let t = Tuning { scale: Some(&DORIAN), ..Default::default() };
        assert!((t.map(174.61, &AEOLIAN) - 185.0).abs() < 0.1); // F3 -> F#3
        assert!((t.map(130.81, &AEOLIAN) - 130.81).abs() < 0.1); // C3 stays
        let t = Tuning { scale: Some(&PHRYGIAN), ..Default::default() };
        assert!((t.map(123.47, &AEOLIAN) - 116.54).abs() < 0.1); // B2 -> Bb2
    }

    #[test]
    fn key_and_reference() {
        let t = Tuning { root: 2, reference: 432.0, ..Default::default() }; // B, A4 = 432
        let b = 432.0 * 2f32.powf(2.0 / 12.0) / 4.0; // B2
        assert!((t.map(110.0, &AEOLIAN) - b).abs() < 0.01);
    }

    #[test]
    fn just_intonation_ratios() {
        let t = Tuning { intonation: Intonation::Just5, ..Default::default() };
        assert!((t.map(164.81, &AEOLIAN) / 110.0 - 1.5).abs() < 1e-5); // E3 is a pure 3/2 over A2
        assert!((t.map(196.0, &AEOLIAN) / 110.0 - 1.8).abs() < 1e-5); // G3 = 9/5
        let t = Tuning { intonation: Intonation::Just7, ..Default::default() };
        assert!((t.map(196.0, &AEOLIAN) / 110.0 - 1.75).abs() < 1e-5); // G3 = 7/4
    }
}
