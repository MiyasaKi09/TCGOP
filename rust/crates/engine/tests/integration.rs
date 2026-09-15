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

/// Decision §8.28 (follow-up) — every shipped decklist's signature SR Devil
/// Fruit has a legal bearer.
///
/// `MG-014` / `BW-011` / `MR-011` are printed "Équipable sur Luffy /
/// Crocodile / Akainu": names that exist in the game only as captains. Under
/// the character-only reading each of the three decks carried a permanently
/// dead SR card, and `BW-011`'s Ground Death — the §8.38 × §8.48 work — could
/// never be played at all.
#[test]
fn every_signature_fruit_has_a_bearer_in_its_own_deck() {
    let registry = cards::registry();
    for (deck, fruit_id) in [
        ("Mugiwara - Pre-Ellipse", "MG-014"),
        ("Marines - Justice Absolue", "MR-011"),
        ("Baroque Works - Utopia", "BW-011"),
    ] {
        let deck_def = all_decks()
            .into_iter()
            .find(|d| d.name == deck)
            .expect("a shipped decklist");
        assert!(
            deck_def.cards.iter().any(|c| c.card_id == fruit_id),
            "{deck} ships {fruit_id}"
        );
        let fruit = registry.get_card_def(fruit_id).expect("a shipped fruit");
        let restriction = fruit
            .restriction
            .as_deref()
            .expect("the fruit prints a restriction");
        // No character in the whole catalogue satisfies it …
        assert!(
            !registry.all_card_defs().values().any(|c| c.card_type
                == tcgop_engine::types::CardType::Character
                && (c.name.contains(restriction)
                    || c.tags
                        .as_deref()
                        .unwrap_or(&[])
                        .iter()
                        .any(|t| t == restriction))),
            "{fruit_id} would have a character bearer after all"
        );
        // … and the deck's own captain does.
        let cap = registry
            .get_captain_def(&deck_def.captain_id)
            .expect("a shipped captain");
        assert!(
            cap.name.contains(restriction),
            "{} does not satisfy \"Équipable sur {restriction}\"",
            cap.name
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
                !valid_actions(&state, &registry)
                    .expect("a fresh game enumerates")
                    .is_empty(),
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
    // Decision §8.57: `get_valid_actions` is the legality contract, so the AI
    // never needs the host's `passCounter` / `endTurn` catch. The last five
    // fallbacks were `survive` counters offered against a captain-targeted
    // attack, which `get_eligible_counters` now screens out.
    assert_eq!(
        fallbacks, 0,
        "the AI lost {fallbacks} action(s) to the host fallback — an offered action threw"
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

/// Decision §8.57: **every** action `get_valid_actions` offers must be one
/// `apply` accepts — there is no whitelist any more.
///
/// The four screening gaps the TypeScript engine had (`equipObject` ignoring
/// the slot cap and the printed restriction, `playEvent` ignoring a
/// `requiresOwnKO` Flashback, `useHaki { observation }` run for the wrong
/// player, and a `survive` `playCounter` offered against a captain target)
/// were all closed by decisions §8.28/§8.43/§8.57, so a rejection here is a
/// bug, not a documented mismatch. Every state of every game is checked (the
/// old stride of 7 hid the `equipObject: Not your character` class that
/// §8.37's borrowed bodies introduced).
#[test]
fn every_valid_action_is_accepted_by_apply() {
    let registry = cards::registry();
    let decks = all_decks();
    let mut checked_states = 0usize;
    let mut checked_actions = 0usize;
    let mut rejected: Vec<String> = Vec::new();
    let mut loan_states = 0usize;

    for difficulty in DIFFICULTIES {
        for (i, seed) in [5u64, 17, 2024, 999_983].iter().enumerate() {
            let a = &decks[i % decks.len()];
            let b = &decks[(i + 2) % decks.len()];
            let out = play_game(&registry, a, b, *seed, difficulty, 1).expect("game");
            assert!(
                out.state.winner.is_some(),
                "{difficulty:?} seed {seed}: the game did not finish"
            );

            for sample in &out.samples {
                if sample.winner.is_some() {
                    continue;
                }
                let actions = valid_actions(sample, &registry).expect("a live state enumerates");
                assert!(
                    !actions.is_empty(),
                    "{difficulty:?} seed {seed}: a live state offered no legal action"
                );
                checked_states += 1;
                if sample.cards.values().any(|c| c.controlled_by.is_some()) {
                    loan_states += 1;
                }
                for action in actions {
                    // Decision §8.37: an attack is always offered against the
                    // *other* side, borrowed bodies included — a loan fights
                    // for whoever controls it, never against its borrower.
                    let friendly_fire = match &action {
                        GameAction::BaseAttack {
                            attacker_instance_id,
                            target_instance_id,
                            ..
                        }
                        | GameAction::FruitSpecialAttack {
                            attacker_instance_id,
                            target_instance_id,
                            ..
                        } => {
                            // Decision §8.28 (follow-up): a captain now wears
                            // the fruit printed for it, so both ends of an
                            // attack may be the synthetic `captain_{playerId}`
                            // id, which is never a key of `state.cards`.
                            let side = |id: &String| -> Option<PlayerId> {
                                if let Some(rest) = id.strip_prefix("captain_") {
                                    return match rest {
                                        "player1" => Some(PlayerId::Player1),
                                        _ => Some(PlayerId::Player2),
                                    };
                                }
                                sample
                                    .cards
                                    .get(id)
                                    .map(|c| c.controlled_by.unwrap_or(c.owner))
                            };
                            side(attacker_instance_id) == side(target_instance_id)
                        }
                        _ => false,
                    };
                    assert!(
                        !friendly_fire,
                        "{difficulty:?} seed {seed} turn {}: friendly fire offered: {}",
                        sample.turn_number,
                        describe(&action)
                    );
                    let mut probe = sample.clone();
                    let label = describe(&action);
                    if let Err(e) = apply(&mut probe, &registry, action) {
                        rejected.push(format!(
                            "{difficulty:?} seed {seed} turn {}: {label} -> {e}",
                            sample.turn_number
                        ));
                    }
                    checked_actions += 1;
                }
            }
        }
    }

    assert!(
        checked_states > 500 && checked_actions > 5_000,
        "the sample is too thin to be meaningful ({checked_states} states, \
         {checked_actions} actions)"
    );
    assert!(
        loan_states > 0,
        "no sampled state had a body on loan — the friendly-fire screen above \
         (decision §8.37) went unexercised"
    );
    assert!(
        rejected.is_empty(),
        "{} of {checked_actions} actions offered by valid_actions were rejected by apply:\n{}",
        rejected.len(),
        rejected.join("\n")
    );
    println!(
        "checked {checked_actions} actions over {checked_states} states \
         ({loan_states} of them with a body on loan); 0 rejections"
    );
}

/// A compact label for panic messages (`GameAction` has no `Display`).
fn describe(action: &GameAction) -> String {
    format!("{}{:?}", action.type_name(), action)
}
