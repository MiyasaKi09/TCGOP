//! End-to-end integration tests for the ported engine.
//!
//! These drive the crate strictly through its public facade — `cards::registry()`,
//! `GameState::new_game_from_decks`, `valid_actions`, `apply` and
//! `ai::ai_choose_action` — i.e. exactly the surface a host (the Bevy client)
//! sees. Nothing here reaches into module internals.

use std::collections::BTreeSet;

use tcgop_engine::ai::{Difficulty, ai_choose_action};
use tcgop_engine::context::EngineContext;
use tcgop_engine::decks::{all_decks, verify_deck, verify_deck_against};
use tcgop_engine::error::EngineError;
use tcgop_engine::registry::CardRegistry;
use tcgop_engine::state::GameState;
use tcgop_engine::types::{DeckDef, GameAction, PlayerId};
use tcgop_engine::{apply, cards, valid_actions};

// ============================================================
// Harness
// ============================================================

/// The three difficulties, in `ai.ts` declaration order.
const DIFFICULTIES: [Difficulty; 3] = [
    Difficulty::Beginner,
    Difficulty::Intermediate,
    Difficulty::Expert,
];

/// Turn cap from the task contract: a game must reach a winner within 200 turns.
const MAX_TURNS: u32 = 200;

/// Safety net so a stuck action loop fails the test instead of hanging: an
/// upper bound on the number of `apply` calls a 200-turn game can need.
const MAX_ACTIONS: usize = 200_000;

/// Whose turn it is to act — the defender during a counter window, otherwise
/// the active player. Mirrors what `valid_actions` itself picks.
fn actor(state: &GameState) -> PlayerId {
    if state.pending_attack.is_some() {
        state.current_player.opponent()
    } else {
        state.current_player
    }
}

/// The outcome of one scripted AI-vs-AI game.
struct GameRun {
    state: GameState,
    actions: usize,
    /// How often the AI's own pick was refused by `apply` and the TS host
    /// fallback (`passCounter` / `endTurn`) had to take over.
    fallbacks: usize,
    /// States sampled along the way, for the "every valid action applies" check.
    samples: Vec<GameState>,
}

/// Play one full AI-vs-AI game and return the final state.
///
/// `sample_every` states are kept aside (0 = keep none). Both seats share one
/// seeded [`EngineContext`], which is the AI's only source of randomness;
/// `apply` derives its own context from the state, so the whole run is a pure
/// function of `(deck_a, deck_b, seed, difficulty)`.
fn play_game(
    registry: &CardRegistry,
    deck_a: &DeckDef,
    deck_b: &DeckDef,
    seed: u64,
    difficulty: Difficulty,
    sample_every: usize,
) -> Result<GameRun, EngineError> {
    let mut state = GameState::new_game_from_decks(registry, deck_a, deck_b, seed)?;
    let mut ai_ctx = EngineContext::seeded(seed ^ 0x5eed_a1a1_u64);
    let mut actions = 0usize;
    let mut fallbacks = 0usize;
    let mut samples = Vec::new();

    while state.winner.is_none() && state.turn_number <= MAX_TURNS && actions < MAX_ACTIONS {
        if sample_every > 0 && actions.is_multiple_of(sample_every) {
            samples.push(state.clone());
        }
        let player = actor(&state);
        let action = ai_choose_action(&state, registry, &mut ai_ctx, player, difficulty)?;
        if apply(&mut state, registry, action).is_err() {
            // Mirror the TS host (`useGameEngine.ts:106-109`): a throwing action
            // falls back to passCounter / endTurn.
            let fallback = if state.pending_attack.is_some() {
                GameAction::PassCounter
            } else {
                GameAction::EndTurn
            };
            apply(&mut state, registry, fallback)?;
            fallbacks += 1;
        }
        actions += 1;
    }

    Ok(GameRun {
        state,
        actions,
        fallbacks,
        samples,
    })
}

// ============================================================
// (a) Registry + decks
// ============================================================

