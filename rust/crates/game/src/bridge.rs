//! The engine bridge — the **only** place the client talks to `tcgop_engine`.
//!
//! Contract (mirrors `src/hooks/useGameEngine.ts`):
//! - the client never re-implements a rule; it reads [`Session::valid`] and
//!   dispatches a `GameAction`;
//! - [`Session::valid`] always holds the **human** player's legal actions
//!   (empty while the AI is thinking, filled during a counter window even
//!   though it is the AI's turn) — exactly the TS `validActions` memo;
//! - every mutation goes through [`Session::dispatch`], which applies the
//!   action with the session's long-lived [`EngineContext`] (one continuous
//!   random stream for the whole game) and recomputes `valid`.
//!
//! Systems talk to the bridge with messages rather than calling `dispatch`
//! directly, so the whole flow is observable and head-less testable:
//!
//! ```text
//! any UI system  --DispatchAction(action)-->  apply_dispatched_actions
//!                                              |-- ok  --> ActionApplied(action) + StateChanged
//!                                              `-- err --> EngineErrorEvent { action, error }
//!                                                          refresh_valid_actions (on StateChanged)
//! ```

use std::sync::Arc;

use bevy::prelude::*;

use crate::app::{AppSet, configure_pipeline};
use tcgop_engine::ai::Difficulty;
use tcgop_engine::cards;
use tcgop_engine::context::EngineContext;
use tcgop_engine::error::EngineError;
use tcgop_engine::registry::CardRegistry;
use tcgop_engine::state::{GameState, LogEntry, create_game};
use tcgop_engine::types::{DeckDef, GameAction, PlayerId};
use tcgop_engine::{valid_actions_for, apply_with_ctx};

// ============================================================
// Messages ("events")
// ============================================================

/// Ask the bridge to apply an action. Written by every interaction system
/// (board clicks, hand clicks, modal buttons, the AI driver).
#[derive(Message, Debug, Clone, PartialEq)]
pub struct DispatchAction(pub GameAction);

/// An action was applied successfully. Carries the action that was applied, so
/// vfx / announcement systems can react to it.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct ActionApplied(pub GameAction);

/// `Session::state` changed. Emitted after every successful dispatch; rendering
/// systems rebuild from it.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StateChanged;

/// The engine refused an action. The client should never produce one of these
/// (it only ever dispatches actions taken from [`Session::valid`]), so it is a
/// bug report: log it, do not surface it to the player.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct EngineErrorEvent {
    pub action: GameAction,
    pub error: EngineError,
}

// ============================================================
// Session
// ============================================================

/// One game: the engine state, the card registry, the seeded context, who the
/// human is, and the cached list of that human's legal actions.
#[derive(Resource)]
pub struct Session {
    /// The authoritative rules state.
    pub state: GameState,
    /// Shared card catalogue (`tcgop_engine::cards::registry()`).
    pub registry: Arc<CardRegistry>,
    /// Long-lived, seeded RNG + instance counter + clock.
    pub ctx: EngineContext,
    /// Which side the player controls (always `Player1` today).
    pub human: PlayerId,
    /// Difficulty passed to `ai::ai_choose_action`.
    pub ai_level: Difficulty,
    /// The human's legal actions right now — TS `validActions`.
    pub valid: Vec<GameAction>,
    /// How much of `state.log` has already been consumed by the UI.
    pub log_cursor: usize,
}

impl Session {
    /// Create a game: `deck_you` is dealt to [`Session::human`] (`player1`),
    /// `deck_foe` to the AI. `seed` makes the whole game reproducible.
    pub fn new(
        deck_you: &DeckDef,
        deck_foe: &DeckDef,
        level: Difficulty,
        seed: u64,
    ) -> Result<Self, EngineError> {
        Session::with_registry(Arc::new(cards::registry()), deck_you, deck_foe, level, seed)
    }

