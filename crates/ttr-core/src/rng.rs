//! RNG seeded deterministico (piano §9.5).
//!
//! Usa `ChaCha12Rng` (via `StdRng`) per riproducibilità cross-platform.
//! Il seed è un `u64` passato alla creazione del motore.

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use serde::Serialize;

/// Generatore di numeri casuali seeded.
#[derive(Debug, Clone, Serialize)]
pub struct SeededRng {
    seed: u64,
    #[serde(skip)]
    rng: StdRng,
}

impl SeededRng {
    /// Crea un nuovo RNG dal seed.
    pub fn new(seed: u64) -> Self {
        let rng = StdRng::seed_from_u64(seed);
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

    /// Genera un `u64` casuale.
    #[allow(dead_code)]
    pub fn r#gen(&mut self) -> u64 {
        self.rng.r#gen()
    }

    /// Genera un `usize` nell'intervallo [0, max).
    #[allow(dead_code)]
    pub fn gen_usize(&mut self, max: usize) -> usize {
        if max == 0 {
            return 0;
        }
        self.rng.gen_range(0..max)
    }

    /// Ricrea l'RNG dal seed (per replay).
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.rng = StdRng::seed_from_u64(self.seed);
    }

    /// Serializza per il log (solo seed).
    #[allow(dead_code)]
    pub fn to_log(&self) -> u64 {
        self.seed
    }
}

impl PartialEq for SeededRng {
    fn eq(&self, other: &Self) -> bool {
        self.seed == other.seed
    }
}

impl Eq for SeededRng {}