#[test]
fn registry_loads_every_set_and_all_captains() {
    let sets = cards::all_sets();
    assert_eq!(
        sets.len(),
        5,
        "index.ts spreads five card sets into allCards"
    );
    for (i, set) in sets.iter().enumerate() {
        assert!(!set.is_empty(), "card set #{i} is empty");
    }

    let all = cards::all_cards();
    let ids: BTreeSet<&str> = all.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        ids.len(),
        all.len(),
        "duplicate card id in allCards — the registry would silently drop one"
    );

    let registry = cards::registry();
    assert_eq!(registry.card_count(), all.len());
    for card in &all {
        assert!(
            registry.get_card_def(&card.id).is_ok(),
            "{} is missing from the registry",
            card.id
        );
    }

    let captains = cards::all_captains();
    assert!(!captains.is_empty());
    assert_eq!(registry.captain_count(), captains.len());
    for cap in &captains {
        assert!(
            registry.get_captain_def(&cap.id).is_ok(),
            "{} is missing from the captain registry",
            cap.id
        );
    }
}

#[test]
fn all_four_decks_verify_against_the_registry() {
    let registry = cards::registry();
    let decks = all_decks();
    assert_eq!(decks.len(), 4, "decks.ts exports four decks");

    for deck in &decks {
        // decks.ts `verifyDeck` — the size warning (None == no warning).
        assert_eq!(
            verify_deck(deck),
            None,
            "deck \"{}\" does not have 50 cards",
            deck.name
        );
        // Every referenced card id + the captain resolve.
        verify_deck_against(deck, &registry)
            .unwrap_or_else(|e| panic!("deck \"{}\" does not verify: {e}", deck.name));
        assert!(
            registry.get_captain_def(&deck.captain_id).is_ok(),
            "deck \"{}\" references unknown captain {}",
            deck.name,
            deck.captain_id
        );
    }
}

#[test]
fn a_game_can_be_created_from_every_deck_pairing() {
    let registry = cards::registry();
    let decks = all_decks();
    for a in &decks {
        for b in &decks {
            let state = GameState::new_game_from_decks(&registry, a, b, 7)
                .unwrap_or_else(|e| panic!("{} vs {}: {e}", a.name, b.name));
            assert_eq!(state.turn_number, 1);
            assert!(state.winner.is_none());
            assert!(
                !valid_actions(&state, &registry).is_empty(),
                "{} vs {}: no legal action on turn 1",
                a.name,
                b.name
            );
        }
    }
}

// ============================================================
// (b) AI-vs-AI full games
// ============================================================

#[test]
fn ai_games_terminate_with_a_winner_for_every_difficulty_and_seed() {
    let registry = cards::registry();
    let decks = all_decks();
    let seeds: [u64; 8] = [1, 2, 3, 7, 42, 1337, 12345, 999_983];

    let mut run = 0usize;
    let mut finished = 0usize;
    let mut fallbacks = 0usize;
    let mut total_actions = 0usize;
    let mut longest = 0u32;
    let mut wins = [0usize; 2];
    let mut unfinished: Vec<String> = Vec::new();

    for difficulty in DIFFICULTIES {
        for (i, seed) in seeds.iter().enumerate() {
            // Rotate the deck pairing so every set gets played by both seats.
            let a = &decks[i % decks.len()];
            let b = &decks[(i + 1) % decks.len()];
            run += 1;
            let out = play_game(&registry, a, b, *seed, difficulty, 0)
                .unwrap_or_else(|e| panic!("{difficulty:?} seed {seed}: apply failed: {e}"));
            total_actions += out.actions;
            fallbacks += out.fallbacks;
            longest = longest.max(out.state.turn_number);
            match out.state.winner {
                Some(PlayerId::Player1) => {
                    finished += 1;
                    wins[0] += 1;
                }
                Some(PlayerId::Player2) => {
                    finished += 1;
                    wins[1] += 1;
                }
                None => unfinished.push(format!(
                    "  {difficulty:?} seed {seed} ({} vs {}): stopped at turn {} after {} actions",
                    a.name, b.name, out.state.turn_number, out.actions
                )),
            }
            assert!(
                out.actions < MAX_ACTIONS,
                "{difficulty:?} seed {seed}: hit the action safety net"
            );
            // A finished game must also be internally consistent.
            if let Some(winner) = out.state.winner {
                assert!(
                    out.state.player(winner.opponent()).captain.current_pv <= 0
                        || out.state.player(winner.opponent()).deck.is_empty(),
                    "{difficulty:?} seed {seed}: {winner:?} won but the loser is still standing"
                );
                assert!(
                    !out.state.log.is_empty(),
                    "{difficulty:?} seed {seed}: a finished game produced no log"
                );
            }
        }
    }

    println!(
        "{finished}/{run} AI games finished; longest {longest} turns, {total_actions} actions \
         total, {fallbacks} host fallbacks; P1 {} / P2 {}",
        wins[0], wins[1]
    );
    assert_eq!(
        finished,
        run,
        "{}/{run} AI games did not reach a winner within {MAX_TURNS} turns:\n{}",
        run - finished,
        unfinished.join("\n")
    );
}

