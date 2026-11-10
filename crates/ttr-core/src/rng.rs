//! RNG seeded deterministico (piano §9.5).
//!
//! Usa `ChaCha12Rng` esplicito (non `StdRng`, che può cambiare algoritmo tra
//! versioni di `rand`) per riproducibilità cross-platform; la sequenza coincide
//! con quella di `StdRng` di rand 0.8. Il seed è un `u64` passato alla
//! creazione del motore.
//!
//! L'algoritmo di `SliceRandom::shuffle` dipende dalla versione di `rand`,
//! fissata con `=` in `Cargo.toml`.

#[cfg(any(test, feature = "test-helpers"))]
use rand::Rng;
use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha12Rng;
use serde::Serialize;

/// Generatore di numeri casuali seeded.
#[derive(Debug, Clone, Serialize)]
pub struct SeededRng {
    seed: u64,
    #[serde(skip)]
    rng: ChaCha12Rng,
}

impl SeededRng {
    /// Crea un nuovo RNG dal seed.
    pub fn new(seed: u64) -> Self {
        let rng = ChaCha12Rng::seed_from_u64(seed);
        Self { seed, rng }
    }

    /// Restituisce il seed.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Mescola un vettore in-place (deterministico).
    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        slice.shuffle(&mut self.rng);
    }

    /// Prossima parola casuale a 64 bit.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn next_word(&mut self) -> u64 {
        self.rng.r#gen()
    }

    /// Ricrea l'RNG dal seed.
    #[cfg(test)]
    pub fn reset(&mut self) {
        self.rng = ChaCha12Rng::seed_from_u64(self.seed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Expected = (u64, [usize; 110], [u64; 3]);

    /// Per seed: `shuffle` di `0..110`, poi tre parole a 64 bit. Catturati con
    /// `rand::rngs::StdRng` (rand 0.8.8): un valore diverso vuol dire che i
    /// replay dei save esistenti divergono.
    const EXPECTED: [Expected; 3] = [
        (
            0,
            [
                29, 32, 58, 19, 28, 103, 68, 50, 69, 7, 20, 31, 54, 6, 74, 81, 5, 13, 33, 1, 55,
                14, 27, 82, 0, 105, 24, 35, 23, 76, 98, 65, 22, 3, 34, 17, 9, 15, 91, 57, 44, 95,
                67, 8, 106, 84, 51, 41, 80, 52, 48, 62, 43, 63, 47, 108, 102, 45, 97, 75, 42, 38,
                73, 71, 96, 25, 61, 66, 30, 11, 104, 109, 53, 64, 59, 36, 49, 39, 85, 87, 100, 56,
                46, 107, 78, 16, 40, 37, 92, 18, 90, 10, 4, 101, 94, 93, 72, 99, 21, 77, 12, 26,
                89, 60, 70, 2, 86, 83, 79, 88,
            ],
            [
                7949954922436041877,
                3564269110045988749,
                10075349004696675567,
            ],
        ),
        (
            42,
            [
                22, 109, 84, 66, 8, 65, 29, 41, 85, 81, 16, 106, 59, 15, 38, 9, 5, 48, 2, 25, 49,
                10, 26, 67, 23, 62, 17, 107, 108, 69, 7, 28, 80, 33, 54, 34, 47, 39, 51, 32, 93, 6,
                74, 90, 88, 86, 82, 92, 71, 103, 78, 97, 99, 19, 44, 60, 36, 77, 24, 96, 75, 91,
                87, 27, 56, 105, 11, 89, 40, 31, 21, 79, 30, 72, 37, 64, 20, 50, 52, 4, 104, 13, 1,
                53, 98, 70, 94, 95, 102, 100, 46, 55, 76, 0, 45, 12, 61, 83, 18, 73, 35, 42, 63, 3,
                101, 43, 68, 58, 57, 14,
            ],
            [
                8993766681204884938,
                13958912327111801715,
                5906399668772012937,
            ],
        ),
        (
            16045690984503111693,
            [
                41, 42, 36, 15, 84, 51, 46, 95, 72, 71, 49, 62, 37, 77, 68, 20, 34, 70, 83, 29, 22,
                26, 97, 17, 104, 28, 88, 44, 79, 4, 25, 101, 39, 74, 86, 92, 103, 48, 108, 60, 80,
                5, 40, 57, 13, 87, 102, 14, 50, 59, 43, 76, 24, 9, 54, 75, 61, 1, 8, 53, 30, 6, 94,
                45, 3, 47, 52, 38, 82, 73, 96, 89, 31, 90, 16, 58, 12, 91, 81, 64, 0, 63, 2, 35,
                69, 100, 107, 106, 55, 11, 67, 10, 33, 65, 27, 19, 85, 7, 105, 66, 109, 93, 32, 21,
                99, 18, 56, 98, 23, 78,
            ],
            [
                16193501367240947001,
                10672023964031919846,
                2937878312653908278,
            ],
        ),
    ];

    fn sequence(rng: &mut SeededRng) -> ([usize; 110], [u64; 3]) {
        let mut deck: [usize; 110] = std::array::from_fn(|i| i);
        rng.shuffle(&mut deck);
        let words = [rng.next_word(), rng.next_word(), rng.next_word()];
        (deck, words)
    }

    #[test]
    fn sequence_matches_values_captured_from_std_rng() {
        for (seed, deck, words) in EXPECTED {
            let mut rng = SeededRng::new(seed);
            assert_eq!(sequence(&mut rng), (deck, words), "seed {seed}");
        }
    }

    #[test]
    fn reset_restarts_the_sequence() {
        for (seed, deck, words) in EXPECTED {
            let mut rng = SeededRng::new(seed);
            sequence(&mut rng);
            rng.reset();
            assert_eq!(sequence(&mut rng), (deck, words), "seed {seed}");
        }
    }
}