    /// Same as [`Session::new`] but re-using an already built registry (the
    /// catalogue is immutable, so one `Arc` can be shared by every session).
    pub fn with_registry(
        registry: Arc<CardRegistry>,
        deck_you: &DeckDef,
        deck_foe: &DeckDef,
        level: Difficulty,
        seed: u64,
    ) -> Result<Self, EngineError> {
        let mut ctx = EngineContext::seeded(seed);
        let state = create_game(deck_you, deck_foe, &registry, &mut ctx)?;
        let mut session = Session {
            state,
            registry,
            ctx,
            human: PlayerId::Player1,
            ai_level: level,
            valid: Vec::new(),
            log_cursor: 0,
        };
        session.refresh_valid();
        Ok(session)
    }

    /// Apply one action and recompute [`Session::valid`].
    ///
    /// The engine is transactional: on `Err` the state is byte-for-byte
    /// unchanged, so a rejected action is a no-op.
    pub fn dispatch(&mut self, action: GameAction) -> Result<(), EngineError> {
        apply_with_ctx(&mut self.state, &self.registry, &mut self.ctx, action)?;
        self.refresh_valid();
        Ok(())
    }

    /// Recompute the human's legal actions — TS `useGameEngine`'s
    /// `validActions` memo, 1:1:
    /// - the game is over → nothing;
    /// - a counter window is open on the AI's turn → the human defends;
    /// - it is the human's turn and no attack is pending → the human plays;
    /// - otherwise (AI turn, or the human is the attacker waiting for the AI's
    ///   counter) → nothing.
    pub fn refresh_valid(&mut self) {
        let ai = self.ai_player();
        self.valid = if self.state.winner.is_some() {
            Vec::new()
        } else if self.state.pending_attack.is_some() && self.state.current_player == ai {
            // The engine enumeration is fallible (it mirrors the TS throw on a
            // corrupt state); a UI has nothing to offer in that case.
            valid_actions_for(&self.state, &self.registry, self.human).unwrap_or_default()
        } else if self.state.pending_attack.is_none() && self.state.current_player == self.human {
            valid_actions_for(&self.state, &self.registry, self.human).unwrap_or_default()
        } else {
            Vec::new()
        };
    }

    /// The AI's player id.
    pub fn ai_player(&self) -> PlayerId {
        self.human.opponent()
    }

    /// The human's `PlayerState`.
    pub fn you(&self) -> &tcgop_engine::state::PlayerState {
        self.state.player(self.human)
    }

    /// The AI's `PlayerState`.
    pub fn foe(&self) -> &tcgop_engine::state::PlayerState {
        self.state.player(self.ai_player())
    }

    /// TS `isAiTurn` — the AI is playing and no counter window is open.
    pub fn is_ai_turn(&self) -> bool {
        self.state.current_player == self.ai_player() && self.state.pending_attack.is_none()
    }

    /// TS `inCounterWindow` — an attack is waiting for a response.
    pub fn in_counter_window(&self) -> bool {
        self.state.pending_attack.is_some()
    }

    /// The winner, if the game is over.
    pub fn winner(&self) -> Option<PlayerId> {
        self.state.winner
    }

    /// `true` when the human won (only meaningful once [`Session::winner`] is `Some`).
    pub fn human_won(&self) -> bool {
        self.state.winner == Some(self.human)
    }

    /// Log entries appended since the last call, advancing [`Session::log_cursor`].
    pub fn drain_log(&mut self) -> Vec<LogEntry> {
        let from = self.log_cursor.min(self.state.log.len());
        self.log_cursor = self.state.log.len();
        self.state.log[from..].to_vec()
    }

    /// What the driver has to do next, if anything — TS `needsAutoAction`.
    pub fn needs_auto_action(&self) -> AutoAction {
        let ai = self.ai_player();
        if self.state.winner.is_some() {
            return AutoAction::None;
        }
        if self.state.pending_attack.is_some() {
            if self.state.current_player == self.human {
                // The human attacked: the AI decides whether to counter.
                return AutoAction::AiDefend;
            }
            // The AI attacked: the human answers — unless `passCounter` is the
            // only thing they could do, in which case pass automatically.
            let has_real_counter = valid_actions_for(&self.state, &self.registry, self.human)
                .unwrap_or_default()
                .iter()
                .any(|a| *a != GameAction::PassCounter);
            return if has_real_counter {
                AutoAction::None
            } else {
                AutoAction::AutoPass
            };
        }
        if self.state.current_player == ai {
            return AutoAction::AiTurn;
        }
        AutoAction::None
    }
}

