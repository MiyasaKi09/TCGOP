//! Engine context — the TypeScript engine's *module-level globals* gathered
//! into one explicit, injectable, serialisable value.
//!
//! The TS engine reaches for three ambient sources that are not part of
//! `GameState`:
//! - `Math.random()` (utils.ts `shuffle`, random discards / picks) → [`EngineContext::rng`];
//! - `utils.ts::instanceCounter` — a module global that lives for the whole
//!   process and is **never reset** by `createInitialState` (only by the
//!   unused `resetInstanceCounter`) → [`EngineContext::instance_counter`];
//! - `Date.now()` (the `generateInstanceId` suffix and a few modifier ids) →
//!   [`EngineContext::now_ms`].
//!
//! Keeping them out of `GameState` means the serialised `GameState` has
//! exactly the nine keys of the TS `GameState` interface, while the context can
//! still be saved next to it so a game resumes with the same RNG position and
//! counter.

use serde::{Deserialize, Serialize};

use crate::rng::EngineRng;

/// The TS module globals (`Math.random`, `instanceCounter`, `Date.now`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineContext {
    /// Seeded replacement for `Math.random()`.
    pub rng: EngineRng,
    /// TS `utils.ts::instanceCounter` — process-lifetime, never reset between
    /// games created from the same context.
    pub instance_counter: u64,
    /// The value `Date.now()` returns (milliseconds since the Unix epoch).
    /// Fixed for determinism; hosts may advance it with [`EngineContext::set_now_ms`].
    pub now_ms: u64,
}

impl Default for EngineContext {
    /// Seed 0, counter 0, clock 0 — only meaningful for tests / placeholders.
    fn default() -> Self {
        EngineContext::new(0, 0)
    }
}

impl EngineContext {
    /// A fresh context from a 64-bit seed and a fixed `Date.now()` reading.
    pub fn new(seed: u64, now_ms: u64) -> Self {
        EngineContext {
            rng: EngineRng::from_seed_u64(seed),
            instance_counter: 0,
            now_ms,
        }
    }

    /// Fully deterministic context: seed only, `Date.now()` pinned to `0`.
    pub fn seeded(seed: u64) -> Self {
        EngineContext::new(seed, 0)
    }

    /// Like the TS engine: `Date.now()` read from the system clock at
    /// construction. Only the id suffixes depend on it, never the rules.
    pub fn with_system_time(seed: u64) -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        EngineContext::new(seed, now_ms)
    }

    /// TS `Date.now()`.
    pub fn now(&self) -> u64 {
        self.now_ms
    }

    /// Advance / set the clock (`Date.now()` reading).
    pub fn set_now_ms(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// TS `Date.now().toString(36)`.
    pub fn now_base36(&self) -> String {
        to_base36(self.now_ms)
    }

    /// TS `generateInstanceId(defId)` = `${defId}_${++instanceCounter}_${Date.now().toString(36)}`.
    pub fn generate_instance_id(&mut self, def_id: &str) -> String {
        generate_instance_id(def_id, &mut self.instance_counter, self.now_ms)
    }

    /// TS `resetInstanceCounter()` (utils.ts, "for testing").
    pub fn reset_instance_counter(&mut self) {
        self.instance_counter = 0;
    }
}

/// TS `generateInstanceId(defId)` with the two globals passed explicitly:
/// `${defId}_${++counter}_${now_ms.toString(36)}`.
pub fn generate_instance_id(def_id: &str, counter: &mut u64, now_ms: u64) -> String {
    *counter += 1;
    format!("{def_id}_{counter}_{}", to_base36(now_ms))
}

/// JS `Number.prototype.toString(36)` for a non-negative integer
/// (lower-case digits `0-9a-z`, `0` for zero).
pub fn to_base36(mut n: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".to_string();
    }
    let mut buf: Vec<u8> = Vec::with_capacity(13);
    while n > 0 {
        buf.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    buf.reverse();
    // Only ASCII digits/letters were pushed.
    String::from_utf8(buf).expect("base36 digits are ASCII")
}