// ============================================================
// (c) Determinism
// ============================================================

#[test]
fn the_same_seed_produces_the_same_serialised_final_state() {
    let registry = cards::registry();
    let decks = all_decks();

    for difficulty in DIFFICULTIES {
        for seed in [4u64, 99] {
            let a = &decks[0];
            let b = &decks[1];
            let first = play_game(&registry, a, b, seed, difficulty, 0).expect("run 1");
            let second = play_game(&registry, a, b, seed, difficulty, 0).expect("run 2");
            assert!(
                first.state.winner.is_some(),
                "{difficulty:?} seed {seed}: the game did not finish"
            );

            assert_eq!(
                first.actions, second.actions,
                "{difficulty:?} seed {seed}: action count differs between runs"
            );
            let j1 = serde_json::to_string(&first.state).expect("serialise run 1");
            let j2 = serde_json::to_string(&second.state).expect("serialise run 2");
            assert_eq!(
                j1, j2,
                "{difficulty:?} seed {seed}: final states differ between two runs of the same seed"
            );
        }
    }
}

#[test]
fn different_seeds_produce_different_games() {
    let registry = cards::registry();
    let decks = all_decks();
    let a = &decks[0];
    let b = &decks[1];

    let x = play_game(&registry, a, b, 1, Difficulty::Intermediate, 0).expect("seed 1");
    let y = play_game(&registry, a, b, 2, Difficulty::Intermediate, 0).expect("seed 2");
    assert!(x.state.winner.is_some() && y.state.winner.is_some());
    assert_ne!(
        serde_json::to_string(&x.state).unwrap(),
        serde_json::to_string(&y.state).unwrap(),
        "two different seeds produced an identical game — the RNG is not wired in"
    );
}

#[test]
fn a_serde_round_trip_of_a_final_state_is_stable() {
    let registry = cards::registry();
    let decks = all_decks();
    let out = play_game(&registry, &decks[2], &decks[3], 8, Difficulty::Expert, 0).expect("game");
    assert!(out.state.winner.is_some(), "the game did not finish");
    let json = serde_json::to_string(&out.state).expect("serialise");
    let back: GameState = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(out.state, back);
    assert_eq!(json, serde_json::to_string(&back).unwrap());
}

// ============================================================
// (d) valid_actions ⊆ accepted-by-apply
// ============================================================