/// The four outcomes of TS `needsAutoAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoAction {
    /// Nothing to do — waiting for the human.
    None,
    /// The AI must answer the human's attack (counter window).
    AiDefend,
    /// The human has no real counter: pass automatically.
    AutoPass,
    /// The AI plays its turn.
    AiTurn,
}

// ============================================================
// Plugin
// ============================================================

/// Systems that own [`Session`]: they are grouped so feature systems can order
/// themselves with `.after(BridgeSet::Sync)` and always see a fresh state.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BridgeSet;

/// Registers the bridge messages and the apply / refresh pair.
pub struct BridgePlugin;

impl Plugin for BridgePlugin {
    fn build(&self, app: &mut App) {
        configure_pipeline(app);
        app.add_message::<DispatchAction>()
            .add_message::<ActionApplied>()
            .add_message::<StateChanged>()
            .add_message::<EngineErrorEvent>()
            .add_systems(
                Update,
                (
                    apply_dispatched_actions.in_set(AppSet::Dispatch),
                    refresh_valid_actions.in_set(AppSet::Refresh),
                )
                    .chain()
                    .in_set(BridgeSet)
                    .run_if(resource_exists::<Session>),
            );
    }
}

/// Drain [`DispatchAction`] and apply each action in order.
fn apply_dispatched_actions(
    mut requests: MessageReader<DispatchAction>,
    mut session: ResMut<Session>,
    mut applied: MessageWriter<ActionApplied>,
    mut changed: MessageWriter<StateChanged>,
    mut failed: MessageWriter<EngineErrorEvent>,
) {
    let actions: Vec<GameAction> = requests.read().map(|r| r.0.clone()).collect();
    for action in actions {
        match session.dispatch(action.clone()) {
            Ok(()) => {
                applied.write(ActionApplied(action));
                changed.write(StateChanged);
            }
            Err(error) => {
                warn!("engine refused {}: {error}", action.type_name());
                failed.write(EngineErrorEvent { action, error });
            }
        }
    }
}

/// Recompute [`Session::valid`] whenever the state changed. `dispatch` already
/// does it, so this is the safety net for any code path that mutates
/// `session.state` directly and writes [`StateChanged`] itself.
fn refresh_valid_actions(mut changed: MessageReader<StateChanged>, mut session: ResMut<Session>) {
    if changed.read().count() == 0 {
        return;
    }
    session.refresh_valid();
}

// ============================================================
// Tests
// ============================================================

/// Helpers shared by the unit tests of every module: they build a real session
/// and drive it forward without a window.
#[cfg(test)]
pub(crate) mod testkit {
    use super::*;
    use tcgop_engine::ai::ai_choose_action;
    use tcgop_engine::decks::{marines_deck, mugiwara_deck};

    /// A reproducible Mugiwara-vs-Marines game.
    pub(crate) fn session(seed: u64) -> Session {
        Session::new(
            &mugiwara_deck(),
            &marines_deck(),
            Difficulty::Intermediate,
            seed,
        )
        .expect("decks are valid")
    }

