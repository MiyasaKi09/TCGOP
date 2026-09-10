//! Seeded, serialisable random source.
//!
//! The TypeScript engine uses `Math.random()` (utils.ts `shuffle`, and the
//! random discards / token picks in later modules). The Rust engine injects
//! a `ChaCha8Rng` instead so that every game is reproducible from its seed.
//!
//! Draw semantics mirror `gameState.ts` exactly:
//! - the deck is a `Vec<instanceId>` whose **top is index 0** (TS comment
//!   `// instanceIds (top = index 0)`);
//! - `drawCard` does `player.deck.shift()` — i.e. `Vec::remove(0)`;
//! - the starting hand is `shuffledDeck.splice(0, STARTING_HAND_SIZE)` — i.e.
//!   `Vec::drain(..STARTING_HAND_SIZE)`;
//! - `shuffle` is the same Fisher–Yates loop as utils.ts
//!   (`for i = n-1 .. 1: j = floor(random * (i+1)); swap(i, j)`).

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

/// Deterministic RNG wrapper around `ChaCha8Rng`.
///
/// Serialised as `{ seed, word_pos_hi, word_pos_lo }` so a saved `GameState`
/// resumes with the exact same random stream position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineRng {
    inner: ChaCha8Rng,
}

#[derive(Serialize, Deserialize)]
struct EngineRngRepr {
    seed: [u8; 32],
    word_pos_hi: u64,
    word_pos_lo: u64,
}

impl Serialize for EngineRng {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let pos = self.inner.get_word_pos();
        EngineRngRepr {
            seed: self.inner.get_seed(),
            word_pos_hi: (pos >> 64) as u64,
            word_pos_lo: pos as u64,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EngineRng {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let repr = EngineRngRepr::deserialize(deserializer)?;
        let mut inner = ChaCha8Rng::from_seed(repr.seed);
        let pos = ((repr.word_pos_hi as u128) << 64) | repr.word_pos_lo as u128;
        inner.set_word_pos(pos);
        Ok(EngineRng { inner })
    }
}

impl Default for EngineRng {
    /// Seed 0 — only meaningful for tests / placeholder states.
    fn default() -> Self {
        EngineRng::from_seed_u64(0)
    }
}

impl EngineRng {
    /// Build from a 64-bit seed (`ChaCha8Rng::seed_from_u64`).
    pub fn from_seed_u64(seed: u64) -> Self {
        EngineRng {
            inner: ChaCha8Rng::seed_from_u64(seed),
        }
    }

    /// Build from a full 256-bit seed.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        EngineRng {
            inner: ChaCha8Rng::from_seed(seed),
        }
    }

    /// The 256-bit seed this stream was created from.
    pub fn seed(&self) -> [u8; 32] {
        self.inner.get_seed()
    }

    /// Direct access to the underlying generator for callers that need
    /// distributions not wrapped here.
    pub fn inner_mut(&mut self) -> &mut ChaCha8Rng {
        &mut self.inner
    }

    /// TS `Math.random()` — uniform in `[0, 1)`.
    pub fn random_f64(&mut self) -> f64 {
        self.inner.random::<f64>()
    }

    /// TS `Math.floor(Math.random() * len)` — uniform index in `0..len`.
    ///
    /// Panics if `len == 0` (callers must check emptiness first, exactly like
    /// the TS code guards `if (hand.length > 0)`).
    pub fn random_index(&mut self, len: usize) -> usize {
        assert!(len > 0, "random_index called with len == 0");
        self.inner.random_range(0..len)
    }

    /// Uniform pick from a slice (`arr[Math.floor(Math.random() * arr.length)]`).
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            let i = self.random_index(items.len());
            Some(&items[i])
        }
    }

    /// TS utils.ts `shuffle` — in-place Fisher–Yates with the identical loop
    /// (`i` from `n-1` down to `1`, `j` uniform in `0..=i`).
    ///
    /// Takes `&mut Vec<T>` (rather than a slice) on purpose: it is the
    /// signature the port contract specifies and what deck-building code holds.
    #[allow(clippy::ptr_arg)]
    pub fn shuffle<T>(&mut self, arr: &mut Vec<T>) {
        if arr.len() < 2 {
            return;
        }
        for i in (1..arr.len()).rev() {
            let j = self.inner.random_range(0..=i);
            arr.swap(i, j);
        }
    }

    /// TS utils.ts `shuffle` returning a shuffled copy (the TS function does
    /// not mutate its input).
    pub fn shuffled<T: Clone>(&mut self, arr: &[T]) -> Vec<T> {
        let mut out = arr.to_vec();
        self.shuffle(&mut out);
        out
    }
}

/// Draw from the **top** of a deck (`deck.shift()` in gameState.ts `drawCard`).
pub fn draw_top<T>(deck: &mut Vec<T>) -> Option<T> {
    if deck.is_empty() {
        None
    } else {
        Some(deck.remove(0))
    }
}

/// Take the first `n` cards from the top (`deck.splice(0, n)`), fewer if the
/// deck is shorter.
pub fn draw_top_n<T>(deck: &mut Vec<T>, n: usize) -> Vec<T> {
    let n = n.min(deck.len());
    deck.drain(..n).collect()
}