/// The four places where the TypeScript engine itself offers an action that
/// its own `executeAction` then refuses — `getValidActions` screens on card
/// *type* and cost only, while the mutator re-checks the rule. The port
/// reproduces them exactly (zero behavioural drift), so they are listed rather
/// than fixed; anything outside this list is a port bug.
///
/// - `equipObject`: `getValidActions` (turnManager.ts:899-913) never looks at
///   equipment slots; `equipObject` (board.ts:383-387) throws
///   `"${name} already has max ${subtype} equipped"`.
/// - `playEvent`: the event branch (turnManager.ts:915-922) only checks cost;
///   the `buffSingle` / Flashback effect (turnManager.ts:379-381) throws when
///   no own character has been KO'd this game.
/// - `useHaki { observation }`: offered when the *defender* still has it
///   (turnManager.ts:848), but `executeAction` runs `handleHaki` with
///   `state.currentPlayer` — the attacker (turnManager.ts:108-109, 809-816) —
///   so `useObservationHaki` (haki.ts:41) throws on the wrong player's flag.
/// - `playCounter`: every eligible counter is offered
///   (turnManager.ts:841-843), but a `survive` counter refuses a captain
///   target (combat.ts:572).
fn is_known_ts_mismatch(action: &GameAction, err: &EngineError) -> bool {
    let msg = err.to_string();
    match action {
        GameAction::EquipObject { .. } => msg.ends_with(" equipped"),
        GameAction::PlayEvent { .. } => {
            msg == "Flashback: aucun de vos personnages n'a été KO ce match"
        }
        GameAction::UseHaki { .. } => msg == "Observation Haki not available",
        GameAction::PlayCounter { .. } => msg == "Survive only protects allies",
        _ => false,
    }
}

#[test]
fn every_valid_action_is_accepted_by_apply() {
    let registry = cards::registry();
    let decks = all_decks();
    let mut checked_states = 0usize;
    let mut checked_actions = 0usize;
    let mut known_mismatches = 0usize;
    let mut unexpected: Vec<String> = Vec::new();

    for difficulty in DIFFICULTIES {
        for (i, seed) in [5u64, 17, 2024].iter().enumerate() {
            let a = &decks[i % decks.len()];
            let b = &decks[(i + 2) % decks.len()];
            let out = play_game(&registry, a, b, *seed, difficulty, 7).expect("game");
            assert!(
                out.state.winner.is_some(),
                "{difficulty:?} seed {seed}: the game did not finish"
            );

            for sample in &out.samples {
                if sample.winner.is_some() {
                    continue;
                }
                let actions = valid_actions(sample, &registry);
                assert!(
                    !actions.is_empty(),
                    "{difficulty:?} seed {seed}: a live state offered no legal action"
                );
                checked_states += 1;
                for action in actions {
                    let mut probe = sample.clone();
                    let label = describe(&action);
                    match apply(&mut probe, &registry, action.clone()) {
                        Ok(()) => {}
                        Err(e) if is_known_ts_mismatch(&action, &e) => known_mismatches += 1,
                        Err(e) => unexpected.push(format!(
                            "{difficulty:?} seed {seed} turn {}: {label} -> {e}",
                            sample.turn_number
                        )),
                    }
                    checked_actions += 1;
                }
            }
        }
    }

    assert!(
        checked_states > 50 && checked_actions > 500,
        "the sample is too thin to be meaningful ({checked_states} states, \
         {checked_actions} actions)"
    );
    assert!(
        unexpected.is_empty(),
        "{} of {checked_actions} actions offered by valid_actions were rejected by apply \
         for a reason the TypeScript engine does not have:\n{}",
        unexpected.len(),
        unexpected.join("\n")
    );
    // The documented TS mismatches must stay a rounding error, not the norm.
    assert!(
        known_mismatches * 20 < checked_actions,
        "{known_mismatches}/{checked_actions} rejections — far more than the four \
         known TypeScript screening gaps can explain"
    );
    println!(
        "checked {checked_actions} actions over {checked_states} states; \
         {known_mismatches} known TS screening gaps, 0 unexpected rejections"
    );
}

/// A compact label for panic messages (`GameAction` has no `Display`).
fn describe(action: &GameAction) -> String {
    format!("{}{:?}", action.type_name(), action)
}