    /// Drive the game — the AI plays for itself, the human passes / ends its
    /// turn — until `stop` holds or `max_steps` actions have been applied.
    /// Returns whether `stop` held in the end.
    pub(crate) fn advance(
        session: &mut Session,
        max_steps: usize,
        stop: impl Fn(&Session) -> bool,
    ) -> bool {
        for _ in 0..max_steps {
            if stop(session) {
                return true;
            }
            if session.winner().is_some() {
                return false;
            }
            let ai = session.ai_player();
            let level = session.ai_level;
            let action = match session.needs_auto_action() {
                AutoAction::None => {
                    if session.in_counter_window() {
                        GameAction::PassCounter
                    } else {
                        GameAction::EndTurn
                    }
                }
                AutoAction::AutoPass => GameAction::PassCounter,
                AutoAction::AiDefend => ai_choose_action(
                    &session.state,
                    &session.registry,
                    &mut session.ctx,
                    ai,
                    level,
                )
                .unwrap_or(GameAction::PassCounter),
                AutoAction::AiTurn => ai_choose_action(
                    &session.state,
                    &session.registry,
                    &mut session.ctx,
                    ai,
                    level,
                )
                .unwrap_or(GameAction::EndTurn),
            };
            if session.dispatch(action).is_err() {
                return stop(session);
            }
        }
        stop(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::testkit::{advance, session as make_session};

    fn session() -> Session {
        make_session(42)
    }

    #[test]
    fn new_session_starts_on_the_humans_turn_with_actions() {
        let s = session();
        assert_eq!(s.human, PlayerId::Player1);
        assert_eq!(s.ai_player(), PlayerId::Player2);
        assert_eq!(s.state.current_player, PlayerId::Player1);
        assert!(!s.is_ai_turn());
        assert!(!s.in_counter_window());
        assert!(!s.valid.is_empty(), "the human must have legal actions");
        assert!(s.valid.contains(&GameAction::EndTurn));
        assert_eq!(s.needs_auto_action(), AutoAction::None);
    }

    #[test]
    fn ending_the_turn_hands_over_to_the_ai_and_clears_valid() {
        let mut s = session();
        s.dispatch(GameAction::EndTurn).unwrap();
        assert_eq!(s.state.current_player, PlayerId::Player2);
        assert!(s.is_ai_turn());
        assert!(s.valid.is_empty(), "no human actions during the AI turn");
        assert_eq!(s.needs_auto_action(), AutoAction::AiTurn);
    }

    #[test]
    fn drain_log_only_returns_new_entries() {
        let mut s = session();
        assert!(s.drain_log().is_empty(), "a fresh game has logged nothing");

        // Play on until the engine actually writes a line.
        assert!(
            advance(&mut s, 60, |s| !s.state.log.is_empty()),
            "60 actions must produce at least one log line"
        );
        let entries = s.drain_log();
        assert_eq!(entries.len(), s.state.log.len());
        assert_eq!(s.log_cursor, s.state.log.len());
        assert!(s.drain_log().is_empty(), "the cursor consumed everything");
    }

    #[test]
    fn a_seed_reproduces_the_same_opening_hand() {
        let a = session();
        let b = session();
        assert_eq!(a.you().hand, b.you().hand);
    }

    #[test]
    fn dispatch_is_a_no_op_when_the_engine_refuses() {
        let mut s = session();
        let before = s.state.clone();
        let bogus = GameAction::DeployCharacter {
            instance_id: "nope".into(),
            slot: tcgop_engine::types::Slot::V1,
        };
        assert!(s.dispatch(bogus).is_err());
        assert_eq!(s.state, before, "a refused action must not mutate the state");
    }

    #[test]
    fn plugin_applies_actions_through_messages_headlessly() {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(BridgePlugin)
            .insert_resource(session());

        app.world_mut()
            .write_message(DispatchAction(GameAction::EndTurn));
        app.update();

        let applied: Vec<GameAction> = app
            .world()
            .resource::<Messages<ActionApplied>>()
            .iter_current_update_messages()
            .map(|m| m.0.clone())
            .collect();
        assert_eq!(applied, vec![GameAction::EndTurn]);

        let session = app.world().resource::<Session>();
        assert_eq!(session.state.current_player, PlayerId::Player2);
        assert!(session.valid.is_empty());
    }

    #[test]
    fn plugin_reports_engine_errors_without_touching_the_state() {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(BridgePlugin)
            .insert_resource(session());

        app.world_mut()
            .write_message(DispatchAction(GameAction::PassCounter));
        app.update();

        let failures = app
            .world()
            .resource::<Messages<EngineErrorEvent>>()
            .iter_current_update_messages()
            .count();
        assert_eq!(failures, 1);
        assert_eq!(
            app.world().resource::<Session>().state.current_player,
            PlayerId::Player1
        );
    }
}
